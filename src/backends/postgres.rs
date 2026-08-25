use crate::backends::TableIntrospector;
use crate::models::schema::TableMetadata;
use crate::prelude::*;
use std::io;
use std::io::Write;
use tokio_postgres::{Client, NoTls};

pub struct PostgresBackend {
    client: Client
}

impl PostgresBackend {

    /// Create a new Postgres backend with the given connection string
    pub async fn new(connection_str: &str) -> Result<Self> {

        // Flush stdout so the text immediately appears before the network request starts
        print!("Connecting to the database ... ");
        io::stdout().flush()?;

        // Note that the "propagation error operator" (denoted as "?") gets applied to a Result<T, Error>
        // value does perform an early return if an error is encountered, or unwraps the success value
        // if it succeeds. Is exact syntactic sugar for this match block:
        //
        //    match io::stdout().flush() {
        //     Ok(()) => (), // Result succeeded; unwrap the unit value () and continue execution
        //     Err(err) => {
        //         // Result failed; convert the std::io::Error into the function's return error type
        //         // and return early from the function immediately
        //         return Err(From::from(err));
        //     }
        // }

        // Note that calling tokio_postgres::connect(...) creates exactly one TCP connection to
        // PostgreSQL. There is no built-in connection pool inside tokio-postgres itself. In Rust,
        // connection teardown is managed automatically via ownership, reference counting, and the
        // Drop trait. There is no client.close() or connection.close() method in tokio-postgres
        //

        let (client, connection) =
            tokio_postgres::connect(connection_str, NoTls).await?;

        // =============================================================================================
        // 2. Spawn the connection driver task

        // The reason we must spawn connection into a background driver task is due to a deliberate
        // design choice in tokio-postgres: it separates the request/response interface from the network
        // driver engine. When we call tokio_postgres::connect(), it returns two distinct objects:
        //
        //   1. the 'client', which is our handle to issue queries (client.query(...)), as it formats
        //      our SQL instructions, sends them across an internal channel, and receives response channels.
        //
        //   2. the 'connection', which is the state machine that actually owns the raw TCP socket,
        //      manages network socket reads/writes, TLS encryption, TCP framing, and background heartbeats.
        //

        // Because connection is a Rust Future, it does nothing unless driven by an executor.
        // Spawning it gives it to Tokio to execute in the background, allowing your main task to use
        // the client freely.

        tokio::spawn(async move {
            if let Err(err) = connection.await {
                eprintln!("Connection driver error: {}", err);
            }
        });

        // Note that the "async" keyword defines an asynchronous block (a Rust closure that returns a Future).
        // Note that the "move" keyword forces the block to take full ownership of any variables captured
        // from its surrounding scope, moving them into the task's environment. Without "move", this block
        // would try to borrow 'connection'. The compiler rejects it because 'connection' could be
        // dropped if main() exits early.

        //
        // Think of the "connection.await" as saying: "Run the connection driver until the network
        // connection completely shuts down or crashes."
        //
        //
        //                              connection.await IS RUNNING
        //          ┌──────────────────────────────────────────────────────────────────┐
        //          │                                                                  │
        //          │  1. Receives SQL query from client.query(...)                    │
        //          │  2. Sends bytes over TCP to Postgres                             │
        //          │  3. Waits for Postgres response                                  │
        //          │  4. Routes data back to client                                   │
        //          │  ... (Repeats for minutes, hours, or days over the same socket)  │
        //          │                                                                  │
        //          └──────────────────────────────────────────────────────────────────┘
        //                                           │
        //                               Connection drops or error occurs
        //                                           │
        //                                           ▼
        //                                   connection.await RESOLVES
        //                                           │
        //                                           ▼
        //                            if let Err(e) triggers (or task completes)
        //
        //
        // * It suspends the task: when "connection.await" starts, Tokio registers the TCP socket,
        //    and the task yields control whenever there are no bytes to read or write.
        //
        // * It loops internally: inside tokio-postgres, the connection object has its own internal
        //    loop processing network packets; to our code, this all happens inside that single .await.
        //
        // * It resolves once at the end: the .await line does not finish while your database queries
        //   are running. It only resolves and returns its final Result<(), Error> when:
        //     * The connection is gracefully closed (returns Ok(())),
        //     * A unrecoverable network/protocol error occurs (returns Err(e)).
        //     * The client is dropped, signaling no more queries will arrive.
        //
        // Note PostgreSQL connection errors don't always happen in response to a query you just sent.
        // The server might close an idle connection or fail during a background health-check message.
        // Explicitly logging inside the spawned task ensures you capture driver-level transport errors
        // regardless of what your main application code is doing at that exact moment.
        //

        Ok(PostgresBackend { client })
    }
}


impl TableIntrospector for PostgresBackend {
    async fn introspect_table(&self, table_name: &str) -> Result<TableMetadata> {
        // TODO Clone the client using an async reference counted smart pointer
        //      the issue introspection queries concurrently by spawning multiple Tokio tasks
        let _ = self.client.query("SELECT 1", &[]).await?;
        println!("Introspecting table {} ... ", table_name);

        Ok(TableMetadata {
            name: table_name.into(),
            schema: None, // TODO How about schema name?
            columns: vec!(), // TODO how about columns?
            primary_key: None, // TODO how about the primary key?
        })
    }
}
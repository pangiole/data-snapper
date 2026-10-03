use crate::backends::TableIntrospector;
use crate::models::schema::{ColumnMetadata, TableMetadata};
use crate::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls};
// DO NOT use std::sync::Mutex


/// A lazily initialized value
type LazilyInitialized<T> = Mutex<Option<T>>;

/// A shareable Tokio Postgres client (which is already thread-safe by its own merit),
/// used to issue SQL statements from concurrent Tokio tasks
type ShareableClient = Arc<Client>;


/// The data access layer for Postgres
pub struct PostgresBackend {
    connection_str: String,
    client: LazilyInitialized<ShareableClient>
}

impl PostgresBackend {
    /// Create a new Postgres backend with the given connection string.
    /// The actual database connection is deferred until the first introspection.
    pub fn new(connection_str: &str) -> Self {
        PostgresBackend {
            connection_str: connection_str.to_string(),
            client: Mutex::new(None)
        }
    }


    /// Lazily initialize the Postgres connection and client on first use
    async fn ensure_connected(&self) -> Result<ShareableClient> {

        // Note that we are using the Tokio Mutex instead of the standard Mutex smart pointer.
        // The difference between these two smart pointers comes down to who gets put to sleep
        // when the lock is already taken: the entire Operating System thread, or just the
        // individual asynchronous Tokio task.
        //
        let mut self_client = self.client.lock().await;

        if self_client.is_none() {
            info!("Connecting to the database");

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
                tokio_postgres::connect(&self.connection_str, NoTls)
                    .await
                    .context("Failed to connect to the Postgres database")?;


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
                    error!("Database connection error: {}", err);
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

            // Initialize the client using interior mutability
            *self_client = Some(Arc::new(client));
        }

        // Return a shareable client
        Ok(
            self_client
                .as_ref()
                .expect("Client should exist after ensure_connected")
                .clone()
        )
    }
}


impl TableIntrospector for PostgresBackend {
    const DEFAULT_SCHEMA: &'static str = "public";

    async fn introspect_table(&self, given_name: &str) -> Result<TableMetadata> {

        let (schema_name, table_name) =
            Self::extract_schema_and_table_name(given_name)?;

        // Ensure connection exists before using it and return a reference to the client
        let client = self.ensure_connected().await?;

        // TODO Why format!() and then reference? Couldn't we improve the .info() signature instead?
        info!("Introspecting table {}", given_name);

        let sql = r#"
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position;
        "#.trim();

        let res = client.query(sql,
            // Note that Postgres database objects are catalogued in lower case
            &[
                &schema_name.to_lowercase(),
                &table_name.to_lowercase()
            ],
        ).await?;

        let _columns = res.iter()
            .map(|row| {
                ColumnMetadata {
                    name: row.get("column_name"),
                    tpe: row.get("data_type"),
                }
            })
            .collect::<Vec<_>>();

        Ok(TableMetadata {
            schema: schema_name.into(),
            table: table_name.into(),
            columns: vec!(), // TODO how about columns?
            primary_key: None, // TODO how about the primary key?
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn introspect_table_given_name_ending_with_dot() {

        let backend =
            PostgresBackend::new("connection_str");

        let result =
            backend.introspect_table("my_schema.").await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "The given table name 'my_schema.' mistakenly ends with '.' (dot)"
        );
    }
}
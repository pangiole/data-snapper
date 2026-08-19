// Note that the #[tokio::main] attribute macro is going to alter our Rust code to make it look
// like the following:
//
//     fn main() {
//         tokio::runtime::Builder::new_current_thread()
//             .enable_all()
//             .build()
//             .unwrap()
//             .block_on(async {
//                 println!("Hello world");
//             })
//     }
//
//

// Note that we are configuring the Tokio runtime on a single OS thread and relying
// upon the Tokio ability to execute multiple tasks concurrently. This is an acceptable configuration
// for our data-snapper tool because the database extractions and CSV streaming are overwhelmingly
// I/O-bound (waiting on Postgres to send data over TCP and waiting for disk writes).
// Because the CPU spends most of its time waiting for network or disk operations to finish rather
// than doing heavy mathematical calculations, a single OS thread using asynchronous non-blocking
// I/O can easily saturate gigabit network interfaces and max out disk speeds.


use std::io::{self, Write};
use tokio_postgres::NoTls;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {


    // =============================================================================================
    // 1. Attempt connection

    let connection_str = "host=localhost user=postgres password=postgres dbname=postgres port=5432";

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

    let (client, connection) = match tokio_postgres::connect(connection_str, NoTls).await {
        Ok(tuple) => { println!("done"); tuple }
        Err(err) => { println!("error"); return Err(err.into()); }
    };


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
    // If the client had managed its own network socket synchronously, or implicitly inside .query(),
    // we would hit three major limitations:
    //
    //   * no connection sharing and no concurrent queries
    //     by separating client from connection, we can clone client (client.clone())
    //     across multiple tasks and execute queries concurrently over the same single
    //     TCP connection, which multiplexes those requests over the socket in the background.
    //
    //   * no spontaneous network processing
    //     PostgreSQL sends unexpected messages over the wire (notice warnings, async notifications,
    //     error signals, disconnects). The connection task continuously loops in the background
    //     to handle these socket events even when your main code isn't actively making a query.
    //
    //   * clear task cancellation
    //     if your main task drops client or gets cancelled, the background connection task notices
    //     that no clients are listening and gracefully closes the underlying TCP socket.
    //
    //
    // Because connection is a Rust Future, it does nothing unless driven by an executor.
    // Spawning it gives it to Tokio to execute in the background, allowing your main task to use
    // the client freely.

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection driver error: {}", e);
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

    // TODO You can now freely use `client` to run queries
    let _ = client.query("SELECT 1", &[]).await?;
    println!("Query executed successfully!");

    Ok(())
}
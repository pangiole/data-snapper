
#[macro_use] mod logger;

pub mod prelude;
mod models;
mod backends;
pub mod engine;

// TODO use backends::oracle::_OracleBackend;
// TODO use:backends::microsoft::_MicrosoftSQLServerBackend;
use crate::engine::discovery::RelationshipDiscovery;
use crate::engine::snapper::SnapperEngine;
use crate::prelude::*;
use backends::postgres::PostgresBackend;



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

#[tokio::main(flavor = "current_thread")]
async fn main() -> () {
    // Immediately initialize the logger mechanism and hold its guard, so that
    // when it goes out of scope the logger thread will be gracefully killed.
    let _g = logger::init();

    if let Err(e) = run().await {
        error!("{}: {}", e, e.root_cause());
    }
    else {
        info!("Completed successfully");
    }
}



async fn run() -> Result<()> {
    // TODO Derive this app version from the build.
    let version = "0.1.0";
    info!("Welcome to Data Snapper {version}");

    // TODO the connection string should be propagated as command line argument
    let connection_str = "host=localhost user=snapper password=snapper dbname=snapper port=5432";
    let backend = PostgresBackend::new(connection_str);

    let discovery = RelationshipDiscovery::new(&backend);
    let snapper = SnapperEngine::new(discovery);
    snapper.snap("public.instrument_date_clients").await?;

    Ok(())
}



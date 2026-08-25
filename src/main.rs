
pub mod prelude;
mod models;
mod backends;
pub mod engine;



use crate::prelude::*;
use backends::postgres::PostgresBackend;
// TODO use backends::oracle::_OracleBackend;
// TODO use:backends::microsoft::_MicrosoftSQLServerBackend;
use crate::engine::discovery::RelationshipDiscovery;
use crate::engine::snapper::SnapperEngine;


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
async fn main() -> Result<()> {

    // TODO the connection string should be propagated as command line argument
    let connection_str = "host=localhost user=postgres password=postgres dbname=postgres port=5432";
    let backend = PostgresBackend::new(connection_str).await?;
    let discovery = RelationshipDiscovery::new(&backend);
    let snapper = SnapperEngine::new(discovery);

    snapper.snap("public.instrument_date_clients").await?;

    // TODO Shouldn't we explicitly close the introspector (and the underlying database connections?)

    Ok(())
}



//! Defines the core traits, such as the database introspector and the query stream.

pub mod postgres;
pub mod oracle;
pub mod microsoft;

use super::models::schema::TableMetadata;
use super::prelude::*;

/// Defines the introspection behavior that automatically discovers tables, columns, constraints and relationships.
pub trait TableIntrospector {

    // Note we are allowing the lint because we're using single-threaded Tokio (flavor = "current_thread"),
    // therefore thread safety isn't an issue across OS threads, and we don't need the Future to be a Send
    #[allow(async_fn_in_trait)]
    async fn introspect_table(&self, table_name: &str) -> Result<TableMetadata>;
}
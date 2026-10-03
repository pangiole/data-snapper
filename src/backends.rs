//! Defines the core traits, such as the database introspector and the query stream.

pub mod postgres;
pub mod oracle;
pub mod microsoft;

use super::models::schema::TableMetadata;
use super::prelude::*;
use anyhow::anyhow;

/// Defines the introspection behavior that automatically discovers tables, columns, constraints and relationships.
pub trait TableIntrospector {

    const DEFAULT_SCHEMA: &str;

    /// Extracts the schema and table name from the given string slice, by splitting it around
    /// the `.` (dot) character. If no `.` (dot) character is given then it uses the
    /// `Self::DEFAULT_SCHEMA` associated constant
    fn extract_schema_and_table_name(given_str: &str) -> Result<(&str, &str)> {
        match given_str.split_once('.') {
            Some((_, "")) =>
                Err(anyhow!("The given table name '{}' mistakenly ends with '.' (dot)", given_str)),

            Some((ts, tn)) =>
                Ok((ts, tn)),

            None =>
                Ok((Self::DEFAULT_SCHEMA, given_str)),
        }
    }

    // Note we are allowing the lint because we're using single-threaded Tokio (flavor = "current_thread"),
    // therefore thread safety isn't an issue across OS threads, and we don't need the Future to be a Send
    #[allow(async_fn_in_trait)]
    async fn introspect_table(&self, name: &str) -> Result<TableMetadata>;
}
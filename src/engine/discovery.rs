use crate::backends::TableIntrospector;
use crate::prelude::*;

struct _SnapperNode;

// TODO Choose the most appropriate data structure able to represent our tables graph
/// Represents the table set in need to be extracted
pub struct SnapperGraph;

/// Discovers the relationships between database tables and builds up the correspondent snapper graph
pub struct  RelationshipDiscovery<'a, T: TableIntrospector> {
    introspector: &'a T,
}

impl<'a, T: TableIntrospector> RelationshipDiscovery<'a, T> {

    /// Creates a new schema discovery with the given table introspector
    pub fn new(introspector: &'a T) -> Self {
        Self { introspector }
    }

    /// Discovers the relationships between database tables, starting from the given pivot_table,
    /// and builds the correspondent snapper graph
    ///
    /// # Arguments
    /// * `pivot_table`- the starting table name, eventually prefixed by its schema name
    ///                  (e.g. `my_schema.my_table`)
    ///
    /// # Returns
    /// Either the snapper graph or an error
    ///
    pub async fn discover_relationships(&self, pivot_table: &str) -> Result<SnapperGraph> {
        let _ = self.introspector.introspect_table(pivot_table).await?;
        // TODO Analyze the resulting table metadata and keep introspecting its neighborhood
        Ok(SnapperGraph {})
    }
}
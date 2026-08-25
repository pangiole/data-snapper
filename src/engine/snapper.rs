use crate::backends::TableIntrospector;
use crate::engine::discovery::{RelationshipDiscovery, SnapperGraph};
use crate::prelude::*;
use std::path::Path;
use tempfile::TempDir;

/// Orchestrates the data snap
pub struct SnapperEngine<'a, T: TableIntrospector> {
    discovery: RelationshipDiscovery<'a, T>
}

impl<'a, T: TableIntrospector> SnapperEngine<'a, T> {

    /// Creates a new engine with the given discovery
    pub fn new(discovery: RelationshipDiscovery<'a, T>) -> Self {
        Self { discovery }
    }


    pub async fn snap(self, pivot_table: &str) -> Result<()> {
        let _ = self.discovery.discover_relationships(pivot_table).await?;
        // TODO let _ = self.extract_data(graph).await?;
        // TODO merge
        Ok(())
    }


    // ---------------------------------------
    // PRIVATE functions

    async fn _extract_data(&self, _graph: SnapperGraph) -> Result<TempDir> {
        let temp_dir = TempDir::new()?;
        // TODO traverse the graph and extract into the temp_dir
        Ok(temp_dir)
    }

    fn _merge(_extracted_dir: TempDir, _target_dir: &Path) {

    }
}
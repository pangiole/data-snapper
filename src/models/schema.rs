//! Defines the core structs modeling database tables, columns, foreign keys, etc.

pub struct TableMetadata {
    pub name: String,
    pub schema: Option<String>,
    pub columns: Vec<ColumnMetadata>,
    pub primary_key: Option<Vec<ColumnMetadata>>,
}

pub struct ColumnMetadata {
    pub name: String,
    pub data_type: String,
    pub belongs_to_primary_key: bool,
}
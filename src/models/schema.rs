//! Defines the core structs modeling database tables, columns, foreign keys, etc.

#[derive(Debug)]
pub struct TableMetadata {
    pub schema: String,
    pub table: String,
    pub columns: Vec<ColumnMetadata>,
    pub primary_key: Option<Vec<ColumnMetadata>>,
}

#[derive(Debug)]
pub struct ColumnMetadata {
    pub name: String,
    pub tpe: String
}
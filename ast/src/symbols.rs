use crate::types::{Expression, Location, SourceFile, Type};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolType {
    Global(String),
    Inner(String),
}

pub enum SymbolScope {}

pub struct Symbol {
    name: String,
    location: Location,
    ty: Type,
    expr: Expression,
}

#[derive(Clone, Default)]
pub struct SymbolTable {
    types: Vec<SymbolType>,
}

impl SymbolTable {
    pub fn build(types: Vec<SymbolType>, source_files: &Vec<SourceFile>) -> Self {
        Self { types }
    }
}

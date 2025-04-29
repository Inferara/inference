use crate::{
    arena::Arena,
    symbols::SymbolTable,
    types::{AstNode, SourceFile},
};

#[derive(Clone, Default)]
pub struct TypedAst {
    pub source_files: Vec<SourceFile>,
    pub symbol_table: SymbolTable,
    arena: Arena,
}

impl TypedAst {
    pub fn new(source_files: Vec<SourceFile>, symbol_table: SymbolTable, arena: Arena) -> Self {
        Self {
            source_files,
            symbol_table,
            arena,
        }
    }

    pub fn filter_nodes<T: Fn(&AstNode) -> bool>(&self, fn_predicate: T) -> Vec<AstNode> {
        self.arena
            .nodes
            .values()
            .filter(|node| fn_predicate(node))
            .cloned()
            .collect()
    }
}

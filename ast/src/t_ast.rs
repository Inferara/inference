use crate::{
    arena::Arena,
    types::{AstNode, SourceFile},
};

#[derive(Clone, Default)]
pub struct TypedAst {
    pub source_files: Vec<SourceFile>,
    arena: Arena,
}

impl TypedAst {
    #[must_use]
    pub fn new(source_files: Vec<SourceFile>, arena: Arena) -> Self {
        Self {
            source_files,
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

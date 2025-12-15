use crate::{arena::Arena, type_infer::TypeChecker};
use inference_ast::arena::Arena as AstArena;

#[derive(Clone, Default)]
pub struct Hir {
    pub arena: Arena,
    pub type_checker: TypeChecker,
}

impl Hir {
    #[must_use]
    pub fn new(arena: AstArena) -> Self {
        Self {
            arena: Arena::default(),
            type_checker: TypeChecker::default(),
        }
    }
}

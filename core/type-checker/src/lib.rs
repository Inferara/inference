use std::marker::PhantomData;

use inference_ast::arena::Arena;

use crate::{type_checker::TypeChecker, typed_context::TypedContext};

pub mod errors;
mod symbol_table;
mod type_checker;
pub mod type_info;
pub mod typed_context;

#[allow(dead_code)]
trait TypeCheckerBuilderInit {}
#[allow(dead_code)]
trait TypeCheckerBuilderComplete {}

pub struct TypeCheckerInitState;
impl TypeCheckerBuilderInit for TypeCheckerInitState {}
pub struct TypeCheckerCompleteState;
impl TypeCheckerBuilderComplete for TypeCheckerCompleteState {}

pub type CompletedTypeCheckerBuilder = TypeCheckerBuilder<TypeCheckerCompleteState>;

#[allow(dead_code)]
pub struct TypeCheckerBuilder<S> {
    typed_context: TypedContext,
    _state: PhantomData<S>,
}

impl Default for TypeCheckerBuilder<TypeCheckerInitState> {
    fn default() -> Self {
        TypeCheckerBuilder::new()
    }
}

impl TypeCheckerBuilder<TypeCheckerInitState> {
    pub fn new() -> Self {
        TypeCheckerBuilder {
            typed_context: TypedContext::default(),
            _state: PhantomData,
        }
    }

    pub fn build_typed_context(
        arena: Arena,
    ) -> anyhow::Result<TypeCheckerBuilder<TypeCheckerCompleteState>> {
        let mut ctx = TypedContext::new(arena);
        let mut type_checker = TypeChecker::default();
        match type_checker.infer_types(&mut ctx) {
            Ok(symbol_table) => {
                ctx.symbol_table = symbol_table;
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(TypeCheckerBuilder {
            typed_context: ctx,
            _state: PhantomData,
        })
    }
}

impl TypeCheckerBuilder<TypeCheckerCompleteState> {
    pub fn typed_context(self) -> TypedContext {
        self.typed_context
    }
}

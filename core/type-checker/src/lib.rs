//! Type Checker Crate
//!
//! This crate provides type checking and type inference for the Inference language.
//! It implements bidirectional type checking with support for:
//!
//! - Primitive types (i8, i16, i32, i64, u8, u16, u32, u64, bool, string)
//! - Struct and enum types with visibility checking
//! - Method resolution on struct types
//! - Generic function type parameter inference and substitution
//! - Import resolution with glob and partial imports
//! - Error recovery for collecting multiple errors
//!
//! ## Entry Point
//!
//! Use [`TypeCheckerBuilder`] to type-check an AST arena:
//!
//! ```ignore
//! let arena = parse_source(source_code);
//! let typed_context = TypeCheckerBuilder::build_typed_context(arena)?.typed_context();
//! ```
//!
//! ## Architecture
//!
//! The type checker operates in phases:
//! 1. Process directives (register raw imports)
//! 2. Register types (collect type definitions into symbol table)
//! 3. Resolve imports (bind import paths to symbols)
//! 4. Collect function definitions (register functions)
//! 5. Infer variable types in function bodies
//!
//! ## Modules
//!
//! - [`errors`] - Typed error system with 29 variants
//! - [`type_info`] - Type representation (TypeInfo, TypeInfoKind)
//! - [`typed_context`] - Type information storage for AST nodes

use std::marker::PhantomData;

use inference_ast::arena::Arena;

use crate::{type_checker::TypeChecker, typed_context::TypedContext};

pub mod errors;
mod symbol_table;
mod type_checker;
pub mod type_info;
pub mod typed_context;

/// Marker state indicating builder has not yet been initialized with an arena.
pub struct TypeCheckerInitState;

/// Marker state indicating type checking is complete and context is ready.
pub struct TypeCheckerCompleteState;

/// Type alias for a completed type checker builder ready to yield its context.
pub type CompletedTypeCheckerBuilder = TypeCheckerBuilder<TypeCheckerCompleteState>;

/// Builder for running type checking on an AST arena.
///
/// Uses the typestate pattern to ensure type checking completes before
/// accessing the typed context.
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
    #[must_use]
    pub fn new() -> Self {
        TypeCheckerBuilder {
            typed_context: TypedContext::default(),
            _state: PhantomData,
        }
    }

    /// Run type checking on the provided arena and return a completed builder.
    ///
    /// # Errors
    ///
    /// Returns an error if type checking fails with unrecoverable errors.
    #[must_use = "returns builder with typed context, extract with .typed_context()"]
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

        debug_assert!(
            {
                let untyped = ctx.find_untyped_expressions();
                if !untyped.is_empty() {
                    eprintln!(
                        "Type checker bug: {} expression(s) without TypeInfo:",
                        untyped.len()
                    );
                    for m in &untyped {
                        eprintln!("  - {} at {} (id: {})", m.kind, m.location, m.id);
                    }
                }
                untyped.is_empty()
            },
            "All expressions should have TypeInfo after type checking"
        );

        Ok(TypeCheckerBuilder {
            typed_context: ctx,
            _state: PhantomData,
        })
    }
}

impl TypeCheckerBuilder<TypeCheckerCompleteState> {
    /// Consume the builder and return the typed context.
    #[must_use = "consumes builder and returns the typed context"]
    pub fn typed_context(self) -> TypedContext {
        self.typed_context
    }
}

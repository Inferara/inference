//! Type Checker for the Inference Programming Language
//!
//! This crate provides comprehensive type checking and type inference for Inference,
//! implementing bidirectional type checking with multi-phase analysis.
//!
//! ## Core Features
//!
//! **Type System Support**:
//! - Primitive types: `bool`, `unit`, `i8`-`i64`, `u8`-`u64` (using efficient `SimpleTypeKind` enum)
//! - Compound types: arrays with fixed sizes, structs with fields, enums with variants
//! - Generic types: type parameter inference and substitution for generic functions
//! - Visibility control: `pub` modifiers with private-by-default semantics
//!
//! **Type Checking**:
//! - Bidirectional inference: combines synthesis (bottom-up) and checking (top-down)
//! - Multi-phase analysis: handles forward references and circular dependencies
//! - Scope-aware symbol table: hierarchical scope management with proper shadowing
//! - Method resolution: instance methods and associated functions on structs
//! - Import system: plain, glob, and partial imports with visibility checking
//!
//! **Operator Support**:
//! - Arithmetic: `+`, `-`, `*`, `/`, `%`, `**`
//! - Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
//! - Logical: `&&`, `||`, `!`
//! - Bitwise: `&`, `|`, `^`, `<<`, `>>`, `~`
//! - Unary: `-` (negation), `!` (logical NOT), `~` (bitwise NOT)
//!
//! **Error Handling**:
//! - Comprehensive error types with detailed context
//! - Error recovery: collects multiple errors before failing
//! - Error deduplication: avoids repeated reports of the same issue
//! - Precise locations: all errors include source line and column information
//!
//! ## Type Representation
//!
//! The type checker uses a two-level type representation strategy:
//!
//! **Level 1 - AST Types** (`Type` enum from `inference_ast`):
//! - Source-level representation parsed from code
//! - Uses `Type::Simple(SimpleTypeKind)` for primitive builtin types
//! - `SimpleTypeKind` is a lightweight enum without heap allocation
//! - Efficient for the parser and AST construction
//!
//! **Level 2 - Type Information** (`TypeInfo` from this crate):
//! - Semantic representation for type checking and inference
//! - Uses `TypeInfoKind` with rich semantic information
//! - Supports type parameter substitution and unification
//!
//! This design provides both parse efficiency and semantic flexibility.
//!
//! ## Quick Start
//!
//! Use [`TypeCheckerBuilder`] to type-check an AST arena:
//!
//! ```ignore
//! use inference_ast::arena::Arena;
//! use inference_type_checker::TypeCheckerBuilder;
//!
//! // Parse source code into an arena
//! let arena: Arena = parse_source(source_code)?;
//!
//! // Run type checking
//! let typed_context = TypeCheckerBuilder::build_typed_context(arena)?
//!     .typed_context();
//!
//! // Query type information
//! if let Some(type_info) = typed_context.get_node_typeinfo(node_id) {
//!     println!("Node {} has type: {}", node_id, type_info);
//! }
//! ```
//!
//! ## Multi-Phase Architecture
//!
//! The type checker operates in five sequential phases:
//!
//! 1. **Process Directives** - Register raw import statements in scope tree
//! 2. **Register Types** - Collect struct, enum, spec, and type alias definitions
//! 3. **Resolve Imports** - Bind import paths to symbols in symbol table
//! 4. **Register Functions** - Collect function and method signatures
//! 5. **Infer Variables** - Type-check function bodies and variable declarations
//!
//! This ordering ensures that types are available before functions reference them,
//! and imports are resolved before symbol lookup.
//!
//! ## Public Modules
//!
//! - [`errors`] - Comprehensive error types with detailed context information
//! - [`type_info`] - Type representation system (`TypeInfo`, `TypeInfoKind`, `NumberType`)
//! - [`typed_context`] - Storage for type annotations on AST nodes with query API
//!
//! ## Documentation
//!
//! For detailed information, see the `docs/` directory:
//! - [Architecture Guide](../docs/architecture.md) - Internal design and implementation
//! - [API Guide](../docs/api-guide.md) - Practical usage examples and patterns
//! - [Type System Reference](../docs/type-system.md) - Complete type system rules
//! - [Error Reference](../docs/errors.md) - Catalog of all error types

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

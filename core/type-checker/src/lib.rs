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
//! - Import system: file and item imports with visibility checking and `pub use` re-export
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
//! use inference_ast::arena::AstArena;
//! use inference_type_checker::TypeCheckerBuilder;
//!
//! // Parse source code into an arena
//! let arena: AstArena = parse_source(source_code)?;
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

use anyhow::bail;
use inference_ast::arena::AstArena;

use crate::{errors::TypeCheckError, type_checker::TypeChecker, typed_context::TypedContext};

mod definition_graph;
pub mod errors;
mod extern_index;
mod symbol_table;
mod type_checker;
pub mod type_info;
pub mod typed_context;

pub use extern_index::ExternIndex;
pub use symbol_table::{BindingMutability, EnumInfo, ExternOrigin, StructFieldInfo, StructInfo};
pub use typed_context::MethodMetadata;

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
    /// Create a new builder in the initial (untyped) state.
    ///
    /// Prefer [`TypeCheckerBuilder::build_typed_context`] for the common case
    /// where you have an arena ready to check immediately.
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
        arena: AstArena,
    ) -> anyhow::Result<TypeCheckerBuilder<TypeCheckerCompleteState>> {
        let TypeCheckOutcome {
            typed_context,
            errors,
        } = check_with_diagnostics(arena);
        if !errors.is_empty() {
            // Prefix each error with the `::`-joined module path of the file it
            // was produced in; the entry file stays a bare `line:col` (its label
            // is `None`), so single-file programs read exactly as before.
            let error_messages: Vec<String> = errors
                .into_iter()
                .map(|d| match d.file_label {
                    Some(label) => format!("{label}:{}", d.error),
                    None => d.error.to_string(),
                })
                .collect();
            bail!(error_messages.join("; "));
        }

        Ok(TypeCheckerBuilder {
            typed_context,
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

/// A single structured type-check diagnostic: a [`TypeCheckError`] paired with
/// the module-path file label of the file it was produced in.
///
/// The label is required because source locations in a multi-file program are
/// per-file-local in the merged arena, so a bare `line:col` from an imported
/// file would otherwise be misattributed to the entry file. It matches
/// [`inference_ast::nodes::file_label`]: `None` for the entry file, the
/// `::`-joined module path (e.g. `lib::geom`) otherwise. The per-error source
/// location is available without string parsing via
/// [`TypeCheckError::location`](crate::errors::TypeCheckError::location).
#[derive(Debug, Clone)]
pub struct TypeCheckDiagnostic {
    /// The `::`-joined module path of the file this error belongs to, or `None`
    /// for the entry file.
    pub file_label: Option<String>,
    /// The structured error, carrying its own per-file-local source location.
    pub error: TypeCheckError,
}

/// The lossless outcome of a type-check run: the typed context together with
/// every structured diagnostic collected. Produced by [`check_with_diagnostics`].
///
/// # Partial context guarantees
///
/// The [`typed_context`](Self::typed_context) is returned whether or not type
/// checking found errors, because the checker recovers from errors and runs
/// every phase to completion rather than aborting on the first one. As a result
/// the *whole-program* tables are always fully built, up to what was resolvable:
///
/// - [`TypedContext::arena`] — the parsed arena, always complete (type checking
///   never mutates it).
/// - [`TypedContext::lookup_struct`] / [`TypedContext::lookup_enum`] and the
///   underlying symbol table (methods, functions, imports) — populated for every
///   definition the checker could register, independent of body errors, because
///   the symbol table is assigned into the context and its canonical-key indexes
///   are built even on the error path.
///
/// Only *per-node* results are affected by errors, and only for the nodes that
/// failed:
///
/// - [`TypedContext::get_node_typeinfo`] answers for every node that was
///   successfully typed. A node inside a definition whose body failed to
///   type-check may be absent, but each definition is inferred independently, so
///   a node in a *sibling* definition that did check is unaffected.
/// - [`TypedContext::call_target`] answers for every call that resolved to a
///   known function; an unresolved (erroring) call is absent.
///
/// When [`errors`](Self::errors) is empty the context is exactly the one
/// [`TypeCheckerBuilder::build_typed_context`] yields on success.
///
/// [`TypedContext::arena`]: crate::typed_context::TypedContext::arena
/// [`TypedContext::lookup_struct`]: crate::typed_context::TypedContext::lookup_struct
/// [`TypedContext::lookup_enum`]: crate::typed_context::TypedContext::lookup_enum
/// [`TypedContext::get_node_typeinfo`]: crate::typed_context::TypedContext::get_node_typeinfo
/// [`TypedContext::call_target`]: crate::typed_context::TypedContext::call_target
pub struct TypeCheckOutcome {
    /// The typed context, populated as far as error recovery allowed (see the
    /// type-level docs for the exact partial guarantees).
    pub typed_context: TypedContext,
    /// Every structured diagnostic collected, in the same order the aggregated
    /// [`TypeCheckerBuilder::build_typed_context`] message renders them. Empty
    /// when the program type-checks cleanly.
    pub errors: Vec<TypeCheckDiagnostic>,
}

/// Type-checks `arena` losslessly: runs the same pipeline as
/// [`TypeCheckerBuilder::build_typed_context`] but returns the (possibly
/// partially populated) [`TypedContext`] together with the structured errors,
/// instead of discarding the context and joining the errors into one string.
///
/// This is the structured entry point the IDE/LSP layer builds on: every error
/// keeps its variant, its per-file-local source location (via
/// [`TypeCheckError::location`](crate::errors::TypeCheckError::location)), and
/// its optional module-path file label without any string parsing. See
/// [`TypeCheckOutcome`] for what the returned context is guaranteed to contain
/// when errors are present.
///
/// [`TypeCheckerBuilder::build_typed_context`] is re-expressed on top of this
/// function, so the two share exactly one checking implementation.
#[must_use = "the outcome carries both the typed context and the diagnostics"]
pub fn check_with_diagnostics(arena: AstArena) -> TypeCheckOutcome {
    let mut ctx = TypedContext::new(arena);
    let mut type_checker = TypeChecker::default();
    let (symbol_table, errors) = type_checker.check_collecting(&mut ctx);
    // Assign the symbol table and build the canonical-key indexes even when there
    // are errors, so the partial context answers `lookup_struct`/`lookup_enum`
    // and method/call queries for the parts of the program that did check — the
    // whole point of a lossless entry point for tooling.
    ctx.symbol_table = symbol_table;
    ctx.build_type_indexes();
    let errors = errors
        .into_iter()
        .map(|(file_label, error)| TypeCheckDiagnostic { file_label, error })
        .collect();
    TypeCheckOutcome {
        typed_context: ctx,
        errors,
    }
}

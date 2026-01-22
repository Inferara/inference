#![warn(clippy::pedantic)]
//! Core Orchestration Crate for the Inference Compiler
//!
//! This crate provides the main entry points for the Inference compiler pipeline.
//! It orchestrates the compilation process from source code to WebAssembly binary
//! and optionally to Rocq (Coq) verification code.
//!
//! ## Overview
//!
//! The Inference compiler implements a multi-phase compilation pipeline:
//!
//! ```text
//! .inf source → tree-sitter → Typed AST → Type Check → LLVM IR → WASM → Rocq (.v)
//! ```
//!
//! Each phase is exposed as a standalone function in this crate, allowing flexible
//! control over which compilation stages to execute.
//!
//! ## Compilation Pipeline
//!
//! ### Phase 1: Parse
//!
//! Transforms source code into an arena-based Abstract Syntax Tree (AST).
//!
//! ```ignore
//! use inference::parse;
//!
//! let source = r#"fn main() { return 42; }"#;
//! let arena = parse(source)?;
//! ```
//!
//! The parser uses tree-sitter for concrete syntax tree (CST) construction,
//! then transforms it into a typed AST stored in an [`Arena`]. The arena provides
//! O(1) node lookup and maintains parent-child relationships for efficient traversal.
//!
//! ### Phase 2: Type Check
//!
//! Performs type inference and validation on the AST.
//!
//! ```ignore
//! use inference::{parse, type_check};
//!
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! ```
//!
//! The type checker operates in multiple phases:
//! 1. Process directives (register imports)
//! 2. Register types (collect type definitions)
//! 3. Resolve imports (bind import paths to symbols)
//! 4. Collect functions (register function signatures)
//! 5. Infer types (type-check function bodies)
//!
//! The result is a [`TypedContext`] that maps AST nodes to their inferred types.
//!
//! ### Phase 3: Analyze
//!
//! Performs semantic analysis on the typed AST. This phase is currently under
//! active development (WIP) and serves as a placeholder for future semantic
//! analysis passes.
//!
//! ```ignore
//! use inference::{parse, type_check, analyze};
//!
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! analyze(&typed_context)?;
//! ```
//!
//! ### Phase 4: Codegen
//!
//! Generates WebAssembly binary format from the typed AST.
//!
//! ```ignore
//! use inference::{parse, type_check, codegen};
//!
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! let wasm_bytes = codegen(&typed_context)?;
//! ```
//!
//! The code generator uses LLVM IR as an intermediate representation and supports
//! custom intrinsics for non-deterministic instructions specific to Inference:
//! - `forall` - Universal quantification blocks
//! - `exists` - Existential quantification blocks
//! - `uzumaki` - Non-deterministic value generation
//! - `assume` - Constraint specification
//! - `unique` - Uniqueness constraints
//!
//! ### Phase 5: WASM to Rocq Translation
//!
//! Translates WebAssembly binary to Rocq (Coq) verification code.
//!
//! ```ignore
//! use inference::{parse, type_check, codegen, wasm_to_v};
//!
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! let wasm_bytes = codegen(&typed_context)?;
//! let rocq_code = wasm_to_v("MyModule", &wasm_bytes)?;
//! ```
//!
//! The resulting `.v` file can be used with Rocq for formal verification of
//! program properties.
//!
//! ## Architecture
//!
//! This crate acts as a thin orchestration layer that delegates to specialized crates:
//!
//! - [`inference_ast`] - Arena-based AST construction and tree-sitter parsing
//! - [`inference_type_checker`] - Bidirectional type checking with error recovery
//! - [`inference_wasm_codegen`] - LLVM-based code generation
//! - [`inference_wasm_to_v_translator`] - WASM to Rocq translation
//!
//! ## Error Handling
//!
//! All public functions return `anyhow::Result` for flexible error propagation.
//! Each phase collects and reports errors before failing, allowing users to see
//! all issues at once rather than fixing one error at a time.
//!
//! ## Example: Complete Pipeline
//!
//! ```ignore
//! use inference::{parse, type_check, analyze, codegen, wasm_to_v};
//!
//! fn compile_to_wasm(source_code: &str) -> anyhow::Result<Vec<u8>> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     analyze(&typed_context)?;
//!     codegen(&typed_context)
//! }
//!
//! fn compile_to_rocq(source_code: &str, module_name: &str) -> anyhow::Result<String> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     let wasm = codegen(&typed_context)?;
//!     wasm_to_v(module_name, &wasm)
//! }
//! ```
//!
//! ## Current Limitations
//!
//! - **Single-file support**: Multi-file compilation is not yet implemented.
//!   The AST expects a single source file as input.
//! - **Analyze phase**: The semantic analysis phase is work-in-progress and
//!   currently returns `Ok(())` without performing any checks.
//!
//! ## See Also
//!
//! - [`inference_ast::arena::Arena`] - Arena-based AST storage
//! - [`inference_ast::builder::Builder`] - AST construction from tree-sitter CST
//! - [`inference_type_checker::TypeCheckerBuilder`] - Type checking entry point
//! - [`inference_type_checker::typed_context::TypedContext`] - Type information storage

use inference_ast::{arena::Arena, builder::Builder};
use inference_type_checker::typed_context::TypedContext;

/// Parses source code and builds an arena-based Abstract Syntax Tree.
///
/// This function orchestrates the parsing pipeline:
/// 1. Initializes a tree-sitter parser with the Inference grammar
/// 2. Parses the source code into a Concrete Syntax Tree (CST)
/// 3. Transforms the CST into an arena-based AST using [`Builder`]
///
/// The resulting [`Arena`] stores all AST nodes with unique IDs and maintains
/// parent-child relationships for efficient traversal. Root nodes are
/// [`SourceFile`] nodes that represent the top-level compilation unit.
///
/// # Examples
///
/// ```ignore
/// use inference::parse;
///
/// let source = r#"
///     fn add(a: i32, b: i32) -> i32 {
///         return a + b;
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let source_files = arena.source_files();
/// assert_eq!(source_files.len(), 1);
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The source code contains syntax errors that prevent AST construction
/// - The tree-sitter parser fails to generate a valid CST
/// - The [`Builder`] encounters malformed nodes during AST construction
///
/// The error collection mechanism reports all parsing errors at once rather than
/// failing on the first error, enabling faster iteration during development.
///
/// # Panics
///
/// Panics if the Inference language grammar cannot be loaded into the tree-sitter
/// parser. This indicates a critical setup issue with the `tree-sitter-inference`
/// dependency.
///
/// [`SourceFile`]: inference_ast::nodes::SourceFile
pub fn parse(source_code: &str) -> anyhow::Result<Arena> {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code, None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    let mut builder = Builder::new();
    builder.add_source_code(root_node, code);
    let arena = builder.build_ast()?;
    Ok(arena)
}

/// Performs bidirectional type checking and inference on the AST.
///
/// This function analyzes the AST to build a complete type mapping for all
/// expressions, statements, and declarations. It implements a multi-phase
/// type checking algorithm with error recovery.
///
/// ## Type Checking Phases
///
/// 1. **Process Directives**: Registers raw import statements
/// 2. **Register Types**: Collects struct, enum, and type alias definitions
/// 3. **Resolve Imports**: Binds import paths to symbols from other modules
/// 4. **Collect Functions**: Registers function signatures and constants
/// 5. **Infer Variables**: Type-checks function bodies and local variables
///
/// The result is a [`TypedContext`] that maps AST node IDs to their inferred
/// [`TypeInfo`]. This context is required for code generation.
///
/// # Examples
///
/// ```ignore
/// use inference::{parse, type_check};
///
/// let source = r#"
///     fn multiply(x: i32, y: i32) -> i32 {
///         return x * y;
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
///
/// // The typed context now contains type information for all nodes
/// let functions = typed_context.arena().functions();
/// assert_eq!(functions.len(), 1);
/// ```
///
/// # Type Inference
///
/// The type checker uses bidirectional type checking:
/// - **Inference mode**: Synthesizes types from expressions (bottom-up)
/// - **Checking mode**: Validates expressions against expected types (top-down)
///
/// This hybrid approach enables:
/// - Type inference for local variables
/// - Generic function parameter inference
/// - Method resolution on struct types
/// - Operator overloading resolution
///
/// # Error Recovery
///
/// The type checker collects multiple errors before failing, allowing
/// developers to see all type errors at once. Common error categories:
/// - Undefined variables, functions, or types
/// - Type mismatches in assignments and return statements
/// - Invalid operations for given types
/// - Visibility violations (private access)
/// - Unresolved imports
///
/// # Errors
///
/// Returns an error if:
/// - Type inference fails due to ambiguous or contradictory constraints
/// - Required type information is missing (e.g., untyped function parameters)
/// - Type mismatches occur between expressions and their expected types
/// - Symbols are used before being defined
/// - Import resolution fails
///
/// The error message aggregates all type checking errors found during analysis.
///
/// [`TypeInfo`]: inference_type_checker::type_info::TypeInfo
pub fn type_check(arena: Arena) -> anyhow::Result<TypedContext> {
    let type_checker_builder =
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)?;
    Ok(type_checker_builder.typed_context())
}

/// Performs semantic analysis on the typed AST.
///
/// This function is currently a placeholder for future semantic analysis passes.
/// Planned analyses include:
/// - Dead code detection
/// - Unused variable warnings
/// - Unreachable code analysis
/// - Control flow validation
/// - Lifetime and borrow checking (if applicable)
///
/// # Current Status
///
/// **Work in Progress**: This phase is under active development and currently
/// returns `Ok(())` without performing any checks. Once implemented, it will
/// provide additional semantic guarantees beyond type correctness.
///
/// # Examples
///
/// ```ignore
/// use inference::{parse, type_check, analyze};
///
/// let source = r#"fn main() { return 0; }"#;
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
///
/// // Currently a no-op, but will perform semantic checks in the future
/// analyze(&typed_context)?;
/// ```
///
/// # Errors
///
/// Currently always returns `Ok(())`. Future implementations will return errors
/// for semantic violations that are not type errors, such as:
/// - Use of uninitialized variables
/// - Unreachable code paths
/// - Dead code that should be removed
/// - Control flow violations (e.g., missing return statements)
pub fn analyze(typed_context: &TypedContext) -> anyhow::Result<()> {
    inference_semantic_analysis::analyze(typed_context)
}

/// Generates WebAssembly binary format from the typed AST.
///
/// This function compiles the typed AST into WebAssembly bytecode using LLVM
/// as an intermediate representation. The compilation pipeline:
///
/// 1. Transforms AST nodes into LLVM IR
/// 2. Applies LLVM optimization passes
/// 3. Compiles LLVM IR to WebAssembly using `inf-llc`
/// 4. Links the object files using `rust-lld`
/// 5. Returns the final WASM binary
///
/// ## Non-Deterministic Extensions
///
/// Inference extends WebAssembly with custom instructions for non-deterministic
/// computation. These are encoded using reserved opcodes and implemented as
/// LLVM intrinsics:
///
/// | Instruction | Opcode      | Purpose |
/// |-------------|-------------|---------|
/// | `forall`    | `0xfc 0x3a` | Universal quantification block |
/// | `exists`    | `0xfc 0x3b` | Existential quantification block |
/// | `uzumaki`   | `0xfc 0x3c` | Non-deterministic value generation |
/// | `assume`    | `0xfc 0x3d` | Constraint specification |
/// | `unique`    | `0xfc 0x3e` | Uniqueness constraint |
///
/// These extensions enable formal verification workflows by making
/// non-deterministic choices explicit in the binary format.
///
/// # Examples
///
/// ```ignore
/// use inference::{parse, type_check, codegen};
/// use std::fs;
///
/// let source = r#"
///     fn factorial(n: i32) -> i32 {
///         if n <= 1 {
///             return 1;
///         } else {
///             return n * factorial(n - 1);
///         }
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let wasm_bytes = codegen(&typed_context)?;
///
/// fs::write("out/factorial.wasm", &wasm_bytes)?;
/// ```
///
/// # Generated WASM Structure
///
/// The generated WebAssembly module includes:
/// - **Type section**: Function signatures
/// - **Import section**: External dependencies (if any)
/// - **Function section**: Function declarations
/// - **Memory section**: Linear memory allocation
/// - **Export section**: Public API exports
/// - **Code section**: Function bodies in WASM bytecode
///
/// # Errors
///
/// Returns an error if:
/// - LLVM IR generation fails for any AST node
/// - The LLVM optimization passes encounter invalid IR
/// - The `inf-llc` compiler fails to generate object files
/// - The `rust-lld` linker fails to produce a valid WASM binary
/// - Required external binaries (`inf-llc`, `rust-lld`) are not found
/// - Type information is missing or inconsistent in the [`TypedContext`]
///
/// # Dependencies
///
/// This function requires the following external binaries:
/// - `inf-llc`: LLVM compiler with Inference intrinsic support
/// - `rust-lld`: WebAssembly linker
///
/// These must be available in the `external/bin/` directory relative to the
/// binary location. See the repository README for download instructions.
pub fn codegen(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
    inference_wasm_codegen::codegen(typed_context)
}

/// Translates WebAssembly binary to Rocq (Coq) verification code.
///
/// This function parses a WebAssembly binary and generates equivalent Rocq
/// (formerly Coq) definitions that can be used for formal verification. The
/// translation preserves the semantics of the WebAssembly program, including
/// Inference's non-deterministic instruction extensions.
///
/// ## Translation Process
///
/// 1. Parse the WebAssembly binary format
/// 2. Extract function signatures, types, and module structure
/// 3. Translate each function body to Rocq tactics and definitions
/// 4. Generate Rocq module with imports and exports
/// 5. Include axioms for non-deterministic instructions
///
/// ## Rocq Output Structure
///
/// The generated `.v` file contains:
/// - Module header and imports
/// - Type definitions for WebAssembly types
/// - Function definitions as Rocq `Definition` or `Fixpoint`
/// - Axioms for non-deterministic operations (`forall`, `exists`, `uzumaki`)
/// - Export declarations for public API
///
/// # Examples
///
/// ```ignore
/// use inference::{parse, type_check, codegen, wasm_to_v};
/// use std::fs;
///
/// let source = r#"
///     fn is_even(n: i32) -> bool {
///         return n % 2 == 0;
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let wasm_bytes = codegen(&typed_context)?;
/// let rocq_code = wasm_to_v("EvenChecker", &wasm_bytes)?;
///
/// fs::write("out/even_checker.v", rocq_code)?;
/// ```
///
/// ## Example Output
///
/// ```coq
/// Require Import ZArith.
/// Require Import List.
/// Import ListNotations.
///
/// Module EvenChecker.
///   Definition is_even (n : Z) : bool :=
///     Z.eqb (Z.modulo n 2) 0.
/// End EvenChecker.
/// ```
///
/// ## Non-Deterministic Instructions
///
/// Non-deterministic Inference instructions are translated to Rocq axioms:
/// - `forall` → `Axiom forall_block : forall T, (T -> Prop) -> Prop`
/// - `exists` → `Axiom exists_block : forall T, (T -> Prop) -> Prop`
/// - `uzumaki` → `Axiom uzumaki : forall T, T`
///
/// These axioms allow verification of properties that must hold for all possible
/// non-deterministic choices.
///
/// # Parameters
///
/// - `mod_name`: The name of the Rocq module to generate. Should be a valid
///   Rocq identifier (alphanumeric, starting with uppercase letter).
/// - `wasm`: The WebAssembly binary to translate, as produced by [`codegen`].
///
/// # Errors
///
/// Returns an error if:
/// - The WebAssembly binary is malformed or cannot be parsed
/// - The WASM structure contains unsupported features
/// - Translation of a specific instruction or construct fails
/// - The module name is invalid for Rocq
///
/// # Use Cases
///
/// The generated Rocq code enables:
/// - **Correctness proofs**: Prove that functions satisfy their specifications
/// - **Equivalence proofs**: Show two implementations are equivalent
/// - **Security properties**: Verify absence of vulnerabilities
/// - **Non-deterministic reasoning**: Prove properties hold for all possible
///   non-deterministic choices
///
/// # See Also
///
/// - [Rocq Documentation](https://rocq-lang.org)
/// - [WebAssembly Specification](https://webassembly.github.io/spec/)
/// - Inference language spec for non-deterministic instruction semantics
pub fn wasm_to_v(mod_name: &str, wasm: &Vec<u8>) -> anyhow::Result<String> {
    if let Ok(v) =
        inference_wasm_to_v_translator::wasm_parser::translate_bytes(mod_name, wasm.as_slice())
    {
        Ok(v)
    } else {
        Err(anyhow::anyhow!("Error translating WebAssembly to V"))
    }
}

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
//! ## Quick Start
//!
//! ```rust,no_run
//! use inference::{parse, type_check, codegen};
//!
//! fn compile(source_code: &str) -> anyhow::Result<Vec<u8>> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     let wasm_bytes = codegen(&typed_context)?;
//!     Ok(wasm_bytes)
//! }
//! ```
//!
//! ## Compilation Pipeline
//!
//! ### Phase 1: Parse
//!
//! Transforms source code into an arena-based Abstract Syntax Tree (AST).
//!
//! ```rust,no_run
//! use inference::parse;
//!
//! let source = r#"fn main() { return 42; }"#;
//! let arena = parse(source)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The parser uses tree-sitter for concrete syntax tree (CST) construction,
//! then transforms it into a typed AST stored in an [`Arena`]. The arena provides
//! O(1) node lookup and maintains parent-child relationships for efficient traversal.
//!
//! [`Arena`]: inference_ast::arena::Arena
//!
//! ### Phase 2: Type Check
//!
//! Performs type inference and validation on the AST.
//!
//! ```rust,no_run
//! use inference::{parse, type_check};
//!
//! let source = "fn add(x: i32, y: i32) -> i32 { return x + y; }";
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The type checker operates in multiple phases:
//! 1. **Process directives**: Register raw import statements
//! 2. **Register types**: Collect struct, enum, and type alias definitions
//! 3. **Resolve imports**: Bind import paths to symbols from other modules
//! 4. **Collect functions**: Register function signatures and constants
//! 5. **Infer variables**: Type-check function bodies and local variables
//!
//! The result is a [`TypedContext`] that maps AST nodes to their inferred types.
//!
//! [`TypedContext`]: inference_type_checker::typed_context::TypedContext
//!
//! ### Phase 3: Analyze
//!
//! Performs semantic analysis on the typed AST. This phase is currently under
//! active development (WIP) and serves as a placeholder for future semantic
//! analysis passes.
//!
//! ```rust,no_run
//! use inference::{parse, type_check, analyze};
//!
//! let source = "fn main() { return 0; }";
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! analyze(&typed_context)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! **Status**: Work in progress. Currently returns `Ok(())` without performing checks.
//!
//! ### Phase 4: Codegen
//!
//! Generates WebAssembly binary format from the typed AST.
//!
//! ```rust,no_run
//! use inference::{parse, type_check, codegen};
//!
//! let source = "fn factorial(n: i32) -> i32 { if n <= 1 { return 1; } else { return n * factorial(n - 1); } }";
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! let wasm_bytes = codegen(&typed_context)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The code generator uses LLVM IR as an intermediate representation and supports
//! custom intrinsics for non-deterministic instructions specific to Inference:
//! - `@` (uzumaki) - Non-deterministic value generation (rvalue)
//! - `forall { }` - Universal quantification blocks
//! - `exists { }` - Existential quantification blocks
//! - `assume { }` - Precondition filtering blocks
//! - `unique { }` - Uniqueness constraint blocks
//!
//! ### Phase 5: WASM to Rocq Translation
//!
//! Translates WebAssembly binary to Rocq (Coq) verification code.
//!
//! ```rust,no_run
//! use inference::{parse, type_check, codegen, wasm_to_v};
//!
//! let source = "fn is_even(n: i32) -> bool { return n % 2 == 0; }";
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! let wasm_bytes = codegen(&typed_context)?;
//! let rocq_code = wasm_to_v("MyModule", &wasm_bytes)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The resulting `.v` file can be used with Rocq for formal verification of
//! program properties. Non-deterministic instructions are translated to Rocq axioms
//! that enable reasoning about all possible execution paths.
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
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    inference (this crate)                   │
//! │  ┌────────┐  ┌────────────┐  ┌─────────┐  ┌─────────────┐ │
//! │  │ parse  │→ │ type_check │→ │ analyze │→ │   codegen   │ │
//! │  └────────┘  └────────────┘  └─────────┘  └─────────────┘ │
//! │                                                      ↓      │
//! │                                               ┌─────────────┤
//! │                                               │ wasm_to_v   │
//! │                                               └─────────────┘
//! └─────────────────────────────────────────────────────────────┘
//!          ↓              ↓              ↓              ↓
//!   inference_ast  type_checker  (WIP)  wasm_codegen  wasm_to_v
//! ```
//!
//! ## Error Handling
//!
//! All public functions return `anyhow::Result` for flexible error propagation.
//! Each phase collects and reports errors before failing, allowing users to see
//! all issues at once rather than fixing one error at a time.
//!
//! ```rust,no_run
//! use inference::parse;
//!
//! let invalid_source = "fn main( { return 42 }"; // missing closing paren
//! match parse(invalid_source) {
//!     Ok(_) => println!("Success"),
//!     Err(e) => eprintln!("Parse error: {}", e),
//! }
//! ```
//!
//! ## Complete Pipeline Examples
//!
//! ### Standard Compilation
//!
//! ```rust,no_run
//! use inference::{parse, type_check, analyze, codegen};
//!
//! fn compile_to_wasm(source_code: &str) -> anyhow::Result<Vec<u8>> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     analyze(&typed_context)?;
//!     codegen(&typed_context)
//! }
//! ```
//!
//! ### Verification Workflow
//!
//! ```rust,no_run
//! use inference::{parse, type_check, codegen, wasm_to_v};
//!
//! fn compile_to_rocq(source_code: &str, module_name: &str) -> anyhow::Result<String> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     let wasm = codegen(&typed_context)?;
//!     wasm_to_v(module_name, &wasm)
//! }
//! ```
//!
//! ### Non-Deterministic Program Example
//!
//! ```rust,no_run
//! use inference::{parse, type_check, codegen};
//!
//! fn compile_nondet_example() -> anyhow::Result<Vec<u8>> {
//!     let source = r#"
//!         pub fn verify_property() {
//!             forall {
//!                 let x: i32 = @;
//!                 let y: i32 = @;
//!                 assume {
//!                     assert(x < y);
//!                 }
//!                 assert(x <= y);
//!             }
//!         }
//!     "#;
//!
//!     let arena = parse(source)?;
//!     let typed_context = type_check(arena)?;
//!     codegen(&typed_context)
//! }
//! ```
//!
//! ## Limitations
//!
//! - **Single-file support**: Multi-file compilation is not yet implemented.
//!   The AST expects a single source file as input.
//! - **Analyze phase**: The semantic analysis phase is work-in-progress and
//!   currently returns `Ok(())` without performing any checks.
//! - **External dependencies**: Code generation requires `inf-llc` and `rust-lld`
//!   binaries in the `external/bin/` directory.
//!
//! ## CLI Tools
//!
//! For command-line usage, use one of the CLI tools:
//!
//! - **`infs`** - Modern unified toolchain manager (recommended)
//! - **`infc`** - Legacy compiler CLI
//!
//! Both tools use this crate internally for compilation.
//!
//! ## See Also
//!
//! ### Internal Crates
//!
//! - [`inference_ast::arena::Arena`] - Arena-based AST storage
//! - [`inference_ast::builder::Builder`] - AST construction from tree-sitter CST
//! - [`inference_type_checker::TypeCheckerBuilder`] - Type checking entry point
//! - [`inference_type_checker::typed_context::TypedContext`] - Type information storage
//! - [`inference_wasm_codegen::codegen`] - WebAssembly code generation entry point
//! - [`inference_wasm_to_v_translator::wasm_parser`] - WASM to Rocq translation
//!
//! ### External Resources
//!
//! - [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
//! - [Inference Book](https://github.com/Inferara/book)
//! - [Tree-sitter Grammar](https://github.com/Inferara/tree-sitter-inference)
//! - [LLVM Intrinsics for Non-deterministic Instructions](https://github.com/Inferara/llvm-project/pull/2)

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
/// ## Basic Function Parsing
///
/// ```rust,no_run
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
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Querying the AST
///
/// ```rust,no_run
/// use inference::parse;
///
/// let source = "fn factorial(n: i32) -> i32 { return n; }";
/// let arena = parse(source)?;
///
/// // Access parsed functions
/// let functions = arena.functions();
/// assert_eq!(functions.len(), 1);
/// assert_eq!(functions[0].name.name, "factorial");
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Non-deterministic Constructs
///
/// ```rust,no_run
/// use inference::parse;
///
/// let source = r#"
///     fn verify() {
///         forall {
///             let x: i32 = @;
///             assert(x >= 0 || x < 0);
///         }
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let functions = arena.functions();
/// assert_eq!(functions.len(), 1);
/// # Ok::<(), anyhow::Error>(())
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
/// This function will panic if the Inference language grammar cannot be loaded
/// into the tree-sitter parser. This indicates a critical setup issue with the
/// `tree-sitter-inference` dependency and should never occur in normal operation.
///
/// [`SourceFile`]: inference_ast::nodes::SourceFile
/// [`Builder`]: inference_ast::builder::Builder
/// [`Arena`]: inference_ast::arena::Arena
pub fn parse(source_code: &str) -> anyhow::Result<Arena> {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .map_err(|e| anyhow::anyhow!("Failed to load Inference grammar: {e}"))?;
    let tree = parser
        .parse(source_code, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse source code"))?;
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
/// ## Basic Type Checking
///
/// ```rust,no_run
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
/// let functions = typed_context.functions();
/// assert_eq!(functions.len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Type Inference
///
/// ```rust,no_run
/// use inference::{parse, type_check};
///
/// let source = r#"
///     fn infer_example() -> i32 {
///         let x = 42;  // Type inferred as i32
///         let y = x + 1;  // Also i32
///         return y;
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Struct Type Checking
///
/// ```rust,no_run
/// use inference::{parse, type_check};
///
/// let source = r#"
///     struct Point {
///         x: i32;
///         y: i32;
///         fn distance_squared() -> i32 {
///             return self.x * self.x + self.y * self.y;
///         }
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Type Inference Strategy
///
/// The type checker uses bidirectional type checking:
/// - **Inference mode**: Synthesizes types from expressions (bottom-up)
/// - **Checking mode**: Validates expressions against expected types (top-down)
///
/// This hybrid approach enables:
/// - Type inference for local variables
/// - Generic function parameter inference
/// - Method resolution on struct types
/// - Operator type resolution
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
/// [`TypedContext`]: inference_type_checker::typed_context::TypedContext
pub fn type_check(arena: Arena) -> anyhow::Result<TypedContext> {
    let type_checker_builder =
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)?;
    Ok(type_checker_builder.typed_context())
}

/// Performs semantic analysis on the typed AST.
///
/// This function runs semantic checks that are beyond the scope of type checking,
/// such as detecting prohibited combined unary operators, and other
/// scope-bounded language constraints.
///
/// Diagnostics are reported with three severity levels:
/// - `Error` — semantic violations that must be fixed
/// - `Warning` — suspicious patterns that may indicate bugs
/// - `Info` — informational notes about code style or usage
///
/// # Examples
///
/// ```rust,no_run
/// use inference::{parse, type_check, analyze};
///
/// let source = r#"fn main() { return 0; }"#;
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// analyze(&typed_context)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error if any semantic diagnostic with `Error` severity is found.
/// Warnings are reported to stderr but do not fail compilation.
pub fn analyze(ctx: &TypedContext) -> anyhow::Result<()> {
    let result = inference_semantic_analysis::analyze(ctx);

    // Report warnings to stderr
    if result.has_warnings() {
        eprintln!("{}", result.format_warnings());
    }

    // Only fail on errors
    if result.has_errors() {
        return Err(anyhow::anyhow!("{}", result.format_errors()));
    }
    Ok(())
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
/// | `@` (uzumaki) | `0xfc 0x3c` | Non-deterministic value generation |
/// | `forall`    | `0xfc 0x3a` | Universal quantification block |
/// | `exists`    | `0xfc 0x3b` | Existential quantification block |
/// | `assume`    | `0xfc 0x3d` | Precondition filtering |
/// | `unique`    | `0xfc 0x3e` | Uniqueness constraint |
///
/// These extensions enable formal verification workflows by making
/// non-deterministic choices explicit in the binary format.
///
/// # Examples
///
/// ## Basic Compilation
///
/// ```rust,no_run
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
/// fs::write("factorial.wasm", &wasm_bytes)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Non-Deterministic Code Generation
///
/// ```rust,no_run
/// use inference::{parse, type_check, codegen};
///
/// let source = r#"
///     pub fn verify_addition() {
///         forall {
///             let a: i32 = @;
///             let b: i32 = @;
///             assume {
///                 assert(a >= 0);
///                 assert(b >= 0);
///             }
///             assert(a + b >= a);
///             assert(a + b >= b);
///         }
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let wasm = codegen(&typed_context)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Public Function Export
///
/// ```rust,no_run
/// use inference::{parse, type_check, codegen};
///
/// let source = r#"
///     pub fn add(x: i32, y: i32) -> i32 {
///         return x + y;
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let wasm = codegen(&typed_context)?;
/// // The function "add" will be exported in the WASM module
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Generated WASM Structure
///
/// The generated WebAssembly module includes:
/// - **Type section**: Function signatures
/// - **Import section**: External dependencies (if any)
/// - **Function section**: Function declarations
/// - **Memory section**: Linear memory allocation
/// - **Export section**: Public API exports (functions marked `pub`)
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
/// - More than one source file is present (multi-file not yet supported)
///
/// # Dependencies
///
/// This function requires the following external binaries:
/// - **inf-llc**: Modified LLVM compiler with Inference intrinsic support
/// - **rust-lld**: WebAssembly linker from the Rust toolchain
///
/// These must be available in the `external/bin/{platform}/` directory relative to the
/// binary location, where `{platform}` is `linux`, `macos`, or `windows`.
/// See the repository README for download instructions.
///
/// # Platform Support
///
/// - Linux x86-64 (requires libLLVM.so in `external/lib/linux/`)
/// - macOS Apple Silicon (M1/M2/M3)
/// - Windows x86-64 (requires DLLs in `external/bin/windows/`)
///
/// [`TypedContext`]: inference_type_checker::typed_context::TypedContext
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
/// - Axioms for non-deterministic operations (`forall`, `exists`, `@`)
/// - Export declarations for public API
///
/// # Examples
///
/// ## Basic Translation
///
/// ```rust,no_run
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
/// fs::write("even_checker.v", rocq_code)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Non-Deterministic Code Translation
///
/// ```rust,no_run
/// use inference::{parse, type_check, codegen, wasm_to_v};
///
/// let source = r#"
///     pub fn verify_commutativity() {
///         forall {
///             let x: i32 = @;
///             let y: i32 = @;
///             assert(x + y == y + x);
///         }
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let wasm = codegen(&typed_context)?;
/// let rocq = wasm_to_v("CommutativityProof", &wasm)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Example Rocq Output
///
/// For a simple function like `fn add(a: i32, b: i32) -> i32 { return a + b; }`:
///
/// ```coq
/// Require Import ZArith.
/// Require Import List.
/// Import ListNotations.
///
/// Module AddModule.
///   Definition add (a : Z) (b : Z) : Z :=
///     Z.add a b.
/// End AddModule.
/// ```
///
/// ## Non-Deterministic Instructions
///
/// Non-deterministic Inference instructions are translated to Rocq axioms:
/// - `@` (uzumaki) → `Axiom uzumaki : forall T, T`
/// - `forall { }` → `Axiom forall_block : forall T, (T -> Prop) -> Prop`
/// - `exists { }` → `Axiom exists_block : forall T, (T -> Prop) -> Prop`
/// - `assume { }` → `Axiom assume_block : forall T, (T -> Prop) -> Prop`
///
/// These axioms allow verification of properties that must hold for all possible
/// non-deterministic choices.
///
/// # Parameters
///
/// - `mod_name`: The name of the Rocq module to generate. Should be a valid
///   Rocq identifier (alphanumeric, starting with an uppercase letter).
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
/// Error messages will indicate "Error translating WebAssembly to V" with
/// details from the underlying parser.
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
/// # Verification Workflow
///
/// After generating the `.v` file:
/// 1. Load the file in Rocq (formerly Coq)
/// 2. Write theorems about the generated definitions
/// 3. Prove the theorems using Rocq tactics
/// 4. Extract verified code back to executable formats
///
/// # See Also
///
/// - [Rocq Documentation](https://rocq-lang.org)
/// - [WebAssembly Specification](https://webassembly.github.io/spec/)
/// - [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
/// - [`inference_wasm_to_v_translator`] for implementation details
pub fn wasm_to_v(mod_name: &str, wasm: &Vec<u8>) -> anyhow::Result<String> {
    if let Ok(v) =
        inference_wasm_to_v_translator::wasm_parser::translate_bytes(mod_name, wasm.as_slice())
    {
        Ok(v)
    } else {
        Err(anyhow::anyhow!("Error translating WebAssembly to V"))
    }
}

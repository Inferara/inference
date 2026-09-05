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
//! .inf source → parser → Typed AST → Type Check → WASM → Rocq (.v)
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
//! fn compile(source_code: &str) -> anyhow::Result<inference_wasm_codegen::CodegenOutput> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     let codegen_output = codegen(&typed_context, "module")?;
//!     Ok(codegen_output)
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
//! The parser is a resilient recursive-descent front end
//! ([`inference_parser`]) that lowers the source directly into a typed AST
//! stored in an [`AstArena`]. The arena provides O(1) node lookup and maintains
//! parent-child relationships for efficient traversal.
//!
//! [`AstArena`]: inference_ast::arena::AstArena
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
//! Performs semantic analysis on the typed AST. Uses a Rule-based architecture where each check is
//! an independent struct implementing the `Rule` trait.
//!
//! ```rust,no_run
//! use inference::{parse, type_check, analyze};
//!
//! let source = "fn main() { return 0; }";
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! let _analysis_result = analyze(&typed_context)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
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
//! let codegen_output = codegen(&typed_context, "module")?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The code generator produces WebAssembly binary directly via `wasm-encoder` and supports
//! custom instructions for non-deterministic operations specific to Inference:
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
//! use inference::{parse, type_check, codegen};
//!
//! let source = "fn is_even(n: i32) -> bool { return n % 2 == 0; }";
//! let arena = parse(source)?;
//! let typed_context = type_check(arena)?;
//! let codegen_output = codegen(&typed_context, "module")?;
//! // WASM bytes are directly available from codegen output:
//! // wasm_to_v("MyModule", codegen_output.wasm(), codegen_output.spec_func_indices_by_spec())
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
//! - [`inference_ast`] - Arena-based AST data model
//! - [`inference_parser`] - Resilient parser front end
//! - [`inference_type_checker`] - Bidirectional type checking with error recovery
//! - [`inference_wasm_codegen`] - WebAssembly code generation via wasm-encoder
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
//!   inference_ast  type_checker  analysis  wasm_codegen  wasm_to_v
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
//! fn compile_to_wasm(source_code: &str) -> anyhow::Result<inference_wasm_codegen::CodegenOutput> {
//!     let arena = parse(source_code)?;
//!     let typed_context = type_check(arena)?;
//!     let _analysis_result = analyze(&typed_context)?;
//!     codegen(&typed_context, "module")
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
//!     let codegen_output = codegen(&typed_context, "module")?;
//!     let rocq_code = wasm_to_v(
//!         module_name,
//!         codegen_output.wasm(),
//!         codegen_output.spec_func_indices_by_spec(),
//!         codegen_output.hspecs(),
//!     )?;
//!     Ok(rocq_code)
//! }
//! ```
//!
//! ### Non-Deterministic Program Example
//!
//! ```rust,no_run
//! use inference::{parse, type_check, codegen};
//!
//! fn compile_nondet_example() -> anyhow::Result<inference_wasm_codegen::CodegenOutput> {
//!     let source = r#"
//!         spec Ordering {
//!             fn verify_property() forall {
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
//!     codegen(&typed_context, "module")
//! }
//! ```
//!
//! ## Limitations
//!
//! - **Analyze phase**: [`analyze`] runs the whole registered rule set, but
//!   against the *default* memory layout. A caller that emits a different one
//!   must use [`analyze_with_options`] and pass the matching stack budget, or
//!   A036 measures cumulative call-chain frame usage against a shadow stack the
//!   artifact does not have — accepting a program that overflows a smaller stack,
//!   or rejecting one a larger stack accommodates.
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
//! - [`inference_ast::arena::AstArena`] - Arena-based AST storage
//! - [`inference_parser::parse`] - Source-to-AST parsing entry point
//! - [`inference_type_checker::TypeCheckerBuilder`] - Type checking entry point
//! - [`inference_type_checker::typed_context::TypedContext`] - Type information storage
//! - [`inference_wasm_codegen::codegen`] - WebAssembly code generation entry point
//! - [`inference_wasm_to_v_translator::wasm_parser`] - WASM to Rocq translation
//!
//! ### External Resources
//!
//! - [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
//! - [Inference Book](https://github.com/Inferara/book)
//! - [Inference Grammar](https://github.com/Inferara/tree-sitter-inference)

pub use inference_analysis::errors::{AnalysisErrors, AnalysisResult};

/// Re-export of the analysis settings so a caller that configures code
/// generation can hand [`analyze_with_options`] a budget matching the artifact
/// it is about to emit, without a direct dependency on `inference-analysis`.
pub use inference_analysis::AnalysisOptions;
use inference_ast::arena::AstArena;
pub use inference_type_checker::typed_context::TypedContext;
/// Re-export of the lossless type-check entry point and its result types so
/// downstream consumers (IDE/LSP) get the structured diagnostics and the
/// partially-populated [`TypedContext`] without a direct dependency on
/// `inference-type-checker`. Mirrors the [`type_check`] wrapper, but keeps the
/// per-error [`TypeCheckError`] and file label instead of joining them into one
/// string.
///
/// [`TypeCheckError`]: inference_type_checker::errors::TypeCheckError
pub use inference_type_checker::{TypeCheckDiagnostic, TypeCheckOutcome};

/// Re-export of the structured type-check error so downstream consumers can
/// match on a [`TypeCheckDiagnostic`]'s variant and read its source location
/// (via [`TypeCheckError::location`]) without a direct dependency on
/// `inference-type-checker`. Mirrors the [`WasmToVError`]/[`LinkError`] re-exports.
///
/// [`TypeCheckError::location`]: inference_type_checker::errors::TypeCheckError::location
pub use inference_type_checker::errors::TypeCheckError;

pub mod extern_prelude;
pub mod wasm_link;

/// Re-export of the shared project front end so compiler-side consumers reach the
/// import-closure walk, file loaders, and manifest discovery as `inference::…`
/// without a direct dependency on `inference-project-model`. This is the same leaf
/// crate `ide-db` depends on directly, which is what keeps the compiler and the
/// IDE from ever disagreeing about which files a program is made of — there is one
/// closure-walk implementation, parameterized over a [`FileLoader`].
pub use inference_project_model::{
    load_project_resilient, load_project_resilient_with_root, manifest_source_root, parse_project,
    read_source_file, strip_utf8_bom, DiskLoader, FileLoader, FileParseErrors, ImportProblem,
    InferenceError, LoadedFile, ProjectParse, ProjectWarning, ResilientProjectParse,
};

/// Re-export of `rustc_hash::FxHashMap` so library consumers of `inference`
/// can construct the spec-funcs map passed to [`wasm_to_v`] without taking a
/// direct dependency on `rustc-hash`.
pub use rustc_hash::FxHashMap;

/// Re-export of the [`wasm_to_v`] error types so downstream consumers (CLI,
/// LSP, tools) can match on translation failures without taking a direct
/// dependency on `inference-wasm-to-v-translator`.
pub use inference_wasm_to_v_translator::errors::{InvalidIdentifierReason, WasmToVError};

/// Re-export of the static-merge linker's error type so downstream consumers
/// can match on link failures (e.g. an unsatisfied import or a Tier-C module)
/// without taking a direct dependency on `inference-wasm-linker`.
pub use inference_wasm_linker::LinkError;

/// Re-export of the static-merge linker's success types, so a consumer of
/// [`link_with_warnings`] can name and match on what it returns without taking a
/// direct dependency on `inference-wasm-linker`.
pub use inference_wasm_linker::{LinkOutput, LinkWarning};

/// Re-export of the static-merge linker's policy inputs, so a caller of
/// [`link_with_options`] can say what the merge should do with the verification
/// sections a linked library ships without taking a direct dependency on
/// `inference-wasm-linker`.
pub use inference_wasm_linker::{ExternalSpecPolicy, LinkOptions};

/// Re-export of the linker's write-set contract, which
/// [`wasm_link::resolve_external_modules`] produces and [`link`] consumes, so a
/// caller can carry one to the other without a direct dependency on
/// `inference-wasm-linker`.
pub use inference_wasm_linker::ImportWriteSet;

/// Re-export of the `inference.spec_funcs` custom-section identifiers so
/// downstream consumers (CLI tools, integration tests) share a single source
/// of truth with the codegen and translator crates.
pub use inference_wasm_codegen::{SPEC_FUNCS_SECTION_NAME, SPEC_FUNCS_SECTION_VERSION};

/// Re-export of the per-program `hassert` obligation map so consumers of
/// [`wasm_to_v`] can construct the argument (empty post-link, populated for the
/// pre-link cross-check) without depending on `inference-hassert` directly.
pub use inference_wasm_codegen::HSpecMap;

/// Runs `f` on a thread reserving [`inference_parser::MIN_COMPILE_STACK`].
///
/// The compiler's phases recurse once per level of the input's syntactic nesting,
/// and a stack overflow aborts the process rather than unwinding — so the stack
/// each phase gets cannot be left to whatever the host thread happens to have. That
/// default varies with the platform and with how the thread was created — a test
/// harness thread gets a fraction of what a process main thread gets — and none of
/// them reach the front end's requirement. Every embedder that drives the pipeline
/// in-process runs it through this helper, so the stack available to the recursive
/// phases is the same everywhere the compiler runs.
///
/// This generalizes a mitigation that already shipped once in this repository for
/// this exact failure: the language server hit it first — a pathological document
/// aborting the whole process and taking every open file's state with it — and gave
/// each of its threads an explicit big stack (`SERVER_STACK_SIZE`, `apps/lsp`). That
/// server keeps its own long-lived threads rather than calling this helper, and now
/// sizes them from the same constant.
///
/// `f` runs on a scoped thread and may borrow from its environment. Panics
/// propagate unchanged: the payload is re-raised on the calling thread with
/// [`std::panic::resume_unwind`], which does not run the panic hook a second time,
/// so the message still prints exactly once and a wrapped driver keeps the exit
/// code it had before. The one observable difference is the panic header, which
/// now names this thread rather than `main` — which is also why the thread is
/// named, since it makes an overflow say which thread overflowed.
///
/// The helper lives in the orchestration crate rather than beside the CLI driver
/// because `core/cli` is binary-only — it has no library target, so the
/// integration-test crates could not import a helper defined there — and because
/// every in-process consumer of the pipeline already links this crate. The `infs`
/// toolchain driver is deliberately not wrapped: it has no in-process compile path
/// and always spawns `infc` as a subprocess, so wrapping that binary covers
/// `infs build` and `infs run` as well, and `infs` need not grow a dependency on
/// the compiler back end.
///
/// # Panics
///
/// Panics if the operating system refuses the stack reservation, which leaves no
/// way to run the phases at all. Also re-raises any panic from `f` itself, so the
/// caller observes it exactly as if `f` had run inline.
pub fn with_compiler_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("inference-compile".to_owned())
            .stack_size(inference_parser::MIN_COMPILE_STACK)
            .spawn_scoped(scope, f)
            .expect("failed to spawn the compiler thread");
        match worker.join() {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

/// Parses source code and builds an arena-based Abstract Syntax Tree.
///
/// This function delegates to the [`inference_parser`] front end,
/// which lexes the source, parses it with a resilient recursive-descent grammar,
/// and lowers the result directly into an [`AstArena`].
///
/// The resulting [`AstArena`] stores all AST nodes with unique IDs and maintains
/// parent-child relationships for efficient traversal. Root nodes are
/// [`SourceFileData`] entries that represent the top-level compilation unit.
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
/// // Access parsed function definitions
/// let func_ids = arena.function_def_ids();
/// assert_eq!(func_ids.len(), 1);
/// assert_eq!(arena.def_name(func_ids[0]), "factorial");
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
/// let func_ids = arena.function_def_ids();
/// assert_eq!(func_ids.len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error if the source code contains syntax errors. The parser is
/// resilient and collects every syntax error rather than failing on the first
/// one, so the returned error aggregates all of them at once, enabling faster
/// iteration during development.
///
/// [`SourceFileData`]: inference_ast::nodes::SourceFileData
/// [`AstArena`]: inference_ast::arena::AstArena
pub fn parse(source_code: &str) -> anyhow::Result<AstArena> {
    let parsed = inference_parser::parse(source_code);
    if parsed.errors.is_empty() {
        return Ok(parsed.arena);
    }

    let lines: Vec<String> = parsed
        .errors
        .iter()
        .map(|error| {
            format!(
                "  {}:{}: {}",
                error.span.start_line, error.span.start_column, error.message
            )
        })
        .collect();
    Err(anyhow::anyhow!(
        "AST building failed due to errors:\n{}",
        lines.join("\n")
    ))
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
/// let func_ids = typed_context.function_def_ids();
/// assert_eq!(func_ids.len(), 1);
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
pub fn type_check(arena: AstArena) -> anyhow::Result<TypedContext> {
    let type_checker_builder =
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)?;
    Ok(type_checker_builder.typed_context())
}

/// Type-checks `arena` losslessly, returning the [`TypedContext`] together with
/// the structured type-check diagnostics instead of one aggregated string.
///
/// Unlike [`type_check`], which discards the context on any error and joins the
/// errors into a single [`anyhow::Error`], this preserves every error's variant,
/// per-file-local source location, and optional module-path file label, and
/// returns the (possibly partially populated) context alongside them. It is the
/// entry point tooling (IDE/LSP) uses to report diagnostics and still serve
/// features on the parts of the program that type-checked.
///
/// See [`TypeCheckOutcome`] for the guarantees the returned context provides
/// when errors are present. The runtime compilation pipeline keeps using
/// [`type_check`]; the two share exactly one checking implementation.
#[must_use = "the outcome carries both the typed context and the diagnostics"]
pub fn type_check_with_diagnostics(arena: AstArena) -> TypeCheckOutcome {
    inference_type_checker::check_with_diagnostics(arena)
}

/// Performs semantic analysis on the typed AST.
///
/// Runs the whole registered rule set on the typed AST, validating invariants
/// that go beyond type correctness: control flow, unreachable code, variable
/// initialization, recursion and cumulative stack depth, lint warnings, and the
/// codegen restrictions that describe constructs the type system admits but the
/// code generator cannot lower. `inference_analysis`'s module documentation
/// carries the catalogue rule by rule and is the list to consult; a summary
/// repeated here would go stale as rules are added.
///
/// This is the **default-layout** entry point: it measures A036 against the
/// stack budget a default build emits. A caller that configures the memory
/// layout must call [`analyze_with_options`] with the matching budget instead,
/// or that rule polices a shadow stack the artifact does not have.
///
/// # Examples
///
/// ```rust,no_run
/// use inference::{parse, type_check, analyze};
///
/// let source = r#"fn main() { return 0; }"#;
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let _analysis_result = analyze(&typed_context)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Returns `AnalysisErrors` when any rule produces an `Error`-severity finding.
/// A control-flow violation is one such finding — a `break` outside a loop, a
/// `return` inside one, an infinite loop with no `break` — but so are an
/// unreachable-statement, uninitialized-variable, recursion, stack-depth, or
/// unsupported-codegen-construct finding. Every rule runs before the errors are
/// returned, so one call reports everything that is wrong rather than the first
/// thing.
///
/// # Parameters
///
/// - `typed_context`: The typed AST context from [`type_check`]
///
/// # Returns
///
/// On success, returns an [`AnalysisResult`] containing any warnings and
/// informational findings collected during analysis.
pub fn analyze(typed_context: &TypedContext) -> Result<AnalysisResult, AnalysisErrors> {
    inference_analysis::analyze(typed_context)
}

/// Performs static analysis on the typed AST under the given artifact settings.
///
/// [`analyze`] assumes the default memory layout. A caller that compiles with a
/// different one must use this entry point and pass the matching stack budget,
/// or A036 measures call-chain depth against a shadow stack the emitted module
/// does not have.
///
/// # Errors
///
/// Returns `AnalysisErrors` on the same conditions as [`analyze`].
pub fn analyze_with_options(
    typed_context: &TypedContext,
    options: AnalysisOptions,
) -> Result<AnalysisResult, AnalysisErrors> {
    inference_analysis::analyze_with_options(typed_context, options)
}

/// Generates WebAssembly binary from a typed AST for the default target (`Wasm32`)
/// and default compilation mode (`Compile`).
///
/// This is a convenience wrapper around [`inference_wasm_codegen::codegen`] that
/// uses default settings. The returned [`CodegenOutput`] contains the WASM binary
/// bytes and compilation metadata.
///
/// `module_name` is written into the WASM module-name subsection and flows
/// downstream into the Rocq translator. The CLI passes the input file stem;
/// library callers can pass any Rocq-identifier-compatible name.
///
/// The output stays within the WebAssembly 1.0 instruction set. For
/// target-specific or proof-mode compilation, or to opt into a post-MVP
/// instruction family, call `inference_wasm_codegen::codegen()` directly with
/// an explicit `CodegenOptions` value.
///
/// # Errors
///
/// Returns an error if:
/// - WebAssembly generation fails for any AST node
/// - Type information is missing or inconsistent in the [`TypedContext`]
///
/// [`TypedContext`]: inference_type_checker::typed_context::TypedContext
/// [`CodegenOutput`]: inference_wasm_codegen::CodegenOutput
pub fn codegen(
    typed_context: &TypedContext,
    module_name: &str,
) -> anyhow::Result<inference_wasm_codegen::CodegenOutput> {
    inference_wasm_codegen::codegen(
        typed_context,
        module_name,
        inference_wasm_codegen::CodegenOptions::default(),
    )
}

/// Folds external `.wasm` modules into the codegen output, producing a single
/// self-contained module with no cross-module imports.
///
/// This is the post-codegen link step (Phase 4 of Issue #9). When a program
/// `use`s functions from an external module, [`codegen`] emits those calls as
/// WASM `(import …)` entries. This function consumes that intermediate module
/// plus the resolved external module bytes and merges the imported functions'
/// bodies in, re-indexing so the result imports nothing — the single artifact
/// the user asked for, ready for [`wasm_to_v`].
///
/// `externals` is the set of resolved, validated external module binaries, each
/// paired with the logical `::`-joined module name it was bound under so the
/// merge can match an import's recorded `(module, field)` against the right
/// external. When it is empty the call is a no-op pass-through: a program
/// without externs links to byte-identical output, so callers can route every
/// program through this step unconditionally.
///
/// # The two `contracts` modes
///
/// `contracts` carries what each bound `external fn` declaration said its
/// parameters may be written through, and it decides whether that check runs at
/// all. It is an explicit mode rather than an emptiness test, because an empty
/// list is a real and *strict* answer:
///
/// * `None` — **merge mechanics only.** Nothing is held to a write set. This is
///   the mode for a caller with no Inference source behind `main_wasm`.
/// * `Some(list)` — **checked.** Every satisfied import is held to the write set
///   `list` declares for it, and a merged closure that may store through a
///   parameter no entry declares `mut` fails the link. An import `list` does not
///   mention is held to writing nothing, never exempted.
///
/// The compiler driver supplies `Some(..)` for every program it compiles from
/// Inference source, with one entry per bound import, so the check is on
/// throughout the live pipeline.
///
/// # Errors
///
/// Returns an error if any module fails to parse, an import is left unsatisfied
/// by the supplied externals, or a merged function falls into the unsupported
/// Tier C — its module declares a data or element segment, or its closure names
/// the table space. In the checked mode it additionally fails when a merged
/// closure may store through a parameter the declaration did not declare `mut`
/// ([`LinkError::UndeclaredExternWrite`]), when no contract entry describes a
/// storing import at all ([`LinkError::UndescribedExternWrite`]), or when
/// `contracts` holds two entries for one `(module, field)`
/// ([`LinkError::DuplicateWriteContract`]). A main module carrying proof
/// obligations additionally fails when a function symbol one of them applies is
/// carried by no function of the merged output
/// ([`LinkError::UnresolvedObligationSymbol`]) or by more than one
/// ([`LinkError::AmbiguousObligationSymbol`]). The underlying error downcasts to
/// [`LinkError`].
///
/// Globals are classified on use, not declaration: a closure that reads or
/// writes one is Tier A — or Tier B if it also touches memory — and the
/// external's globals are merged into the output above main's with its accessors
/// remapped, an admission kept sound by address provenance tagging a
/// global-derived value `NotParam`, so a closure that computes a memory address
/// through a global is still rejected. That admission is what makes a real
/// toolchain artifact linkable when its closure genuinely reads or writes a
/// module global — a counter, a mode flag, a seed — and not only when its leaf
/// functions leave lld's `__stack_pointer` untouched.
pub fn link(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
) -> anyhow::Result<Vec<u8>> {
    link_with_warnings(main_wasm, externals, contracts).map(|out| out.wasm)
}

/// Folds external `.wasm` modules into the codegen output, reporting what the
/// completed link owes the user.
///
/// Identical to [`link`], including the byte-identical no-op path for a program
/// without externs and the two `contracts` modes documented there, but keeps the
/// [`LinkWarning`]s the merge raised instead of dropping them. Any caller that
/// can put text in front of a user should prefer this form: a warning describes
/// the artifact that was just written, and [`link`] discards it.
///
/// This wrapper is **not** interchangeable with
/// [`inference_wasm_linker::link_with_warnings`] on an empty `externals`. The
/// no-op path below returns the input bytes without running the linker at all,
/// so main-side shapes the linker rejects are accepted here: a data or element
/// segment, a start function, a table, a second memory, a float, `v128`, or
/// reference-typed value in one of main's own signatures, and a duplicated or
/// malformed `inference.spec_funcs` or `inference.hspecs` custom section. The
/// same holds for a malformed `contracts` argument: a list holding two entries
/// for one `(module, field)` is rejected by the linker and passes here, because
/// the fast path returns before the list is read. Two entry points reaching
/// different verdicts on identical bytes is recorded because a later caller will
/// otherwise assume it cannot happen. It is benign on the live pipeline — main
/// is always this compiler's own codegen output, and a program with no import to
/// satisfy has no declaration to contract — and the documented error contract is
/// honoured as written, since every input carrying an import to satisfy goes
/// through the linker.
///
/// # Errors
///
/// The same conditions as [`link`].
pub fn link_with_warnings(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
) -> anyhow::Result<LinkOutput> {
    link_with_options(main_wasm, externals, contracts, &LinkOptions::default())
}

/// Folds external `.wasm` modules into the codegen output under the given policy
/// inputs, reporting what the completed link owes the user.
///
/// Identical to [`link_with_warnings`], of which it is the general form: that is
/// this with [`LinkOptions::default`], whose external-specification policy is
/// [`ExternalSpecPolicy::Warn`]. Everything documented there — the
/// byte-identical no-op path for a program without externs, the two `contracts`
/// modes, and the entry-point divergence from
/// [`inference_wasm_linker::link_with_warnings`] — applies here unchanged.
///
/// # Errors
///
/// The same conditions as [`link`], plus — under
/// [`ExternalSpecPolicy::Adopt`] alone — every way an adoption can be refused;
/// see [`inference_wasm_linker::link_with_options`].
pub fn link_with_options(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
    options: &LinkOptions,
) -> anyhow::Result<LinkOutput> {
    // Byte-identical fast path *only* for a module that is provably import-free —
    // it is already the self-contained artifact this step would produce. A module
    // that still carries imports (e.g. a caller that passed no resolved externals
    // for a program that actually uses them), or one that does not parse, must go
    // through the linker so the unsatisfied-import / parse failure surfaces as an
    // error instead of being silently passed through. Keying the fast path on the
    // *module's own imports* rather than merely on `externals.is_empty()` keeps it
    // fail-closed and honours the documented error contract above.
    //
    // Its empty warning list is a fact about the path, not an omission — but the
    // fact rests on something nothing else records. Every `LinkWarning` variant
    // there is today concerns a merged external, and this path returns before any
    // external is examined, indeed only when there is none to examine. The type
    // is documented far more broadly than that, as anything a successful link
    // owes the user, so a variant about the reconciled memory or about main's own
    // shape would be dropped here with nothing failing. Adding one means deciding
    // whether it can arise with no externals, and moving this return if it can.
    // The same premise is what keeps this path equivalent under every
    // external-specification policy: both of the variants that policy raises are
    // raised only for an external that contributed at least one merged body, and
    // this path is taken only when there is no external at all.
    if externals.is_empty() && module_is_import_free(main_wasm) {
        return Ok(LinkOutput {
            wasm: main_wasm.to_vec(),
            warnings: Vec::new(),
        });
    }
    Ok(inference_wasm_linker::link_with_options(
        main_wasm, externals, contracts, options,
    )?)
}

/// Whether `wasm` parses and declares no imports. Returns `false` on any parse
/// failure or on the first surviving import, so [`link`] routes such a module
/// through the linker — which validates it and reports the precise error —
/// rather than taking the byte-identical no-op path.
fn module_is_import_free(wasm: &[u8]) -> bool {
    use inf_wasmparser::{Parser, Payload};
    for payload in Parser::new(0).parse_all(wasm) {
        match payload {
            Ok(Payload::ImportSection(reader)) => {
                // Any entry (well-formed or not) means the module is not yet
                // self-contained, so it must not take the no-op path.
                if reader.into_iter().next().is_some() {
                    return false;
                }
            }
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    true
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
///
/// let source = r#"
///     fn is_even(n: i32) -> bool {
///         return n % 2 == 0;
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let codegen_output = codegen(&typed_context, "module")?;
/// let rocq_code = wasm_to_v(
///     "EvenChecker",
///     codegen_output.wasm(),
///     codegen_output.spec_func_indices_by_spec(),
///     codegen_output.hspecs(),
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Non-Deterministic Code Translation
///
/// ```rust,no_run
/// use inference::{parse, type_check, codegen, wasm_to_v};
///
/// let source = r#"
///     spec Commutativity {
///         fn verify_commutativity() forall {
///             let x: i32 = @;
///             let y: i32 = @;
///             assert(x + y == y + x);
///         }
///     }
/// "#;
///
/// let arena = parse(source)?;
/// let typed_context = type_check(arena)?;
/// let codegen_output = codegen(&typed_context, "module")?;
/// let rocq_code = wasm_to_v(
///     "CommutativityProof",
///     codegen_output.wasm(),
///     codegen_output.spec_func_indices_by_spec(),
///     codegen_output.hspecs(),
/// )?;
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
/// - `spec_funcs_by_spec`: WASM function indices that originated from `spec`
///   blocks, grouped by spec name (typically obtained from
///   [`CodegenOutput::spec_func_indices_by_spec`]). Decides which functions the
///   emitted module record omits and how every surviving function reference is
///   renumbered; the per-spec obligations are carried separately, as `hassert`
///   payloads in `inference.hspecs`. Pass an empty `FxHashMap` when no spec
///   marker is needed.
///
/// # Errors
///
/// Returns an `anyhow::Result` whose underlying error is typically a
/// downcastable `inference_wasm_to_v_translator::errors::WasmToVError`:
///
/// - `WasmToVError::InvalidRocqIdentifier` — the module or a spec name does
///   not satisfy the Rocq identifier rules
/// - `WasmToVError::RocqStdlibShadow` — the module or a spec name would
///   shadow a Rocq stdlib type
/// - `WasmToVError::EmbeddedSpecMismatch` — the caller passed a non-empty
///   explicit spec map that disagrees with the binary's embedded section
/// - `WasmToVError::ModuleNameShadowsPreambleHelper` — the module name is one
///   of the helper definitions the emitted `.v` preamble always occupies
/// - `WasmToVError::WasmParse` — the WASM binary is malformed or contains
///   unsupported features
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
// FxHashMap is part of the public contract for spec maps — don't generalize to a BuildHasher bound.
#[allow(clippy::implicit_hasher)]
pub fn wasm_to_v(
    mod_name: &str,
    wasm: &[u8],
    spec_funcs_by_spec: &FxHashMap<String, Vec<u32>>,
    hspecs_by_spec: &HSpecMap,
) -> anyhow::Result<String> {
    inference_wasm_to_v_translator::wasm_parser::translate_bytes(
        mod_name,
        wasm,
        spec_funcs_by_spec,
        hspecs_by_spec,
    )
}

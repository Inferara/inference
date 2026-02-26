# Changelog

All notable changes to the Inference compiler project.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Codegen

- Switch from LLVM to direct WebAssembly emission via `wasm-encoder` ([#125])
  - Remove all LLVM dependencies: `inkwell`, `build.rs`, external binaries (`inf-llc`, `rust-lld`)
  - Rewrite `compiler.rs` to generate WASM binary directly in-process
  - Non-deterministic instructions emitted as custom opcodes via `Function::raw()` byte sequences
  - Custom opcodes in 0xfc prefix space: uzumaki (0x31/0x32), forall (0x3a), exists (0x3b), assume (0x3c), unique (0x3d)
  - Reactor model: all `pub` functions exported individually, no `_start` entry point
- Add compilation architecture with `CodegenOutput` boundary ([#97], [#125])
  - `codegen()` returns `CodegenOutput` (WASM bytes + metadata)
  - `CodegenOutput` carries WASM binary, target, mode, opt level, module name, and `has_main` flag
  - New `Target` (Wasm32/Soroban), `CompilationMode` (Compile/Proof), and `OptLevel` (O0–O3/Os/Oz) enums
- Add per-function optimization strategy for proof mode (Decision #32) ([#97])
  - Spec functions compiled unoptimized to preserve structural correspondence with source for Rocq translation
  - Execution functions use target's release optimization so proofs cover actual deployed code
  - `OptLevel` is currently metadata only; optimization passes planned for future
- Add validation guards in `codegen()`: reject proof mode with non-Wasm32 targets, reject Soroban with non-det operations ([#97])
- Upgrade shadowing detection from `debug_assert!` to `assert!` in `pre_scan_locals` — fires in release builds for parameter, constant, and variable name collisions in `locals_map`
- Add `Statement::Loop` body recursion to `pre_scan_locals()` — locals inside loop bodies will be pre-registered when loop lowering is implemented
- Replace silent `if let ArgumentType::Argument` skip with exhaustive `match` covering `SelfReference`, `IgnoreArgument`, and `Type` variants, each with an explicit `todo!()`
- Add assignment statement lowering to WebAssembly codegen ([#146])
  - `mut` keyword support in AST: `is_mut: bool` field on `VariableDefinitionStatement`
  - Mutability enforcement in type-checker: `AssignToImmutable` error for assignment to non-`mut` variables
  - `lower_assign_statement()` emits `lower_expression(rhs)` + `LocalSet` for identifier targets
  - Mutable function parameters (`fn f(mut a: i32)`) supported
  - Number literal type propagation in assignments: `x = 42;` where `x: i64` correctly infers `42` as `i64`
  - Non-identifier targets (member access, array index) deferred to compound type support
- Add conditional statement lowering (`if`/`else`) to WebAssembly codegen ([#144])
  - `if`/`else` lowered to WASM structured control flow (`If`/`Else`/`End` with `BlockType::Empty`)
  - `pre_scan_locals` recurses into both if and else arms to declare locals upfront (WASM requirement)
  - Nested if statements supported via recursive descent
  - Emit `unreachable` instruction before function `end` for all non-void functions as defense-in-depth safety net (industry-standard pattern used by rustc, LLVM, GCC, Zig, Binaryen)
  - If-statements inside non-deterministic blocks (`forall`, `exists`, etc.) supported
- Add binary and unary expression lowering to WebAssembly codegen ([#140])
  - All arithmetic operators (`+`, `-`, `*`, `/`, `%`) for i32 and i64, signed and unsigned variants
  - All comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`) with correct sign-sensitive dispatch
  - All logical operators (`&&`, `||`) lowered as bitwise `i32.and`/`i32.or` (bool operands guaranteed by type-checker)
  - All bitwise operators (`&`, `|`, `^`) and shift operators (`<<`, `>>`) for i32 and i64
  - Unary negation (`-x`) via `0 - x` idiom (no native WASM integer negate instruction)
  - Logical not (`!x`) via `i32.eqz`
  - Bitwise not (`~x`) via `x ^ -1` idiom (works for both i32 and i64)
  - Parenthesized expressions lowered transparently (no extra instructions emitted)
  - Variable definition initializers now accept any value-producing expression (not just literals/identifiers/uzumaki)
  - `Pow` operator (`**`) deferred — no native WASM instruction
- Add function parameter lowering and function call support to WebAssembly codegen ([#136])
  - Function parameters mapped to WASM local indices `0..n`; body locals start at `n`
  - Pre-scan builds `func_name_to_idx` map for forward reference support
  - `Expression::FunctionCall` lowered to `call` instruction with positional arguments
  - Void function calls in expression-statement position correctly omit `Drop`
  - Value-returning function calls in expression-statement position emit `Drop`
- Add local variable lowering (`let` bindings) to WebAssembly codegen ([#134])
  - Emit `local.set` / `local.get` for variable definitions with literal, identifier, and uzumaki initializers
  - Support all numeric types (i8, i16, i32, i64, u8, u16, u32, u64), bool, and uzumaki
  - Type-checker propagates declared type into numeric literal initializers for sub-i32 types
  - Refactor `ConstantDefinition` lowering to share `lower_literal` helper with `VariableDefinition` (~130 lines removed)
  - Remove dead `is_uzumaki: bool` field from `VariableDefinitionStatement` AST node

### CLI

- Simplify `infc` and `infs build` default behavior: running without phase flags now performs full compilation and writes `out/<name>.wasm` ([#138])
  - `infc example.inf` equivalent to `infc example.inf --codegen -o`
  - `infc example.inf -v` produces both `out/example.wasm` and `out/example.v`
  - Supplying `--parse`, `--analyze`, or `--codegen` still overrides the default
  - Matches conventional compiler UX (e.g. `gcc foo.c`)
- Add `BuildProfile` (Debug/Release) with `resolve_opt_level()` for target-aware optimization ([#97])
- Remove external toolchain dependencies: no `inf-llc`, `rust-lld`, or platform-specific library paths required ([#125])
- Defer WASM compilation until output files are actually needed (`-o` or `-v` flags) ([#97])

### Documentation

- Add compilation targets matrix documentation (`book/compilation_targets.md`) ([#97])
  - 6-option matrix: Compile/Proof x Debug/Release x with/without non-det operations
- Add `unreachable` emission rationale document (`book/unreachable-emission-in-codegen.md`) ([#144])
- Add arithmetic overflow in WASM codegen deep-dive (`book/arithmetic-overflow-in-wasm-codegen.md`) ([#146])
  - WASM wrapping semantics, trapping instructions, negation behavior
  - Comparison with Rust, C, Zig, Go, Java overflow handling
  - Formal verification implications for Rocq translation
  - Empirical comparison: Inference vs rustc release vs rustc debug vs Soroban

### Testing

- Add execution test for `numeric_literals` verifying MIN/MAX boundary values for all 8 integer types (i8, i16, i32, i64, u8, u16, u32, u64) via Wasmtime
- Add `arith_overflow` test module with 8 functions covering two's-complement wrapping arithmetic: i32/i64/u32 overflow and underflow, multiplication overflow, and negation of MIN (8 Wasmtime execution assertions)
- Add `expr_deep_nesting` test module with 5 functions verifying 8+ level expression nesting: left-associative addition chain, mixed arithmetic in nested groups, boolean connectives over nested comparisons, function calls embedded in expressions, and chained unary negation (6 Wasmtime execution assertions)
- Add 4 algorithm integration test modules exercising assignments, conditionals, and expressions in realistic patterns:
  - `algo_bitwise`: bit manipulation (popcount, reverse bits, parity, hamming distance, power-of-2 check)
  - `algo_converge`: iterative convergence (integer sqrt, binary search, GCD, collatz steps)
  - `algo_i64_mixed`: i64 arithmetic (sum range, factorial, fibonacci, digit sum, geometric progression)
  - `algo_recursive_math`: recursive functions (factorial, fibonacci, GCD, power, sum-to-n)
- Add 2 assignment test fixtures with 10 Wasmtime execution assertions ([#146])
  - `assign.inf`: 10 functions covering simple i32/i64 assignment, expression RHS, parameter assignment, multiple reassignment, function call RHS, bool assignment, assignment inside conditional, mutable parameter assignment
  - `assign_nondet.inf`: assignment inside `forall` non-det block with uzumaki RHS
  - AST parse tests for `is_mut` flag on `VariableDefinitionStatement`
  - Type-checker tests for mutability enforcement (immutable, mutable, parameter mutability)
- Add WAT golden file testing with `wasmprinter` for human-readable codegen verification ([#144])
  - `assert_wat_equivalence()` compares generated WAT against committed `.wat` reference files
  - `regenerate_wat()` writes WAT alongside WASM during test data regeneration
  - Non-det modules gracefully skipped (custom opcodes unsupported by `wasmprinter`)
- Add 3 conditional test fixtures with 62 Wasmtime execution assertions ([#144])
  - `if_else.inf`: 6 functions covering if-only, if/else, locals in arms, nested if, void if
  - `if_bool_exprs.inf`: 16 functions across 7 groups (bool params, logical ops, De Morgan, range checks, bool locals)
  - `if_nondet.inf`: if-statement inside `forall` non-det block
- Flatten per-module test directory structure to avoid double-nesting ([#144])
  - `get_test_dir()` helper deduplicates module-name paths
- Migrate codegen test data to per-test subdirectory layout ([#134])
  - `tests/test_data/codegen/wasm/base/{name}/{name}.{inf,wasm}` replaces flat `base/{name}.{inf,wasm}`
  - `get_test_file_path` / `get_test_wasm_path` helpers updated to resolve through subdirectory
- Add 28 codegen tests with three-tier verification architecture ([#97], [#125])
  - Byte comparison tests against committed `.wasm` reference files
  - `inf_wasmparser::validate()` validation on all generated output
  - 2 Wasmtime execution tests verifying runtime behavior
  - Validation tests for metadata, target/mode combinations, non-det opcode presence
- Add codegen test helpers ([#97], [#125])
  - `codegen_output()`, `codegen_output_with_mode()`, `codegen_with_target_mode()`, `codegen_with_full_config()`
  - `wasm_codegen()`, `wasm_codegen_with_target()`, `assert_wasms_modules_equivalence()`
- Expand `infs` test coverage from 282 to 429 tests (360 unit + 69 integration) ([#96])
  - Add TUI rendering tests using TestBackend for main_view, doctor_view, toolchain_view
  - Add integration tests for non-deterministic features (forall, exists, assume, unique, oracle)
  - Add tests for error handling, environment variables, and edge cases
  - Consolidate test fixtures in `apps/infs/tests/fixtures/`
- Move QA test suite to `apps/infs/docs/qa-test-suite.md` with 9 truly manual tests ([#96])

### infs CLI

- Add automatic PATH configuration on first install ([#96])
  - Unix: Modifies shell profile (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish`)
  - Windows: Modifies user PATH in registry (`HKCU\Environment\Path`)
  - Users only need to restart their terminal after installation
- Rename environment variable and directory for consistency ([#96])
  - `INFS_HOME` → `INFERENCE_HOME`
  - `~/.infs` → `~/.inference`
- Add `infc` symlink to installed toolchain ([#96])
- Improve `infs install` to auto-set default toolchain when none is configured ([#96])
  - When installing an already-installed version without a default toolchain, `infs install` now automatically sets that version as default and updates symlinks
  - Provides graceful recovery if default toolchain file was manually removed
- Improve `infs doctor` recommendations for missing default toolchain ([#96])
  - When no default is set but toolchains exist, suggests `infs default <version>` instead of `infs install`
  - When no toolchains exist, suggests `infs install`
- Fix `infs install` and `infs self update` to fall back to latest pre-release version when no stable versions exist ([#96])
  - Previously failed with "No stable version found in manifest" error
  - Now uses latest stable version if available, otherwise falls back to latest version regardless of stability
- Fix `infs install` failing with nested archive structure from GitHub releases ([#96])
  - GitHub releases wrap tar.gz archives in ZIP files
  - Now automatically detects and extracts nested tar.gz after ZIP extraction
- Fix `infs uninstall` leaving broken symlinks when removing non-default toolchains ([#96])
  - Previously, `Path::exists()` returned false for broken symlinks, causing them to remain in `~/.inference/bin/`
  - Now uses `symlink_metadata().is_ok()` to correctly detect and remove both valid and broken symlinks
  - Added `validate_symlinks()` to check for broken symlinks after uninstallation
  - Added `repair_symlinks()` to automatically fix broken symlinks by updating them to the default version or removing them
- Change `infs doctor` to exit with non-zero status when checks fail ([#116])
  - Previously always exited 0; now returns non-zero so callers can detect failures
- Remove manifest caching from `infs` CLI ([#116])
  - `fetch_manifest()` now always fetches from network
  - Simplifies CLI code; VS Code extension manages its own fetching lifecycle
- Remove LLVM toolchain management from `infs` CLI ([#126])
  - Flatten toolchain layout: `infc` binary now at toolchain root (no more `bin/` subdirectory)
  - Remove `inf-llc`, `rust-lld`, and `libLLVM` binary management
  - Simplify doctor checks: single `infc` check replaces `inf-llc`, `rust-lld`, and `libLLVM` checks
  - Remove platform-specific `#[cfg(target_os = "linux")]` branching in `run_all_checks()`
  - Slim `InfsError` to single `ProcessExitCode` variant; all other errors use `anyhow::Result`
  - Replace `rand` dependency with lighter-weight `fastrand`
  - Remove dead code: unused error variants, `create_project_default()`, `available_versions()`, `selected_bg` theme field

### Build

- Add `infs` binaries to release artifacts for all platforms (Linux x64, Windows x64, macOS ARM64)
- Update release manifest to schema version 2 with separate `infc` and `infs` tool entries

### Project Manifest

- Replace `manifest_version` field with `infc_version` in Inference.toml ([#96])
  - `infc_version` is a String (semver format) that records the compiler version used to create the project
  - Automatically detected from `infc --version` when running `infs new` or `infs init`
  - Falls back to `infs` version if `infc` is not available
  - All Inference ecosystem crates share the same version number

### Editor Support

- Add VS Code extension with syntax highlighting for Inference language ([#94])
- Add TextMate grammar with hierarchical scopes for non-deterministic keywords (`forall`, `exists`, `assume`, `unique`, `@`)
- Add language configuration with bracket matching, comment toggling, and code folding
- Publish extension to VS Code Marketplace: [inference-lang.inference](https://marketplace.visualstudio.com/items?itemName=inference-lang.inference)
- Add Configuration sidebar (TreeView) to VS Code extension with toolchain info and settings overview ([#116])
  - Activity bar icon opens a Configuration view with Toolchain and Settings groups
  - Displays resolved infs path, version, INFERENCE_HOME, platform, and health status
  - Click settings items to open VS Code settings; click status to run doctor
  - Right-click path items for "Copy Value" and "Reveal in File Explorer"
  - Auto-refreshes on settings change, after install, and after doctor
- Add automatic terminal PATH integration to VS Code extension ([#116])
  - `infs` and `infc` are available in integrated terminals immediately after install or update
  - Existing open terminals show a relaunch indicator when PATH changes
  - PATH modification persists across VS Code sessions
- Add toolchain management commands to VS Code extension ([#116])
  - Install Toolchain: downloads, verifies (SHA-256), extracts, and runs `infs install`
  - Update Toolchain: checks for newer versions and applies updates
  - Select Version: switch between installed toolchain versions via QuickPick
  - Run Doctor: executes `infs doctor` and displays results in output channel
- Add Getting Started walkthrough to VS Code extension ([#116])
  - Four-step guided setup: install toolchain, verify with doctor, create project, build
- Add status bar integration showing toolchain health at a glance ([#116])
- Update VS Code extension tests and QA docs after LLVM removal ([#127])
  - Remove `inf-llc`, `rust-lld`, `libLLVM` references from e2e tests and doctor tests
  - Update fake `infs` shell script to use flat toolchain layout (`TOOLCHAIN_DIR/infc`, no `bin/` subdirectory)
  - Simplify `buildFakeInfcArchive()` to emit only `infc` binary
  - Update doctor check expectations from 6 to 5 checks (single `infc` check replaces `inf-llc`, `rust-lld`, `libLLVM`)
  - Change "missing lib directory triggers doctor warning" to "missing infc triggers doctor failure"

### Language

- Add struct definition and parsing support ([#14])
- Add division operator (`/`) support ([#86])
- Add unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- Parse visibility modifiers (`pub`) for functions, structs, enums, constants, and type aliases ([#86])

### Compiler

- ast: Remove dead `OperatorKind::BitNot` variant — `~x` is always parsed as `UnaryOperatorKind::BitNot` in a `PrefixUnaryExpression`; the binary enum variant was never produced by the AST builder ([#142])
- ast: Introduce `SimpleTypeKind` enum for primitive types, replacing string-based type matching ([#50])
- ast: Simplify Builder API to return `Arena` directly instead of using state machine pattern ([#50])
- ast: Add error collection in Builder with `collect_errors()` for better parse error reporting ([#50])
- ast: Add `@skip` macro annotation for enum variants without stable node IDs ([#50])
- type-checker: Add `type_kind_from_simple_type_kind()` for type-safe primitive type conversion ([#50])
- type-checker: Add type checking for unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- type-checker: Change expression inference to use immutable references ([#86])
- ast: Use atomic counter for deterministic node ID generation ([#86])
- type-checker: Add bidirectional type inference with scope-aware symbol table ([#54])
- type-checker: Implement import system with registration and resolution phases ([#54])
- type-checker: Add visibility handling for modules, structs, and enums ([#54])
- type-checker: Implement enum support with variant access validation ([#54])
- ast: Add `#[derive(Copy)]` to `Location` for efficient stack copies ([#69])
- ast: Replace `Vec<NodeRoute>` with `FxHashMap` for O(1) parent/children lookup ([#69])
- ast: Add `get_node_source()` and `find_source_file_for_node()` convenience API ([#69])
- ast: Implement arena-based AST with ID-based node references ([#25])
- ast: Add `NodeKind` support for AST node classification ([#25])

### Codegen

- Add LLVM-based WASM code generation using `inf-llc` ([#44])
- Add custom LLVM intrinsics for non-deterministic instructions ([#44])
- Implement `forall`, `exists`, `uzumaki`, `assume`, `unique` block codegen ([#44])
- Add `rust-lld` linker invocation for WASM linking ([#44])
- Add mutable globals support in WASM compilation ([#44])
- Add base WASM code generation from typed AST ([#29])

### Rocq Translation

- Rewrite WASM-to-V translator for WasmCertCoq theory syntax ([#23])
- Add function name propagation to V output ([#24])

### CLI

- Refactor CLI architecture with improved argument handling ([#28])

### Tooling

- Remove `playground-server` tool (unused, superseded by external playground infrastructure) ([#56])
- Reorganize project structure: move crates to `core/` and `tools/` directories ([#43])
- Add `inf-wasmparser` crate (fork with non-det instruction support) ([#43])
- Add `inf-wat` crate for WAT parsing ([#43])
- Add `wat-fmt` crate for pretty-formatting WAT files ([#21])
- Improve error handling with `anyhow::Result` for AST parsing ([#22])

### Build

- Add macOS Apple Silicon (M1/M2) support to build workflows ([#55])
- Add Codecov integration for test coverage reporting ([#57], [#58])
- Optimize local build time and refactor CI workflows ([#60])
- Add Windows development setup with cross-platform LLVM binaries
- Update libLLVM download URL to use consistent filename with `-nightly` suffix ([#56])
- Remove unused PATH configuration from `.cargo/config.toml` ([#56])
- Bump CI cache keys to invalidate stale binary caches ([#56])
- Fix LLVM environment variable reference in Windows installation guide ([#56])
- Add Linux development setup guide (`book/installation_linux.md`) ([#56])
- Add macOS development setup guide (`book/installation_macos.md`) ([#56])
- Add cross-platform dependency check script (`book/check_deps.sh`) ([#56])

### Testing

- tests: Consolidate builder tests by removing redundant `builder_extended.rs` module ([#50])
- tests: Add `builder_features.rs` module with feature-specific AST tests ([#50])
- tests: Add `primitive_type.rs` module with `SimpleTypeKind` tests ([#50])
- tests: Add utility assertions: `assert_single_binary_op`, `assert_function_signature`, etc. ([#50])

### Performance

- ast: 98% memory reduction in `Location` struct by removing unused source field ([#69])

### Fixed

- Fix FxHashMap non-deterministic iteration in `Arena` — `filter_nodes()` and `list_nodes_cmp()` now sort by node ID, ensuring reproducible WASM function emission order
- Fix Drop instruction emission for nested non-det blocks — `parent_blocks_stack.last()` (innermost block) is now used instead of `.first()` (outermost block)
- Fix `lower_literal` to emit type-correct WASM const instructions — number literals now consult `TypedContext` and emit `i32.const` or `i64.const` based on inferred type instead of always emitting `i32.const`
- Fix `wasm_to_v` public API signature — parameter changed from `&Vec<u8>` to idiomatic `&[u8]`

---

## [0.0.1-alpha] - 2026-01-03

Initial tagged release.

### Language

- Support for non-deterministic blocks: `uzumaki`, `forall`, `exists`, `assume`, `unique`
- Function definitions with generic type parameters
- Module system with visibility modifiers
- Add `undef` syntax support ([#10])

### Compiler

- Tree-sitter-based parsing with error recovery
- Arena-based AST node storage
- Basic type inference

### Rocq Translation

- Add complete WASM module translation to Rocq (Coq) ([#11])
- Implement instruction translation: memory ops, control flow, numeric ops ([#11])
- Add element segment and data segment translation ([#11])
- Add function, table, global, and memory translation ([#11])

### CLI

- Add `infc` CLI binary with parsing diagnostics ([#12])
- Add V file output formatting ([#12])

### Build

- Add CI build workflow with cross-platform support ([#1])

---

[Unreleased]: https://github.com/Inferara/inference/releases/tag/v0.0.1-alpha...HEAD
[0.0.1-alpha]: https://github.com/Inferara/inference/releases/tag/v0.0.1-alpha

[#1]: https://github.com/Inferara/inference/pull/1
[#10]: https://github.com/Inferara/inference/pull/10
[#11]: https://github.com/Inferara/inference/pull/11
[#12]: https://github.com/Inferara/inference/pull/12
[#14]: https://github.com/Inferara/inference/pull/14
[#21]: https://github.com/Inferara/inference/pull/21
[#22]: https://github.com/Inferara/inference/pull/22
[#23]: https://github.com/Inferara/inference/pull/23
[#24]: https://github.com/Inferara/inference/pull/24
[#25]: https://github.com/Inferara/inference/pull/25
[#28]: https://github.com/Inferara/inference/pull/28
[#29]: https://github.com/Inferara/inference/pull/29
[#43]: https://github.com/Inferara/inference/pull/43
[#44]: https://github.com/Inferara/inference/pull/44
[#50]: https://github.com/Inferara/inference/pull/50
[#54]: https://github.com/Inferara/inference/pull/54
[#55]: https://github.com/Inferara/inference/pull/55
[#56]: https://github.com/Inferara/inference/pull/56
[#57]: https://github.com/Inferara/inference/pull/57
[#58]: https://github.com/Inferara/inference/pull/58
[#60]: https://github.com/Inferara/inference/pull/60
[#69]: https://github.com/Inferara/inference/pull/69
[#86]: https://github.com/Inferara/inference/pull/86
[#94]: https://github.com/Inferara/inference/pull/94
[#96]: https://github.com/Inferara/inference/pull/96
[#97]: https://github.com/Inferara/inference/issues/97
[#116]: https://github.com/Inferara/inference/pull/116
[#125]: https://github.com/Inferara/inference/pull/125
[#126]: https://github.com/Inferara/inference/pull/126
[#127]: https://github.com/Inferara/inference/pull/127
[#134]: https://github.com/Inferara/inference/pull/135
[#136]: https://github.com/Inferara/inference/pull/136
[#138]: https://github.com/Inferara/inference/pull/138
[#140]: https://github.com/Inferara/inference/pull/140
[#142]: https://github.com/Inferara/inference/pull/142
[#144]: https://github.com/Inferara/inference/pull/144
[#146]: https://github.com/Inferara/inference/pull/146

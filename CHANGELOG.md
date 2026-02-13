# Changelog

All notable changes to the Inference compiler project.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Codegen

- Separate LLVM IR generation from external tool invocation ([#97])
  - `codegen()` now returns `CodegenOutput` (LLVM IR + metadata) instead of WASM bytes
  - New `CodegenOutput` struct carries IR text, target, mode, opt level, module name, and `has_main` flag
  - New `Target` (Wasm32/Soroban), `CompilationMode` (Compile/Proof), and `OptLevel` (O0–O3/Os/Oz) enums in `wasm-codegen`
  - Delete `utils.rs` from `wasm-codegen` (tool invocation moved to CLI toolchain layer)
- Add size optimization IR attributes for Os/Oz levels ([#97])
  - `optsize` attribute for Os and Oz, `minsize` for Oz only
  - Matches Clang's implementation: `llc` receives `-O2` while IR attributes control size optimization
- Add per-function optimization barriers for proof mode (Decision #32) ([#97])
  - Spec functions (`is_non_det()`) receive `optnone` + `noinline` attributes
  - Execution functions use target's release optimization (same as compile mode)
- Add validation in `codegen()`: reject proof mode with non-Wasm32 targets, reject Soroban with non-det operations ([#97])

### CLI

- Add toolchain module for external tool invocation ([#97])
  - `llc.rs`: `inf-llc` invocation with target-aware flags (`-mcpu`, `-mattr`, optimization)
  - `lld.rs`: `rust-lld` invocation with target-aware linker flags (Wasm32 vs Soroban)
  - `paths.rs`: Binary discovery for `inf-llc` and `rust-lld`
  - `env.rs`: Platform-specific environment configuration (`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`)
  - `profile.rs`: `BuildProfile` (Debug/Release) with `resolve_opt_level()`
- Defer WASM compilation until output files are actually needed (`-o` or `-v` flags) ([#97])

### Documentation

- Add compilation targets matrix documentation (`book/compilation_targets.md`) ([#97])
  - 6-option matrix: Compile/Proof x Debug/Release x with/without non-det operations
- Document FxHashMap non-deterministic iteration bug in `arena.rs` ([#97])

### Testing

- Add 20 new codegen and toolchain tests ([#97])
  - 11 codegen validation tests (size attributes, metadata, target/mode combinations)
  - 2 `inference::codegen()` wrapper tests
  - 4 `inf-llc` argument-building unit tests
  - 3 `rust-lld` argument-building unit tests
- Add Tier 1 test helpers for codegen without external binaries ([#97])
  - `codegen_ir()`, `codegen_ir_with_mode()`, `codegen_with_target_mode()`, `codegen_with_full_config()`
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
- Add `infc` symlink to installed toolchain alongside `inf-llc` and `rust-lld` ([#96])
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

### Language

- Add struct definition and parsing support ([#14])
- Add division operator (`/`) support ([#86])
- Add unary negation (`-`) and bitwise NOT (`~`) operators ([#86])
- Parse visibility modifiers (`pub`) for functions, structs, enums, constants, and type aliases ([#86])

### Compiler

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

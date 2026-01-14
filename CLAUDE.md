# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Inference is a programming language compiler targeting WebAssembly with extensions for non-deterministic instructions (uzumaki, forall, exists, assume, unique). The compiler can translate WASM to Rocq (Coq) for formal verification. See the [language spec](https://github.com/Inferara/inference-language-spec).

## Additional Context

Before starting work, check these sources for relevant context:

- **`CHANGELOG.md`** - Recent changes, version history, and notable updates
- **`.claude/knowledge_base/`** - Accumulated knowledge and patterns learned from working on this codebase
- **`.claude/docs/`** - Additional documentation, design decisions, and implementation notes
- **`.claude/agents/_SUBAGENTS.md`** - Complete reference for available specialized agents, their purposes, and orchestration patterns

## Working Principles

### Quality Over Speed
Always prioritize better results and quality over time. Take the time needed to produce correct, well-designed solutions. Rushing leads to technical debt and rework.

### Ask Clarifying Questions
Ask clarifying questions early and often. Do not make assumptions about requirements, design decisions, or implementation details. When in doubt, ask. It is better to clarify upfront than to redo work later.

### Use Specialized Agents
Involve relevant agents frequently to ensure thorough work. Key agents include:
- **chief-architect** - For architectural decisions, crate organization, module boundaries
- **compliance-reviewer** - To verify adherence to CONTRIBUTING.md guidelines
- **test-master** - For writing tests and measuring coverage
- **work-done-examiner** - To verify all requirements are met before declaring work complete
- **Explore** - For understanding the codebase structure and finding relevant code

See **`.claude/agents/_SUBAGENTS.md`** for the complete list of 12 available agents, detailed usage guidelines, and orchestration patterns for complex workflows.

Do not hesitate to invoke these agents. Better to over-verify than to miss something important.

## Build Commands

```bash
cargo build                    # Build core/ crates only (default)
cargo build-full               # Build entire workspace including tools/
cargo build --release          # Release build

cargo test                     # Test core/ crates and tests/ integration suite
cargo test-full                # Test all workspace members

# Run a specific test
cargo test -p inference-tests test_name

# Run compiler directly
cargo run -p inference-cli -- infc path/to/file.inf --parse --codegen -o
./target/debug/infc file.inf --parse
```

**Required external binaries**: Before building, download platform-specific LLVM tools (`inf-llc`, `rust-lld`) from links in README.md and place in `external/bin/{linux,macos,windows}/`.

## Architecture

Multi-phase compilation pipeline:
```
.inf source → tree-sitter → Typed AST → Type Check → LLVM IR → WASM → Rocq (.v)
```

### Cross-platform compatibility
Compiled project should run on:
- Linux x64
- Windows x64
- macOS Apple Silicon (M1/M2)

### Core Crates (`core/`)
- **`core/inference/`** - Main orchestration: `parse()`, `type_check()`, `analyze()`, `codegen()`, `wasm_to_v()`
- **`core/ast/`** - Arena-based AST with tree-sitter parsing
- **`core/type-checker/`** - Type inference and bidirectional type checking (WIP)
- **`core/wasm-codegen/`** - LLVM-based codegen with custom intrinsics for non-det blocks
- **`core/wasm-to-v/`** - WASM to Rocq translator
- **`core/cli/`** - `infc` binary entry point

### Tools (`tools/`)
- **`tools/inf-wasmparser/`** - Fork of wasmparser with non-det instruction support
- **`tools/inf-wast/`**, **`tools/wasm-fmt/`**, **`tools/wat-fmt/`** - WASM utilities

### Non-deterministic Instructions
Binary encoding: `0xfc 0x3a` (forall), `0xfc 0x3b` (exists), etc. LLVM intrinsics:
```rust
const FORALL_START_INTRINSIC: &str = "llvm.wasm.forall.start";
const UZUMAKI_I32_INTRINSIC: &str = "llvm.wasm.uzumaki.i32";
```

## Testing Conventions

### Unit Tests
Inline source in compact format:
```rust
#[test]
fn test_parse() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let ast = build_ast(source.to_string());
    // assertions...
}
```

### Codegen Tests
Test path convention: `tests/src/codegen/wasm/base.rs::trivial_test` → `tests/test_data/codegen/wasm/base/trivial.inf`

Use `get_test_file_path(module_path!(), "test_name")` for path resolution.

### Test Rules
- No `#[should_panic]` - explicitly check for None/Err instead
- No `#[ignore]` - assert wrong behavior with fixme comment if test doesn't work yet
- Use `llvm-cov` to measure coverage

## Coding Conventions

### Rust Patterns
- **Arenas**: AST/HIR nodes use arena allocation with ID-based references, not raw pointers
- **`#[must_use]`**: Required on constructors and methods returning owned data; use `#[must_use=reason]` when applicable
- **Error handling**: `anyhow::Result` for library code, explicit `process::exit(1)` in CLI
- **Clippy pedantic**: Enabled workspace-wide
- **Collections**: Prefer `FxHashMap`/`FxHashSet` from `rustc-hash` over std collections

### Type Preferences
```rust
// Prefer left    Avoid right
&[T]              &Vec<T>
&str              &String
Option<&T>        &Option<T>
&Path             &PathBuf
```

### Function Design
- Prefer constructed parameters over `Option`s - caller handles validation
- Prefer `Default` over zero-argument `new` functions
- Use `if let` and `let match` for condition checks

### Commits and PRs
- Branch naming: `<issue-number>-<type>-<short-description>` (e.g., `9-feature-develop-linker`)
- No emojis in commit messages
- AI-generated code must be reviewed, tested, and disclosed in PR description
- Don't mix refactoring with features/fixes in single PR

## Current Limitations
- Multi-file support not yet implemented - AST expects single source file
- Analyze phase is WIP - type inference in `core/type-checker/` under active development
- Output goes to `out/` relative to CWD, not source file location

## Don'ts

- never use `main` branch
- always use `gh` cli for `git` operations
- never use slashes as path separators
- use line comments inside functions or types definitions only when it is required to understand complex things or some guidance, prefer using type, function or module level comments

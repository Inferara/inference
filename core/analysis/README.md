# inference-analysis

Static analysis pass for the Inference compiler. Runs after type checking, before code generation, and validates semantic invariants that the type system cannot express.

## Pipeline Position

```
parse -> type_check -> analyze -> codegen -> wasm_to_v
```

The `analyze()` function is the entry point. It accepts a `&TypedContext` produced by the type checker and returns either an `AnalysisResult` (success, possibly with warnings) or `AnalysisErrors` (one or more hard errors, all collected before returning).

## Architecture

```
analyze()
    |
    +-- rules::all_rules()  (static slice of &dyn Rule)
         |
         +-- Rule::check(&TypedContext) -> Vec<AnalysisDiagnostic>
                  |
                  +-- walker::walk_function_bodies()  (shared traversal)
                  |        visits every Stmt in every function body
                  |        tracks loop_depth, nondet_depth, nondet_block_kind
                  |
                  +-- or custom traversal  (A004 only)
```

Each rule is a zero-sized struct implementing the `Rule` trait. Rules are stateless and `Send + Sync`, which keeps the door open for parallel execution in the future. The `rule!` macro generates the struct and trait implementation from a compact attribute syntax, eliminating boilerplate.

Errors, warnings, and informational findings are partitioned by severity. The `analyze()` function returns `Err(AnalysisErrors)` only when at least one `Error`-severity finding exists; `Warning` and `Info` findings are always returned via the success path or bundled inside `AnalysisErrors`.

## Module Organization

| Module | Description |
|--------|-------------|
| `lib.rs` | `analyze()` entry point; partitions findings by severity |
| `rule` | `Rule` trait and `rule!` / `__severity!` macros |
| `errors` | `AnalysisDiagnostic`, `AnalysisErrors`, `AnalysisResult`, `Severity` |
| `walker` | `walk_function_bodies()`, `for_each_function_body()`, `WalkContext` |
| `rules` | `all_rules()` registry and one sub-module per rule |

## Rules

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A001 | `BreakOutsideLoop` | error | `break` must be inside a loop body |
| A002 | `BreakInsideNonDetBlock` | error | `break` must not be inside a `forall`/`exists`/`assume`/`unique` block |
| A003 | `ReturnInsideLoop` | error | `return` must not appear inside a loop body |
| A004 | `InfiniteLoopWithoutBreak` | error | `loop { }` without a condition must contain a reachable `break` |
| A005 | `ReturnInsideNonDetBlock` | error | `return` must not appear inside a non-deterministic block |

The rationale behind A003 and A005 is that a single exit point per function simplifies formal verification. The rationale behind A002 is that `break` inside a non-deterministic block would prematurely terminate path exploration.

## Diagnostic Output Format

```
<line>:<column>: <severity>[<rule_id>]: <message>
```

All diagnostics are sorted by source location (line, then column) before display. Messages follow a `what; why; how` structure separated by semicolons.

Example output for two violations:

```
1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'
3:10: error[A002]: break statement is not allowed inside a 'forall' block; break would interfere with the path exploration required for formal verification; move the break outside the 'forall' block
```

## Usage

```rust
use inference_analysis::{analyze, errors::{AnalysisErrors, AnalysisResult}};
use inference_type_checker::typed_context::TypedContext;

fn run(ctx: &TypedContext) {
    match analyze(ctx) {
        Ok(result) => {
            // Compilation can continue. result.warnings() and result.infos()
            // may still contain non-fatal findings.
            if result.has_findings() {
                eprintln!("{result}");
            }
        }
        Err(errors) => {
            // At least one hard error. errors.errors() is guaranteed non-empty.
            // errors.warnings() and errors.infos() may also be populated.
            eprintln!("{errors}");
            std::process::exit(1);
        }
    }
}
```

The orchestration layer in `core/inference/src/lib.rs` wraps this call and re-exports `analyze()` as part of the public compiler API.

## How to Add a New Rule

1. Create `src/rules/my_rule.rs` using the `rule!` macro (copy an existing simple rule such as `break_outside_loop.rs` as a starting point).
2. Add `pub mod my_rule;` to `src/rules/mod.rs`.
3. Add `&MyRule` to the slice in `all_rules()`.
4. Add a matching `AnalysisDiagnostic` variant to `errors.rs` with `rule_id()` returning the new ID.
5. Update the integration test in `lib.rs` (`rule_ids_match_diagnostic_rule_ids`) to include the new variant.

Rules that need scoping logic beyond `loop_depth` and `nondet_depth` can implement a custom traversal. See `InfiniteLoopWithoutBreak` (`src/rules/infinite_loop_without_break.rs`) for an example: it uses `for_each_function_body()` directly and provides its own recursive descent so that `break` inside a nested loop is not counted as a break for the outer loop.

## The Shared Walker

`walk_function_bodies()` drives traversal for most rules. It visits every statement in every function body (including struct methods, spec functions, and module-level functions) in pre-order and calls a `dyn FnMut` visitor with two arguments:

- `StmtId` — the current statement
- `&WalkContext` — read-only snapshot of traversal state:
  - `loop_depth: u32` — incremented when entering a `Loop` body, decremented on exit
  - `nondet_depth: u32` — incremented when entering a non-det block, decremented on exit
  - `nondet_block_kind: Option<&'static str>` — label of the innermost non-det block (`"forall"`, `"exists"`, `"assume"`, or `"unique"`)

Using `dyn FnMut` instead of a generic parameter avoids monomorphization cost when the number of rules grows.

## Testing

Unit tests live alongside each source file in `src/`. The integration test `rule_ids_match_diagnostic_rule_ids` in `lib.rs` asserts that:
- The number of registered rules equals the number of `AnalysisDiagnostic` variants.
- Each rule's `id()` matches the `rule_id()` of its corresponding diagnostic variant.

End-to-end tests that compile `.inf` source and assert on diagnostic output live in `tests/src/analysis/` (part of the `inference-tests` crate). Run them with:

```
cargo test -p inference-tests analysis
```

## Dependencies

| Crate | Role |
|-------|------|
| `inference-ast` | AST arena types, node kinds, `Location` |
| `inference-type-checker` | `TypedContext` input to every rule |
| `thiserror` | Derive `Error` for `AnalysisDiagnostic` |

## Current Limitations

1. All five rules are `Error` severity. No rules currently produce `Warning` or `Info` findings; those severity levels are wired and ready but unused.
2. `ArrayLiteralAsArgument` and `ArrayReturnCallInExpressionPosition` are restrictions currently enforced in the type checker (`core/type-checker`) rather than here. They would fit naturally as analysis rules A006 and A007.
3. The walker visits all statements but does not expose expression-level traversal. Rules that need to inspect expressions must do their own descent.

# inference-analysis

Static analysis pass for the Inference compiler. Runs after type checking, before code generation, and validates semantic invariants beyond what the type system expresses.

The type checker is responsible only for type correctness — blocking errors that would prevent further analysis. Everything else (control flow validation, lint warnings, codegen restrictions) lives here.

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

### Control Flow (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A001 | `BreakOutsideLoop` | error | `break` must be inside a loop body |
| A002 | `BreakInsideNonDetBlock` | error | `break` must not be inside a `forall`/`exists`/`assume`/`unique` block |
| A003 | `ReturnInsideLoop` | error | `return` must not appear inside a loop body |
| A004 | `InfiniteLoopWithoutBreak` | error | `loop { }` without a condition must contain a reachable `break` |
| A005 | `ReturnInsideNonDetBlock` | error | `return` must not appear inside a non-deterministic block |
| A006 | `UzumakiOutsideNonDetBlock` | error | uzumaki (`@`) must not appear outside a `forall`/`exists`/`assume`/`unique` block |
| A007 | `MissingReturn` | error | non-void function must have a `return` statement on every branch (branch-aware) |
| A008 | `StandaloneUzumaki` | error | uzumaki expression that is not assigned to a variable serves no purpose |

A003 and A005 require a single exit point per function to simplify formal verification. A002 prevents `break` from prematurely terminating path exploration in non-det blocks.

### Lint Warnings

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A009 | `EmptyEnumDefinition` | warning | enum with no variants is likely an oversight |
| A010 | `MethodNeverAccessesSelf` | warning | method declares `self` but never reads or writes a field through it |
| A011 | `EmptyStructDefinition` | warning | struct with no fields and no methods |

### Variable Initialization (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A025 | `UninitializedVariable` | error | variable declared without an initializer |

### Codegen Restrictions (errors)

These rules cover constructs that are valid in the type system but cannot yet be lowered by the code generator. They live here rather than in the type checker because they are implementation limits, not type errors.

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A012 | `ArrayLiteralAsArgument` | error | array literal passed directly as a function argument |
| A013 | `StructLiteralAsArgument` | error | struct literal passed directly as a function argument |
| A014 | `ArrayUzumakiAsArgument` | error | array uzumaki (`@`) passed directly as a function argument |
| A015 | `CompoundLiteralPosition` | error | compound literal (array or struct) in an unsupported expression position |
| A016 | `CompoundReturnCallPosition` | error | compound-returning function call in a general expression position |
| A017 | `CompoundReturnCallAssignment` | error | compound-returning function call on the RHS of an assignment statement |
| A018 | `MethodCallChainCompound` | error | method call chained on the result of a compound-returning function |
| A019 | `ArrayIndex64Bit` | error | 64-bit integer used as an array index |
| A022 | `LiteralOutOfRange` | error | numeric literal is outside the valid range for its declared type |
| A023 | `UzumakiInReassignment` | error | uzumaki (`@`) used in a variable reassignment (only `let` initializers are supported) |
| A024 | `ExternFunctionCall` | error | call to an external (`extern`) function (not yet implemented in codegen) |
| A026 | `NestedCompoundDepth` | error | struct field is itself a nested compound type beyond one level of nesting |
| A027 | `UzumakiOnNestedStruct` | error | uzumaki (`@`) assigned to a struct whose fields include another struct or an array of structs |
| A028 | `UzumakiOnStructInArray` | error | uzumaki (`@`) assigned to an array whose element type is a struct |
| A029 | `CompoundLiteralMemberAssign` | error | compound literal (struct or array) used directly as the RHS of a member-access or array-index assignment |
| A030 | *(removed)* | — | *(uzumaki on scalar arrays now supported at any depth)* |
| A031 | `UnsupportedCompoundReturnExpr` | error | return expression in a compound-returning function is not a supported form (identifier, literal, call, or field/element access) |

### Recursion (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A035 | `RecursionDetected` | error | direct or indirect (mutual) recursion is forbidden so stack usage stays statically bounded (Power of 10, Rule 1) |

A035 builds a whole-program call graph keyed by `FnKey` (from `inference-fn-key`, the shared canonical function identity used by both this crate and `wasm-codegen`) and reports each call cycle once, pointing at the call site that closes the cycle.

### Stack Depth (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A036 | `StackDepthExceeded` | error | cumulative shadow-stack usage along a call chain must not exceed the 64 KB stack budget |

A036 reuses A035's whole-program call graph (a DAG, since recursion is forbidden) and computes the maximum-weight root-to-leaf path, where each node's weight is a conservative upper bound on that function's compound (array/struct) frame size. Scalar locals live in WASM locals and contribute nothing. The estimate over-approximates codegen's real frame layout by construction, so the rule never accepts a program codegen would overflow. The shared graph construction lives in `src/call_graph.rs`.

### Array Bounds (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A037 | `ArrayIndexConstOutOfBounds` | error | a constant array index (`arr[c]`) is negative or `>= length` |

A037 is the static half of array bounds checking. When the index is a constant integer literal, the array length is known at compile time from the array sub-expression's `Array(_, length)` type info, so an out-of-range access is rejected with zero runtime cost in every build profile and compilation mode. A negative literal such as `arr[-1]` lowers to a single `NumberLiteral` whose text keeps the leading `-`, so it is caught here as well. Dynamic (non-literal) indices are out of A037's scope — they are guarded at run time in all Compile-mode builds by `core/wasm-codegen` (see that crate's docs); the two mechanisms together close the bounds-safety hole.

### Uzumaki in unsupported positions (errors)

| ID | Struct | Severity | What it checks |
|----|--------|----------|----------------|
| A038 | `UzumakiOnCompoundField` | error | uzumaki (`@`) used as the value of a struct- or array-typed field in a struct literal (e.g. `Outer { inner: @ }`); only scalar and enum fields may use `@` |
| A039 | `StructUzumakiAsArgument` | error | struct-typed uzumaki (`@`) passed directly as a function argument; assign to a local variable first (struct sibling of A014, which covers array uzumaki) |
| A040 | `UzumakiOnCompoundArrayElement` | error | uzumaki (`@`) used as a struct- or array-typed element of an array literal (e.g. `[p, @]` where `p` is a struct); only scalar and enum elements may use `@` |

A compound (struct or array) `@` is lowered by writing into a *named* frame slot, which only a `let`/`const` binding supplies, so `@` of such a type is rejected wherever no slot exists. These complement A014 (array `@` as a function argument) and A027/A028 (whole-binding compound `@`). A scalar or enum `@` is unaffected: the type checker threads it its declared field/element type, so it lowers to a single uzumaki opcode. Array-literal arguments are handled separately by A012, so these rules do not extend to that position.

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

The walker module also exposes several type-inspection helpers used by multiple rules:

- `array_nesting_depth(kind)` — returns how many array layers deep a type is (`[[i32; 3]; 2]` → 2, `i32` → 0)
- `has_compound_fields(ctx, kind)` — returns true if a struct or array type contains fields that are themselves structs, arrays of structs, or multidimensional arrays; used by A026, A027, and A028
- `is_compound_return_call(arena, expr_id, ctx)` — returns true when an expression is a function call that returns a compound type (struct or array); used by A016, A017, and A018

## Testing

Unit tests live alongside each source file in `src/`. The integration test `rule_ids_match_diagnostic_rule_ids` in `lib.rs` asserts that:
- The number of registered rules equals the number of `AnalysisDiagnostic` variants.
- Each rule's `id()` matches the `rule_id()` of its corresponding diagnostic variant.

End-to-end tests that compile `.inf` source and assert on diagnostic output live in `tests/src/analysis/` (part of the `inference-tests` crate). Run them with:

```
cargo test -p inference-tests analysis
```

Test files are organized by rule group:

| File | Rules covered |
|------|---------------|
| `rules_a006_a011.rs` | A006–A011 (uzumaki, missing return, lint warnings) |
| `rules_a012_a022.rs` | A012–A022 (codegen restrictions, literal range) |
| `rules_a023.rs` | A023 (uzumaki in reassignment) |
| `rules_a024.rs` | A024 (extern function calls) |
| `rules_a025.rs` | A025 (uninitialized variable) |
| `rules_a026_a028.rs` | A026–A028 (nested compound depth, uzumaki on nested structs, uzumaki on struct arrays) |
| `rules_a029_a030.rs` | A029 (compound literal in compound assign), A030 removal acceptance tests |
| `rules_a031.rs` | A031 (unsupported compound return expression) |
| `rules_a035.rs` | A035 (direct and mutual/indirect recursion) |
| `rules_a036.rs` | A036 (cumulative stack depth exceeded) |
| `rules_a037.rs` | A037 (constant array index out of bounds) |
| `rules_a038.rs` | A038 (uzumaki on compound struct field) |
| `rules_a039.rs` | A039 (struct uzumaki passed as function argument) |
| `rules_a040.rs` | A040 (uzumaki as compound element of an array literal) |
| `walker_tests.rs` | `walk_function_bodies`, `WalkContext` depth tracking |

## Dependencies

| Crate | Role |
|-------|------|
| `inference-ast` | AST arena types, node kinds, `Location` |
| `inference-type-checker` | `TypedContext` input to every rule |
| `inference-fn-key` | `FnKey` — shared canonical function identity used to key the call graph |
| `thiserror` | Derive `Error` for `AnalysisDiagnostic` |

## Current Limitations

1. The walker visits all statements but does not expose expression-level traversal. Rules that need to inspect expressions must do their own descent.
2. Rules are executed sequentially on a single thread. The infrastructure is designed for parallel execution (rules are `Send + Sync`) but parallelism is not yet enabled.
3. `AssignToImmutable` and `VariableShadowed` remain in the type checker because they depend on scope state that the type checker tracks but the analysis pass does not replicate.
4. Nested compound type support (A026) limits nesting depth to one level. Structs whose fields are structs or arrays of structs are permitted; structs whose fields contain further nested structs or arrays-of-structs are rejected. This bound matches what the code generator can lower.

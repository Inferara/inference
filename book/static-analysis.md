# Static Analysis in Inference

The Inference compiler performs a static analysis pass between type checking and code generation. This document explains why that pass exists, what invariants it enforces, how it is structured internally, and what the formal verification implications are for each rule.

## Why Analysis Exists

The Inference compilation pipeline is:

```
parse -> type_check -> analyze -> codegen -> wasm_to_v
```

Type checking verifies that every expression has a consistent type: that a function receives arguments of the declared types, that struct fields are accessed correctly, that return types match. It answers the question "is this program well-typed?"

Analysis answers a different question: "does this program have the control flow structure that makes formal verification tractable?" These are separable concerns. A program can be perfectly well-typed and still contain control flow patterns that make it impossible — or extremely difficult — to reason about in a proof assistant.

The analysis crate (`core/analysis`) enforces those structural invariants. It receives the `TypedContext` produced by the type checker, runs a set of independent rules over every function body, and either returns an `AnalysisResult` containing advisory findings or an `AnalysisErrors` value that blocks compilation.

### Why Formal Verification Demands Stricter Control Flow

Inference targets Rocq (Coq) as its verification backend. Rocq is a proof assistant based on the calculus of constructions: proofs are total, all functions must terminate, and reasoning about a program requires that the program's execution paths be fully enumerable.

Inference's non-deterministic blocks (`forall`, `exists`, `unique`) make this requirement explicit at the language level. A `forall` block asserts that a property holds on all computation paths through the block. An `exists` block asserts that at least one path satisfies the property. These semantics are sound only when all paths are actually explored. A `break` or `return` inside such a block would short-circuit path exploration, silently invalidating the assertion without a type error. That is the class of mistake that analysis is designed to catch.

## Fundamental Principles

### Rule Independence

Each analysis check is a distinct zero-sized struct implementing the `Rule` trait. Rules do not communicate with each other. They do not share mutable state. Each rule receives the same `&TypedContext` and produces its own `Vec<AnalysisDiagnostic>` independently.

This design has several consequences. Rules can be added or removed without touching each other. Rules could be executed in parallel on separate threads — the `Send + Sync` bounds on `Rule` are specified now, not added later. Test cases for one rule cannot silently affect another.

### Severity Model

Every rule declares exactly one severity level: `Error`, `Warning`, or `Info`. The severity is not a runtime value — it is encoded in the rule's `severity()` method, which the `rule!` macro generates from a literal identifier at the call site.

The `analyze()` function in `core/analysis/src/lib.rs` routes each finding into one of three buckets based on its rule's severity:

```
Errors   -> Err(AnalysisErrors) when any are present
Warnings -> Ok(AnalysisResult)  always
Infos    -> Ok(AnalysisResult)  always
```

The bifurcation is intentional. Errors block the pipeline: `codegen` and `wasm_to_v` will not run if `analyze()` returns `Err`. Warnings and infos are advisory — the pipeline continues, and the orchestration layer decides whether to display them to the user.

This means a rule author makes a deliberate choice when assigning a severity. An error-severity rule is making a strong claim: no valid Inference program should ever trigger this finding, and any program that does must be corrected before it can be compiled.

### Exhaustive Collection

All rules run to completion before any findings are reported. The loop in `analyze()` iterates every registered rule and extends the appropriate bucket:

```rust
for &r in rules::all_rules() {
    let findings = r.check(typed_context);
    match r.severity() {
        Severity::Error   => errors.extend(findings),
        Severity::Warning => warnings.extend(findings),
        Severity::Info    => infos.extend(findings),
    }
}
```

There is no early exit on the first error. A developer fixing a compilation failure sees every problem in a single compile cycle, not one at a time. This is the same design Rust's type checker uses: collect all errors, report all errors.

### Diagnostic Format

Diagnostics follow the GCC/Clang/rustc convention:

```
<line>:<column>: <severity>[<rule_id>]: <message>
```

A concrete example:

```
3:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'
```

The message body follows a `what; why; how` structure. The `what` states the violation directly. The `why` explains the constraint in terms the developer can reason about. The `how` gives actionable guidance. All three are present in every diagnostic message.

When multiple diagnostics are present, they are sorted by source location (line, then column) before display, regardless of severity. A developer reading output from top to bottom encounters issues in the order they appear in the source file.

## Data Flow

```
TypedContext (read-only)
      |
      v
  analyze()
      |
      +-- for each Rule in all_rules():
      |       rule.check(ctx)
      |           |
      |           v
      |       walk_function_bodies(ctx, visitor)
      |           |
      |           +-- for each source file:
      |           |       for_each_function_body(defs)
      |           |           |
      |           |           v
      |           |       walk_block / walk_statement
      |           |           |
      |           |           v
      |           |       visitor(stmt_id, &WalkContext)
      |           |           { loop_depth, nondet_depth, nondet_block_kind }
      |           |
      |           v
      |       Vec<AnalysisDiagnostic>
      |
      v
  partition by severity
      |
      +-- errors non-empty -> Err(AnalysisErrors { errors, warnings, infos })
      +-- errors empty     -> Ok(AnalysisResult  { warnings, infos })
```

The analysis crate receives `&TypedContext` — a shared, read-only reference. It does not modify the AST, does not modify type annotations, and does not produce any output other than diagnostics. It is a pure query over an immutable data structure.

## The `rule!` Macro

Defining a rule requires a struct and a `Rule` trait implementation covering four methods: `id()`, `name()`, `severity()`, and `check()`. Without the macro, even a trivial rule requires roughly 25 lines of boilerplate. With the macro, the same rule is expressed in about 10 lines, and the connection between the struct declaration and the trait implementation is visually immediate.

### Syntax

```rust
crate::rule! {
    /// Break statement must appear inside a loop body.
    #[id = "A001"]
    #[name = "Break outside loop"]
    #[severity = error]
    pub struct BreakOutsideLoop;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        // implementation
    }
}
```

Each pseudo-attribute has a specific role:

- `#[id = "A001"]` — the string that appears in diagnostic output as `[A001]` and in `rule_id()`. Conventionally `A` followed by a three-digit decimal number.
- `#[name = "Break outside loop"]` — a human-readable name for tooling and documentation.
- `#[severity = error]` — one of the three literal identifiers `error`, `warning`, or `info`. Any other identifier is a compile error via `__severity!`.

### What the Macro Expands To

The macro produces a public struct with the given name and a full `Rule` trait implementation:

```rust
/// Break statement must appear inside a loop body.
pub struct BreakOutsideLoop;

impl crate::rule::Rule for BreakOutsideLoop {
    fn id(&self) -> &'static str { "A001" }
    fn name(&self) -> &'static str { "Break outside loop" }
    fn severity(&self) -> crate::errors::Severity {
        crate::__severity!(error)   // expands to Severity::Error
    }
    fn check(&self, ctx: &crate::rule::TypedContext) -> Vec<crate::errors::AnalysisDiagnostic> {
        // the body you wrote
    }
}
```

The struct is zero-sized. `Rule` objects are stored as `&'static dyn Rule` in the `all_rules()` slice, meaning no heap allocation occurs for rule instances.

### The `__severity!` Helper

The `__severity!` macro validates the severity identifier at compile time:

```rust
macro_rules! __severity {
    (error)   => { $crate::errors::Severity::Error   };
    (warning) => { $crate::errors::Severity::Warning };
    (info)    => { $crate::errors::Severity::Info    };
    ($other:ident) => {
        compile_error!(concat!(
            "invalid severity: `", stringify!($other),
            "`, expected `error`, `warning`, or `info`"
        ))
    };
}
```

The catch-all arm produces a `compile_error!` for any unrecognized identifier. This is a design choice: rather than silently defaulting to a severity or requiring a runtime parse, the compiler rejects malformed rule definitions at build time. There is no way to add a rule with an invalid severity and have it compile.

### Why a Macro Rather than a Derive

A derive macro operates on `struct` and `enum` items only. The `rule!` macro needs to capture a function body (`fn check(...) { ... }`) as part of the same syntactic unit as the struct declaration. That is not possible with derive — derive can only inspect the struct's fields and attributes, not an accompanying function definition. A declarative `macro_rules!` macro can match any syntactic pattern, including the function body, making the check implementation co-located with the struct declaration in a single syntactic block.

### Complete Example: A001

The full source of the simplest rule in the codebase:

```rust
crate::rule! {
    /// Break statement must appear inside a loop body.
    #[id = "A001"]
    #[name = "Break outside loop"]
    #[severity = error]
    pub struct BreakOutsideLoop;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Break)
                && walk_ctx.loop_depth == 0
            {
                errors.push(AnalysisDiagnostic::BreakOutsideLoop {
                    location: arena[stmt_id].location,
                });
            }
        });
        errors
    }
}
```

The check body is two conditions: the statement is a `Break`, and `loop_depth` is zero, meaning no enclosing loop exists at this point in the traversal. When both conditions are true, the statement's source location is captured in the diagnostic.

## The Shared Walker

Most rules need to visit every statement in every function body and inspect the statement in the context of its enclosing scopes. Implementing that traversal separately in each rule would produce identical boilerplate and separate compilation of the same code. `walk_function_bodies` extracts the traversal once.

### `walk_function_bodies` and `for_each_function_body`

`walk_function_bodies` is the entry point for rules that use the shared traversal. It iterates all source files in the `TypedContext`, calls `for_each_function_body` to locate every function body, asserts that the `WalkContext` depths are clean at function boundaries (a debug-time invariant check), and delegates to `walk_block` for each body.

`for_each_function_body` handles all definition kinds:

- `Def::Function` — calls the callback with the function's body block
- `Def::Struct` — iterates methods and calls the callback for each method's body
- `Def::Spec` — recurses into the spec's nested definitions
- `Def::Module` — recurses into the module's nested definitions (if the module has a body)
- `Def::Enum`, `Def::Constant`, `Def::ExternFunction`, `Def::TypeAlias` — skipped

This ensures every function body in the program is visited regardless of where it is defined: top-level functions, struct methods, functions inside spec blocks, and functions inside modules are all covered by a single call to `walk_function_bodies`.

### Depth Tracking

`WalkContext` carries three fields through the traversal:

```rust
pub(crate) struct WalkContext {
    pub loop_depth: u32,
    pub nondet_depth: u32,
    pub nondet_block_kind: Option<&'static str>,
}
```

`loop_depth` is incremented when the walker enters a `Stmt::Loop` body and decremented when it exits. A rule checking the placement of `break` reads `walk_ctx.loop_depth == 0` to determine whether the current statement is inside a loop.

`nondet_depth` is incremented when the walker enters a block with a non-deterministic `BlockKind` (`forall`, `exists`, `assume`, `unique`) and decremented on exit. A rule checking for statements inside non-det blocks reads `walk_ctx.nondet_depth > 0`.

`nondet_block_kind` stores the label of the innermost non-deterministic block (`"forall"`, `"exists"`, etc.) so that diagnostic messages can name the specific block kind. The walker saves and restores the previous value on entry and exit from each non-det block:

```rust
if block.block_kind.is_non_det() {
    let prev_kind = ctx.nondet_block_kind;           // save
    ctx.nondet_block_kind = Some(block_kind_label(block.block_kind));
    ctx.nondet_depth += 1;
    walk_statements(arena, &block.stmts, ctx, visitor);
    ctx.nondet_depth -= 1;
    ctx.nondet_block_kind = prev_kind;               // restore
}
```

This save/restore pattern handles nested non-det blocks correctly: if a `forall` block contains an `exists` block, the visitor sees `nondet_block_kind = Some("exists")` while inside the inner block, and `Some("forall")` again after exiting it.

At each function boundary, `walk_function_bodies` asserts that all three fields have returned to their initial values. This is a programming invariant, not user input validation: a mismatch would indicate a bug in the walker itself.

### Why `dyn FnMut`

The visitor parameter is `&mut dyn FnMut(StmtId, &WalkContext)` rather than a generic `impl FnMut(...)`. A generic parameter would cause the compiler to monomorphize `walk_function_bodies` separately for every closure passed to it — one copy per rule that uses the shared walker. With `dyn FnMut`, a single compiled copy of the walker is shared by every rule, at the cost of one indirect call per statement per rule. For a traversal whose bottleneck is AST access rather than dispatch overhead, the monomorphization savings dominate.

### A004: Custom Traversal

A004 (infinite loop without break) does not use `walk_function_bodies`. It implements its own traversal. The reason is that its check requires scoping logic that differs from the shared walker's logic in a critical way.

The question A004 asks is: "does this loop body contain a `break` that targets this loop?" A `break` inside a nested loop targets the inner loop, not the outer one, so it must not be counted. A `break` inside a non-det block is prohibited by A002, so the search can stop at non-det block boundaries. But a `break` inside an `if/else` arm or a regular `{ }` block does target the enclosing loop, so those must be recursed into.

The shared walker cannot express this. It tracks depth globally and visits every statement regardless of the nesting pattern. A004's `contains_break_for_this_loop` function implements exactly the required scoping:

- Recurses into `if/else` then-arms and else-arms
- Recurses into `Stmt::Block` when `block_kind == Regular`
- Does **not** recurse into `Stmt::Loop` bodies (break there targets the nested loop)
- Does **not** recurse into non-det blocks (`block_kind != Regular`)

This custom traversal is explicitly documented in a module-level comment in `core/analysis/src/rules/infinite_loop_without_break.rs` so that future maintainers understand why A004 diverges from the shared walker pattern.

## Current Rules

| ID | Name | Severity | What it checks | Verification rationale |
|----|------|----------|----------------|------------------------|
| A001 | Break outside loop | Error | `break` appears where no enclosing loop exists | Structural error: `break` has no target; the WASM `br` instruction it would lower to would be malformed |
| A002 | Break inside non-det block | Error | `break` appears inside a `forall`, `exists`, `assume`, or `unique` block | `break` would short-circuit path exploration; `forall` and `exists` require all paths to be enumerated for the assertion to be sound |
| A003 | Return inside loop | Error | `return` appears inside a loop body | Single-exit-point discipline simplifies the Rocq translation; multiple exits from inside a loop create branching proof obligations that are difficult to discharge uniformly |
| A004 | Infinite loop without break | Error | A `loop { ... }` without a condition contains no `break` that targets it | A non-terminating loop makes the Rocq translation non-total; Rocq requires proof of termination for every recursive construct |
| A005 | Return inside non-det block | Error | `return` appears inside a `forall`, `exists`, `assume`, or `unique` block | Same as A002: `return` exits the enclosing function, cutting off path exploration before the non-det block's assertion can be checked on all paths |

All five current rules are error-severity. Advisory rules (warning or info severity) are supported by the infrastructure but none have been defined yet.

## Adding a New Rule

### Decision: Shared Walker or Custom Traversal?

Use the shared walker when the rule's check is a simple predicate on each statement given its current depth counters. The visitor closure receives a `StmtId` and a `&WalkContext` and needs only to inspect those two values.

Use a custom traversal when the check requires different scoping than the shared walker provides — specifically, when "does this enclosing construct contain a pattern" needs to stop recursion at different boundaries than loop/non-det nesting. A004 is the canonical example.

### Step-by-Step Recipe

**Step 1.** Create `core/analysis/src/rules/your_rule.rs`:

```rust
//! AXXX: Description of what the rule checks.

use inference_ast::nodes::Stmt;
use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// One-line description for rustdoc.
    #[id = "AXXX"]
    #[name = "Human readable name"]
    #[severity = error]
    pub struct YourRuleName;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            // inspect arena[stmt_id].kind and walk_ctx fields
            // push to errors when the rule fires
        });
        errors
    }
}
```

**Step 2.** Register the module in `core/analysis/src/rules/mod.rs`:

```rust
pub mod your_rule;
use your_rule::YourRuleName;

pub fn all_rules() -> &'static [&'static dyn crate::rule::Rule] {
    &[
        // ... existing rules ...
        &YourRuleName,
    ]
}
```

**Step 3.** Add a diagnostic variant to `AnalysisDiagnostic` in `core/analysis/src/errors.rs`:

```rust
#[error("what happened; why it is a problem; how to fix it")]
YourDiagnosticVariant { location: Location },
```

**Step 4.** Add the `rule_id()` arm in the same file:

```rust
AnalysisDiagnostic::YourDiagnosticVariant { .. } => "AXXX",
```

**Step 5.** Update the `rule_ids_match_diagnostic_rule_ids` test in `core/analysis/src/lib.rs`. This test asserts that `all_rules().len() == diagnostics.len()` and that each rule's `id()` matches its corresponding diagnostic's `rule_id()`. Adding a rule without updating this test will fail the build.

```rust
AnalysisDiagnostic::YourDiagnosticVariant { location: dummy_location() },
```

**Step 6.** Write tests. The test suite for each rule lives alongside the rule source or in `tests/src/analysis/`. Test at minimum: the rule fires on a program that should trigger it, and the rule does not fire on a valid program.

## Related Resources

- `core/analysis/src/rule.rs` — `Rule` trait definition and `rule!` / `__severity!` macro implementations
- `core/analysis/src/walker.rs` — `walk_function_bodies`, `for_each_function_body`, `WalkContext`
- `core/analysis/src/lib.rs` — `analyze()` entry point, rule dispatch loop, `rule_ids_match_diagnostic_rule_ids` test
- `core/analysis/src/errors.rs` — `AnalysisDiagnostic`, `AnalysisErrors`, `AnalysisResult`, `Severity`
- `core/analysis/src/rules/` — one file per rule
- `book/unreachable-emission-in-codegen.md` — related discussion of control flow enforcement in the codegen pass
- `book/arithmetic-overflow-in-wasm-codegen.md` — example of another property with formal verification implications

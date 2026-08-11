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

Each analysis check is a distinct zero-sized struct implementing the `Rule` trait. Rules do not communicate with each other. They do not share mutable state. Each rule receives the same `&TypedContext` and produces its own `Vec<LabeledDiagnostic>` independently — each finding paired with the module path of the file it belongs to, so a multi-file report can name the file an imported finding came from.

This design has several consequences. Rules can be added or removed without touching each other. Rules could be executed in parallel on separate threads — the `Send + Sync` bounds on `Rule` are specified now, not added later. Test cases for one rule cannot silently affect another. And because a rule is a pure query over the `TypedContext`, the same `all_rules()` set runs unchanged inside the [language server](the-language-server.md), where each finding becomes an editor diagnostic tagged with its rule id.

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
      |           |           { loop_depth, nondet_depth,
      |           |             nondet_block_kind, module_path }
      |           |
      |           v
      |       Vec<LabeledDiagnostic>
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
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
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
    fn check(&self, ctx: &crate::rule::TypedContext) -> Vec<crate::errors::LabeledDiagnostic> {
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
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Break)
                && walk_ctx.loop_depth == 0
            {
                errors.push(LabeledDiagnostic::new(
                    walk_ctx.module_path.clone(),
                    AnalysisDiagnostic::BreakOutsideLoop {
                        location: arena[stmt_id].location,
                    },
                ));
            }
        });
        errors
    }
}
```

The check body is two conditions: the statement is a `Break`, and `loop_depth` is zero, meaning no enclosing loop exists at this point in the traversal. When both conditions are true, the statement's source location is captured in the diagnostic, wrapped in a `LabeledDiagnostic` carrying the module path of the file being walked.

## The Shared Walker

Most rules need to visit every statement in every function body and inspect the statement in the context of its enclosing scopes. Implementing that traversal separately in each rule would produce identical boilerplate and separate compilation of the same code. `walk_function_bodies` extracts the traversal once.

### `walk_function_bodies` and `for_each_function_body`

`walk_function_bodies` is the entry point for rules that use the shared traversal. It iterates all source files in the `TypedContext`, calls `for_each_function_body` to locate every function body, asserts that the `WalkContext` depths are clean at function boundaries (a debug-time invariant check), and delegates to `walk_block` for each body.

`for_each_function_body` handles all definition kinds:

- `Def::Function` — calls the callback with the function's body block
- `Def::Struct` — iterates methods and calls the callback for each method's body
- `Def::Spec` — recurses into the spec's nested definitions
- `Def::Enum`, `Def::Constant`, `Def::ExternFunction`, `Def::TypeAlias` — skipped

This ensures every function body in the program is visited regardless of where it is defined: top-level functions, struct methods, and functions inside spec blocks are all covered by a single call to `walk_function_bodies`. There is no module arm because
[modules are files](module-hierarchy-and-multi-file-compilation.md), not AST nodes — `walk_function_bodies` reaches an imported module's bodies by iterating every source file in the `TypedContext`.

### Depth Tracking

`WalkContext` carries four fields through the traversal:

```rust
pub(crate) struct WalkContext {
    pub loop_depth: u32,
    pub nondet_depth: u32,
    pub nondet_block_kind: Option<&'static str>,
    pub module_path: Vec<String>,
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

`module_path` is not nesting state: it names the file whose bodies are currently being walked (empty for the entry file) and is reset as `walk_function_bodies` moves from one source file to the next. A rule clones it into each `LabeledDiagnostic` it emits, which is how a finding inside an imported file is attributed to that file in a multi-file report.

At each function boundary, `walk_function_bodies` asserts that the three nesting fields have returned to their initial values. This is a programming invariant, not user input validation: a mismatch would indicate a bug in the walker itself.

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

Forty rules are registered in `all_rules()`. Thirty-five are
error-severity — they block compilation — and five are warnings; no
info-severity rule has been defined yet. Three ids in the numbering range
(A013, A021, A030) are currently unassigned. The tables below group the rules
by the invariant family they protect; the descriptions are condensed from the
rules' own doc comments.

### Control flow and termination

| ID | What it enforces |
|----|------------------|
| A001 | `break` must appear inside a loop body |
| A002 | `break` must not appear inside a non-deterministic block |
| A003 | `return` must not appear inside a loop body |
| A004 | an infinite loop must contain a reachable `break` |
| A005 | `return` must not appear inside a non-deterministic block |
| A007 | non-void functions must return on all code paths |
| A035 | direct and mutual/indirect recursion is forbidden |
| A036 | cumulative shadow-stack depth must not exceed the stack budget |

This family carries the core verification argument. A `break` with no
enclosing loop (A001) has no target — the WASM `br` it would lower to would be
malformed. A `break` or `return` inside a `forall`, `exists`, `assume`, or
`unique` block (A002, A005) would short-circuit path exploration, silently
invalidating an assertion whose soundness depends on every path being
enumerated. A `return` inside a loop (A003) breaks the single-exit discipline
that keeps the Rocq translation's proof obligations uniform. An infinite loop
with no reachable `break` (A004) makes the translation non-total — Rocq
requires termination. A function that can fall off its end without returning
(A007) is the same non-totality in another guise (see
[Unreachable Emission](unreachable-emission-in-codegen.md)). Recursion (A035)
and unbounded stack growth (A036) are rejected in the spirit of the Power of
Ten rules for safety-critical code: a statically bounded call graph is one the
proof — and the runtime — can always exhaust.

### Uzumaki placement

| ID | What it enforces |
|----|------------------|
| A006 | `@` must appear inside a non-deterministic block |
| A008 | a standalone `@` expression has no effect |
| A014 | an array `@` cannot be used as a function argument |
| A023 | `@` in a reassignment is not allowed |
| A027 | `@` on a nested struct type is rejected |
| A028 | `@` on an array of structs is rejected |
| A038 | `@` on a struct- or array-typed struct-literal field is rejected |
| A039 | a struct `@` cannot be used as a function argument |
| A040 | `@` on a struct- or array-typed array-literal element is rejected |

An `@` outside a non-deterministic block (A006) has no set of execution paths
to range over, and a standalone `@` (A008) selects a value nobody observes.
The rest of the family protects a codegen invariant: a *compound* (struct- or
array-typed) `@` lowers through a named stack slot, so every position that has
no such slot — an argument list, a literal field or element, a reassignment —
is rejected rather than silently mis-lowered.

### Compound values (structs and arrays)

| ID | What it enforces |
|----|------------------|
| A012 | compound literals cannot be passed directly as function arguments |
| A015 | compound literals appear only in supported positions |
| A016 | compound-returning function calls appear only in `let` or `return` |
| A017 | assignment from a compound-returning function call is rejected |
| A018 | method-call chains on compound-returning function calls are rejected |
| A026 | nested compound type depth must not exceed the maximum |
| A029 | compound literals in compound assignments are rejected |
| A031 | unsupported compound return expressions are rejected |

Compound values live in linear memory, not on the WASM value stack (see
[Memory Allocation](memory-allocation-in-wasm-codegen.md)). Every rule in this
family fences off a position where a compound value would need to materialize
without a named destination to own its memory.

### Values and indexing

| ID | What it enforces |
|----|------------------|
| A019 | an array index must be a 32-bit integer type |
| A022 | a numeric literal must fit the valid range of its target type |
| A037 | a constant array index must be within the array's bounds |

A022 exists because WebAssembly arithmetic wraps silently — the rule closes
the front door on values that could never round-trip through their declared
type (see [Arithmetic Overflow](arithmetic-overflow-in-wasm-codegen.md)).
A037 turns a guaranteed runtime trap into a compile-time error when the index
is statically known.

### Language restrictions

| ID | What it enforces |
|----|------------------|
| A024 | calls to unbound external functions are not supported in codegen |
| A025 | variable declarations must have an initializer |
| A032 | top-level `const` declarations are not yet supported |
| A033 | combined unary operators are prohibited |
| A041 | a function-local name is declared at most once per function body |
| A042 | non-deterministic constructs (`forall`/`exists`/`assume`/`unique`) are only valid inside a `spec` declaration |
| A043 | an entry-file top-level `pub fn` may not use a reserved export name (`memory`, `__stack_pointer`) |
| A046 | a unary minus applied to a numeric literal must be written glued to the digits (`-42`, never `- 42`) |

These are honesty rules: each rejects, with a named diagnostic, a construct
the pipeline does not (or does not yet) support — rather than letting it fail
obscurely further down. A025 and A041 also remove whole classes of ambiguity
(reads of uninitialized memory, shadowing) that would otherwise need proof
obligations of their own. A042, A043, and A046 are permanent rather than
not-yet-supported restrictions: non-deterministic blocks are proof-only by
design, the two reserved names collide with codegen's own synthetic WASM
exports, and a negative literal has exactly one spelling — the lexer folds the
sign into the digits only when they are written together, so a separated minus
is a negation of the bare magnitude and would make the same value compile or
fail on a space (`- 100` fits `i8`, `- 128` does not, though `-128` is a valid
`i8`). Rejecting the separated spelling is what keeps whitespace out of the
meaning of a program, in the same spirit as A033's ban on combined unary
operators.

### Advisory rules

| ID | Severity | What it flags |
|----|----------|---------------|
| A009 | Warning | an enum definition with no variants |
| A010 | Warning | a method that declares `self` but never references it |
| A011 | Warning | a struct definition with neither fields nor methods |
| A020 | Warning | unreachable code after `return`, `break`, or an infinite loop |
| A034 | Warning | a visibility modifier on a definition inside a `spec` body |

The warnings are the advisory tier the severity model was designed for: each
flags code that is almost certainly not what the author meant, but blocks
nothing — the pipeline continues and the finding is reported alongside any
errors.

## Adding a New Rule

### Decision: Shared Walker or Custom Traversal?

Use the shared walker when the rule's check is a simple predicate on each statement given its current depth counters. The visitor closure receives a `StmtId` and a `&WalkContext` and needs only to inspect those two values.

Use a custom traversal when the check requires different scoping than the shared walker provides — specifically, when "does this enclosing construct contain a pattern" needs to stop recursion at different boundaries than loop/non-det nesting. A004 is the canonical example.

### Step-by-Step Recipe

**Step 1.** Create `core/analysis/src/rules/your_rule.rs`:

```rust
//! AXXX: Description of what the rule checks.

use inference_ast::nodes::Stmt;
use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// One-line description for rustdoc.
    #[id = "AXXX"]
    #[name = "Human readable name"]
    #[severity = error]
    pub struct YourRuleName;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            // inspect arena[stmt_id].kind and walk_ctx fields; when the
            // rule fires, push a LabeledDiagnostic pairing
            // walk_ctx.module_path with the diagnostic
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

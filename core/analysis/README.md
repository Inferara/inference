# inference-analysis

Static analysis rules for the Inference compiler. Runs after type checking, before code generation.

## Architecture

Each analysis check is an independent struct implementing the `Rule` trait. Rules receive the
fully-typed `TypedContext` and return a list of `AnalysisDiagnostic` values. The `analyze()` entry
point runs all rules sequentially and collects every error before returning.

A shared `walk_function_bodies()` walker handles AST traversal with `loop_depth` and
`nondet_depth` counters so individual rules focus on detection logic only.

The `rule!` macro reduces boilerplate for rule definitions:

```rust
rule! {
    /// Break must appear inside a loop body.
    #[id = "A001"]
    #[name = "Break outside loop"]
    #[severity = error]
    pub struct BreakOutsideLoop;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        // implementation using walk_function_bodies
    }
}
```

## How to add a new rule

1. Create `src/rules/new_rule.rs` using the `rule!` macro (see existing rules for examples).
2. Add `pub mod new_rule;` to `src/rules/mod.rs`.
3. Add `&NewRule` to the static slice in `all_rules()`.

## Rules

| ID | Rule | Diagnostic Message |
|----|------|--------------------|
| A001 | `break` must be inside a loop body | `break statement is only valid inside a loop body; if you intended to exit the function, use 'return'` |
| A002 | `break` must not be inside a non-deterministic block | `break statement is not allowed inside a '{kind}' block; break would interfere with the path exploration required for formal verification; move the break outside the '{kind}' block` |
| A003 | `return` must not appear inside a loop body | `return inside a loop is not allowed; a single exit point per function simplifies formal verification; use break to exit the loop, then return after it` |
| A004 | Infinite `loop { }` must contain a reachable `break` | `infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop does not count)` |
| A005 | `return` must not appear inside a non-deterministic block | `return statement is not allowed inside a '{kind}' block; return would exit the enclosing function, interfering with the path exploration required for formal verification; move the return outside the '{kind}' block` |

## Diagnostic output format

Diagnostics follow the convention:

```
<line>:<column>: <severity>[<rule_id>]: <message>
```

Example:
```
1:5: error[A001]: break statement is only valid inside a loop body; if you intended to exit the function, use 'return'
3:10: error[A002]: break statement is not allowed inside a 'forall' block; break would interfere with the path exploration required for formal verification; move the break outside the 'forall' block
```

When multiple diagnostics are present, they are sorted by source location (line, then column). Messages follow a `what; why; how` structure separated by semicolons.

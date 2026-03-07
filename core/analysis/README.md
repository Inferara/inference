# inference-analysis

Static analysis rules for the Inference compiler. Runs after type checking, before code generation.

## Architecture

Each analysis check is an independent struct implementing the `Rule` trait. Rules receive the
fully-typed `TypedContext` and return a list of `AnalysisError` values. The `analyze()` entry
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
    fn check(ctx: &TypedContext) -> Vec<AnalysisError> {
        // implementation using walk_function_bodies
    }
}
```

## How to add a new rule

1. Create `src/rules/new_rule.rs` using the `rule!` macro (see existing rules for examples).
2. Add `pub mod new_rule;` to `src/rules/mod.rs`.
3. Add `Box::new(NewRule)` to the vec in `all_rules()`.

## Rules

| ID | Rule | Diagnostic Message |
|----|------|--------------------|
| A001 | `break` must be inside a loop body | `break statement is only valid inside a loop body` |
| A002 | `break` must not be inside a non-deterministic block (`forall`, `exists`, `assume`, `unique`) | `break statement is not allowed inside a non-deterministic block; ...break would disrupt path exploration; move the break outside the non-deterministic block` |
| A003 | `return` must not appear inside a loop body | `return inside a loop is not allowed; use break to exit the loop, then return after it` |
| A004 | Infinite `loop { }` must contain a reachable `break` | `infinite loop must contain a reachable break statement; a loop without a condition requires break to terminate (break inside a nested loop or non-deterministic block does not count)` |
| A005 | `return` must not appear inside a non-deterministic block | `return statement is not allowed inside a non-deterministic block; ...move the return outside the non-deterministic block` |

# Analysis-Guarded Panics

## Overview

Several `panic!` sites in `core/wasm-codegen/src/compiler.rs` are unreachable for valid
programs. The analysis pass (`core/analysis`) detects and rejects the corresponding programs
with structured diagnostics before codegen runs. The panics serve as defense-in-depth backstops:
if a program somehow bypasses the analysis pass (e.g., due to an analysis regression or by
constructing an AST directly in tests), the panic message identifies the compiler bug rather
than producing silently malformed WASM.

> **Note**: These guards were originally in the type checker (`TypeCheckError`) and were
> migrated to the analysis pass (`AnalysisDiagnostic`) as rules A012–A019. The panic
> backstops in codegen remain unchanged — only the upstream guard moved.

This document catalogues those guarded sites and explains the convention for adding new ones.

---

## Why Two Layers

**Analysis diagnostics** include source location, user-readable context, and actionable
suggestions. They are the primary rejection mechanism (rule A012–A019, A022–A024).

**Codegen panics** are opaque by comparison. Their purpose is not to communicate with users but
to catch compiler bugs: if the analysis pass ever fails to reject a forbidden pattern, codegen
fails loudly instead of silently emitting invalid bytecode or corrupting the shadow stack.

The combination satisfies two goals simultaneously:

- Separation of concerns: codegen can assume its input has passed analysis and focus on emission.
- Defense-in-depth: regressions in the analysis pass surface as an obvious panic rather than a
  subtle runtime failure.

---

## Current Inventory

### 1. Compound-returning method call in expression position — instance method

**File:** `core/wasm-codegen/src/compiler.rs`, function `lower_instance_method_call` (~line 1658)

**Panic message:**
```
Instance method call to compound-returning method '{mangled_name}' in expression position
without sret destination. Compound-returning calls are only supported in variable
initialization and return positions.
```

**Guard:** `AnalysisDiagnostic::CompoundReturnCallInExpressionPosition` (rule A016)

The sret calling convention requires the caller to pass a destination pointer as the first
argument. When the call appears in expression position (e.g., as an argument to another call),
there is no named destination to point at. The analysis pass rejects this before codegen runs.
Codegen panics if `sret_local` is `None` for a compound-returning method.

---

### 2. Compound-returning method call in expression position — associated function

**File:** `core/wasm-codegen/src/compiler.rs`, function `lower_associated_function_call` (~line 1759)

**Panic message:**
```
Associated function call to compound-returning method '{mangled_name}' in expression position
without sret destination. Compound-returning calls are only supported in variable
initialization and return positions.
```

**Guard:** `AnalysisDiagnostic::CompoundReturnCallInExpressionPosition` (rule A016)

Same invariant as the instance method case above, applied to `Type::method(args)` call syntax.

---

### 3. Array literal in unsupported position

**File:** `core/wasm-codegen/src/compiler.rs`, `lower_expression` arm for `Expr::ArrayLiteral` (~line 1456)

**Panic message (via `unreachable!`):**
```
array literal in unsupported position should have been caught by analysis pass
```

**Guards:**
- `AnalysisDiagnostic::ArrayLiteralAsArgument` (rule A012) — when the literal appears directly as a function argument
- `AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition` (rule A015) — when the literal appears in any other
  unsupported position (e.g., as an operand in a binary expression)

Array literals require a named frame slot for memory allocation. The codegen can only lower them
when `enclosing_var_name` is set (i.e., in a `let` binding, assignment, or `return`). The
analysis pass rejects all other positions before codegen reaches this branch.

---

### 4. Struct literal in unsupported position

**File:** `core/wasm-codegen/src/compiler.rs`, `lower_expression` arm for `Expr::StructLiteral` (~line 1439)

**Panic message (via `unreachable!`):**
```
struct literal in unsupported position should have been caught by analysis pass
```

**Guards:**
- `AnalysisDiagnostic::StructLiteralAsArgument` (rule A013) — when the literal appears directly as a function argument
- `AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition` (rule A015) — when the literal appears in any other
  unsupported position

Struct literals share the same frame-slot dependency as array literals. The analysis pass
enforces that struct literals only appear where the codegen can name a destination.

---

## `panic!` vs `todo!()`

Both crash the process, but they signal different things:

| Macro | Meaning | Expected resolution |
|-------|---------|---------------------|
| `todo!("...")` | Feature not yet implemented; will be built later | Implement the feature |
| `panic!("... type checker should have ...")` | Invariant violation; compiler bug if reached | Fix the type-checker regression |

When reading compiler.rs, use this distinction to understand whether a crash path represents
planned future work or an internal consistency check.

---

## Convention for Adding New Guarded Sites

Follow this sequence when a new pattern is unsupported in codegen and must be rejected at the
source level:

**Step 1 — Determine the right layer.**
If the restriction is a type error (e.g., wrong type for an operand), add it to
`core/type-checker/src/errors.rs`. If the restriction is a codegen limitation that does not
involve type correctness, add it as a new analysis rule in `core/analysis/src/rules/`.

**Step 2 — Add detection logic.**
For analysis rules: create `src/rules/my_rule.rs` using the `rule!` macro, register it in
`all_rules()`, and add the `AnalysisDiagnostic` variant. Add tests.

**Step 3 — Add a `panic!` backstop in codegen.**
At the exact site in `compiler.rs` where the unsupported pattern would be lowered, add:

```rust
// Guarded by AnalysisDiagnostic::YourNewRule (rule AXXX)
panic!(
    "Descriptive message explaining what invariant was violated and \
     that the analysis pass should have prevented this"
);
```

The comment and the panic message are both important — the comment helps future readers
navigate to the guard, and the message helps diagnose a real regression.

**Step 4 — Add a negative codegen test (recommended).**
In `tests/src/codegen/wasm/negative.rs`, use `try_codegen()` to construct a program that
bypasses the type checker and hits the panic:

```rust
#[test]
fn your_pattern_panics_in_codegen() {
    // Construct a program that would trigger the panic if the type checker were absent.
    // try_codegen runs codegen directly without the full pipeline guard.
    let result = try_codegen("...");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("expected substring from panic message"),
        "unexpected error: {err}"
    );
}
```

---

## Related Files

- `core/wasm-codegen/src/compiler.rs` — all guarded panic sites
- `core/analysis/src/rules/` — analysis rules that are the primary guard (A012–A019, A022–A024)
- `core/analysis/src/errors.rs` — `AnalysisDiagnostic` variant definitions
- `core/type-checker/src/errors.rs` — type-level guard error variant definitions
- `tests/src/codegen/wasm/negative.rs` — negative codegen tests that verify panic behavior
- `core/wasm-codegen/docs/arrays-and-memory.md` — sret calling convention details
- `core/wasm-codegen/docs/function-calls-lowering.md` — call lowering pipeline and `ResolvedCallee`

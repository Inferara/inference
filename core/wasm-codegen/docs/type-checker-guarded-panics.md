# Fail-Closed Code Generation

## Overview

Code generation runs last. Every construct that reaches it has already been parsed,
type-checked and analysed, so an arm with no lowering is looking at a program some earlier
phase was supposed to have rejected. This document describes what happens when one arrives
anyway: the *poison* mechanism that turns such an arrival into a refusal, which sites use it,
and which `assert!` / `panic!` invariants deliberately stay panics.

The short version: code generation never aborts the process on a program the front end
accepted. It records a typed error, stops lowering, and returns that error, which the CLI
renders as a diagnostic and exits 1.

**Prerequisites.** Familiarity with the pipeline (`parse → type_check → analyze → codegen`)
and with the analysis rule catalog in `core/analysis/README.md`.

---

## Why a poison slot and not a `Result`

The body-lowering recursion is **infallible**. `lower_statement`, `lower_block`,
`lower_expression` and `lower_assign_statement` emit into a `wasm_encoder::Function` and
return `()`; only the signature and layout helpers around them return
`Result<_, CodegenError>`. That shape is deliberate — emission has no partial success to
report and no per-leaf recovery to perform — but it leaves an arm with no lowering holding
an error and no way to return it.

The tool previously reached for was `todo!()`. That aborts the process with exit code 101
and stock Rust panic text, which `infs build` forwards verbatim, on a program the user was
just told is valid. Threading `Result` through the recursion instead would touch roughly 35
signatures and 134 call sites, and buys nothing here: rustc's `tainted_by_errors` and
clang's `ExprError()` / `hasErrorOccurred()` are the same shape as what follows.

So the compiler carries one slot:

```rust
/// The first refusal raised where the lowering recursion cannot return one.
poisoned: Option<CodegenError>,
```

and one setter, `Compiler::poison`, built on `get_or_insert` so the **first** error wins. A
later arm reporting a consequence of the first refusal cannot displace the cause.

---

## The invariant

```text
lower_statement ──┐
lower_block ──────┤   each returns immediately while `poisoned` is set
lower_expression ─┤
lower_assign_statement ─┘
                              │
                              ▼
        visit_function_definition_body
          … body loop …
          if let Some(error) = self.poisoned.take() { return Err(error); }   ← take point
          … epilogue …
          … trailing `End` …
          … covers_every_uzumaki assertion …
          … take_completed_function …
```

Two properties make this safe, and both are worth stating because a reader who assumes
otherwise will "fix" the mechanism into a second one.

**A poisoned body may be operand-stack-inconsistent, and that is fine.** A poisoning arm
emits nothing further, but two things can still leave a partial sequence behind. Some arms
poison an error a *callee* raised after that callee had already emitted — an sret return, a
call lowering, an indexed access. And the *enclosing* helpers still run their tails:
`lower_named_binding_init` still emits its `LocalSet`, a loop lowering still closes its
block. Neither matters, because `wasm_encoder::Function` does not validate, and the take
point sits ahead of both the epilogue and `take_completed_function` — the truncated body is
discarded rather than assembled.

**The take point precedes every consumer.** It runs before the epilogue, before the trailing
`End`, and before the vanilla-contract assertion that a choice-lowered body wrote no custom
opcode. A poisoned function therefore never reaches an assertion that would report a
consequence of the refusal as if it were a separate bug.

`poisoned` is reset per function, alongside the rest of the per-function state, at the single
body entry point.

**Green paths are unaffected.** Each guard is a branch on an `Option` that is `None` on every
compiling program, so no emitted byte moves.

---

## What a refusal looks like

`CodegenError::UnsupportedConstruct` is the variant every poisoning site raises:

```rust
UnsupportedConstruct {
    construct: String,
    rule: &'static str,
    location: Option<Location>,
}
```

rendered as

```text
[<line>:<column>: ]<construct> has no WebAssembly lowering; <rule> rejects it before code generation
```

and printed by the CLI as `Codegen failed: …`, exit code 1.

`location` is `None` where the refusal is made against a *type* rather than a node: the
layout helpers in `memory.rs` are handed a `TypeInfoKind` and have no source position to
report. Their callers usually do, so `Compiler::at` fills one in where it can — which is what
makes `pub fn f(s: string)` report a line and column instead of a bare sentence.

`rule` names what rejects the shape earlier: an `A0xx` id, a family of them written as a
singular noun phrase so it reads as the subject of "rejects it before code generation", or a
prose phrase such as `the type checker`. A few shapes have no earlier owner at all, and there
the field says so in prose.

**No compile-mode diagnostic code is minted.** The `P0xx` namespace belongs to proof-mode
obligations. A second compile-mode namespace would give every shape here a catalog entry that
a user can only reach by skipping analysis, so the rule id of the *real* owner is named
instead. `core/wasm-codegen/src/errors.rs` carries `NAMED_ANALYSIS_RULES`, the list of every
rule id the crate's diagnostics name, and a test pins it against the ids actually written in
the sources — with an anti-vacuity floor, so a scan that stopped matching fails rather than
passing.

---

## Who can reach a refusal

Every `UnsupportedConstruct` site is a **backstop**. On the command line, an earlier phase
rejects each of these programs first, with a source location and a repair. The refusal exists
for a library caller that drives code generation straight off a typed context without running
analysis, or that ignores the diagnostics it was handed — so such a caller gets a refusal
rather than a malformed artifact.

The sites, by owner:

| Construct | Rejected earlier by |
|---|---|
| a string literal | A048 |
| a `string` value in memory (frame or struct layout) | A048 |
| a `string` value in a signature | A048 |
| a unit value in memory | A049 |
| the unit-typed parameter `x`; a unit-typed parameter written `_` | A049 |
| the unit-typed binding `x` | A049 |
| a parameter declared by its type alone (`T`) | A050 |
| the uninitialized binding `x` | A025 |
| a number literal with no recorded type, one whose text does not fit the width it is typed at, one typed as a non-number | A022 and the type checker |
| a unit value returned from a function that declares a result | the type checker |
| `T::member` on a non-enum type | the type checker |
| a field access on a type with no fields | the type checker |
| the `**` operator | the type checker |
| an assignment to a target that is not a variable, an array element or a field | the type checker |
| a call whose callee resolves to no lowerable form | the type checker |
| a compound-returning method or associated function called where nothing provides a destination | A016, A017, A018 |
| an array or struct literal in a position that binds no variable | A012, A015 |
| an array or struct `@` in a position that binds no variable | A014, A038, A039, A040 |
| an `@` over a type with no value representation | A014, A038, A039, A040 |
| an `@` over a struct whose field is itself a struct | A027 |
| a generic type in expression position | *nothing* — generics are unimplemented (#320) |

The last row is the only one with no earlier owner, which is why its `rule` field says so in
prose instead of naming an id. Its two companions, `TypeNode::Generic` and
`TypeNode::Function`, are refused by signature lowering as `CodegenError::UnsupportedType`
rather than through the poison slot, because `val_type_from_type_id` already returns a
`Result`.

The layout boundary is worth one note of its own. `[string; N]` and `[(); N]` reach frame
layout through three entry points — `compute_frame_layout`, `array_index_elem_size` and
`lower_array_uzumaki` — and all three route through the two fallible wrappers,
`type_byte_size_with_visited` and `natural_alignment_with_visited`. The refusal lives there,
once, ahead of any instruction being emitted. Do not add a fourth check at the `element_size`
leaf.

---

## What deliberately stays a panic

Poison is for a typed `CodegenError`: a statement about the *program*. The panics that remain
hold no `CodegenError` and make no statement about the program at all. They are bookkeeping
invariants about the compiler's own tables — a lookup that must succeed because this same
compiler filled the table a few hundred lines earlier. There is nothing to tell a user, and a
refusal would report a compiler bug as a source-level diagnostic.

Four families:

**Frame-layout lookups.** `Destination variable 'v' not found in array_offsets or
struct_offsets`; `Array variable 'a' not found in frame layout offsets`; `Struct field 'f'
not found in layout for struct 'S'`. `compute_frame_layout` runs before any instruction is
emitted and enters a slot for every compound binding in the function. A miss is a divergence
between the layout pass and the emission pass, not a property of the source.

**Return-shape bookkeeping.** `sret function 'f' has neither ArrayReturnInfo nor
StructReturnInfo`. The two maps are populated from the same signature walk that decides a
function uses the sret convention at all.

**Recorded-type agreement.** `array literal 'a' has non-array type info`, at the two array
literal reassignment and initialization sites. These read the type checker's recorded kind for
an expression, but they are not a statement about the source: each is reached only after the
frame layout has already produced an *array* slot under the same binding name, so the panic
says the two tables disagree with each other. A program whose array literal is not typed as an
array never gets that slot.

**Exhaustiveness claims, written as `unreachable!`.** `element_size`, `store_instruction` and
`load_instruction` each carry one. Their justification is that the two layout wrappers above
them are exhaustive over `TypeInfoKind` — a `Bool | Number | Enum` scalar arm, a struct and a
`::`-qualified nominal type sized by struct resolution, an array sized recursively, and an
outright refusal for `string`, `()` and the generic, function and spec types. So a kind that
describes no bytes cannot reach a leaf that assumes it does.

The distinction to carry away:

| Site holds | Mechanism | Means |
|---|---|---|
| a `CodegenError` about the program | `self.poison(e); return;` | some phase should have rejected this source |
| a claim about the compiler's own tables | `panic!` / `unreachable!` / `assert!` | this compiler is inconsistent with itself |

---

## Adding a new site

**Step 1 — decide the layer.** A restriction about type correctness belongs in
`core/type-checker/src/errors.rs`. A restriction about what code generation can lower belongs
in `core/analysis/src/rules/` as a new rule, with a source location and a repair, because that
is the message a user will actually read.

**Step 2 — write the rule.** Create `src/rules/my_rule.rs` with the `rule!` macro, add
`pub mod` and the `&MyRule` entry to `all_rules()`, add the `AnalysisDiagnostic` variant with
its `location()` and `rule_id()` arms, and add the display test. All five steps, or
`rule_ids_match_diagnostic_rule_ids` fails.

**Step 3 — add the code generation backstop.** At the arm that would have lowered the
construct:

```rust
cov_mark::hit!(wasm_codegen_my_construct_rejected);
self.poison(CodegenError::UnsupportedConstruct {
    construct: "a description that completes \"… has no WebAssembly lowering\"".to_string(),
    rule: "A0XX",
    location: Some(arena[expr_id].location),
});
return;
```

Add the rule id to `NAMED_ANALYSIS_RULES` in `errors.rs`; the pinning test will tell you if
you forget. In a helper that already returns `Result`, return the error instead of poisoning
— poison is for the infallible recursion only.

**Step 4 — test the refusal, not the error.** Use `CodegenAttempt` from `tests/src/utils.rs`:

```rust
match codegen_attempt(source, AnalysisMode::Skip) {
    CodegenAttempt::Ok(_) => panic!("code generation must refuse this program, it produced a module"),
    CodegenAttempt::Panicked(payload) => panic!("code generation must refuse, not crash: {payload}"),
    CodegenAttempt::Rejected(message) => assert!(message.contains("…"), "got: {message}"),
}
```

`negative.rs` wraps exactly this in an `assert_codegen_rejects(source, needle)` helper; reuse
it rather than writing the match out again.

A test that checks only `is_err()` passes just as well when the compiler aborted, which is
exactly the failure this mechanism exists to prevent. `codegen_attempt` distinguishes `Ok`,
`Rejected` and `Panicked` for that reason.

**Step 5 — add a sweep row.** If the construct is reachable from source, add a single-offence
fixture to `tests/test_data/panic_free/` and a row to `SHAPES` in `tests/src/panic_free.rs`
declaring the stage it stops at. The sweep runs every fixture in the repository through the
whole pipeline in both compilation modes inside `catch_unwind`, and `Panicked` is the verdict
no row may declare.

---

## Related Files

- `core/wasm-codegen/src/compiler.rs` — the `poisoned` field, `Compiler::poison`, the four
  guards, and the take point in `visit_function_definition_body`
- `core/wasm-codegen/src/errors.rs` — `CodegenError`, `UnsupportedConstruct`,
  `NAMED_ANALYSIS_RULES` and its pinning test
- `core/wasm-codegen/src/memory.rs` — the two fallible layout wrappers that own the
  `string` / `()` refusal, and the exhaustiveness leaves below them
- `core/analysis/README.md` — the rule catalog the `rule` fields name
- `tests/src/panic_free.rs` — the sweep that keeps a new abort from shipping green
- `tests/src/codegen/wasm/negative.rs` — refusal tests, written against `CodegenAttempt`
- `core/wasm-codegen/docs/arrays-and-memory.md` — sret calling convention and frame layout
- `core/wasm-codegen/docs/function-calls-lowering.md` — call lowering and `ResolvedCallee`

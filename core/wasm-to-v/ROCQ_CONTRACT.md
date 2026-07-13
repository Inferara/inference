# Rocq Output Contract

This document describes the external Rocq definitions that the generated
`.v` files depend on, and the proof-skeleton shape the translator emits.
It is the **contract** between this crate and any downstream Rocq library
that consumes the generated `.v` files (the in-tree Inference Rocq
library plus anything that imports it).

## Required external Rocq definitions

The translator assumes a Rocq context that supplies the following:

- A `module` record with the per-section fields used by every emitted
  module: `mod_types`, `mod_funcs`, `mod_tables`, `mod_mems`, `mod_globals`,
  `mod_elems`, `mod_datas`, `mod_start`, `mod_imports`, `mod_exports`.
  Field types match WasmCertCoq.
- A `host` typeclass (`Context `{ho: host}`) under which all theorems are
  stated. Generated code wraps theorems in a `Section Host. ... End Host.`
  pair.
- A predicate `ValidModule : module -> Prop`. **One argument**: this is
  the proof obligation for the structural well-formedness of the module
  (typing, locality, validation of the WASM sections themselves). It does
  **not** mention spec indices.
- A type `assertion` (the downstream library's spec-assertion syntax,
  defined in its `Assertions` module) — the element type of every emitted
  `_specs` list.
- A predicate `ValidSpec : module -> list assertion -> Prop`. **Two
  arguments**: the module and a list of spec assertions, each of which
  must be witnessed by some function of the module. This is the per-spec
  verification obligation. The contract is intentionally generic: this
  document fixes the arity (`module → list assertion → Prop`) and the call
  shape (`Theorem valid_<mod>__<SpecName> : ValidSpec <mod> <mod>__<SpecName>_specs`);
  the downstream library defines what holding a spec assertion means.
  `ValidSpec` is the obligation for `forall`-quantified and regular spec
  functions: a **universal** safety property (the witnessing function is
  trap-free for every input, reaching the assertion).
- A predicate `ValidExistsSpec : module -> list assertion -> Prop`, the
  obligation for `exists`-quantified spec functions. Same arity and call
  shape as `ValidSpec` (`Theorem valid_<mod>__<SpecName>__exists :
  ValidExistsSpec <mod> <mod>__<SpecName>__exists_specs`), but a different
  property: **existential reachability** — there is an input from which
  the witnessing function runs to completion (without trapping) into the
  assertion. This is *more than trap-freedom*; a universal `ValidSpec`
  does not imply it.
- A predicate `ValidUniqueSpec : module -> list assertion -> Prop`, the
  obligation for `unique`-quantified spec functions (`Theorem
  valid_<mod>__<SpecName>__unique : ValidUniqueSpec <mod>
  <mod>__<SpecName>__unique_specs`): existential reachability **plus**
  that the witness input is the only non-trapping one.

`ValidSpec`/`ValidExistsSpec`/`ValidUniqueSpec` all share the
`module → list assertion → Prop` arity (assertion-valued: wasm-verifier
PR #2 for `ValidSpec`, issue #6 for the existential pair — the former
`module → list N → Prop` index-list forms are gone downstream).
`ValidModule` and `ValidSpec` are defined in the downstream library's
`Verifier` module; `ValidExistsSpec` and `ValidUniqueSpec` in its `Exists`
module; `assertion` in its `Assertions` module. The generated file always
`Require Import Verifier` (and `From Wasm Require Import host`, which
supplies the `host` typeclass behind the always-emitted `Section Host`
wrapper); it additionally `Require Import Assertions` when (and only
when) it emits at least one `_specs` list — so a zero-spec module's
output gains no WasmVerifier imports beyond `Verifier` — and
`Require Import Exists` when (and only when) it emits an
`exists`/`unique` obligation. With these imports the raw artifact
type-checks against the downstream library as-is (checked by replacing
the `Qed` skeletons with `Abort`).

The translator cannot yet synthesize assertion payloads from spec bodies,
so **every emitted `_specs` list is currently empty** —
`(@nil assertion)` — with the group's WASM function indices preserved in
a preceding comment (`(* function indices: … (assertion payloads
pending) *)`) for the downstream prover, who carries the semantic content
in standalone per-function lemmas (see wasm-verifier's
`examples/E2EQuantKinds.v` for the reference shape). Emitting real
assertion payloads is the next translator milestone.

The pre-#21 form `ValidModule : module -> list N -> Prop` is no longer
emitted. Downstream proofs that consumed the old 2-argument shape must
migrate; see [Migration](#migration) below.

## What the generator emits

For a WASM module compiled in `proof` mode with module name `Foo` and
two spec blocks `A`, `B` whose inner functions land at WASM function
indices `[3, 4]` and `[7]` respectively, the generator produces:

```coq
(* helpers + module record ... *)

Definition Foo : module := {| ... |}.

(* function indices: 3 4 (assertion payloads pending) *)
Definition Foo__A_specs : list assertion := (@nil assertion).
(* function indices: 7 (assertion payloads pending) *)
Definition Foo__B_specs : list assertion := (@nil assertion).

Section Host.
Context `{ho: host}.

(* Theorems *)
Theorem valid_Foo : ValidModule Foo.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_Foo__A : ValidSpec Foo Foo__A_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_Foo__B : ValidSpec Foo Foo__B_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.
```

### Quantified specs: `exists` / `unique`

When a spec function is `exists`- or `unique`-quantified, its obligation
is `ValidExistsSpec` / `ValidUniqueSpec` instead of `ValidSpec`, over a
kind-suffixed list. A spec's functions are partitioned by kind and each
non-empty group emits its own `_specs` list and theorem:

```coq
(* spec Q with a forall fn at index 3 and an exists fn at index 4 *)
(* function indices: 3 (assertion payloads pending) *)
Definition Foo__Q_specs : list assertion := (@nil assertion).
(* function indices: 4 (assertion payloads pending) *)
Definition Foo__Q__exists_specs : list assertion := (@nil assertion).

Theorem valid_Foo__Q : ValidSpec Foo Foo__Q_specs.
Theorem valid_Foo__Q__exists : ValidExistsSpec Foo Foo__Q__exists_specs.
```

The kind-suffixed list/theorem names join the kind with the reserved
`__` separator: `<mod>__<Spec>__exists_specs` / `valid_<mod>__<Spec>__exists`
and `<mod>__<Spec>__unique_specs` / `valid_<mod>__<Spec>__unique`. The `__`
(not a plain `_`) is deliberate: since `validate_rocq_identifier` forbids
`__` inside any module or spec name, a kind-suffixed name can never alias
another spec's `_specs` list (a plain-`_` form would let a spec literally
named `<Spec>_exists` collide with spec `<Spec>`'s exists list). A spec
with only `forall`/regular functions emits exactly its single
`_specs`/`ValidSpec` pair; a spec with no functions at all keeps the
legacy `_specs := (@nil assertion)` + `ValidSpec` shape (with no indices
comment). The obligation kind is recovered from the
`inference.spec_funcs` section (see below) — the vanilla WASM body no
longer carries the quantifier.

Notes:

- Per-spec lists and per-spec theorems are emitted **only when** there is
  at least one spec block. A module with zero specs emits only `Foo` and
  `Theorem valid_Foo`.
- The separator between the module name and the spec name is `__`
  (two underscores). Single `_` would be ambiguous when the user's spec
  names contain underscores: a module `foo_foo` with a spec `bar` and a
  module `foo` with a spec `foo_bar` would both emit `foo_foo_bar_specs`
  under the single-underscore scheme. The same shape of collision would
  reappear with `__` if either side were allowed to contain it. For
  example, a module `foo__bar` with a spec `baz` and a module `foo` with
  a spec `_bar__baz` would both produce `foo__bar__baz_specs` under
  single-underscore splitting. To prevent both cases,
  `validate_rocq_identifier` rejects any candidate name containing `__`
  (the source-level error variant is
  `InvalidIdentifierReason::ContainsDoubleUnderscore`).
- Spec entries are sorted by spec name. The order is deterministic
  regardless of how the spec map was assembled (codegen ordering,
  embedded-section ordering, or caller-supplied ordering).
- Every per-spec list is emitted as `(@nil assertion)` (not `[]` or
  `nil`): the explicit form resolves regardless of consumer scope/notation
  state at the `Require` site, which the emitter cannot guarantee. This
  matches the convention used by Coq's own program extraction and
  CompCert's AST emission (the same rationale that previously mandated
  `(@nil N)` over `[]%N`) — future contributors should not "modernize"
  to bracket notation.
- Spec names are validated against the Rocq identifier rules
  (see `core/wasm-to-v/src/rocq_names.rs`). A spec named `Definition`,
  `forall`, or `list` is rejected with `WasmToVError::InvalidRocqIdentifier`
  (or `RocqStdlibShadow`) at translation time — the generated file is
  always syntactically valid Rocq.

## Where the spec indices come from

In `proof` mode `wasm-codegen` populates `CodegenOutput.spec_func_indices_by_spec :
FxHashMap<String, Vec<u32>>` with the WASM function indices assigned to
every function emitted from inside a `spec` block. Codegen *also* embeds
the same map in the WASM binary as a custom section named
`inference.spec_funcs`, so a bare `.wasm` file produced by this compiler in
`proof` mode contains the per-spec mapping self-describingly.

### Wire format of the `inference.spec_funcs` payload

The payload uses LEB128 unsigned varints throughout (except the v2 kind
bytes, which are raw `u8`):

```text
version            : varuint32   -- 1 = legacy (indices only), 2 = with kinds
count              : varuint32   -- number of (spec_name, indices) pairs
repeated `count` times:
  spec_name_len    : varuint32
  spec_name_bytes  : utf-8       -- not NUL-terminated
  indices_count    : varuint32
  repeated `indices_count` times:
    func_idx       : varuint32
  -- version 2 only: one obligation-kind byte per index, same order:
  repeated `indices_count` times (v2 only):
    kind_byte      : u8          -- 0 = Spec, 1 = Exists, 2 = Unique
```

Entries are emitted sorted by spec name for deterministic, byte-stable
output. The decoder validates each spec name against the Rocq identifier
rules at the decode boundary, rejecting `WasmToVError::InvalidRocqIdentifier`
before any Rocq emission runs.

The obligation kind selects the downstream predicate (`Spec` →
`ValidSpec`, `Exists` → `ValidExistsSpec`, `Unique` → `ValidUniqueSpec`).
A `forall`/regular/`assume` spec function maps to `Spec`, so the encoder
emits **version 1** (no kind bytes) whenever every obligation is `Spec` —
keeping all pre-quantifier modules byte-identical — and **version 2** (the
trailing kind bytes) only when an `exists`/`unique` obligation is present.
Both decoders (the Rocq translator and the linker, which remaps each index
on link while carrying the kind byte through verbatim) accept either
version; a v1 payload decodes with every kind defaulting to `Spec`.

The leading `version` varuint32 is the contract's escape hatch. The
decoder rejects *unrecognised* versions (neither 1 nor 2) with
`WasmToVError::WasmParse` carrying the literal string "version" — see
`inference_wasm_codegen::SPEC_FUNCS_SECTION_VERSION` and
`…SPEC_FUNCS_SECTION_VERSION_WITH_KINDS` for the constants. A future
revision that bumps the version further will trip this branch on today's
parsers instead of silently misparsing the rest of the payload.

`wasm_to_v` / `translate_bytes` accept the map as an explicit argument and
also parse the embedded custom section:

- Explicit map non-empty and binary section absent: explicit wins.
- Explicit map empty and binary section present: binary wins.
- Both present and they agree: success.
- Both present and they disagree: `WasmToVError::EmbeddedSpecMismatch`.
  The translator refuses to silently override one side with the other.

In `compile` mode the spec map is empty and no custom section is emitted,
so compile-mode `.wasm` is byte-identical to pre-`spec` output.

## Migration

### `list N` → `list assertion` (assertion-valued specs)

The `_specs` lists were previously `list N` (WASM function indices).
They are now `list assertion` and emitted empty, with the indices in a
comment. Downstream proofs that destructured the index lists must move
that per-function content into standalone lemmas (wasm-verifier's
`examples/E2EQuantKinds.v` shows the pattern: the emptied obligation is
discharged over `[::]`, the per-function reachability/uniqueness lemmas
stand alone). This tracks wasm-verifier PR #2 (`ValidSpec`) and issue #6
(`ValidExistsSpec`/`ValidUniqueSpec`).

### Pre-#21 `ValidModule` (2-argument)

If you previously consumed:

```coq
(* Old, pre-#21 — no longer emitted *)
Theorem valid_Foo : ValidModule Foo Foo_specs.
```

…and `Foo_specs : list N` was the single union of all spec indices,
update consumers as follows:

1. Split `ValidModule` into the new 1-arg `ValidModule : module -> Prop`
   and `ValidSpec : module -> list N -> Prop` predicates. The
   well-formedness component of the old proof goes under `ValidModule`,
   the per-spec verification component goes under `ValidSpec` and must be
   discharged separately for each emitted `_specs` list.
2. There is no longer a single `Foo_specs`. Replace references with the
   per-spec `Foo__<SpecName>_specs` definitions emitted alongside.
3. The structural validity theorem is now `valid_Foo : ValidModule Foo`
   (one argument). Per-spec theorems are emitted with names of the form
   `valid_Foo__<SpecName>`.

See `CHANGELOG.md` under `### Breaking` for the corresponding API
changes on the Rust side.

## Related

- `core/wasm-to-v/README.md` — translator overview and examples.
- `book/compilation_targets.md` — when `proof` mode kicks in.
- `core/wasm-to-v/src/rocq_names.rs` — the rules a module or spec name
  must satisfy.

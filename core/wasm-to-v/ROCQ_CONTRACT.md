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
- A predicate `ValidSpec : module -> list N -> Prop`. **Two arguments**:
  the module and a list of WASM function indices, each of which must
  hold a property captured by the spec. This is the per-spec verification
  obligation. Each `n ∈ idxs` is a WASM function index in module `m`;
  holding `ValidSpec m idxs` asserts that, for every such index, the
  function at that index satisfies the per-spec invariant supplied by
  the consumer's Rocq library. The contract is intentionally generic:
  this document fixes the arity (`module → list N → Prop`) and the call
  shape (`Theorem valid_<mod>__<SpecName> : ValidSpec <mod> <mod>__<SpecName>_specs`);
  the downstream library defines what the per-spec invariant actually
  says about each indexed function.

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

Definition Foo__A_specs : list N := [3; 4]%N.
Definition Foo__B_specs : list N := [7]%N.

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

Notes:

- Per-spec lists and per-spec `ValidSpec` theorems are emitted **only
  when** there is at least one spec block. A module with zero specs
  emits only `Foo` and `Theorem valid_Foo`.
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
- Empty per-spec lists are emitted as `(@nil N)` (not `[]%N`) so that
  the generated definition type-checks regardless of whether
  `Open Scope N_scope` is in effect at the Require site. The `%N` scope
  notation depends on `Open Scope N_scope` being active at the consumer's
  `Require` site, which the emitter cannot guarantee. `(@nil N)` is the
  explicit form and resolves regardless of consumer scope state. This
  matches the convention used by Coq's own program extraction and
  CompCert's AST emission — future contributors should not "modernize"
  to `[]%N` because that breaks consumer modules that omit
  `Open Scope N_scope`.
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

The payload uses LEB128 unsigned varints throughout:

```text
version            : varuint32   -- currently 1; bump on breaking change
count              : varuint32   -- number of (spec_name, indices) pairs
repeated `count` times:
  spec_name_len    : varuint32
  spec_name_bytes  : utf-8       -- not NUL-terminated
  indices_count    : varuint32
  repeated `indices_count` times:
    func_idx       : varuint32
```

Entries are emitted sorted by spec name for deterministic, byte-stable
output. The decoder validates each spec name against the Rocq identifier
rules at the decode boundary, rejecting `WasmToVError::InvalidRocqIdentifier`
before any Rocq emission runs.

The leading `version` varuint32 is the contract's escape hatch. The current
decoder rejects unsupported versions with `WasmToVError::WasmParse` carrying
the literal string "version" — see
`inference_wasm_codegen::SPEC_FUNCS_SECTION_VERSION` for the constant. A
future revision that bumps the version will trip this branch on today's
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

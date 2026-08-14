# Rocq Output Contract

This document describes the external Rocq definitions that the generated
`.v` files depend on, and the proof-skeleton shape the translator emits.
It is the **contract** between this crate and the downstream Rocq library
that consumes the generated `.v` files.

## Consumer

The consumer is wasm-verifier (a private Inferara repository; this
document is the authoritative, in-repo statement of the contract —
verified against wasm-verifier commit `0c5d525e` — and the vendored
signature stub in `rocq-stub/` type-checks the emittable subset of it
locally), built on **vanilla WasmCert-Coq v2.2.0** — not the
`WasmCert-Coq-Essence` fork this crate previously targeted. The fork's
non-deterministic constructors (`BI_forall`, `BI_exists`, `BI_assume`,
`BI_unique`, `BI_uzumaki_num`) do not exist in vanilla WasmCert, so a
`spec` function's logical content can no longer be represented as WASM
instructions in the emitted module — it is translated instead into a
value of wasm-verifier's `hassert` assertion type and the `spec` function
itself is **omitted** from the module record entirely. This document
covers both halves of that change: the (unchanged) executable module
translation and the (new) `hassert` obligation translation.

This contract supersedes two earlier states, neither of which is emitted
any more:

1. The shape this crate actually emitted before this change: a **2-ary**
   `ValidModule <mod> <mod>__<Spec>_specs` (`specs : list N`, a list of
   WASM function indices), with no separate `ValidSpec` predicate at all.
2. A shape that was *documented* (in this file and in `CHANGELOG.md`,
   under issue #21) but **never implemented**: a 1-ary `ValidModule` split
   from a `ValidSpec : module -> list N -> Prop`. The translator never
   emitted this; the two-shape confusion is resolved below by describing
   only what the emitter now actually writes.

## Required Rocq context

The translator assumes a Rocq context supplying two logical libraries,
`Wasm` (vanilla WasmCert) and `WasmVerifier`, and writes exactly these
`Require` lines at the top of every emitted `.v` file:

```coq
Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier.
```

One further preamble line is conditional: a module carrying at least one
data segment also emits

```coq
Open Scope byte_scope.
```

because most data bytes are spelled with a `byte_scope` notation (see
below) and those parse only while that scope is open. Whether an `Import`
chain leaves the scope open is a detail of the library rather than of
this contract, so a module that can spell a byte notation states the
requirement itself. The line is keyed on a data segment being present
rather than on the bytes inside it: opening a scope nothing happens to
use is inert. A module with no data segment names no byte at all and
emits no such line.

Within that context, the translator depends on:

- A `module` record with the per-section fields every emitted module
  populates: `mod_types`, `mod_funcs`, `mod_tables`, `mod_mems`,
  `mod_globals`, `mod_elems`, `mod_datas`, `mod_start`, `mod_imports`,
  `mod_exports`. A `module_func` record (`modfunc_type`,
  `modfunc_locals`, `modfunc_body`). Field types match vanilla
  WasmCert-Coq v2.2.0.
- Element and data segments in the shapes wasm-verifier's `coq-wasm`
  dependency defines. `module_element` splits contents from placement:
  `modelem_init` is a list of *initializer expressions* (each a
  `list basic_instruction`), and the placement is the separate
  `modelem_mode` field (`ME_passive` | `ME_declarative` |
  `ME_active tableidx expr`). The binary format's shorthand of bare
  function indexes has no constructor of its own — it is desugared as the
  WASM specification defines it, one `BI_ref_func i` expression per
  index. `module_data`'s `moddata_init` is a `list byte`, where `byte` is
  CompCert's `Integers.byte` built from a `Z` by the exported
  `encode : Z -> byte`. That library abbreviates `encode` with two-digit
  **uppercase** hex notations in `byte_scope`, but its notation block is
  hand-written and covers 244 of the 256 values — `#12` .. `#19` and
  `#1C` .. `#1F` have no notation. A byte is emitted in its notation
  where one exists, and as the `encode` application that notation would
  have abbreviated (`(encode 18%Z)`) for the twelve that have none. The
  notations parse only while `byte_scope` is open, which is what the
  conditional preamble line above supplies; the applications need no
  scope.
- `BI_br_table : list N -> N -> basic_instruction` — the explicit label
  vector **and** the default label. The default is a separate immediate
  in the binary format and never appears in the vector, and a table whose
  vector is empty (`br_table 0`) is valid WASM that still carries one.
- A `host` typeclass (`Context `{ho: host}`). Every emitted theorem is
  wrapped in a `Section Host. Context `{ho: host}. ... End Host.` pair,
  even `ValidModule`'s, which does not itself depend on the host context —
  the section variable is simply unused there.
- The `term`/`hassert` inductives (wasm-verifier `theories/Assertions.v`):
  `term` has constructors `T_const`, `T_lvar`, `T_local`, `T_global`,
  `T_app`, `T_binop`, `T_relop`; `hassert` has (among others) `HA_false`,
  `HA_true`, `HA_not`, `HA_and`, `HA_ex`, `HA_pred`, `HA_has_type`,
  `HA_defined`, `HA_app_ok`, plus the heap-fragment constructors the
  emitter never produces. `T_app`/`HA_app_ok` take a `nat` function index
  (not `N`) and a `seq term` argument list.
- The sugar `Definition`s built on `hassert`, referenced by name from the
  printer rather than re-derived: `term_eq a b := HA_pred pred_eq (a ::
  b :: nil)`, `Himpl p q := HA_not (HA_and p (HA_not q))`,
  `Hor p q := HA_not (HA_and (HA_not p) (HA_not q))`. `Hall` (the
  universal-quantifier sugar) exists in the library but is not emitted by
  this milestone (see [Translation scheme summary](#translation-scheme-summary)).

  **Real library versus vendored stub.** Both bullets above describe
  wasm-verifier's own `theories/Assertions.v` — the library an emitted
  `.v` is ultimately checked against, and the thing this contract is a
  contract *with*. The vendored signature stub in
  [`rocq-stub/`](rocq-stub/README.md) is deliberately **strictly
  smaller**: it declares only the subset the emitter can actually print,
  so `T_global`, the entire heap fragment (`HA_emp`, `HA_star`,
  `HA_iter`, `HA_pto`, `HA_size`) and `Hall` are absent from it. That is
  not a contract gap. A name the real library has and the stub omits is a
  name no emitted module may mention, and the local `coqc` gate turns any
  accidental mention of one into an unbound-constructor error instead of
  a silently type-checking term. `rocq-stub/README.md` tabulates every
  omission and the mechanical reason it is unreachable; when a name
  becomes emittable, it has to be added back there in the same change.
- A 1-ary `ValidModule : module -> Prop` — structural well-formedness,
  always emitted, independent of specs.
- A `ValidSpec : forall `{ho : host}, module -> list hassert -> Prop` —
  the per-spec obligation, now **hassert-valued** rather than an index
  list.

wasm-verifier's own naming note (in its `theories/Verifier.v`) is worth
carrying forward: the *old* external-contract name (indexed by a WASM
function list) now lives in that library as `ValidSpecFI`; emitting
`ValidSpec` with that old payload would silently change what the name
means. The emitter never emitted that shape, so there is nothing to
reconcile on this side, but a downstream proof that expects
index-oriented `ValidSpec` should look for `ValidSpecFI` instead.

`seq` in the real library is a mathcomp notation for `list`. The emitted
`.v` imports no mathcomp and spells the payload type `list hassert`
directly — definitionally identical, syntactically plain standard-library
`list`.

## Emitted-file anatomy

The following is the complete, unedited `.v` output for
`tests/test_data/inf/rocq_spec_shapes.inf`, generated via
`infs build … --mode proof -v`. The source declares two executable
functions (`check`, `main`) and one `spec` block (`Shapes`) with a single
`forall`-quantified function (`shape_prop`) that exercises an `assume`
antecedent, an `if` guard, a cross-call into `check`, a universal `@`, a
`==` comparison, and a nested `exists` block:

```coq
Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier.

Definition Vi32 i := VAL_int32 (Wasm_int.int_of_Z i32m i).
Definition Vi64 i := VAL_int64 (Wasm_int.int_of_Z i64m i).
Definition Mt l et := {|modtab_type := {|tt_limits := l; tt_elem_type := et|}|}.
Definition Mm l := {|modmem_type := l|}.
Definition Mg mut t init := {|modglob_type := {|tg_mut := mut; tg_t := t|}; modglob_init := init|}.

Definition Mi m n d := {|
  imp_module := list_byte_of_string m;
  imp_name := list_byte_of_string n;
  imp_desc := d;
|}.

Definition Me n d := {|
  modexp_name := list_byte_of_string n;
  modexp_desc := d;
|}.

Definition Ma of al := {|memarg_offset := of; memarg_align := al|}.

Definition check : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi32 2) ::
    BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
    BI_if (BT_valtype None) (
      BI_const_num (Vi32 0) ::
      BI_return ::
      nil) (
      nil) ::
    BI_const_num (Vi32 1) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition main : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_const_num (Vi32 3) ::
    BI_call 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition rocq_spec_shapes : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (nil) (T_num T_i32 :: nil) ::
    Tf (nil) (nil) ::
    nil;
  mod_funcs :=
    check ::
    main ::
    nil;
  mod_tables :=
    nil;
  mod_mems :=
    nil;
  mod_globals :=
    nil;
  mod_elems :=
    nil;
  mod_datas :=
    nil;
  mod_start := None;
  mod_imports :=
    nil;
  mod_exports :=
    Me "main" (MED_func 1%N) ::
    nil;
|}.

Definition rocq_spec_shapes__Shapes_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 1))) (T_const (Vi32 0))))) (HA_and (Himpl (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 ((T_local 0%N) :: nil)) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0))))) (HA_ex (term_eq (T_lvar 0) (T_local 0%N)))).
Definition rocq_spec_shapes__Shapes_specs : list hassert := (rocq_spec_shapes__Shapes_hspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_rocq_spec_shapes : ValidModule rocq_spec_shapes.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_rocq_spec_shapes__Shapes : ValidSpec rocq_spec_shapes rocq_spec_shapes__Shapes_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.
```

Notes on the shape, in emission order:

- **Helpers**: `Vi32`/`Vi64`/`Mt`/`Mm`/`Mg`/`Mi`/`Me`/`Ma`, unchanged from
  before this contract.
- **Per-function `Definition`s**: one `module_func` per *executable*
  (non-spec) function, `check` and `main` here. `shape_prop`, the sole
  function inside `spec Shapes`, contributes no `Definition` at all — it
  is not part of the executable module.
- **Module record**: `mod_types` stays **complete** — all three function
  types are present even though `shape_prop`'s type is now unused by any
  `mod_funcs` entry — but `mod_funcs` lists only `check` and `main`. See
  [Spec-function omission and index remap](#spec-function-omission-and-index-remap).
- **Per-spec `hassert` definitions**: `<mod>__<Spec>_hspec{k} : hassert`
  for each obligation in source order (1-based), then a gathering
  `<mod>__<Spec>_specs : list hassert := (hspec1 :: … :: nil)`. A spec
  with no free-function obligations (only methods, or an empty
  `spec { }`) emits `(@nil hassert)` instead — explicitly typed so the
  `Definition` type-checks regardless of the consumer's `Require` order.
- **`Section Host` block**: always opens with the 1-ary `ValidModule`
  theorem (emitted for every module, spec-bearing or not), then one
  `ValidSpec` theorem per spec, each named `valid_<mod>__<Spec>` and
  consuming that spec's `_specs` definition. Every theorem carries an
  unfilled `(* TODO: fill the proof *)` body terminated by `Qed.` — the
  prover worker is what fills these; the `coqc` type-check gate
  (`tests/src/rocq_typecheck.rs`) rewrites `Qed.` to `Admitted.` before
  compiling so it can assert *type-checking*, not proof closure.

## Spec-function omission and index remap

A `spec` function is a downstream contract obligation, not part of the
executable module, so it is dropped from the `.v` module record — but
WASM function indices are dense and positional, so removing one shifts
every later function down by one. Every surviving reference to a
function index must be renumbered accordingly. This is `FuncRemap`
(`src/translator.rs`):

```text
remap(i) = i - |{ omitted spec-function indices strictly below i }|
```

`FuncRemap` is built once per module from the (already-merged)
`inference.spec_funcs` map and the function-import count, and offers two
forms:

- `instantiated(abs)` — the renumbered index into the emitted module's
  function space (imports, then surviving locals). Used for `BI_call`
  and `BI_ref_func` operands, export/element/start descriptors.
- `mod_funcs_index(abs)` — `instantiated(abs)` minus the import count,
  the `mod_funcs`-relative index `T_app`/`HA_app_ok` need.

Both are **fail-closed**: `instantiated` rejects a reference to an
omitted spec function (`HspecInconsistent`, "a surviving construct
references function N, which is an omitted spec function"), and
`mod_funcs_index` additionally rejects an imported function
("… only module-defined functions can be applied").

| Site | Renumbered how |
| --- | --- |
| Function bodies / `mod_funcs` list | `translate_functions` skips any absolute index `remap.is_omitted` reports; the omitted function contributes no `Definition` and no `mod_funcs` entry |
| `mod_types` (positional type indices) | **Unchanged** — kept complete. The type section itself is untouched, so a surviving function's `modfunc_type` needs no adjustment even though its own type index may now be unused by any function |
| `BI_call` operands | `translate_basic_operator`'s `Operator::Call` arm, via `remap.instantiated` |
| Export descriptors (`MED_func`) | `translate_module_export_desc`, via `remap.instantiated` |
| Element segments (function-index items) | `translate_element`, via `remap.instantiated`, before each index is wrapped in its `BI_ref_func` initializer expression |
| `BI_ref_func` operands | `translate_basic_operator`'s `Operator::RefFunc` arm, via `remap.instantiated`. An element segment's other item form spells its reference as this instruction directly, and a body may too, so both forms land on the same renumbered index |
| `mod_start` | via `remap.instantiated` |
| `T_app` / `HA_app_ok` targets | via `remap.mod_funcs_index` (see [T_app resolution discipline](#t_app-resolution-discipline)) |

Imports are never spec functions (codegen records only local functions
in `inference.spec_funcs`), so imported indices are always stable under
this remap; in practice the always-link pipeline invariant means the
translator never even sees an import (the static-merge linker satisfies
every import before `-v` runs), but the offset is still applied
correctly when a pre-link or third-party module is translated directly.

A regression test in `src/lib.rs`
(`omitting_a_spec_function_renumbers_a_surviving_cross_call` —
`func 0` calls `func 2` while `func 1` is the omitted spec function)
pins the exact-operand behavior: the emitted body must read `BI_call
1%N`, not `BI_call 2%N`, and the omitted function must contribute no
`Definition`. The `coqc` gate catches shape errors (a mis-aritied
constructor) but not a wrong index, so this exact-operand assertion is
the load-bearing check for remap correctness.

## T_app resolution discipline

`hassert` obligations reference callees **symbolically**, by the exact
name-section string codegen wrote via `FnKey::Display` (e.g. `is_prime`,
`lib.arith.add`, `Point.new`) — the static-merge linker carries an
`inference.hspecs` section verbatim (index-free) precisely so this
resolution can happen once, post-link, when the final function layout is
known.

`WasmParseData::resolve_app_symbols` (`src/translator.rs`) resolves every
symbol any obligation applies, up front, before any output is built:

1. Collect every `T_app`/`HA_app_ok` symbol referenced anywhere in
   `hspecs_by_spec` (`hassert_print::collect_symbols`).
2. Invert the **raw**, unsanitized name-section map (`raw_func_names_map`
   — kept separate from the Rocq-sanitized `func_names_map` used for
   `Definition` names, because a symbol like `Struct.method` is a valid
   WASM name-section string but not a valid Rocq identifier on its own).
3. Each symbol must resolve to **exactly one** defined function: zero
   matches is `HspecInconsistent` ("… which no defined function in the
   module carries"); more than one is `HspecInconsistent` ("… which N
   defined functions share; the target is ambiguous").
4. The resolved absolute index passes through `remap.mod_funcs_index`,
   which fails closed on an omitted (spec) or imported target.

This mirrors wasm-verifier's soundness discipline for the obligation
language:

- **`pred_eq`/2 only** — the Rust IR's `HAssert` has exactly one
  predicate variant, `TermEq`; there is no general `HA_pred` escape
  hatch, so every equality obligation the translator can produce is
  exactly `term_eq a b`.
- **Universal slots state their own typing** — every slot a payload
  reads is introduced under an explicit `HA_has_type (T_local i)
  T_i32`/`T_i64` antecedent. wasm-verifier's `ValidSpec` evaluates
  payloads through a strong-Kleene strictification (`Assertions.ktrue`)
  over unconstrained valuations: a negated equality demands its terms
  denote, so definedness needs no emitted conjunct (`!=` is the bare
  `nz(relop Ne …)`), while a slot readout the payload depends on must be
  guarded on explicitly — `T_local` is prover-uncontrolled and
  `T_app`-free, so the guard is honestly refutable rather than a
  silence escape.
- **Single-result `T_app`** — a call in *term* position must have a
  single scalar result (`ResultClass::Scalar`); a void or compound
  result is `P005` ("its result is not a single scalar").
- **`HA_app_ok` for a bare call** — a call used as a statement (its
  result, if any, discarded) instead becomes `HA_app_ok f τs`, at any
  result arity including void.

## Custom WASM sections

Two custom sections carry proof-mode metadata through codegen → linker →
`wasm-to-v`, emitted in that order in `finish_and_take` (`hspecs`
directly after `spec_funcs`, both after the `name` section):

### `inference.spec_funcs` (unchanged)

Per-spec WASM function indices, `varuint32` LEB128 throughout:

```text
version            : varuint32   -- 1
count               : varuint32   -- number of (spec_name, indices) pairs
repeated `count` times:
  spec_name_len    : varuint32
  spec_name_bytes  : utf-8
  indices_count    : varuint32
  repeated `indices_count` times:
    func_idx       : varuint32
```

### `inference.hspecs` (new)

Per-spec `hassert` obligations, owned by the `inference-hassert` crate
(`core/hassert/src/codec.rs`) so the encoder (codegen) and both decoders
(linker, `wasm-to-v`) share one implementation. LEB128 throughout;
`varu32` = unsigned LEB128:

```text
version      varu32 = 1
sym_count    varu32
  repeated sym_count times, STRICTLY ASCENDING and unique:
    name_len   varu32
    name_bytes utf-8              -- a function symbol; not NUL-terminated
spec_count   varu32
  repeated spec_count times, spec names STRICTLY ASCENDING and unique:
    name_len    varu32
    name_bytes  utf-8             -- folded spec name
    entry_count varu32
    repeated entry_count times, in source order:
      symbol_idx varu32           -- into the symbol table
      hassert                     -- preorder, tag-prefixed
```

Both a spec entry's own `fn_symbol` and any `App`/`AppOk` symbol inside
its tree are indices into one shared, sorted symbol table — the union of
every function symbol referenced anywhere in the map. Full tag tables for
the `hassert`/`term`/`HConst`/binop/relop encodings are documented in the
codec's own module doc (`core/hassert/src/codec.rs`); the tag values
follow each Rust enum's declaration order and are part of the wire
format.

**Determinism**: `encode` sorts the symbol table and the spec list, so
two `HSpecMap`s that are equal (regardless of insertion or map-iteration
order) encode to identical bytes.

**Decoder hardening**: both the leading `version` (rejecting anything but
`1`), and, specific to `inference.hspecs`: a 1024-byte sanity cap on any
symbol or spec-name key (`MAX_NAME_LEN` — deliberately larger than
`inference.spec_funcs`' 255-byte spec-name cap, because an hspecs function
symbol combines a spec name with function identifiers and so is a longer
kind of string), a 256-level cap on assertion/term nesting
(`MAX_TREE_DEPTH`, matching `wasm-to-v`'s unrelated
`MAX_EXPRESSION_DEPTH` WASM-body-nesting cap only by coincidence of
value), strict-ascending-and-thus-unique ordering on both the symbol
table and the spec list, bounds-checked counts (an advertised count is
rejected before any allocation it would drive), and full UTF-8/trailing-byte
validation. Because [`encode`] is infallible but [`decode`] enforces the
depth cap and a non-empty, capped-length name contract, codegen runs its
own pre-encode check (`hspecs_section::check_payload`, delegating to the
shared `inference_hassert::validate`) so an over-deep obligation or an
over-long identifier is refused with an actionable diagnostic
(`CodegenError::HspecTreeTooDeep` / `HspecNameTooLong`) naming the
offending spec and function, rather than silently producing a `.wasm`
that fails its own downstream decode.

### Explicit-vs-embedded merge (both sections)

`wasm_to_v` / `translate_bytes` accepts each map as an explicit argument
and also parses the embedded custom section, with identical semantics
for both sections:

- Explicit map non-empty, section absent: explicit wins.
- Explicit map empty, section present: the section wins.
- Both present and they agree: success.
- Both present and they disagree: `WasmToVError::EmbeddedSpecMismatch`
  (for `spec_funcs`) / `WasmToVError::EmbeddedHspecsMismatch` (for
  `hspecs`) — the translator refuses to silently prefer one side.

### Cross-invariant

Every spec name carrying `hspecs` obligations must also be a key of
`spec_funcs_by_spec` — `hspecs` is a **subset**, not an equal set: a spec
block containing only methods has function indices but no free-function
obligations. A `.wasm` violating this (an `hspecs` entry naming a spec
`spec_funcs` does not know about) is a corrupt proof artifact, rejected
with `HspecInconsistent` at parse time before translation begins.

Within a spec, `hspecs` entries are **not** positionally paired with
`spec_funcs`' index list — a spec block may contain both free functions
and methods, so each `HSpecEntry` carries its own `fn_symbol` and
entries are matched by name, not position.

## Language rules surfacing at `-v`

- **A042** (`NonDetOutsideSpec`, error, `core/analysis/src/rules/nondet_outside_spec.rs`):
  the non-deterministic block forms — inline `forall`/`exists`/`assume`/
  `unique` statement blocks and the function-body-modifier form
  (`fn f() forall { … }`) — are legal only lexically inside a `spec { }`
  declaration. The check is purely lexical (mode-independent) and fires
  in both compile and proof modes, so no Inference-compiled program can
  reach the codegen stage with non-det syntax outside a spec.
- **P001–P010** (fatal, `core/wasm-codegen/src/hassert/diag.rs`): a
  specification function that cannot be encoded as an obligation — or
  whose obligation says nothing — aborts code generation
  (`CodegenError::UntranslatableSpec`) rather than silently emitting an
  unverifiable module. Every diagnostic is collected before failing, so
  several mistakes in one spec surface together.

  | Code | Condition |
  | --- | --- |
  | P001 | Body is `exists`/`unique`/`assume`-quantified (this milestone translates `forall`-quantified and plain bodies only; nested `exists` is supported) |
  | P002 | A construct with no assertion encoding: `loop`, `break`, a `unique` block, `**`, array indexing, struct field access, a struct/array/string literal |
  | P003 | Reassignment (`Stmt::Assign`) in a specification body |
  | P004 | A non-scalar type in a term, parameter, or `@` position (only bool, integer, and enum values are representable) |
  | P005 | A call that cannot be represented as a `T_app`/`HA_app_ok` term: an external function, an instance method, an unresolved target, a non-deterministic-bodied callee, or (in term position specifically) a non-scalar result |
  | P006 | A bare `@` outside a `let` right-hand side or a call-argument position |
  | P007 | A `forall` block nested inside an `exists` context (needs `Hall`, deferred past this milestone; lifting it must also restore the `Hall` `Definition` to `rocq-stub/wasm_verifier/Assertions.v`, which omits it as unemittable) |
  | P008 | `@` at a compound (array/struct) type |
  | P009 | A specification *method* that carries a proof obligation the translation cannot deliver — quantified, or plain but stating a property. Never silently dropped, since a method has no free-function fallback path. A method that only computes stays a helper and is not reported |
  | P010 | A specification function whose obligation collapses to the vacuous `HA_true`: an empty or assert-free body, a body that only computes (`return`, pure `let`/`const`), a trailing `assume` (`Imp(p, ⊤) = ⊤`), or an `if` whose branches all vacuate. An obligation any proof discharges without reading the program is indistinguishable from no verification at all, so a computing helper belongs at file scope, where a specification function can still apply it as a `T_app` |

- **Non-det instructions in a surviving body** (`translator.rs`,
  `translate_basic_operator`'s `Operator::Forall | Exists | Assume |
  Unique | I32Uzumaki | I64Uzumaki` arm): rejected as
  `WasmToVError::UnsupportedFeature`. With A042 in place and spec
  functions omitted from the module record, this path is unreachable
  from Inference-compiled code; it is defense-in-depth against a foreign
  or hand-crafted `.wasm` that reintroduces one of these opcodes into an
  executable body.

- **Float, SIMD/vector, and conversion constructs** (`translator.rs`,
  the three grouped operator arms plus `translate_value_type`): rejected
  as `WasmToVError::UnsupportedFeature`. The context in "Required Rocq
  context" is the whole vocabulary the emitted `.v` may use, and it
  contains no `T_f32`/`T_f64`, no vector type or vector instruction, and
  no `cvtop`/`BI_cvtop`. Emitting any of them would produce a file that
  fails `coqc` at the consumer, so the translator refuses instead:

  | Rejected | Scope |
  |---|---|
  | float instructions | all loads, stores, constants, comparisons, unops, binops |
  | vector instructions | the entire SIMD proposal, relaxed-SIMD included |
  | conversion instructions | the whole `cvtop` block — sign-extension, saturating float-to-int, **and the integer width conversions** (`i32.wrap_i64`, `i64.extend_i32_s/u`), since the contract covers no conversion at all |
  | `f32`, `f64`, `v128` value types | every position: parameters, results, locals, globals, block result types |
  | unmodeled proposal families | GC, exception handling (modern and legacy), stack switching, tail calls, 128-bit wide arithmetic, typed function references, `memory.discard`, segment-indexed table operations |

  Like the non-det rule above, this is unreachable from
  Inference-compiled code — the language has no floating-point or vector
  types and its codegen emits no conversion — so the `coqc` gate over
  the Inference corpus can never exercise it. It is reachable only
  through foreign bytes: the external linking path and the public
  `translate_bytes` API. `core/wasm-linker` refuses the same content in
  external modules, so this is the second of two layers, and on the CLI
  path the linker's diagnostic normally arrives first.

  A value-type rejection is safe at any position because the
  section-level error accumulator is checked fail-closed: a rejected
  entry fails the whole translation rather than being omitted from a
  section and silently shifting every later index.

  One exception to the contract-attributed reasoning: `select t`
  (`TypedSelect`) is rejected even though the context *does* declare
  `BI_select : option (list value_type) -> basic_instruction`. No
  lowering is wired for it, and the message says so, attributing the
  gap to the translator rather than to WasmCert.

  The attribution in every message names the *contract*, not WasmCert:
  vanilla WasmCert-Coq does model floats and conversions (which is why
  the old float relop emission was *ill-typed* rather than unbound),
  but the wasm-verifier program logic covers none of that surface, so
  no such term can be verified. The stub mirrors the contract subset,
  not all of WasmCert.

## Migration

Three shapes exist in this project's history; only the third is emitted
today:

1. **Pre-#21 (what the translator actually emitted until this change)**:
   a **2-ary** `ValidModule <mod> <mod>__<Spec>_specs`
   (`_specs : list N`, WASM function indices), no separate `ValidSpec`.
2. **The #21-documented, never-emitted shape**: a 1-ary `ValidModule` and
   a `ValidSpec : module -> list N -> Prop`, described in this file and
   `CHANGELOG.md` but never implemented in `translator.rs`.
3. **Current**: a 1-ary `ValidModule : module -> Prop`, always emitted,
   plus `ValidSpec : module -> list hassert -> Prop`, hassert-**valued**
   rather than index-valued.

If a downstream proof consumed either of the earlier shapes:

- From shape 1 (2-ary `ValidModule`): split the well-formedness
  component of the old proof into the new `ValidModule <mod>` theorem;
  the per-spec verification component must be redone entirely against
  `hassert`-valued obligations rather than an index list — there is no
  mechanical translation from "these WASM functions satisfy an external
  property" to "this logical formula holds", since the whole point of
  this change is that the formula is now derived from the specification
  body instead of asserted externally.
- From shape 2 (never-emitted, so nothing to migrate on the Rust side):
  any Rocq-side scaffolding written against `ValidSpec : module -> list
  N -> Prop` must be redefined against the `list hassert` arity, or
  renamed — wasm-verifier's own index-oriented predicate is named
  `ValidSpecFI`, not `ValidSpec`.

## Translation scheme summary

Each `forall`-quantified (or plain) specification free function
translates to one `hassert` via a right-folded statement translator with
two polarities, universal (`Mode::Univ`) and existential (`Mode::Exist`)
(`core/wasm-codegen/src/hassert/translate.rs`):

| Source construct | Universal mode | Existential mode |
| --- | --- | --- |
| Parameter / forall-context `@` | `T_local` slot (sequential, never rewound), plus a pending `HA_has_type (T_local i) T_i32`/`T_i64` typing guard for the slot (64-bit for `i64`/`u64`, i32 for every other scalar) | — |
| Pending slot guards | Discharged at the next structural statement: fused into an immediately-following `assume`'s antecedent as `HA_and (guard, …, assume-body)`, otherwise an `Himpl` antecedent over the statement's claim conjoined with the rest of the block; a slot introduced after the last structural statement guards nothing (an unread slot introduced earlier is still guarded — uniformity beats a use analysis). A pure `let`/`const` is not structural, but one that binds a short-circuit witness drains here too, so the guard dominates the `HA_ex` instead of sitting inside it — the witness constraint reads the very slot the guard types | — (a prover-chosen `HA_ex` variable needs no typing guard) |
| Call-argument / `let`-bound `@` | (not applicable — only forall context takes slots) | `HA_ex` binder at the binding point; body uses `T_lvar` at its de Bruijn index, resolved by a final level-to-index pass (no shifting needed while building) |
| `assume { … }` | Implication antecedent (`Himpl`) | Conjunct (`HA_and`) |
| `if c { A } else { B }` | `HA_and (Himpl (nz c) A') (Himpl (eqz c) B')` | Strict `HA_or (HA_and (nz c) A') (HA_and (eqz c) B')` — no witness is fabricated via undefinedness |
| `&&` (assertion position) | `HA_and (left', HA_and (Cᵣ, right'))` | same |
| `\|\|` (assertion position) | `Hor (left', HA_and (Cᵣ, right'))` | same |
| `&&` (term position) | A fresh `HA_ex` witness `v` pinned by `Hor (HA_and (nz l) (HA_and Cᵣ (term_eq v r))) (HA_and (eqz l) (term_eq v 0))`; the binder wraps the enclosing statement's atom, `v` is the operator's term | same |
| `\|\|` (term position) | A fresh `HA_ex` witness `v` pinned by `Hor (HA_and (nz l) (term_eq v 1)) (HA_and (eqz l) (HA_and Cᵣ (term_eq v r)))` | same |
| `!` (assertion position) | Falsiness dual (De Morgan push-through) | same |
| `==` | `nz(relop Eq …)` — wasm-verifier's `ValidSpec` evaluates the payload through a strong-Kleene strictification (`Assertions.ktrue`), under which the negated equality demands the relop denote, so this is only dischargeable inside the slots' typing guards | strict `term_eq` |
| `!=` | `nz(relop Ne …)` — no per-side `HA_defined` conjunct: the strictified negation already demands both sides denote, so an emitted conjunct would be implied | same |
| A call in term position | `T_app` (single scalar result required) | same |
| A bare call statement | `HA_app_ok`, any result arity | same |

The binary-operator table (`+`, `-`, `*`, `/`, `%`, bitwise, shifts,
comparisons) mirrors `lower_binary_expression` exactly: number class and
signedness come from the left operand's type, sub-word results are
narrowed with the same `shl`/`shr_s` (signed) or `and`-mask (unsigned)
sequences codegen emits, and `**` has no encoding (`P002`).

`&&` and `||` are the exception, because they are the two operators
`lower_binary_expression` does not lower itself — it delegates them to
`lower_short_circuit_binary`, which compiles `a && b` to
`if a != 0 then b else 0` and `a || b` to `if a != 0 then 1 else b` over
canonical 0/1 truth values. The term language is strict in every operand
and has no conditional, so an eager `T_binop` term would demand a right
operand the program never evaluates and turn `x == 0 || 10 / x == 10 / x`
— true for every `i32` — into a refutable claim at `x = 0`. The pinned
witness above names the same two cases the compiled code branches on.
`Cᵣ` is the conjunction of the constraints the *right operand itself*
introduced; it rides in the arm that evaluates that operand, in term
position and in both assertion polarities, since a constraint planted
unconditionally would be demanded on the arm the source skips. The binder
still hoists to the statement's atom, and a witness the payload never
reads is emitted without its definition, so a specification that claims
nothing stays `HA_true`. Implication
and disjunction are explicit `Imp`/`Or` IR nodes — never a De Morgan
encoding the printer has to pattern-match — because wasm-verifier's
`Himpl`/`Hor` are definitionally-transparent `Definition`s the printer
can name directly. Every universal slot leads its antecedent with an
explicit `HA_has_type (T_local i) T_i32`/`T_i64` guard: `ValidSpec`
quantifies its valuations with no constraint at all, so a payload that
needs a slot readout to exist must say so itself, and `T_local` being
prover-uncontrolled and `T_app`-free keeps that antecedent honestly
refutable rather than a silence escape. This matches the canonical
worked example below.

The worked derivation to compare against is `prime_hspec1` from
wasm-verifier's own `theories/examples/PrimeExample.v`; the smart
constructors in `core/hassert/src/ir.rs` reproduce that tree node-for-node
in `canonical_prime_hspec1_structure`, the semantic anchor test for the
whole translation.

## Related

- [`core/wasm-to-v/README.md`](README.md) — translator overview and
  general (non-spec) translation examples.
- [`core/wasm-to-v/rocq-stub/README.md`](rocq-stub/README.md) — the
  vendored two-namespace signature stub this contract type-checks
  against locally, and how the `coqc` gate uses it.
- `core/hassert/src/ir.rs`, `core/hassert/src/codec.rs` — the `HAssert`/
  `HTerm` IR and the `inference.hspecs` wire format.
- `core/wasm-codegen/src/hassert/` — the specification-to-`hassert`
  translation pass and its `P0xx` diagnostics.
- `core/wasm-to-v/src/translator.rs`, `src/hassert_print.rs` — the
  index remap, `T_app` resolution, and the `hassert` → Gallina printer.
- `tests/src/rocq_typecheck.rs` — the `coqc` type-check gate.

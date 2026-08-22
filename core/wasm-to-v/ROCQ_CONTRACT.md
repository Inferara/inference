# Rocq Output Contract

This document describes the external Rocq definitions that the generated
`.v` files depend on, and the proof-skeleton shape the translator emits.
It is the **contract** between this crate and the downstream Rocq library
that consumes the generated `.v` files.

## Consumer

The consumer is wasm-verifier (a private Inferara repository; this
document is the authoritative, in-repo statement of the contract —
verified against wasm-verifier commit `0c5d525e`, the reachability
additions in its `theories/Exists.v` against `cf39c4f` — and the
vendored signature stub in `rocq-stub/` type-checks the emittable subset
of it locally), built on **vanilla WasmCert-Coq v2.2.0** — not the
`WasmCert-Coq-Essence` fork this crate previously targeted. The fork's
non-deterministic constructors (`BI_forall`, `BI_exists`, `BI_assume`,
`BI_unique`, `BI_uzumaki_num`) do not exist in vanilla WasmCert, so a
`spec` function's logical content can no longer be represented as
non-deterministic WASM instructions in the emitted module. What replaces
them depends on the function's quantifier kind:

- a `forall`-quantified (or plain) `spec` function is translated into a
  value of wasm-verifier's `hassert` assertion type and the function
  itself is **omitted** from the module record entirely;
- an `exists`- or `unique`-quantified `spec` function is **retained** in
  the module record with a vanilla (non-deterministic-free) body — each
  scalar `@` arrives as a hidden trailing *choice parameter*, and every
  `assume`/`assert` compiles to a trap-on-false filter — and its
  obligation is a `reachability_spec` record pairing that function with
  an `hassert` payload, consumed by the `ValidExistsSpec`/
  `ValidUniqueSpec` reachability predicates. The retained body is what
  the downstream judgment actually reduces, which is why it must stay in
  `mod_funcs` (see
  [Two judgments](#two-judgments-validspec-versus-the-reachability-predicates)).

This document covers all three: the (unchanged) executable module
translation, the universal `hassert` obligation translation, and the
reachability retention and emission.

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

Two parts of the preamble are conditional on module content. The
`From WasmVerifier` import gains a trailing ` Exists` —

```coq
From WasmVerifier Require Import Assertions Verifier Exists.
```

— exactly when the module carries at least one `exists`/`unique`
(reachability) obligation, because `reachability_spec` and the two
reachability predicates live in wasm-verifier's `theories/Exists.v` and
a forall-only module names nothing from it. (Coq's standard `List` also
exports an inductive named `Exists`; the two occupy separate namespaces
— one is a module, the other a term — and no emitted term spells bare
`Exists`, so the combination is unambiguous.) And a module carrying at
least one data segment also emits both of

```coq
Open Scope byte_scope.
Local Delimit Scope Z_scope with Zst.
```

because most data bytes are spelled with a `byte_scope` notation (see
below) and those parse only while that scope is open, while the rest are
spelled as `encode` applications whose argument carries an explicit `Z`
scope key — the only place an emitted module spells that key at all.
(The full key inventory of an emitted module is three: `%N` on indices
and the other structural `N` fields, `%nat` on a reachability record's
`reach_entry_arity`, and this `%Zst`; every other numeral is unkeyed,
taking its scope from the expected type.) Whether an `Import` chain
leaves `byte_scope` open, and which scope the `Z` delimiting key still
names once that chain has been walked, are details of the libraries
rather than of this contract, so a module that can spell either states
the requirement itself.

The second line exists because mathcomp's algebra library delimits its
own `int_scope` with the `Z` key (`ssrint.v`, alongside a `Number
Notation` on its `int`), so in any file whose `Import`/`Export` chain
applies that `Delimit` — the rebinding takes effect at import time, and
does not travel through a non-exporting intermediate —
`(encode 18%Z)` re-reads its argument as mathcomp's `int` and fails to
type-check against `encode : Z -> byte`. Delimiting is last-writer-wins:
whichever `Delimit` the chain applies last decides the key, an explicit
`%Z` is read through the key no matter which scopes the file opens (so
`Open Scope Z_scope.` does not recover it), and only a later re-delimit
does. Since a scope may carry several delimiting keys, the emitter takes
a private one for `Z_scope` rather than compete for `Z`, which would
take mathcomp's own `%Z` away from anything read alongside the module.
This mirrors mathcomp's own move in the other direction, where `ssrnat`
claims `%num` for `BinNat`'s scope after taking `%N` for its own (see
"mathcomp consumers" below). `Local` confines the claim to the emitted
file: a file-global `Delimit` would leak the key to every consumer that
imports the module.

Both lines are keyed on a data segment being present rather than on the
bytes inside it: a scope nothing uses and a key nothing spells are
equally inert. A module with no data segment names no byte and spells no
`Z` key, and emits neither line.

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
  have abbreviated (`(encode 18%Zst)`) for the twelve that have none.
  The notations parse only while `byte_scope` is open, and `Zst` is the
  private key the emitted module claims for the standard `Z_scope` its
  argument lives in — both supplied by the conditional preamble lines
  above, and neither one inherited from an import chain.
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
  `Hor p q := HA_not (HA_and (HA_not p) (HA_not q))`, and
  `Hall body := HA_not (HA_ex (HA_not body))` — the universal-quantifier
  sugar, whose body binds logical de Bruijn index 0 exactly as `HA_ex`
  does. A `forall` block nested inside an `exists` context emits it.

  **Real library versus vendored stub.** Both bullets above describe
  wasm-verifier's own `theories/Assertions.v` — the library an emitted
  `.v` is ultimately checked against, and the thing this contract is a
  contract *with*. The vendored signature stub in
  [`rocq-stub/`](rocq-stub/README.md) is deliberately **strictly
  smaller**: it declares only the subset the emitter can actually print,
  so `T_global` and the entire heap fragment (`HA_emp`, `HA_star`,
  `HA_iter`, `HA_pto`, `HA_size`) are absent from it. That is
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
- Only under the conditional ` Exists` import: the `reachability_spec`
  record —

  ```coq
  Record reachability_spec : Type := {
    reach_func : N;            (* index into mod_funcs of the emitted module *)
    reach_entry_arity : nat;   (* source parameters, before the choice suffix *)
    reach_visible_locs : seq N;(* producer-declared source-visible frame slots *)
    reach_payload : hassert
  }.
  ```

  — and the two predicates
  ``ValidExistsSpec : forall `{ho : host}, module -> list reachability_spec -> Prop``
  and `ValidUniqueSpec` at the same type (wasm-verifier
  `theories/Exists.v`). The library proves `ValidUniqueSpec` implies
  `ValidExistsSpec`, but the emitter selects exactly one predicate per
  obligation kind and never leans on that lemma.

wasm-verifier's own naming note (in its `theories/Verifier.v`) is worth
carrying forward: the *old* external-contract name (indexed by a WASM
function list) now lives in that library as `ValidSpecFI`; emitting
`ValidSpec` with that old payload would silently change what the name
means. The emitter never emitted that shape, so there is nothing to
reconcile on this side, but a downstream proof that expects
index-oriented `ValidSpec` should look for `ValidSpecFI` instead.

`seq` in the real library is a mathcomp notation for `list`. The emitted
`.v` imports no mathcomp and spells the payload types `list hassert` and
`list reachability_spec` directly — definitionally identical,
syntactically plain standard-library `list`.

### mathcomp consumers

The emitted `.v` is vanilla-Rocq-first: it imports no mathcomp, and the
standalone `coqc` gate elaborates it against plain standard-library
context. A consumer that *does* import mathcomp — wasm-verifier's proof
developments do — needs one accommodation, discovered while discharging
the emitted reachability obligations downstream
(Inference-Global-Software/wasm-verifier#42): re-delimit `%N` after the
mathcomp imports. Every `N` literal in an emitted file is spelled with
the standard-library scope key — `0%N`, `1%N` (indices, `reach_func`,
`reach_visible_locs`, `Ma` arguments). mathcomp's `ssrnat` rebinds that
key to `nat_scope` (keeping `%num` as its replacement key for `BinNat`'s
`N_scope`), so once `ssrnat` is imported — directly or through
`all_ssreflect`, `seq`, `div`, … — a `1%N` written to mean `1 : N`
re-reads as `1 : nat` and fails with type errors at the record fields.
The working recipe is one line, placed after the mathcomp imports and
before the emitted definitions:

```coq
Local Delimit Scope N_scope with N.
```

Delimiting is last-writer-wins, which is why placement after the
mathcomp imports matters, and why the line triggers Rocq's default-on
`hiding-delimiting-key` warning (prefix `#[warning="-hiding-delimiting-key"]`
to silence it). The trade is symmetrical: after the re-delimit,
mathcomp's own `%N`-keyed `nat` notations no longer parse in that file
— `%nat` and `%num` still do. `Local` also matters: a file-global
`Delimit` leaks to every file that imports the consumer and re-breaks
`%N` there in the other direction. The emitter deliberately
does not switch to `%num`: that key is *defined by `ssrnat`*, so it does
not exist in the mathcomp-free context this contract targets, and
emitting it would break both the standalone contract and this repo's
own `coqc` gate. `%N` is the right key for what the emitter targets;
the re-delimit line is the consumer's half of the bargain.

The `%Z` analogue of that hijack exists, and needs no consumer
accommodation because the emitter already carries it. mathcomp's algebra
library delimits its own `int_scope` with the `Z` key (`ssrint.v`,
alongside a `Number Notation` on its `int`), so `18%Z` re-reads as
mathcomp's `int` in any file whose `Import`/`Export` chain applies that
`Delimit` — one scope key over from the `ssrnat` case above, and the
same failure mode. Two files can be that file: the emitted `.v` itself,
should the backend build behind its preamble ever re-export mathcomp
algebra, and a consumer that imports it and restates emitted data bytes
in its own text.

The emitter answers both by spelling its `Z`-keyed literals — the
`encode` arguments of the twelve notation-less data bytes, and nothing
else — with the private `%Zst` key it claims in the preamble (see
"Required Rocq context" above). The two keys get different treatment out
of cost, not mechanism: emitted `%N` literals are everywhere — every
index, every payload a consumer restates, every committed golden and
downstream proof — so re-keying them would churn all of that, and the
one-line recipe above already covers the consumer files that read them;
the `Z` key is spelled only in the `encode` arguments of data bytes,
which only foreign or statically-linked modules carry, so the emitter
re-keys it at the source and nothing downstream has to know (#416). A
consumer restating emitted data bytes in its own file carries the
emitter's own line, `Local Delimit Scope Z_scope with Zst.`, alongside
them (with `ZArith` in scope for `Z_scope` to exist).

The discharge originally needed a second accommodation, now gone.
Emitted files once bound the `Ma` helper's offset argument as `of`,
which ssreflect turns into a keyword (its anonymous-binder syntax,
`of T` for `(_ : T)`), so the helper's `memarg_offset := of` field
stopped parsing once ssreflect was loaded, and a consumer had to keep
that one preamble line ahead of every ssreflect-importing line. The
binder is `ofs` now (#412): current emissions parse with mathcomp
loaded first, the re-delimit line above being the only accommodation
left, and only a `.v` generated before the rename still needs the old
ordering. The rename covers the fixed preamble, not names the source
supplies — emitted definition names are user identifiers printed
verbatim, so a source function named `of` would reintroduce the
collision for its own `Definition` line.

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

Definition Ma ofs al := {|memarg_offset := ofs; memarg_align := al|}.

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
  function inside `spec Shapes`, is `forall`-quantified and contributes
  no `Definition` at all — it is not part of the executable module. (An
  `exists`/`unique`-quantified spec function *would* keep its
  `Definition`; see
  [Reachability additions to the anatomy](#reachability-additions-to-the-anatomy).)
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

### Reachability additions to the anatomy

The following is the complete, unedited `.v` output for
`tests/test_data/inf/rocq_exists_spec.inf`, generated the same way (it
is also committed byte-for-byte as the golden
`tests/test_data/rocq/rocq_exists_spec.v`, which a regression test
compares against regenerated output). The source declares one executable
function (`double`) and one `spec` block (`ReachableDouble`) with a
single `exists`-quantified function (`ex_double`) that draws two named
choices (`let n: i32 = @`, `let wide: i64 = @`) and one anonymous
call-argument choice, filters them through `assume` blocks, and
cross-calls `double`:

```coq
Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier Exists.

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

Definition Ma ofs al := {|memarg_offset := ofs; memarg_align := al|}.

Definition double : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_local_get 0%N (*n*) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition ex_double : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*n*) ::
    BI_local_set 1%N (*n*) ::
    BI_local_get 1%N (*n*) ::
    BI_local_get 0%N (*lo*) ::
    BI_relop T_i32 (Relop_i (ROI_ge SX_S)) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 2%N (*wide*) ::
    BI_local_set 2%N (*wide*) ::
    BI_local_get 2%N (*wide*) ::
    BI_const_num (Vi64 0) ::
    BI_relop T_i64 (Relop_i (ROI_ge SX_S)) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 1%N (*n*) ::
    BI_call 0%N ::
    BI_local_get 1%N (*n*) ::
    BI_local_get 1%N (*n*) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 3%N (*__choice2*) ::
    BI_call 0%N ::
    BI_const_num (Vi32 0) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition rocq_exists_spec : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i64 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    double ::
    ex_double ::
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
    nil;
|}.

Definition rocq_exists_spec__ReachableDouble_specs : list hassert := (@nil hassert).
Definition rocq_exists_spec__ReachableDouble_exspec1 : reachability_spec :=
  {| reach_func := 1%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: 2%N :: nil); reach_payload := HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 1%N) (T_local 0%N)) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_ge SX_S)) (T_local 2%N) (T_const (Vi64 0))) (T_const (Vi32 0)))) (HA_and (term_eq (T_app 0 ((T_local 1%N) :: nil)) (T_binop T_i32 (Binop_i BOI_add) (T_local 1%N) (T_local 1%N))) (term_eq (T_app 0 ((T_local 3%N) :: nil)) (T_const (Vi32 0))))) |}.
Definition rocq_exists_spec__ReachableDouble_ex_specs : list reachability_spec := (rocq_exists_spec__ReachableDouble_exspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_rocq_exists_spec : ValidModule rocq_exists_spec.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_rocq_exists_spec__ReachableDouble : ValidSpec rocq_exists_spec rocq_exists_spec__ReachableDouble_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_exists_rocq_exists_spec__ReachableDouble : ValidExistsSpec rocq_exists_spec rocq_exists_spec__ReachableDouble_ex_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.
```

Notes on what reachability adds, in emission order:

- **Preamble**: the proof-contract import line reads
  `… Assertions Verifier Exists.` (the conditional trailing import).
- **The retained function**: `ex_double` is emitted as an ordinary
  `module_func` `Definition` and listed in `mod_funcs`. Its body is
  vanilla WASM: each `@` reads a hidden trailing choice parameter —
  `mod_types` entry 1 is
  `Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i64 :: T_num T_i32 :: nil) (nil)`,
  one source parameter (`lo`) followed by the three choices in source
  order, the anonymous one under the name-section label `__choice2` —
  and every `assume`/`assert` compiles to a trap-on-false
  `BI_if … BI_unreachable` filter. The body has no result and no
  `BI_return`; it exits by falling off the end (see
  [Two judgments](#two-judgments-validspec-versus-the-reachability-predicates)
  for why both are contract-forced).
- **The universal grammar is untouched**: `_specs : list hassert` and
  its `ValidSpec` theorem are still emitted for every spec name — here
  in the explicitly-typed empty form `(@nil hassert)`, since
  `ReachableDouble` has no `forall`/plain obligations. A forall-only
  module's output is byte-identical to what it was before reachability
  emission existed.
- **Per-obligation records**: one
  `<mod>__<Spec>_exspec{k} : reachability_spec` per `exists` obligation
  in source order (1-based), then a gathering
  `<mod>__<Spec>_ex_specs : list reachability_spec`. `reach_func` is the
  retained function's `mod_funcs` index (`%N`), `reach_entry_arity` the
  source parameter count (`%nat`), `reach_visible_locs` the ascending
  source-visible slot list (`%N`), and `reach_payload` the body's
  `hassert` — whose `T_local` indices are *frame* indices of the
  compiled function, entry parameters and choice parameters alike (the
  anonymous choice's slot `3` appears inside the payload but not in
  `reach_visible_locs`). A `unique` obligation is emitted identically
  under `_uqspec{k}`/`_uq_specs` (see the committed golden
  `tests/test_data/rocq/rocq_unique_spec.v`).
- **Theorems**: the `Section Host` block gains one
  `Theorem valid_exists_<mod>__<Spec> : ValidExistsSpec <mod>
  <mod>__<Spec>_ex_specs` per spec whose `exists` partition is
  non-empty, and one `valid_unique_<mod>__<Spec>` over `ValidUniqueSpec`
  per non-empty `unique` partition. An empty partition emits nothing —
  no empty `list reachability_spec`, no vacuous theorem — which is what
  keeps forall-only output byte-identical.

## Spec-function omission and index remap

What a `spec` function contributes to the `.v` depends on its quantifier
kind:

- A `forall`-quantified (or plain) spec function is a downstream
  contract obligation, not part of the executable module, so it is
  **omitted** from the `.v` module record — but WASM function indices
  are dense and positional, so removing one shifts every later function
  down by one. Every surviving reference to a function index must be
  renumbered accordingly.
- An `exists`/`unique`-quantified spec function is **retained**: its
  reachability judgment looks the function up in `mod_funcs` of the
  emitted module and reduces its (vanilla) body, so it keeps its
  `Definition` and its `mod_funcs` entry. Retained functions shift
  nothing — only omitted indices count toward the remap — but they must
  stay *unreferenceable* from executable constructs: the signature
  carries hidden choice parameters and the body traps on filtered
  paths, so it is not a callable.

Both concerns are `FuncRemap` (`src/translator.rs`):

```text
remap(i) = i - |{ omitted spec-function indices strictly below i }|
```

`FuncRemap` is built once per module from the (already-merged)
`inference.spec_funcs` map, the hspecs kind classification (which
indices are retained), and the function-import count, and offers three
forms:

- `instantiated(abs)` — the renumbered index into the emitted module's
  function space (imports, then surviving locals). The raw index
  arithmetic: it rejects an omitted spec function but *accepts* a
  retained one, because the `reach_func` computation needs exactly that.
- `referenced_instantiated(abs)` — `instantiated(abs)` behind a
  retained-spec-function rejection. This is the operand form: `BI_call`
  and `BI_ref_func` operands, export/element/start descriptors.
- `mod_funcs_index(abs)` — `instantiated(abs)` minus the import count,
  the `mod_funcs`-relative index `T_app`/`HA_app_ok` and `reach_func`
  need. Rejects an imported function.

All three are **fail-closed** (`HspecInconsistent`): `instantiated`
rejects a reference to an omitted spec function ("a construct retained
in the emitted module references function N, which is an omitted
specification function"), `referenced_instantiated` additionally rejects
a retained one ("a surviving construct references function N, which is a
retained `exists`/`unique` specification function; its body stays in the
emitted module only as the subject of its reachability obligation, not
as a callable"), and `mod_funcs_index` additionally rejects an imported
function ("… only module-defined functions can be applied"). The split
is deliberate: the reference guard cannot live inside `instantiated`
itself, because the reachability obligation's own `reach_func` lookup
goes through the same index arithmetic for the retained function — the
guard sits at the reference sites, and the `reach_func` computation
bypasses it.

| Site | Renumbered how |
| --- | --- |
| Function bodies / `mod_funcs` list | `translate_functions` skips any absolute index `remap.is_omitted` reports; the omitted function contributes no `Definition` and no `mod_funcs` entry. A retained index is not skipped — its vanilla body translates through the normal path |
| `mod_types` (positional type indices) | **Unchanged** — kept complete. The type section itself is untouched, so a surviving function's `modfunc_type` needs no adjustment even though its own type index may now be unused by any function |
| `BI_call` operands | `translate_basic_operator`'s `Operator::Call` arm, via `remap.referenced_instantiated` |
| Export descriptors (`MED_func`) | `translate_module_export_desc`, via `remap.referenced_instantiated` |
| Element segments (function-index items) | `translate_element`, via `remap.referenced_instantiated`, before each index is wrapped in its `BI_ref_func` initializer expression |
| `BI_ref_func` operands | `translate_basic_operator`'s `Operator::RefFunc` arm, via `remap.referenced_instantiated`. An element segment's other item form spells its reference as this instruction directly, and a body may too, so both forms land on the same renumbered index |
| `mod_start` | via `remap.referenced_instantiated` |
| `T_app` / `HA_app_ok` targets | via `remap.mod_funcs_index`, behind an explicit is-retained rejection (see [T_app resolution discipline](#t_app-resolution-discipline)) |
| `reach_func` fields | via `remap.mod_funcs_index` directly — the one consumer that must accept a retained index |

Imports are never spec functions (codegen records only local functions
in `inference.spec_funcs`), so imported indices are always stable under
this remap; in practice the always-link pipeline invariant means the
translator never even sees an import (the static-merge linker satisfies
every import before `-v` runs), but the offset is still applied
correctly when a pre-link or third-party module is translated directly.

Regression tests in `src/lib.rs` pin the exact-operand behavior:
`omitting_a_spec_function_renumbers_a_surviving_cross_call` (`func 0`
calls `func 2` while `func 1` is the omitted spec function — the emitted
body must read `BI_call 1%N`, not `BI_call 2%N`, and the omitted
function must contribute no `Definition`),
`retaining_a_spec_function_preserves_surviving_operands` (a retained
function shifts nothing), and
`omission_and_retention_renumber_independently` (mixed kinds: only the
omitted index moves the remap). The `coqc` gate catches shape errors (a
mis-aritied constructor) but not a wrong index, so these exact-operand
assertions are the load-bearing check for remap correctness.

## T_app resolution discipline

`hassert` obligations reference callees **symbolically**, by the exact
name-section string the emitted module carries for them — the
static-merge linker carries an `inference.hspecs` section verbatim
(index-free) precisely so this resolution can happen once, post-link,
when the final function layout is known.

Two producers write those strings, and an obligation may name either:

- **compiled from source** — code generation's own mangled name
  (`is_prime`, `Point.new`);
- **linked from an external `.wasm`** — the name the merge gives the
  body it splices in for a satisfied import,
  `inference_fn_key::merged_name::root` (`mathlib.sum`). Code generation
  writes that same string for a call to a bound `external fn`, resolving
  the declaration by `DefId` rather than by name: two `external fn`s may
  share a name across scopes and only one of them be bound (a `use … from`
  clause binds top-level declarations only), so a name-keyed lookup would
  hand a spec-inner declaration the top-level one's origin and name a
  merged body the call does not reach. `A024` resolves unbound-extern
  calls through the same scope walk and rejects such a call first, so the
  agreement is defense in depth — except on a pipeline that skips
  analysis, where it is the only guard. An *unbound* extern stays `P005`
  — no module supplies a body for the downstream realization obligation
  to reduce.

An obligation about a linked external therefore resolves only against
the **merged** module. Translating the compiler's direct output instead
leaves the symbol naming an import, which `resolve_app_symbols` rejects
with a message naming the missing link step.

### What a linked body brings that compiled code cannot

A merged body is translated by exactly the paths a compiled one is —
there is no separate lowering for foreign functions, and the module
record cannot tell them apart. What differs is the *instruction
selection*: the body was chosen by whatever compiler produced the
external, so it can spell things the Inference emitter has no way to
emit. Nothing here relaxes the contract; the accepted surface is the
same for both, and it is `core/wasm-linker`'s envelope — not this
translator — that decides which foreign bodies arrive at all.

The gap is worth stating because it is the only reason the accepted
surface is more than a description of one emitter's output.
`tests/test_data/wasmlib/rustlib.wasm`, a committed
`wasm32-unknown-unknown` artifact, is merged into
`tests/test_data/inf/spec_linked_toolchain.inf` by the `coqc` corpus so
these shapes are elaborated rather than merely permitted. Through it the
gate sees a `BI_select` standing in for a branch LLVM removed, a
`BI_loop` carrying a result type where Inference's `while` lowering
always emits `BT_valtype None`, `BI_cvtop` in both directions from a
32x32 high-product whose 64-bit intermediate no narrower lowering
computes, and a `BI_load` off a pointer walked by a loop-carried local.
The `BI_cvtop` pair matters most of the four: the integer width
conversions are accepted by this translator, and until that artifact
existed every module elaborating one was hand-assembled. An obligation applying such a body — `T_app` at the
merged function's own index — is what makes the claim about it a claim
about the bytes that will run.

A declared **write set** — `mut` on an `external fn` parameter, checked by
`core/wasm-linker` against the merged bytes before the merge is allowed to
happen — is a link-time gate with no representation anywhere in this
translator's output, and that is correct rather than an omission. It decides
*whether* a body may be merged at all; once merged, the body is bytes like any
other, translated by exactly the paths above. The module the translator reads
has no imports left for a record to describe — the linker either satisfied
every one or refused to produce the module — so there is nothing for a write
set to be attached to by the time this translator runs, and no obligation
quantifies over "what an import was permitted to write" in the first place. A
merged function's stores are already present in the translated body, not
promised by a signature a proof would need to consult.

The module record changes shape too, in one place. Code generation emits
its linear memory with the minimum and the maximum equal, so a module it
produced alone always reads `Mm {|lim_min := N%N; lim_max := Some(N%N)|}`.
A memoryless main that adopts a Tier-B external's memory takes that
external's limits verbatim, and a `wasm32-unknown-unknown` artifact
declares no maximum — so `lim_max := None` is a shape only a link
produces, and `spec_linked_toolchain.inf` is the first corpus module to
put it in front of `coqc`.

`WasmParseData::resolve_app_symbols` (`src/translator.rs`) resolves every
symbol any obligation applies, up front, before any output is built:

1. Collect every `T_app`/`HA_app_ok` symbol referenced anywhere in
   `hspecs_by_spec` (`hassert_print::collect_symbols`).
2. Look each symbol up in one shared inversion of the **raw**,
   unsanitized name-section map (`raw_func_names_map` — kept separate
   from the Rocq-sanitized `func_names_map` used for `Definition` names,
   because a symbol like `Struct.method` is a valid WASM name-section
   string but not a valid Rocq identifier on its own). The inversion is
   built once per `translate()` and also feeds the reachability target
   classification (see
   [The name section is load-bearing](#the-name-section-is-load-bearing-for-reachability)),
   so the two resolutions cannot disagree about what a symbol names.
3. Each symbol must resolve to **exactly one** defined function: zero
   matches is `HspecInconsistent` ("… which no defined function in the
   module carries"); more than one is `HspecInconsistent` ("… which N
   defined functions share; the target is ambiguous").
4. A retained `exists`/`unique` spec function is rejected explicitly ("a
   specification function is the subject of its own obligation, not an
   interpretable symbol") — necessary because a retained index passes
   the arithmetic below. The resolved absolute index then passes through
   `remap.mod_funcs_index`, which fails closed on an omitted (spec) or
   imported target.
5. The application's **arity** must equal the resolved function's
   parameter count. This is the only place that can be checked: a
   `T_app`'s arguments are a `seq term`, so an application of the wrong
   width is still well-formed Gallina — it elaborates, the `coqc` gate
   passes, and the obligation goes on to state something about a
   function other than the one it names.

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
- **…and the values their declaration admits** — the same
  unconstrained-valuation reading is why a *width* alone is not enough.
  `u8`, `u16`, `i8`, `i16`, `bool` and every enum tag all ride `i32`, so
  a slot's hypothesis carries its declared value domain beside its class,
  grouped into one antecedent:
  `HA_and (HA_has_type (T_local i) T_i32) (nz (relop LtU (T_local i) 256))`
  for a `u8`. The sets are exactly those code generation's normalization
  of a draw produces — `<u 256`/`<u 65536` for the zero-extending
  widths, a signed pair `-128 <=s x < 128`/`-32768 <=s x < 32768` for the
  sign-extending ones, `<u 2` for `bool`, `<u N` for an N-variant enum,
  nothing for `i32`/`u32`/`i64`/`u64` — so signedness follows the
  normalization rather than the source spelling, and the upper bound is
  strict at the domain's exclusive top. Existentially the same bound is an
  `HA_and` conjunct *inside* the binder rather than an antecedent: under
  `HA_ex` an `Himpl` would let a proof pick an out-of-domain witness,
  refute the antecedent and discharge the obligation without meeting the
  payload. A bound over a variable the claim never reads is dropped, so a
  specification that claims nothing still collapses to the `HA_true`
  `P010` refuses. An uninhabited declared type is `P015`.
- **Single-result `T_app`** — a call in *term* position must have a
  single scalar result (`ResultClass::Scalar`); a void or compound
  result is `P005` ("its result is not a single scalar").
- **`HA_app_ok` for a bare call** — a call used as a statement (its
  result, if any, discarded) instead becomes `HA_app_ok f τs`, at any
  result arity including void.

## Two judgments: `ValidSpec` versus the reachability predicates

A `forall` obligation and an `exists`/`unique` obligation are not two
flavors of one predicate — they are different *kinds of statement*, and
the asymmetry runs through everything this contract emits.

`ValidSpec` is **denotational**: the payload is a logical formula
evaluated over valuations the predicate constrains in no way, and the
spec function's compiled body plays no part in the judgment at all —
which is why the function can be omitted from the module record, why
every slot readout must carry its own `HA_has_type` guard, and why a
slot-index skew there would be inert (an unconstrained valuation has no
"right" index to disagree with).

`ValidExistsSpec`/`ValidUniqueSpec` are **operational**: the predicate
looks `reach_func` up in `mod_funcs` of the emitted module and *reduces
the retained body* under vanilla WASM semantics. Entry arguments range
over the first `reach_entry_arity` parameters; the predicate itself
quantifies the trailing choice parameters (existentially — at least one
choice vector must reach a non-trapping exit; `unique` additionally
compares the exits). The payload is evaluated against the frame the
reduction actually reaches, where every slot carries its runtime value
and type. Three consequences:

- **No `HA_has_type` slot guards and no `HA_ex` binders for `@`** in a
  reachability payload: the frame supplies the typing, and the predicate
  already quantifies the choices operationally — an `HA_ex` binder would
  double-quantify and detach the payload from the frame. (`HA_ex` still
  appears for short-circuit `&&`/`||` witnesses, whose machinery is
  mode-independent.)
- **Slot indices are load-bearing**: a payload `T_local i` must equal
  the actual frame index of the compiled function — codegen and the
  obligation translator consume one shared pre-scan plan keyed by
  expression identity, so the payload slot of the k-th `@` equals its
  appended parameter index by construction, not by parallel counting.
- **Theorem selection is either/or**: an `exists`/`unique` payload is
  never emitted under `ValidSpec` (a purely logical encoding of
  reachability is tautological — nothing ties it to execution), and a
  `forall` payload is never wrapped in a `reachability_spec`. The
  `_specs : list hassert` grammar stays unconditional per spec name;
  the reachability partitions appear only when non-empty.

### The source-visible face of `unique`

`reach_visible_locs` is the producer's declaration of which frame slots
count as *source-visible* when `ValidUniqueSpec` compares exits. The
compiler's rule, which a spec author needs to know because it decides
what "exactly one exiting state" distinguishes:

- every **entry parameter** is visible;
- a **named choice** (`let x: i32 = @;`) is visible — hiding it would
  collapse distinct named outcomes into one observation and quietly
  degrade `unique` toward `exists`;
- an **anonymous choice** (`f(@)`) and every compiler temporary are
  hidden; a `let` bound to a pure expression occupies no payload slot at
  all (its value is a function of the visible slots).

For `exists` the list is inert — projecting locals cannot change whether
the observation set is non-empty — so its consequences land on `unique`
alone. That is also why an anonymous `@` is *rejected* in a `unique`
body (`P012`: a choice nothing names cannot distinguish exit states —
bind it first so it participates in uniqueness) while remaining legal in
an `exists` body.

### What `unique` compares

`ValidUniqueSpec` compares, for each entry state, every non-trapping
exit's *observation*: the *whole* linear store, the module instance, the
result values (none — bodies are void), and the locals projected through
`reach_visible_locs`. Several succeeding choice vectors are permitted
when they converge to one observation. Two practical consequences:

- "the whole linear store" includes shadow-stack residue left by calls
  into memory-using functions, which the producer cannot project out —
  so a `unique` obligation over a body that calls a memory-using
  function is practically unprovable today. The verifier owns the
  observation; narrowing it is a verifier-side follow-up.
- entry-state quantification means *every argument vector at one fixed
  instantiation store* (the freshly instantiated module's store), not
  "for every store".

One more scope note: the language specification currently defines
`unique` only as a block form nested inside `exists`; a top-level
`unique`-quantified *body* (`fn f(...) unique { … }`) is this compiler's
deliberate extension, ahead of the pending specification amendment.
Nested `unique` *blocks* remain rejected (`P002`).

### Reachability bodies are void-only and `return`-free

Contract-forced, not a style choice: the reachability judgment reduces
the retained body *frameless* — `to_e_list (modfunc_body f)` directly,
with no enclosing activation frame — and WasmCert's `rs_return` rule
fires only under an `AI_frame`, so a `BI_return` in a retained body can
never take a step and the obligation is silently unprovable. A body must
exit by falling off its end. Analysis already closes both doors (A005
bans `return` inside quantified blocks; A007 rejects a declared return
type whose paths don't all return), and the codegen pre-scan carries its
own hard error for both clauses so an analysis-skipping pipeline cannot
emit an unprovable obligation.

### Entry parameters are universally quantified — filters cannot carve out entries

`exists_spec_holds_at`/`unique_spec_holds_at` fix an *arbitrary* typed
entry vector and only then let the choices range: the obligation demands
a successful choice for **every** entry value (`unique` demands a
non-empty singleton per entry). An `assume` over an entry parameter
alone therefore never restricts the theorem to "nice" entries — at every
entry it rejects, all choices trap, the observation set is empty, and
the whole theorem is **false**, not narrowed. A spec author must write
`exists`/`unique` bodies whose filters stay satisfiable at every typed
entry (e.g. `n >= lo` admits the witness `n := lo` everywhere, where a
strict `n > lo` has no witness at `lo = i32::MAX`; a claim `f(@) >= lo`
over an even-valued `f` fails at `lo = i32::MAX` even without filters,
where `f(@) == 0` does not). Both corpus fixtures were redesigned to
satisfy this after their original obligations were formally refuted at
boundary entries — and both corrected obligations (`ValidExistsSpec`
and `ValidUniqueSpec`) were then discharged end to end against the
real verifier with `Qed`, which is what fixed the constraint's exact
shape.

### Obligations only an import-free module can discharge

`ValidExistsSpec`/`ValidUniqueSpec` existentially quantify a module
*runtime* whose construction requires typing and allocating the module
with an **empty import list**. `ValidSpec` needs no runtime, so this
constraint is new with reachability: a module that still carries
imports and one `exists`/`unique` obligation gets a well-formed but
undischargeable theorem. The linked pipeline's always-link invariant
removes imports before `-v` runs, so the normal path is unaffected; it
bites only direct translation of pre-link or third-party modules.

### The name section is load-bearing for reachability

An `exists`/`unique` obligation must name its retained function, and
obligations reference functions **symbolically** (name-section strings).
`classify_reachability_targets` (`src/translator.rs`) resolves each
reachability entry's own `fn_symbol` through the same shared
name-section inversion `T_app` resolution uses — stripping the
`<folded_spec>.` qualifier the symbol carries (the name section stores
the bare function name; spec membership travels in
`inference.spec_funcs`) and disambiguating through the spec's own index
list — and fails closed (`HspecInconsistent`) when the module carries no
name section, no defined function carries the name, none of the carriers
is listed under the obligation's spec, or several are. It also
cross-checks the wire metadata against the located function:
`entry_arity` must not exceed the function's parameter count, and every
`visible_locs` slot must fall inside the frame (parameters + declared
locals) — a bad record here would otherwise surface only as an
unprovable theorem at the paid prover.

This is a hard dependency a forall-only module does not have: a module
whose obligations apply no symbols translates without any name section,
but a stripped or rewritten name section turns a reachability-bearing
module into a clean `HspecInconsistent` error.

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

### `inference.hspecs` (wire v2)

Per-spec `hassert` obligations, each tagged with its quantifier kind,
owned by the `inference-hassert` crate (`core/hassert/src/codec.rs`) so
the encoder (codegen) and both decoders (linker, `wasm-to-v`) share one
implementation. LEB128 throughout; `varu32` = unsigned LEB128:

```text
version      varu32 = 2
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
      kind       u8               -- 0x00 Forall | 0x01 Exists | 0x02 Unique
      reach_meta                  -- present iff kind != 0x00:
        entry_arity varu32
        locs_count  varu32
        loc         varu32 * locs_count
                                  -- STRICTLY ASCENDING and unique
      hassert                     -- preorder, tag-prefixed
```

The kind byte follows the Rust `SpecKind` enum's declaration order; a
`Forall` entry carries no reachability metadata, so the universal common
case costs one byte. `entry_arity` and `visible_locs` are *carried* on
the wire rather than re-derived from the module's bytes — the producer
alone knows which parameters are choices — and the emitter cross-checks
them against the located function (see
[The name section is load-bearing](#the-name-section-is-load-bearing-for-reachability)),
so producer drift is a loud `HspecInconsistent` instead of an unprovable
theorem.

Both a spec entry's own `fn_symbol` and any `App`/`AppOk` symbol inside
its tree are indices into one shared, sorted symbol table — the union of
every function symbol referenced anywhere in the map. Full tag tables for
the `hassert`/`term`/`HConst`/binop/relop encodings are documented in the
codec's own module doc (`core/hassert/src/codec.rs`); the tag values
follow each Rust enum's declaration order and are part of the wire
format.

Version 1 (no kind byte, no reachability metadata) is superseded and
**rejected on decode**: the section is proof-mode intermediate data, so
recompilation, not migration, is the compatibility story.

**Determinism**: `encode` sorts the symbol table and the spec list, so
two `HSpecMap`s that are equal (regardless of insertion or map-iteration
order) encode to identical bytes.

**Decoder hardening**: both the leading `version` (rejecting anything but
`2`), and, specific to `inference.hspecs`: a 1024-byte sanity cap on any
symbol or spec-name key (`MAX_NAME_LEN` — deliberately larger than
`inference.spec_funcs`' 255-byte spec-name cap, because an hspecs function
symbol combines a spec name with function identifiers and so is a longer
kind of string), a 256-level cap on assertion/term nesting
(`MAX_TREE_DEPTH`, matching `wasm-to-v`'s unrelated
`MAX_EXPRESSION_DEPTH` WASM-body-nesting cap only by coincidence of
value), strict-ascending-and-thus-unique ordering on both the symbol
table and the spec list, a kind-tag range check plus
strict-ascending-and-thus-unique `visible_locs` capped in count and
value (`MAX_VISIBLE_LOCS`, 65 536), bounds-checked counts (an advertised
count is rejected before any allocation it would drive), and full
UTF-8/trailing-byte validation. Because [`encode`] is infallible but [`decode`] enforces the
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
- **P001–P015** (fatal, `core/wasm-codegen/src/hassert/diag.rs`): a
  specification function that cannot be encoded as an obligation — or
  whose obligation says nothing — aborts code generation
  (`CodegenError::UntranslatableSpec`) rather than silently emitting an
  unverifiable module. Every diagnostic is collected before failing, so
  several mistakes in one spec surface together.

  | Code | Condition |
  | --- | --- |
  | P001 | Body is `assume`-quantified. `assume` is not a quantifier — it only reinterprets a failing path as a filtered-out one for an enclosing `forall` — so a standalone `assume` body states no property. (`forall`/plain bodies translate to `ValidSpec` obligations; `exists`/`unique` bodies to reachability obligations; nested `exists`/`assume` blocks are supported in every translatable body) |
  | P002 | A construct with no assertion encoding: `loop`, `break`, a nested `unique` *block*, `**`, a string literal, an array/struct literal read in scalar term position or written at a shape outside the representable surface, or an access chain the element encoding cannot pin — one carrying more than one non-constant index, or one whose non-constant index lands on an aggregate rather than a scalar leaf. Four of these carry their own message rather than the shared no-encoding template, because the template's "this has no encoding; move the logic into a helper" is false or useless for them: `loop` (a loop's purpose in a specification is exactly what quantifying an index and constraining it says directly), an out-of-surface literal (literals encode now — the restriction is the shape, and the helper remedy dead-ends because a compound call result is P005 and a compound call argument P004: a `T_app` symbol resolves to the compiled function, whose real signature takes and returns pointers, so leaf-expanding an application would change its arity and detach the symbol from the function it names), and each of the two access-chain cases |
  | P003 | Reassignment (`Stmt::Assign`) in a specification body. A permanent rule, not a pending feature: a specification names values, not storage, so every name stands for one value throughout the claim — which is what lets the translation read a name as the same term wherever it appears. Mutation would need per-branch value versioning across quantifier scopes for no expressive gain over a fresh `let` |
  | P004 | A type with no place in a specification term — `unit`, a string, a function type — or an aggregate outside the representable surface: arrays of scalars at any rank and structs whose fields are scalars or one-dimensional scalar arrays (the executable aggregate `@` surface, bounded by analysis rules A027/A028). Two further wordings live under this code: an aggregate read *whole* where a term is required (an aggregate call argument, most often — its type is nameable, it is just not a term) and any *aggregate* parameter of an `exists`/`unique` body, whose obligation denotes against a real frame in which the parameter is one pointer local. A non-scalar, non-aggregate parameter there is unrepresentable for the first reason instead, and takes the first wording |
  | P005 | A call that cannot be represented as a `T_app`/`HA_app_ok` term: an *unbound* external function (a bound one resolves — see [T_app resolution discipline](#t_app-resolution-discipline)), an instance method, an unresolved target, a non-deterministic-bodied callee, or (in term position specifically) a non-scalar result |
  | P006 | A bare `@` outside a `let` right-hand side or a call-argument position |
  | P007 | A `forall` block nested inside an `exists`/`unique`-quantified body. Inside a `forall`/plain body the same nesting translates: the inner block binds a `Hall` logical variable per `@` and the alternation is emitted as written. A reachability body cannot: there every `@` is a hidden trailing choice parameter the judgment quantifies operationally, and a universal binder over one would need a choice-plan and lowering redesign |
  | P008 | `@` at a compound (array/struct) type outside the representable surface, or at any compound type in an `exists`/`unique` body. In a `forall`/plain body a supported-shape compound `@` quantifies one universal slot per scalar leaf instead of raising this; in a reachability body the message names the quantifier, since the identical declaration translates in a `forall` body — the obligation there is about one actual run, each of whose choices arrives as one scalar parameter, and a compound value lives in linear memory |
  | P009 | A specification *method* that carries a proof obligation the translation cannot deliver — quantified (any kind, `exists`/`unique` included: a method has no obligation channel), or plain but carrying an `assert` at any depth. Never silently dropped, since a method has no free-function fallback path. A plain method that asserts nothing stays a helper and is not reported, a non-deterministic block that claims nothing included |
  | P010 | A specification function whose obligation collapses to the vacuous `HA_true`: an empty or assert-free body, a body that only computes (`return`, pure `let`/`const`), a trailing `assume` (`Imp(p, ⊤) = ⊤`), or an `if` whose branches all vacuate. An obligation any proof discharges without reading the program is indistinguishable from no verification at all, so a computing helper belongs at file scope, where a specification function can still apply it as a `T_app`. Applies to every kind — an `exists`/`unique` body that asserts nothing is P010, not P001 |
  | P011 | A call from any specification body to an `exists`/`unique`-quantified spec function. Such a function is the subject of a reachability judgment about running its own body with its own choices — not a callable predicate — and its compiled form carries hidden trailing choice parameters no call site supplies. State the property directly, or move the shared part into an ordinary function both spec functions can call |
  | P012 | An anonymous (call-argument) `@` in a `unique`-quantified body: a choice nothing names has no source-visible face, so it cannot distinguish exit states — bind it first (`let c: i32 = @;`). Legal in `exists` bodies, where the visible-locals projection is inert |
  | P013 | An aggregate introduction — a compound `@`, a compound parameter, or an array/struct literal — whose scalar leaves would push the specification function past `SPEC_FN_MAX_QUANTIFIED_LEAVES` (64). A quantified leaf brings a binder and a hypothesis: universally that is one assertion-tree level whatever the leaf's declared type, since a narrow leaf's bound is grouped into its hypothesis level rather than added beside it, while existentially a narrow leaf costs two — the binder plus the conjunct its bound rides in, where a full-width one's absorbed ⊤ leaves the binder alone. A literal's leaves still nest one level apiece through a leafwise comparison; the levels accumulate across every introduction in the function, so the budget is a per-function running total rather than a per-introduction cap, and it is checked from the declared type before any leaf is materialized |
  | P014 | A constant-*folded* array index that is out of bounds — `const K: i32 = 5; a[K]`, or `a[1 + 4]`, on `[i32; 3]`. States the same fact analysis rule A037 states for a direct-literal index; A037's pattern requires the literal directly under the access, so a named or computed constant reaches the translator even with analysis on, and the codegen paths that skip analysis make this the only guard for any of the spellings |
  | P015 | A quantified introduction — a parameter, a `let … = @`, an anonymous call-argument `@`, or a leaf of an aggregate one — at an `enum` declared with no variants, in every mode including the reachability mode. The declared type admits no value, so there is nothing for the claim to range over: `HA_false` would discharge every claim over it for the wrong reason and any inhabited bound would be a lie. Analysis rule A009 only *warns* about the declaration, so such an enum compiles and a `@` over it really does reach the translator, where executable code generation's three treatments of it disagree with one another (a draw is left unconstrained since `rem_u 0` would trap; an exported entry's tag guard traps on every call; a memory round-trip constrains nothing) — there is no consistent behaviour for an antecedent to mirror |

- **The reachability pre-scan's no-return rule** (fatal,
  `core/wasm-codegen/src/hassert/reach.rs`): an `exists`/`unique` body
  may neither declare a return type nor contain a `return` statement —
  the downstream judgment reduces the retained body without an enclosing
  activation frame, so a `BI_return` could never take a step (see
  [Two judgments](#two-judgments-validspec-versus-the-reachability-predicates)).
  Analysis rules A005/A007 already reject both shapes; the pre-scan
  carries its own hard error because codegen can run without analysis.

- **Non-det instructions in a retained body** (`translator.rs`,
  `translate_basic_operator`'s `Operator::Forall | Exists | Assume |
  Unique | I32Uzumaki | I64Uzumaki` arm): rejected as
  `WasmToVError::UnsupportedFeature` ("non-deterministic instruction in
  a function body the emitted module retains cannot be represented in
  the vanilla WasmCert proof model"). The bodies the module record keeps
  are executable functions — where A042 bars non-det — and retained
  `exists`/`unique` spec functions, whose reachability lowering is
  vanilla WASM by construction; neither can carry one of these opcodes
  from Inference-compiled code, so this is defense-in-depth against a
  foreign or hand-crafted `.wasm` that reintroduces one.

- **Float, SIMD/vector, and float-naming conversion constructs**
  (`translator.rs`, the grouped operator arms plus
  `translate_value_type`): rejected as
  `WasmToVError::UnsupportedFeature`. The context in "Required Rocq
  context" is the whole vocabulary the emitted `.v` may use, and it
  contains no `T_f32`/`T_f64` and no vector type or vector instruction.
  Emitting any of them would produce a file that fails `coqc` at the
  consumer, so the translator refuses instead:

  | Rejected | Scope |
  |---|---|
  | float instructions | all loads, stores, constants, comparisons, unops, binops |
  | vector instructions | the entire SIMD proposal, relaxed-SIMD included |
  | float-naming conversions | `trunc`, `trunc_sat`, `convert`, `demote`, `promote` and every `reinterpret` — each names a float on one side, and the contract's `cvtop` declares only the two integer-to-integer constructors |
  | `f32`, `f64`, `v128` value types | every position: parameters, results, locals, globals, block result types |
  | unmodeled proposal families | GC, exception handling (modern and legacy), stack switching, tail calls, 128-bit wide arithmetic, typed function references, `memory.discard`, segment-indexed table operations |

  The **integer-to-integer** width conversions are not rejected. They
  emit `BI_cvtop`, whose four arguments are the target number type, the
  `cvtop`, the source number type, and an `option sx`. Only three
  instances are well-typed under the model's `cvtop_valid`, and the
  emitter writes exactly those:

  | WASM | Emitted |
  |---|---|
  | `i32.wrap_i64` | `BI_cvtop T_i32 CVO_wrap T_i64 None` |
  | `i64.extend_i32_s` | `BI_cvtop T_i64 CVO_extend T_i32 (Some SX_S)` |
  | `i64.extend_i32_u` | `BI_cvtop T_i64 CVO_extend T_i32 (Some SX_U)` |

  Sign-extension is **not** a conversion in the contract. The five
  `extendN_s` operators emit `BI_unop`, alongside `clz`/`ctz`/`popcnt`:

  | WASM | Emitted |
  |---|---|
  | `i32.extend8_s` | `BI_unop T_i32 (Unop_extend 8%N)` |
  | `i32.extend16_s` | `BI_unop T_i32 (Unop_extend 16%N)` |
  | `i64.extend8_s` | `BI_unop T_i64 (Unop_extend 8%N)` |
  | `i64.extend16_s` | `BI_unop T_i64 (Unop_extend 16%N)` |
  | `i64.extend32_s` | `BI_unop T_i64 (Unop_extend 32%N)` |

  `Unop_extend`'s argument is the source width in **bits**, not bytes.
  A consumer reading these terms should know that the distinction is
  invisible to type-checking: the model's `unop_type_agree` ignores the
  argument, while its `app_unop` divides by eight, so `Unop_extend 1`
  elaborates and denotes the constant zero. The emitter pins the bit
  convention by byte comparison in `tests/src/rocq_typecheck.rs` and
  `core/wasm-to-v/src/lib.rs`, because no `coqc` gate can.

  Like the non-det rule above, the rejected set is unreachable from
  Inference-compiled code — the language has no floating-point or vector
  types, and its codegen emits no conversion or sign-extension at all
  (it narrows sub-`i32` values with shifts and masks). It is reachable
  only through foreign bytes: the external linking path and the public
  `translate_bytes` API. `core/wasm-linker` refuses the same content in
  external modules, so this is the second of two layers, and on the CLI
  path the linker's diagnostic normally arrives first.

  The `coqc` corpus does carry foreign bytes — a linked
  `wasm32-unknown-unknown` artifact, see [What a linked body
  brings](#what-a-linked-body-brings-that-compiled-code-cannot) — but
  that changes nothing here: an external carrying any of the above is
  refused by the linker before translation, so no corpus module can ever
  exercise the rejected set. These arms stay covered by the translator's
  own unit tests, which feed the bytes directly.

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
   rather than index-valued — and, per spec with `exists`/`unique`
   obligations, `ValidExistsSpec`/`ValidUniqueSpec` over
   `list reachability_spec` (purely additive: a forall-only module's
   output is unchanged).

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

Each specification free function translates to one `hassert` via a
right-folded statement translator with four modes
(`core/wasm-codegen/src/hassert/translate.rs`): universal
(`Mode::Univ`, a `forall`/plain body's own statements), nested universal
(`Mode::UnivLvl`, a `forall` block inside an `exists`/`assume` context),
existential (`Mode::Exist`, statements inside a nested `exists` block), and
reachability (`Mode::Reach`, the whole body of an `exists`/`unique`
function). `Mode::UnivLvl` keeps `Mode::Univ`'s statement semantics — an
`assume` is an antecedent, an `if` a conjunction of guarded implications —
but binds on the logical-variable channel instead of the slot channel: each
`@` takes a `T_lvar` level under a `Hall` wrap, carrying its own
`HA_has_type (T_lvar n)` guard. That is what keeps an alternating claim
alternating; binding a nested `forall`'s `@` to a slot would read as
`∀x ∃m` under `ValidSpec`'s outer universal, silently weakening the
claim the source wrote. `Mode::Reach` reuses `Mode::Exist`'s statement semantics —
an `assume` block is a conjunct, an `if` a strict disjunction of guarded
conjunctions — but binds every `@` to the `T_local` slot of its own
choice parameter (no `HA_ex` binder, no `HA_has_type` guard; see
[Two judgments](#two-judgments-validspec-versus-the-reachability-predicates)).
The table below shows the two payload polarities:

| Source construct | Universal mode | Existential mode |
| --- | --- | --- |
| Parameter / forall-context `@` | `T_local` slot (sequential, never rewound), plus one pending hypothesis for the slot: its `HA_has_type (T_local i) T_i32`/`T_i64` typing guard (64-bit for `i64`/`u64`, i32 for every other scalar), conjoined with the values its declared type admits where that says more than the class — `HA_and (HA_has_type …) (nz (relop LtU … 256))` for a `u8`, the bare guard for `i32`/`u32`/`i64`/`u64`. A variantless enum is `P015` | — |
| Pending slot hypotheses | One channel entry per introduction, whatever its declared type, discharged at the next structural statement: fused into an immediately-following `assume`'s antecedent as `HA_and (hypothesis, …, assume-body)`, otherwise an `Himpl` antecedent over the statement's claim conjoined with the rest of the block; a slot introduced after the last structural statement guards nothing (an unread slot introduced earlier is still guarded — uniformity beats a use analysis). The drain is a right fold, so grouping a narrow slot's bound into its own entry rather than pairing it as a second entry is what keeps the antecedent one level deep per introduction. A pure `let`/`const` is not structural, but one that binds a short-circuit witness drains here too, so the hypothesis dominates the `HA_ex` instead of sitting inside it — the witness constraint reads the very slot the guard types | — (a prover-chosen `HA_ex` variable needs no typing guard; a narrow one still carries its declared bound) |
| Call-argument / `let`-bound `@` | `Mode::Univ` takes a slot as above; under `Mode::UnivLvl` it takes a `Hall` binder instead, with the same hypothesis over its `T_lvar` as that binder's own antecedent. An *anonymous* `@` has no annotation, so both halves come from the type recorded for the argument — the callee's declared parameter type | `HA_ex` binder at the binding point, carrying the declared value domain as a conjunct inside itself (never an antecedent, which a proof could refute to escape the payload) and nothing where the declaration admits the whole class; body uses `T_lvar` at its de Bruijn index, resolved by a final level-to-index pass (no shifting needed while building) |
| Universal binder (`Mode::UnivLvl`) | `Hall` (the derived `HA_not (HA_ex (HA_not _))`, declared in the stub) wrapping the rest of the block; an aggregate `@` binds one level per scalar leaf, each guarded. **A `T_lvar` guard must be drained inside its own `Hall`**: `term_unsafe d (T_lvar n)` holds once the index escapes its binder, and `strictify` then collapses that guard to `HA_false`, deleting the antecedent and leaving a strictly harder obligation that still compiles. Guard placement here is a soundness-of-meaning rule, not a formatting choice | — |
| Aggregate introduction (compound `@`, compound parameter, array/struct literal) | No aggregate term exists: the value becomes a shape-preserving tree of scalar leaves, one slot and one hypothesis per leaf in enumeration order (arrays row-major, struct fields in layout order, recursing). A leaf's hypothesis is the one its own declared *element* or *field* type would state as a scalar, resolved against the referencing file for an array element and against the struct's *defining* file for a field, so a `[u8; 2]` bounds both leaves at `0..255` and a struct bounds each field at its own type. Allocation across a function is parameters in declaration order, then each `@` in binding order. A literal's leaves are constants — no slot, no guard — but still count against the budget, since a leafwise comparison nests one conjunct per leaf whichever side it came from. Capped at 64 leaves per spec function (`P013`) | One `HA_ex` binder per leaf, consecutive levels, the block's translation wrapped in that many binders, innermost binder = last leaf allocated; no typing guard, since a prover-chosen value states none, but each leaf keeps its declared bound as a conjunct inside its own binder wherever the claim reads that leaf |
| Aggregate *copy* (`let b = a;`) | Clones the bound leaf tree — value-copy semantics make the pure inlining exact. No slot, no guard, no binder, and nothing charged to the leaf budget | same |
| `e.f`, `e[k]` with a foldable constant `k` | Resolved at translation time against the leaf tree; the access never appears in the obligation, only the selected leaf's term. A folded out-of-bounds `k` is `P014` | same |
| `e[i]` with a non-constant `i` | A fresh `HA_ex` witness `v` pinned by `HA_and (nz (relop LtU i N)) (⋀_{c<N} Himpl (term_eq i c) (term_eq v (leaf c)))` — the unsigned range bound **first**, then one implication per element. Out of range the definition is unsatisfiable and the enclosing atom is refuted: `a[i]` denotes *the element at index `i`, which exists*. Constant steps of a chain descend first, so `m[1][j]` splits over the selected row. Two `P002`s guard the shape: a chain with two non-constant steps (the split would be their product), and a chain whose non-constant step selects an aggregate rather than a scalar leaf (there is no single term for the cases to define) | same |
| `a == b` / `a != b` at aggregate type, assertion position | Leafwise: the conjunction of per-leaf `term_eq`, or its De Morgan dual for the negation. `==` compares values, and an aggregate's value is exactly its ordered scalar leaves | same |
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

The `==`/`!=` split in that table is deliberate, not an oversight, and a
consumer feels it: an assumption `a[0] != 3` arrives as "the `ne` relop is
nonzero" while a leafwise aggregate conclusion is stated with `term_eq`, so a
proof relating the two pays a bridging step.

The rule behind the split is **positional, not operator-wise**. The existential
column above — which `Mode::Reach` shares — holds the positions that *pin* a
value: an existential
`@`'s witness, and — since an `assume` body translates existentially even inside
a `forall` function — the antecedent of a universal claim. `term_eq` is the
pinning form, so `==` takes it in those positions and the refutable relop in a
claim position; that is why the same operator appears in both columns with
different encodings. `!=` pins nothing — there is no value it names, only a
computation whose result must be nonzero — so it stays with the operator the
program executes, at that operator's own width and signedness. Encoding it as
`¬term_eq` would state a disequality of mathematical values where the program
compares two registers, the same class of divergence the eager `&&`/`||` term
had before it became a pinned witness. The bridging step is the cost of that
fidelity.

Aggregate `==`/`!=` is not an exception to that rule but a different question,
answered by the language rather than by the encoding: `==` at aggregate type
compares *values*, and an aggregate's value is its ordered scalar leaves, so it
is leafwise in every mode. The compiled comparison of frame pointers is the side
that must change; it is tracked as a separate defect.

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
refutable rather than a silence escape. A narrow slot leads with the same
guard conjoined with its declared value domain, since the width alone
would leave the valuation free to pick values the declaration forbids.
This matches the canonical worked example below.

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
- [`core/wasm-codegen/docs/specification-obligations.md`](../wasm-codegen/docs/specification-obligations.md)
  — the same translation read from the specification author's side: what
  each construct becomes, which slot each value gets, and which
  properties are provable as a result. Start there if you are reading an
  emitted obligation because a proof did not close.
- `core/wasm-codegen/src/hassert/` — the specification-to-`hassert`
  translation pass and its `P0xx` diagnostics.
- `core/wasm-to-v/src/translator.rs`, `src/hassert_print.rs` — the
  index remap, `T_app` resolution, and the `hassert` → Gallina printer.
- `tests/src/rocq_typecheck.rs` — the `coqc` type-check gate.

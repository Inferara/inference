(* WasmVerifier.Exists -- vendored signature stub for the Inference
   proof-mode Rocq output contract.

   The reachability half of the obligation surface: an `exists`- or
   `unique`-bodied specification function is retained in the emitted module
   record with a vanilla body, and its obligation is a `reachability_spec`
   record naming that function, its entry arity (the declared parameters,
   ahead of the hidden choice suffix), the producer-declared source-visible
   frame slots, and the payload evaluated against the reached frame. The
   record must be concrete -- a `Parameter` type has no fields for `coqc` to
   elaborate the emitted `{| ... |}` literals against. It deliberately names
   no `Section` variable, mirroring the real library, where the record sits
   inside `Section Host` but uses nothing host-dependent; the two predicates
   are host-generalized exactly like `ValidSpec`.

   Nothing in this file may be *declared* under the bare name of the module
   itself: the emitted preamble import puts that token into every
   reachability-bearing module's text, so such a declaration would count as
   produced without anything referencing it. Signatures only; no semantics,
   no proofs. See README.md. *)

Require Import BinNat.
Require Import List.
Require Import Wasm.datatypes.
Require Import Wasm.host.
Require Import WasmVerifier.Assertions.

Record reachability_spec : Type := {
  reach_func : N;
  reach_entry_arity : nat;
  reach_visible_locs : seq N;
  reach_payload : hassert
}.

Parameter ValidExistsSpec :
  forall `{ho : host}, module -> list reachability_spec -> Prop.
Parameter ValidUniqueSpec :
  forall `{ho : host}, module -> list reachability_spec -> Prop.

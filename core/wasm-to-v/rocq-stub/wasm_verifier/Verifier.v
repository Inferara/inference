(* WasmVerifier.Verifier -- vendored signature stub for the Inference
   proof-mode Rocq output contract.

   The two downstream proof obligations the emitter references, with the arities
   the real wasm-verifier library declares in its `theories/Verifier.v`:

     - `ValidModule : module -> Prop` -- structural well-formedness, 1-ary.
       In the real library its body uses no `Section Host` variable, so post-
       section it is the bare `module -> Prop` declared here.

     - `ValidSpec : forall `{ho : host}, module -> list hassert -> Prop` -- the
       hassert-valued per-spec obligation. In the real library it is defined
       inside `Section Host` and uses host-dependent machinery, so post-section
       it carries an implicit `{ho : host}`; the emitted theorems discharge it
       under their own `Section Host. Context `{ho: host}.`.

   Signatures only; no semantics, no proofs. See README.md. *)

Require Import BinNat.
Require Import List.
Require Import Wasm.datatypes.
Require Import Wasm.host.
Require Import WasmVerifier.Assertions.

Parameter ValidModule : module -> Prop.

Parameter ValidSpec : forall `{ho : host}, module -> list hassert -> Prop.

(* Wasm.verifier -- vendored signature stub. Signatures only; no semantics,
   no proofs. See README.md. *)

Require Import BinNat.
Require Import List.
Require Import Wasm.datatypes.

(* Emitted theorems are stated under `Section Host. Context `{ho: host}.`.
   An empty class reproduces that binder shape without modelling any host
   operations. *)
Class host : Type := { }.

(* The proof obligation the emitter references. Its arity is the CONTRACT AS
   THE EMITTER WRITES IT: `wasm-to-v` currently emits per-spec theorems of the
   form `ValidModule <mod> <mod>__<Spec>_specs`, i.e. TWO arguments (a module
   and a `list N` of spec function indices). Note this diverges from the prose
   in `core/wasm-to-v/ROCQ_CONTRACT.md`, which describes a post-#21 split into a
   one-argument `ValidModule : module -> Prop` plus a separate
   `ValidSpec : module -> list N -> Prop`. The stub matches the emitter (so
   current `main` type-checks); reconciling the emitter with that prose -- and
   with the real verifier library -- is tracked as an emitter/library concern.
   See README.md. *)
Parameter ValidModule : module -> list N -> Prop.

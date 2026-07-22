(* WasmVerifier.Assertions -- vendored signature stub for the Inference
   proof-mode Rocq output contract.

   The `term`/`hassert` assertion language and its derived sugar, mirrored
   verbatim from the wasm-verifier library (theories/Assertions.v:21-88) so the
   emitted per-spec obligations type-check against the same constructors the
   real prover uses. Signatures only; no denotation, no proofs. See README.md
   for scope, drift risk, and the swap-to-real-library follow-up. *)

Require Import BinNat.
Require Import List.
Require Import Wasm.datatypes.

(* The real library spells term/hassert sequences with mathcomp's `seq`, which
   is notation for `list`. The emitted `.v` imports no mathcomp and spells the
   payload `list hassert`; a `Notation` keeps the inductive fields readable while
   staying definitionally the standard-library `list`. *)
Notation seq := list.

(* Terms  tau ::= c | nu | f(tau...).  Logical variables use de Bruijn indices;
   [T_local]/[T_global] are the WASM local/global variables. *)
Unset Elimination Schemes.
Inductive term : Type :=
| T_const  : value_num -> term
| T_lvar   : nat -> term
| T_local  : N -> term
| T_global : N -> term
| T_app    : nat -> seq term -> term
| T_binop  : number_type -> binop -> term -> term -> term
| T_relop  : number_type -> relop -> term -> term -> term.
Set Elimination Schemes.

(* Heap assertions H.  [HA_ex] binds logical de Bruijn index 0 in its body. *)
Unset Elimination Schemes.
Inductive hassert : Type :=
| HA_false : hassert
| HA_true  : hassert
| HA_not   : hassert -> hassert
| HA_and   : hassert -> hassert -> hassert
| HA_ex    : hassert -> hassert
| HA_pred  : nat -> seq term -> hassert
| HA_emp   : hassert
| HA_star  : hassert -> hassert -> hassert
| HA_iter  : term -> term -> hassert -> hassert
| HA_pto   : term -> term -> hassert
| HA_size  : term -> hassert
| HA_has_type : term -> number_type -> hassert
| HA_defined  : term -> hassert
| HA_app_ok   : nat -> seq term -> hassert.
Set Elimination Schemes.

(* The distinguished predicate index used for term equality. *)
Definition pred_eq : nat := 0.

(* Term-equality assertion  tau1 = tau2. *)
Definition term_eq (a b : term) : hassert := HA_pred pred_eq (a :: b :: nil).

(* [hassert] has no primitive implication / disjunction / universal-quantifier
   constructors; these are the standard classical De Morgan encodings, mirrored
   as definitionally-transparent Definitions so the emitter can print them by
   name. *)
Definition Himpl (p q : hassert) : hassert := HA_not (HA_and p (HA_not q)).
Definition Hor (p q : hassert) : hassert := HA_not (HA_and (HA_not p) (HA_not q)).
Definition Hall (body : hassert) : hassert := HA_not (HA_ex (HA_not body)).

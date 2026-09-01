(* WasmVerifier.Assertions -- vendored signature stub for the Inference
   proof-mode Rocq output contract.

   The *emittable subset* of the wasm-verifier library's `term`/`hassert`
   assertion language and its derived sugar, so the emitted per-spec
   obligations type-check against the same constructors the real prover
   uses. A declaration the emitter physically cannot print is deliberately
   left out, so emitting one anyway becomes an "unbound constructor" error
   here instead of a silently type-checking term; README.md tabulates every
   omission and why it is unreachable.

   The real library spells its term and hassert sequences with mathcomp's
   `seq`, which is notation for `list`; this mirror spells `list` directly. A
   `Notation seq := list.` here would look like the closer mirror and be the
   worse one: in the context an emitted module's preamble builds, `seq`
   resolves to `Coq.Lists.List.seq : nat -> nat -> list nat`, so the local
   notation would silently shadow a different name than the one the real
   library binds. The emitted `.v` imports no mathcomp and writes
   `list hassert`, which is what `seq hassert` denotes there.

   Signatures only; no denotation, no proofs. See README.md for scope, drift
   risk, and the swap-to-real-library follow-up. *)

Require Import BinNat.
Require Import List.
Require Import Wasm.datatypes.

(* Terms  tau ::= c | nu | f(tau...).  Logical variables use de Bruijn indices;
   [T_local] is the WASM local variable a quantifier slot lowers to. The real
   library also carries [T_global]; an Inference specification cannot reference
   a global, so no such term is declared here. *)
Unset Elimination Schemes.
Inductive term : Type :=
| T_const  : value_num -> term
| T_lvar   : nat -> term
| T_local  : N -> term
| T_app    : nat -> list term -> term
| T_binop  : number_type -> binop -> term -> term -> term
| T_relop  : number_type -> relop -> term -> term -> term.
Set Elimination Schemes.

(* Heap assertions H.  [HA_ex] binds logical de Bruijn index 0 in its body.
   The real library's heap fragment ([HA_emp], [HA_star], [HA_iter], [HA_pto],
   [HA_size]) is not declared: an Inference specification that touches memory
   is rejected before any assertion is built, so no obligation can name one. *)
Unset Elimination Schemes.
Inductive hassert : Type :=
| HA_false : hassert
| HA_true  : hassert
| HA_not   : hassert -> hassert
| HA_and   : hassert -> hassert -> hassert
| HA_ex    : hassert -> hassert
| HA_pred  : nat -> list term -> hassert
| HA_has_type : term -> number_type -> hassert
| HA_defined  : term -> hassert
| HA_app_ok   : nat -> list term -> hassert.
Set Elimination Schemes.

(* The distinguished predicate index used for term equality. Neither [HA_pred]
   nor [pred_eq] is ever printed by name -- [term_eq] is the only predicate form
   the emitter produces -- but both are what [term_eq] unfolds to, so dropping
   either would stop this file itself from compiling. *)
Definition pred_eq : nat := 0.

(* Term-equality assertion  tau1 = tau2. *)
Definition term_eq (a b : term) : hassert := HA_pred pred_eq (a :: b :: nil).

(* [hassert] has no primitive implication, disjunction or universal-quantifier
   constructor; these are the standard classical De Morgan encodings, mirrored
   as definitionally-transparent Definitions so the emitter can print them by
   name. [Hall]'s body binds logical de Bruijn index 0, exactly like the
   [HA_ex] it is built from. *)
Definition Himpl (p q : hassert) : hassert := HA_not (HA_and p (HA_not q)).
Definition Hor (p q : hassert) : hassert := HA_not (HA_and (HA_not p) (HA_not q)).
Definition Hall (body : hassert) : hassert := HA_not (HA_ex (HA_not body)).

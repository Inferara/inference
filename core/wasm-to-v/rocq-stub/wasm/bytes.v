(* Wasm.bytes -- vendored signature stub for the Inference proof-mode Rocq
   output contract. Signatures only; no semantics. See README.md for purpose,
   drift risk, and the swap-to-real-library follow-up. *)

Require Import String.
Require Import List.
Require Import ZArith.

(* The emitted `Mi`/`Me` helpers store import/export module and field names as a
   `list byte` via `list_byte_of_string`. We declare our own opaque `byte`
   rather than reuse the standard library's `Byte.byte`, whose module path was
   renamed across the Coq -> Rocq transition (`Coq.*` vs `Stdlib.*`); an opaque
   parameter keeps the stub compiling on both apt Coq 8.x and brew Rocq 9.x. *)
Parameter byte : Type.

Parameter list_byte_of_string : string -> list byte.

(* Data-segment contents reach the `.v` as byte literals, each one an
   application of `encode`. Both names mirror the interface of wasm-verifier's
   `coq-wasm` dependency, where `byte` is CompCert's `Integers.byte` and
   `encode` builds one from a `Z` via `Byte.repr`. Here `encode` stays an
   opaque parameter of the opaque `byte` above: the spelling and the byte
   result type are the contract an emitted `moddata_init` has to meet, not the
   arithmetic behind the value, so the stub type-checks the emitted shape
   without modelling byte values.

   The dependency also abbreviates 244 of the 256 values with two-digit
   uppercase hex notations, in a `byte_scope` it opens at module level. This
   mirror declares neither the scope nor the notations, because no emitted
   module can spell one: each notation expands to arithmetic over the
   dependency's single hex-digit notations, which stand for bare numerals, so
   every value whose spelling carries a digit `A` .. `F` elaborates at `nat`
   and fails against `encode`. Declaring them here would be worse than leaving
   them out -- this `encode` is opaque, so the notations would type-check
   spellings the backend rejects -- and with nothing emitting one they would be
   declarations `coqc` never elaborates, free to drift out of agreement with
   the dependency while every gate stayed green. *)

Parameter encode : Z -> byte.

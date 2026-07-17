(* Wasm.bytes -- vendored signature stub for the Inference proof-mode Rocq
   output contract. Signatures only; no semantics. See README.md for purpose,
   drift risk, and the swap-to-real-library follow-up. *)

Require Import String.
Require Import List.

(* The emitted `Mi`/`Me` helpers store import/export module and field names as a
   `list byte` via `list_byte_of_string`. We declare our own opaque `byte`
   rather than reuse the standard library's `Byte.byte`, whose module path was
   renamed across the Coq -> Rocq transition (`Coq.*` vs `Stdlib.*`); an opaque
   parameter keeps the stub compiling on both apt Coq 8.x and brew Rocq 9.x. *)
Parameter byte : Type.

Parameter list_byte_of_string : string -> list byte.

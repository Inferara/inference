(* Wasm.numerics -- vendored signature stub. Signatures only; no semantics.
   See README.md. *)

Require Import ZArith.

(* Machine-integer / float representation types. Opaque: the stub type-checks
   the *shape* of emitted terms, it does not model wrap-around arithmetic. *)
Parameter i32 : Type.
Parameter i64 : Type.
Parameter f32 : Type.
Parameter f64 : Type.

(* The emitted `Vi32`/`Vi64` helpers read
   `VAL_int32 (Wasm_int.int_of_Z i32m z)`. `int_of_Z` takes an integer-type
   witness and a `Z` and yields a value of that integer type; `i32m`/`i64m`
   are the witnesses for the 32-/64-bit machines. Keeping them definitionally
   equal to `i32`/`i64` makes `int_of_Z i32m z : i32`, which is exactly the
   argument type `VAL_int32` expects. *)
Module Wasm_int.
  Parameter int_of_Z : forall (m : Type), Z -> m.
End Wasm_int.

Definition i32m : Type := i32.
Definition i64m : Type := i64.

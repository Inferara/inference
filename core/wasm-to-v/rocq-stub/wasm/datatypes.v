(* Wasm.datatypes -- vendored signature stub for the Inference proof-mode Rocq
   output contract. Every declaration fixes the *arity and shape* that the
   `wasm-to-v` emitter writes, so an emitter regression (a mis-aritied or
   renamed constructor -- the #230 `BI_forall`/`BI_exists` bug class) becomes a
   `coqc` type error against this file. Signatures only; no semantics, no
   proofs. See README.md for scope, drift risk, and the swap-to-real-library
   follow-up. *)

Require Import BinNat.
Require Import List.
Require Import Wasm.bytes.
Require Import Wasm.numerics.

(* ------------------------------------------------------------------ *)
(* Value, number and reference types                                  *)
(*                                                                    *)
(* Deliberately narrower than the real library: no float (`T_f32`/    *)
(* `T_f64`) and no vector (`T_v128`) constructors, because Inference  *)
(* has no floating-point or SIMD and no reachable emitter output can  *)
(* mention them. Their absence makes the gate stricter -- an          *)
(* accidental float/vector emission is a type error here, not a       *)
(* silently type-checking term. See README.md "Scope".                *)
(* ------------------------------------------------------------------ *)

Inductive number_type : Type :=
| T_i32 : number_type
| T_i64 : number_type.

Inductive reference_type : Type :=
| T_funcref : reference_type
| T_externref : reference_type.

Inductive value_type : Type :=
| T_num : number_type -> value_type
| T_ref : reference_type -> value_type.

Inductive packed_type : Type :=
| Tp_i8 : packed_type
| Tp_i16 : packed_type
| Tp_i32 : packed_type.

Inductive sx : Type :=
| SX_S : sx
| SX_U : sx.

Inductive mutability : Type :=
| MUT_const : mutability
| MUT_var : mutability.

Inductive value_num : Type :=
| VAL_int32 : i32 -> value_num
| VAL_int64 : i64 -> value_num.

Inductive block_type : Type :=
| BT_id : N -> block_type
| BT_valtype : option value_type -> block_type.

(* ------------------------------------------------------------------ *)
(* Operator families                                                  *)
(*                                                                    *)
(* Integer families only. The float families (`relop_f`/`binop_f`/    *)
(* `unop_f`) and the conversion family (`cvtop`) are deliberately     *)
(* absent: Inference has no floating-point and its codegen emits no   *)
(* conversion instructions, so no reachable emitter output uses them. *)
(* The single-constructor wrappers (`Relop_i` etc.) stay because the  *)
(* emitter writes the wrapped form. See README.md "Scope".            *)
(* ------------------------------------------------------------------ *)

Inductive testop : Type :=
| TO_eqz : testop.

Inductive relop_i : Type :=
| ROI_eq : relop_i
| ROI_ne : relop_i
| ROI_lt : sx -> relop_i
| ROI_gt : sx -> relop_i
| ROI_le : sx -> relop_i
| ROI_ge : sx -> relop_i.

Inductive relop : Type :=
| Relop_i : relop_i -> relop.

Inductive binop_i : Type :=
| BOI_add : binop_i
| BOI_sub : binop_i
| BOI_mul : binop_i
| BOI_div : sx -> binop_i
| BOI_rem : sx -> binop_i
| BOI_and : binop_i
| BOI_or : binop_i
| BOI_xor : binop_i
| BOI_shl : binop_i
| BOI_shr : sx -> binop_i
| BOI_rotl : binop_i
| BOI_rotr : binop_i.

Inductive binop : Type :=
| Binop_i : binop_i -> binop.

Inductive unop_i : Type :=
| UOI_clz : unop_i
| UOI_ctz : unop_i
| UOI_popcnt : unop_i.

Inductive unop : Type :=
| Unop_i : unop_i -> unop.

(* ------------------------------------------------------------------ *)
(* Memory argument                                                    *)
(* ------------------------------------------------------------------ *)

Record memarg : Type := {
  memarg_offset : N;
  memarg_align : N
}.

(* ------------------------------------------------------------------ *)
(* Instructions                                                       *)
(*                                                                    *)
(* Arities below are the contract the emitter writes. The fork-only    *)
(* non-deterministic constructors `BI_forall`/`BI_exists`/`BI_assume`/ *)
(* `BI_unique`/`BI_uzumaki_num` are DELIBERATELY ABSENT: the emitter    *)
(* omits `spec` functions from the module record, and rejects any      *)
(* non-deterministic instruction reaching a surviving (executable)     *)
(* body, so no reachable emitter output mentions them. Their absence is *)
(* itself the regression guard — should a non-det instruction ever leak *)
(* into the module record again, it becomes an "unbound constructor"    *)
(* `coqc` error here rather than a silently type-checking term.         *)
(* ------------------------------------------------------------------ *)

(* This stub never performs induction over `basic_instruction`; suppressing the
   auto-generated recursion schemes silences the "nested using list" scheme
   warning that its `list basic_instruction` fields would otherwise emit once
   per constructor. `Unset/Set Elimination Schemes` is portable to both apt Coq
   8.x and brew Rocq 9.x. *)
Unset Elimination Schemes.
Inductive basic_instruction : Type :=
| BI_nop : basic_instruction
| BI_unreachable : basic_instruction
| BI_drop : basic_instruction
| BI_return : basic_instruction
| BI_ref_is_null : basic_instruction
| BI_memory_size : basic_instruction
| BI_memory_grow : basic_instruction
| BI_memory_copy : basic_instruction
| BI_memory_fill : basic_instruction
| BI_select : option (list value_type) -> basic_instruction
| BI_const_num : value_num -> basic_instruction
| BI_block : block_type -> list basic_instruction -> basic_instruction
| BI_loop : block_type -> list basic_instruction -> basic_instruction
| BI_if : block_type -> list basic_instruction -> list basic_instruction -> basic_instruction
| BI_br : N -> basic_instruction
| BI_br_if : N -> basic_instruction
| BI_br_table : list N -> basic_instruction
| BI_call : N -> basic_instruction
| BI_call_indirect : N -> N -> basic_instruction
| BI_ref_func : N -> basic_instruction
| BI_local_get : N -> basic_instruction
| BI_local_set : N -> basic_instruction
| BI_local_tee : N -> basic_instruction
| BI_global_get : N -> basic_instruction
| BI_global_set : N -> basic_instruction
| BI_table_get : N -> basic_instruction
| BI_table_set : N -> basic_instruction
| BI_table_fill : N -> basic_instruction
| BI_table_grow : N -> basic_instruction
| BI_table_size : N -> basic_instruction
| BI_memory_init : N -> basic_instruction
| BI_data_drop : N -> basic_instruction
| BI_load : number_type -> option (packed_type * sx) -> memarg -> basic_instruction
| BI_store : number_type -> option packed_type -> memarg -> basic_instruction
| BI_testop : number_type -> testop -> basic_instruction
| BI_relop : number_type -> relop -> basic_instruction
| BI_binop : number_type -> binop -> basic_instruction
| BI_unop : number_type -> unop -> basic_instruction.
Set Elimination Schemes.

(* ------------------------------------------------------------------ *)
(* Function types                                                     *)
(* ------------------------------------------------------------------ *)

Inductive function_type : Type :=
| Tf : list value_type -> list value_type -> function_type.

(* ------------------------------------------------------------------ *)
(* Section-record shapes                                              *)
(* ------------------------------------------------------------------ *)

Record limits : Type := {
  lim_min : N;
  lim_max : option N
}.

Record table_type : Type := {
  tt_limits : limits;
  tt_elem_type : reference_type
}.

Record global_type : Type := {
  tg_mut : mutability;
  tg_t : value_type
}.

Inductive module_import_desc : Type :=
| MID_func : N -> module_import_desc
| MID_table : table_type -> module_import_desc
| MID_mem : limits -> module_import_desc
| MID_global : global_type -> module_import_desc.

Record module_import : Type := {
  imp_module : list byte;
  imp_name : list byte;
  imp_desc : module_import_desc
}.

Inductive module_export_desc : Type :=
| MED_func : N -> module_export_desc
| MED_table : N -> module_export_desc
| MED_mem : N -> module_export_desc
| MED_global : N -> module_export_desc.

Record module_export : Type := {
  modexp_name : list byte;
  modexp_desc : module_export_desc
}.

Record module_table : Type := {
  modtab_type : table_type
}.

Record module_mem : Type := {
  modmem_type : limits
}.

Record module_glob : Type := {
  modglob_type : global_type;
  modglob_init : list basic_instruction
}.

Inductive module_elemmode : Type :=
| ME_passive : module_elemmode
| ME_declared : module_elemmode
| ME_active : N -> list basic_instruction -> module_elemmode
| ME_functions : list N -> module_elemmode.

Record module_element : Type := {
  modelem_type : reference_type;
  modelem_init : list (list basic_instruction);
  modelem_mode : module_elemmode
}.

Inductive module_datamode : Type :=
| MD_passive : module_datamode
| MD_active : N -> list basic_instruction -> module_datamode.

Record module_data : Type := {
  moddata_init : list byte;
  moddata_mode : module_datamode
}.

Record module_start : Type := {
  modstart_func : N
}.

Record module_func : Type := {
  modfunc_type : N;
  modfunc_locals : list value_type;
  modfunc_body : list basic_instruction
}.

Record module : Type := {
  mod_types : list function_type;
  mod_funcs : list module_func;
  mod_tables : list module_table;
  mod_mems : list module_mem;
  mod_globals : list module_glob;
  mod_elems : list module_element;
  mod_datas : list module_data;
  mod_start : option module_start;
  mod_imports : list module_import;
  mod_exports : list module_export
}.

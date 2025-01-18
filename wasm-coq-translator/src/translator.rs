/*
(** Definition of Wasm datatypes
    See https://webassembly.github.io/spec/core/syntax/index.html
    and https://webassembly.github.io/spec/core/exec/index.html **)
(* (C) J. Pichon, M. Bodin - see LICENSE.txt *)

From Wasm Require array.
From Wasm Require Import common memory memory_list.
From Wasm Require Export numerics bytes.
From mathcomp Require Import ssreflect ssrfun ssrnat ssrbool eqtype seq.
From compcert Require common.Memdata.
Require Import BinNat.

Set Implicit Arguments.
Unset Strict Implicit.
Unset Printing Implicit Defensive.


(** * Basic Datatypes **)

(* TODO: use a more faithful definition of u32. *)

(** std-doc:
Definitions are referenced with zero-based indices. Each class of definition has its own index space, as distinguished by the following classes.

The index space for functions, tables, memories and globals includes respective imports declared in the same module. The indices of these imports precede the indices of other definitions in the same index space.

Element indices reference element segments and data indices reference data segments.

The index space for locals is only accessible inside a function and includes the parameters of that function, which precede the local variables.

Label indices reference structured control instructions inside an instruction sequence.

[https://www.w3.org/TR/wasm-core-2/syntax/modules.html#indices]
*)
Definition u32 : Set := N.
Definition u8: Set := N.

(* 2^32 *)
Definition u32_bound : N := 4294967296%N.

Definition typeidx : Set := u32.
Definition funcidx : Set := u32.
Definition tableidx : Set := u32.
Definition memidx : Set := u32.
Definition globalidx : Set := u32.
Definition elemidx : Set := u32.
Definition dataidx : Set := u32.
Definition localidx : Set := u32.
Definition labelidx : Set := u32.

(** std-doc:
Function instances, table instances, memory instances, and global instances, element instances,
and data instances in the store are referenced with abstract addresses. These are simply indices
into the respective store component. In addition, an embedder may supply an uninterpreted set of
host addresses.

An embedder may assign identity to exported store objects corresponding to their addresses, even
where this identity is not observable from within WebAssembly code itself (such as for function
instances or immutable globals).

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#addresses]
*)

Definition addr := N.

Definition funcaddr : Set := addr.
Definition tableaddr : Set := addr.
Definition memaddr : Set := addr.
Definition globaladdr : Set := addr.
Definition elemaddr : Set := addr.
Definition dataaddr : Set := addr.
Definition externaddr : Set := addr.

(** std-doc:
WebAssembly programs operate on primitive numeric values. Moreover, in the definition of programs, immutable sequences of values occur to represent more complex data, such as text strings or other vectors.
*)

Inductive value_num : Type :=
  | VAL_int32 : i32 -> value_num
  | VAL_int64 : i64 -> value_num
  | VAL_float32 : f32 -> value_num
  | VAL_float64 : f64 -> value_num
.

(* We are not implementing SIMD at the moment. *)
Inductive value_vec : Set :=
  | VAL_vec128: unit -> value_vec
.

(* TODO: Unicode support? *)
Definition name := list Byte.byte.

Section Types.

(** std-doc:
Number types classify numeric values.

The types i32 and i64 classify 32 and 64 bit integers, respectively. Integers are not inherently signed or unsigned, their interpretation is determined by individual operations.

The types f32 and f64 classify 32 and 64 bit floating-point data, respectively. They correspond to the respective binary floating-point representations, also known as single and double precision, as defined by the IEEE 754-2019 standard (Section 3.3).

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#number-types]
*)
Inductive number_type : Set :=
  | T_i32
  | T_i64
  | T_f32
  | T_f64
  .


(** std-doc:

Vector types classify vectors of numeric values processed by vector instructions (also known as SIMD instructions, single instruction multiple data).

The type v128 corresponds to a 128 bit vector of packed integer or floating-point data. The packed data can be interpreted as signed or unsigned integers, single or double precision floating-point values, or a single 128 bit type. The interpretation is determined by individual operations.

Vector types, like number types are transparent, meaning that their bit patterns can be observed. Values of vector type can be stored in memories.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#vector-types]
*)

Inductive vector_type : Set :=
| T_v128
.

(** std-doc:

Reference types classify first-class references to objects in the runtime store.

The type funcref denotes the infinite union of all references to functions, regardless of their function types.

The type externref denotes the infinite union of all references to objects owned by the embedder and that can be passed into WebAssembly under this type.

Reference types are opaque, meaning that neither their size nor their bit pattern can be observed. Values of reference type can be stored in tables.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#reference-types]
*)
Inductive reference_type : Set :=
| T_funcref
| T_externref
.

(** std-doc:

Value types classify the individual values that WebAssembly code can compute with and the values that a variable accepts. They are either number types, vector types, or reference types.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#value-types]
*)
(* Note here that we incorporate the bottom type (indicating that the type is unconstrained) in the operand types as part of the base value type, so that we can later drop the distinction between function types and stack types (and operand type vs value type). This is also a preparation for future extensions such as the funcref/GC proposal. *)
Inductive value_type: Set := (* t *)
| T_num: number_type -> value_type
| T_vec: vector_type -> value_type
| T_ref: reference_type -> value_type
| T_bot: value_type
.

(** std-doc:
Result types classify the result of executing instructions or functions, which is a sequence of values, written with brackets.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#result-types]
*)
Definition result_type : Set :=
  list value_type.

(** std-doc:

Function types classify the signature of functions, mapping a vector of
parameters to a vector of results. They are also used to classify the inputs
and outputs of instructions.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#function-types]
*)
Inductive function_type := (* tf *)
| Tf : result_type -> result_type -> function_type
.

(* This is a definition for future extensions, where instruction types are no longer
the same as function types. *)
Definition instr_type := function_type.

(*
This is technically part of the spec, but the actual definitions never used the bottom case concretely except for the type checking algorithm.
(** std-doc:
Instructions are classified by stack types [t1∗]→[t2∗] that describe how instructions manipulate the operand stack.
 *)
Definition operand_type := option value_type.

Inductive stack_type :=
| Tfs: list operand_type -> list operand_type -> stack_type
.
*)

(** std-doc:
Limits classify the size range of resizeable storage associated with memory types and table types.
If no maximum is given, the respective storage can grow to any size.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#limits]
*)
Record limits : Set := {
  lim_min : u32;
  lim_max : option u32;
}.

(** std-doc:
Limits must have meaningful bounds that are within a given range.
https://www.w3.org/TR/wasm-core-2/valid/types.html#limits
**)
Definition limit_valid_range (lim: limits) (k: N) : bool :=
  (N.leb lim.(lim_min) k) &&
    match lim.(lim_max) with
    | Some lmax => (N.leb lim.(lim_min) lmax) && (N.leb lmax k)
    | None => true
    end.


(** std-doc:
Memory types classify linear memories and their size range.
The limits constrain the minimum and optionally the maximum size of a memory. The limits are given in units of page size.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#memory-types]
*)
Definition memory_type := limits.


(** std-doc:
Table types classify tables over elements of reference types within a size range.

Like memories, tables are constrained by limits for their minimum and
optionally maximum size. The limits are given in numbers of entries.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#table-types]
*)
Record table_type : Set := {
  tt_limits : limits;
  tt_elem_type : reference_type;
}.


(** std-doc:

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#global-types]
*)
Inductive mutability : Set := (* mut *)
  | MUT_const
  | MUT_var
  .


(** std-doc:
Global types classify global variables, which hold a value and can either be mutable or immutable.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#global-types]
*)
Record global_type : Set := (* tg *) {
  tg_mut : mutability;
  tg_t : value_type
}.


(** std-doc:
External types classify imports and external values with their respective types.

[https://www.w3.org/TR/wasm-core-2/syntax/types.html#external-types]
*)

Inductive extern_type : Set :=
| ET_func : function_type -> extern_type
| ET_table : table_type -> extern_type
| ET_mem : memory_type -> extern_type
| ET_global : global_type -> extern_type
.

(** std-doc:
A structured instruction can consume input and produce output on the operand stack according to its annotated block type.
*)
Inductive block_type : Set :=
| BT_id: typeidx -> block_type
| BT_valtype: option value_type -> block_type
.

(** std-doc:
Most types are universally valid. However, restrictions apply to limits, which must be checked during validation. Moreover, block types are converted to plain function types for ease of processing.
**)

Definition functype_valid (ft: function_type) : bool :=
  true.

Definition table_limit_bound : N := (N.sub u32_bound 1%N).

Definition tabletype_valid (tt: table_type) : bool :=
  limit_valid_range tt.(tt_limits) table_limit_bound.

Definition mem_limit_bound : N := 65536%N.

Definition memtype_valid (mt: memory_type) : bool :=
  limit_valid_range mt mem_limit_bound.

Definition globaltype_valid (gt: global_type) : bool :=
  true.

End Types.


Definition static_offset := (* off *) u32.

Definition alignment_exponent := (* a *) u32.

Definition serialise_i32 (i : i32) : bytes :=
  common.Memdata.encode_int 4%nat (numerics.Wasm_int.Int32.unsigned i).

Definition serialise_i64 (i : i64) : bytes :=
  common.Memdata.encode_int 8%nat (numerics.Wasm_int.Int64.unsigned i).

Definition serialise_f32 (f : f32) : bytes :=
  common.Memdata.encode_int 4%nat (Integers.Int.unsigned (numerics.Wasm_float.FloatSize32.to_bits f)).

Definition serialise_f64 (f : f64) : bytes :=
  common.Memdata.encode_int 8%nat (Integers.Int64.unsigned (numerics.Wasm_float.FloatSize64.to_bits f)).

(*
(* TODO: factor this out, following the `memory` branch *)
Module Byte_Index <: array.Index_Sig.
Definition Index := N.
Definition Value := byte.
Definition index_eqb := N.eqb.
End Byte_Index.

Module Byte_array := array.Make Byte_Index.

Record data_vec : Set := {
  dv_length : N;
  dv_array : Byte_array.array;
}.

Record memory : Set := {
  mem_data : memory_list;
  mem_max_opt: option N; (* TODO: should be u32 *)
}.
*)


Section Instructions.

(** * Basic Instructions **)


Inductive sx : Set :=
  | SX_S
  | SX_U
  .

Inductive unop_i : Set :=
  | UOI_clz
  | UOI_ctz
  | UOI_popcnt
  .

Inductive unop_f : Set :=
  | UOF_abs
  | UOF_neg
  | UOF_sqrt
  | UOF_ceil
  | UOF_floor
  | UOF_trunc
  | UOF_nearest
  .

Inductive unop : Set :=
  | Unop_i : unop_i -> unop
  | Unop_f : unop_f -> unop
  | Unop_extend : N -> unop
  .

Inductive binop_i : Set :=
  | BOI_add
  | BOI_sub
  | BOI_mul
  | BOI_div : sx -> binop_i
  | BOI_rem : sx -> binop_i
  | BOI_and
  | BOI_or
  | BOI_xor
  | BOI_shl
  | BOI_shr : sx -> binop_i
  | BOI_rotl
  | BOI_rotr
  .

Inductive binop_f : Set :=
  | BOF_add
  | BOF_sub
  | BOF_mul
  | BOF_div
  | BOF_min
  | BOF_max
  | BOF_copysign
  .

Inductive binop : Set :=
  | Binop_i : binop_i -> binop
  | Binop_f : binop_f -> binop
  .

Inductive testop : Set :=
  | TO_eqz
  .

Inductive relop_i : Set :=
  | ROI_eq
  | ROI_ne
  | ROI_lt : sx -> relop_i
  | ROI_gt : sx -> relop_i
  | ROI_le : sx -> relop_i
  | ROI_ge : sx -> relop_i
  .

Inductive relop_f : Set :=
  | ROF_eq
  | ROF_ne
  | ROF_lt
  | ROF_gt
  | ROF_le
  | ROF_ge
  .

Inductive relop : Set :=
  | Relop_i : relop_i -> relop
  | Relop_f : relop_f -> relop
  .

(* TODO: comment on the other cvtops *)
Inductive cvtop : Set :=
  | CVO_wrap
  | CVO_extend
  | CVO_trunc
  | CVO_trunc_sat
  | CVO_convert
  | CVO_demote
  | CVO_promote
  | CVO_reinterpret
  .

Inductive packed_type : Set := (* tp *)
  | Tp_i8
  | Tp_i16
  | Tp_i32
  .

Inductive shape_vec_i: Set :=
  | SVI_8_16
  | SVI_16_8
  | SVI_32_4
  | SVI_64_2
  .

Inductive shape_vec_f: Set :=
  | SVF_32_4
  | SVF_64_2
  .

Inductive shape_vec : Set := (* shape *)
  | SV_ishape: shape_vec_i -> shape_vec
  | SV_fshape: shape_vec_f -> shape_vec
  .

Inductive unop_vec : Set :=
  | VUO_not
  .

Inductive binop_vec : Set :=
  | VBO_and
  .

Inductive ternop_vec : Set :=
  | VTO_bitselect
  .

Inductive test_vec : Set :=
  | VT_any_true
  .

Inductive shift_vec : Set :=
  | VSH_any_true
  .

Definition laneidx := u8.

Inductive packed_type_vec :=
  | Tptv_8_8
  | Tptv_16_4
  | Tptv_32_2
.

Inductive zero_type_vec :=
  | Tztv_32
  | Tztv_64
.

Inductive width_vec :=
  | Twv_8
  | Twv_16
  | Twv_32
  | Twv_64
  .

Inductive load_vec_arg :=
  | LVA_packed: packed_type_vec -> sx -> load_vec_arg
  | LVA_zero: zero_type_vec -> load_vec_arg
  | LVA_splat: width_vec -> load_vec_arg
  .

Record memarg : Set :=
  { memarg_offset : u32;
    memarg_align: u32
  }.

Inductive basic_instruction : Type := (* be *)
(** std-doc:
Numeric instructions provide basic operations over numeric values of specific type. These operations closely match respective operations available in hardware.
 **)
  | BI_const_num : value_num -> basic_instruction
  | BI_unop : number_type -> unop -> basic_instruction
  | BI_binop : number_type -> binop -> basic_instruction
  | BI_testop : number_type -> testop -> basic_instruction
  | BI_relop : number_type -> relop -> basic_instruction
  | BI_cvtop : number_type -> cvtop -> number_type -> option sx -> basic_instruction
(** std-doc: (not implemented yet)
Vector instructions (also known as SIMD instructions, single data multiple value) provide basic operations over values of vector type.
Vector instructions can be grouped into several subcategories:

Constants: return a static constant.
Unary Operations: consume one v128 operand and produce one v128 result.
Binary Operations: consume two v128 operands and produce one v128 result.
Ternary Operations: consume three v128 operands and produce one v128 result.
Tests: consume one v128 operand and produce a Boolean integer result.
Shifts: consume a v128 operand and a i32 operand, producing one v128 result.
Splats: consume a value of numeric type and produce a v128 result of a specified shape.
Extract lanes: consume a v128 operand and return the numeric value in a given lane.
Replace lanes: consume a v128 operand and a numeric value for a given lane, and produce a v128 result.
**)
  | BI_const_vec : value_vec -> basic_instruction
  | BI_unop_vec: unop_vec -> basic_instruction
  | BI_binop_vec: binop_vec -> basic_instruction
  | BI_ternop_vec: ternop_vec -> basic_instruction
  | BI_test_vec: test_vec -> basic_instruction
  | BI_shift_vec: shift_vec -> basic_instruction
  | BI_splat_vec: shape_vec -> basic_instruction
  | BI_extract_vec: shape_vec -> option sx -> laneidx -> basic_instruction
  | BI_replace_vec: shape_vec -> laneidx -> basic_instruction
(** std-doc:
Instructions in this group are concerned with accessing references.
**)
  | BI_ref_null : reference_type -> basic_instruction
  | BI_ref_is_null
  | BI_ref_func : funcidx -> basic_instruction
(** std-doc:
Instructions in this group can operate on operands of any value type.
**)
  | BI_drop
  | BI_select : option (list value_type) -> basic_instruction
(** std-doc:
Variable instructions are concerned with access to local or global variables.
**)
  | BI_local_get : localidx -> basic_instruction
  | BI_local_set : localidx -> basic_instruction
  | BI_local_tee : localidx -> basic_instruction
  | BI_global_get : globalidx -> basic_instruction
  | BI_global_set : globalidx -> basic_instruction
(** std-doc:
Instructions in this group are concerned with tables.
**)
  | BI_table_get : tableidx -> basic_instruction
  | BI_table_set : tableidx -> basic_instruction
  | BI_table_size : tableidx -> basic_instruction
  | BI_table_grow : tableidx -> basic_instruction
  | BI_table_fill : tableidx -> basic_instruction
  | BI_table_copy : tableidx -> tableidx -> basic_instruction
  | BI_table_init : tableidx -> elemidx -> basic_instruction
  | BI_elem_drop : elemidx -> basic_instruction
(** std-doc:
Instructions in this group are concerned with linear memory.
**)
  | BI_load : number_type -> option (packed_type * sx) -> memarg -> basic_instruction
  | BI_load_vec : load_vec_arg -> memarg -> basic_instruction
  (* the lane version has a different type signature *)
  | BI_load_vec_lane : width_vec -> memarg -> laneidx -> basic_instruction
  | BI_store : number_type -> option packed_type -> memarg-> basic_instruction
  | BI_store_vec_lane : width_vec -> memarg -> laneidx -> basic_instruction
  | BI_memory_size
  | BI_memory_grow
  | BI_memory_fill
  | BI_memory_copy
  | BI_memory_init: dataidx -> basic_instruction
  | BI_data_drop: dataidx -> basic_instruction
(** std-doc:
Instructions in this group affect the flow of control.
**)
  | BI_nop
  | BI_unreachable
  | BI_block : block_type -> list basic_instruction -> basic_instruction
  | BI_loop : block_type -> list basic_instruction -> basic_instruction
  | BI_if : block_type -> list basic_instruction -> list basic_instruction -> basic_instruction
  | BI_br : labelidx -> basic_instruction
  | BI_br_if : labelidx -> basic_instruction
  | BI_br_table : list labelidx -> labelidx -> basic_instruction
  | BI_return
  | BI_call : funcidx -> basic_instruction
  | BI_call_indirect : tableidx -> typeidx -> basic_instruction
  .


(** std-doc:
Function bodies, initialization values for globals, and offsets of element or data segments are given as expressions, which are sequences of instructions terminated by an 𝖾𝗇𝖽 marker.

In some places, validation restricts expressions to be constant, which limits the set of allowable instructions.

*)
Definition expr := list basic_instruction.

End Instructions.

(** std-doc:
WebAssembly computations manipulate values of either the four basic number types, i.e., integers and floating-point data of 32 or 64 bit width each, of vectors of 128 bit width, or of reference type.

In most places of the semantics, values of different types can occur. In order to avoid ambiguities, values are therefore represented with an abstract syntax that makes their type explicit. It is convenient to reuse the same notation as for the
const instructions and ref.null producing them.

References other than null are represented with additional administrative instructions. They either are function references, pointing to a specific function address, or external references pointing to an uninterpreted form of extern address that can be defined by the embedder to represent its own objects.
*)
Inductive value_ref : Set :=
| VAL_ref_null: reference_type -> value_ref
| VAL_ref_func: funcaddr -> value_ref
| VAL_ref_extern: externaddr -> value_ref
.

Inductive value : Type :=
| VAL_num: value_num -> value
| VAL_vec: value_vec -> value
| VAL_ref: value_ref -> value
.


Section Modules.

(** std-doc:
The imports component of a module defines a set of imports that are required for instantiation.

Each import is labeled by a two-level name space, consisting of a module name and a name for
 an entity within that module. Importable definitions are functions, tables, memories, and globals.
 Each import is specified by a descriptor with a respective type that a definition provided during
 instantiation is required to match.

Every import defines an index in the respective index space. In each index space, the indices of imports go before the first index of any definition contained in the module itself.

[https://www.w3.org/TR/wasm-core-2/syntax/modules.html#imports]
*)
Inductive module_import_desc : Set :=
| MID_func : typeidx -> module_import_desc
| MID_table : table_type -> module_import_desc
| MID_mem : memory_type -> module_import_desc
| MID_global : global_type -> module_import_desc.

Record module_import : Set := {
  imp_module : name;
  imp_name : name;
  imp_desc : module_import_desc;
}.

Record module_func : Type := {
  modfunc_type : typeidx;
  modfunc_locals : list value_type;
  modfunc_body : expr;
}.

Record module_table : Set := {
  modtab_type : table_type;
}.

Record module_mem : Set := {
  modmem_type : memory_type;
}.

Record module_global : Type := {
  modglob_type : global_type;
  modglob_init : expr;
}.


(** std-doc:
The initial contents of a table is uninitialized. Element segments can be used to initialize a
subrange of a table from a static vector of elements.

The elems component of a module defines a vector of element segments. Each element segment defines
a reference type and a corresponding list of constant element expressions.

Element segments have a mode that identifies them as either passive, active, or declarative.
A passive element segment’s elements can be copied to a table using the table.init instruction.
An active element segment copies its elements into a table during instantiation, as specified by
a table index and a constant expression defining an offset into that table. A declarative element
segment is not available at runtime but merely serves to forward-declare references that are formed
in code with instructions like ref.func.

The offset is given by a constant expression.

Element segments are referenced through element indices.
[https://www.w3.org/TR/wasm-core-2/syntax/modules.html#element-segments]
*)

Inductive module_elemmode : Type :=
| ME_passive
| ME_active : tableidx -> expr -> module_elemmode
| ME_declarative
.

Record module_element : Type := {
  modelem_type : reference_type;
  modelem_init : list expr;
  modelem_mode : module_elemmode;
}.


(** std-doc:
The initial contents of a memory are zero bytes. Data segments can be used to initialize a range
of memory from a static vector of bytes.

The datas component of a module defines a vector of data segments.

Like element segments, data segments have a mode that identifies them as either passive or active.
A passive data segment’s contents can be copied into a memory using the memory.init instruction.
An active data segment copies its contents into a memory during instantiation, as specified by a
memory index and a constant expression defining an offset into that memory.

The initial contents of a table is uninitialized. Element segments can be used to initialize a
subrange of a table from a static vector of elements.

Data segments are referenced through data indices.
[https://www.w3.org/TR/wasm-core-2/syntax/modules.html#data-segments]
*)

Inductive module_datamode : Type :=
| MD_passive
| MD_active : memidx -> expr -> module_datamode
.

Record module_data : Type := {
  moddata_init : list byte;
  moddata_mode : module_datamode;
}.

Record module_start : Set := {
  modstart_func : funcidx;
}.


Inductive module_export_desc : Set :=
| MED_func : funcidx -> module_export_desc
| MED_table : tableidx -> module_export_desc
| MED_mem : memidx -> module_export_desc
| MED_global : globalidx -> module_export_desc.

Record module_export : Set := {
  modexp_name : name;
  modexp_desc : module_export_desc;
}.

(** std-doc:
WebAssembly programs are organized into modules, which are the unit of deployment, loading, and compilation. A module collects definitions for types, functions, tables, memories, and globals. In addition, it can declare imports and exports and provide initialization logic in the form of data and element segments or a start function.
[https://webassembly.github.io/spec/core/syntax/modules.html]
*)
Record module : Type := {
  mod_types : list function_type;
  mod_funcs : list module_func;
  mod_tables : list module_table;
  mod_mems : list module_mem;
  mod_globals : list module_global;
  mod_elems : list module_element;
  mod_datas : list module_data;
  mod_start : option module_start;
  mod_imports : list module_import;
  mod_exports : list module_export;
}.

End Modules.

(** Validation **)

(** Typing context. **)
(** std-doc:
Validity of an individual definition is specified relative to a context, which
collects relevant information about the surrounding module and the definitions
in scope:
- Types: the list of types defined in the current module.
- Functions: the list of functions declared in the current module, represented
  by their function type.
- Tables: the list of tables declared in the current module, represented by
  their table type.
- Memories: the list of memories declared in the current module, represented by
  their memory type.
- Globals: the list of globals declared in the current module, represented by
  their global type.
- Element Segments: the list of element segments declared in the current module,
  represented by their element type.
- Data Segments: the list of data segments declared in the current module, each
  represented by an ok entry.
- Locals: the list of locals declared in the current function (including
  parameters), represented by their value type.
- Labels: the stack of labels accessible from the current position, represented
  by their result type.
- Return: the return type of the current function, represented as an optional
  result type that is absent when no return is allowed, as in free-standing
  expressions.
- References: the list of function indices that occur in the module outside functions and can hence be used to form references inside them.

In other words, a context contains a sequence of suitable types for each index
space, describing each defined entry in that space. Locals, labels and return
type are only used for validating instructions in function bodies, and are left
empty elsewhere. The label stack is the only part of the context that changes
as validation of an instruction sequence proceeds.

[https://www.w3.org/TR/wasm-core-2/valid/conventions.html#contexts]
 *)

Definition ok: Set := unit.

Record t_context : Set := {
  tc_types : list function_type;
  tc_funcs : list function_type;
  tc_tables : list table_type;
  tc_mems : list memory_type;
  tc_globals : list global_type;
  tc_elems : list reference_type;
  tc_datas : list ok;
  tc_locals : list value_type;
  tc_labels : list result_type;
  tc_return : option result_type;
  tc_refs : list funcidx;
}.

Inductive result : Type :=
| result_values : list value -> result
(** Note from the specification:
  In the current version of WebAssembly, a result can consist of at most one value. **)
| result_trap : result
.


(** * Functions and Store **)


(** std-doc:
A table instance is the runtime representation of a table. It records its type and
holds a vector of reference values.

Table elements can be mutated through table instructions, the execution of an active
element segment, or by external means provided by the embedder.

It is an invariant of the semantics that all table elements have a type equal to the
element type of tabletype. It also is an invariant that the length of the element vector
never exceeds the maximum size of tabletype, if present.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#table-instances]
*)
Record tableinst : Set := {
  tableinst_type : table_type;
  tableinst_elem : list value_ref;
}.


(** std-doc:
A memory instance is the runtime representation of a linear memory. It records
its type and holds a vector of bytes.

The length of the vector always is a multiple of the WebAssembly page size, which
is defined to be the constant 65536 – abbreviated 64Ki.

The bytes can be mutated through memory instructions, the execution of an active data
segment, or by external means provided by the embedder.

It is an invariant of the semantics that the length of the byte vector, divided by page
size, never exceeds the maximum size of memtype, if present.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#memory-instances]
*)
Record meminst : Set := {
  meminst_type : memory_type;
  meminst_data: memory_list;
}.


(** std-doc:
A global instance is the runtime representation of a global variable. It records its
type and holds an individual value.

The value of mutable globals can be mutated through variable instructions or by external
means provided by the embedder.

It is an invariant of the semantics that the value has a type equal to the value type of globaltype.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#global-instances]
*)
Record globalinst : Type := {
  g_type : global_type;
  g_val : value;
}.


(** std-doc:
An element instance is the runtime representation of an element segment. It holds a vector
of references and their common type.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#element-instances]
*)
Record eleminst : Set := {
  eleminst_type : reference_type;
  eleminst_elem : list value_ref;
}.


(** std-doc:
A data instance is the runtime representation of a data segment. It holds a vector of bytes.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#data-instances]
*)
Record datainst : Set := {
  datainst_data : list byte;
}.

(** std-doc:
An external value is the runtime representation of an entity that can be imported or
exported. It is an address denoting either a function instance, table instance, memory
instance, or global instances in the shared store.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#external-values]
*)
Inductive extern_value: Set :=
| EV_func: funcaddr -> extern_value
| EV_table: tableaddr -> extern_value
| EV_mem: memaddr -> extern_value
| EV_global: globaladdr -> extern_value
.


(** std-doc:
An export instance is the runtime representation of an export. It defines the export’s
name and the associated external value.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#export-instances]
*)
Record exportinst : Type := {
  exportinst_name: name;
  exportinst_val: extern_value;
}.


(** std-doc:
A module instance is the runtime representation of a module. It is created by
instantiating a module, and collects runtime representations of all entities
that are imported, defined, or exported by the module.

Each component references runtime instances corresponding to respective
declarations from the original module – whether imported or defined – in the
order of their static indices. Function instances, table instances, memory
instances, and global instances are referenced with an indirection through
their respective addresses in the store.

It is an invariant of the semantics that all export instances in a given module
instance have different names.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#module-instances]
*)
Record moduleinst : Type := (* inst *) {
  inst_types : list function_type;
  inst_funcs : list funcaddr;
  inst_tables : list tableaddr;
  inst_mems : list memaddr;
  inst_globals : list globaladdr;
  inst_elems : list elemaddr;
  inst_datas : list dataaddr;
  inst_exports: list exportinst;
  }.

Definition empty_moduleinst := Build_moduleinst nil nil nil nil nil nil nil nil.


(** We assume a family of host functions. **)
Class host_function_class : Type :=
  { host_function : Type;
    host_function_eq_dec : forall f1 f2 : host_function, {f1 = f2} + {f1 <> f2}
  }.

Section Host.

  Context `{host_function_class}.

(** std-doc:
A function instance is the runtime representation of a function. It effectively
is a closure of the original function over the runtime module instance of its
originating module. The module instance is used to resolve references to other
definitions during execution of the function.

A host function is a function expressed outside WebAssembly but passed to a module
as an import. The definition and behavior of host functions are outside the scope
of this specification. For the purpose of this specification, it is assumed that
when invoked, a host function behaves non-deterministically, but within certain
constraints that ensure the integrity of the runtime.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#function-instances]
*)
Inductive funcinst : Type := (* cl *)
  | FC_func_native : function_type -> moduleinst -> module_func -> funcinst
  | FC_func_host (tf: function_type) (hf: host_function) : funcinst
.

(** std-doc:
The store represents all global state that can be manipulated by WebAssembly programs.
It consists of the runtime representation of all instances of functions, tables,
memories, and globals, element segments, and data segments that have been allocated
during the life time of the abstract machine.

It is an invariant of the semantics that no element or data instance is addressed from
anywhere else but the owning module instances.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#store]
*)
Record store_record : Type := (* s *) {
  s_funcs : list funcinst;
  s_tables : list tableinst;
  s_mems : list meminst;
  s_globals : list globalinst;
  s_elems: list eleminst;
  s_datas: list datainst;
}.


(** std-doc:

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#activations-and-frames]
*)
Record frame : Type := (* f *) {
  f_locs: list value;
  f_inst: moduleinst
}.

Definition empty_frame := Build_frame nil empty_moduleinst.


(** * Administrative Instructions **)

(** std-doc:
WebAssembly code consists of sequences of instructions. Its computational model is based
on a stack machine in that instructions manipulate values on an implicit operand stack,
consuming (popping) argument values and producing or returning (pushing) result values.

In addition to dynamic operands from the stack, some instructions also have static
immediate arguments, typically indices or type annotations, which are part of the
instruction itself.

Some instructions are structured in that they bracket nested sequences of instructions.
[https://webassembly.github.io/spec/core/syntax/instructions.html]

In order to express the reduction of traps, calls, and control instructions,
the syntax of instructions is extended to include the following administrative
instructions:
*)
Inductive administrative_instruction : Type := (* e *)
| AI_basic : basic_instruction -> administrative_instruction
| AI_trap
| AI_ref : funcaddr -> administrative_instruction
| AI_ref_extern: externaddr -> administrative_instruction
| AI_invoke : funcaddr -> administrative_instruction
| AI_label : nat -> list administrative_instruction -> list administrative_instruction -> administrative_instruction
| AI_frame : nat -> frame -> list administrative_instruction -> administrative_instruction
.

(** std-doc:
In order to specify the reduction of branches, the following syntax of block
contexts is defined, indexed by the count k of labels surrounding a hole [_] that
marks the place where the next step of computation is taking place.

This definition allows to index active labels surrounding a branch or return instruction.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#block-contexts]
 *)

Inductive lholed : nat -> Type :=
| LH_base : list value -> list administrative_instruction -> lholed 0
| LH_rec {k: nat}: list value -> nat -> list administrative_instruction -> lholed k -> list administrative_instruction -> lholed (S k)
.

(** std-doc:
A configuration consists of the current store and an executing thread.

A thread is a computation over instructions that operates relative to a current
frame referring to the module instance in which the computation runs, i.e., where
the current function originates from.

[https://www.w3.org/TR/wasm-core-2/exec/runtime.html#configurations]
 *)
Definition thread : Type := frame * list administrative_instruction.

Definition config_tuple : Type := store_record * thread.
End Host.

(* Notations for values to basic/admin instructions *)
Notation "$VN v" := (AI_basic (BI_const_num v)) (at level 60).
Notation "$VV v" := (AI_basic (BI_const_vec v)) (at level 60).
*/

use uuid::Uuid;
use wasmparser::{
    AbstractHeapType, BlockType, CompositeInnerType, Data, DataKind, Element, ElementKind, Export,
    FunctionBody, Global, HeapType, Import, MemoryType, Operator, OperatorsReader, RecGroup,
    RefType, Table, TableType, TypeRef, ValType,
};

#[derive(Debug)]
pub enum WasmModuleParseError {
    UnsupportedOperation(String),
}

impl WasmModuleParseError {
    fn add_string_to_reported_error(
        info: &String,
        error: WasmModuleParseError,
    ) -> WasmModuleParseError {
        let WasmModuleParseError::UnsupportedOperation(error_message) = error;
        let ret_err = format!("{info}\n\t{error_message}").to_string();
        WasmModuleParseError::UnsupportedOperation(ret_err)
    }
}

pub(crate) struct WasmParseData<'a> {
    mod_name: String,

    pub(crate) start_function: Option<u32>,

    pub(crate) imports: Vec<Import<'a>>,
    pub(crate) exports: Vec<Export<'a>>,
    pub(crate) tables: Vec<Table<'a>>,
    pub(crate) memory_types: Vec<MemoryType>,
    pub(crate) globals: Vec<Global<'a>>,
    pub(crate) data: Vec<Data<'a>>,
    pub(crate) elements: Vec<Element<'a>>,
    pub(crate) function_types: Vec<RecGroup>,
    pub(crate) function_type_indexes: Vec<u32>,
    pub(crate) function_bodies: Vec<FunctionBody<'a>>,
}

impl WasmParseData<'_> {
    pub(crate) fn new<'a>(mod_name: String) -> WasmParseData<'a> {
        WasmParseData {
            mod_name,
            start_function: None,
            imports: Vec::new(),
            exports: Vec::new(),
            tables: Vec::new(),
            memory_types: Vec::new(),
            globals: Vec::new(),
            data: Vec::new(),
            elements: Vec::new(),
            function_types: Vec::new(),
            function_type_indexes: Vec::new(),
            function_bodies: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate(&self) -> anyhow::Result<String /* WasmModuleParseError*/> {
        let mut res = String::new();
        res.push_str("Require Import List.\n");
        res.push_str("Require Import String.\n");
        res.push_str("Require Import BinNat.\n");
        res.push_str("Require Import ZArith.\n");
        res.push_str("From Wasm Require Import numerics.\n");
        res.push_str("From Wasm Require Import datatypes.\n");

        let mut translated_imports = Vec::new();
        let mut errors = Vec::new();
        for import in &self.imports {
            //let (definition_name, res) = translate_import(import);
            match translate_module_import(import) {
                Ok(translated_import) => {
                    res.push_str(translated_import.as_str());
                    translated_imports.push(import);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        let mut created_exports = Vec::new();
        for export in &self.exports {
            // let (name, res) = translate_export_module(export);
            match translate_export_module(export) {
                Ok(translated_export) => {
                    res.push_str(translated_export.as_str());
                    created_exports.push(translated_export);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        let mut created_tables = Vec::new();
        for table in &self.tables {
            // let (name, res) = translate_table(table);
            match translate_table_type(table) {
                Ok(translated_table_type) => {
                    res.push_str(translated_table_type.as_str());
                    created_tables.push(translated_table_type);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        let mut created_memory_types = Vec::new();
        for memory_type in &self.memory_types {
            // let (name, res) = translate_memory(memory_type);
            match translate_memory_type(memory_type) {
                Ok(translated_memory) => {
                    res.push_str(translated_memory.as_str());
                    created_memory_types.push(translated_memory);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        let mut created_globals = Vec::new();
        for global in &self.globals {
            match translate_global(global) {
                Ok(translated_global) => {
                    res.push_str(translated_global.as_str());
                    created_globals.push(translated_global);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        let mut created_data_segments = Vec::new();
        for data in &self.data {
            match translate_data(data) {
                Ok(translated_data) => {
                    res.push_str(translated_data.as_str());
                    created_data_segments.push(translated_data);
                }
                Err(e) => errors.push(e),
            }
        }
        // let mut created_elements = Vec::new();
        // for element in &self.elements {
        //     match translate_element(element) {
        //         Ok((name, res)) => {
        //             res.push_str(res.as_str());
        //             created_elements.push(name);
        //         }
        //         Err(e) => {
        //             return Err(self.add_module_name_to_reported_error(e));
        //         }
        //     }
        // }

        // let mut created_function_types = Vec::new();
        // for rec_group in &self.function_types {
        //     let (name, res) = translate_rec_group(rec_group);
        //     res.push_str(res.as_str());
        //     created_function_types.push(name);
        // }

        // let created_functions =
        //     match translate_functions(&self.function_type_indexes, &self.function_bodies) {
        //         Ok((names, res)) => {
        //             res.push_str(res.as_str());
        //             names
        //         }
        //         Err(e) => {
        //             return Err(self.add_module_name_to_reported_error(e));
        //         }
        //     };

        // let module_name = &self.mod_name;
        // res.push_str(format!("Definition {module_name} : WasmModule :=\n").as_str());
        // res.push_str("{|\n");

        // let mut types = String::new();
        // for ty in created_function_types {
        //     types.push_str(format!("{ty} :: ").as_str());
        // }
        // types.push_str("nil;\n");
        // res.push_str(format!("m_types := {types}").as_str());

        // let mut funcs = String::new();
        // for func in created_functions {
        //     funcs.push_str(format!("{func} :: ").as_str());
        // }
        // funcs.push_str("nil;\n");
        // res.push_str(format!("m_funcs := {funcs}").as_str());

        // let mut tables = String::new();
        // for table in created_tables {
        //     tables.push_str(format!("{table} :: ").as_str());
        // }
        // tables.push_str("nil;\n");
        // res.push_str(format!("m_tables := {tables}").as_str());

        // let mut mems = String::new();
        // for mem in created_memory_types {
        //     mems.push_str(format!("{mem} :: ").as_str());
        // }
        // mems.push_str("nil;\n");
        // res.push_str(format!("m_mems := {mems}").as_str());

        // let mut globals = String::new();
        // for global in created_globals {
        //     globals.push_str(format!("{global} :: ").as_str());
        // }
        // globals.push_str("nil;\n");
        // res.push_str(format!("m_globals := {globals}").as_str());

        // let mut elems = String::new();
        // for elem in created_elements {
        //     elems.push_str(format!("{elem} :: ").as_str());
        // }
        // elems.push_str("nil;\n");
        // res.push_str(format!("m_elems := {elems}").as_str());

        // let mut datas = String::new();
        // for data in created_data_segments {
        //     datas.push_str(format!("{data} :: ").as_str());
        // }

        // datas.push_str("nil;\n");
        // res.push_str(format!("m_datas := {datas}").as_str());

        // if let Some(start_function) = self.start_function {
        //     res.push_str(format!("m_start := Some({start_function});\n").as_str());
        // } else {
        //     res.push_str("m_start := None;\n");
        // }

        // let mut imports = String::new();
        // for import in translated_imports {
        //     imports.push_str(format!("{import} :: ").as_str());
        // }
        // imports.push_str("nil;\n");
        // res.push_str(format!("m_imports := {imports}").as_str());

        // let mut exports = String::new();
        // for export in created_exports {
        //     exports.push_str(format!("{export} :: ").as_str());
        // }
        // exports.push_str("nil;\n");
        // res.push_str(format!("m_exports := {exports}").as_str());

        // res.push_str("|}.");
        Ok(res)
    }
}

const RLB: &str = "{|";
const RRB: &str = "{|";

// //Inductive value_num
// fn translate_value_num(val_type: &ValType) -> anyhow::Result<String> {
//     let res = match val_type {
//         ValType::I32 => "VAL_int32",
//         ValType::I64 => "VAL_int64",
//         ValType::F32 => "VAL_float32",
//         ValType::F64 => "VAL_float64",
//         ValType::V128 => return Err(anyhow::anyhow!("V128 is not supported")),
//         ValType::Ref(_) => return Err(anyhow::anyhow!("Ref is not supported")),
//     };
//     Ok(res.to_string())
// }

// //Inductive number_type
// fn translate_number_type(val_type: &ValType) -> anyhow::Result<String> {
//     let res = match val_type {
//         ValType::I32 => "T_i32",
//         ValType::I64 => "T_i64",
//         ValType::F32 => "T_f32",
//         ValType::F64 => "T_f64",
//         ValType::V128 => return Err(anyhow::anyhow!("V128 is not supported")),
//         ValType::Ref(_) => return Err(anyhow::anyhow!("Ref is not supported")),
//     };
//     Ok(res.to_string())
// }

//Inductive reference_type
fn translate_ref_type(ref_type: &RefType) -> anyhow::Result<String> {
    if *ref_type == RefType::FUNCREF {
        Ok(String::from("T_funcref"))
    } else if *ref_type == RefType::EXTERNREF {
        Ok(String::from("T_externref"))
    } else {
        Err(anyhow::anyhow!("UNSUPPORTED REF TYPE"))
    }
}

//Inductive value_type
fn translate_value_type(val_type: &ValType) -> anyhow::Result<String> {
    let res = match val_type {
        ValType::I32 => "T_num",
        ValType::I64 => "T_num",
        ValType::F32 => "T_num",
        ValType::F64 => "T_num",
        ValType::V128 => return Err(anyhow::anyhow!("T_vec is not supported")),
        ValType::Ref(_) => return Err(anyhow::anyhow!("T_ref is not supported")),
    };
    Ok(res.to_string())
}

//Record module_import
fn translate_module_import(import: &Import) -> anyhow::Result<String> {
    let imp_name = String::from(import.name);
    let imp_module = String::from(import.module);
    let definition_name =
        imp_module.clone() + &imp_name.clone().remove(0).to_uppercase().to_string();
    let imp_desc = translate_module_import_desc(import)?;
    let mut res = String::new();
    res.push_str(format!("Definition {definition_name} : module_import :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("imp_module := \"{imp_module}\";\n").as_str());
    res.push_str(format!("imp_name := \"{imp_name}\";\n").as_str());
    res.push_str(format!("imp_desc := {imp_desc}\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

//Inductive module_import_desc
fn translate_module_import_desc(import: &Import) -> anyhow::Result<String> {
    let res = match import.ty {
        TypeRef::Func(index) => format!("MID_func {index}"),
        TypeRef::Global(global_type) => {
            let tg_mut = translate_mutability(global_type.mutable);
            let tg_t = translate_value_type(&global_type.content_type)?;
            format!("MID_global tg_mut := {tg_mut}; tg_t := {tg_t}")
        }
        TypeRef::Memory(memory_type) => {
            let limits = translate_memory_type_limits(&memory_type)?;
            format!("MID_mem {limits}")
        }
        TypeRef::Table(table_type) => {
            let table_type_translated = translate_table_type_limits(&table_type)?;
            format!("MID_table {table_type_translated}")
        }
        TypeRef::Tag(_) => return Err(anyhow::anyhow!("Tag is not supported in import")),
    };
    Ok(res)
}

//Inductive mutability
fn translate_mutability(mutable: bool) -> String {
    if mutable {
        "MUT_var".to_string()
    } else {
        "MUT_const".to_string()
    }
}

//Record limits
fn translate_table_type_limits(table_type: &TableType) -> anyhow::Result<String> {
    let lim_min = table_type.initial.to_string();
    let lim_max = match table_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };
    let ref_type = translate_ref_type(&table_type.element_type)?;
    Ok(format!("{RLB} tt_limits := {RLB} lim_min := {lim_min}; lim_max := {lim_max} {RRB}; tt_elem_type := {ref_type} {RRB}"))
}

//Record limits
fn translate_memory_type_limits(memory_type: &MemoryType) -> anyhow::Result<String> {
    let lim_min = memory_type.initial.to_string();
    let lim_max = match memory_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };
    Ok(format!(
        "{RLB} l_min := {lim_min}; l_max := {lim_max} {RRB}"
    ))
}

//Inductive translate_export_module
fn translate_export_module(export: &Export) -> anyhow::Result<String> {
    let mut res = String::new();
    let modexp_name = export.name;
    let modexp_desc = translate_module_export_desc(export)?;
    res.push_str(format!("Definition {modexp_name} : module_export :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("modexp_name := \"{modexp_name}\";\n").as_str());
    res.push_str(format!("modexp_desc := {modexp_desc}\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

//Inductive module_export_desc
fn translate_module_export_desc(export: &Export) -> anyhow::Result<String> {
    let res = match export.kind {
        wasmparser::ExternalKind::Func => format!("MED_func {}", export.index),
        wasmparser::ExternalKind::Table => format!("MED_table {}", export.index),
        wasmparser::ExternalKind::Memory => format!("MED_mem {}", export.index),
        wasmparser::ExternalKind::Global => format!("MED_global {}", export.index),
        wasmparser::ExternalKind::Tag => return Err(anyhow::anyhow!("Tag is not supported")),
    };
    Ok(res)
}

//Record table_type
fn translate_table_type(table: &Table) -> anyhow::Result<String> {
    let mut res = String::new();
    let tt_limits = translate_table_type_limits(&table.ty)?;
    let tt_elem_type = translate_ref_type(&table.ty.element_type)?;
    let id = get_id();
    res.push_str(format!("Definition tt_{id} : table_type :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("tt_limits := {tt_limits};\n").as_str());
    res.push_str(format!("tt_elem_type := {tt_elem_type}\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

//Definition memory_type
fn translate_memory_type(memory_type: &MemoryType) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let limits = translate_memory_type_limits(memory_type)?;
    res.push_str(format!("Definition mem_{id} : memory_type :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("limits := {limits}\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

//Record global_type
fn translate_global(global: &Global) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let tg_mut = translate_mutability(global.ty.mutable);
    let tg_t = translate_value_type(&global.ty.content_type)?;
    res.push_str(format!("Definition global_{id} : global_type :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("tg_mut := {tg_mut};\n").as_str());
    res.push_str(format!("tg_t := {tg_t}\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

//Inductive module_datamode
fn translate_module_datamode(data: &Data) -> anyhow::Result<String> {
    let res = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => {
            let expression = translate_expr(offset_expr.get_operators_reader())?;
            format!("MD_active {memory_index} ({expression})")
        }
        DataKind::Passive => "MD_passive".to_string(),
    };
    Ok(res)
}

//Definition expr
fn translate_expr(operators_reader: OperatorsReader) -> anyhow::Result<String> {
    let mut res = String::new();
    for operator in operators_reader {
        let op = operator?;
        let translated_op = translate_basic_instruction(op)?;
        res.push_str(translated_op.as_str());
        res.push_str("::");
    }
    res.push_str("nil");
    Ok(res)
}

fn translate_block_type(block_type: &BlockType) -> anyhow::Result<String> {
    let res = match block_type {
        BlockType::Empty => String::new(),
        BlockType::FuncType(index) => format!("BT_id {index}"),
        BlockType::Type(valtype) => {
            let valtype = translate_value_type(&valtype)?;
            format!("BT_valtype {valtype}")
        }
    };
    Ok(res)
}

//Record memarg
fn parse_memarg(memarg: &wasmparser::MemArg) -> anyhow::Result<String> {
    let mut res = String::new();
    let memarg_offset = memarg.offset.to_string();
    let memarg_align = memarg.align.to_string();
    res.push_str(format!("Definition memarg : memarg :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("memarg_offset := {memarg_offset};\n").as_str());
    res.push_str(format!("memarg_align := {memarg_align}\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

//Inductive basic_instruction
fn translate_basic_instruction(operator: Operator) -> anyhow::Result<String> {
    let operator = match operator {
        wasmparser::Operator::Nop => "BI_nop".to_string(),
        wasmparser::Operator::Unreachable => "BI_unreachable".to_string(),
        wasmparser::Operator::Block { blockty } => {
            let blockty = translate_block_type(&blockty)?;
            format!("(BI_block {blockty}")
        }
        Operator::Loop { blockty } => {
            let blockty = translate_block_type(&blockty)?;
            format!("(BI_loop {blockty}")
        }
        Operator::If { blockty } => {
            let blockty = translate_block_type(&blockty)?;
            format!("(BI_if {blockty}")
        }
        Operator::Else => "nil) (".to_string(),
        Operator::End => "nil) :: ".to_string(),
        Operator::Br { relative_depth } => format!("BI_br {relative_depth}"),
        Operator::BrIf { relative_depth } => format!("BI_br_if {relative_depth}"),
        Operator::BrTable { targets } => {
            if targets.is_empty() {
                "BI_br_table".to_string()
            } else {
                let mut labelidx = String::new();
                for target in targets.targets() {
                    let id = target.unwrap();
                    labelidx.push_str(format!("{id}").as_str());
                    labelidx.push_str(" :: ");
                }
                labelidx.push_str("nil");
                format!("BI_br_table {labelidx}")
            }
        }
        Operator::Return => "BI_return".to_string(),
        Operator::Call { function_index } => format!("BI_call {function_index}"),
        Operator::CallIndirect {
            type_index,
            table_index,
        } => format!("BI_call_indirect {type_index} {table_index}"),
        Operator::Drop => "BI_drop".to_string(),
        Operator::Select => "BI_select None".to_string(),
        Operator::LocalGet { local_index } => format!("BI_local_get {local_index}"),
        Operator::LocalSet { local_index } => format!("BI_local_set {local_index}"),
        Operator::LocalTee { local_index } => format!("BI_local_tee {local_index}"),
        Operator::GlobalGet { global_index } => format!("BI_global_get {global_index}"),
        Operator::GlobalSet { global_index } => format!("BI_global_set {global_index}"),
        Operator::I32Load { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_load T_i32 None {memarg}")
        }
        Operator::I64Load { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_load T_i64 None {memarg}")
        }
        Operator::F32Load { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_load T_f32 None {memarg}")
        }
        Operator::F64Load { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_load T_f64 None {memarg}")
        }
        Operator::I32Load8S { memarg } => todo!(),
        Operator::I32Load8U { memarg } => todo!(),
        Operator::I32Load16S { memarg } => todo!(),
        Operator::I32Load16U { memarg } => todo!(),
        Operator::I64Load8S { memarg } => todo!(),
        Operator::I64Load8U { memarg } => todo!(),
        Operator::I64Load16S { memarg } => todo!(),
        Operator::I64Load16U { memarg } => todo!(),
        Operator::I64Load32S { memarg } => todo!(),
        Operator::I64Load32U { memarg } => todo!(),
        Operator::I32Store { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_store T_i32 None {memarg}")
        }
        Operator::I64Store { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_store T_i64 None {memarg}")
        }
        Operator::F32Store { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_store T_f32 None {memarg}")
        }
        Operator::F64Store { memarg } => {
            let memarg = parse_memarg(&memarg)?;
            format!("BI_store T_f64 None {memarg}")
        }
        Operator::I32Store8 { memarg } => todo!(),
        Operator::I32Store16 { memarg } => todo!(),
        Operator::I64Store8 { memarg } => todo!(),
        Operator::I64Store16 { memarg } => todo!(),
        Operator::I64Store32 { memarg } => todo!(),
        Operator::MemorySize { mem } => todo!(),
        Operator::MemoryGrow { mem } => todo!(),
        Operator::I32Const { value } => format!("BI_const_num (VAL_int32 {value})"),
        Operator::I64Const { value } => format!("BI_const_num (VAL_int64 {value})"),
        Operator::F32Const { value } => {
            let val = value.bits();
            format!("BI_const_num (VAL_float32 {val})")
        }
        Operator::F64Const { value } => {
            let val = value.bits();
            format!("BI_const_num (VAL_float64 {val})")
        }
        Operator::I32Eqz => "BI_testop T_i32 TO_eqz".to_string(),
        Operator::I32Eq => "BI_relop T_i32 Relop_i ROI_eq".to_string(),
        Operator::I32Ne => "BI_relop T_i32 Relop_i ROI_ne".to_string(),
        Operator::I32LtS => "BI_relop T_i32 Relop_i ROI_lt SX_S".to_string(),
        Operator::I32LtU => "BI_relop T_i32 Relop_i ROI_lt SX_U".to_string(),
        Operator::I32GtS => "BI_relop T_i32 Relop_i ROI_gt SX_S".to_string(),
        Operator::I32GtU => "BI_relop T_i32 Relop_i ROI_gt SX_U".to_string(),
        Operator::I32LeS => "BI_relop T_i32 Relop_i ROI_le SX_S".to_string(),
        Operator::I32LeU => "BI_relop T_i32 Relop_i ROI_le SX_U".to_string(),
        Operator::I32GeS => "BI_relop T_i32 Relop_i ROI_ge SX_S".to_string(),
        Operator::I32GeU => "BI_relop T_i32 Relop_i ROI_ge SX_U".to_string(),
        Operator::I64Eqz => "BI_testop T_i64 TO_eqz".to_string(),
        Operator::I64Eq => "BI_relop T_i64 Relop_i ROI_eq".to_string(),
        Operator::I64Ne => "BI_relop T_i64 Relop_i ROI_ne".to_string(),
        Operator::I64LtS => "BI_relop T_i64 Relop_i ROI_lt SX_S".to_string(),
        Operator::I64LtU => "BI_relop T_i64 Relop_i ROI_lt SX_U".to_string(),
        Operator::I64GtS => "BI_relop T_i64 Relop_i ROI_gt SX_S".to_string(),
        Operator::I64GtU => "BI_relop T_i64 Relop_i ROI_gt SX_U".to_string(),
        Operator::I64LeS => "BI_relop T_i64 Relop_i ROI_le SX_S".to_string(),
        Operator::I64LeU => "BI_relop T_i64 Relop_i ROI_le SX_U".to_string(),
        Operator::I64GeS => "BI_relop T_i64 Relop_i ROI_ge SX_S".to_string(),
        Operator::I64GeU => "BI_relop T_i64 Relop_i ROI_ge SX_U".to_string(),
        Operator::F32Eq => "BI_relop T_f32 relop_f ROI_eq".to_string(),
        Operator::F32Ne => "BI_relop T_f32 relop_f ROI_ne".to_string(),
        Operator::F32Lt => "BI_relop T_f32 relop_f ROI_lt".to_string(),
        Operator::F32Gt => "BI_relop T_f32 relop_f ROI_gt".to_string(),
        Operator::F32Le => "BI_relop T_f32 relop_f ROI_le".to_string(),
        Operator::F32Ge => "BI_relop T_f32 relop_f ROI_ge".to_string(),
        Operator::F64Eq => "BI_relop T_f64 relop_f ROI_eq".to_string(),
        Operator::F64Ne => "BI_relop T_f64 relop_f ROI_ne".to_string(),
        Operator::F64Lt => "BI_relop T_f64 relop_f ROI_lt".to_string(),
        Operator::F64Gt => "BI_relop T_f64 relop_f ROI_gt".to_string(),
        Operator::F64Le => "BI_relop T_f64 relop_f ROI_le".to_string(),
        Operator::F64Ge => "BI_relop T_f64 relop_f ROI_ge".to_string(),
        Operator::I32Clz => "BI_unop T_i32 Unop_i UOI_clz".to_string(),
        Operator::I32Ctz => "BI_unop T_i32 Unop_i UOI_ctz".to_string(),
        Operator::I32Popcnt => "BI_unop T_i32 Unop_i UOI_popcnt".to_string(),
        Operator::I32Add => "BI_binop T_i32 binop_i BOI_add".to_string(),
        Operator::I32Sub => "BI_binop T_i32 binop_i BOI_sub".to_string(),
        Operator::I32Mul => "BI_binop T_i32 binop_i BOI_mul".to_string(),
        Operator::I32DivS => "BI_binop T_i32 binop_i BOI_div SX_S".to_string(),
        Operator::I32DivU => "BI_binop T_i32 binop_i BOI_div SX_U".to_string(),
        Operator::I32RemS => "BI_binop T_i32 binop_i BOI_rem SX_S".to_string(),
        Operator::I32RemU => "BI_binop T_i32 binop_i BOI_rem SX_U".to_string(),
        Operator::I32And => "BI_binop T_i32 binop_i BOI_and".to_string(),
        Operator::I32Or => "BI_binop T_i32 binop_i BOI_or".to_string(),
        Operator::I32Xor => "BI_binop T_i32 binop_i BOI_xor".to_string(),
        Operator::I32Shl => "BI_binop T_i32 binop_i BOI_shl".to_string(),
        Operator::I32ShrS => "BI_binop T_i32 binop_i BOI_shr SX_S".to_string(),
        Operator::I32ShrU => "BI_binop T_i32 binop_i BOI_shr SX_U".to_string(),
        Operator::I32Rotl => "BI_binop T_i32 binop_i BOI_rotl".to_string(),
        Operator::I32Rotr => "BI_binop T_i32 binop_i BOI_rotr".to_string(),
        Operator::I64Clz => "BI_unop T_i64 Unop_i UOI_clz".to_string(),
        Operator::I64Ctz => "BI_unop T_i64 Unop_i UOI_ctz".to_string(),
        Operator::I64Popcnt => "BI_unop T_i64 Unop_i UOI_popcnt".to_string(),
        Operator::I64Add => "BI_binop T_i64 binop_i BOI_add".to_string(),
        Operator::I64Sub => "BI_binop T_i64 binop_i BOI_sub".to_string(),
        Operator::I64Mul => "BI_binop T_i64 binop_i BOI_mul".to_string(),
        Operator::I64DivS => "BI_binop T_i64 binop_i BOI_div SX_S".to_string(),
        Operator::I64DivU => "BI_binop T_i64 binop_i BOI_div SX_U".to_string(),
        Operator::I64RemS => "BI_binop T_i64 binop_i BOI_rem SX_S".to_string(),
        Operator::I64RemU => "BI_binop T_i64 binop_i BOI_rem SX_U".to_string(),
        Operator::I64And => "BI_binop T_i64 binop_i BOI_and".to_string(),
        Operator::I64Or => "BI_binop T_i64 binop_i BOI_or".to_string(),
        Operator::I64Xor => "BI_binop T_i64 binop_i BOI_xor".to_string(),
        Operator::I64Shl => "BI_binop T_i64 binop_i BOI_shl".to_string(),
        Operator::I64ShrS => "BI_binop T_i64 binop_i BOI_shr SX_S".to_string(),
        Operator::I64ShrU => "BI_binop T_i64 binop_i BOI_shr SX_U".to_string(),
        Operator::I64Rotl => "BI_binop T_i64 binop_i BOI_rotl".to_string(),
        Operator::I64Rotr => "BI_binop T_i64 binop_i BOI_rotr".to_string(),
        Operator::F32Abs => "BI_unop T_f32 Unop_f UOF_abs".to_string(),
        Operator::F32Neg => "BI_unop T_f32 Unop_f UOF_neg".to_string(),
        Operator::F32Ceil => "BI_unop T_f32 Unop_f UOF_ceil".to_string(),
        Operator::F32Floor => "BI_unop T_f32 Unop_f UOF_floor".to_string(),
        Operator::F32Trunc => "BI_unop T_f32 Unop_f UOF_trunc".to_string(),
        Operator::F32Nearest => "BI_unop T_f32 Unop_f UOF_nearest".to_string(),
        Operator::F32Sqrt => "BI_unop T_f32 Unop_f UOF_sqrt".to_string(),
        Operator::F32Add => "BI_binop T_f32 binop_f BOF_add".to_string(),
        Operator::F32Sub => "BI_binop T_f32 binop_f BOF_sub".to_string(),
        Operator::F32Mul => "BI_binop T_f32 binop_f BOF_mul".to_string(),
        Operator::F32Div => "BI_binop T_f32 binop_f BOF_div".to_string(),
        Operator::F32Min => "BI_binop T_f32 binop_f BOF_min".to_string(),
        Operator::F32Max => "BI_binop T_f32 binop_f BOF_max".to_string(),
        Operator::F32Copysign => "BI_binop T_f32 binop_f BOF_copysign".to_string(),
        Operator::F64Abs => "BI_unop T_f64 Unop_f UOF_abs".to_string(),
        Operator::F64Neg => "BI_unop T_f64 Unop_f UOF_neg".to_string(),
        Operator::F64Ceil => "BI_unop T_f64 Unop_f UOF_ceil".to_string(),
        Operator::F64Floor => "BI_unop T_f64 Unop_f UOF_floor".to_string(),
        Operator::F64Trunc => "BI_unop T_f64 Unop_f UOF_trunc".to_string(),
        Operator::F64Nearest => "BI_unop T_f64 Unop_f UOF_nearest".to_string(),
        Operator::F64Sqrt => "BI_unop T_f64 Unop_f UOF_sqrt".to_string(),
        Operator::F64Add => "BI_binop T_f64 binop_f BOF_add".to_string(),
        Operator::F64Sub => "BI_binop T_f64 binop_f BOF_sub".to_string(),
        Operator::F64Mul => "BI_binop T_f64 binop_f BOF_mul".to_string(),
        Operator::F64Div => "BI_binop T_f64 binop_f BOF_div".to_string(),
        Operator::F64Min => "BI_binop T_f64 binop_f BOF_min".to_string(),
        Operator::F64Max => "BI_binop T_f64 binop_f BOF_max".to_string(),
        Operator::F64Copysign => "BI_binop T_f64 binop_f BOF_copysign".to_string(),
        Operator::I32WrapI64 => "BI_cvtop T_i32 CVO_wrap T_i64 None".to_string(),
        Operator::I32TruncF32S => "BI_cvtop T_i32 CVO_trunc T_f32 (Some SX_S)".to_string(),
        Operator::I32TruncF32U => "BI_cvtop T_i32 CVO_trunc T_f32 (Some SX_U)".to_string(),
        Operator::I32TruncF64S => "BI_cvtop T_i32 CVO_trunc T_f64 (Some SX_S)".to_string(),
        Operator::I32TruncF64U => "BI_cvtop T_i32 CVO_trunc T_f64 (Some SX_U)".to_string(),
        Operator::I64ExtendI32S => "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_S)".to_string(),
        Operator::I64ExtendI32U => "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_U)".to_string(),
        Operator::I64TruncF32S => "BI_cvtop T_i64 CVO_trunc T_f32 (Some SX_S)".to_string(),
        Operator::I64TruncF32U => "BI_cvtop T_i64 CVO_trunc T_f32 (Some SX_U)".to_string(),
        Operator::I64TruncF64S => "BI_cvtop T_i64 CVO_trunc T_f64 (Some SX_S)".to_string(),
        Operator::I64TruncF64U => "BI_cvtop T_i64 CVO_trunc T_f64 (Some SX_U)".to_string(),
        Operator::F32ConvertI32S => "BI_cvtop T_f32 CVO_convert T_i32 (Some SX_S)".to_string(),
        Operator::F32ConvertI32U => "BI_cvtop T_f32 CVO_convert T_i32 (Some SX_U)".to_string(),
        Operator::F32ConvertI64S => "BI_cvtop T_f32 CVO_convert T_i64 (Some SX_S)".to_string(),
        Operator::F32ConvertI64U => "BI_cvtop T_f32 CVO_convert T_i64 (Some SX_U)".to_string(),
        Operator::F32DemoteF64 => "BI_cvtop T_f32 CVO_demote T_f64 None".to_string(),
        Operator::F64ConvertI32S => "BI_cvtop T_f64 CVO_convert T_i32 (Some SX_S)".to_string(),
        Operator::F64ConvertI32U => "BI_cvtop T_f64 CVO_convert T_i32 (Some SX_U)".to_string(),
        Operator::F64ConvertI64S => "BI_cvtop T_f64 CVO_convert T_i64 (Some SX_S)".to_string(),
        Operator::F64ConvertI64U => "BI_cvtop T_f64 CVO_convert T_i64 (Some SX_U)".to_string(),
        Operator::F64PromoteF32 => "BI_cvtop T_f64 CVO_promote T_f32 None".to_string(),
        Operator::I32ReinterpretF32 => "BI_cvtop T_i32 CVO_reinterpret T_f32 None".to_string(),
        Operator::I64ReinterpretF64 => "BI_cvtop T_i64 CVO_reinterpret T_f64 None".to_string(),
        Operator::F32ReinterpretI32 => "BI_cvtop T_f32 CVO_reinterpret T_i32 None".to_string(),
        Operator::F64ReinterpretI64 => "BI_cvtop T_f64 CVO_reinterpret T_i64 None".to_string(),
        Operator::I32Extend8S => todo!(),
        Operator::I32Extend16S => todo!(),
        Operator::I64Extend8S => todo!(),
        Operator::I64Extend16S => todo!(),
        Operator::I64Extend32S => todo!(),
        Operator::RefEq => todo!(),
        Operator::StructNew { struct_type_index } => todo!(),
        Operator::StructNewDefault { struct_type_index } => todo!(),
        Operator::StructGet {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructGetS {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructGetU {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructSet {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::ArrayNew { array_type_index } => todo!(),
        Operator::ArrayNewDefault { array_type_index } => todo!(),
        Operator::ArrayNewFixed {
            array_type_index,
            array_size,
        } => todo!(),
        Operator::ArrayNewData {
            array_type_index,
            array_data_index,
        } => todo!(),
        Operator::ArrayNewElem {
            array_type_index,
            array_elem_index,
        } => todo!(),
        Operator::ArrayGet { array_type_index } => todo!(),
        Operator::ArrayGetS { array_type_index } => todo!(),
        Operator::ArrayGetU { array_type_index } => todo!(),
        Operator::ArraySet { array_type_index } => todo!(),
        Operator::ArrayLen => todo!(),
        Operator::ArrayFill { array_type_index } => todo!(),
        Operator::ArrayCopy {
            array_type_index_dst,
            array_type_index_src,
        } => todo!(),
        Operator::ArrayInitData {
            array_type_index,
            array_data_index,
        } => todo!(),
        Operator::ArrayInitElem {
            array_type_index,
            array_elem_index,
        } => todo!(),
        Operator::RefTestNonNull { hty } => todo!(),
        Operator::RefTestNullable { hty } => todo!(),
        Operator::RefCastNonNull { hty } => todo!(),
        Operator::RefCastNullable { hty } => todo!(),
        Operator::BrOnCast {
            relative_depth,
            from_ref_type,
            to_ref_type,
        } => todo!(),
        Operator::BrOnCastFail {
            relative_depth,
            from_ref_type,
            to_ref_type,
        } => todo!(),
        Operator::AnyConvertExtern => todo!(),
        Operator::ExternConvertAny => todo!(),
        Operator::RefI31 => todo!(),
        Operator::I31GetS => todo!(),
        Operator::I31GetU => todo!(),
        Operator::I32TruncSatF32S => todo!(),
        Operator::I32TruncSatF32U => todo!(),
        Operator::I32TruncSatF64S => todo!(),
        Operator::I32TruncSatF64U => todo!(),
        Operator::I64TruncSatF32S => todo!(),
        Operator::I64TruncSatF32U => todo!(),
        Operator::I64TruncSatF64S => todo!(),
        Operator::I64TruncSatF64U => todo!(),
        Operator::MemoryInit { data_index, mem } => todo!(),
        Operator::DataDrop { data_index } => todo!(),
        Operator::MemoryCopy { dst_mem, src_mem } => todo!(),
        Operator::MemoryFill { mem } => todo!(),
        Operator::TableInit { elem_index, table } => todo!(),
        Operator::ElemDrop { elem_index } => todo!(),
        Operator::TableCopy {
            dst_table,
            src_table,
        } => todo!(),
        Operator::TypedSelect { ty } => todo!(),
        Operator::RefNull { hty } => todo!(),
        Operator::RefIsNull => "BI_ref_is_null".to_string(),
        Operator::RefFunc { function_index } => format!("BI_ref_func {function_index}"),
        Operator::TableFill { table } => format!("BI_table_fill {table}"),
        Operator::TableGet { table } => format!("BI_table_get {table}"),
        Operator::TableSet { table } => format!("BI_table_set {table}"),
        Operator::TableGrow { table } => format!("BI_table_grow {table}"),
        Operator::TableSize { table } => format!("BI_table_size {table}"),
        Operator::ReturnCall { function_index } => todo!(),
        Operator::ReturnCallIndirect {
            type_index,
            table_index,
        } => todo!(),
        Operator::MemoryDiscard { mem } => todo!(),
        Operator::MemoryAtomicNotify { memarg: _ }
        | Operator::MemoryAtomicWait32 { memarg: _ }
        | Operator::MemoryAtomicWait64 { memarg: _ }
        | Operator::AtomicFence
        | Operator::I32AtomicLoad { memarg: _ }
        | Operator::I64AtomicLoad { memarg: _ }
        | Operator::I32AtomicLoad8U { memarg: _ }
        | Operator::I32AtomicLoad16U { memarg: _ }
        | Operator::I64AtomicLoad8U { memarg: _ }
        | Operator::I64AtomicLoad16U { memarg: _ }
        | Operator::I64AtomicLoad32U { memarg: _ }
        | Operator::I32AtomicStore { memarg: _ }
        | Operator::I64AtomicStore { memarg: _ }
        | Operator::I32AtomicStore8 { memarg: _ }
        | Operator::I32AtomicStore16 { memarg: _ }
        | Operator::I64AtomicStore8 { memarg: _ }
        | Operator::I64AtomicStore16 { memarg: _ }
        | Operator::I64AtomicStore32 { memarg: _ }
        | Operator::I32AtomicRmwAdd { memarg: _ }
        | Operator::I64AtomicRmwAdd { memarg: _ }
        | Operator::I32AtomicRmw8AddU { memarg: _ }
        | Operator::I32AtomicRmw16AddU { memarg: _ }
        | Operator::I64AtomicRmw8AddU { memarg: _ }
        | Operator::I64AtomicRmw16AddU { memarg: _ }
        | Operator::I64AtomicRmw32AddU { memarg: _ }
        | Operator::I32AtomicRmwSub { memarg: _ }
        | Operator::I64AtomicRmwSub { memarg: _ }
        | Operator::I32AtomicRmw8SubU { memarg: _ }
        | Operator::I32AtomicRmw16SubU { memarg: _ }
        | Operator::I64AtomicRmw8SubU { memarg: _ }
        | Operator::I64AtomicRmw16SubU { memarg: _ }
        | Operator::I64AtomicRmw32SubU { memarg: _ }
        | Operator::I32AtomicRmwAnd { memarg: _ }
        | Operator::I64AtomicRmwAnd { memarg: _ }
        | Operator::I32AtomicRmw8AndU { memarg: _ }
        | Operator::I32AtomicRmw16AndU { memarg: _ }
        | Operator::I64AtomicRmw8AndU { memarg: _ }
        | Operator::I64AtomicRmw16AndU { memarg: _ }
        | Operator::I64AtomicRmw32AndU { memarg: _ }
        | Operator::I32AtomicRmwOr { memarg: _ }
        | Operator::I64AtomicRmwOr { memarg: _ }
        | Operator::I32AtomicRmw8OrU { memarg: _ }
        | Operator::I32AtomicRmw16OrU { memarg: _ }
        | Operator::I64AtomicRmw8OrU { memarg: _ }
        | Operator::I64AtomicRmw16OrU { memarg: _ }
        | Operator::I64AtomicRmw32OrU { memarg: _ }
        | Operator::I32AtomicRmwXor { memarg: _ }
        | Operator::I64AtomicRmwXor { memarg: _ }
        | Operator::I32AtomicRmw8XorU { memarg: _ }
        | Operator::I32AtomicRmw16XorU { memarg: _ }
        | Operator::I64AtomicRmw8XorU { memarg: _ }
        | Operator::I64AtomicRmw16XorU { memarg: _ }
        | Operator::I64AtomicRmw32XorU { memarg: _ }
        | Operator::I32AtomicRmwXchg { memarg: _ }
        | Operator::I64AtomicRmwXchg { memarg: _ }
        | Operator::I32AtomicRmw8XchgU { memarg: _ }
        | Operator::I32AtomicRmw16XchgU { memarg: _ }
        | Operator::I64AtomicRmw8XchgU { memarg: _ }
        | Operator::I64AtomicRmw16XchgU { memarg: _ }
        | Operator::I64AtomicRmw32XchgU { memarg: _ }
        | Operator::I32AtomicRmwCmpxchg { memarg: _ }
        | Operator::I64AtomicRmwCmpxchg { memarg: _ }
        | Operator::I32AtomicRmw8CmpxchgU { memarg: _ }
        | Operator::I32AtomicRmw16CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw8CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw16CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw32CmpxchgU { memarg: _ } => {
            return Err(anyhow::anyhow!(
                "Atomic instruction {:?} are not supported",
                operator
            ))
        }
        /*
                        Inductive shape_vec_i: Set :=
                  | SVI_8_16
                  | SVI_16_8
                  | SVI_32_4
                  | SVI_64_2
                  .

                Inductive shape_vec_f: Set :=
                  | SVF_32_4
                  | SVF_64_2
                  .

                Inductive shape_vec : Set := (* shape *)
                  | SV_ishape: shape_vec_i -> shape_vec
                  | SV_fshape: shape_vec_f -> shape_vec
                  .

                  Inductive unop_vec : Set :=
          | VUO_not
          .

        Inductive binop_vec : Set :=
          | VBO_and
          .

        Inductive ternop_vec : Set :=
          | VTO_bitselect
          .

        Inductive test_vec : Set :=
          | VT_any_true
          .

        Inductive shift_vec : Set :=
          | VSH_any_true
          .

        Definition laneidx := u8.

        Inductive packed_type_vec :=
          | Tptv_8_8
          | Tptv_16_4
          | Tptv_32_2
        .

        Inductive zero_type_vec :=
          | Tztv_32
          | Tztv_64
        .

        Inductive width_vec :=
          | Twv_8
          | Twv_16
          | Twv_32
          | Twv_64
          .

        Inductive load_vec_arg :=
          | LVA_packed: packed_type_vec -> sx -> load_vec_arg
          | LVA_zero: zero_type_vec -> load_vec_arg
          | LVA_splat: width_vec -> load_vec_arg
          .
                         */
        Operator::V128Load { memarg } => todo!(),
        Operator::V128Load8x8S { memarg } => todo!(),
        Operator::V128Load8x8U { memarg } => todo!(),
        Operator::V128Load16x4S { memarg } => todo!(),
        Operator::V128Load16x4U { memarg } => todo!(),
        Operator::V128Load32x2S { memarg } => todo!(),
        Operator::V128Load32x2U { memarg } => todo!(),
        Operator::V128Load8Splat { memarg } => todo!(),
        Operator::V128Load16Splat { memarg } => todo!(),
        Operator::V128Load32Splat { memarg } => todo!(),
        Operator::V128Load64Splat { memarg } => todo!(),
        Operator::V128Load32Zero { memarg } => todo!(),
        Operator::V128Load64Zero { memarg } => todo!(),
        Operator::V128Store { memarg } => todo!(),
        Operator::V128Load8Lane { memarg, lane } => todo!(),
        Operator::V128Load16Lane { memarg, lane } => todo!(),
        Operator::V128Load32Lane { memarg, lane } => todo!(),
        Operator::V128Load64Lane { memarg, lane } => todo!(),
        Operator::V128Store8Lane { memarg, lane } => todo!(),
        Operator::V128Store16Lane { memarg, lane } => todo!(),
        Operator::V128Store32Lane { memarg, lane } => todo!(),
        Operator::V128Store64Lane { memarg, lane } => todo!(),
        Operator::V128Const { value } => todo!(),
        Operator::I8x16Shuffle { lanes } => todo!(),
        Operator::I8x16ExtractLaneS { lane } => todo!(),
        Operator::I8x16ExtractLaneU { lane } => todo!(),
        Operator::I8x16ReplaceLane { lane } => todo!(),
        Operator::I16x8ExtractLaneS { lane } => todo!(),
        Operator::I16x8ExtractLaneU { lane } => todo!(),
        Operator::I16x8ReplaceLane { lane } => todo!(),
        Operator::I32x4ExtractLane { lane } => todo!(),
        Operator::I32x4ReplaceLane { lane } => todo!(),
        Operator::I64x2ExtractLane { lane } => todo!(),
        Operator::I64x2ReplaceLane { lane } => todo!(),
        Operator::F32x4ExtractLane { lane } => todo!(),
        Operator::F32x4ReplaceLane { lane } => todo!(),
        Operator::F64x2ExtractLane { lane } => todo!(),
        Operator::F64x2ReplaceLane { lane } => todo!(),
        Operator::I8x16Swizzle => todo!(),
        Operator::I8x16Splat => todo!(),
        Operator::I16x8Splat => todo!(),
        Operator::I32x4Splat => todo!(),
        Operator::I64x2Splat => todo!(),
        Operator::F32x4Splat => todo!(),
        Operator::F64x2Splat => todo!(),
        Operator::I8x16Eq => todo!(),
        Operator::I8x16Ne => todo!(),
        Operator::I8x16LtS => todo!(),
        Operator::I8x16LtU => todo!(),
        Operator::I8x16GtS => todo!(),
        Operator::I8x16GtU => todo!(),
        Operator::I8x16LeS => todo!(),
        Operator::I8x16LeU => todo!(),
        Operator::I8x16GeS => todo!(),
        Operator::I8x16GeU => todo!(),
        Operator::I16x8Eq => todo!(),
        Operator::I16x8Ne => todo!(),
        Operator::I16x8LtS => todo!(),
        Operator::I16x8LtU => todo!(),
        Operator::I16x8GtS => todo!(),
        Operator::I16x8GtU => todo!(),
        Operator::I16x8LeS => todo!(),
        Operator::I16x8LeU => todo!(),
        Operator::I16x8GeS => todo!(),
        Operator::I16x8GeU => todo!(),
        Operator::I32x4Eq => todo!(),
        Operator::I32x4Ne => todo!(),
        Operator::I32x4LtS => todo!(),
        Operator::I32x4LtU => todo!(),
        Operator::I32x4GtS => todo!(),
        Operator::I32x4GtU => todo!(),
        Operator::I32x4LeS => todo!(),
        Operator::I32x4LeU => todo!(),
        Operator::I32x4GeS => todo!(),
        Operator::I32x4GeU => todo!(),
        Operator::I64x2Eq => todo!(),
        Operator::I64x2Ne => todo!(),
        Operator::I64x2LtS => todo!(),
        Operator::I64x2GtS => todo!(),
        Operator::I64x2LeS => todo!(),
        Operator::I64x2GeS => todo!(),
        Operator::F32x4Eq => todo!(),
        Operator::F32x4Ne => todo!(),
        Operator::F32x4Lt => todo!(),
        Operator::F32x4Gt => todo!(),
        Operator::F32x4Le => todo!(),
        Operator::F32x4Ge => todo!(),
        Operator::F64x2Eq => todo!(),
        Operator::F64x2Ne => todo!(),
        Operator::F64x2Lt => todo!(),
        Operator::F64x2Gt => todo!(),
        Operator::F64x2Le => todo!(),
        Operator::F64x2Ge => todo!(),
        Operator::V128Not => todo!(),
        Operator::V128And => todo!(),
        Operator::V128AndNot => todo!(),
        Operator::V128Or => todo!(),
        Operator::V128Xor => todo!(),
        Operator::V128Bitselect => todo!(),
        Operator::V128AnyTrue => todo!(),
        Operator::I8x16Abs => todo!(),
        Operator::I8x16Neg => todo!(),
        Operator::I8x16Popcnt => todo!(),
        Operator::I8x16AllTrue => todo!(),
        Operator::I8x16Bitmask => todo!(),
        Operator::I8x16NarrowI16x8S => todo!(),
        Operator::I8x16NarrowI16x8U => todo!(),
        Operator::I8x16Shl => todo!(),
        Operator::I8x16ShrS => todo!(),
        Operator::I8x16ShrU => todo!(),
        Operator::I8x16Add => todo!(),
        Operator::I8x16AddSatS => todo!(),
        Operator::I8x16AddSatU => todo!(),
        Operator::I8x16Sub => todo!(),
        Operator::I8x16SubSatS => todo!(),
        Operator::I8x16SubSatU => todo!(),
        Operator::I8x16MinS => todo!(),
        Operator::I8x16MinU => todo!(),
        Operator::I8x16MaxS => todo!(),
        Operator::I8x16MaxU => todo!(),
        Operator::I8x16AvgrU => todo!(),
        Operator::I16x8ExtAddPairwiseI8x16S => todo!(),
        Operator::I16x8ExtAddPairwiseI8x16U => todo!(),
        Operator::I16x8Abs => todo!(),
        Operator::I16x8Neg => todo!(),
        Operator::I16x8Q15MulrSatS => todo!(),
        Operator::I16x8AllTrue => todo!(),
        Operator::I16x8Bitmask => todo!(),
        Operator::I16x8NarrowI32x4S => todo!(),
        Operator::I16x8NarrowI32x4U => todo!(),
        Operator::I16x8ExtendLowI8x16S => todo!(),
        Operator::I16x8ExtendHighI8x16S => todo!(),
        Operator::I16x8ExtendLowI8x16U => todo!(),
        Operator::I16x8ExtendHighI8x16U => todo!(),
        Operator::I16x8Shl => todo!(),
        Operator::I16x8ShrS => todo!(),
        Operator::I16x8ShrU => todo!(),
        Operator::I16x8Add => todo!(),
        Operator::I16x8AddSatS => todo!(),
        Operator::I16x8AddSatU => todo!(),
        Operator::I16x8Sub => todo!(),
        Operator::I16x8SubSatS => todo!(),
        Operator::I16x8SubSatU => todo!(),
        Operator::I16x8Mul => todo!(),
        Operator::I16x8MinS => todo!(),
        Operator::I16x8MinU => todo!(),
        Operator::I16x8MaxS => todo!(),
        Operator::I16x8MaxU => todo!(),
        Operator::I16x8AvgrU => todo!(),
        Operator::I16x8ExtMulLowI8x16S => todo!(),
        Operator::I16x8ExtMulHighI8x16S => todo!(),
        Operator::I16x8ExtMulLowI8x16U => todo!(),
        Operator::I16x8ExtMulHighI8x16U => todo!(),
        Operator::I32x4ExtAddPairwiseI16x8S => todo!(),
        Operator::I32x4ExtAddPairwiseI16x8U => todo!(),
        Operator::I32x4Abs => todo!(),
        Operator::I32x4Neg => todo!(),
        Operator::I32x4AllTrue => todo!(),
        Operator::I32x4Bitmask => todo!(),
        Operator::I32x4ExtendLowI16x8S => todo!(),
        Operator::I32x4ExtendHighI16x8S => todo!(),
        Operator::I32x4ExtendLowI16x8U => todo!(),
        Operator::I32x4ExtendHighI16x8U => todo!(),
        Operator::I32x4Shl => todo!(),
        Operator::I32x4ShrS => todo!(),
        Operator::I32x4ShrU => todo!(),
        Operator::I32x4Add => todo!(),
        Operator::I32x4Sub => todo!(),
        Operator::I32x4Mul => todo!(),
        Operator::I32x4MinS => todo!(),
        Operator::I32x4MinU => todo!(),
        Operator::I32x4MaxS => todo!(),
        Operator::I32x4MaxU => todo!(),
        Operator::I32x4DotI16x8S => todo!(),
        Operator::I32x4ExtMulLowI16x8S => todo!(),
        Operator::I32x4ExtMulHighI16x8S => todo!(),
        Operator::I32x4ExtMulLowI16x8U => todo!(),
        Operator::I32x4ExtMulHighI16x8U => todo!(),
        Operator::I64x2Abs => todo!(),
        Operator::I64x2Neg => todo!(),
        Operator::I64x2AllTrue => todo!(),
        Operator::I64x2Bitmask => todo!(),
        Operator::I64x2ExtendLowI32x4S => todo!(),
        Operator::I64x2ExtendHighI32x4S => todo!(),
        Operator::I64x2ExtendLowI32x4U => todo!(),
        Operator::I64x2ExtendHighI32x4U => todo!(),
        Operator::I64x2Shl => todo!(),
        Operator::I64x2ShrS => todo!(),
        Operator::I64x2ShrU => todo!(),
        Operator::I64x2Add => todo!(),
        Operator::I64x2Sub => todo!(),
        Operator::I64x2Mul => todo!(),
        Operator::I64x2ExtMulLowI32x4S => todo!(),
        Operator::I64x2ExtMulHighI32x4S => todo!(),
        Operator::I64x2ExtMulLowI32x4U => todo!(),
        Operator::I64x2ExtMulHighI32x4U => todo!(),
        Operator::F32x4Ceil => todo!(),
        Operator::F32x4Floor => todo!(),
        Operator::F32x4Trunc => todo!(),
        Operator::F32x4Nearest => todo!(),
        Operator::F32x4Abs => todo!(),
        Operator::F32x4Neg => todo!(),
        Operator::F32x4Sqrt => todo!(),
        Operator::F32x4Add => todo!(),
        Operator::F32x4Sub => todo!(),
        Operator::F32x4Mul => todo!(),
        Operator::F32x4Div => todo!(),
        Operator::F32x4Min => todo!(),
        Operator::F32x4Max => todo!(),
        Operator::F32x4PMin => todo!(),
        Operator::F32x4PMax => todo!(),
        Operator::F64x2Ceil => todo!(),
        Operator::F64x2Floor => todo!(),
        Operator::F64x2Trunc => todo!(),
        Operator::F64x2Nearest => todo!(),
        Operator::F64x2Abs => todo!(),
        Operator::F64x2Neg => todo!(),
        Operator::F64x2Sqrt => todo!(),
        Operator::F64x2Add => todo!(),
        Operator::F64x2Sub => todo!(),
        Operator::F64x2Mul => todo!(),
        Operator::F64x2Div => todo!(),
        Operator::F64x2Min => todo!(),
        Operator::F64x2Max => todo!(),
        Operator::F64x2PMin => todo!(),
        Operator::F64x2PMax => todo!(),
        Operator::I32x4TruncSatF32x4S => todo!(),
        Operator::I32x4TruncSatF32x4U => todo!(),
        Operator::F32x4ConvertI32x4S => todo!(),
        Operator::F32x4ConvertI32x4U => todo!(),
        Operator::I32x4TruncSatF64x2SZero => todo!(),
        Operator::I32x4TruncSatF64x2UZero => todo!(),
        Operator::F64x2ConvertLowI32x4S => todo!(),
        Operator::F64x2ConvertLowI32x4U => todo!(),
        Operator::F32x4DemoteF64x2Zero => todo!(),
        Operator::F64x2PromoteLowF32x4 => todo!(),
        Operator::I8x16RelaxedSwizzle => todo!(),
        Operator::I32x4RelaxedTruncF32x4S => todo!(),
        Operator::I32x4RelaxedTruncF32x4U => todo!(),
        Operator::I32x4RelaxedTruncF64x2SZero => todo!(),
        Operator::I32x4RelaxedTruncF64x2UZero => todo!(),
        Operator::F32x4RelaxedMadd => todo!(),
        Operator::F32x4RelaxedNmadd => todo!(),
        Operator::F64x2RelaxedMadd => todo!(),
        Operator::F64x2RelaxedNmadd => todo!(),
        Operator::I8x16RelaxedLaneselect => todo!(),
        Operator::I16x8RelaxedLaneselect => todo!(),
        Operator::I32x4RelaxedLaneselect => todo!(),
        Operator::I64x2RelaxedLaneselect => todo!(),
        Operator::F32x4RelaxedMin => todo!(),
        Operator::F32x4RelaxedMax => todo!(),
        Operator::F64x2RelaxedMin => todo!(),
        Operator::F64x2RelaxedMax => todo!(),
        Operator::I16x8RelaxedQ15mulrS => todo!(),
        Operator::I16x8RelaxedDotI8x16I7x16S => todo!(),
        Operator::I32x4RelaxedDotI8x16I7x16AddS => todo!(),
        Operator::TryTable { try_table } => todo!(),
        Operator::Throw { tag_index } => todo!(),
        Operator::ThrowRef => todo!(),
        Operator::Try { blockty } => todo!(),
        Operator::Catch { tag_index } => todo!(),
        Operator::Rethrow { relative_depth } => todo!(),
        Operator::Delegate { relative_depth } => todo!(),
        Operator::CatchAll => todo!(),
        Operator::GlobalAtomicGet {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicSet {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwAdd {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwSub {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwAnd {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwOr {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwXor {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwXchg {
            ordering,
            global_index,
        } => todo!(),
        Operator::GlobalAtomicRmwCmpxchg {
            ordering,
            global_index,
        } => todo!(),
        Operator::TableAtomicGet {
            ordering,
            table_index,
        } => todo!(),
        Operator::TableAtomicSet {
            ordering,
            table_index,
        } => todo!(),
        Operator::TableAtomicRmwXchg {
            ordering,
            table_index,
        } => todo!(),
        Operator::TableAtomicRmwCmpxchg {
            ordering,
            table_index,
        } => todo!(),
        Operator::StructAtomicGet {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicGetS {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicGetU {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicSet {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwAdd {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwSub {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwAnd {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwOr {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwXor {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwXchg {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructAtomicRmwCmpxchg {
            ordering,
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::ArrayAtomicGet {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicGetS {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicGetU {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicSet {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwAdd {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwSub {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwAnd {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwOr {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwXor {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwXchg {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::ArrayAtomicRmwCmpxchg {
            ordering,
            array_type_index,
        } => todo!(),
        Operator::RefI31Shared => todo!(),
        Operator::CallRef { type_index } => todo!(),
        Operator::ReturnCallRef { type_index } => todo!(),
        Operator::RefAsNonNull => todo!(),
        Operator::BrOnNull { relative_depth } => todo!(),
        Operator::BrOnNonNull { relative_depth } => todo!(),
        Operator::ContNew { cont_type_index } => todo!(),
        Operator::ContBind {
            argument_index,
            result_index,
        } => todo!(),
        Operator::Suspend { tag_index } => todo!(),
        Operator::Resume {
            cont_type_index,
            resume_table,
        } => todo!(),
        Operator::ResumeThrow {
            cont_type_index,
            tag_index,
            resume_table,
        } => todo!(),
        Operator::Switch {
            cont_type_index,
            tag_index,
        } => todo!(),
        Operator::I64Add128 => todo!(),
        Operator::I64Sub128 => todo!(),
        Operator::I64MulWideS => todo!(),
        Operator::I64MulWideU => todo!(),
        Operator::Unreachable => todo!(),
        Operator::Nop => todo!(),
        Operator::Block { blockty } => todo!(),
        _ => return Err(anyhow::anyhow!("Operator {:?} not recognized", operator)),
    };
    Ok(operator.to_string())
}

//Record module_data
fn translate_data(data: &Data) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let moddata_mode = translate_module_datamode(data)?;
    let mut moddata_init = String::new();
    for byte in data.data {
        if *byte < 0x10 {
            moddata_init.push_str(format!("x0{byte:x}").as_str());
        } else {
            moddata_init.push_str(&format!("{byte:#2x?}")[1..]);
        }
        moddata_init.push_str(" :: ");
    }
    moddata_init.push_str("nil");
    res.push_str(format!("Definition moddata_{id} : module_data :=\n").as_str());
    res.push_str(RLB);
    res.push_str(format!("moddata_init := {moddata_init};\n").as_str());
    res.push_str(format!("moddata_mode := {moddata_mode};\n").as_str());
    res.push_str(RRB);
    res.push_str(".\n");
    Ok(res)
}

// fn translate_element(element: &Element) -> Result<(String, String), WasmModuleParseError> {
//     let mut res = String::new();
//     let id = get_id();
//     let name = format!("ElementSegment{id}");

//     res.push_str(format!("Definition {name} : WasmElementSegment :=\n").as_str());
//     res.push_str("{|\n");

//     match &element.items {
//         wasmparser::ElementItems::Expressions(ref_type, expr) => {
//             match *ref_type {
//                 RefType::FUNCREF => {
//                     res.push_str("es_type := rt_func;\n");
//                 }
//                 RefType::EXTERNREF => {
//                     res.push_str("es_type := rt_extern;\n");
//                 }
//                 _ => {}
//             }
//             let mut expression_translated = String::new();
//             for e in expr.clone() {
//                 match translate_operators_reader(e.unwrap().get_operators_reader()) {
//                     Ok(expression) => {
//                         expression_translated.push_str(expression.as_str());
//                     }
//                     Err(e) => {
//                         return Err(WasmModuleParseError::add_string_to_reported_error(
//                             &String::from("Failed to translate element segment expression"),
//                             e,
//                         ));
//                     }
//                 }
//             }

//             res.push_str(format!("es_init := ({expression_translated});\n").as_str());
//         }
//         wasmparser::ElementItems::Functions(indexes) => {
//             let mut index_val = String::new();
//             for index in indexes.clone() {
//                 let index_unwrapped = index.unwrap();
//                 index_val.push_str(format!("{index_unwrapped}").as_str());
//             }
//             res.push_str("es_type := rt_func;\n");
//             res.push_str(
//                 format!("es_init := (i_reference (ri_ref_func {index_val}) :: nil) :: nil;\n")
//                     .as_str(),
//             );
//         }
//     }

//     match &element.kind {
//         ElementKind::Active {
//             table_index,
//             offset_expr,
//         } => match translate_operators_reader(offset_expr.get_operators_reader()) {
//             Ok(expression) => {
//                 let index = table_index.unwrap_or(0);
//                 res.push_str(format!("es_mode := esm_active {index} ({expression})\n").as_str());
//             }
//             Err(e) => {
//                 return Err(WasmModuleParseError::add_string_to_reported_error(
//                     &String::from("Failed to translate element segment offset expression"),
//                     e,
//                 ));
//             }
//         },
//         ElementKind::Passive => {
//             res.push_str("es_mode := esm_passive\n");
//         }
//         ElementKind::Declared => {
//             res.push_str("es_mode := esm_declarative\n");
//         }
//     }

//     res.push_str("|}.\n\n");
//     Ok((name, res))
// }

// fn translate_rec_group(rec_group: &RecGroup) -> (String, String) {
//     let mut res = String::new();
//     let id = get_id();
//     let name = format!("FuncionType{id}");
//     res.push_str(format!("Definition {name} : WasmFuncionType :=\n").as_str());
//     res.push_str("{|\n");

//     for ty in rec_group.types() {
//         match &ty.composite_type.inner {
//             CompositeInnerType::Func(ft) => {
//                 let mut params_str = String::new();
//                 for param in ft.params() {
//                     let sp = stringify_val_type(*param);
//                     params_str.push_str(format!("{sp} :: ").as_str());
//                 }
//                 params_str.push_str("nil;\n");
//                 res.push_str(format!("ft_params := {params_str}").as_str());

//                 let mut results_str = String::new();
//                 for result in ft.results() {
//                     let sp = stringify_val_type(*result);
//                     results_str.push_str(format!("{sp} :: ").as_str());
//                 }
//                 results_str.push_str("nil;\n");
//                 res.push_str(format!("ft_results := {results_str}").as_str());
//             }
//             CompositeInnerType::Array(_)
//             | CompositeInnerType::Struct(_)
//             | CompositeInnerType::Cont(_) => {
//                 //TODO
//             }
//         }
//     }
//     res.push_str("|}.\n\n");
//     (name, res)
// }

// fn translate_functions(
//     function_type_indexes: &[u32],
//     function_bodies: &[FunctionBody],
// ) -> Result<(Vec<String>, String), WasmModuleParseError> {
//     let mut res = String::new();
//     let mut function_names = Vec::new();
//     for (index, function_body) in function_bodies.iter().enumerate() {
//         let id = get_id();
//         let name = format!("Function{id}");
//         let type_index = *function_type_indexes.get(index).unwrap_or(&0);

//         res.push_str(format!("Definition {name} : WasmFunction :=\n").as_str());
//         res.push_str("{|\n");
//         res.push_str(format!("f_typeidx := {type_index};\n").as_str());
//         let mut locals = String::new();
//         if let Ok(locals_reader) = function_body.get_locals_reader() {
//             for local in locals_reader {
//                 let (_, val_type) = local.unwrap();
//                 let val_type = match val_type {
//                     ValType::I32 => "vt_num nt_i32",
//                     ValType::I64 => "vt_num nt_i64",
//                     ValType::F32 => "vt_num nt_f32",
//                     ValType::F64 => "vt_num nt_f64",
//                     ValType::V128 => "vt_vec vt_v128",
//                     ValType::Ref(ref_type) => match ref_type {
//                         RefType::FUNCREF => "vt_ref rt_func",
//                         RefType::EXTERNREF => "vt_ref rt_extern",
//                         _ => "vt_ref _",
//                     },
//                 };
//                 locals.push_str(format!("{val_type} :: ").as_str());
//             }
//         }
//         locals.push_str("nil");
//         res.push_str(format!("f_locals := {locals};\n").as_str());
//         match translate_operators_reader(function_body.get_operators_reader().unwrap()) {
//             Ok(expression) => {
//                 res.push_str(format!("f_body := {expression}").as_str());
//             }
//             Err(e) => {
//                 return Err(WasmModuleParseError::add_string_to_reported_error(
//                     &String::from("Failed to translate function body"),
//                     e,
//                 ));
//             }
//         }
//         res.push_str("|}.\n");
//         res.push('\n');
//         function_names.push(name);
//     }
//     Ok((function_names, res))
// }

fn get_id() -> String {
    let uuid = Uuid::new_v4().to_string();
    let mut parts = uuid.split('-');
    parts.next().unwrap().to_string()
}

// fn stringify_val_type(val_type: ValType) -> String {
//     match val_type {
//         ValType::I32 => "vt_num nt_i32",
//         ValType::I64 => "vt_num nt_i64",
//         ValType::F32 => "vt_num nt_f32",
//         ValType::F64 => "vt_num nt_f64",
//         ValType::V128 => "vt_vec vt_v128",
//         ValType::Ref(ref_type) => match ref_type {
//             RefType::FUNCREF => "vt_ref rt_func",
//             RefType::EXTERNREF => "vt_ref rt_extern",
//             _ => "vt_ref _",
//         },
//     }
//     .to_string()
// }

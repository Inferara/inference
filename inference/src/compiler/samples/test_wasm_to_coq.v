Require Import String List BinInt BinNat.
From Exetasis Require Import WasmStructure.
Require Import Coq.Init.Byte.
Definition envA : WasmImport :=
{|
i_module := "abort";
i_name := "env";
i_desc := id_func 3 |}.

Definition memory : WasmExport :=
{|
e_name := "memory";
e_desc := ed_mem 0 |}.

Definition __alloc : WasmExport :=
{|
e_name := "__alloc";
e_desc := ed_func 10 |}.

Definition __retain : WasmExport :=
{|
e_name := "__retain";
e_desc := ed_func 12 |}.

Definition __release : WasmExport :=
{|
e_name := "__release";
e_desc := ed_func 21 |}.

Definition __collect : WasmExport :=
{|
e_name := "__collect";
e_desc := ed_func 26 |}.

Definition __rtti_base : WasmExport :=
{|
e_name := "__rtti_base";
e_desc := ed_global 9 |}.

Definition INPUT_BUFFER_POINTER : WasmExport :=
{|
e_name := "INPUT_BUFFER_POINTER";
e_desc := ed_global 5 |}.

Definition INPUT_BUFFER_SIZE : WasmExport :=
{|
e_name := "INPUT_BUFFER_SIZE";
e_desc := ed_global 6 |}.

Definition OUTPUT_BUFFER_POINTER : WasmExport :=
{|
e_name := "OUTPUT_BUFFER_POINTER";
e_desc := ed_global 7 |}.

Definition OUTPUT_BUFFER_SIZE : WasmExport :=
{|
e_name := "OUTPUT_BUFFER_SIZE";
e_desc := ed_global 8 |}.

Definition amplifyAudioInBuffer : WasmExport :=
{|
e_name := "amplifyAudioInBuffer";
e_desc := ed_func 28 |}.

Definition Table1dcb6204 : WasmTableType :=
{|
tt_limits := {| l_min := 4; l_max := None |};
tt_reftype := rt_func
|}.

Definition MemTypefe434bd7 : WasmMemoryType :=
{|
l_min := 4; l_max := None
|}.

Definition Globald5d934e6 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global0ac17305 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global7211d487 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalcb399fd5 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Globaldf838b74 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalb09db01c : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globald4573eb3 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globala0abc086 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalcdcf0bb3 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globale6092ba3 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global7590923b : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition DataSegmenta2281312 : WasmDataSegment :=
{|
ds_init := x1e :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x1e :: x00 :: x00 :: x00 :: x7e :: x00 :: x6c :: x00 :: x69 :: x00 :: x62 :: x00 :: x2f :: x00 :: x72 :: x00 :: x74 :: x00 :: x2f :: x00 :: x74 :: x00 :: x6c :: x00 :: x73 :: x00 :: x66 :: x00 :: x2e :: x00 :: x74 :: x00 :: x73 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (8))
 :: 
nil
);
|}.

Definition DataSegment619626c6 : WasmDataSegment :=
{|
ds_init := x28 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x28 :: x00 :: x00 :: x00 :: x61 :: x00 :: x6c :: x00 :: x6c :: x00 :: x6f :: x00 :: x63 :: x00 :: x61 :: x00 :: x74 :: x00 :: x69 :: x00 :: x6f :: x00 :: x6e :: x00 :: x20 :: x00 :: x74 :: x00 :: x6f :: x00 :: x6f :: x00 :: x20 :: x00 :: x6c :: x00 :: x61 :: x00 :: x72 :: x00 :: x67 :: x00 :: x65 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (56))
 :: 
nil
);
|}.

Definition DataSegment0f62283a : WasmDataSegment :=
{|
ds_init := x1e :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x1e :: x00 :: x00 :: x00 :: x7e :: x00 :: x6c :: x00 :: x69 :: x00 :: x62 :: x00 :: x2f :: x00 :: x72 :: x00 :: x74 :: x00 :: x2f :: x00 :: x70 :: x00 :: x75 :: x00 :: x72 :: x00 :: x65 :: x00 :: x2e :: x00 :: x74 :: x00 :: x73 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (112))
 :: 
nil
);
|}.

Definition DataSegmenta109dd59 : WasmDataSegment :=
{|
ds_init := x24 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x24 :: x00 :: x00 :: x00 :: x49 :: x00 :: x6e :: x00 :: x64 :: x00 :: x65 :: x00 :: x78 :: x00 :: x20 :: x00 :: x6f :: x00 :: x75 :: x00 :: x74 :: x00 :: x20 :: x00 :: x6f :: x00 :: x66 :: x00 :: x20 :: x00 :: x72 :: x00 :: x61 :: x00 :: x6e :: x00 :: x67 :: x00 :: x65 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (160))
 :: 
nil
);
|}.

Definition DataSegmentbcf1f647 : WasmDataSegment :=
{|
ds_init := x14 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x14 :: x00 :: x00 :: x00 :: x7e :: x00 :: x6c :: x00 :: x69 :: x00 :: x62 :: x00 :: x2f :: x00 :: x72 :: x00 :: x74 :: x00 :: x2e :: x00 :: x74 :: x00 :: x73 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (216))
 :: 
nil
);
|}.

Definition DataSegmentc60a82fd : WasmDataSegment :=
{|
ds_init := x03 :: x00 :: x00 :: x00 :: x10 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x10 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x10 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (256))
 :: 
nil
);
|}.

Definition ElementSegment9fa6356c : WasmElementSegment :=
{|
es_type := rt_func;
es_init := (i_reference (ri_ref_func 32) :: nil) :: nil;
es_mode := esm_active 0 (i_numeric (ni_i32_const (0))
 :: 
nil
)
|}.
Definition Function3fb4b3b0 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (276))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1073741808))
 :: 
i_numeric ni_i32_lt_u
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (278))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (256))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shr_u
 :: 
nil )( i_numeric (ni_i32_const (31))
 :: 
i_numeric ni_i32_clz
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_sub
 :: 
nil)):: 
i_numeric (ni_i32_const (23))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_lt_u
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (291))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_memory (mi_i32_load {| mi_offset := 16; mi_align := 2 |})
 :: 
i_memory (mi_i32_load {| mi_offset := 20; mi_align := 2 |})
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_store {| mi_offset := 20; mi_align := 2 |})
 :: 
nil) nil):: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_store {| mi_offset := 16; mi_align := 2 |})
 :: 
nil) nil):: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
nil) nil):: 
nil) nil):: 
nil) nil):: 
nil

|}.

Definition Function60927586 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (204))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (206))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (1073741808))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 1)
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
nil) nil):: 
nil) nil):: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_sub
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
nil))
:: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (227))
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (1073741808))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 1)
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
nil) nil):: 
nil) nil):: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1073741808))
 :: 
i_numeric ni_i32_lt_u
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (242))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_eq
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (243))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_sub
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (256))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shr_u
 :: 
nil )( i_numeric (ni_i32_const (31))
 :: 
i_numeric ni_i32_clz
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_sub
 :: 
nil)):: 
i_numeric (ni_i32_const (23))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_lt_u
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (259))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric (ni_i32_const (0))
 :: 
i_memory (mi_i32_store {| mi_offset := 16; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 20; mi_align := 2 |})
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_store {| mi_offset := 16; mi_align := 2 |})
 :: 
nil) nil):: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
nil

|}.

Definition Function58aa1f63 : WasmFunction :=
{|
f_typeidx := 2;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric ni_i32_le_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (385))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_memory (mi_i32_load {| mi_offset := 1568; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric (ni_i32_const (0))
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_ge_u
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (395))
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
nil )( i_control ci_nop  :: 
nil)):: 
nil )( i_numeric (ni_i32_const (1572))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_ge_u
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (407))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
nil)):: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (48))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_control ci_return
 :: 
nil) nil):: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_mul
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_or
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_memory (mi_i32_store {| mi_offset := 16; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_memory (mi_i32_store {| mi_offset := 20; mi_align := 2 |})
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val None) (i_memory (mi_i32_store {| mi_offset := 1568; mi_align := 2 |})
 :: 
nil))
:: 
i_control (ci_call 2)
 :: 
i_numeric (ni_i32_const (1))
 :: 
nil

|}.

Definition Function86005c4c : WasmFunction :=
{|
f_typeidx := 1;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory 0 :: 
i_numeric (ni_i32_const (1572))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (65535))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (65535))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric ni_i32_gt_s
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric ni_i32_sub
 :: 
i_memory 0 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_lt_s
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_control (ci_if (bt_val None) (i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (0))
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_memory (mi_i32_store {| mi_offset := 1568; mi_align := 2 |})
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (23))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_numeric ni_i32_eqz
 :: 
(ci_br_if 1)
 :: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_numeric ni_i32_eqz
 :: 
(ci_br_if 1)
 :: 
i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_numeric (ni_i32_const (1572))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory 0 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_control (ci_call 3)
 :: 
nil

|}.

Definition Function62ce531c : WasmFunction :=
{|
f_typeidx := 5;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric (ni_i32_const (1073741808))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (72))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (447))
 :: 
i_numeric (ni_i32_const (29))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_gt_u
 :: 
nil

|}.

Definition Function38443638 : WasmFunction :=
{|
f_typeidx := 0;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric (ni_i32_const (256))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shr_u
 :: 
nil )( i_numeric (ni_i32_const (536870904))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric (ni_i32_const (27))
 :: 
i_numeric ni_i32_clz
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
nil )( nil)):: 
i_numeric (ni_i32_const (31))
 :: 
i_numeric ni_i32_clz
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_sub
 :: 
nil)):: 
i_numeric (ni_i32_const (23))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_lt_u
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (337))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
nil )( i_numeric ni_i32_ctz
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
nil))
:: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (350))
 :: 
i_numeric (ni_i32_const (17))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric ni_i32_ctz
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
nil)):: 
nil )( i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric ni_i32_ctz
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 96; mi_align := 2 |})
 :: 
nil))
:: 
nil)):: 
nil

|}.

Definition Function00b08a4e : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory 0 :: 
i_numeric (ni_i32_const (65535))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (65535))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric ni_i32_gt_s
 :: 
i_memory 0 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_lt_s
 :: 
i_control (ci_if (bt_val None) (i_memory 0 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_lt_s
 :: 
i_control (ci_if (bt_val None) (i_control ci_unreachable  :: 
nil) nil):: 
nil) nil):: 
i_memory 0 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_control (ci_call 3)
 :: 
nil

|}.

Definition Functiondb835608 : WasmFunction :=
{|
f_typeidx := 6;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (364))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (32))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_call 2)
 :: 
nil )( i_numeric (ni_i32_const (1))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
nil)):: 
nil

|}.

Definition Function3a14485f : WasmFunction :=
{|
f_typeidx := 0;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_call 5)
 :: 
i_control (ci_call 6)
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 7)
 :: 
i_control (ci_call 6)
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (477))
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
nil) nil):: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_ge_u
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (479))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (0))
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 12; mi_align := 2 |})
 :: 
i_control (ci_call 1)
 :: 
i_control (ci_call 8)
 :: 
nil

|}.

Definition Functionaf85fc62 : WasmFunction :=
{|
f_typeidx := 0;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 4)
 :: 
nil) nil):: 
i_control (ci_call 9)
 :: 
i_memory (mi_i32_store {| mi_offset := 8; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
nil

|}.

Definition Functionb6253320 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (268435455))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (268435455))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eq
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (104))
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (107))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
nil

|}.

Definition Function6644d9c1 : WasmFunction :=
{|
f_typeidx := 5;
f_locals := nil;
f_body := i_numeric ni_i32_gt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_call 11)
 :: 
nil) nil):: 
nil

|}.

Definition Functionf4bb45a9 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (531))
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_call 2)
 :: 
nil

|}.

Definition Function3e8cf57f : WasmFunction :=
{|
f_typeidx := 5;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric ni_i32_gt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (176))
 :: 
i_numeric (ni_i32_const (232))
 :: 
i_numeric (ni_i32_const (22))
 :: 
i_numeric (ni_i32_const (27))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_mul
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
nil

|}.

Definition Function8d37dd1c : WasmFunction :=
{|
f_typeidx := 6;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_and
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (12))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (12))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
nil) nil):: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
nil) nil):: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i32_load16_u {| mi_offset := 0; mi_align := 1 |})
 :: 
i_memory (mi_i32_store16 {| mi_offset := 0; mi_align := 1 |})
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_add
 :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
nil) nil):: 
i_control ci_return
 :: 
nil) nil):: 
i_numeric (ni_i32_const (32))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 0)
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 1)
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 2)
 :: 
(ci_br 3)
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (17))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (5))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (9))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (12))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
(ci_br 3)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (18))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (6))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (10))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (14))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (12))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
(ci_br 2)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (19))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (11))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (12))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_shr_u
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
(ci_br 1)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
nil) nil):: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
nil) nil):: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
nil) nil):: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
nil) nil):: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
nil) nil):: 
nil

|}.

Definition Function813dbf13 : WasmFunction :=
{|
f_typeidx := 6;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) ((ci_br 1)
 :: 
nil) nil):: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_le_u
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
nil )( i_numeric ni_i32_add
 :: 
i_numeric ni_i32_le_u
 :: 
nil)):: 
i_control (ci_if (bt_val None) (i_control (ci_call 15)
 :: 
(ci_br 1)
 :: 
nil) nil):: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) ((ci_br 6)
 :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_memory (mi_i64_load {| mi_offset := 0; mi_align := 3 |})
 :: 
i_memory (mi_i64_store {| mi_offset := 0; mi_align := 3 |})
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
nil) nil):: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_control (ci_if (bt_val None) (i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_control (ci_block (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
nil))
:: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
nil )( i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (7))
 :: 
i_numeric ni_i32_and
 :: 
i_control (ci_if (bt_val None) (i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) ((ci_br 6)
 :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i64_load {| mi_offset := 0; mi_align := 3 |})
 :: 
i_memory (mi_i64_store {| mi_offset := 0; mi_align := 3 |})
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
nil) nil):: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
(ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
nil)):: 
nil))
:: 
nil

|}.

Definition Function857c6e7e : WasmFunction :=
{|
f_typeidx := 7;
f_locals := nil;
f_body := i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (561))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_ne
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (15))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_numeric (ni_i32_const (562))
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_call 13)
 :: 
nil

|}.

Definition Function31b059c9 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric ni_i32_sub
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_mul
 :: 
i_numeric (ni_i32_const (64))
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_shl
 :: 
i_numeric ni_i32_gt_u
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_control (ci_call 10)
 :: 
i_control (ci_call 16)
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 17)
 :: 
nil) nil):: 
i_numeric ni_i32_add
 :: 
i_numeric ni_i32_add
 :: 
nil

|}.

Definition Function4ec4ce1c : WasmFunction :=
{|
f_typeidx := 7;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric ni_i32_ge_u
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 18)
 :: 
nil) nil):: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
nil

|}.

Definition Function93749d57 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (268435455))
 :: 
i_numeric ni_i32_and
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (115))
 :: 
i_numeric (ni_i32_const (13))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_control (ci_call 31)
 :: 
i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 13)
 :: 
nil )( i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_or
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
nil)):: 
nil )( i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_gt_u
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (124))
 :: 
i_numeric (ni_i32_const (15))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_memory (mi_i32_load {| mi_offset := 8; mi_align := 2 |})
 :: 
i_control (ci_call 14)
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric (ni_i32_const (805306368))
 :: 
i_numeric ni_i32_or
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 19)
 :: 
nil) nil):: 
nil )( i_numeric (ni_i32_const (268435455))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
nil)):: 
nil)):: 
nil

|}.

Definition Function8b3c6241 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := nil;
f_body := i_numeric ni_i32_gt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_call 20)
 :: 
nil) nil):: 
nil

|}.

Definition Functiona4224870 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (268435456))
 :: 
i_numeric ni_i32_ne
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (268435456))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_control (ci_call 31)
 :: 
nil) nil):: 
nil

|}.

Definition Functioncecb7e2f : WasmFunction :=
{|
f_typeidx := 7;
f_locals := nil;
f_body := i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_control (ci_call 31)
 :: 
nil

|}.

Definition Function0fb7d95c : WasmFunction :=
{|
f_typeidx := 7;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (268435456))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (268435455))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_gt_u
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 23)
 :: 
nil )( i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (536870912))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_control (ci_call 31)
 :: 
nil)):: 
nil) nil):: 
nil

|}.

Definition Functione935d6c5 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (536870912))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_or
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (5))
 :: 
i_control (ci_call 31)
 :: 
i_control (ci_call 13)
 :: 
nil) nil):: 
nil

|}.

Definition Function8def5d96 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (nil))
:: 
i_control (ci_loop (bt_val None) (i_numeric ni_i32_lt_u
 :: 
i_numeric ni_i32_eqz
 :: 
(ci_br_if 1)
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (805306368))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (268435455))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_gt_u
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_control (ci_if (bt_val None) (i_control (ci_call 22)
 :: 
i_memory (mi_i32_store {| mi_offset := 0; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
nil )( i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val (Some (vt_num nt_i32))) ( i_numeric (ni_i32_const (268435455))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eqz
 :: 
nil )( i_numeric (ni_i32_const (0))
 :: 
nil)):: 
i_control (ci_if (bt_val None) (i_control (ci_call 13)
 :: 
nil )( i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
nil)):: 
nil)):: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric ni_i32_lt_u
 :: 
i_numeric ni_i32_eqz
 :: 
(ci_br_if 1)
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_call 24)
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_loop (bt_val None) (i_numeric ni_i32_lt_u
 :: 
i_numeric ni_i32_eqz
 :: 
(ci_br_if 1)
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (-2147483648))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_control (ci_call 25)
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
nil

|}.

Definition Function671ea4f5 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := nil;
f_body := i_numeric (ni_i32_const (1))
 :: 
i_memory 0 :: 
nil

|}.

Definition Functionc01517da : WasmFunction :=
{|
f_typeidx := 1;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_control (ci_loop (bt_val None) (i_numeric (ni_i32_const (1024))
 :: 
i_numeric ni_i32_lt_s
 :: 
i_numeric ni_i32_eqz
 :: 
(ci_br_if 1)
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_load8_u {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (127))
 :: 
i_numeric ni_i32_gt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (127))
 :: 
i_numeric ni_i32_sub
 :: 
i_numeric ni_i32_add
 :: 
nil )( i_numeric (ni_i32_const (127))
 :: 
i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_div_u
 :: 
nil) nil):: 
nil)):: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store8 {| mi_offset := 0; mi_align := 0 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
(ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
nil

|}.

Definition Function6997ec5b : WasmFunction :=
{|
f_typeidx := 1;
f_locals := nil;
f_body := i_control (ci_call 27)
 :: 
nil

|}.

Definition Functionab60929e : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_numeric ni_i32_lt_u
 :: 
i_control (ci_if (bt_val None) (i_control ci_return
 :: 
nil) nil):: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_sub
 :: 
i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 0)
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 1)
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 2)
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 3)
 :: 
i_numeric (ni_i32_const (5))
 :: 
i_numeric ni_i32_eq
 :: 
(ci_br_if 4)
 :: 
(ci_br 5)
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_call 20)
 :: 
(ci_br 6)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (268435455))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_gt_u
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (75))
 :: 
i_numeric (ni_i32_const (17))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_sub
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_control (ci_call 22)
 :: 
(ci_br 5)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_call 24)
 :: 
(ci_br 4)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_memory (mi_i32_load {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (268435455))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
i_numeric (ni_i32_const (268435455))
 :: 
i_numeric (ni_i32_const (-1))
 :: 
i_numeric ni_i32_xor
 :: 
i_numeric ni_i32_and
 :: 
i_numeric ni_i32_eq
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (86))
 :: 
i_numeric (ni_i32_const (6))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
i_numeric (ni_i32_const (1))
 :: 
i_numeric ni_i32_add
 :: 
i_memory (mi_i32_store {| mi_offset := 4; mi_align := 2 |})
 :: 
i_numeric (ni_i32_const (1879048192))
 :: 
i_numeric ni_i32_and
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_ne
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 23)
 :: 
nil) nil):: 
(ci_br 3)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_call 25)
 :: 
(ci_br 2)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_if (bt_val None) (i_numeric (ni_i32_const (0))
 :: 
i_numeric (ni_i32_const (128))
 :: 
i_numeric (ni_i32_const (97))
 :: 
i_numeric (ni_i32_const (24))
 :: 
i_control (ci_call 0)
 :: 
i_control ci_unreachable  :: 
nil) nil):: 
nil))
:: 
nil

|}.

Definition Function77a10a13 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_numeric (ni_i32_const (8))
 :: 
i_numeric ni_i32_sub
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
ci_br_table(0 :: 0 :: 1) 2
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control ci_return
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_if (bt_val None) (i_control (ci_call 30)
 :: 
nil) nil):: 
i_control ci_return
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (i_control ci_unreachable  :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
nil

|}.

Definition Function6a3c71d8 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := nil;
f_body := nil

|}.



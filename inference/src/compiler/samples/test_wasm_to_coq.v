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

Definition Table008f4ff9 : WasmTableType :=
{|
tt_limits := {| l_min := 4; l_max := None |};
tt_reftype := rt_func
|}.

Definition MemType52012ce6 : WasmMemoryType :=
{|
l_min := 4; l_max := None
|}.

Definition Globaled5a71f6 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalbd4c3e26 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global1e816df6 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalb8afae24 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global537b12ac : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global1ee1130a : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global1ef6dbaa : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalc99807f8 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global7962b7af : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global6808d2f6 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global0cc578dd : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition DataSegment8c2d2f95 : WasmDataSegment :=
{|
ds_init := x1e :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x1e :: x00 :: x00 :: x00 :: x7e :: x00 :: x6c :: x00 :: x69 :: x00 :: x62 :: x00 :: x2f :: x00 :: x72 :: x00 :: x74 :: x00 :: x2f :: x00 :: x74 :: x00 :: x6c :: x00 :: x73 :: x00 :: x66 :: x00 :: x2e :: x00 :: x74 :: x00 :: x73 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (8))
 :: 
nil
);
|}.

Definition DataSegment8f370231 : WasmDataSegment :=
{|
ds_init := x28 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x28 :: x00 :: x00 :: x00 :: x61 :: x00 :: x6c :: x00 :: x6c :: x00 :: x6f :: x00 :: x63 :: x00 :: x61 :: x00 :: x74 :: x00 :: x69 :: x00 :: x6f :: x00 :: x6e :: x00 :: x20 :: x00 :: x74 :: x00 :: x6f :: x00 :: x6f :: x00 :: x20 :: x00 :: x6c :: x00 :: x61 :: x00 :: x72 :: x00 :: x67 :: x00 :: x65 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (56))
 :: 
nil
);
|}.

Definition DataSegmentabffd69a : WasmDataSegment :=
{|
ds_init := x1e :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x1e :: x00 :: x00 :: x00 :: x7e :: x00 :: x6c :: x00 :: x69 :: x00 :: x62 :: x00 :: x2f :: x00 :: x72 :: x00 :: x74 :: x00 :: x2f :: x00 :: x70 :: x00 :: x75 :: x00 :: x72 :: x00 :: x65 :: x00 :: x2e :: x00 :: x74 :: x00 :: x73 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (112))
 :: 
nil
);
|}.

Definition DataSegment9eff7e53 : WasmDataSegment :=
{|
ds_init := x24 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x24 :: x00 :: x00 :: x00 :: x49 :: x00 :: x6e :: x00 :: x64 :: x00 :: x65 :: x00 :: x78 :: x00 :: x20 :: x00 :: x6f :: x00 :: x75 :: x00 :: x74 :: x00 :: x20 :: x00 :: x6f :: x00 :: x66 :: x00 :: x20 :: x00 :: x72 :: x00 :: x61 :: x00 :: x6e :: x00 :: x67 :: x00 :: x65 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (160))
 :: 
nil
);
|}.

Definition DataSegment77bfaa13 : WasmDataSegment :=
{|
ds_init := x14 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x01 :: x00 :: x00 :: x00 :: x14 :: x00 :: x00 :: x00 :: x7e :: x00 :: x6c :: x00 :: x69 :: x00 :: x62 :: x00 :: x2f :: x00 :: x72 :: x00 :: x74 :: x00 :: x2e :: x00 :: x74 :: x00 :: x73 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (216))
 :: 
nil
);
|}.

Definition DataSegment252f89ae : WasmDataSegment :=
{|
ds_init := x03 :: x00 :: x00 :: x00 :: x10 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x10 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x10 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: x00 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const (256))
 :: 
nil
);
|}.

Definition ElementSegment5dfd06e6 : WasmElementSegment :=
{|
es_type := rt_func;
es_init := (i_reference (ri_ref_func 32) :: nil) :: nil;
es_mode := esm_active 0 (i_numeric (ni_i32_const (0))
 :: 
nil
)
|}.
Definition Function8fa24178 : WasmFunction :=
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

Definition Function787b642c : WasmFunction :=
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

Definition Function7ceb89cc : WasmFunction :=
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

Definition Functionc10fb962 : WasmFunction :=
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
i_memory mi_memory_size
 :: 
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
i_memory mi_memory_grow
 :: 
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
i_control (ci_br_if 1)
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
i_control (ci_br_if 1)
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
i_control (ci_br 0)
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
i_control (ci_br 0)
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
i_memory mi_memory_size
 :: 
i_numeric (ni_i32_const (16))
 :: 
i_numeric ni_i32_shl
 :: 
i_control (ci_call 3)
 :: 
nil

|}.

Definition Function6c48afed : WasmFunction :=
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

Definition Function45283f14 : WasmFunction :=
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

Definition Function52c9c5b7 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := vt_num nt_i32 :: nil;
f_body := i_memory mi_memory_size
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
i_memory mi_memory_grow
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_lt_s
 :: 
i_control (ci_if (bt_val None) (i_memory mi_memory_grow
 :: 
i_numeric (ni_i32_const (0))
 :: 
i_numeric ni_i32_lt_s
 :: 
i_control (ci_if (bt_val None) (i_control ci_unreachable  :: 
nil) nil):: 
nil) nil):: 
i_memory mi_memory_size
 :: 
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

Definition Function3b07cc6d : WasmFunction :=
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

Definition Functionc3206969 : WasmFunction :=
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

Definition Functioncf418d3f : WasmFunction :=
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

Definition Function9a3a6fb2 : WasmFunction :=
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

Definition Function1e0ece08 : WasmFunction :=
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

Definition Functionc9fc983d : WasmFunction :=
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

Definition Function00dd1931 : WasmFunction :=
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

Definition Functionbf7ac083 : WasmFunction :=
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
i_control (ci_br 1)
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
i_control (ci_br 1)
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
i_control (ci_br_if 0)
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_br_if 1)
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_br_if 2)
 :: 
i_control (ci_br 3)
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
i_control (ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_control (ci_br 3)
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
i_control (ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_control (ci_br 2)
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
i_control (ci_br 1)
 :: 
nil) nil):: 
nil))
:: 
nil))
:: 
i_control (ci_br 1)
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

Definition Function54dd6b96 : WasmFunction :=
{|
f_typeidx := 6;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (i_numeric ni_i32_eq
 :: 
i_control (ci_if (bt_val None) (i_control (ci_br 1)
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
i_control (ci_br 1)
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
i_control (ci_if (bt_val None) (i_control (ci_br 6)
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
i_control (ci_br 1)
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
i_control (ci_br 1)
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
i_control (ci_br 1)
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
i_control (ci_if (bt_val None) (i_control (ci_br 6)
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
i_control (ci_br 1)
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
i_control (ci_br 1)
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
i_control (ci_br 1)
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

Definition Function8b229043 : WasmFunction :=
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

Definition Function452e0908 : WasmFunction :=
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

Definition Function27b43d0f : WasmFunction :=
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

Definition Function221390fe : WasmFunction :=
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

Definition Functionbab7f16c : WasmFunction :=
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

Definition Functionc274b753 : WasmFunction :=
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

Definition Function74f56776 : WasmFunction :=
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

Definition Function6d9d1f6c : WasmFunction :=
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

Definition Functiond0b67084 : WasmFunction :=
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

Definition Functiond49fef86 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := vt_num nt_i32 :: nil;
f_body := i_control (ci_block (bt_val None) (i_control (ci_block (bt_val None) (nil))
:: 
i_control (ci_loop (bt_val None) (i_numeric ni_i32_lt_u
 :: 
i_numeric ni_i32_eqz
 :: 
i_control (ci_br_if 1)
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
i_control (ci_br 0)
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
i_control (ci_br_if 1)
 :: 
i_memory (mi_i32_load {| mi_offset := 0; mi_align := 2 |})
 :: 
i_control (ci_call 24)
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_add
 :: 
i_control (ci_br 0)
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
i_control (ci_br_if 1)
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
i_control (ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
nil

|}.

Definition Function70ada68f : WasmFunction :=
{|
f_typeidx := 1;
f_locals := nil;
f_body := i_numeric (ni_i32_const (1))
 :: 
i_memory mi_memory_grow
 :: 
nil

|}.

Definition Function3bdb5a31 : WasmFunction :=
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
i_control (ci_br_if 1)
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
i_control (ci_br 0)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
nil

|}.

Definition Functione95fdf13 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := nil;
f_body := i_control (ci_call 27)
 :: 
nil

|}.

Definition Function8a3e3579 : WasmFunction :=
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
i_control (ci_br_if 0)
 :: 
i_numeric (ni_i32_const (2))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_br_if 1)
 :: 
i_numeric (ni_i32_const (3))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_br_if 2)
 :: 
i_numeric (ni_i32_const (4))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_br_if 3)
 :: 
i_numeric (ni_i32_const (5))
 :: 
i_numeric ni_i32_eq
 :: 
i_control (ci_br_if 4)
 :: 
i_control (ci_br 5)
 :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_call 20)
 :: 
i_control (ci_br 6)
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
i_control (ci_br 5)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_call 24)
 :: 
i_control (ci_br 4)
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
i_control (ci_br 3)
 :: 
i_control ci_unreachable  :: 
nil))
:: 
i_control ci_unreachable  :: 
nil))
:: 
i_control (ci_block (bt_val None) (i_control (ci_call 25)
 :: 
i_control (ci_br 2)
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

Definition Functionfa44f2dc : WasmFunction :=
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
i_control (ci_br_table(0 :: 0 :: 1 :: nil) 2)
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

Definition Function85195891 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := nil;
f_body := nil

|}.



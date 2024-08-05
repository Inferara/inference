Require Import String List BinInt BinNat.
From Exetasis Require Import WasmStructure.
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

Definition Table64f7a170 : WasmTableType :=
{|
tt_limits := {| l_min := 4; l_max := None |};
tt_reftype := rt_func
|}.

Definition MemType620eb68f : WasmMemoryType :=
{|
l_min := 4; l_max := None
|}.

Definition Globald5d47c42 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global7e9129c6 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalbfee64b6 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global9ff385e6 : WasmGlobalType :=
{|
gt_mut := true;
gt_valtype := vt_num nt_i32;
|}.

Definition Global8c40f85d : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global4971550d : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalf9846247 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Globalf45428d0 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global3a583bc8 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global3e8c9190 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition Global2afa84f3 : WasmGlobalType :=
{|
gt_mut := false;
gt_valtype := vt_num nt_i32;
|}.

Definition DataSegment800cb6a3 : WasmDataSegment :=
{|
ds_init := 30 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 30 :: 0 :: 0 :: 0 :: 126 :: 0 :: 108 :: 0 :: 105 :: 0 :: 98 :: 0 :: 47 :: 0 :: 114 :: 0 :: 116 :: 0 :: 47 :: 0 :: 116 :: 0 :: 108 :: 0 :: 115 :: 0 :: 102 :: 0 :: 46 :: 0 :: 116 :: 0 :: 115 :: 0 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const 8)
:: 
nil
);
|}.

Definition DataSegment5be9b991 : WasmDataSegment :=
{|
ds_init := 40 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 40 :: 0 :: 0 :: 0 :: 97 :: 0 :: 108 :: 0 :: 108 :: 0 :: 111 :: 0 :: 99 :: 0 :: 97 :: 0 :: 116 :: 0 :: 105 :: 0 :: 111 :: 0 :: 110 :: 0 :: 32 :: 0 :: 116 :: 0 :: 111 :: 0 :: 111 :: 0 :: 32 :: 0 :: 108 :: 0 :: 97 :: 0 :: 114 :: 0 :: 103 :: 0 :: 101 :: 0 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const 56)
:: 
nil
);
|}.

Definition DataSegmentbb35d0ed : WasmDataSegment :=
{|
ds_init := 30 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 30 :: 0 :: 0 :: 0 :: 126 :: 0 :: 108 :: 0 :: 105 :: 0 :: 98 :: 0 :: 47 :: 0 :: 114 :: 0 :: 116 :: 0 :: 47 :: 0 :: 112 :: 0 :: 117 :: 0 :: 114 :: 0 :: 101 :: 0 :: 46 :: 0 :: 116 :: 0 :: 115 :: 0 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const 112)
:: 
nil
);
|}.

Definition DataSegment5ec19f13 : WasmDataSegment :=
{|
ds_init := 36 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 36 :: 0 :: 0 :: 0 :: 73 :: 0 :: 110 :: 0 :: 100 :: 0 :: 101 :: 0 :: 120 :: 0 :: 32 :: 0 :: 111 :: 0 :: 117 :: 0 :: 116 :: 0 :: 32 :: 0 :: 111 :: 0 :: 102 :: 0 :: 32 :: 0 :: 114 :: 0 :: 97 :: 0 :: 110 :: 0 :: 103 :: 0 :: 101 :: 0 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const 160)
:: 
nil
);
|}.

Definition DataSegment52f28083 : WasmDataSegment :=
{|
ds_init := 20 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 1 :: 0 :: 0 :: 0 :: 20 :: 0 :: 0 :: 0 :: 126 :: 0 :: 108 :: 0 :: 105 :: 0 :: 98 :: 0 :: 47 :: 0 :: 114 :: 0 :: 116 :: 0 :: 46 :: 0 :: 116 :: 0 :: 115 :: 0 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const 216)
:: 
nil
);
|}.

Definition DataSegment2a7842ea : WasmDataSegment :=
{|
ds_init := 3 :: 0 :: 0 :: 0 :: 16 :: 0 :: 0 :: 0 :: 0 :: 0 :: 0 :: 0 :: 16 :: 0 :: 0 :: 0 :: 0 :: 0 :: 0 :: 0 :: 16 :: 0 :: 0 :: 0 :: 0 :: 0 :: 0 :: 0 :: nil;
ds_mode := dsm_active 0 (i_numeric (ni_i32_const 256)
:: 
nil
);
|}.

Definition ElementSegment427c33c4 : WasmElementSegment :=
{|
es_mode := esm_active (0 i_numeric (ni_i32_const 0)
:: 
nil
;
es_type := rt_func;
Definition Function56279237 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := 4;
f_body := (mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 276)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1073741808)
:: 
i_numeric ni_i32_lt_u
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 278)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 256)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shr_u
:: 
nil
i_numeric (ni_i32_const 31)
:: 
i_numeric ni_i32_clz
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_xor
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_sub
:: 
nil
i_numeric (ni_i32_const 23)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_lt_u
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 291)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
mi_i32_load (16, 2))
:: 
mi_i32_load (20, 2))
:: 
((ci_if (:: 
mi_i32_store (20, 2))
:: 
nil
((ci_if (:: 
mi_i32_store (16, 2))
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (96, 2))
:: 
nil
i_numeric ni_i32_eq
:: 
((ci_if (:: 
((ci_block (:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (96, 2))
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (4, 2))
:: 
nil
((ci_block (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_shl
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (4, 2))
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_shl
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
mi_i32_store (0, 2))
:: 
nil
nil
nil
nil
);
|}.

Definition Function3800e17c : WasmFunction :=
{|
f_typeidx := 4;
f_locals := 4;
f_body := (i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 204)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 206)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 1073741808)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
ci_call 1)
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load (0, 2))
:: 
nil
nil
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_sub
:: 
mi_i32_load (0, 2))
:: 
nil
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 227)
:: 
i_numeric (ni_i32_const 15)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 1073741808)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
ci_call 1)
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
nil
nil
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1073741808)
:: 
i_numeric ni_i32_lt_u
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 242)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_eq
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 243)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_sub
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 256)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shr_u
:: 
nil
i_numeric (ni_i32_const 31)
:: 
i_numeric ni_i32_clz
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_xor
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_sub
:: 
nil
i_numeric (ni_i32_const 23)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_lt_u
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 259)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (96, 2))
:: 
nil
i_numeric (ni_i32_const 0)
:: 
mi_i32_store (16, 2))
:: 
mi_i32_store (20, 2))
:: 
((ci_if (:: 
mi_i32_store (16, 2))
:: 
nil
((ci_block (:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (96, 2))
:: 
nil
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
((ci_block (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (4, 2))
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (4, 2))
:: 
nil
nil
);
|}.

Definition Functionfbb7430f : WasmFunction :=
{|
f_typeidx := 2;
f_locals := 2;
f_body := (i_numeric ni_i32_le_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 385)
:: 
i_numeric (ni_i32_const 4)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
((ci_block bt_val nt_i32 (:: 
mi_i32_load (1568, 2))
:: 
nil
i_numeric (ni_i32_const 0)
:: 
((ci_if (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_ge_u
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 395)
:: 
i_numeric (ni_i32_const 15)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
mi_i32_load (0, 2))
:: 
nil
(ci_nop):: 
nil
nil
i_numeric (ni_i32_const 1572)
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_ge_u
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 407)
:: 
i_numeric (ni_i32_const 4)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
nil
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 48)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
ci_return
:: 
nil
i_numeric (ni_i32_const 2)
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_mul
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_or
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 0)
:: 
mi_i32_store (16, 2))
:: 
i_numeric (ni_i32_const 0)
:: 
mi_i32_store (20, 2))
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
((ci_block (:: 
mi_i32_store (1568, 2))
:: 
nil
ci_call 2)
:: 
i_numeric (ni_i32_const 1)
:: 
nil
);
|}.

Definition Functiond4ad0983 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
0:: 
i_numeric (ni_i32_const 1572)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 65535)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 65535)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric ni_i32_gt_s
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric ni_i32_sub
:: 
0:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_lt_s
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
((ci_if (:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 0)
:: 
mi_i32_store (0, 2))
:: 
((ci_block (:: 
i_numeric (ni_i32_const 0)
:: 
mi_i32_store (1568, 2))
:: 
nil
((ci_block (:: 
i_numeric (ni_i32_const 0)
:: 
((ci_loop (:: 
i_numeric (ni_i32_const 23)
:: 
i_numeric ni_i32_lt_u
:: 
i_numeric ni_i32_eqz
:: 
ci_br_if 1)
:: 
((ci_block (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (4, 2))
:: 
nil
((ci_block (:: 
i_numeric (ni_i32_const 0)
:: 
((ci_loop (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_lt_u
:: 
i_numeric ni_i32_eqz
:: 
ci_br_if 1)
:: 
((ci_block (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (96, 2))
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
ci_br 0)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
ci_br 0)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 1572)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
0:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
ci_call 3)
:: 
nil
);
|}.

Definition Function0cf6b24e : WasmFunction :=
{|
f_typeidx := 5;
f_locals := 5;
f_body := (i_numeric (ni_i32_const 1073741808)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 72)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 447)
:: 
i_numeric (ni_i32_const 29)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_gt_u
:: 
nil
);
|}.

Definition Functiona44b97c3 : WasmFunction :=
{|
f_typeidx := 0;
f_locals := 0;
f_body := (i_numeric (ni_i32_const 256)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shr_u
:: 
nil
i_numeric (ni_i32_const 536870904)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric (ni_i32_const 27)
:: 
i_numeric ni_i32_clz
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
nil
nil
i_numeric (ni_i32_const 31)
:: 
i_numeric ni_i32_clz
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_xor
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_sub
:: 
nil
i_numeric (ni_i32_const 23)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_lt_u
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 337)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (4, 2))
:: 
nil
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_ctz
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (4, 2))
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 350)
:: 
i_numeric (ni_i32_const 17)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric ni_i32_ctz
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (96, 2))
:: 
nil
nil
nil
((ci_block bt_val nt_i32 (:: 
i_numeric ni_i32_ctz
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (96, 2))
:: 
nil
nil
nil
);
|}.

Definition Functiond7529ba5 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := 4;
f_body := (0:: 
i_numeric (ni_i32_const 65535)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 65535)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric ni_i32_gt_s
:: 
0:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_lt_s
:: 
((ci_if (:: 
0:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_lt_s
:: 
((ci_if (:: 
(ci_unreachable):: 
nil
nil
0:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
ci_call 3)
:: 
nil
);
|}.

Definition Functione8c2c11f : WasmFunction :=
{|
f_typeidx := 6;
f_locals := 6;
f_body := (mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 364)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 32)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
ci_call 2)
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
mi_i32_store (0, 2))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
mi_i32_store (0, 2))
:: 
nil
nil
);
|}.

Definition Function7493bef1 : WasmFunction :=
{|
f_typeidx := 0;
f_locals := 0;
f_body := (ci_call 5)
:: 
ci_call 6)
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
ci_call 7)
:: 
ci_call 6)
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 477)
:: 
i_numeric (ni_i32_const 15)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
nil
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_ge_u
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 479)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 0)
:: 
mi_i32_store (4, 2))
:: 
mi_i32_store (12, 2))
:: 
ci_call 1)
:: 
ci_call 8)
:: 
nil
);
|}.

Definition Function6d0dcdbc : WasmFunction :=
{|
f_typeidx := 0;
f_locals := 0;
f_body := (i_numeric ni_i32_eqz
:: 
((ci_if (:: 
ci_call 4)
:: 
nil
ci_call 9)
:: 
mi_i32_store (8, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
nil
);
|}.

Definition Function7758955d : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eq
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 104)
:: 
i_numeric (ni_i32_const 2)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (4, 2))
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 107)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
nil
);
|}.

Definition Functiona6afdf12 : WasmFunction :=
{|
f_typeidx := 5;
f_locals := 5;
f_body := (i_numeric ni_i32_gt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_call 11)
:: 
nil
nil
);
|}.

Definition Function583df369 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := 4;
f_body := (mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 531)
:: 
i_numeric (ni_i32_const 2)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
ci_call 2)
:: 
nil
);
|}.

Definition Function1bc0f3b2 : WasmFunction :=
{|
f_typeidx := 5;
f_locals := 5;
f_body := (mi_i32_load (0, 2))
:: 
i_numeric ni_i32_gt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 176)
:: 
i_numeric (ni_i32_const 232)
:: 
i_numeric (ni_i32_const 22)
:: 
i_numeric (ni_i32_const 27)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_mul
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
nil
);
|}.

Definition Function56696f46 : WasmFunction :=
{|
f_typeidx := 6;
f_locals := 6;
f_body := (((ci_block (:: 
((ci_loop (:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_and
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
ci_br 1)
:: 
nil
nil
nil
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 12)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 12)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_br 1)
:: 
nil
nil
nil
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
nil
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
mi_i32_load (0, 2))
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
nil
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
mi_i32_load16_u (0, 1))
:: 
mi_i32_store16 (0, 1))
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_add
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
nil
ci_return
:: 
nil
i_numeric (ni_i32_const 32)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 0)
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 1)
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 2)
:: 
ci_br 3)
:: 
nil
((ci_block (:: 
mi_i32_load (0, 2))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_sub
:: 
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 17)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 5)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 9)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 13)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 12)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_br 1)
:: 
nil
nil
nil
ci_br 3)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
mi_i32_load (0, 2))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_sub
:: 
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 18)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 6)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 10)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 14)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 12)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_br 1)
:: 
nil
nil
nil
ci_br 2)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
mi_i32_load (0, 2))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 19)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 11)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 12)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_shr_u
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_br 1)
:: 
nil
nil
nil
ci_br 1)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
nil
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
nil
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
nil
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
nil
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
nil
nil
);
|}.

Definition Function869afd84 : WasmFunction :=
{|
f_typeidx := 6;
f_locals := 6;
f_body := (((ci_block (:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
ci_br 1)
:: 
nil
i_numeric ni_i32_add
:: 
i_numeric ni_i32_le_u
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
nil
i_numeric ni_i32_add
:: 
i_numeric ni_i32_le_u
:: 
nil
((ci_if (:: 
ci_call 15)
:: 
ci_br 1)
:: 
nil
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
ci_br 6)
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
ci_br 1)
:: 
nil
nil
nil
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
mi_i64_load (0, 3))
:: 
mi_i64_store (0, 3))
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_add
:: 
ci_br 1)
:: 
nil
nil
nil
nil
((ci_block (:: 
((ci_loop (:: 
((ci_if (:: 
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
((ci_block bt_val nt_i32 (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
nil
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
ci_br 1)
:: 
nil
nil
nil
nil
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
((ci_block (:: 
((ci_loop (:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 7)
:: 
i_numeric ni_i32_and
:: 
((ci_if (:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
ci_br 6)
:: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_add
:: 
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
ci_br 1)
:: 
nil
nil
nil
((ci_block (:: 
((ci_loop (:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_add
:: 
mi_i64_load (0, 3))
:: 
mi_i64_store (0, 3))
:: 
ci_br 1)
:: 
nil
nil
nil
nil
((ci_block (:: 
((ci_loop (:: 
((ci_if (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_add
:: 
i_numeric ni_i32_add
:: 
mi_i32_load8_u (0, 0))
:: 
mi_i32_store8 (0, 0))
:: 
ci_br 1)
:: 
nil
nil
nil
nil
nil
nil
);
|}.

Definition Functione9b6d5c9 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 561)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_ne
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 15)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 24)
:: 
i_numeric (ni_i32_const 562)
:: 
i_numeric (ni_i32_const 2)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_call 13)
:: 
nil
);
|}.

Definition Function21017bce : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (i_numeric ni_i32_sub
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_mul
:: 
i_numeric (ni_i32_const 64)
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_shl
:: 
i_numeric ni_i32_gt_u
:: 
i_numeric (ni_i32_const 0)
:: 
ci_call 10)
:: 
ci_call 16)
:: 
((ci_if (:: 
ci_call 17)
:: 
nil
i_numeric ni_i32_add
:: 
i_numeric ni_i32_add
:: 
nil
);
|}.

Definition Function14609d6e : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (i_numeric ni_i32_ge_u
:: 
((ci_if (:: 
ci_call 18)
:: 
nil
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
nil
);
|}.

Definition Function6d9c4209 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric ni_i32_and
:: 
mi_i32_load (0, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 115)
:: 
i_numeric (ni_i32_const 13)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 1)
:: 
ci_call 31)
:: 
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
ci_call 13)
:: 
nil
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_or
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
nil
nil
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_gt_u
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 124)
:: 
i_numeric (ni_i32_const 15)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
mi_i32_load (8, 2))
:: 
ci_call 14)
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric (ni_i32_const 805306368)
:: 
i_numeric ni_i32_or
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
ci_call 19)
:: 
nil
nil
i_numeric (ni_i32_const 268435455)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
nil
nil
nil
);
|}.

Definition Functionc40cd617 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (i_numeric ni_i32_gt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
ci_call 20)
:: 
nil
nil
);
|}.

Definition Functionce6cd440 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 268435456)
:: 
i_numeric ni_i32_ne
:: 
((ci_if (:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 268435456)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 2)
:: 
ci_call 31)
:: 
nil
nil
);
|}.

Definition Function14cd6ea0 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 4)
:: 
ci_call 31)
:: 
nil
);
|}.

Definition Function953c181a : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 268435456)
:: 
i_numeric ni_i32_eq
:: 
((ci_if (:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_gt_u
:: 
((ci_if (:: 
ci_call 23)
:: 
nil
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 536870912)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 3)
:: 
ci_call 31)
:: 
nil
nil
nil
);
|}.

Definition Function76959866 : WasmFunction :=
{|
f_typeidx := 7;
f_locals := 7;
f_body := (mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 536870912)
:: 
i_numeric ni_i32_eq
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
((ci_if (:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_or
:: 
mi_i32_store (4, 2))
:: 
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 5)
:: 
ci_call 31)
:: 
ci_call 13)
:: 
nil
nil
);
|}.

Definition Function5d01e818 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (((ci_block (:: 
((ci_block (:: 
nil
((ci_loop (:: 
i_numeric ni_i32_lt_u
:: 
i_numeric ni_i32_eqz
:: 
ci_br_if 1)
:: 
mi_i32_load (0, 2))
:: 
mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 805306368)
:: 
i_numeric ni_i32_eq
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_gt_u
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
((ci_if (:: 
ci_call 22)
:: 
mi_i32_store (0, 2))
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
nil
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_eq
:: 
((ci_if bt_val nt_i32 (:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eqz
:: 
nil
i_numeric (ni_i32_const 0)
:: 
nil
((ci_if (:: 
ci_call 13)
:: 
nil
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
mi_i32_store (4, 2))
:: 
nil
nil
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
ci_br 0)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
((ci_loop (:: 
i_numeric ni_i32_lt_u
:: 
i_numeric ni_i32_eqz
:: 
ci_br_if 1)
:: 
mi_i32_load (0, 2))
:: 
ci_call 24)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
ci_br 0)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
((ci_loop (:: 
i_numeric ni_i32_lt_u
:: 
i_numeric ni_i32_eqz
:: 
ci_br_if 1)
:: 
mi_i32_load (0, 2))
:: 
mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const -2147483648)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
mi_i32_store (4, 2))
:: 
ci_call 25)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_add
:: 
ci_br 0)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
nil
);
|}.

Definition Functionc83c0add : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (i_numeric (ni_i32_const 1)
:: 
0:: 
nil
);
|}.

Definition Functione9cbc516 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (((ci_block (:: 
i_numeric (ni_i32_const 0)
:: 
((ci_loop (:: 
i_numeric (ni_i32_const 1024)
:: 
i_numeric ni_i32_lt_s
:: 
i_numeric ni_i32_eqz
:: 
ci_br_if 1)
:: 
i_numeric ni_i32_add
:: 
mi_i32_load8_u (0, 0))
:: 
i_numeric (ni_i32_const 127)
:: 
i_numeric ni_i32_gt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 127)
:: 
i_numeric ni_i32_sub
:: 
i_numeric ni_i32_add
:: 
nil
i_numeric (ni_i32_const 127)
:: 
i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_div_u
:: 
nil
nil
i_numeric ni_i32_add
:: 
mi_i32_store8 (0, 0))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
ci_br 0)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
nil
);
|}.

Definition Functionf3a6a41c : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (ci_call 27)
:: 
nil
);
|}.

Definition Function256232c9 : WasmFunction :=
{|
f_typeidx := 4;
f_locals := 4;
f_body := (i_numeric ni_i32_lt_u
:: 
((ci_if (:: 
ci_return
:: 
nil
i_numeric (ni_i32_const 16)
:: 
i_numeric ni_i32_sub
:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 0)
:: 
i_numeric (ni_i32_const 2)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 1)
:: 
i_numeric (ni_i32_const 3)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 2)
:: 
i_numeric (ni_i32_const 4)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 3)
:: 
i_numeric (ni_i32_const 5)
:: 
i_numeric ni_i32_eq
:: 
ci_br_if 4)
:: 
ci_br 5)
:: 
nil
((ci_block (:: 
ci_call 20)
:: 
ci_br 6)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_gt_u
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 75)
:: 
i_numeric (ni_i32_const 17)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_sub
:: 
mi_i32_store (4, 2))
:: 
ci_call 22)
:: 
ci_br 5)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
ci_call 24)
:: 
ci_br 4)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
mi_i32_load (4, 2))
:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
i_numeric (ni_i32_const 268435455)
:: 
i_numeric (ni_i32_const -1)
:: 
i_numeric ni_i32_xor
:: 
i_numeric ni_i32_and
:: 
i_numeric ni_i32_eq
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 86)
:: 
i_numeric (ni_i32_const 6)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 1)
:: 
i_numeric ni_i32_add
:: 
mi_i32_store (4, 2))
:: 
i_numeric (ni_i32_const 1879048192)
:: 
i_numeric ni_i32_and
:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_ne
:: 
((ci_if (:: 
ci_call 23)
:: 
nil
ci_br 3)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
ci_call 25)
:: 
ci_br 2)
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
i_numeric (ni_i32_const 0)
:: 
i_numeric ni_i32_eqz
:: 
((ci_if (:: 
i_numeric (ni_i32_const 0)
:: 
i_numeric (ni_i32_const 128)
:: 
i_numeric (ni_i32_const 97)
:: 
i_numeric (ni_i32_const 24)
:: 
ci_call 0)
:: 
(ci_unreachable):: 
nil
nil
nil
);
|}.

Definition Function20c98bec : WasmFunction :=
{|
f_typeidx := 4;
f_locals := 4;
f_body := (((ci_block (:: 
nil
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
i_numeric (ni_i32_const 8)
:: 
i_numeric ni_i32_sub
:: 
mi_i32_load (0, 2))
:: 
ci_br_table(0 :: 0 :: 1 ::)2)
:: 
nil
((ci_block (:: 
((ci_block (:: 
ci_return
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
((ci_block (:: 
((ci_block (:: 
mi_i32_load (0, 2))
:: 
((ci_if (:: 
ci_call 30)
:: 
nil
ci_return
:: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
((ci_block (:: 
((ci_block (:: 
(ci_unreachable):: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
(ci_unreachable):: 
nil
(ci_unreachable):: 
nil
nil
);
|}.

Definition Function5b65ccc0 : WasmFunction :=
{|
f_typeidx := 1;
f_locals := 1;
f_body := (nil
);
|}.



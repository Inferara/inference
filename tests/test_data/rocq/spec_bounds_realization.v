Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier.

Definition Vi32 i := VAL_int32 (Wasm_int.int_of_Z i32m i).
Definition Vi64 i := VAL_int64 (Wasm_int.int_of_Z i64m i).
Definition Mt l et := {|modtab_type := {|tt_limits := l; tt_elem_type := et|}|}.
Definition Mm l := {|modmem_type := l|}.
Definition Mg mut t init := {|modglob_type := {|tg_mut := mut; tg_t := t|}; modglob_init := init|}.

Definition Mi m n d := {|
  imp_module := list_byte_of_string m;
  imp_name := list_byte_of_string n;
  imp_desc := d;
|}.

Definition Me n d := {|
  modexp_name := list_byte_of_string n;
  modexp_desc := d;
|}.

Definition Ma ofs al := {|memarg_offset := ofs; memarg_align := al|}.

Definition lookup : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_global_get 0%N ::
    BI_const_num (Vi32 32) ::
    BI_binop T_i32 (Binop_i BOI_sub) ::
    BI_local_tee 2%N (*__frame_ptr*) ::
    BI_global_set 0%N ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 0%N 3%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 8%N 3%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 16%N 3%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 24%N 3%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 0) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 10) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 4) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 20) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 8) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 30) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 12) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 40) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 16) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 50) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_local_set 1%N (*table*) ::
    BI_local_get 1%N (*table*) ::
    BI_local_get 0%N (*i*) ::
    BI_local_tee 3%N ::
    BI_local_get 3%N ::
    BI_const_num (Vi32 5) ::
    BI_relop T_i32 (Relop_i (ROI_ge SX_U)) ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_const_num (Vi32 4) ::
    BI_binop T_i32 (Binop_i BOI_mul) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 1%N (*table*) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 32) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_global_set 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition copy_slot : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_global_get 0%N ::
    BI_const_num (Vi32 16) ::
    BI_binop T_i32 (Binop_i BOI_sub) ::
    BI_local_tee 3%N (*__frame_ptr*) ::
    BI_global_set 0%N ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 0%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 8%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi32 0) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 1) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi32 4) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 2) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi32 8) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 3) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_local_set 2%N (*buf*) ::
    BI_local_get 2%N (*buf*) ::
    BI_local_get 0%N (*dst*) ::
    BI_local_tee 4%N ::
    BI_local_get 4%N ::
    BI_const_num (Vi32 3) ::
    BI_relop T_i32 (Relop_i (ROI_ge SX_U)) ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_const_num (Vi32 4) ::
    BI_binop T_i32 (Binop_i BOI_mul) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_local_get 2%N (*buf*) ::
    BI_local_get 1%N (*src*) ::
    BI_local_tee 4%N ::
    BI_local_get 4%N ::
    BI_const_num (Vi32 3) ::
    BI_relop T_i32 (Relop_i (ROI_ge SX_U)) ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_const_num (Vi32 4) ::
    BI_binop T_i32 (Binop_i BOI_mul) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi32 16) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_global_set 0%N ::
    nil;
|}.

Definition spec_bounds_realization : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    nil;
  mod_funcs :=
    lookup ::
    copy_slot ::
    nil;
  mod_tables :=
    nil;
  mod_mems :=
    Mm {|lim_min := 1%N; lim_max := Some(1%N)|} ::
    nil;
  mod_globals :=
    Mg MUT_var (T_num T_i32) (    BI_const_num (Vi32 65536) ::
    nil) ::
    nil;
  mod_elems :=
    nil;
  mod_datas :=
    nil;
  mod_start := None;
  mod_imports :=
    nil;
  mod_exports :=
    Me "memory" (MED_mem 0%N) ::
    Me "__stack_pointer" (MED_global 0%N) ::
    nil;
|}.

Definition spec_bounds_realization__BoundsRealization_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 0%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 0%N) (T_const (Vi32 5))) (T_const (Vi32 0)))))) (HA_app_ok 0 ((T_local 0%N) :: nil)).
Definition spec_bounds_realization__BoundsRealization_hspec2 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 0%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 0%N) (T_const (Vi32 3))) (T_const (Vi32 0))))))) (Himpl (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 1%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 1%N) (T_const (Vi32 3))) (T_const (Vi32 0))))) (HA_app_ok 1 ((T_local 0%N) :: (T_local 1%N) :: nil))).
Definition spec_bounds_realization__BoundsRealization_specs : list hassert := (spec_bounds_realization__BoundsRealization_hspec1 :: spec_bounds_realization__BoundsRealization_hspec2 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_bounds_realization : ValidModule spec_bounds_realization.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_spec_bounds_realization__BoundsRealization : ValidSpec spec_bounds_realization spec_bounds_realization__BoundsRealization_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

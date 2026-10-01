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

Definition ends : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil;
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
    BI_local_get 0%N (*n*) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_load T_i32 None (Ma 0%N 0%N) ::
    BI_store T_i32 None (Ma 4%N 0%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 0%N 0%N) ::
    BI_store T_i64 None (Ma 8%N 0%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 0%N 0%N) ::
    BI_store T_i64 None (Ma 16%N 0%N) ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_local_set 1%N (*xs*) ::
    BI_local_get 1%N (*xs*) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 1%N (*xs*) ::
    BI_const_num (Vi32 20) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_local_set 4%N ::
    BI_local_tee 3%N ::
    BI_local_get 4%N ::
    BI_binop T_i32 (Binop_i BOI_sub) ::
    BI_local_tee 5%N ::
    BI_local_get 3%N ::
    BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
    BI_local_get 4%N ::
    BI_const_num (Vi32 0) ::
    BI_relop T_i32 (Relop_i (ROI_gt SX_S)) ::
    BI_relop T_i32 (Relop_i ROI_ne) ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 5%N ::
    BI_local_get 2%N (*__frame_ptr*) ::
    BI_const_num (Vi32 32) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_global_set 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition buffer_sum : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_global_get 0%N ::
    BI_const_num (Vi32 64) ::
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
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 16%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 24%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 32%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 40%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 48%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 56%N 3%N) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_local_set 0%N (*buf*) ::
    BI_local_get 0%N (*buf*) ::
    BI_const_num (Vi32 12) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 7) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_const_num (Vi32 0) ::
    BI_local_set 1%N (*s*) ::
    BI_const_num (Vi32 0) ::
    BI_local_set 2%N (*i*) ::
    BI_block (BT_valtype None) (
      BI_loop (BT_valtype None) (
        BI_local_get 2%N ::
        BI_const_num (Vi32 16) ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_testop T_i32 TO_eqz ::
        BI_br_if 1%N ::
        BI_local_get 1%N ::
        BI_local_get 0%N ::
        BI_local_get 2%N ::
        BI_local_tee 4%N ::
        BI_local_get 4%N ::
        BI_const_num (Vi32 16) ::
        BI_relop T_i32 (Relop_i (ROI_ge SX_U)) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        BI_const_num (Vi32 4) ::
        BI_binop T_i32 (Binop_i BOI_mul) ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_load T_i32 None (Ma 0%N 2%N) ::
        BI_local_set 6%N ::
        BI_local_tee 5%N ::
        BI_local_get 6%N ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_local_tee 7%N ::
        BI_local_get 5%N ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_local_get 6%N ::
        BI_const_num (Vi32 0) ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_relop T_i32 (Relop_i ROI_ne) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        BI_local_get 7%N ::
        BI_local_set 1%N ::
        BI_local_get 2%N ::
        BI_const_num (Vi32 1) ::
        BI_local_set 6%N ::
        BI_local_tee 5%N ::
        BI_local_get 6%N ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_local_tee 7%N ::
        BI_local_get 5%N ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_local_get 6%N ::
        BI_const_num (Vi32 0) ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_relop T_i32 (Relop_i ROI_ne) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        BI_local_get 7%N ::
        BI_local_set 2%N ::
        BI_br 0%N ::
        nil) ::
      nil) ::
    BI_local_get 1%N (*s*) ::
    BI_local_get 3%N (*__frame_ptr*) ::
    BI_const_num (Vi32 64) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_global_set 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition grid_corner : module_func := {|
  modfunc_type := 2%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_global_get 0%N ::
    BI_const_num (Vi32 32) ::
    BI_binop T_i32 (Binop_i BOI_sub) ::
    BI_local_tee 1%N (*__frame_ptr*) ::
    BI_global_set 0%N ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 0%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 8%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 16%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 24%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi32 0) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 5) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i32 None (Ma 0%N 0%N) ::
    BI_store T_i32 None (Ma 4%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i32 None (Ma 0%N 0%N) ::
    BI_store T_i32 None (Ma 8%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 0%N 0%N) ::
    BI_store T_i64 None (Ma 12%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i32 None (Ma 8%N 0%N) ::
    BI_store T_i32 None (Ma 20%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_set 0%N (*grid*) ::
    BI_local_get 0%N (*grid*) ::
    BI_const_num (Vi32 12) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 8) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi32 32) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_global_set 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition heaviest : module_func := {|
  modfunc_type := 3%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_global_get 0%N ::
    BI_const_num (Vi32 64) ::
    BI_binop T_i32 (Binop_i BOI_sub) ::
    BI_local_tee 1%N (*__frame_ptr*) ::
    BI_global_set 0%N ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 0%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 8%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 16%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 24%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 32%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 40%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 48%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi64 0) ::
    BI_store T_i64 None (Ma 56%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi32 1) ::
    BI_store T_i32 None (Ma 0%N 2%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi32 8) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi64 9) ::
    BI_store T_i64 None (Ma 0%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 0%N 0%N) ::
    BI_store T_i64 None (Ma 16%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 8%N 0%N) ::
    BI_store T_i64 None (Ma 24%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 0%N 0%N) ::
    BI_store T_i64 None (Ma 32%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 8%N 0%N) ::
    BI_store T_i64 None (Ma 40%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 16%N 0%N) ::
    BI_store T_i64 None (Ma 48%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_load T_i64 None (Ma 24%N 0%N) ::
    BI_store T_i64 None (Ma 56%N 0%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_local_set 0%N (*cells*) ::
    BI_local_get 0%N (*cells*) ::
    BI_const_num (Vi32 48) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_const_num (Vi32 8) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_load T_i64 None (Ma 0%N 3%N) ::
    BI_local_get 1%N (*__frame_ptr*) ::
    BI_const_num (Vi32 64) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_global_set 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_array_repeat : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (nil) (T_num T_i32 :: nil) ::
    Tf (nil) (T_num T_i32 :: nil) ::
    Tf (nil) (T_num T_i64 :: nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    ends ::
    buffer_sum ::
    grid_corner ::
    heaviest ::
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
    Me "ends" (MED_func 0%N) ::
    Me "buffer_sum" (MED_func 1%N) ::
    Me "grid_corner" (MED_func 2%N) ::
    Me "heaviest" (MED_func 3%N) ::
    Me "memory" (MED_mem 0%N) ::
    Me "__stack_pointer" (MED_global 0%N) ::
    nil;
|}.

Definition spec_array_repeat__ArrayRepeat_hspec1 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_local 0%N) (T_local 0%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_local 0%N) (T_local 0%N)) (T_const (Vi32 0))))).
Definition spec_array_repeat__ArrayRepeat_hspec2 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_and (term_eq (T_local 0%N) (T_local 0%N)) (HA_and (term_eq (T_local 0%N) (T_local 0%N)) (term_eq (T_local 0%N) (T_local 0%N)))).
Definition spec_array_repeat__ArrayRepeat_hspec3 : hassert :=
  HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_const (Vi32 7)) (T_const (Vi32 7))) (T_const (Vi32 0))).
Definition spec_array_repeat__ArrayRepeat_hspec4 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 0%N) (T_const (Vi32 (-1000)))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_local 0%N) (T_const (Vi32 1000))) (T_const (Vi32 0)))))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 ((T_local 0%N) :: nil)) (T_const (Vi32 0))) (T_const (Vi32 0)))).
Definition spec_array_repeat__ArrayRepeat_specs : list hassert := (spec_array_repeat__ArrayRepeat_hspec1 :: spec_array_repeat__ArrayRepeat_hspec2 :: spec_array_repeat__ArrayRepeat_hspec3 :: spec_array_repeat__ArrayRepeat_hspec4 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_array_repeat : ValidModule spec_array_repeat.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_array_repeat__ArrayRepeat : ValidSpec spec_array_repeat spec_array_repeat__ArrayRepeat_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

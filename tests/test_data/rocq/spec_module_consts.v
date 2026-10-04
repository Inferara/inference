Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier.
Local Delimit Scope Z_scope with Zst.

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

Definition weight_sum : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_const_num (Vi32 0) ::
    BI_local_set 0%N (*s*) ::
    BI_const_num (Vi32 0) ::
    BI_local_set 1%N (*i*) ::
    BI_block (BT_valtype None) (
      BI_loop (BT_valtype None) (
        BI_local_get 1%N ::
        BI_const_num (Vi32 4) ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_testop T_i32 TO_eqz ::
        BI_br_if 1%N ::
        BI_local_get 0%N ::
        BI_const_num (Vi32 65504) ::
        BI_local_get 1%N ::
        BI_local_tee 2%N ::
        BI_local_get 2%N ::
        BI_const_num (Vi32 4) ::
        BI_relop T_i32 (Relop_i (ROI_ge SX_U)) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        BI_const_num (Vi32 4) ::
        BI_binop T_i32 (Binop_i BOI_mul) ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_load T_i32 None (Ma 0%N 2%N) ::
        BI_local_set 4%N ::
        BI_local_tee 3%N ::
        BI_local_get 4%N ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_local_tee 5%N ::
        BI_local_get 3%N ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_local_get 4%N ::
        BI_const_num (Vi32 0) ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_relop T_i32 (Relop_i ROI_ne) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        BI_local_get 5%N ::
        BI_local_set 0%N ::
        BI_local_get 1%N ::
        BI_const_num (Vi32 1) ::
        BI_local_set 4%N ::
        BI_local_tee 3%N ::
        BI_local_get 4%N ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_local_tee 5%N ::
        BI_local_get 3%N ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_local_get 4%N ::
        BI_const_num (Vi32 0) ::
        BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
        BI_relop T_i32 (Relop_i ROI_ne) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        BI_local_get 5%N ::
        BI_local_set 1%N ::
        BI_br 0%N ::
        nil) ::
      nil) ::
    BI_local_get 0%N (*s*) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition clamp : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*v*) ::
    BI_const_num (Vi32 65520) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
    BI_if (BT_valtype None) (
      BI_const_num (Vi32 65520) ::
      BI_load T_i32 None (Ma 0%N 2%N) ::
      BI_return ::
      nil) (
      nil) ::
    BI_local_get 0%N (*v*) ::
    BI_const_num (Vi32 65520) ::
    BI_const_num (Vi32 4) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_load T_i32 None (Ma 0%N 2%N) ::
    BI_relop T_i32 (Relop_i (ROI_gt SX_S)) ::
    BI_if (BT_valtype None) (
      BI_const_num (Vi32 65520) ::
      BI_const_num (Vi32 4) ::
      BI_binop T_i32 (Binop_i BOI_add) ::
      BI_load T_i32 None (Ma 0%N 2%N) ::
      BI_return ::
      nil) (
      nil) ::
    BI_local_get 0%N (*v*) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_module_consts : module := {|
  mod_types :=
    Tf (nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    weight_sum ::
    clamp ::
    nil;
  mod_tables :=
    nil;
  mod_mems :=
    Mm {|lim_min := 1%N; lim_max := Some(1%N)|} ::
    nil;
  mod_globals :=
    Mg MUT_var (T_num T_i32) (    BI_const_num (Vi32 65504) ::
    nil) ::
    nil;
  mod_elems :=
    nil;
  mod_datas :=
    {|
    moddata_init := (encode 1%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 2%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 4%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 8%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 156%Zst) :: (encode 255%Zst) :: (encode 255%Zst) :: (encode 255%Zst) :: (encode 100%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: (encode 0%Zst) :: nil;
    moddata_mode := MD_active 0%N (    BI_const_num (Vi32 65504) ::
    nil);
|} ::
    nil;
  mod_start := None;
  mod_imports :=
    nil;
  mod_exports :=
    Me "weight_sum" (MED_func 0%N) ::
    Me "clamp" (MED_func 1%N) ::
    Me "memory" (MED_mem 0%N) ::
    Me "__stack_pointer" (MED_global 0%N) ::
    nil;
|}.

Definition spec_module_consts__ModuleConsts_hspec1 : hassert :=
  HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_const (Vi32 15)) (T_const (Vi32 15))) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_const (Vi32 8)) (T_const (Vi32 8))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_const (Vi32 4)) (T_const (Vi32 4))) (T_const (Vi32 0))))).
Definition spec_module_consts__ModuleConsts_hspec2 : hassert :=
  HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 nil) (T_const (Vi32 15))) (T_const (Vi32 0))).
Definition spec_module_consts__ModuleConsts_hspec3 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_app 1 ((T_local 0%N) :: nil)) (T_const (Vi32 (-100)))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_app 1 ((T_local 0%N) :: nil)) (T_const (Vi32 100))) (T_const (Vi32 0))))).
Definition spec_module_consts__ModuleConsts_specs : list hassert := (spec_module_consts__ModuleConsts_hspec1 :: spec_module_consts__ModuleConsts_hspec2 :: spec_module_consts__ModuleConsts_hspec3 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_module_consts : ValidModule spec_module_consts.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_module_consts__ModuleConsts : ValidSpec spec_module_consts spec_module_consts__ModuleConsts_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

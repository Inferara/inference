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

Definition fixmul : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := T_num T_i64 :: T_num T_i64 :: T_num T_i64 :: T_num T_i64 :: nil;
  modfunc_body :=
    BI_const_num (Vi64 1048576) ::
    BI_local_set 2%N (*ONE*) ::
    BI_local_get 0%N (*a*) ::
    BI_local_get 1%N (*b*) ::
    BI_local_set 4%N ::
    BI_local_tee 3%N ::
    BI_local_get 4%N ::
    BI_binop T_i64 (Binop_i BOI_mul) ::
    BI_local_set 5%N ::
    BI_local_get 4%N ::
    BI_const_num (Vi64 0) ::
    BI_relop T_i64 (Relop_i ROI_ne) ::
    BI_if (BT_valtype None) (
      BI_local_get 4%N ::
      BI_const_num (Vi64 (-1)) ::
      BI_relop T_i64 (Relop_i ROI_eq) ::
      BI_if (BT_valtype None) (
        BI_local_get 3%N ::
        BI_const_num (Vi64 (-9223372036854775808)) ::
        BI_relop T_i64 (Relop_i ROI_eq) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        nil) (
        BI_local_get 5%N ::
        BI_local_get 4%N ::
        BI_binop T_i64 (Binop_i (BOI_div SX_S)) ::
        BI_local_get 3%N ::
        BI_relop T_i64 (Relop_i ROI_ne) ::
        BI_if (BT_valtype None) (
          BI_unreachable ::
          nil) (
          nil) ::
        nil) ::
      nil) (
      nil) ::
    BI_local_get 5%N ::
    BI_local_get 2%N (*ONE*) ::
    BI_binop T_i64 (Binop_i (BOI_div SX_S)) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition bump : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_local_get 0%N (*x*) ::
    BI_const_num (Vi32 24) ::
    BI_binop T_i32 (Binop_i BOI_shl) ::
    BI_const_num (Vi32 24) ::
    BI_binop T_i32 (Binop_i (BOI_shr SX_S)) ::
    BI_local_set 0%N (*x*) ::
    BI_local_get 0%N (*x*) ::
    BI_const_num (Vi32 1) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_local_tee 1%N ::
    BI_const_num (Vi32 24) ::
    BI_binop T_i32 (Binop_i BOI_shl) ::
    BI_const_num (Vi32 24) ::
    BI_binop T_i32 (Binop_i (BOI_shr SX_S)) ::
    BI_local_tee 2%N ::
    BI_local_get 1%N ::
    BI_relop T_i32 (Relop_i ROI_ne) ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 2%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_overflow_realization : module := {|
  mod_types :=
    Tf (T_num T_i64 :: T_num T_i64 :: nil) (T_num T_i64 :: nil) ::
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i64 :: T_num T_i64 :: nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    fixmul ::
    bump ::
    nil;
  mod_tables :=
    nil;
  mod_mems :=
    nil;
  mod_globals :=
    nil;
  mod_elems :=
    nil;
  mod_datas :=
    nil;
  mod_start := None;
  mod_imports :=
    nil;
  mod_exports :=
    Me "fixmul" (MED_func 0%N) ::
    Me "bump" (MED_func 1%N) ::
    nil;
|}.

Definition spec_overflow_realization__OverflowRealization_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i64) (HA_and (HA_has_type (T_local 1%N) T_i64) (HA_and (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_le SX_S)) (T_const (Vi64 (-3037000499))) (T_local 0%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_le SX_S)) (T_local 0%N) (T_const (Vi64 3037000499))) (T_const (Vi32 0))))))) (Himpl (HA_and (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_le SX_S)) (T_const (Vi64 (-3037000499))) (T_local 1%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_le SX_S)) (T_local 1%N) (T_const (Vi64 3037000499))) (T_const (Vi32 0))))) (HA_app_ok 0 ((T_local 0%N) :: (T_local 1%N) :: nil))).
Definition spec_overflow_realization__OverflowRealization_hspec2 : hassert :=
  Himpl (HA_and (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 (-128))) (T_local 0%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 0%N) (T_const (Vi32 128))) (T_const (Vi32 0)))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 0%N) (T_const (Vi32 127))) (T_const (Vi32 0))))) (HA_app_ok 1 ((T_local 0%N) :: nil)).
Definition spec_overflow_realization__OverflowRealization_specs : list hassert := (spec_overflow_realization__OverflowRealization_hspec1 :: spec_overflow_realization__OverflowRealization_hspec2 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_overflow_realization : ValidModule spec_overflow_realization.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_overflow_realization__OverflowRealization : ValidSpec spec_overflow_realization spec_overflow_realization__OverflowRealization_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

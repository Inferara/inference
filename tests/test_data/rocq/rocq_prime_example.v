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

Definition is_prime : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := T_num T_i32 :: T_num T_i32 :: nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi32 2) ::
    BI_relop T_i32 (Relop_i (ROI_lt SX_S)) ::
    BI_if (BT_valtype None) (
      BI_const_num (Vi32 0) ::
      BI_return ::
      nil) (
      nil) ::
    BI_const_num (Vi32 1) ::
    BI_local_set 1%N (*result*) ::
    BI_const_num (Vi32 2) ::
    BI_local_set 2%N (*i*) ::
    BI_block (BT_valtype None) (
      BI_loop (BT_valtype None) (
        BI_local_get 2%N ::
        BI_local_get 2%N ::
        BI_binop T_i32 (Binop_i BOI_mul) ::
        BI_local_get 0%N ::
        BI_relop T_i32 (Relop_i (ROI_le SX_S)) ::
        BI_testop T_i32 TO_eqz ::
        BI_br_if 1%N ::
        BI_local_get 0%N ::
        BI_local_get 2%N ::
        BI_binop T_i32 (Binop_i (BOI_rem SX_S)) ::
        BI_const_num (Vi32 0) ::
        BI_relop T_i32 (Relop_i ROI_eq) ::
        BI_if (BT_valtype None) (
          BI_const_num (Vi32 0) ::
          BI_local_set 1%N ::
          nil) (
          nil) ::
        BI_local_get 2%N ::
        BI_const_num (Vi32 1) ::
        BI_binop T_i32 (Binop_i BOI_add) ::
        BI_local_set 2%N ::
        BI_br 0%N ::
        nil) ::
      nil) ::
    BI_local_get 1%N (*result*) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition rocq_prime_example : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (nil) (nil) ::
    nil;
  mod_funcs :=
    is_prime ::
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
    nil;
|}.

Definition rocq_prime_example__prime_properties_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 1))) (T_const (Vi32 0))))) (HA_and (Himpl (HA_not (term_eq (T_app 0 ((T_local 0%N) :: nil)) (T_const (Vi32 0)))) (Himpl (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 1%N) (T_local 0%N)) (T_const (Vi32 0)))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_binop T_i32 (Binop_i (BOI_rem SX_S)) (T_local 0%N) (T_local 1%N)) (T_const (Vi32 0))) (T_const (Vi32 0)))))) (Himpl (term_eq (T_app 0 ((T_local 0%N) :: nil)) (T_const (Vi32 0))) (HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_lvar 0) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_lvar 0) (T_local 0%N)) (T_const (Vi32 0))))) (term_eq (T_binop T_i32 (Binop_i (BOI_rem SX_S)) (T_local 0%N) (T_lvar 0)) (T_const (Vi32 0))))))).
Definition rocq_prime_example__prime_properties_specs : list hassert := (rocq_prime_example__prime_properties_hspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_rocq_prime_example : ValidModule rocq_prime_example.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_rocq_prime_example__prime_properties : ValidSpec rocq_prime_example rocq_prime_example__prime_properties_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

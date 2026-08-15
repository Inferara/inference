Require Import List.
Require Import String.
Require Import BinNat.
Require Import ZArith.
From Wasm Require Import bytes numerics datatypes host.
From WasmVerifier Require Import Assertions Verifier Exists.

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

Definition Ma of al := {|memarg_offset := of; memarg_align := al|}.

Definition double : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_local_get 0%N (*n*) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition ex_double : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*n*) ::
    BI_local_set 1%N (*n*) ::
    BI_local_get 1%N (*n*) ::
    BI_local_get 0%N (*lo*) ::
    BI_relop T_i32 (Relop_i (ROI_gt SX_S)) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 2%N (*wide*) ::
    BI_local_set 2%N (*wide*) ::
    BI_local_get 2%N (*wide*) ::
    BI_const_num (Vi64 0) ::
    BI_relop T_i64 (Relop_i (ROI_ge SX_S)) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 1%N (*n*) ::
    BI_call 0%N ::
    BI_local_get 1%N (*n*) ::
    BI_local_get 1%N (*n*) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 3%N (*__choice2*) ::
    BI_call 0%N ::
    BI_local_get 0%N (*lo*) ::
    BI_relop T_i32 (Relop_i (ROI_ge SX_S)) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition rocq_exists_spec : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i64 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    double ::
    ex_double ::
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

Definition rocq_exists_spec__ReachableDouble_specs : list hassert := (@nil hassert).
Definition rocq_exists_spec__ReachableDouble_exspec1 : reachability_spec :=
  {| reach_func := 1%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: 2%N :: nil); reach_payload := HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_local 0%N)) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_ge SX_S)) (T_local 2%N) (T_const (Vi64 0))) (T_const (Vi32 0)))) (HA_and (term_eq (T_app 0 ((T_local 1%N) :: nil)) (T_binop T_i32 (Binop_i BOI_add) (T_local 1%N) (T_local 1%N))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_app 0 ((T_local 3%N) :: nil)) (T_local 0%N)) (T_const (Vi32 0)))))) |}.
Definition rocq_exists_spec__ReachableDouble_ex_specs : list reachability_spec := (rocq_exists_spec__ReachableDouble_exspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_rocq_exists_spec : ValidModule rocq_exists_spec.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_rocq_exists_spec__ReachableDouble : ValidSpec rocq_exists_spec rocq_exists_spec__ReachableDouble_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_exists_rocq_exists_spec__ReachableDouble : ValidExistsSpec rocq_exists_spec rocq_exists_spec__ReachableDouble_ex_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

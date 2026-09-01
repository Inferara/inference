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

Definition Ma ofs al := {|memarg_offset := ofs; memarg_align := al|}.

Definition uq_parity : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*bit*) ::
    BI_local_set 1%N (*bit*) ::
    BI_local_get 1%N (*bit*) ::
    BI_local_get 0%N (*seed*) ::
    BI_const_num (Vi32 2) ::
    BI_binop T_i32 (Binop_i (BOI_rem SX_S)) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 1%N (*bit*) ::
    BI_local_get 1%N (*bit*) ::
    BI_binop T_i32 (Binop_i BOI_mul) ::
    BI_local_get 1%N (*bit*) ::
    BI_binop T_i32 (Binop_i BOI_mul) ::
    BI_local_get 1%N (*bit*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition rocq_unique_spec : module := {|
  mod_types :=
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    uq_parity ::
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

Definition rocq_unique_spec__UniqueParity_specs : list hassert := (@nil hassert).
Definition rocq_unique_spec__UniqueParity_uqspec1 : reachability_spec :=
  {| reach_func := 0%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_binop T_i32 (Binop_i (BOI_rem SX_S)) (T_local 0%N) (T_const (Vi32 2)))) (term_eq (T_binop T_i32 (Binop_i BOI_mul) (T_binop T_i32 (Binop_i BOI_mul) (T_local 1%N) (T_local 1%N)) (T_local 1%N)) (T_local 1%N)) |}.
Definition rocq_unique_spec__UniqueParity__uq_specs : list reachability_spec := (rocq_unique_spec__UniqueParity_uqspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_rocq_unique_spec : ValidModule rocq_unique_spec.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_rocq_unique_spec__UniqueParity : ValidSpec rocq_unique_spec rocq_unique_spec__UniqueParity_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_unique_rocq_unique_spec__UniqueParity : ValidUniqueSpec rocq_unique_spec rocq_unique_spec__UniqueParity__uq_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

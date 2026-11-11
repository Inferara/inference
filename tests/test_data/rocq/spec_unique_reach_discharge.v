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

Definition pinned : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*x*) ::
    BI_local_set 1%N (*x*) ::
    BI_local_get 1%N (*x*) ::
    BI_local_get 0%N (*n*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition both : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*bit*) ::
    BI_local_set 1%N (*bit*) ::
    BI_local_get 1%N (*bit*) ::
    BI_local_get 0%N (*seed*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 2%N (*x*) ::
    BI_local_set 2%N (*x*) ::
    BI_local_get 2%N (*x*) ::
    BI_local_get 1%N (*bit*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition spec_unique_reach_discharge : module := {|
  mod_types :=
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    pinned ::
    both ::
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

Definition spec_unique_reach_discharge__UniqueReachDischarge_specs : list hassert := (@nil hassert).
Definition spec_unique_reach_discharge__UniqueReachDischarge_exspec1 : reachability_spec :=
  {| reach_func := 0%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 0%N))) (term_eq (T_lvar 0) (T_local 1%N)))) |}.
Definition spec_unique_reach_discharge__UniqueReachDischarge__ex_specs : list reachability_spec := (spec_unique_reach_discharge__UniqueReachDischarge_exspec1 :: nil).
Definition spec_unique_reach_discharge__UniqueReachDischarge_uqspec1 : reachability_spec :=
  {| reach_func := 1%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_local 0%N)) (HA_and (term_eq (T_local 2%N) (T_local 1%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 1%N))) (term_eq (T_lvar 0) (T_local 2%N))))) |}.
Definition spec_unique_reach_discharge__UniqueReachDischarge__uq_specs : list reachability_spec := (spec_unique_reach_discharge__UniqueReachDischarge_uqspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_unique_reach_discharge : ValidModule spec_unique_reach_discharge.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_unique_reach_discharge__UniqueReachDischarge : ValidSpec spec_unique_reach_discharge spec_unique_reach_discharge__UniqueReachDischarge_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_exists_spec_unique_reach_discharge__UniqueReachDischarge : ValidExistsSpec spec_unique_reach_discharge spec_unique_reach_discharge__UniqueReachDischarge__ex_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_unique_spec_unique_reach_discharge__UniqueReachDischarge : ValidUniqueSpec spec_unique_reach_discharge spec_unique_reach_discharge__UniqueReachDischarge__uq_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

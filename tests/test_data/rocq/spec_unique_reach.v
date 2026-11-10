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

Definition shift : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi32 1) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition pinned : module_func := {|
  modfunc_type := 1%N;
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

Definition chosen : module_func := {|
  modfunc_type := 2%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*k*) ::
    BI_local_set 0%N (*k*) ::
    BI_local_get 1%N (*x*) ::
    BI_local_set 1%N (*x*) ::
    BI_local_get 1%N (*x*) ::
    BI_local_get 0%N (*k*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition one_preimage : module_func := {|
  modfunc_type := 3%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*x*) ::
    BI_local_set 1%N (*x*) ::
    BI_local_get 1%N (*x*) ::
    BI_call 0%N ::
    BI_local_get 0%N (*y*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition on_an_arm : module_func := {|
  modfunc_type := 4%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi32 0) ::
    BI_relop T_i32 (Relop_i (ROI_gt SX_S)) ::
    BI_if (BT_valtype None) (
      BI_local_get 1%N ::
      BI_local_set 1%N ::
      BI_local_get 1%N ::
      BI_local_get 0%N ::
      BI_relop T_i32 (Relop_i ROI_eq) ::
      BI_testop T_i32 TO_eqz ::
      BI_if (BT_valtype None) (
        BI_unreachable ::
        nil) (
        nil) ::
      nil) (
      nil) ::
    nil;
|}.

Definition one_flag : module_func := {|
  modfunc_type := 5%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*positive*) ::
    BI_const_num (Vi32 1) ::
    BI_binop T_i32 (Binop_i BOI_and) ::
    BI_local_set 1%N (*positive*) ::
    BI_local_get 1%N (*positive*) ::
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi32 0) ::
    BI_relop T_i32 (Relop_i (ROI_gt SX_S)) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition through_a_choice : module_func := {|
  modfunc_type := 6%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 1%N (*x*) ::
    BI_local_set 1%N (*x*) ::
    BI_local_get 2%N (*e*) ::
    BI_local_set 2%N (*e*) ::
    BI_local_get 2%N (*e*) ::
    BI_local_get 0%N (*n*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    BI_local_get 1%N (*x*) ::
    BI_local_get 2%N (*e*) ::
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition both : module_func := {|
  modfunc_type := 7%N;
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

Definition dismissed_draw : module_func := {|
  modfunc_type := 8%N;
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
    BI_local_get 2%N (*w*) ::
    BI_local_set 2%N (*w*) ::
    BI_local_get 2%N (*w*) ::
    BI_local_get 1%N (*bit*) ::
    BI_relop T_i32 (Relop_i (ROI_ge SX_S)) ::
    BI_testop T_i32 TO_eqz ::
    BI_if (BT_valtype None) (
      BI_unreachable ::
      nil) (
      nil) ::
    nil;
|}.

Definition spec_unique_reach : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    shift ::
    pinned ::
    chosen ::
    one_preimage ::
    on_an_arm ::
    one_flag ::
    through_a_choice ::
    both ::
    dismissed_draw ::
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

Definition spec_unique_reach__UniqueReach_specs : list hassert := (@nil hassert).
Definition spec_unique_reach__UniqueReach_exspec1 : reachability_spec :=
  {| reach_func := 1%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 0%N))) (term_eq (T_lvar 0) (T_local 1%N)))) |}.
Definition spec_unique_reach__UniqueReach_exspec2 : reachability_spec :=
  {| reach_func := 2%N; reach_entry_arity := 0%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 0%N))) (term_eq (T_lvar 0) (T_local 1%N)))) |}.
Definition spec_unique_reach__UniqueReach_exspec3 : reachability_spec :=
  {| reach_func := 3%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_app 0 ((T_local 1%N) :: nil)) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_app 0 ((T_lvar 0) :: nil)) (T_local 0%N))) (term_eq (T_lvar 0) (T_local 1%N)))) |}.
Definition spec_unique_reach__UniqueReach_exspec4 : reachability_spec :=
  {| reach_func := 4%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := Hor (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) (HA_and (term_eq (T_local 1%N) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 0%N))) (term_eq (T_lvar 0) (T_local 1%N)))))) (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0))) |}.
Definition spec_unique_reach__UniqueReach_exspec5 : reachability_spec :=
  {| reach_func := 5%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0)))) (Hall (Himpl (HA_and (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 0) (T_const (Vi32 2))) (T_const (Vi32 0))))) (term_eq (T_lvar 0) (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))))) (term_eq (T_lvar 0) (T_local 1%N)))) |}.
Definition spec_unique_reach__UniqueReach_exspec6 : reachability_spec :=
  {| reach_func := 6%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: 2%N :: nil); reach_payload := HA_and (HA_and (term_eq (T_local 2%N) (T_local 0%N)) (term_eq (T_local 1%N) (T_local 2%N))) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (term_eq (T_lvar 1) (T_lvar 0)))))) (term_eq (T_lvar 0) (T_local 1%N)))) |}.
Definition spec_unique_reach__UniqueReach__ex_specs : list reachability_spec := (spec_unique_reach__UniqueReach_exspec1 :: spec_unique_reach__UniqueReach_exspec2 :: spec_unique_reach__UniqueReach_exspec3 :: spec_unique_reach__UniqueReach_exspec4 :: spec_unique_reach__UniqueReach_exspec5 :: spec_unique_reach__UniqueReach_exspec6 :: nil).
Definition spec_unique_reach__UniqueReach_uqspec1 : reachability_spec :=
  {| reach_func := 7%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_binop T_i32 (Binop_i (BOI_rem SX_S)) (T_local 0%N) (T_const (Vi32 2)))) (HA_and (term_eq (T_local 2%N) (T_local 1%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 1%N))) (term_eq (T_lvar 0) (T_local 2%N))))) |}.
Definition spec_unique_reach__UniqueReach_uqspec2 : reachability_spec :=
  {| reach_func := 8%N; reach_entry_arity := 1%nat;
     reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_and (term_eq (T_local 1%N) (T_binop T_i32 (Binop_i (BOI_rem SX_S)) (T_local 0%N) (T_const (Vi32 2)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 2%N) (T_local 1%N)) (T_const (Vi32 0)))) |}.
Definition spec_unique_reach__UniqueReach__uq_specs : list reachability_spec := (spec_unique_reach__UniqueReach_uqspec1 :: spec_unique_reach__UniqueReach_uqspec2 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_unique_reach : ValidModule spec_unique_reach.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_unique_reach__UniqueReach : ValidSpec spec_unique_reach spec_unique_reach__UniqueReach_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_exists_spec_unique_reach__UniqueReach : ValidExistsSpec spec_unique_reach spec_unique_reach__UniqueReach__ex_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_unique_spec_unique_reach__UniqueReach : ValidUniqueSpec spec_unique_reach spec_unique_reach__UniqueReach__uq_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

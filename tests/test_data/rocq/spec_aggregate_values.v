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

Definition sum_pair : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*a*) ::
    BI_local_get 1%N (*b*) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_aggregate_values : module := {|
  mod_types :=
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    nil;
  mod_funcs :=
    sum_pair ::
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

Definition spec_aggregate_values__AggregateValues_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_has_type (T_local 2%N) T_i32))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_local 0%N) (T_local 0%N)) (T_const (Vi32 0)))).
Definition spec_aggregate_values__AggregateValues_hspec2 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_local 2%N)) (T_const (Vi32 0)))))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 1%N) (T_local 2%N)) (T_const (Vi32 0)))).
Definition spec_aggregate_values__AggregateValues_hspec3 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i64) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_const (Vi64 0))) (T_const (Vi32 0)))))))) (Hor (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 3%N) (T_local 2%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_const (Vi64 0))) (T_const (Vi32 0))))).
Definition spec_aggregate_values__AggregateValues_hspec4 : hassert :=
  HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 ((T_const (Vi32 1)) :: (T_const (Vi32 2)) :: nil)) (T_const (Vi32 3))) (T_const (Vi32 0))).
Definition spec_aggregate_values__AggregateValues_hspec5 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (HA_and (term_eq (T_local 0%N) (T_local 2%N)) (term_eq (T_local 1%N) (T_local 3%N))))))) (HA_and (term_eq (T_local 0%N) (T_local 2%N)) (term_eq (T_local 1%N) (T_local 3%N))).
Definition spec_aggregate_values__AggregateValues_hspec6 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_ne) (T_local 0%N) (T_const (Vi32 3))) (T_const (Vi32 0)))))) (Hor (HA_not (term_eq (T_local 0%N) (T_const (Vi32 3)))) (HA_not (term_eq (T_local 1%N) (T_const (Vi32 4))))).
Definition spec_aggregate_values__AggregateValues_hspec7 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (term_eq (T_local 0%N) (T_const (Vi32 3)))) (HA_ex (HA_ex (term_eq (T_binop T_i32 (Binop_i BOI_add) (T_lvar 1) (T_lvar 0)) (T_local 0%N)))).
Definition spec_aggregate_values__AggregateValues_specs : list hassert := (spec_aggregate_values__AggregateValues_hspec1 :: spec_aggregate_values__AggregateValues_hspec2 :: spec_aggregate_values__AggregateValues_hspec3 :: spec_aggregate_values__AggregateValues_hspec4 :: spec_aggregate_values__AggregateValues_hspec5 :: spec_aggregate_values__AggregateValues_hspec6 :: spec_aggregate_values__AggregateValues_hspec7 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_aggregate_values : ValidModule spec_aggregate_values.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_spec_aggregate_values__AggregateValues : ValidSpec spec_aggregate_values spec_aggregate_values__AggregateValues_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

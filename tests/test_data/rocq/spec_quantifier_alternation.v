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

Definition twice : module_func := {|
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

Definition zero_of : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_local_get 0%N (*n*) ::
    BI_binop T_i32 (Binop_i BOI_sub) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_quantifier_alternation : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    twice ::
    zero_of ::
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

Definition spec_quantifier_alternation__QuantifierAlternation_hspec1 : hassert :=
  HA_ex (HA_and (term_eq (T_lvar 0) (T_const (Vi32 0))) (Hall (Himpl (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_binop T_i32 (Binop_i BOI_add) (T_lvar 0) (T_lvar 1)) (T_lvar 0)) (T_const (Vi32 0))))))).
Definition spec_quantifier_alternation__QuantifierAlternation_hspec2 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (Hall (Himpl (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 ((T_lvar 0) :: nil)) (T_binop T_i32 (Binop_i BOI_add) (T_lvar 0) (T_lvar 0))) (T_const (Vi32 0))))))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 ((T_local 0%N) :: nil)) (T_binop T_i32 (Binop_i BOI_add) (T_local 0%N) (T_local 0%N))) (T_const (Vi32 0)))).
Definition spec_quantifier_alternation__QuantifierAlternation_hspec3 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_ex (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (Hor (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 0) (T_local 0%N)) (T_const (Vi32 0)))) (Hall (Himpl (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_binop T_i32 (Binop_i BOI_add) (T_lvar 0) (T_lvar 1)) (T_binop T_i32 (Binop_i BOI_add) (T_lvar 0) (T_local 0%N))) (T_const (Vi32 0))))))) (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 0) (T_local 0%N)) (T_const (Vi32 0)))))).
Definition spec_quantifier_alternation__QuantifierAlternation_hspec4 : hassert :=
  HA_ex (HA_and (term_eq (T_lvar 0) (T_const (Vi32 0))) (Hall (Hall (Himpl (HA_and (HA_has_type (T_lvar 1) T_i32) (HA_has_type (T_lvar 0) T_i32)) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_binop T_i32 (Binop_i BOI_add) (T_lvar 1) (T_lvar 2)) (T_lvar 1)) (T_const (Vi32 0)))))))).
Definition spec_quantifier_alternation__QuantifierAlternation_hspec5 : hassert :=
  HA_ex (HA_and (term_eq (T_lvar 0) (T_const (Vi32 0))) (Hall (Himpl (HA_has_type (T_lvar 0) T_i32) (HA_ex (HA_and (Hor (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 1) (T_const (Vi32 0))) (T_const (Vi32 0)))) (term_eq (T_lvar 0) (T_const (Vi32 1)))) (HA_and (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 1) (T_const (Vi32 0))) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_relop T_i32 (Relop_i ROI_eq) (T_app 0 ((T_lvar 1) :: nil)) (T_binop T_i32 (Binop_i BOI_add) (T_lvar 1) (T_lvar 1)))))) (Hor (HA_not (term_eq (T_lvar 0) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_ne) (T_lvar 2) (T_const (Vi32 0))) (T_const (Vi32 0)))))))))).
Definition spec_quantifier_alternation__QuantifierAlternation_hspec6 : hassert :=
  HA_ex (HA_and (term_eq (T_lvar 0) (T_const (Vi32 0))) (Hall (Himpl (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 1 ((T_lvar 0) :: nil)) (T_lvar 1)) (T_const (Vi32 0))))))).
Definition spec_quantifier_alternation__QuantifierAlternation_specs : list hassert := (spec_quantifier_alternation__QuantifierAlternation_hspec1 :: spec_quantifier_alternation__QuantifierAlternation_hspec2 :: spec_quantifier_alternation__QuantifierAlternation_hspec3 :: spec_quantifier_alternation__QuantifierAlternation_hspec4 :: spec_quantifier_alternation__QuantifierAlternation_hspec5 :: spec_quantifier_alternation__QuantifierAlternation_hspec6 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_quantifier_alternation : ValidModule spec_quantifier_alternation.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_spec_quantifier_alternation__QuantifierAlternation : ValidSpec spec_quantifier_alternation spec_quantifier_alternation__QuantifierAlternation_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

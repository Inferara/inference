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

Definition spec_bounded_iteration : module := {|
  mod_types :=
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (nil) (nil) ::
    Tf (nil) (nil) ::
    nil;
  mod_funcs :=
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

Definition spec_bounded_iteration__BoundedIteration_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 3%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 3%N) (T_const (Vi32 3))) (T_const (Vi32 0))))))))) (HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 3%N) (T_const (Vi32 3))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 0%N))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 1%N))) (Himpl (term_eq (T_local 3%N) (T_const (Vi32 2))) (term_eq (T_lvar 0) (T_local 2%N)))))) (HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 3%N) (T_const (Vi32 3))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 0%N))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 1%N))) (Himpl (term_eq (T_local 3%N) (T_const (Vi32 2))) (term_eq (T_lvar 0) (T_local 2%N)))))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 1) (T_lvar 0)) (T_const (Vi32 0)))))))).
Definition spec_bounded_iteration__BoundedIteration_hspec2 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 1%N) (T_const (Vi32 0))) (T_const (Vi32 0))))))))) (Himpl (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 2%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) (Himpl (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 3%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 3%N) (T_const (Vi32 3))) (T_const (Vi32 0))))) (HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 3%N) (T_const (Vi32 3))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 0%N))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 1%N))) (Himpl (term_eq (T_local 3%N) (T_const (Vi32 2))) (term_eq (T_lvar 0) (T_local 2%N)))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_lvar 0) (T_const (Vi32 0))) (T_const (Vi32 0)))))))).
Definition spec_bounded_iteration__BoundedIteration_hspec3 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (HA_and (HA_has_type (T_local 4%N) T_i32) (term_eq (T_local 0%N) (T_const (Vi32 0)))))))) (HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 0%N) (T_const (Vi32 2))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 0%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 3%N))) (Himpl (term_eq (T_local 0%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 4%N))))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 0) (T_local 3%N)) (T_const (Vi32 0)))))).
Definition spec_bounded_iteration__BoundedIteration_hspec4 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_has_type (T_local 3%N) T_i32) (term_eq (T_local 1%N) (T_local 2%N)))))) (Himpl (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 3%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 3%N) (T_const (Vi32 2))) (T_const (Vi32 0))))) (HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 3%N) (T_const (Vi32 2))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 3%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 1%N))) (Himpl (term_eq (T_local 3%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 2%N))))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_lvar 0) (T_local 1%N)) (T_const (Vi32 0))))))).
Definition spec_bounded_iteration__BoundedIteration_hspec5 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_const (Vi32 0))) (T_const (Vi32 0)))))))) (Himpl (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 0)) (T_local 2%N)) (T_const (Vi32 0)))) (HA_ex (Hor (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 2%N) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 2%N) (T_const (Vi32 2))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 2%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 0%N))) (Himpl (term_eq (T_local 2%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 1%N))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_lvar 0) (T_const (Vi32 0))) (T_const (Vi32 0)))))))).
Definition spec_bounded_iteration__BoundedIteration_hspec6 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_and (HA_has_type (T_local 2%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 1%N) (T_const (Vi32 0))) (T_const (Vi32 0)))))))) (HA_ex (Hor (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_U)) (T_local 2%N) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 2%N) (T_const (Vi32 2))) (T_const (Vi32 0)))) (HA_and (Himpl (term_eq (T_local 2%N) (T_const (Vi32 0))) (term_eq (T_lvar 0) (T_local 0%N))) (Himpl (term_eq (T_local 2%N) (T_const (Vi32 1))) (term_eq (T_lvar 0) (T_local 1%N))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_lvar 0) (T_const (Vi32 0))) (T_const (Vi32 0))))))).
Definition spec_bounded_iteration__BoundedIteration_specs : list hassert := (spec_bounded_iteration__BoundedIteration_hspec1 :: spec_bounded_iteration__BoundedIteration_hspec2 :: spec_bounded_iteration__BoundedIteration_hspec3 :: spec_bounded_iteration__BoundedIteration_hspec4 :: spec_bounded_iteration__BoundedIteration_hspec5 :: spec_bounded_iteration__BoundedIteration_hspec6 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_bounded_iteration : ValidModule spec_bounded_iteration.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_spec_bounded_iteration__BoundedIteration : ValidSpec spec_bounded_iteration spec_bounded_iteration__BoundedIteration_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

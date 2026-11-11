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

Definition spec_unique_blocks : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    shift ::
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

Definition spec_unique_blocks__UniqueBlocks_hspec1 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 0%N))) (term_eq (T_lvar 0) (T_lvar 1))))))).
Definition spec_unique_blocks__UniqueBlocks_hspec2 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (term_eq (T_app 0 ((T_lvar 0) :: nil)) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_app 0 ((T_lvar 0) :: nil)) (T_local 0%N))) (term_eq (T_lvar 0) (T_lvar 1))))))).
Definition spec_unique_blocks__UniqueBlocks_hspec3 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_has_type (T_local 1%N) T_i32) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (term_eq (T_lvar 0) (T_local 1%N))) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (term_eq (T_lvar 0) (T_local 1%N)))) (term_eq (T_lvar 0) (T_lvar 1))))))))) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_local 0%N) (T_local 1%N)) (T_const (Vi32 0)))).
Definition spec_unique_blocks__UniqueBlocks_hspec4 : hassert :=
  HA_ex (HA_ex (HA_and (HA_and (HA_has_type (T_lvar 1) T_i32) (HA_has_type (T_lvar 0) T_i32)) (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_U)) (T_lvar 1) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 1) (T_lvar 0)) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 0) (T_const (Vi32 6))) (T_const (Vi32 0)))) (term_eq (T_binop T_i32 (Binop_i BOI_mul) (T_lvar 1) (T_lvar 0)) (T_const (Vi32 6)))))) (Hall (Hall (Himpl (HA_and (HA_and (HA_has_type (T_lvar 1) T_i32) (HA_has_type (T_lvar 0) T_i32)) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_U)) (T_lvar 1) (T_const (Vi32 1))) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 1) (T_lvar 0)) (T_const (Vi32 0)))) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 0) (T_const (Vi32 6))) (T_const (Vi32 0)))) (term_eq (T_binop T_i32 (Binop_i BOI_mul) (T_lvar 1) (T_lvar 0)) (T_const (Vi32 6))))))) (HA_and (term_eq (T_lvar 1) (T_lvar 3)) (term_eq (T_lvar 0) (T_lvar 2))))))))).
Definition spec_unique_blocks__UniqueBlocks_hspec5 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_ex (HA_and (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 0) (T_const (Vi32 2))) (T_const (Vi32 0))))) (HA_and (term_eq (T_lvar 0) (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0)))) (Hall (Himpl (HA_and (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_lvar 0) (T_const (Vi32 2))) (T_const (Vi32 0))))) (term_eq (T_lvar 0) (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))))) (term_eq (T_lvar 0) (T_lvar 1))))))).
Definition spec_unique_blocks__UniqueBlocks_hspec6 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_lvar 1))))) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_lvar 1)))))) (term_eq (T_lvar 0) (T_lvar 1))))))).
Definition spec_unique_blocks__UniqueBlocks_hspec7 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (Himpl (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) (HA_ex (HA_and (HA_has_type (T_lvar 0) T_i32) (HA_and (term_eq (T_lvar 0) (T_local 0%N)) (Hall (Himpl (HA_and (HA_has_type (T_lvar 0) T_i32) (term_eq (T_lvar 0) (T_local 0%N))) (term_eq (T_lvar 0) (T_lvar 1)))))))).
Definition spec_unique_blocks__UniqueBlocks_specs : list hassert := (spec_unique_blocks__UniqueBlocks_hspec1 :: spec_unique_blocks__UniqueBlocks_hspec2 :: spec_unique_blocks__UniqueBlocks_hspec3 :: spec_unique_blocks__UniqueBlocks_hspec4 :: spec_unique_blocks__UniqueBlocks_hspec5 :: spec_unique_blocks__UniqueBlocks_hspec6 :: spec_unique_blocks__UniqueBlocks_hspec7 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_unique_blocks : ValidModule spec_unique_blocks.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_unique_blocks__UniqueBlocks : ValidSpec spec_unique_blocks spec_unique_blocks__UniqueBlocks_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

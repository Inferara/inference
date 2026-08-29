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

Definition scaled : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi64 2) ::
    BI_binop T_i64 (Binop_i BOI_mul) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition nonzero : module_func := {|
  modfunc_type := 1%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N (*n*) ::
    BI_const_num (Vi64 0) ::
    BI_relop T_i64 (Relop_i (ROI_gt SX_U)) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition threshold : module_func := {|
  modfunc_type := 2%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_const_num (Vi64 4294967296) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition main : module_func := {|
  modfunc_type := 3%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_const_num (Vi64 4294967296) ::
    BI_call 0%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_literal_ctx : module := {|
  mod_types :=
    Tf (T_num T_i64 :: nil) (T_num T_i64 :: nil) ::
    Tf (T_num T_i64 :: nil) (T_num T_i32 :: nil) ::
    Tf (nil) (T_num T_i64 :: nil) ::
    Tf (nil) (T_num T_i64 :: nil) ::
    Tf (nil) (nil) ::
    Tf (T_num T_i64 :: nil) (nil) ::
    nil;
  mod_funcs :=
    scaled ::
    nonzero ::
    threshold ::
    main ::
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
    Me "main" (MED_func 3%N) ::
    nil;
|}.

Definition spec_literal_ctx__LiteralPositions_hspec1 : hassert :=
  HA_not (term_eq (T_relop T_i64 (Relop_i ROI_eq) (T_app 2 nil) (T_const (Vi64 4294967296))) (T_const (Vi32 0))).
Definition spec_literal_ctx__LiteralPositions_hspec2 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i64) (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_gt SX_S)) (T_local 0%N) (T_const (Vi64 4294967296))) (T_const (Vi32 0))))) (HA_and (HA_not (term_eq (T_relop T_i64 (Relop_i (ROI_gt SX_S)) (T_app 0 ((T_local 0%N) :: nil)) (T_binop T_i64 (Binop_i BOI_add) (T_local 0%N) (T_const (Vi64 1)))) (T_const (Vi32 0)))) (HA_not (term_eq (T_app 1 ((T_const (Vi64 (-1))) :: nil)) (T_const (Vi32 0))))).
Definition spec_literal_ctx__LiteralPositions_specs : list hassert := (spec_literal_ctx__LiteralPositions_hspec1 :: spec_literal_ctx__LiteralPositions_hspec2 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_literal_ctx : ValidModule spec_literal_ctx.
Proof.
  (* TODO: fill the proof *)
Qed.

Theorem valid_spec_literal_ctx__LiteralPositions : ValidSpec spec_literal_ctx spec_literal_ctx__LiteralPositions_specs.
Proof.
  (* TODO: fill the proof *)
Qed.

End Host.

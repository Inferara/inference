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
    BI_local_get 0%N (*x*) ::
    BI_call 1%N ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition mathlib_double : module_func := {|
  modfunc_type := 0%N;
  modfunc_locals := nil;
  modfunc_body :=
    BI_local_get 0%N ::
    BI_local_get 0%N ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    BI_return ::
    BI_unreachable ::
    nil;
|}.

Definition spec_linked_extern : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (T_num T_i32 :: nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
    twice ::
    mathlib_double ::
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
    Me "twice" (MED_func 0%N) ::
    nil;
|}.

Definition spec_linked_extern__DoubleSpec_hspec1 : hassert :=
  Himpl (HA_has_type (T_local 0%N) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_app 1 ((T_local 0%N) :: nil)) (T_binop T_i32 (Binop_i BOI_add) (T_local 0%N) (T_local 0%N))) (T_const (Vi32 0)))).
Definition spec_linked_extern__DoubleSpec_specs : list hassert := (spec_linked_extern__DoubleSpec_hspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_linked_extern : ValidModule spec_linked_extern.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_linked_extern__DoubleSpec : ValidSpec spec_linked_extern spec_linked_extern__DoubleSpec_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

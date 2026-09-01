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

Definition spec_narrow_discharge : module := {|
  mod_types :=
    Tf (T_num T_i32 :: nil) (nil) ::
    Tf (T_num T_i32 :: nil) (nil) ::
    nil;
  mod_funcs :=
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

Definition spec_narrow_discharge__NarrowDischarge_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U)) (T_local 0%N) (T_const (Vi32 256))) (T_const (Vi32 0))))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_U)) (T_local 0%N) (T_const (Vi32 255))) (T_const (Vi32 0)))).
Definition spec_narrow_discharge__NarrowDischarge_hspec2 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_const (Vi32 (-128))) (T_local 0%N)) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_S)) (T_local 0%N) (T_const (Vi32 128))) (T_const (Vi32 0)))))) (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_ge SX_S)) (T_local 0%N) (T_const (Vi32 (-128)))) (T_const (Vi32 0)))) (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S)) (T_local 0%N) (T_const (Vi32 127))) (T_const (Vi32 0))))).
Definition spec_narrow_discharge__NarrowDischarge_specs : list hassert := (spec_narrow_discharge__NarrowDischarge_hspec1 :: spec_narrow_discharge__NarrowDischarge_hspec2 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_spec_narrow_discharge : ValidModule spec_narrow_discharge.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_spec_narrow_discharge__NarrowDischarge : ValidSpec spec_narrow_discharge spec_narrow_discharge__NarrowDischarge_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

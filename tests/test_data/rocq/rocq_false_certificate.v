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

Definition rocq_false_certificate : module := {|
  mod_types :=
    Tf (nil) (nil) ::
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

Definition rocq_false_certificate__FalseCertificate_hspec1 : hassert :=
  HA_not (term_eq (T_const (Vi32 0)) (T_const (Vi32 0))).
Definition rocq_false_certificate__FalseCertificate_specs : list hassert := (rocq_false_certificate__FalseCertificate_hspec1 :: nil).

Section Host.
Context `{ho: host}.

Theorem valid_rocq_false_certificate : ValidModule rocq_false_certificate.
Proof.
  (* TODO: fill the proof *)
Admitted.

Theorem valid_rocq_false_certificate__FalseCertificate : ValidSpec rocq_false_certificate rocq_false_certificate__FalseCertificate_specs.
Proof.
  (* TODO: fill the proof *)
Admitted.

End Host.

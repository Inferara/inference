//! The Rocq names a generated `.v` cannot give away, and how each name the
//! translator emits is kept off them.
//!
//! A generated `.v` shares one flat namespace with everything it imports, and
//! Rocq treats the two ways of contesting a name differently. Defining a name
//! the file already defines is an error. Defining a name the file *imports* is
//! legal, but the new definition shadows the import for the rest of the file,
//! so every later reference resolves to it instead. Function names follow the
//! same split:
//!
//! | Contested name | A function named after it |
//! |---|---|
//! | A keyword, a curated prelude name, or a name the proof contract declares | Escaped with a trailing `_` by [`sanitize_rocq_identifier`] |
//! | Spent by the file on its own scaffolding: a preamble helper, the module record, or `valid_<module>` | Disambiguated with `_<abs_idx>`, from the set `reserved_top_level_names` seeds |
//! | The `Section Host` binder, `HOST_INSTANCE_BINDER` | Left alone: nothing inside that section names a function |
//!
//! An escaped spelling depends only on the name, so it survives added or
//! reordered functions, unless another name in the module ends up spelled `X_`
//! too: a function written `X_`, a module named `X_`, or another function that
//! escapes onto the same spelling (`BI.call` and `BI_call` both become
//! `BI_call_`). Then they contest `X_` like any duplicate: the module name, or
//! whichever function comes first, keeps it, and the others are disambiguated
//! off it. The escape
//! applies whether or not the file imports the declaring library (`Exists` is
//! imported only with a reachability obligation), so that adding a spec never
//! renames a function. Only the curated prelude list is escaped: the other
//! names `List`, `String`, `BinNat` and `ZArith` bring in (`length`, `app`, `N`)
//! are not, since nothing the emitter writes after a function spells one.
//!
//! A module name is the artifact's identity, so it is rejected rather than
//! renamed when it is a keyword or a curated prelude name
//! ([`validate_rocq_identifier`]) or a preamble helper
//! (`validate_module_name_available`). Spec names are only ever emitted joined
//! (`<module>__<spec>…`), so they cannot shadow an import.

use crate::errors::{InvalidIdentifierReason, WasmToVError};
use rustc_hash::FxHashSet;

/// Names auto-imported from the Rocq standard library whose shadowing
/// breaks downstream proofs in subtle ways. Surfaced via the dedicated
/// `RocqStdlibShadow` variant (separate from `ReservedKeyword`).
///
/// Includes both type names and the prelude constructors / common functions
/// that ship with `Coq.Init.*`. `comp` is intentionally absent — it lives in
/// `Coq.Program.Basics`, which is not auto-imported.
pub(crate) const REJECTED_ROCQ_STDLIB_NAMES: &[&str] = &[
    // Type-level
    "list", "option", "nat", "bool", "unit", "pair", "True", "False", "Prop", "Type", "Set", "eq",
    "not", "and", "or", "iff", "sum", "prod", "id",
    // `Nat` is the auto-opened `Coq.Init.Nat` module providing `Nat.add`,
    // `Nat.eqb`, etc. Emitting `Module Nat. ... End Nat.` from a source file
    // named `nat.inf` or `Nat.inf` would shadow these across the whole
    // generated proof, so we reject the capitalized form too.
    "Nat",
    // Boolean and unit constructors
    "true", "false", "tt",
    // Peano nat constructors and basic arithmetic
    "O", "S", "pred", "plus", "mult", "minus", "le", "lt", "ge", "gt", "max", "min",
    // Option / list / sum constructors
    "Some", "None", "nil", "cons", "inl", "inr",
    // Pair projection + sigma / equality constructors
    "fst", "snd", "conj", "eq_refl", "exist", "existT", "left", "right",
    // Well-founded recursion
    "Acc", "well_founded",
];

/// The top-level `Definition` names the emitted preamble always occupies.
///
/// Every generated `.v` opens with these eight helpers before a single line of
/// module content, so a module or function emitting one of them as its own
/// `Definition` gives the file two definitions of one name and Rocq rejects the
/// whole file.
///
/// Deliberately not folded into [`REJECTED_ROCQ_KEYWORDS`] or
/// [`REJECTED_ROCQ_STDLIB_NAMES`]: those two list names that are reserved or
/// in scope in every `.v`, and [`sanitize_rocq_identifier`] escapes them by
/// appending `_`. A preamble collision is instead a property of this
/// translator's own output, a name the file would define twice, so it is
/// resolved the way any duplicate definition is. A function named after a
/// helper is disambiguated off it, through [`reserved_top_level_names`]. A
/// module named after one is rejected by [`validate_module_name_available`],
/// because the module name is the artifact's identity and has nowhere to move
/// to.
pub(crate) const PREAMBLE_HELPER_NAMES: &[&str] =
    &["Vi32", "Vi64", "Mt", "Mm", "Mg", "Mi", "Me", "Ma"];

/// Every name the proof contract declares, grouped by the logical library that
/// declares it.
///
/// These are the names a generated `.v` brings into scope by importing the
/// contract: `From Wasm Require Import bytes numerics datatypes host.` in every
/// file, and `From WasmVerifier Require Import Assertions Verifier.`, which
/// also imports `Exists` in a file that carries a reachability obligation. A
/// function `Definition` spelled like one of them shadows the import for the
/// rest of the file. A later body applying `BI_call`, or a later function's
/// `: module_func` annotation, then names the user's function, and `coqc`
/// rejects the file. [`sanitize_rocq_identifier`] therefore escapes every name
/// here, as it escapes `nat`.
///
/// Type names are listed alongside the constructors, record fields,
/// `Definition`s and `Parameter`s, because the emitted text spells several of
/// them after the functions are defined: `module_func` on every function,
/// `module` on the record, `hassert` and `reachability_spec` on the
/// obligations, and `host` on the section binder.
///
/// Two kinds of name are not listed, because no emitted term can be captured
/// through them. The names Rocq derives from a declaration, the `Build_*`
/// record constructors and the `_rect`/`_ind`/`_rec`/`_sind` eliminators, are
/// never spelled by the emitter. And `Wasm_int`, the module `int_of_Z` is
/// declared in, is a *module* name: modules live in their own namespace, so a
/// `Definition Wasm_int` leaves the preamble's `Wasm_int.int_of_Z` resolving to
/// the module.
///
/// The boundary is the vendored stub in `rocq-stub/`, not the full upstream
/// libraries. The emitted text names only contract declarations the stub
/// carries, because every gated module is elaborated against the stub, so
/// denying the stub's vocabulary is enough for the file itself to elaborate
/// against either. A name only the upstream libraries declare can still be
/// shadowed for a downstream proof that imports the `.v`.
///
/// `the_contract_deny_list_is_the_stub_vocabulary` in the tests crate holds
/// this list to the stub, library by library, as set equality: a name the stub
/// gains must be added here, and a name it loses must be removed. That test
/// reads the stub with the tests crate's Rocq declaration reader, so on its own
/// it is exactly as complete as that reader. Wherever `coqc` runs,
/// `the_contract_deny_list_matches_what_coqc_records` checks the list a second,
/// independent way: against the declarations `coqc` itself records in the
/// `.glob` files it writes while compiling the stub. A declaration form the
/// reader misses (a mutual `Inductive … with …`, say) still turns that test
/// red. The list keeps the reader's one known over-collection: `int_of_Z` is
/// declared inside `Module Wasm_int` and only ever spelled qualified, so
/// escaping it is harmless but unnecessary.
pub const ROCQ_CONTRACT_NAMES: &[(&str, &[&str])] = &[
    ("Wasm.bytes", &["byte", "encode", "list_byte_of_string"]),
    ("Wasm.numerics", &["i32", "i32m", "i64", "i64m", "int_of_Z"]),
    (
        "Wasm.datatypes",
        &[
            "BI_binop",
            "BI_block",
            "BI_br",
            "BI_br_if",
            "BI_br_table",
            "BI_call",
            "BI_call_indirect",
            "BI_const_num",
            "BI_cvtop",
            "BI_data_drop",
            "BI_drop",
            "BI_global_get",
            "BI_global_set",
            "BI_if",
            "BI_load",
            "BI_local_get",
            "BI_local_set",
            "BI_local_tee",
            "BI_loop",
            "BI_memory_copy",
            "BI_memory_fill",
            "BI_memory_grow",
            "BI_memory_init",
            "BI_memory_size",
            "BI_nop",
            "BI_ref_func",
            "BI_ref_is_null",
            "BI_relop",
            "BI_return",
            "BI_select",
            "BI_store",
            "BI_table_fill",
            "BI_table_get",
            "BI_table_grow",
            "BI_table_set",
            "BI_table_size",
            "BI_testop",
            "BI_unop",
            "BI_unreachable",
            "BOI_add",
            "BOI_and",
            "BOI_div",
            "BOI_mul",
            "BOI_or",
            "BOI_rem",
            "BOI_rotl",
            "BOI_rotr",
            "BOI_shl",
            "BOI_shr",
            "BOI_sub",
            "BOI_xor",
            "BT_id",
            "BT_valtype",
            "Binop_i",
            "CVO_extend",
            "CVO_wrap",
            "MD_active",
            "MD_passive",
            "MED_func",
            "MED_global",
            "MED_mem",
            "MED_table",
            "ME_active",
            "ME_declarative",
            "ME_passive",
            "MID_func",
            "MID_global",
            "MID_mem",
            "MID_table",
            "MUT_const",
            "MUT_var",
            "ROI_eq",
            "ROI_ge",
            "ROI_gt",
            "ROI_le",
            "ROI_lt",
            "ROI_ne",
            "Relop_i",
            "SX_S",
            "SX_U",
            "TO_eqz",
            "T_externref",
            "T_funcref",
            "T_i32",
            "T_i64",
            "T_num",
            "T_ref",
            "Tf",
            "Tp_i16",
            "Tp_i32",
            "Tp_i8",
            "UOI_clz",
            "UOI_ctz",
            "UOI_popcnt",
            "Unop_extend",
            "Unop_i",
            "VAL_int32",
            "VAL_int64",
            "basic_instruction",
            "binop",
            "binop_i",
            "block_type",
            "cvtop",
            "function_type",
            "global_type",
            "imp_desc",
            "imp_module",
            "imp_name",
            "lim_max",
            "lim_min",
            "limits",
            "memarg",
            "memarg_align",
            "memarg_offset",
            "mod_datas",
            "mod_elems",
            "mod_exports",
            "mod_funcs",
            "mod_globals",
            "mod_imports",
            "mod_mems",
            "mod_start",
            "mod_tables",
            "mod_types",
            "moddata_init",
            "moddata_mode",
            "modelem_init",
            "modelem_mode",
            "modelem_type",
            "modexp_desc",
            "modexp_name",
            "modfunc_body",
            "modfunc_locals",
            "modfunc_type",
            "modglob_init",
            "modglob_type",
            "modmem_type",
            "modstart_func",
            "modtab_type",
            "module",
            "module_data",
            "module_datamode",
            "module_element",
            "module_elemmode",
            "module_export",
            "module_export_desc",
            "module_func",
            "module_global",
            "module_import",
            "module_import_desc",
            "module_mem",
            "module_start",
            "module_table",
            "mutability",
            "number_type",
            "packed_type",
            "reference_type",
            "relop",
            "relop_i",
            "sx",
            "table_type",
            "testop",
            "tg_mut",
            "tg_t",
            "tt_elem_type",
            "tt_limits",
            "unop",
            "unop_i",
            "value_num",
            "value_type",
        ],
    ),
    ("Wasm.host", &["host"]),
    (
        "WasmVerifier.Assertions",
        &[
            "HA_and",
            "HA_app_ok",
            "HA_defined",
            "HA_ex",
            "HA_false",
            "HA_has_type",
            "HA_not",
            "HA_pred",
            "HA_true",
            "Hall",
            "Himpl",
            "Hor",
            "T_app",
            "T_binop",
            "T_const",
            "T_local",
            "T_lvar",
            "T_relop",
            "hassert",
            "pred_eq",
            "term",
            "term_eq",
        ],
    ),
    ("WasmVerifier.Verifier", &["ValidModule", "ValidSpec"]),
    (
        "WasmVerifier.Exists",
        &[
            "ValidExistsSpec",
            "ValidUniqueSpec",
            "reach_entry_arity",
            "reach_func",
            "reach_payload",
            "reach_visible_locs",
            "reachability_spec",
        ],
    ),
];

/// The name every generated `.v` gives its host instance, in the
/// ``Section Host. Context `{ho: host}.`` block its theorems are stated under.
///
/// It is the contract's own binder name (`ValidSpec` is declared over
/// ``forall `{ho : host}, …``), and after `End Host` it becomes a named
/// implicit argument of every emitted theorem, so a downstream proof may spell
/// it. The translator emits the binder from this constant, so whatever reasons
/// about the binder reads the very name the file spells.
pub(crate) const HOST_INSTANCE_BINDER: &str = "ho";

/// Rocq vernacular and Gallina keywords that would cause an immediate parse
/// error if used as an identifier. Surfaced via `InvalidIdentifierReason::ReservedKeyword`.
pub(crate) const REJECTED_ROCQ_KEYWORDS: &[&str] = &[
    // Vernacular
    "Definition",
    "Theorem",
    "Lemma",
    "Fixpoint",
    "CoFixpoint",
    "Inductive",
    "CoInductive",
    "Record",
    "Structure",
    "Module",
    "Section",
    "Import",
    "Export",
    "Require",
    "End",
    "Axiom",
    "Parameter",
    "Variable",
    "Hypothesis",
    "Context",
    "Class",
    "Instance",
    "Notation",
    "Reserved",
    "Hint",
    "Proof",
    "Qed",
    "Defined",
    "Admitted",
    "Abort",
    "Goal",
    "SProp",
    // Gallina term-level
    "fun",
    "match",
    "with",
    "end",
    "let",
    "in",
    "if",
    "then",
    "else",
    "as",
    "return",
    "forall",
    "exists",
    "exists2",
    "fix",
    "cofix",
    "at",
    "where",
    "for",
    "by",
    "using",
];

/// The logical library that declares `name`, if the proof contract declares it.
///
/// A linear scan, which is fine at this size: the list holds about two hundred
/// short names and is consulted once or twice per emitted function name.
#[must_use = "the declaring library is the only result"]
pub(crate) fn contract_library_of(name: &str) -> Option<&'static str> {
    ROCQ_CONTRACT_NAMES
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|&(library, _)| library)
}

/// Validates that `name` is acceptable as a Rocq identifier emitted by this
/// translator (module name, spec name, theorem name suffix).
///
/// Rules:
/// - First character is `[A-Za-z]` (not `_`, since Rocq reserves `_` for
///   wildcards).
/// - Remaining characters are `[A-Za-z0-9_]`. Primes (`'`) are rejected.
/// - Length ≤ 255.
/// - Case-sensitive denylist against Rocq stdlib types and reserved
///   vernacular/Gallina keywords. Both `nat` (the type) and `Nat` (the
///   auto-opened module providing `Nat.add`, `Nat.eqb`, etc.) are rejected.
pub fn validate_rocq_identifier(name: &str) -> Result<(), WasmToVError> {
    if name.is_empty() {
        return Err(WasmToVError::InvalidRocqIdentifier {
            name: name.to_string(),
            reason: InvalidIdentifierReason::EmptyName,
        });
    }

    // Per-char rules first, so that a non-ASCII name reports
    // `ContainsInvalidChar` / `LeadingNonAlpha` (with the offending char)
    // instead of the misleading `TooLong` (which compares byte length).
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty checked above");
    if !first.is_ascii_alphabetic() {
        return Err(WasmToVError::InvalidRocqIdentifier {
            name: name.to_string(),
            reason: InvalidIdentifierReason::LeadingNonAlpha(first),
        });
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Err(WasmToVError::InvalidRocqIdentifier {
                name: name.to_string(),
                reason: InvalidIdentifierReason::ContainsInvalidChar(c),
            });
        }
    }

    // Reject `__` so that pairs of (`<module>`, `<spec>`) splits at the
    // emitted `<module>__<spec>_specs` boundary remain unambiguous. Without
    // this rule, module `Foo` + spec `_X__Y` would collide with module
    // `Foo__X` + spec `Y`.
    if name.contains("__") {
        return Err(WasmToVError::InvalidRocqIdentifier {
            name: name.to_string(),
            reason: InvalidIdentifierReason::ContainsDoubleUnderscore,
        });
    }

    // Now safe to compare `len()` against the 255 cap: by this point we know
    // every char is ASCII, so `len()` equals the character count.
    if name.len() > 255 {
        return Err(WasmToVError::InvalidRocqIdentifier {
            name: name.to_string(),
            reason: InvalidIdentifierReason::TooLong,
        });
    }

    if REJECTED_ROCQ_STDLIB_NAMES.contains(&name) {
        return Err(WasmToVError::RocqStdlibShadow {
            name: name.to_string(),
        });
    }
    if REJECTED_ROCQ_KEYWORDS.contains(&name) {
        return Err(WasmToVError::InvalidRocqIdentifier {
            name: name.to_string(),
            reason: InvalidIdentifierReason::ReservedKeyword,
        });
    }

    Ok(())
}

/// Rejects an output module name that a preamble helper already occupies.
///
/// The module name has nowhere to be disambiguated to: it is the `.v` file's
/// identity, the subject of the emitted
/// `Theorem valid_<module> : ValidModule <module>`, and the prefix of every
/// spec-derived proof name. Renaming it silently would rename the artifact a
/// downstream proof imports, so the name is rejected with a hint instead.
///
/// The preamble helpers are the only names the module name can define twice. It
/// cannot spell a spec-derived name (`<module>__<spec>_specs` and its
/// siblings), because [`validate_rocq_identifier`] has already rejected the
/// `__` separator every one of them carries; and `valid_<module>` is derived
/// from it rather than competing with it.
pub(crate) fn validate_module_name_available(name: &str) -> Result<(), WasmToVError> {
    if PREAMBLE_HELPER_NAMES.contains(&name) {
        return Err(WasmToVError::ModuleNameShadowsPreambleHelper {
            name: name.to_string(),
            fix_hint: format!("{name}_module"),
        });
    }
    Ok(())
}

/// The top-level Rocq names an emitted module claims before it names a single
/// function: the preamble helpers, the `Definition <module> : module` record,
/// and the `Theorem valid_<module>` that judges it.
///
/// Seeded into the function-name disambiguator so a function `Definition` can
/// never claim one of them. Renaming the *function* is the right resolution and
/// renaming the module would be the wrong one: a function's emitted name is
/// read only from `mod_funcs`, and an obligation's `T_app` resolves its callee
/// through the raw name section to an index rather than through the Rocq name,
/// so a disambiguated spelling reaches nothing downstream. The module name, by
/// contrast, *is* the artifact's identity — the `.v`'s subject, the
/// `ValidModule` argument, and the prefix of every spec-derived proof name — so
/// a collision on it is rejected instead, by
/// [`validate_module_name_available`].
///
/// The set is complete for *duplicate definitions*, not merely sufficient. The
/// preamble emits exactly the eight helpers in [`PREAMBLE_HELPER_NAMES`] and
/// nothing else, and every other top-level name an emitted module defines is
/// one of three things: the module record, the `valid_<module>` theorem, or a
/// spec-derived name. Names the file *imports* are a different hazard, a
/// shadowing rather than a duplicate, and keeping a function off them is
/// [`sanitize_rocq_identifier`]'s job, not this set's.
///
/// The spec-derived names (`<module>__<spec>_specs`, `valid_<module>__<spec>`,
/// and their reachability siblings — whose list members double the separator
/// again, as `<module>__<spec>__ex_specs` / `__uq_specs`) need no seat here.
/// In every one of them, each `__` is followed by a letter, because a spec name
/// is letter-led and so is every suffix the emitter joins on. A sanitized
/// function name carries no `__` at all, since [`sanitize_rocq_identifier`]
/// collapses every run and no name it escapes ends in `_`. A disambiguated one
/// can carry a single `__`, when its base ends in `_` (an escaped `nat_`, which
/// becomes `nat__1`, or a name written with a trailing `_`), but that `__` is
/// followed by the index digit. The two shapes can therefore never spell the
/// same name.
#[must_use = "the reserved set is the seed for function-name disambiguation"]
pub(crate) fn reserved_top_level_names(mod_name: &str) -> FxHashSet<String> {
    let mut reserved: FxHashSet<String> = PREAMBLE_HELPER_NAMES
        .iter()
        .map(|helper| (*helper).to_string())
        .collect();
    reserved.insert(mod_name.to_string());
    reserved.insert(format!("valid_{mod_name}"));
    reserved
}

/// Validates that joining `mod_name` and `spec_name` into the emitted Rocq
/// grammar does not fabricate the reserved `__` separator at a join boundary.
///
/// The translator joins the two names into one grammar family: the obligation
/// definitions `<mod_name>__<spec_name>_specs` (with its per-entry
/// `_hspec{k}` members) and, for reachability partitions,
/// `<mod_name>__<spec_name>__ex_specs` / `__uq_specs` (with `_exspec{k}` /
/// `_uqspec{k}` members), plus the theorem names
/// `valid_<mod_name>__<spec_name>`, `valid_exists_<mod_name>__<spec_name>`,
/// and `valid_unique_<mod_name>__<spec_name>`. Every member of the family
/// contains the same `<mod_name>__<spec_name>` join, so one check covers all
/// of them. Each component already passed [`validate_rocq_identifier`], so
/// neither carries an internal `__` and neither starts with `_`. The remaining
/// hazard is a component that *ends* with `_`: the module name then abuts the
/// `__` separator (`app_` -> `app___Foo`), and the spec name abuts a trailing
/// `_`-led suffix (`Spec_` -> `main__Spec__specs`, and identically
/// `main__Spec___ex_specs` or `valid_exists_main__Spec_`-adjacent forms). Both
/// produce an over-long `_` run inside the joined name, which the
/// `<module>__<spec>` split reserves. Rejecting the trailing `_` also keeps
/// the reachability lists unambiguous from the far side: `main__Spec__ex_specs`
/// is the legitimate list name of a spec named `Spec`, so a spec named `Spec_`
/// must not be able to reach the neighbourhood of it. The diagnostic shows the
/// `_specs` member as the representative fabricated name.
///
/// This is the boundary the per-component validation is blind to. It applies
/// uniformly whether the module name is the entry file stem or an imported file's
/// stem. Rejected rather than auto-escaped: the joined name is read verbatim in
/// the generated proof, so the fix is a rename, surfaced via the hint.
pub(crate) fn validate_spec_join_boundary(
    mod_name: &str,
    spec_name: &str,
) -> Result<(), WasmToVError> {
    if mod_name.ends_with('_') {
        return Err(WasmToVError::SpecNameReservesSeparator {
            offender_kind: "output module name".to_string(),
            offender: mod_name.to_string(),
            joined: format!("{mod_name}__{spec_name}"),
            fix_hint: mod_name.trim_end_matches('_').to_string(),
        });
    }
    if spec_name.ends_with('_') {
        return Err(WasmToVError::SpecNameReservesSeparator {
            offender_kind: "spec".to_string(),
            offender: spec_name.to_string(),
            joined: format!("{mod_name}__{spec_name}_specs"),
            fix_hint: spec_name.trim_end_matches('_').to_string(),
        });
    }
    Ok(())
}

/// Rewrites an arbitrary WASM name-section symbol into a syntactically legal
/// Rocq identifier, returning a name that always satisfies
/// [`validate_rocq_identifier`].
///
/// This is the decode-boundary defense for function names copied verbatim
/// from a WASM `name` section. Such names are not constrained to Rocq's
/// identifier grammar: Inference's own codegen emits struct-method names like
/// `Point.sum_coords` (illegal `.`), and an adversarial external `.wasm` can
/// name an inner function with a Coq keyword (`fun`, `match`) or otherwise
/// illegal characters. Emitting any of these verbatim as `Definition <name>`
/// produces invalid Gallina with exit 0 — a silent miscompile of the proof
/// artifact. Sanitizing here guarantees every emitted `Definition` name is
/// well-formed; the emitter additionally de-duplicates the sanitized names so
/// distinct functions never collide on one Rocq `Definition`.
///
/// Rewrite rules (each chosen to map the legal grammar to itself, so a name
/// that passes [`validate_rocq_identifier`] is returned unchanged unless the
/// proof contract declares it):
/// - Characters outside `[A-Za-z0-9_]` become `_`.
/// - A leading non-letter is prefixed with `f_` (Rocq reserves `_`-leading and
///   digit-leading identifiers).
/// - A `__` run is collapsed to `_` (the module/spec separator is reserved).
/// - An over-length name is truncated to the 255-character cap.
/// - A name colliding with a reserved keyword, a stdlib name, or a name the
///   proof contract declares ([`ROCQ_CONTRACT_NAMES`]) is suffixed `_`. The
///   last two are legal identifiers, but a `Definition` spelled like one
///   shadows the import for every later reference in the file. This check runs
///   after every other rewrite, so a name that only becomes one of these on
///   the way through (`T.num` → `T_num`, `BI__call` → `BI_call`) is escaped
///   too.
///
/// The result is never guaranteed globally unique on its own — that is the
/// caller's responsibility — but it is always individually well-formed.
#[must_use]
pub fn sanitize_rocq_identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len().min(255));
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }

    // Enforce a letter-leading identifier first; an empty or non-alpha start
    // is prefixed rather than dropped so distinct inputs stay distinguishable.
    // Done before the `__` collapse so the `f_` prefix joined to a leading `_`
    // (`f_` + `_priv`) does not leave a `__` run behind.
    let needs_prefix = out
        .chars()
        .next()
        .is_none_or(|c| !c.is_ascii_alphabetic());
    if needs_prefix {
        out.insert_str(0, "f_");
    }

    // Collapse `__` runs so the sanitized name cannot collide with the
    // `<module>__<spec>` separator grammar.
    while out.contains("__") {
        out = out.replace("__", "_");
    }

    if out.len() > 255 {
        out.truncate(255);
        // Truncation may leave a trailing `_` adjacent to the cap; that is
        // still a legal identifier, so no further fix-up is needed.
    }

    while REJECTED_ROCQ_KEYWORDS.contains(&out.as_str())
        || REJECTED_ROCQ_STDLIB_NAMES.contains(&out.as_str())
        || contract_library_of(&out).is_some()
    {
        out.push('_');
    }

    debug_assert!(
        validate_rocq_identifier(&out).is_ok(),
        "sanitized identifier `{out}` (from `{name}`) is still invalid",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{
        HOST_INSTANCE_BINDER, PREAMBLE_HELPER_NAMES, REJECTED_ROCQ_KEYWORDS,
        REJECTED_ROCQ_STDLIB_NAMES, ROCQ_CONTRACT_NAMES, contract_library_of,
        sanitize_rocq_identifier, validate_rocq_identifier, validate_spec_join_boundary,
    };
    use crate::errors::WasmToVError;
    use rustc_hash::{FxHashMap, FxHashSet};

    /// A trailing `_` on the module name abuts the `__` separator (`app_` ->
    /// `app___Foo`), so the join is rejected and the module is the offender.
    #[test]
    fn trailing_underscore_module_name_reserves_separator() {
        let err = validate_spec_join_boundary("app_", "Foo").expect_err("must reject");
        let WasmToVError::SpecNameReservesSeparator {
            offender_kind,
            offender,
            joined,
            fix_hint,
        } = err
        else {
            panic!("wrong variant: {err:?}");
        };
        assert_eq!(offender_kind, "output module name");
        assert_eq!(offender, "app_");
        assert_eq!(joined, "app___Foo");
        assert_eq!(fix_hint, "app");
    }

    /// A trailing `_` on the spec name abuts the trailing `_specs` (`Spec_` ->
    /// `main__Spec__specs`), so the join is rejected and the spec is the offender.
    #[test]
    fn trailing_underscore_spec_name_reserves_separator() {
        let err = validate_spec_join_boundary("main", "Spec_").expect_err("must reject");
        let WasmToVError::SpecNameReservesSeparator {
            offender_kind,
            offender,
            joined,
            fix_hint,
        } = err
        else {
            panic!("wrong variant: {err:?}");
        };
        assert_eq!(offender_kind, "spec");
        assert_eq!(offender, "Spec_");
        assert_eq!(joined, "main__Spec__specs");
        assert_eq!(fix_hint, "Spec");
    }

    /// Clean names on both sides join without fabricating a separator.
    #[test]
    fn clean_names_join_without_reserving_separator() {
        assert!(validate_spec_join_boundary("main", "Clean").is_ok());
        // A single underscore in the interior is fine — it never abuts a boundary.
        assert!(validate_spec_join_boundary("my_app", "My_Spec").is_ok());
    }

    /// Every sanitized name must satisfy the validator — the sanitizer's core
    /// contract.
    fn assert_sanitized_is_valid(input: &str) -> String {
        let out = sanitize_rocq_identifier(input);
        assert!(
            validate_rocq_identifier(&out).is_ok(),
            "sanitized `{out}` (from `{input}`) failed validation",
        );
        out
    }

    #[test]
    fn already_valid_names_are_unchanged() {
        for name in ["add_three", "main", "Geometry", "f0", "x_y_z"] {
            assert_eq!(sanitize_rocq_identifier(name), name);
        }
    }

    #[test]
    fn dotted_method_name_becomes_valid_identifier() {
        // Inference emits struct-method names like `Point.sum_coords`.
        let out = assert_sanitized_is_valid("Point.sum_coords");
        assert_eq!(out, "Point_sum_coords");
    }

    #[test]
    fn illegal_characters_become_underscores() {
        let out = assert_sanitized_is_valid("a-b/c:d");
        assert_eq!(out, "a_b_c_d");
    }

    #[test]
    fn leading_non_letter_is_prefixed() {
        assert_eq!(assert_sanitized_is_valid("0abc"), "f_0abc");
        assert_eq!(assert_sanitized_is_valid("_priv"), "f_priv");
        // A digit-only name is prefixed, not emptied.
        assert_eq!(assert_sanitized_is_valid("123"), "f_123");
    }

    #[test]
    fn empty_name_is_prefixed_to_a_legal_identifier() {
        assert_eq!(assert_sanitized_is_valid(""), "f_");
    }

    #[test]
    fn double_underscore_runs_are_collapsed() {
        // `__` is the reserved module/spec separator.
        let out = assert_sanitized_is_valid("a__b");
        assert!(!out.contains("__"), "must not retain `__`: {out}");
        assert_eq!(out, "a_b");
        // A run of illegal chars collapsing to many underscores still collapses.
        assert_eq!(assert_sanitized_is_valid("a...b"), "a_b");
    }

    #[test]
    fn coq_keywords_are_escaped() {
        for kw in ["fun", "match", "Definition", "forall"] {
            let out = assert_sanitized_is_valid(kw);
            assert_ne!(out, kw, "keyword `{kw}` must be escaped");
        }
    }

    #[test]
    fn stdlib_names_are_escaped() {
        for (name, escaped) in [
            ("nat", "nat_"),
            ("Nat", "Nat_"),
            ("list", "list_"),
            ("Some", "Some_"),
        ] {
            assert_eq!(
                assert_sanitized_is_valid(name),
                escaped,
                "stdlib name `{name}` must gain exactly one `_`",
            );
        }
    }

    /// A sample from every contract library, of every declaration shape: a
    /// constructor, a record field, a `Definition`/`Parameter`, and the type
    /// names the emitter spells after the functions. Spelled out rather than
    /// read from the list, so a name silently dropped from the list fails here
    /// instead of hiding in it. Each gains exactly one `_`, the spelling `nat`
    /// already takes.
    #[test]
    fn contract_names_are_escaped_with_a_trailing_underscore() {
        for (name, escaped) in [
            ("byte", "byte_"),
            ("encode", "encode_"),
            ("list_byte_of_string", "list_byte_of_string_"),
            ("i32", "i32_"),
            ("int_of_Z", "int_of_Z_"),
            ("BI_call", "BI_call_"),
            ("Tf", "Tf_"),
            ("modfunc_body", "modfunc_body_"),
            ("module_func", "module_func_"),
            ("module", "module_"),
            ("host", "host_"),
            ("hassert", "hassert_"),
            ("HA_true", "HA_true_"),
            ("term_eq", "term_eq_"),
            ("ValidModule", "ValidModule_"),
            ("ValidSpec", "ValidSpec_"),
            ("reachability_spec", "reachability_spec_"),
            ("reach_func", "reach_func_"),
        ] {
            assert_eq!(
                assert_sanitized_is_valid(name),
                escaped,
                "contract name `{name}` must gain exactly one `_`",
            );
        }
    }

    /// The library each sample name is reported under, at least one per
    /// library. The library is the logical path a generated `.v` imports it
    /// by, so a name filed under the wrong group would point a reader at the
    /// wrong file.
    #[test]
    fn contract_names_report_their_declaring_library() {
        for (name, library) in [
            ("encode", "Wasm.bytes"),
            ("i32m", "Wasm.numerics"),
            ("BI_call", "Wasm.datatypes"),
            ("module", "Wasm.datatypes"),
            ("host", "Wasm.host"),
            ("hassert", "WasmVerifier.Assertions"),
            ("ValidModule", "WasmVerifier.Verifier"),
            ("reach_func", "WasmVerifier.Exists"),
        ] {
            assert_eq!(contract_library_of(name), Some(library), "`{name}`");
        }
    }

    /// The contract check runs after every other rewrite, so a name that only
    /// *becomes* a contract name on the way through is escaped too.
    #[test]
    fn names_rewritten_into_a_contract_name_are_escaped() {
        for (name, escaped) in [
            // Struct methods, as Inference's codegen names them.
            ("T.num", "T_num_"),
            ("BI.call", "BI_call_"),
            ("HA.true", "HA_true_"),
            // Any other illegal character.
            ("BI-call", "BI_call_"),
            // The `__` collapse.
            ("BI__call", "BI_call_"),
            ("BI___call", "BI_call_"),
            // The symbol the linker gives a merged external's function.
            ("mod::funcs", "mod_funcs_"),
        ] {
            assert_eq!(
                assert_sanitized_is_valid(name),
                escaped,
                "`{name}` rewrites to a contract name, which must then be escaped",
            );
        }
    }

    /// Every entry, not a sample: each one gains exactly one `_`, so no listed
    /// name can reach the emitted file verbatim.
    #[test]
    fn every_contract_name_is_escaped() {
        for &(library, names) in ROCQ_CONTRACT_NAMES {
            for &name in names {
                assert_eq!(
                    assert_sanitized_is_valid(name),
                    format!("{name}_"),
                    "`{name}`, declared by {library}",
                );
            }
        }
    }

    /// Controls: a name that merely resembles a contract name is not one, and
    /// keeps its spelling. Rocq names are case-sensitive, and an affix makes a
    /// different identifier. The host binder is here too: a function named
    /// after it is harmless, because nothing inside `Section Host` names a
    /// function.
    #[test]
    fn names_near_a_contract_name_are_unchanged() {
        for name in [
            "bi_call",
            "BI_CALL",
            "BI_calls",
            "my_BI_call",
            "BI_call_",
            "BI_call_1",
            "Host",
            "hosts",
            "modules",
            "module_funcs",
            "ho",
        ] {
            assert_eq!(sanitize_rocq_identifier(name), name, "`{name}`");
            assert_eq!(contract_library_of(name), None, "`{name}`");
        }
    }

    /// Names no emitted term can be captured through are not contract names,
    /// and renaming a function after one would move it for no reason. A
    /// record's `Build_*` constructor and an inductive's eliminators are
    /// derived by Rocq and never spelled by the emitter. `Wasm_int` is declared
    /// by the contract, but as a module, and a module lives in its own
    /// namespace: a `Definition Wasm_int` leaves `Wasm_int.int_of_Z` resolving
    /// to the module.
    #[test]
    fn names_no_emitted_term_can_capture_are_unchanged() {
        for name in [
            "Build_module_func",
            "Build_module",
            "module_func_rect",
            "module_func_ind",
            "module_func_rec",
            "module_func_sind",
            "Wasm_int",
        ] {
            assert_eq!(sanitize_rocq_identifier(name), name, "`{name}`");
            assert_eq!(contract_library_of(name), None, "`{name}`");
        }
    }

    /// Whether `name` ends in `_` followed by digits only, the shape the
    /// function-name disambiguator appends.
    fn ends_in_an_index_suffix(name: &str) -> bool {
        name.rsplit_once('_')
            .is_some_and(|(_, tail)| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()))
    }

    /// The invariants every entry must keep for the escape to be sound.
    ///
    /// - It is a legal identifier, or escaping it would produce an illegal one.
    /// - It does not end in `_`, so the single `_` the escape appends can never
    ///   form a `__` run, which the module/spec separator reserves.
    /// - It does not end in `_<digits>`, so the disambiguator, which appends
    ///   exactly that, can never mint it.
    /// - It does not start with `valid_`, so no module's `Theorem valid_<mod>`
    ///   can bind it.
    /// - It is listed once, within and across libraries, so
    ///   [`contract_library_of`] names exactly one owner, and each group is
    ///   sorted, so a diff adding a name shows where it belongs.
    /// - It is disjoint from the keyword, stdlib and preamble-helper lists and
    ///   from the host binder. A name in two of them is a classification error:
    ///   each list fixes a different treatment.
    #[test]
    fn contract_list_shape() {
        assert!(!ROCQ_CONTRACT_NAMES.is_empty(), "the contract list is empty");
        let mut owners: FxHashMap<&str, &str> = FxHashMap::default();
        let mut libraries: FxHashSet<&str> = FxHashSet::default();
        for &(library, names) in ROCQ_CONTRACT_NAMES {
            assert!(libraries.insert(library), "{library} has two groups");
            assert!(!names.is_empty(), "{library}'s group is empty");
            for pair in names.windows(2) {
                assert!(
                    pair[0] < pair[1],
                    "{library}'s names must be sorted and distinct: `{}` then `{}`",
                    pair[0],
                    pair[1],
                );
            }
            for &name in names {
                assert!(
                    !REJECTED_ROCQ_KEYWORDS.contains(&name),
                    "`{name}` is both a keyword and a contract name",
                );
                assert!(
                    !REJECTED_ROCQ_STDLIB_NAMES.contains(&name),
                    "`{name}` is both a stdlib name and a contract name",
                );
                assert!(
                    !PREAMBLE_HELPER_NAMES.contains(&name),
                    "`{name}` is both a preamble helper and a contract name",
                );
                assert_ne!(
                    name, HOST_INSTANCE_BINDER,
                    "the host binder is also a contract name",
                );
                assert!(
                    validate_rocq_identifier(name).is_ok(),
                    "`{name}` is not a legal Rocq identifier",
                );
                assert!(!name.ends_with('_'), "`{name}` ends in `_`");
                assert!(
                    !ends_in_an_index_suffix(name),
                    "`{name}` ends in a disambiguator suffix",
                );
                assert!(
                    !name.starts_with("valid_"),
                    "`{name}` starts with `valid_`",
                );
                if let Some(first) = owners.insert(name, library) {
                    panic!("`{name}` is listed under both {first} and {library}");
                }
                assert_eq!(contract_library_of(name), Some(library), "`{name}`");
            }
        }
    }

    /// The shape check's own predicate, against both of its outcomes.
    #[test]
    fn index_suffix_shape_is_recognised() {
        for name in ["main_0", "nat__1", "f_12", "BI_call_3"] {
            assert!(ends_in_an_index_suffix(name), "`{name}`");
        }
        for name in ["i32", "T_i32", "Tp_i8", "main_", "main", "f_1a"] {
            assert!(!ends_in_an_index_suffix(name), "`{name}`");
        }
    }

    #[test]
    fn over_length_names_are_truncated() {
        let out = assert_sanitized_is_valid(&"a".repeat(400));
        assert!(out.len() <= 255, "must respect the 255-char cap: {}", out.len());
    }
}

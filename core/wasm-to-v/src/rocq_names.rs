//! Rocq-reserved name lists and the `validate_rocq_identifier` helper used
//! to gate module and spec names emitted into generated `.v` files.

use crate::errors::{InvalidIdentifierReason, WasmToVError};

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

/// Validates that joining `mod_name` and `spec_name` into the emitted Rocq
/// grammar does not fabricate the reserved `__` separator at a join boundary.
///
/// The translator joins the two names into one grammar family: the obligation
/// definitions `<mod_name>__<spec_name>_specs` (with its per-entry
/// `_hspec{k}` members) and, for reachability partitions,
/// `<mod_name>__<spec_name>_ex_specs` / `_uq_specs` (with `_exspec{k}` /
/// `_uqspec{k}` members), plus the theorem names
/// `valid_<mod_name>__<spec_name>`, `valid_exists_<mod_name>__<spec_name>`,
/// and `valid_unique_<mod_name>__<spec_name>`. Every member of the family
/// contains the same `<mod_name>__<spec_name>` join, so one check covers all
/// of them. Each component already passed [`validate_rocq_identifier`], so
/// neither carries an internal `__` and neither starts with `_`. The remaining
/// hazard is a component that *ends* with `_`: the module name then abuts the
/// `__` separator (`app_` -> `app___Foo`), and the spec name abuts a trailing
/// `_`-led suffix (`Spec_` -> `main__Spec__specs`, and identically
/// `main__Spec__ex_specs` or `valid_exists_main__Spec_`-adjacent forms). Both
/// produce a `__` run inside the joined name, which the `<module>__<spec>`
/// split reserves. The diagnostic shows the `_specs` member as the
/// representative fabricated name.
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
/// Rewrite rules (each chosen to map the legal grammar to itself, so already
/// valid names are returned unchanged):
/// - Characters outside `[A-Za-z0-9_]` become `_`.
/// - A leading non-letter is prefixed with `f_` (Rocq reserves `_`-leading and
///   digit-leading identifiers).
/// - A `__` run is collapsed to `_` (the module/spec separator is reserved).
/// - A name colliding with a reserved keyword or stdlib name is suffixed `_`.
/// - An over-length name is truncated to the 255-character cap.
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
    use super::{sanitize_rocq_identifier, validate_rocq_identifier, validate_spec_join_boundary};
    use crate::errors::WasmToVError;

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
        for name in ["nat", "Nat", "list", "Some"] {
            let out = assert_sanitized_is_valid(name);
            assert_ne!(out, name, "stdlib name `{name}` must be escaped");
        }
    }

    #[test]
    fn over_length_names_are_truncated() {
        let out = assert_sanitized_is_valid(&"a".repeat(400));
        assert!(out.len() <= 255, "must respect the 255-char cap: {}", out.len());
    }
}

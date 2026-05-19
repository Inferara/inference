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
///   vernacular/Gallina keywords. `Nat` (capitalized) is allowed even though
///   the stdlib has both `Nat` and `nat`, because user code conventionally
///   uses the capitalized form for module names.
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

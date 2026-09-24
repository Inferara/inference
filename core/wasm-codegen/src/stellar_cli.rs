//! What the `stellar` CLI derives from a contract method's parameter list, and
//! how it resolves a flag to a parameter.
//!
//! `stellar contract invoke` (`stellar` CLI 28.0.0) builds a method's command
//! line in `build_custom_cmd` (`cmd/soroban-cli/src/commands/contract/arg_parsing.rs`
//! at tag `v28.0.0`). Each parameter gets two flags: `--<name>`, the one
//! `--help` lists, and an alias `--help` does not list, the name spelled by
//! `heck` 0.5.0's `to_kebab_case` ([`stellar_cli_flag_alias`]). The parameters
//! are added to the command in the iteration order of a `HashMap`, which
//! changes from run to run, and `clap_builder` 4.6.0, which that source locks,
//! resolves a flag to the first argument declaring it. So when one parameter's
//! name is another's alias, or two names are identical, the flag `--help`
//! lists for one of them may reach the other; [`stellar_flag_collision`] finds
//! the first such pair.
//!
//! This module is the CLI's mechanism, not a policy. Both Stellar gates — the
//! source-level one in [`codegen`](crate::codegen) and the Val-ABI rewriter in
//! `inference-stellar-abi` — refuse a method with such a pair, each with its
//! own error, message and place in its rule order, and both ask this module
//! which pair to name, so the two cannot name different ones.

/// The first two parameter names the `stellar` CLI cannot tell apart, as
/// zero-based indices into `names` in declaration order — the earliest name
/// that collides with a later one, beside the earliest such later one — with
/// the flag the two claim, without its `--`.
///
/// Two names collide when they are identical, or when one of them is the flag
/// the CLI derives for the other ([`stellar_cli_flag_alias`]). A caller builds
/// its refusal from `names[first]` and `names[second]`.
#[must_use = "a collision matters only to a caller that refuses the method for it"]
pub fn stellar_flag_collision<'a>(names: &[&'a str]) -> Option<(usize, usize, &'a str)> {
    names.iter().enumerate().find_map(|(first, earlier)| {
        names
            .iter()
            .enumerate()
            .skip(first + 1)
            .find_map(|(second, later)| {
                contested_flag(earlier, later).map(|flag| (first, second, flag))
            })
    })
}

/// The flag two parameter names both claim at the `stellar` CLI that `--help`
/// lists for one of them, without its `--`: the name itself when the two are
/// identical, or the name that is the flag the CLI derives for the other.
///
/// Compared byte for byte, whatever the names hold. Names like `_x` and `__x`
/// share the alias `--x`, which is contested, but each keeps a `--<name>` of
/// its own, and that is the flag `--help` lists.
fn contested_flag<'a>(a: &'a str, b: &'a str) -> Option<&'a str> {
    if a == b || a == stellar_cli_flag_alias(b) {
        Some(a)
    } else if b == stellar_cli_flag_alias(a) {
        Some(b)
    } else {
        None
    }
}

/// The second flag `stellar contract invoke` gives a contract method's
/// parameter, without its `--`: the parameter's name in kebab case.
///
/// `stellar` CLI 28.0.0 builds a method's command line in `build_custom_cmd`
/// (`cmd/soroban-cli/src/commands/contract/arg_parsing.rs` at tag `v28.0.0`).
/// Each parameter gets the flag `--<name>` and, as an alias, the name spelled
/// by `heck` 0.5.0's `to_kebab_case`. This function transcribes that
/// conversion — `transform` in heck's `src/lib.rs`, with the lowercasing word
/// writer and the `-` separator of its `src/kebab.rs` — and the test tier holds
/// it equal to the crate itself:
///
/// - Words are split at every character that is not alphanumeric, so a run of
///   `_` separates two words and contributes nothing itself.
/// - Inside an alphanumeric run, a word ends after a character read in
///   lowercase mode that is followed by an uppercase letter (`toAddr` is `to`
///   and `Addr`), and before the last uppercase letter of an uppercase run
///   followed by a lowercase one (`HTTPServer` is `HTTP` and `Server`). A
///   character that is neither case keeps the mode it follows, so a digit
///   after a lowercase letter ends a word before an uppercase one: `a1B` is
///   `a1` and `B`.
/// - Each word is lowercased, and the words are joined with `-`.
///
/// `_x`, `__x`, `x_` and `X` all become `x`, and `to_addr`, `_to_addr` and
/// `toAddr` all become `to-addr`. An alias holding `-` is never an Inference
/// identifier, so only a one-word alias — `_x`'s, `X`'s, `Amount`'s, `HTTP`'s
/// `http` — can be another parameter's name. The transcription gives heck's
/// result for every string, not only for identifiers, under the Unicode tables
/// the toolchain is built with: its case predicates are Unicode's, as heck's
/// are, and it keeps heck's one special case, a word-final `Σ` lowercased to
/// `ς`. For ASCII, which every Inference identifier is, the two cannot differ.
#[must_use = "the alias matters only compared with the method's other parameter names"]
pub fn stellar_cli_flag_alias(name: &str) -> String {
    let mut words = Vec::new();
    for run in name.split(|ch: char| !ch.is_alphanumeric()) {
        push_kebab_words(run, &mut words);
    }
    let lowercased: Vec<String> = words.into_iter().map(kebab_lowercase).collect();
    lowercased.join("-")
}

/// Splits one alphanumeric run into words at heck 0.5.0's case boundaries,
/// scanning as its `transform` does, and pushes them onto `words`.
fn push_kebab_words<'a>(run: &'a str, words: &mut Vec<&'a str>) {
    /// The case of the last cased character of the current word: heck's
    /// `WordMode`.
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Boundary,
        Lowercase,
        Uppercase,
    }

    let mut chars = run.char_indices().peekable();
    let mut start = 0;
    let mut mode = Mode::Boundary;
    while let Some((index, ch)) = chars.next() {
        let Some(&(next_index, next)) = chars.peek() else {
            words.push(&run[start..]);
            break;
        };
        let next_mode = if ch.is_lowercase() {
            Mode::Lowercase
        } else if ch.is_uppercase() {
            Mode::Uppercase
        } else {
            mode
        };
        if next_mode == Mode::Lowercase && next.is_uppercase() {
            words.push(&run[start..next_index]);
            start = next_index;
            mode = Mode::Boundary;
        } else if mode == Mode::Uppercase && ch.is_uppercase() && next.is_lowercase() {
            words.push(&run[start..index]);
            start = index;
            mode = Mode::Boundary;
        } else {
            mode = next_mode;
        }
    }
}

/// One word lowercased as heck 0.5.0's `lowercase` does it: character by
/// character, with a word-final `Σ` written `ς`.
fn kebab_lowercase(word: &str) -> String {
    let mut lowered = String::with_capacity(word.len());
    let mut chars = word.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == 'Σ' && chars.peek().is_none() {
            lowered.push('ς');
        } else {
            lowered.extend(ch.to_lowercase());
        }
    }
    lowered
}

#[cfg(test)]
mod tests {
    use super::{stellar_cli_flag_alias, stellar_flag_collision};

    /// The second flag is the name in `heck` 0.5.0's kebab case. The test tier
    /// holds the transcription equal to the crate itself over a wide table;
    /// these rows pin each rule the doc states, so a change to one is seen
    /// here too.
    #[test]
    fn the_flag_alias_is_the_name_in_kebab_case() {
        for (name, alias) in [
            ("x", "x"),
            ("_x", "x"),
            ("__x", "x"),
            ("x_", "x"),
            ("X", "x"),
            ("Amount", "amount"),
            ("__", ""),
            ("to_addr", "to-addr"),
            ("_to_addr", "to-addr"),
            ("toAddr", "to-addr"),
            ("HTTPServer", "http-server"),
            ("ABc", "a-bc"),
            ("a1B", "a1-b"),
            ("x1_2", "x1-2"),
            ("ΑΣ", "ας"),
        ] {
            assert_eq!(stellar_cli_flag_alias(name), alias, "the alias of `{name}`");
        }
    }

    /// The pair found is the first in declaration order: the earliest name
    /// that collides with any later one, beside the earliest later one it
    /// collides with, as zero-based indices, with the flag the two claim. The
    /// rewriter's own test takes these rows through its refusal, and the parity
    /// tier holds both gates to one pair on programs of this shape.
    #[test]
    fn the_collision_found_is_the_first_pair_in_declaration_order() {
        for (names, pair) in [
            (&["a", "b", "_b"][..], (1, 2, "b")),
            (&["a", "b", "_b", "_a"], (0, 3, "a")),
            (&["x", "_x", "__x"], (0, 1, "x")),
            (&["_a", "b", "a"], (0, 2, "a")),
            (&["_x", "__x", "x"], (0, 2, "x")),
            (&["y", "_x", "__x", "X", "x"], (1, 4, "x")),
        ] {
            assert_eq!(stellar_flag_collision(names), Some(pair), "{names:?}");
        }
    }
}

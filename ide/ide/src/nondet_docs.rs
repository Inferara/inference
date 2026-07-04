//! Verbatim hover documentation and inlay-hint texts for the non-deterministic
//! constructs (`forall` / `exists` / `unique` / `assume` / `@`).
//!
//! These strings are the mental-model aid requested in issue #33: hovering a
//! non-det keyword explains its proof obligation, and an inlay hint restates it
//! inline at the block header. The content is authored in
//! `.claude/docs/issues/33/nondet_texts.md` and reproduced here **verbatim** —
//! this module is the single in-code source of truth so the feature layer never
//! parses markdown at runtime.
//!
//! The hover payloads are markdown (with a fenced `inference` example); the inlay
//! texts are one short line each, all beginning with the `▸ ` non-det marker and
//! kept `<= 60` chars, per the style convention in the source document.

use inference_ast::nodes::BlockKind;

/// Hover markdown for a `forall` block or `forall`-marked function body.
pub(crate) const FORALL_HOVER: &str = r"**`forall` — every path must succeed**

A `forall` block (or a `forall`-marked function body) fans the computation out
into one path per possible value of every `@` inside it, and requires **all** of
them to reach the end successfully. If even a single path can fail an `assert`,
the block fails. Execution continues past the block only when the property held
for *every* combination of values.

**Verification meaning.** Lowers to the `BI_forall` quantifier constructor in the
generated Rocq. The prover's obligation is universal: show the block's assertions
hold on **every** path — for all values the inner `@`s could take. One
counterexample discharges nothing; it sinks the proof.

```inference
fn add_is_commutative(a: i32, b: i32) forall {
    // holds for all a, all b
    assert(add(a, b) == add(b, a));
}
```";

/// Hover markdown for an `exists` block.
pub(crate) const EXISTS_HOVER: &str = r"**`exists` — at least one path must succeed**

An `exists` block fans the computation out the same way `forall` does, but only
requires **one** path to reach the end successfully. It states that a solution is
*possible*: some assignment of the inner `@` values makes the block hold.

**Verification meaning.** Lowers to the `BI_exists` quantifier constructor. The
obligation is existential: exhibit **one** witness path that succeeds. A single
working assignment of the `@` values discharges it — the other paths are free to
fail.

```inference
exists {
    let n: i32 = @;
    // proves a solution exists: some n satisfies n * n == 25
    assume { assert(n * n == 25); }
}
```";

/// Hover markdown for a `unique` block.
pub(crate) const UNIQUE_HOVER: &str = r#"**`unique` — exactly one path must succeed**

A `unique` block requires **exactly one** path to reach the end successfully — no
more, no fewer. It states that a solution both *exists* and is *the only one*.

**Verification meaning.** Conceptually the counting quantifier "there is exactly
one". The obligation has two halves: existence (at least one path succeeds) **and**
uniqueness (no two distinct paths both succeed). Proving only that a solution
exists is not enough — you must also rule out a second one.

```inference
unique {
    let n: i32 = @;
    // exactly one n survives the filter: n == 4
    assume { assert(n * 3 == 12); }
}
```"#;

/// Hover markdown for an `assume` block.
pub(crate) const ASSUME_HOVER: &str = r"**`assume` — keep only the paths where this holds**

An `assume` block is a **filter**, not a quantifier. It drops every path on which
its body does not succeed and lets the survivors continue. Use it to state a
precondition: the assertions *after* the `assume` only have to hold on the paths
that made it through.

**Verification meaning.** Lowers to the `BI_assume` constructor (which keeps its
block type). It adds no goal of its own — instead it introduces its condition as a
**hypothesis** the prover may rely on for the rest of the enclosing block, and
narrows what a surrounding `forall`/`exists` has to cover.

```inference
forall {
    let x: i32 = @;
    assume { assert(x > 0); }  // keep only the positive paths
    assert(x - 1 >= 0);        // now provable, given x > 0
}
```";

/// Hover markdown for the `@` (uzumaki) value.
pub(crate) const UZUMAKI_HOVER: &str = r"**`@` (uzumaki) — every value of its type, at once**

`@` is a value that simultaneously stands for **all** values of the type it is
assigned to. It is *not* a random pick: writing `let x: i32 = @;` makes `x` range
over every `i32`, and the block around it splits into one path per value. `@` is
only meaningful inside a non-det block (`forall` / `exists` / `unique` / `assume`),
which is what quantifies over the values it produces.

**Verification meaning.** Lowers to `BI_uzumaki_num` (`T_i32` / `T_i64`). It is the
source of the quantified variable the surrounding block ranges over — universally
under `forall`, existentially under `exists`.

```inference
forall {
    let x: i32 = @;      // x stands for every i32 value
    assert(x * 0 == 0);  // must hold for all of them
}
```";

/// Inlay-hint text shown at the end of a `forall` block header.
pub(crate) const FORALL_INLAY: &str = "▸ every path must succeed";
/// Inlay-hint text shown at the end of an `exists` block header.
pub(crate) const EXISTS_INLAY: &str = "▸ at least one path must succeed";
/// Inlay-hint text shown at the end of a `unique` block header.
pub(crate) const UNIQUE_INLAY: &str = "▸ exactly one path must succeed";
/// Inlay-hint text shown at the end of an `assume` block header.
pub(crate) const ASSUME_INLAY: &str = "▸ keeps only paths where this holds";
/// Inlay-hint text shown just after a `= @` uzumaki binding.
pub(crate) const UZUMAKI_INLAY: &str = "▸ ranges over every value of its type";

/// The source keyword that opens a non-det block, or `None` for a regular block.
///
/// A non-det block's source range begins at this keyword, so its byte length is
/// what separates the keyword span (for keyword hover) from the block header end
/// (for the inlay-hint anchor).
#[must_use]
pub(crate) fn block_keyword(kind: BlockKind) -> Option<&'static str> {
    match kind {
        BlockKind::Forall => Some("forall"),
        BlockKind::Exists => Some("exists"),
        BlockKind::Unique => Some("unique"),
        BlockKind::Assume => Some("assume"),
        BlockKind::Regular => None,
    }
}

/// The hover markdown for a non-det block kind, or `None` for a regular block.
#[must_use]
pub(crate) fn block_hover(kind: BlockKind) -> Option<&'static str> {
    match kind {
        BlockKind::Forall => Some(FORALL_HOVER),
        BlockKind::Exists => Some(EXISTS_HOVER),
        BlockKind::Unique => Some(UNIQUE_HOVER),
        BlockKind::Assume => Some(ASSUME_HOVER),
        BlockKind::Regular => None,
    }
}

/// The inlay-hint text for a non-det block kind, or `None` for a regular block.
#[must_use]
pub(crate) fn block_inlay(kind: BlockKind) -> Option<&'static str> {
    match kind {
        BlockKind::Forall => Some(FORALL_INLAY),
        BlockKind::Exists => Some(EXISTS_INLAY),
        BlockKind::Unique => Some(UNIQUE_INLAY),
        BlockKind::Assume => Some(ASSUME_INLAY),
        BlockKind::Regular => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_nondet_kind_maps_to_keyword_hover_and_inlay() {
        for kind in [
            BlockKind::Forall,
            BlockKind::Exists,
            BlockKind::Unique,
            BlockKind::Assume,
        ] {
            let keyword = block_keyword(kind).expect("non-det kind has a keyword");
            assert_eq!(keyword.len(), 6, "every non-det keyword is six bytes");
            assert!(block_hover(kind).is_some());
            assert!(block_inlay(kind).unwrap().starts_with("▸ "));
        }
    }

    #[test]
    fn regular_block_has_no_nondet_content() {
        assert!(block_keyword(BlockKind::Regular).is_none());
        assert!(block_hover(BlockKind::Regular).is_none());
        assert!(block_inlay(BlockKind::Regular).is_none());
    }

    #[test]
    fn inlay_texts_are_within_the_length_budget() {
        for text in [
            FORALL_INLAY,
            EXISTS_INLAY,
            UNIQUE_INLAY,
            ASSUME_INLAY,
            UZUMAKI_INLAY,
        ] {
            assert!(text.starts_with("▸ "), "inlay marker is the black triangle");
            assert!(
                text.chars().count() <= 60,
                "inlay text stays under 60 chars"
            );
        }
    }
}

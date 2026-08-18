//! The `hassert` verification-obligation IR and its `inference.hspecs`
//! custom-section codec.
//!
//! A proof-mode Inference build turns each specification free function into
//! one logical obligation: a value of the wasm-verifier `hassert` assertion
//! type (theories/`Assertions.v`). [`HAssert`] and [`HTerm`] are the Rust
//! mirror of that inductive; each obligation entry additionally carries its
//! quantifier kind ([`SpecKind`]) — `Forall` for a universal (`ValidSpec`)
//! payload, `Exists`/`Unique` for a reachability payload whose [`ReachMeta`]
//! records the entry arity and source-visible frame slots the downstream
//! `reachability_spec` record needs. [`encode`]/[`decode`] serialize a whole
//! program's obligations into the `inference.hspecs` WASM custom section so
//! they survive linking and reach the Rocq translator.
//!
//! ## Why a separate leaf crate
//!
//! The obligations are *produced* by `wasm-codegen`, *carried verbatim* by
//! `wasm-linker`, and *consumed* by `wasm-to-v`. Placing the IR and its wire
//! format in any one of those crates would force the other two to depend on it
//! (the reason the linker today keeps a hand-copied duplicate of the
//! `inference.spec_funcs` codec). A dependency-light leaf crate below all three
//! gives every phase a single source of truth for both the data model and the
//! bytes on disk — the same layering rationale as [`inference-fn-key`].
//!
//! ## Function references are symbolic
//!
//! [`HFnRef`] stores a WASM name-section function symbol as an opaque
//! non-empty string. The static linker deletes imports and shifts every
//! function index, so an index-based reference would need remapping at link
//! time; a symbolic reference is carried through the merge untouched and
//! resolved to a `mod_funcs` index only by `wasm-to-v`, which alone knows the
//! emitted module's final function layout.
//!
//! That indirection is also what lets an obligation name a *linked external*:
//! the external has no defined body when the obligation is built, and acquires
//! one — under the name `inference_fn_key::merged_name::root` gives it — only
//! at the merge.
//!
//! ## Deliberate deviations from wasm-verifier's inductive
//!
//! The IR omits constructs Inference specifications can never contain, so an
//! ill-formed obligation is unrepresentable rather than merely rejected:
//!
//! - no floating-point number types ([`HNumType`] is `I32`/`I64` only);
//! - no `T_global` term (specifications cannot reference globals);
//! - no heap fragment (`HA_emp`/`HA_star`/`HA_iter`/`HA_pto`/`HA_size`);
//! - no general `HA_pred`: [`HAssert::TermEq`] is the only predicate form,
//!   enforcing wasm-verifier's `pred_eq`/2 discipline by construction.
//!
//! Implication, disjunction and universal quantification are *explicit*
//! [`HAssert::Imp`]/[`HAssert::Or`]/[`HAssert::All`] nodes rather than their
//! classical De Morgan encodings. wasm-verifier's `Himpl`/`Hor`/`Hall` are
//! definitionally-transparent `Definition`s, so the downstream printer can
//! render these nodes by name without ever pattern-matching an encoding.
//!
//! [`HAssert::All`] is also what keeps quantifier *alternation* honest. The
//! downstream `ValidSpec` judgment already quantifies the payload's free
//! variables universally, so encoding an inner universal as anything but a
//! binder of its own — a free slot, say — would read as an outer `∀` over an
//! inner `∃` and silently swap the two.

#![warn(clippy::pedantic)]

mod codec;
mod ir;

pub use codec::{
    DecodeError, HSPECS_SECTION_NAME, HSPECS_SECTION_VERSION, MAX_NAME_LEN, MAX_TREE_DEPTH,
    MAX_VISIBLE_LOCS, PayloadError, decode, encode, validate,
};
pub use ir::{
    HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecEntry, HSpecMap, HTerm, ReachMeta,
    SpecKind,
};

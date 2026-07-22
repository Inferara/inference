//! The `hassert` verification-obligation IR and its `inference.hspecs`
//! custom-section codec.
//!
//! A proof-mode Inference build turns each `forall`-quantified specification
//! function into one logical obligation: a value of the wasm-verifier
//! `hassert` assertion type (theories/`Assertions.v`). [`HAssert`] and
//! [`HTerm`] are the Rust mirror of that inductive, and [`encode`]/[`decode`]
//! serialize a whole program's obligations into the `inference.hspecs` WASM
//! custom section so they survive linking and reach the Rocq translator.
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
//! non-empty string (producers write `FnKey::Display`). The static linker
//! deletes imports and shifts every function index, so an index-based reference
//! would need remapping at link time; a symbolic reference is carried through
//! the merge untouched and resolved to a `mod_funcs` index only by `wasm-to-v`,
//! which alone knows the emitted module's final function layout.
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
//! Implication and disjunction are *explicit* [`HAssert::Imp`]/[`HAssert::Or`]
//! nodes rather than their classical De Morgan encodings. wasm-verifier's
//! `Himpl`/`Hor` are definitionally-transparent `Definition`s, so the
//! downstream printer can render these nodes as `Himpl`/`Hor` without ever
//! pattern-matching an encoding.

#![warn(clippy::pedantic)]

mod codec;
mod ir;

pub use codec::{
    DecodeError, HSPECS_SECTION_NAME, HSPECS_SECTION_VERSION, MAX_NAME_LEN, MAX_TREE_DEPTH,
    PayloadError, decode, encode, validate,
};
pub use ir::{HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecEntry, HSpecMap, HTerm};

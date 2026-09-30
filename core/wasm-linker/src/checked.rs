//! Codec for the `inference.checked` custom section, and the reachability check
//! it exists for.
//!
//! ## What the section says
//!
//! It lists the functions of one module whose bodies trap rather than wrapping
//! when a `+`, `-`, `*` or unary `-` leaves its type. **Absence means the module
//! carries no Inference-emitted overflow guard** — which is exactly true of a
//! module produced by any other toolchain, since none of them emits one, so a
//! foreign `.wasm` with no section is read correctly rather than merely
//! conservatively. The producer never writes an empty section, so presence and a
//! non-empty list say the same thing.
//!
//! ## Why the linker keeps it
//!
//! The merge drops every custom section it does not name, because carrying an
//! unknown one forward would restate, about the merged module, a claim its
//! producer made about a different one. This section is kept for the same reason
//! `inference.spec_funcs` is: it is a verification deliverable whose meaning
//! survives the merge exactly once its indices are rewritten into the output
//! space — and unlike the other two it is *also* read here, by
//! [`crate::merge`]'s reachability check, which is the only consumer anywhere.
//! The Rocq translator never reads it.
//!
//! ## Payload format (LEB128 u32 throughout)
//!
//! Two forms, told apart by the leading version.
//!
//! ```text
//! version = 1          -- the exact form
//! count                -- number of guarded functions
//! repeat `count` times: func_idx   -- strictly ascending
//! ```
//!
//! ```text
//! version = 2          -- the opaque form; nothing follows
//! ```
//!
//! The exact form names the guarded functions. The opaque form says only that
//! *some* function of the module carries a guard and that which ones can no
//! longer be told: it is written by `infs` over an artifact a post-build
//! optimizer has just rewritten, because Binaryen carries an unknown custom
//! section through untouched while inlining, removing and reordering the
//! functions its indices named. Code generation never emits it — a module the
//! compiler produced always knows its own guarded set — and neither does this
//! crate, except when re-emitting a merge that absorbed one.
//!
//! The format mirrors `inference_wasm_codegen::checked_section`; the linker
//! keeps a self-contained copy rather than depend on the codegen crate, exactly
//! as it does for `inference.spec_funcs`. The codec itself is [`crate::func_list`],
//! which `inference.bounds_elided` shares. The decoder is fully bounds-checked: a
//! malformed external `.wasm` must surface a clean [`LinkError`], never a panic
//! or an unbounded allocation.

use crate::LinkError;
use crate::func_list::{self, FuncList};

/// The custom-section name listing the functions that carry an overflow guard.
/// Kept in lock-step with `inference_wasm_codegen`'s emitter; the linker keeps
/// its own copy to avoid depending on the codegen crate.
pub(crate) const SECTION_NAME: &str = "inference.checked";

/// Wire-format version of the exact form, which names every guarded function.
/// Kept in lock-step with the codegen emitter.
pub(crate) const VERSION: u32 = func_list::EXACT_VERSION;

/// What one module's `inference.checked` section says about its own functions:
/// [`FuncList::Exact`] names the guarded functions, so a walk can ask whether it
/// reached one of them, while [`FuncList::Opaque`] says only that the module has
/// at least one and that its producer can no longer say which — which leaves
/// every function of that module a candidate.
pub(crate) type CheckedGuards = FuncList;

/// Decodes the `inference.checked` payload into what its module claims.
///
/// # Errors
///
/// Returns [`LinkError::Parse`] on any malformed input; see
/// [`func_list::decode`].
pub(crate) fn decode(data: &[u8]) -> Result<CheckedGuards, LinkError> {
    func_list::decode(data, "checked section")
}

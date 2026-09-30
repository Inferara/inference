//! The `inference.bounds_elided` custom section: which functions of a module
//! hold an array access code generation emitted without its runtime bounds
//! guard.
//!
//! ## What the section says
//!
//! A listed function contains at least one array access whose index the
//! analysis proved in bounds and whose guard a build under the `omit-proven`
//! bounds-check policy therefore left out: at that access, safety rests on the
//! proof alone. **Absence means every dynamic array access Inference emitted in
//! the module keeps its guard.** A module another toolchain produced has no
//! Inference guard to omit, so its missing section says nothing about how it
//! indexes memory: the record is about this compiler's lowering, not a verdict
//! on the module. The producer never writes an empty section.
//!
//! ## Why the linker keeps it
//!
//! It is a record for whoever holds the artifact — an auditor, or a project that
//! keeps every guard and links a library that did not — and nothing in the
//! instructions says which policy produced them: an access emitted without its
//! guard is an ordinary load or store. The merge drops every custom section it
//! does not name, so without this the merged module would lose the record in
//! exactly the case it exists for, a guarded program absorbing unguarded
//! library bodies. Unlike `inference.checked` it decides nothing here, and no
//! link is refused over it. It is carried through the same index mappings the
//! bodies are, so it names the same bodies after the merge that it named
//! before, and a library function no closure pulled in drops out of it.
//!
//! The payload is [`crate::func_list`]'s: the exact form code generation
//! writes, or the opaque form `infs` writes over an artifact a post-build
//! optimizer has renumbered. The name and version mirror
//! `inference_wasm_codegen`'s emitter; the linker keeps its own copy rather than
//! depend on the codegen crate, as it does for `inference.checked`.

use crate::LinkError;
use crate::func_list::{self, FuncList};

/// The custom-section name listing the functions holding an access emitted
/// without its bounds guard. Kept in lock-step with `inference_wasm_codegen`'s
/// emitter.
pub(crate) const SECTION_NAME: &str = "inference.bounds_elided";

/// Wire-format version of the exact form. Kept in lock-step with the codegen
/// emitter.
pub(crate) const VERSION: u32 = func_list::EXACT_VERSION;

/// Decodes the `inference.bounds_elided` payload into what its module claims.
///
/// # Errors
///
/// Returns [`LinkError::Parse`] on any malformed input; see
/// [`func_list::decode`].
pub(crate) fn decode(data: &[u8]) -> Result<FuncList, LinkError> {
    func_list::decode(data, "bounds_elided section")
}

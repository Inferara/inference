//! Memory-merge feasibility tiers.
//!
//! Whether an external function can be merged into the single shared linear
//! memory depends on what its transitive closure touches:
//!
//! - **Tier A** — pure: no memory, no globals, no data/element segments, no
//!   tables. Merge is a copy + re-index.
//! - **Tier B** — memory via caller-passed pointers only: the closure
//!   loads/stores through addresses the caller supplies, but defines no static
//!   data of its own, no mutable globals, and no table/element entries. The one
//!   shared memory is enough; no address relocation is needed. Admission to
//!   Tier B requires *proof* (via [`crate::provenance`]) that every memory
//!   address derives from a function parameter — a closure that fabricates an
//!   address from a constant or its own state would alias the host program's
//!   memory and is rejected as Tier C instead.
//!   Tier B admission proves *derivation*, not *containment*: the addresses are
//!   shown to flow from caller parameters, not to stay inside the region the
//!   caller granted. `p + 1048576`, `p + q` and `2p` are all admitted, and a
//!   loop may walk a caller pointer off the end of any buffer. See the "What
//!   this proves, and what it does not" section of [`crate::provenance`] before
//!   relying on Tier B for a bounds property; issue #420 tracks closing it.
//! - **Tier C** — own static data, globals, or table/element entries: merging
//!   would require relocating data and rewriting absolute addresses, which
//!   needs relocation metadata the static merge does not consume. Rejected with
//!   a clear error.

use crate::closure::{Closure, ClosureEffects};
use crate::parse::ParsedModule;
use crate::provenance;
use crate::LinkError;

/// The feasibility tier of a merge candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier {
    /// Pure function: no memory, globals, data, or tables.
    A,
    /// Memory through caller-passed pointers only.
    B,
}

/// Classifies a closure against its source module, returning the tier or a
/// [`LinkError::RequiresRelocatableBuild`] for Tier-C inputs.
///
/// A module is Tier C when it carries any of the relocation-sensitive
/// constructs: its own data or element segments, defined globals (a baked-in
/// constant or mutable state), or table definitions / indirect-call use. These
/// imply absolute addresses or per-module state that a position-naive static
/// merge cannot reconcile across two modules sharing one memory.
///
/// A memory-touching closure is admitted to Tier B **only** when the
/// address-provenance analysis ([`provenance::verify_param_addressing`]) proves
/// every memory access — in the closure `root` and in every function it
/// transitively calls — addresses memory through a value derived from the
/// **root export's** parameters, on every reachable control-flow path. The
/// analysis is interprocedural: the root's parameters are the trusted caller
/// pointers, and an inner function's parameter is trusted only when *every*
/// reachable call site passes it a param-derived argument (a sound greatest
/// fixpoint over the call graph that handles self- and mutual recursion). A
/// closure that fabricates a memory address from a constant, a module-internal
/// source, a parameter-cancelling computation (`param - param`, `param & 0`, …),
/// a value laundered across a `call` boundary that the call site does not
/// justify, or an indirect/table-dispatched call result would silently alias the
/// host program's own linear memory, so it is rejected as Tier C rather than
/// merged.
pub(crate) fn classify(
    module: &ParsedModule,
    closure: &Closure,
    root: u32,
    field: &str,
) -> Result<Tier, LinkError> {
    let reasons = tier_c_reasons(module, &closure.effects);
    if !reasons.is_empty() {
        return Err(LinkError::RequiresRelocatableBuild {
            field: field.to_string(),
            reasons,
        });
    }

    if closure.effects.uses_memory {
        provenance::verify_param_addressing(module, &closure.local_func_indices, root, field)?;
        Ok(Tier::B)
    } else {
        Ok(Tier::A)
    }
}

/// Collects every reason the module fails Tier-A/B feasibility. Empty means the
/// module is mergeable.
fn tier_c_reasons(module: &ParsedModule, effects: &ClosureEffects) -> Vec<String> {
    let mut reasons = Vec::new();

    if module.data_count > 0 || effects.uses_data_segments {
        reasons.push("defines or initializes its own static data segments".to_string());
    }
    if !module.globals.is_empty() || effects.uses_globals {
        reasons.push("defines or accesses module globals".to_string());
    }
    if !module.tables.is_empty() || module.element_count > 0 || effects.uses_tables {
        reasons.push("uses a table / element segment (indirect calls)".to_string());
    }

    reasons
}

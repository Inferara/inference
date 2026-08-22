//! Root attribution: which of the closure **root's** parameters a merged body
//! may *store* through.
//!
//! [`super::verify_param_addressing`] proves that every memory address in a
//! closure derives from the root's parameters. It answers a yes/no question, and
//! the dependence it reasons over ([`super::Linear`]) is expressed in the *enclosing*
//! function's parameter space — so a store inside a helper three calls down from
//! the root carries a mask nothing outside that helper can read. This module
//! carries the answer the rest of the toolchain needs instead: **which root
//! parameters**, named in the root's own indices, the closure may write through.
//!
//! That is the space an `external fn`'s declaration is written in. A `mut` on an
//! extern parameter declares that the foreign body may store through the address
//! that parameter denotes, and a declaration can only be checked against a set
//! phrased in the same coordinates.
//!
//! ## The least fixpoint
//!
//! `origin[g][j]` is the set of root parameter indices that function `g`'s
//! parameter `j` may derive from:
//!
//! ```text
//! seed        origin[root][j] = {j};  every other function starts empty
//! transfer    for each call site f -> g and each callee parameter j:
//!                 origin[g][j] |= U { origin[f][i] : i in arg_dep(call, j).support }
//! collect     for each Store access with dependence dep in g:
//!                 W |= U { origin[g][i] : i in dep.support }
//! ```
//!
//! This is a **forward least** fixpoint, and it is a different computation from
//! [`super::compute_trusted_params`] — a *greatest* fixpoint answering "is this
//! parameter caller-derived". Neither can be phrased as the other: the trust
//! pass starts optimistic and removes what a call site contradicts, this one
//! starts empty and adds what a call site contributes. They share the summaries
//! and the [`super::arg_dep`] accessor that reads an argument out of a call
//! site.
//!
//! ## The transfer over-approximates, deliberately
//!
//! The transfer condition is `support` membership and nothing else: it never
//! consults [`super::is_live`] or the trust model. So the two passes are **not**
//! in step, and the direction they diverge in is the point. An argument the
//! trust pass rejects as unjustified — an opaque dependence, say, whose `odd`
//! mask is empty — still has a `support`, and its origins still flow into the
//! callee's parameter here. The write set that results is a superset of the one
//! a liveness-aware transfer would produce.
//!
//! They agree on exactly one case, and it is the one [`super::arg_dep`] itself
//! is responsible for: a call site whose recorded argument count disagrees with
//! the callee's arity yields the default dependence, whose `support` is empty,
//! so it justifies nothing there and contributes nothing here.
//!
//! **Do not narrow this transfer by consulting `is_live`.** A larger write set
//! rejects a link a finer analysis would admit; a smaller one admits a body
//! whose store no declaration covers, which is the unsound direction. The trust
//! pass may safely call an argument unjustified — its own answer is a *proof*
//! obligation, and failing it rejects. This pass's answer is a *may*-write
//! claim, and dropping a contributor from it would let a declaration cover less
//! than the bytes do.
//!
//! **The root is a transfer target like every other function.** The seed is an
//! initial value, not a fixed point. A root that recurses with its arguments
//! swapped and then stores through parameter 0 may, on the recursive
//! invocation, be storing through whatever the *caller* passed as parameter 1 —
//! so its true may-write set is `{0, 1}`, and such a closure is admitted today
//! (the same family as the pinned `ip4a`). Exempting the root, the way
//! [`super::compute_trusted_params`] exempts it from its own second clause,
//! would yield `{0}` and understate the write set. The two passes exempt
//! differently because they answer different questions: the root's *external*
//! caller justifies all of its parameters, but justifies none of them as
//! unwritten.
//!
//! ## Why `support`, never `odd`
//!
//! The derivation property is a statement about a bijection, so it reads
//! [`super::Linear::odd`]: what must be shown is that *some* parameter provably moves
//! the address. May-write is the opposite kind of claim — every possible
//! contributor counts — so it reads [`super::Linear::support`], which
//! over-approximates every parameter the affine form can mention. `p0 + (p1 <<
//! 2)` has `odd == {0}` and `support == {0, 1}`; attributing that store to `p0`
//! alone would let a declaration naming only `p0` cover a body storing at an
//! address `p1` chose.
//!
//! The over-approximation runs in the safe direction: a write set that is too
//! large rejects a link a finer analysis would admit, and never admits one it
//! should not.
//!
//! ## Fail-closed encoding
//!
//! [`WriteTargets::AllUnattributed`] is the widening for a store whose root
//! attribution could not be completed. It is deliberately **not** a bitset of
//! every parameter: [`super::ParamMask`] saturates at 64 bits, and its
//! fail-closed polarity inverts here. In the trust model an absent bit means
//! *untrusted*, so truncating the high-arity tail over-rejects; in a write set
//! an absent bit would mean *not written*, so the same truncation would let a
//! root with more than 64 parameters read as writing nothing. "Every root
//! parameter" is therefore its own variant, and the only place an index past the
//! mask range is ever expressed.
//!
//! `Exactly` cannot hold such an index, and that is a property of the proof
//! rather than of this module: [`super::summarize_function`] seeds a parameter
//! at index 64 or above as `NotParam`, and no lattice rule re-promotes a
//! `NotParam` contributor into a `Param` form, so an address touching parameter
//! 70 fails `is_live` and rejects the whole closure as Tier C before any write
//! set is read.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{AccessKind, FunctionSummary, arg_dep};

/// The root parameter indices a closure may store through.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WriteTargets {
    /// Exactly these root parameter indices, each reached through a completed
    /// attribution. Ascending, and never holding an index past the parameter
    /// mask's 64-bit range (see the module documentation).
    Exactly(BTreeSet<u32>),
    /// Every root parameter, because at least one store's root attribution could
    /// not be completed. The fail-closed widening, and the only producer of an
    /// index past the mask range.
    AllUnattributed,
}

/// Which of a closure root's parameters the closure may store through.
///
/// Three facts are separately observable, because their consumers are different:
///
/// - [`RootWriteSet::never_stores`] — the closure records no store *at all*.
///   Held **structurally**, as "no `Store` access exists", rather than inferred
///   from the parameter set being empty. It is the fact that licenses eliding a
///   caller's defensive copy, and that licence must not depend on the
///   attribution below being right.
/// - [`RootWriteSet::may_store_through`] — the whole attributed parameter set,
///   in the root's own coordinates. An over-approximation, and the shape the
///   attribution's own tests assert on; the merge reads the two answers above
///   plus [`RootWriteSet::first_undeclared`] instead, so it is test-only.
/// - [`RootWriteSet::is_unattributed`] — an attribution that could not be
///   completed, which widens the set to every root parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootWriteSet {
    root_param_count: usize,
    targets: WriteTargets,
    /// Whether the closure records any [`AccessKind::Store`] at all.
    records_stores: bool,
}

impl RootWriteSet {
    /// Whether the closure provably never writes linear memory: no function in
    /// it records a single store.
    ///
    /// This is a structural fact about the merged bytes, independent of the
    /// attribution — a closure with no store has nothing to attribute — so a
    /// consumer resting on it inherits none of the attribution's assumptions.
    #[must_use = "the licence to elide a caller's copy rests on this answer"]
    pub(crate) fn never_stores(&self) -> bool {
        !self.records_stores
    }

    /// Whether some store's root attribution could not be completed, widening
    /// the set to every root parameter.
    ///
    /// Unreachable on the path [`super::verify_param_addressing`] admits; the
    /// argument is recorded at [`super::summaries_are_rooted`].
    #[must_use = "an unattributed write set is a widening, not a failure to report"]
    pub(crate) fn is_unattributed(&self) -> bool {
        matches!(self.targets, WriteTargets::AllUnattributed)
    }

    /// The root parameter indices this closure may store through, ascending.
    #[cfg(test)]
    #[must_use = "the attributed parameters are the whole result of the analysis"]
    pub(crate) fn may_store_through(&self) -> Vec<u32> {
        match &self.targets {
            WriteTargets::Exactly(params) => params.iter().copied().collect(),
            WriteTargets::AllUnattributed => self.every_root_param().collect(),
        }
    }

    /// The lowest root parameter index this closure may store through that
    /// `declared` does not list, or `None` when the declaration covers the whole
    /// write set.
    ///
    /// `declared` is the parameter list a caller wrote down, so it is short and
    /// unordered; the returned index is the lowest offending one so a diagnostic
    /// built from it is deterministic.
    #[must_use = "the offending parameter index is what a rejection must name"]
    pub(crate) fn first_undeclared(&self, declared: &[u32]) -> Option<u32> {
        match &self.targets {
            WriteTargets::Exactly(params) => params.iter().copied().find(|p| !declared.contains(p)),
            WriteTargets::AllUnattributed => {
                self.every_root_param().find(|p| !declared.contains(p))
            }
        }
    }

    /// Every parameter index of the root, ascending. Saturates the count into
    /// `u32`, which no real function arity approaches.
    fn every_root_param(&self) -> std::ops::Range<u32> {
        0..u32::try_from(self.root_param_count).unwrap_or(u32::MAX)
    }
}

/// Computes the closure's root write set from the per-function summaries.
///
/// `root_param_count` is the arity of the closure root, which names the
/// coordinate space the result is phrased in; the caller resolves it from the
/// root's own signature, so a root missing from `summaries` cannot reach here.
pub(super) fn root_write_set(
    summaries: &BTreeMap<u32, FunctionSummary>,
    root: u32,
    root_param_count: usize,
) -> RootWriteSet {
    let origins = compute_root_origins(summaries, root, root_param_count);
    collect_root_writes(summaries, &origins, root_param_count)
}

/// For each function, the root parameter indices each of its own parameters may
/// derive from. Keyed by global function index, matching the summaries.
///
/// Ordered rather than hashed throughout: the write set is read back into
/// diagnostics that are compared byte for byte, so an iteration order that varies
/// between runs would surface as a flaky message. `rustc-hash` is also a
/// dev-dependency of this crate, so the usual `FxHashMap` preference would add a
/// runtime dependency here.
type OriginTable = BTreeMap<u32, Vec<BTreeSet<u32>>>;

/// The forward least fixpoint described in the module documentation, as a
/// worklist over `(function, parameter)` pairs.
///
/// A pair is enqueued when its origin set grows, and popping it propagates the
/// set across every call site that passes that parameter on. Growth is the only
/// thing that enqueues, and each set can grow at most `root_param_count` times,
/// so the total work is bounded by the number of call arguments times the
/// lattice height — a stated bound rather than "it converges eventually", which
/// matters because nothing in this crate caps a closure's function count.
fn compute_root_origins(
    summaries: &BTreeMap<u32, FunctionSummary>,
    root: u32,
    root_param_count: usize,
) -> OriginTable {
    let mut origins: OriginTable = summaries
        .iter()
        .map(|(&idx, summary)| (idx, vec![BTreeSet::new(); summary.param_count]))
        .collect();

    let mut worklist: VecDeque<(u32, usize)> = VecDeque::new();
    if let Some(root_origin) = origins.get_mut(&root) {
        for (j, slot) in root_origin.iter_mut().enumerate().take(root_param_count) {
            slot.insert(u32::try_from(j).unwrap_or(u32::MAX));
            worklist.push_back((root, j));
        }
    }

    while let Some((caller, i)) = worklist.pop_front() {
        let contribution = match origins.get(&caller).and_then(|o| o.get(i)) {
            Some(set) if !set.is_empty() => set.clone(),
            _ => continue,
        };
        let Some(caller_summary) = summaries.get(&caller) else {
            continue;
        };

        for call in &caller_summary.calls {
            let Some(callee_params) = summaries.get(&call.callee).map(|s| s.param_count) else {
                continue;
            };
            for j in 0..callee_params {
                if !arg_dep(call, j).support.contains(i) {
                    continue;
                }
                let mut grew = false;
                if let Some(slot) = origins.get_mut(&call.callee).and_then(|o| o.get_mut(j)) {
                    for &origin in &contribution {
                        grew |= slot.insert(origin);
                    }
                }
                if grew {
                    worklist.push_back((call.callee, j));
                }
            }
        }
    }

    origins
}

/// Unions the root attribution of every store in the closure.
///
/// A store contributes the origins of every parameter its address may mention.
/// Two shapes cannot be attributed and widen the result to every root parameter:
/// an address depending on no parameter at all, and an address depending on a
/// parameter whose own origin set is empty (nothing reached it from the root).
fn collect_root_writes(
    summaries: &BTreeMap<u32, FunctionSummary>,
    origins: &OriginTable,
    root_param_count: usize,
) -> RootWriteSet {
    let mut params: BTreeSet<u32> = BTreeSet::new();
    let mut records_stores = false;
    let mut unattributed = false;

    for (&func_idx, summary) in summaries {
        for access in &summary.accesses {
            if access.kind != AccessKind::Store {
                continue;
            }
            records_stores = true;

            if access.dep.support.is_empty() {
                unattributed = true;
                continue;
            }
            for i in access.dep.support.indices() {
                match origins.get(&func_idx).and_then(|o| o.get(i)) {
                    Some(origin) if !origin.is_empty() => params.extend(origin.iter().copied()),
                    _ => unattributed = true,
                }
            }
        }
    }

    let targets = if unattributed {
        WriteTargets::AllUnattributed
    } else {
        WriteTargets::Exactly(params)
    };

    RootWriteSet {
        root_param_count,
        targets,
        records_stores,
    }
}

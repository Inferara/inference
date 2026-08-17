//! Address-provenance analysis for Tier-B soundness.
//!
//! Tier B's contract is that a merged external touches the single shared memory
//! *only through addresses the caller passes in* — never an address it
//! fabricates from a constant or reads from its own global/stack. Such a
//! fabricated address would alias the host program's own linear memory at a
//! fixed offset, a silent miscompile the static merge cannot detect by section
//! inspection alone (the body validates and the export signature matches).
//!
//! This module proves the contract by a **sound, flow-sensitive,
//! interprocedural** abstract interpretation over the whole closure. The single
//! trusted source of addresses is the **closure root's** parameters — whatever
//! pointer the caller passes the satisfied export, the caller owns. Every memory
//! access, in the root or any function it transitively calls, must address
//! memory through a value that provably derives from a *trusted* parameter on
//! every reachable control-flow path. Anything not proven safe rejects the whole
//! closure as Tier C ([`LinkError::RequiresRelocatableBuild`]). Fail closed.
//!
//! For a bulk-memory op (`memory.fill`/`memory.copy`/`memory.init`) the **size /
//! extent** operand carries the *same* caller-derivation requirement as an
//! address. Such an op touches the contiguous region `[address, address + size)`,
//! so proving only the start caller-relative is not enough: a constant or global
//! extent would let the op clobber or read an unbounded span above a caller
//! pointer (`memory.fill(base, v, 0x8000)` scorches host memory the caller never
//! exposed) — the same unbounded-clobber the rejected counted-loop form achieves,
//! one instruction at a time. Modeling the extent with the address rule keeps the
//! realistic caller-owns-`(ptr, len)` pattern linkable while closing that escape.
//!
//! ## What this proves, and what it does not
//!
//! The property proved here is **derivation**, not **containment**: every
//! address is shown to *flow from* a caller-supplied parameter. Nothing here
//! shows that an address stays *inside the region the caller meant to grant* —
//! and nothing here can, because the analysis carries no sizes. The lattice has
//! three tags and no numeric component; it cannot represent "how far from `p`".
//!
//! So all of these are admitted, by design:
//!
//! ```text
//! store at p + 1048576          ; a constant displacement, unbounded
//! store at p + q                ; two parameters summed - no constant at all
//! store at p + p    (= 2p)      ; nowhere near p for any nonzero p
//! ptr = p; loop { store ptr; ptr += 4 }   ; walks off the end of any buffer
//! ```
//!
//! `Param + Param` is a deliberate, test-pinned admission (`a6`, `a13` in
//! `provenance/tests.rs`): the caller supplied both operands, so under the derivation
//! property their sum is the caller's business. That is also why bounding the
//! *constant* displacement would buy nothing — the cheapest way to address
//! arbitrarily far from `p` uses no constant.
//!
//! The practical consequence is worth stating plainly, because it is easy to
//! read the contract above as stronger than it is: **an admitted external can
//! address anywhere in the shared linear memory.** What limits the damage today
//! is not this analysis but the main module's fixed single page — an
//! out-of-region address is usually out of bounds and traps. That is an
//! *accidental backstop*, not a guarantee, and it weakens as soon as the memory
//! is larger than what the program actually uses.
//!
//! Closing the gap needs a numeric/interval domain over addresses (with
//! occurrence multiplicity, so a repeated parameter cannot fold away) plus
//! declared pointee sizes for `external fn` parameters, which no channel
//! currently carries into this crate. Tracked in issue #420.
//!
//! ## The lattice
//!
//! Every operand-stack slot and every local carries one of three provenance tags:
//!
//! - [`Prov::Param`] — the value provably flows from one or more function
//!   parameters (the carried [`ParamMask`] records *which* of this function's
//!   parameters) through operations that cannot erase the caller's pointer
//!   (`local.get` of a `Param` slot, `add` of a `Param` with a `Param` or a
//!   proven `Const`, `sub` of a `Param` minuend by a non-`Param` subtrahend).
//! - [`Prov::Const`] — the value provably is a compile-time constant
//!   (caller-independent), produced by a `*.const` literal or by `add`/`sub` of
//!   two `Const`s. This tag exists solely so a `Param + Const` (a struct-field or
//!   array-element offset) can stay `Param`, while a `Param + NotParam` cannot —
//!   a `NotParam` addend is only *not provably param-derived*, so it may secretly
//!   hold a negated parameter (`C - p`) that cancels the `Param` operand back to a
//!   caller-independent constant (`(C - p) + p == C`). A constant used *directly*
//!   as a memory address is still rejected (it is not `Param`).
//! - [`Prov::NotParam`] — every other producer: a global, a call result, the
//!   table space, *any* multiplicative/bitwise/shift op (each can cancel the
//!   caller contribution: `param*0`, `param&0`, `param^param`), every unary op,
//!   any binary op whose operands are not both proven constant, or any source the
//!   analysis cannot prove parameter-derived. The fail-closed default for an
//!   uninitialized local, a stack underflow, and any unmodeled situation.
//!
//! The lattice join is the must-join: a value stays `Param` only when it is
//! `Param` on *all* incoming paths (the carried mask is then the **union** of
//! the per-path masks — on every path it derives from *some* parameter, so on the
//! merged path it derives from one of the union), and it widens to `NotParam` the
//! moment any incoming path is `Const` or `NotParam`. The lattice has no value
//! identity, so it can never prove two `Param` operands unequal — which is why
//! every operator that can cancel two equal `Param` inputs to a constant (`sub`,
//! `xor`, `and`, `mul`, …) treats its result as `NotParam`.
//!
//! ### Why the mask join is a union, and verification is `⊆`
//!
//! A `select(p0, p1)` or a two-armed `if` yields `p0` on one path and `p1` on the
//! other; at runtime the address is *either* parameter. The merged value derives
//! from `{p0} ∪ {p1}`, and it is a safe address only when **every** parameter it
//! might resolve to is caller-supplied — so verification requires the access's
//! mask to be a **subset** of the trusted-parameter set, not merely to intersect
//! it. (An `add(p0, p1)` would be safe with only one operand trusted, since
//! `caller_base + anything` stays caller-relative; using the stricter `⊆` rule
//! there is a sound over-approximation that keeps a single, uniform check.)
//!
//! ### Why `add` is not symmetric in `Param`
//!
//! `add` propagates `Param` only when the *other* operand is a `Param` or a
//! proven `Const`, never when it is a general `NotParam`. Tagging `Param + X`
//! as `Param` whenever either operand is `Param` is **unsound**: `NotParam` means
//! "not provably parameter-derived", *not* "constant". A `NotParam` operand may
//! hold `C - p` (the round-2 `sub` rule correctly demotes `const - param` to
//! `NotParam`), and `(C - p) + p == C` is a fixed, caller-independent absolute
//! address. Restricting the non-`Param` addend to a proven `Const` closes this:
//! `caller_base + fixed_offset` provably still varies with the caller's pointer.
//!
//! ## Control flow
//!
//! The analysis is a structured forward abstract interpretation over the WASM
//! structured-control tree (`block`/`loop`/`if`/`else`/`end` and the four
//! Inference non-det blocks), with [`State::join`] of the per-local state at
//! every merge point (`end`, `else`, branch target) and a loop fixpoint over
//! back-edges. A local is `Param` at a use only if `Param` on *every* reaching
//! path, so a `Param` tag written on one branch or one loop iteration cannot
//! survive a merge with a path that leaves it `NotParam`.
//!
//! ## Interprocedural policy (the sound call-graph fixpoint)
//!
//! Each function is summarised *once* against a fixed seed: parameter `i` seeds
//! `Param({i})`. From that single pass the analysis records, per function, the
//! address mask of every memory access and, per call site, the argument mask of
//! every argument (each in the calling function's own parameter terms). A
//! greatest-fixpoint pass over the call graph then computes, for every function
//! `g`, the set `trusted[g]` of `g`'s parameters that are provably caller-derived:
//!
//! - `trusted[root]` is **all** of the root's parameters (the caller owns them).
//! - a parameter `j` of a non-root `g` is trusted iff at *every* recorded call
//!   site `f → g` the argument in position `j` is itself param-derived from
//!   `trusted[f]` — i.e. its mask is non-empty and a subset of `trusted[f]`.
//!
//! Starting from "all parameters trusted" and iteratively removing any parameter
//! contradicted at a call site converges (a finite lattice, monotone descent),
//! handling self- and mutual recursion. A function reachable only through a table
//! (no direct call site) keeps its default-untrusted parameters, so a dereference
//! of an unjustified parameter is rejected. Call/`call_indirect` results are
//! always `NotParam`. Finally every memory access is verified against its
//! function's `trusted` set (`mask ⊆ trusted`); any access that fails rejects the
//! whole closure.

use inf_wasmparser::{BinaryReader, BlockType, FunctionBody, Operator};

use crate::parse::{FuncSig, ParsedModule};
use crate::LinkError;

/// A set of a single function's parameter indices, as a 64-bit bitset.
///
/// WebAssembly permits more parameters than 64, but a function with that many is
/// neither produced by Inference codegen nor a realistic shared-memory helper;
/// any parameter whose index is `>= 64` cannot be represented, so it is treated
/// as **never trusted** — its bit is simply absent, a value deriving solely from
/// it stays `NotParam`, and a dereference through it is rejected. This is a sound
/// over-rejection at the high-arity tail, never an unsound admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ParamMask(u64);

impl ParamMask {
    /// The empty mask: derives from no parameter.
    const EMPTY: ParamMask = ParamMask(0);

    /// The mask of the single parameter `index`, or the empty mask when `index`
    /// exceeds the representable range (so a high-arity parameter is never
    /// trusted).
    fn single(index: usize) -> ParamMask {
        if index < 64 {
            ParamMask(1 << index)
        } else {
            ParamMask::EMPTY
        }
    }

    /// The mask of the parameters `0..count`, saturating at the 64-bit range.
    fn first_n(count: usize) -> ParamMask {
        if count >= 64 {
            ParamMask(u64::MAX)
        } else if count == 0 {
            ParamMask::EMPTY
        } else {
            ParamMask((1u64 << count) - 1)
        }
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn union(self, other: ParamMask) -> ParamMask {
        ParamMask(self.0 | other.0)
    }

    /// Whether every parameter in `self` is also in `other` (`self ⊆ other`).
    fn is_subset_of(self, other: ParamMask) -> bool {
        self.0 & !other.0 == 0
    }

    fn without(self, index: usize) -> ParamMask {
        if index < 64 {
            ParamMask(self.0 & !(1 << index))
        } else {
            self
        }
    }
}

/// Upper bound on declared locals, mirroring `inf_wasmparser`'s
/// `MAX_WASM_FUNCTION_LOCALS` (the validator's own cap, which the driver's
/// pre-link validation already enforces for the CLI path). Re-stated here as a
/// private constant because that limit is not re-exported from the parser, and
/// the locals cap below combines it with the body length so the public library
/// API is self-defending even without the driver's validation gate. Each
/// declared local costs at least one byte in the locals encoding, so a body of
/// `B` bytes can never legitimately declare more than `B` locals; a count
/// exceeding `min(this, B)` is a malformed/adversarial group, rejected as a
/// clean [`LinkError::Parse`] rather than a multi-gigabyte allocation.
const MAX_WASM_FUNCTION_LOCALS: usize = 50_000;

/// Maximum structured-block nesting the analysis recurses into before failing
/// closed. The function-size cap bounds total operators, but a deeply-nested
/// body recurses one stack frame per level (`interpret` → `run_block` →
/// `interpret`), so past this depth the body is conservatively rejected (Tier C)
/// rather than risk an abort. The bound is kept well under what the smallest
/// stack the analysis runs on (a 2 MiB test thread) can hold, so the guard fires
/// long before a real overflow.
const MAX_ANALYSIS_DEPTH: usize = 256;

/// The provenance lattice for a value (an operand-stack slot or a local).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prov {
    /// The value provably derives from one or more of *this function's*
    /// parameters (recorded in the [`ParamMask`]) and cannot have been cancelled
    /// to a caller-independent constant. The mask is always non-empty.
    Param(ParamMask),
    /// The value provably is a compile-time constant (a `*.const` literal, or
    /// `add`/`sub` of two `Const`s). Caller-independent: never a valid memory
    /// address on its own, but a valid *offset* to add to a `Param` base.
    Const,
    /// The value derives from a global, call result, a parameter-cancelling
    /// operator, a non-constant binary op, or any source the analysis cannot
    /// prove parameter-derived or constant. The fail-closed default.
    NotParam,
}

impl Prov {
    /// The must-join: a value stays `Param` only when *both* operands are
    /// `Param`, with the **union** of their masks (on every path it derives from
    /// some parameter, so on the merged path it derives from one of the union).
    /// Two `Const`s stay `Const`; anything else widens to `NotParam`. Used to
    /// merge a local (or a stack slot) across control-flow paths.
    fn join(self, other: Prov) -> Prov {
        match (self, other) {
            (Prov::Param(a), Prov::Param(b)) => Prov::Param(a.union(b)),
            (Prov::Const, Prov::Const) => Prov::Const,
            _ => Prov::NotParam,
        }
    }

    /// The mask of parameters this value derives from, empty for a non-`Param`.
    fn mask(self) -> ParamMask {
        match self {
            Prov::Param(m) => m,
            _ => ParamMask::EMPTY,
        }
    }
}

/// The abstract state at a program point: the provenance of every local and of
/// every operand-stack slot in the current block.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    /// Per-local provenance, length = the (capped) local count.
    locals: Vec<Prov>,
    /// The abstract operand stack within the current structured block.
    stack: Vec<Prov>,
}

impl State {
    /// Elementwise must-join of two states. The `locals` vectors always share
    /// one length. When the two operand stacks have equal height (the case at
    /// every real merge point a valid body produces) they join elementwise;
    /// when they differ the merged stack is widened to all-`NotParam` of the
    /// taller height — failing closed, never accepting a stale `Param`.
    fn join(&self, other: &State) -> State {
        let locals = self
            .locals
            .iter()
            .zip(&other.locals)
            .map(|(a, b)| a.join(*b))
            .collect();

        let stack = if self.stack.len() == other.stack.len() {
            self.stack
                .iter()
                .zip(&other.stack)
                .map(|(a, b)| a.join(*b))
                .collect()
        } else {
            vec![Prov::NotParam; self.stack.len().max(other.stack.len())]
        };

        State { locals, stack }
    }
}

/// A direct `call` site recorded during a function's summary pass: the callee's
/// global function index and the provenance mask of each argument, expressed in
/// the *calling* function's own parameter terms.
#[derive(Debug, Clone)]
struct CallSite {
    callee: u32,
    arg_masks: Vec<ParamMask>,
}

/// The flow-sensitive summary of one function, computed *once* with each
/// parameter `i` seeded `Param({i})`. The interprocedural fixpoint reads these
/// summaries; nothing in them depends on which parameters are trusted, so the
/// per-function abstract interpretation never re-runs.
#[derive(Debug, Default, Clone)]
struct FunctionSummary {
    /// The number of leading locals that are parameters.
    param_count: usize,
    /// One entry per memory access: the provenance mask of its required address
    /// operand(s) (`memory.copy` records one entry each for its dest and src so
    /// the single subset check covers both). For a bulk-memory op the **size /
    /// extent** operand is recorded as its own entry too: the op touches the
    /// whole region `[address, address + size)`, so a caller-bounded start is not
    /// enough — the extent must be caller-derived as well, or a constant size
    /// could clobber an unbounded region above a caller pointer. An empty mask is
    /// an address or extent the pass could not prove parameter-derived — it can
    /// never satisfy the subset check, so it rejects unconditionally.
    accesses: Vec<ParamMask>,
    /// One entry per direct `call` site, in body order.
    calls: Vec<CallSite>,
}

/// Verifies that every memory access across the whole closure addresses memory
/// through a value derived from the closure **root's** parameters, propagated
/// across internal `call`s that pass param-derived arguments. Returns `Ok(())`
/// when the closure is provably parameter-addressing (sound for Tier B), or
/// [`LinkError::RequiresRelocatableBuild`] naming `field` when any function
/// performs a memory access whose address cannot be proven caller-supplied.
///
/// `func_indices` are the global function indices of the closure (as produced by
/// [`crate::closure::compute`], ascending); `root` is the satisfied export, whose
/// parameters are the trusted caller pointers. Each function is summarised once,
/// then a greatest-fixpoint over the call graph computes which of every
/// function's parameters are trusted, and finally every access is checked against
/// its function's trusted set.
pub(crate) fn verify_param_addressing(
    module: &ParsedModule,
    func_indices: &[u32],
    root: u32,
    field: &str,
) -> Result<(), LinkError> {
    let base = module.local_func_base();

    // 1. Summarise every closure function once (param i seeded Param({i})).
    //    `summaries` is keyed by global function index for O(1) fixpoint lookup.
    let mut summaries: std::collections::BTreeMap<u32, FunctionSummary> =
        std::collections::BTreeMap::new();
    for &func_idx in func_indices {
        let param_count = module
            .func_sig(func_idx)
            .map(|sig| sig.params.len())
            .ok_or_else(|| {
                LinkError::Parse(format!(
                    "closure function {func_idx} has no function type for provenance analysis"
                ))
            })?;
        let local = module
            .local_funcs
            .get((func_idx - base) as usize)
            .ok_or_else(|| {
                LinkError::Parse(format!(
                    "closure function {func_idx} is out of range for provenance analysis"
                ))
            })?;
        summaries.insert(func_idx, summarize_function(module, &local.body, param_count)?);
    }

    // 2. Greatest fixpoint: trusted[g] starts as all of g's params, root keeps
    //    them all, and any param contradicted at a reachable call site is removed
    //    until the assignment stabilises.
    let trusted = compute_trusted_params(&summaries, root);

    // 3. Verify every access: its address mask must be a non-empty subset of its
    //    function's trusted set.
    for (&func_idx, summary) in &summaries {
        let trust = trusted.get(&func_idx).copied().unwrap_or(ParamMask::EMPTY);
        for &access in &summary.accesses {
            if access.is_empty() || !access.is_subset_of(trust) {
                return Err(reject(
                    field,
                    "accesses memory at an address not derived from the exported function's \
                     parameters (a constant, a module-internal address, or an argument a \
                     caller never supplied); a relocatable build is required to place its \
                     data safely",
                ));
            }
        }
    }

    Ok(())
}

/// Computes, for every summarised function, the subset of its parameters that are
/// provably caller-derived (the *greatest* fixpoint).
///
/// The root has an implicit **external** call site — the host that calls the
/// exported function and supplies its pointer arguments — that justifies all of
/// the root's parameters. Every other function is justified only by the *internal*
/// `call` sites inside the closure. A parameter of a function `g` is trusted only
/// when it is justified at the external site (root only) *and* at every internal
/// call site `f → g`: the argument in that position must be non-empty and a subset
/// of `f`'s own trusted set. Crucially the root is **not** exempt from its own
/// internal call sites: a self- or mutually-recursive call that passes a constant
/// for a root parameter demotes it, because the recursive invocation re-enters the
/// root with an argument the host never supplied.
///
/// Starting from "all parameters trusted" and removing any parameter a reachable
/// call site contradicts is monotone (a parameter never re-enters once removed)
/// over a finite lattice, so the iteration converges — handling self- and mutual
/// recursion. A non-root function with no internal call site (reachable only
/// through a table, or unreachable directly) is left with the empty trusted set,
/// so a dereference of its parameter is rejected.
fn compute_trusted_params(
    summaries: &std::collections::BTreeMap<u32, FunctionSummary>,
    root: u32,
) -> std::collections::BTreeMap<u32, ParamMask> {
    // Optimistic seed: every function starts all-trusted and the fixpoint pares
    // each one down against the current assignment until it stabilises.
    let mut trusted: std::collections::BTreeMap<u32, ParamMask> = summaries
        .iter()
        .map(|(&idx, summary)| (idx, ParamMask::first_n(summary.param_count)))
        .collect();

    loop {
        let mut changed = false;
        let mut next = trusted.clone();
        for (&callee, summary) in summaries {
            // The root's external caller justifies all params and always counts as
            // a caller; a non-root is justified only by internal call sites.
            let mut justified = ParamMask::first_n(summary.param_count);
            let mut has_caller = callee == root;

            for (&caller, caller_summary) in summaries {
                let caller_trust = trusted.get(&caller).copied().unwrap_or(ParamMask::EMPTY);
                for call in &caller_summary.calls {
                    if call.callee != callee {
                        continue;
                    }
                    has_caller = true;
                    for j in 0..summary.param_count {
                        let arg = call.arg_masks.get(j).copied().unwrap_or(ParamMask::EMPTY);
                        let arg_trusted = !arg.is_empty() && arg.is_subset_of(caller_trust);
                        if !arg_trusted {
                            justified = justified.without(j);
                        }
                    }
                }
            }

            let result = if has_caller {
                justified
            } else {
                ParamMask::EMPTY
            };
            if next.get(&callee).copied() != Some(result) {
                next.insert(callee, result);
                changed = true;
            }
        }
        trusted = next;
        if !changed {
            break;
        }
    }

    trusted
}

/// Builds the Tier-C rejection error naming `field` with a single `reason`.
fn reject(field: &str, reason: &str) -> LinkError {
    LinkError::RequiresRelocatableBuild {
        field: field.to_string(),
        reasons: vec![reason.to_string()],
    }
}

/// Abstractly interprets one function body once, seeding each parameter `i` with
/// `Param({i})`, and returns its [`FunctionSummary`]: the address mask of every
/// memory access and the argument masks of every direct call site. A hard parse
/// failure or adversarial deep nesting surfaces as an `Err`; an address the pass
/// cannot prove parameter-derived is recorded as an empty access mask (which the
/// verifier then rejects), never as a silent success.
fn summarize_function(
    module: &ParsedModule,
    body: &[u8],
    param_count: usize,
) -> Result<FunctionSummary, LinkError> {
    let func_body = FunctionBody::new(BinaryReader::new(body, 0));

    let local_count = count_and_cap_locals(&func_body, param_count, body.len())?;
    let mut locals = vec![Prov::NotParam; local_count];
    for (i, slot) in locals.iter_mut().take(param_count).enumerate() {
        *slot = Prov::Param(ParamMask::single(i));
    }

    let ops = collect_operators(&func_body)?;

    // A function body's operator stream ends with the function-terminating `End`.
    // That `End` closes the implicit function frame, not a structured block, so
    // the analyzed region excludes it; analyzing it as a block terminator would
    // spuriously reject every body.
    let body_end = match ops.last() {
        Some(Operator::End) => ops.len() - 1,
        // A body without a trailing `End` is malformed; the parser would have
        // already rejected it, but fail closed rather than under-run the slice.
        _ => ops.len(),
    };

    let mut summary = FunctionSummary {
        param_count,
        ..FunctionSummary::default()
    };
    let mut interp = Interp {
        module,
        ops: &ops,
        summary: &mut summary,
    };
    let entry = State {
        locals,
        stack: Vec::new(),
    };
    // The whole function body is one region. Deep nesting or a bracket mismatch
    // makes `interpret` return `None`; that is a hard fail-closed signal (the body
    // is recorded as having an unprovable access so the closure rejects). Branches
    // that exit the function body do not feed any access, so the region result is
    // not inspected further.
    if interp.interpret(0, body_end, entry, 0)?.is_none() {
        // Record an unprovable access so the verifier rejects this function even
        // if it had no explicit memory op (a deep-nesting / structural reject).
        summary.accesses.push(ParamMask::EMPTY);
    }
    Ok(summary)
}

/// Single-function convenience used by the unit tests: runs [`summarize_function`]
/// over one body, treating that function as its own closure root (all parameters
/// trusted), and reports whether every memory access is provably parameter-derived.
#[cfg(test)]
fn function_is_param_addressing(
    module: &ParsedModule,
    body: &[u8],
    param_count: usize,
) -> Result<bool, LinkError> {
    let summary = summarize_function(module, body, param_count)?;
    let trusted = ParamMask::first_n(param_count);
    Ok(summary
        .accesses
        .iter()
        .all(|access| !access.is_empty() && access.is_subset_of(trusted)))
}

/// Collects the body's operator stream into an owned vector. The body length is
/// already bounded by the parser's function-size cap, so this is a bounded
/// allocation; an owned stream lets the structured walk re-run loop body regions
/// to a fixpoint without re-decoding.
fn collect_operators<'a>(body: &FunctionBody<'a>) -> Result<Vec<Operator<'a>>, LinkError> {
    body.get_operators_reader()
        .map_err(|e| LinkError::Parse(e.to_string()))?
        .into_iter()
        .map(|op| op.map_err(|e| LinkError::Parse(e.to_string())))
        .collect()
}

/// Counts the total local slots (parameters + declared locals) and rejects an
/// over-declared count *before* any per-local allocation. The declared-locals
/// vector lists `(count, type)` groups; parameters are not in it, so they are
/// added explicitly. A single malformed group can claim `u32::MAX` locals, so
/// the running sum is capped against both the WASM cap and the body length
/// (locals cost ≥ 1 byte each) and rejected early as a clean [`LinkError`].
fn count_and_cap_locals(
    body: &FunctionBody,
    param_count: usize,
    body_len: usize,
) -> Result<usize, LinkError> {
    let cap = MAX_WASM_FUNCTION_LOCALS.min(body_len);

    let mut reader = body
        .get_locals_reader()
        .map_err(|e| LinkError::Parse(e.to_string()))?;
    let groups = reader.get_count();
    let mut declared: usize = 0;
    for _ in 0..groups {
        let (n, _ty) = reader.read().map_err(|e| LinkError::Parse(e.to_string()))?;
        declared = declared.saturating_add(n as usize);
        if declared > cap {
            return Err(too_many_locals());
        }
    }
    let total = param_count.saturating_add(declared);
    if total > cap.saturating_add(param_count) {
        return Err(too_many_locals());
    }
    Ok(total)
}

fn too_many_locals() -> LinkError {
    LinkError::Parse("function declares too many locals for provenance analysis".to_string())
}

/// The structured abstract interpreter over one function's operator stream.
struct Interp<'a, 'b> {
    /// The source module, for resolving callee signatures (to pop/push the right
    /// number of `NotParam` call results).
    module: &'b ParsedModule,
    /// The full operator stream of the function under analysis.
    ops: &'b [Operator<'a>],
    /// The summary being built: every memory access records its address mask and
    /// every direct `call` records its argument masks here. Loop bodies are
    /// re-interpreted to a fixpoint, so an access inside a loop can be recorded
    /// more than once; recording the *same or weaker* mask on a re-run is sound
    /// (the verifier rejects on any empty/non-subset access), so the duplicates
    /// are harmless.
    summary: &'b mut FunctionSummary,
}

/// The control-flow effect of interpreting a structured region: where control
/// arrives at the region's end, and how branches targeting outer blocks
/// contribute their state.
struct RegionResult {
    /// The state at the region's normal fall-through end, if it is reachable.
    /// `None` when every path out of the region branched away or terminated.
    fallthrough: Option<State>,
    /// Per-outer-frame branch accumulators: `branch_acc[d]` joins the state of
    /// every branch that targeted the frame `d` levels *outside* this region
    /// (relative depth `d + region_depth`). Index 0 is the immediately enclosing
    /// frame. Threaded outward so an enclosing `end` can merge them.
    branch_acc: Vec<Option<State>>,
}

impl<'a, 'b> Interp<'a, 'b> {
    /// Interprets `[start, end)` from `entry`. Returns `Ok(None)` the moment a
    /// memory access addresses a `NotParam` value (the whole analysis must
    /// reject), or `Ok(Some(region))` describing where control leaves the region.
    ///
    /// The region is assumed to be one structured block's body: every `block`/
    /// `loop`/`if`/non-det block opened inside it is matched by an `end` inside
    /// it, and the region's own terminating `end`/`else` (the enclosing block's)
    /// lies at `end`.
    fn interpret(
        &mut self,
        start: usize,
        end: usize,
        entry: State,
        depth: usize,
    ) -> Result<Option<RegionResult>, LinkError> {
        if depth > MAX_ANALYSIS_DEPTH {
            // Fail closed on adversarial deep nesting rather than overflow the
            // analysis stack: report an unsafe access so the closure is rejected.
            return Ok(None);
        }

        let mut state = entry;
        let mut reachable = true;
        // branch_acc[d]: branches targeting the frame d levels outside this
        // region (the enclosing block is d = 0). Grown on demand.
        let mut branch_acc: Vec<Option<State>> = Vec::new();

        let mut i = start;
        while i < end {
            let op = &self.ops[i];

            match op {
                Operator::Block { blockty }
                | Operator::Forall { blockty }
                | Operator::Exists { blockty }
                | Operator::Assume { blockty }
                | Operator::Unique { blockty } => {
                    let body_end = self.match_end(i, end)?;
                    if reachable {
                        let outcome =
                            self.run_block(*blockty, i + 1, body_end, &state, depth)?;
                        let Some((exit, inner_acc)) = outcome else {
                            return Ok(None);
                        };
                        state = exit;
                        merge_outer(&mut branch_acc, inner_acc);
                    }
                    i = body_end + 1;
                    continue;
                }
                Operator::Loop { blockty } => {
                    let body_end = self.match_end(i, end)?;
                    if reachable {
                        let outcome =
                            self.run_loop(*blockty, i + 1, body_end, &state, depth)?;
                        let Some((exit, inner_acc)) = outcome else {
                            return Ok(None);
                        };
                        state = exit;
                        merge_outer(&mut branch_acc, inner_acc);
                    }
                    i = body_end + 1;
                    continue;
                }
                Operator::If { blockty } => {
                    let if_end = self.match_end(i, end)?;
                    if reachable {
                        let outcome = self.run_if(*blockty, i + 1, if_end, &state, depth)?;
                        let Some((exit, inner_acc)) = outcome else {
                            return Ok(None);
                        };
                        state = exit;
                        merge_outer(&mut branch_acc, inner_acc);
                    }
                    i = if_end + 1;
                    continue;
                }
                Operator::Else | Operator::End => {
                    // The region terminator of the enclosing block. The caller
                    // (`run_block`/`run_if`/`run_loop`) drove `end` to exactly
                    // this region's bound, so an `end`/`else` at `end` is consumed
                    // by the caller, not here. Reaching one before `end` means the
                    // bracket matcher disagreed with the stream; fail closed.
                    return Ok(None);
                }
                _ => {
                    if reachable {
                        // Borrow the three disjoint fields explicitly so the
                        // straight-line step can record into `summary` while `op`
                        // and `module`/`ops` stay borrowed immutably.
                        match Self::step_straight_line(
                            self.module,
                            self.summary,
                            op,
                            &mut state,
                            &mut branch_acc,
                        )? {
                            StepOutcome::Continue => {}
                            StepOutcome::Unreachable => reachable = false,
                        }
                    }
                    i += 1;
                }
            }
        }

        let fallthrough = if reachable { Some(state) } else { None };
        Ok(Some(RegionResult {
            fallthrough,
            branch_acc,
        }))
    }

    /// Interprets a `block`/non-det block body `[body_start, body_end)` opened in
    /// state `outer`, returning the post-block state and the branch accumulators
    /// for frames *outside* the block, or `None` on an unsafe access.
    ///
    /// A forward branch targeting this block (relative depth 0 inside it) merges
    /// with the block's normal fall-through at `end`. The block's params are the
    /// top `param_arity` operand slots of `outer`; its results are `result_arity`
    /// slots left on `outer`'s stack below the params.
    #[allow(clippy::type_complexity)]
    fn run_block(
        &mut self,
        blockty: BlockType,
        body_start: usize,
        body_end: usize,
        outer: &State,
        depth: usize,
    ) -> Result<Option<(State, Vec<Option<State>>)>, LinkError> {
        let (param_arity, result_arity) = self.block_arity(blockty);
        let entry = self.block_entry_state(outer, param_arity);

        let Some(region) = self.interpret(body_start, body_end, entry, depth + 1)? else {
            return Ok(None);
        };

        // The block's exit = join of the normal fall-through end-state and every
        // forward branch that targeted this block (its own branch_acc[0]).
        let mut self_acc = region.branch_acc;
        let target0 = if self_acc.is_empty() {
            None
        } else {
            self_acc.remove(0)
        };
        let exit_inner = join_opt(region.fallthrough, target0);

        let exit = match exit_inner {
            Some(inner) => self.block_exit_state(outer, &inner, param_arity, result_arity),
            // No path reaches the block's end (every path branched out or
            // terminated). Control continues after the block with whatever the
            // branches that skipped past it carry; model the post-block state as
            // the outer state minus params plus NotParam results (fail-closed).
            None => self.unreachable_exit_state(outer, param_arity, result_arity),
        };

        // self_acc now holds branches targeting frames *outside* this block, with
        // depth shifted down by one (the block frame was index 0).
        Ok(Some((exit, self_acc)))
    }

    /// Interprets a `loop` body to a fixpoint over its back-edges. A branch
    /// targeting the loop (relative depth 0 inside it) re-enters the loop header,
    /// so the header entry state is the join of the loop's outer entry and every
    /// back-edge, iterated until it stabilizes (monotone descent toward
    /// `NotParam`, bounded by the slot count).
    #[allow(clippy::type_complexity)]
    fn run_loop(
        &mut self,
        blockty: BlockType,
        body_start: usize,
        body_end: usize,
        outer: &State,
        depth: usize,
    ) -> Result<Option<(State, Vec<Option<State>>)>, LinkError> {
        let (param_arity, result_arity) = self.block_arity(blockty);
        let mut header_in = self.block_entry_state(outer, param_arity);

        // Fixpoint: re-run the body until the header entry state stops changing.
        // Each iteration can only flip slots Param -> NotParam, so the loop
        // terminates in at most (slot count + 1) rounds.
        let max_rounds = header_in.locals.len() + header_in.stack.len() + 2;
        let mut final_region;
        let mut rounds = 0;
        loop {
            let Some(region) = self.interpret(body_start, body_end, header_in.clone(), depth + 1)?
            else {
                return Ok(None);
            };

            // Back-edges target the loop header (its own branch_acc[0]).
            let back = region.branch_acc.first().cloned().flatten();
            let next_header = match back {
                Some(b) => header_in.join(&b),
                None => header_in.clone(),
            };

            final_region = region;
            rounds += 1;
            if next_header == header_in || rounds >= max_rounds {
                break;
            }
            header_in = next_header;
        }

        // The loop's normal exit is its body's fall-through end (a loop's label
        // is a back-edge, so forward exits leave via the fall-through `end`).
        let exit = match final_region.fallthrough.take() {
            Some(inner) => self.block_exit_state(outer, &inner, param_arity, result_arity),
            None => self.unreachable_exit_state(outer, param_arity, result_arity),
        };

        // Drop the loop's own frame (index 0); the rest are outer branches.
        let mut self_acc = final_region.branch_acc;
        if !self_acc.is_empty() {
            self_acc.remove(0);
        }
        Ok(Some((exit, self_acc)))
    }

    /// Interprets an `if` (and optional `else`) over `[body_start, if_end)`,
    /// where `if_end` is the `if`'s matching `end`. The condition has already
    /// been consumed (it is popped from `outer`'s stack as the `if`'s param
    /// model). Both arms are interpreted from the same entry state; the `if`'s
    /// exit is the join of the true-arm end, the false/implicit-else-arm end, and
    /// any forward branch targeting the `if` block.
    #[allow(clippy::type_complexity)]
    fn run_if(
        &mut self,
        blockty: BlockType,
        body_start: usize,
        if_end: usize,
        outer: &State,
        depth: usize,
    ) -> Result<Option<(State, Vec<Option<State>>)>, LinkError> {
        let (param_arity, result_arity) = self.block_arity(blockty);

        // The `if` consumes the condition (1 slot) plus `param_arity` block
        // params from `outer`'s stack. Pop the condition first, then build the
        // block entry state from the remaining stack.
        let mut after_cond = outer.clone();
        after_cond.stack.pop(); // the i32 condition
        let entry = self.block_entry_state(&after_cond, param_arity);

        // Split the arms at the `else` matching this `if` (if any).
        let else_idx = self.find_else(body_start, if_end);
        let (true_range, false_range) = match else_idx {
            Some(e) => ((body_start, e), Some((e + 1, if_end))),
            None => ((body_start, if_end), None),
        };

        let Some(true_region) = self.interpret(true_range.0, true_range.1, entry.clone(), depth + 1)?
        else {
            return Ok(None);
        };

        let false_region = match false_range {
            Some((fs, fe)) => {
                let Some(r) = self.interpret(fs, fe, entry.clone(), depth + 1)? else {
                    return Ok(None);
                };
                r
            }
            None => {
                // No `else`: the implicit false arm is the entry state unchanged
                // (the block body did not run), contributing `entry` as its
                // fall-through with no branches.
                RegionResult {
                    fallthrough: Some(entry.clone()),
                    branch_acc: Vec::new(),
                }
            }
        };

        // Merge both arms' branch accumulators; the `if` block is each arm's
        // frame 0, so its own branches fold into the exit join.
        let mut merged_acc = true_region.branch_acc;
        merge_outer(&mut merged_acc, false_region.branch_acc);
        let target0 = if merged_acc.is_empty() {
            None
        } else {
            merged_acc.remove(0)
        };

        // The `if` exit = join of the true-arm fall-through, the false-arm
        // fall-through, and any branch targeting the `if`.
        let arms = join_opt(true_region.fallthrough, false_region.fallthrough);
        let exit_inner = join_opt(arms, target0);

        let exit = match exit_inner {
            Some(inner) => self.block_exit_state(&after_cond, &inner, param_arity, result_arity),
            None => self.unreachable_exit_state(&after_cond, param_arity, result_arity),
        };

        Ok(Some((exit, merged_acc)))
    }

    /// Applies one straight-line (non-structured-control) operator to `state`,
    /// reporting whether control continues or becomes unreachable. A memory
    /// access records its address mask into `summary` (an empty mask for an
    /// unprovable address, which the verifier later rejects); a direct `call`
    /// records its argument masks. Branches record their state into `branch_acc`
    /// at their target's relative depth.
    ///
    /// An associated function (no `self`) so the caller can hand it the three
    /// disjoint borrows it needs — `module` immutably, `summary` mutably, and the
    /// borrowed `op` — without re-borrowing the whole interpreter.
    fn step_straight_line(
        module: &ParsedModule,
        summary: &mut FunctionSummary,
        op: &Operator,
        state: &mut State,
        branch_acc: &mut Vec<Option<State>>,
    ) -> Result<StepOutcome, LinkError> {
        use Operator::*;

        match op {
            // -- Locals --
            LocalGet { local_index } => {
                state.stack.push(local_prov(&state.locals, *local_index));
            }
            LocalSet { local_index } => {
                let v = pop(state);
                set_local(&mut state.locals, *local_index, v);
            }
            LocalTee { local_index } => {
                let v = state.stack.last().copied().unwrap_or(Prov::NotParam);
                set_local(&mut state.locals, *local_index, v);
            }

            // -- Constant literals: caller-independent constants (a valid offset
            //    to add to a Param base, never a valid address on their own). --
            I32Const { .. } | I64Const { .. } | F32Const { .. } | F64Const { .. } => {
                state.stack.push(Prov::Const);
            }

            // -- Sources that are neither parameter-derived nor proven constant:
            //    a global is runtime-mutable; an uzumaki is non-deterministic. --
            I32Uzumaki { .. } | I64Uzumaki { .. } | GlobalGet { .. } => {
                state.stack.push(Prov::NotParam);
            }

            // -- Loads: pop the address, record its mask, push NotParam contents --
            I32Load { .. } | I64Load { .. } | F32Load { .. } | F64Load { .. }
            | I32Load8S { .. } | I32Load8U { .. } | I32Load16S { .. }
            | I32Load16U { .. } | I64Load8S { .. } | I64Load8U { .. }
            | I64Load16S { .. } | I64Load16U { .. } | I64Load32S { .. }
            | I64Load32U { .. } => {
                let addr = pop(state);
                record_access(summary, addr.mask());
                state.stack.push(Prov::NotParam);
            }

            // -- Stores: pop value then address, record the address mask --
            I32Store { .. } | I64Store { .. } | F32Store { .. } | F64Store { .. }
            | I32Store8 { .. } | I32Store16 { .. } | I64Store8 { .. }
            | I64Store16 { .. } | I64Store32 { .. } => {
                pop(state); // the stored value
                let addr = pop(state);
                record_access(summary, addr.mask());
            }

            // -- Bulk memory: both the address AND the extent operand must be
            //    parameter-derived. A bulk-memory op touches the contiguous
            //    region `[address, address + size)`, so a caller-bounded *start*
            //    is not enough: a constant or global `size` lets the op clobber
            //    or read an unbounded region above a caller pointer (e.g.
            //    `memory.fill(base, v, 0x8000)` scorches host memory the caller
            //    never exposed). The extent therefore carries the same
            //    caller-derivation requirement as an address — a `Param` size
            //    (`fill(ptr, v, len)` with a trusted `len`) is admitted, a
            //    `Const`/`NotParam` size (empty mask) fails the subset check and
            //    rejects the whole closure. --
            MemoryFill { .. } => {
                // Stack: [dest, value, size]. The size bounds the clobbered
                // extent, so it must be caller-derived; the value is the fill
                // byte (neither an address nor an extent) and is discarded.
                let size = pop(state);
                record_access(summary, size.mask());
                pop(state); // value (the fill byte)
                let dest = pop(state);
                record_access(summary, dest.mask());
            }
            MemoryCopy { .. } => {
                // Stack: [dest, src, size]; both dest and src are addresses and
                // the size bounds the copied extent, so all three must be
                // trusted. Each is recorded as its own access; the verifier
                // rejects if any is empty or not a subset of the trusted set.
                let size = pop(state);
                record_access(summary, size.mask());
                let src = pop(state);
                let dest = pop(state);
                record_access(summary, dest.mask());
                record_access(summary, src.mask());
            }
            MemoryInit { .. } => {
                // Stack: [dest, offset, size]; dest is the address and size
                // bounds the written extent (both caller-derived). The offset is
                // a data-segment offset, not a linear-memory address, so it is
                // discarded. (memory.init also implies a data segment -> already
                // Tier C; this is defense-in-depth on the destination and extent.)
                let size = pop(state);
                record_access(summary, size.mask());
                pop(state); // offset (into the data segment, not linear memory)
                let dest = pop(state);
                record_access(summary, dest.mask());
            }

            // -- memory.size / memory.grow yield page counts, never addresses --
            MemorySize { .. } => {
                state.stack.push(Prov::NotParam);
            }
            MemoryGrow { .. } => {
                pop(state); // delta
                state.stack.push(Prov::NotParam);
            }

            // -- Parametric --
            Drop => {
                pop(state);
            }
            Select | TypedSelect { .. } => {
                // Pops condition + two values, pushes their join. Param only if
                // both value operands are Param.
                pop(state); // condition
                let a = pop(state);
                let b = pop(state);
                state.stack.push(a.join(b));
            }

            // -- Calls: record the per-argument provenance masks so the
            //    interprocedural fixpoint can decide which callee parameters are
            //    trusted, then pop the callee's params and push NotParam results
            //    (a call result is never trusted). --
            Call { function_index } => {
                let sig = module.func_sig(*function_index).cloned();
                if let Some(sig) = sig.as_ref() {
                    let arg_masks = top_arg_masks(state, sig.params.len());
                    summary.calls.push(CallSite {
                        callee: *function_index,
                        arg_masks,
                    });
                }
                apply_call(sig.as_ref(), state);
            }
            ReturnCall { .. } => {
                // Tail call terminates this path locally.
                return Ok(StepOutcome::Unreachable);
            }
            CallIndirect { type_index, .. } => {
                // An indirect call dispatches through the table; its result is
                // never trusted, and no callee parameter can be justified through
                // it (the callee is not statically known). Pop the table index and
                // the callee params, push NotParam results.
                pop(state); // the table index operand
                let sig = type_sig(module, *type_index).cloned();
                apply_call(sig.as_ref(), state);
            }
            ReturnCallIndirect { .. } => {
                return Ok(StepOutcome::Unreachable);
            }

            // -- Branches: record state at the target, end reachability where the
            //    branch is unconditional. --
            Br { relative_depth } => {
                accumulate(branch_acc, *relative_depth, state);
                return Ok(StepOutcome::Unreachable);
            }
            BrIf { relative_depth } => {
                pop(state); // the i32 condition
                accumulate(branch_acc, *relative_depth, state);
                // The false edge falls through; reachability continues.
            }
            BrTable { targets } => {
                pop(state); // the i32 index
                accumulate(branch_acc, targets.default(), state);
                for target in targets.targets() {
                    let target = target.map_err(|e| LinkError::Parse(e.to_string()))?;
                    accumulate(branch_acc, target, state);
                }
                return Ok(StepOutcome::Unreachable);
            }
            Return | Unreachable => {
                return Ok(StepOutcome::Unreachable);
            }
            Nop => {}

            // -- Arithmetic: only `add` and the constrained `sub` propagate Param;
            //    every other binary and every unary op produces NotParam (each can
            //    cancel the caller contribution). --
            _ if is_add(op) => {
                let a = pop(state);
                let b = pop(state);
                state.stack.push(add_prov(a, b));
            }
            _ if is_sub(op) => {
                // WASM stack for `b - a` is [b, a] with `a` (subtrahend) on top.
                let a = pop(state); // subtrahend (top)
                let b = pop(state); // minuend
                state.stack.push(sub_prov(b, a));
            }
            _ if is_other_binary(op) => {
                pop(state);
                pop(state);
                state.stack.push(Prov::NotParam);
            }
            _ if is_unary(op) => {
                pop(state);
                state.stack.push(Prov::NotParam);
            }

            // -- Any operator whose precise stack effect the analysis does not
            //    model: widen the stack to empty so later pops read the
            //    fail-closed NotParam default. The safety allow-list has already
            //    confined the operator set, so this is unreachable for a body that
            //    passed `check_operator`; it is defense in depth. --
            _ => {
                state.stack.clear();
            }
        }

        Ok(StepOutcome::Continue)
    }

    /// The `(param_arity, result_arity)` of a block type: how many operand slots
    /// it consumes at entry and leaves at `end`. A `FuncType` index is resolved
    /// against the module's type section; an unresolvable one fails closed to
    /// `(0, 0)` (the surrounding stack is then widened by the result model).
    fn block_arity(&self, blockty: BlockType) -> (usize, usize) {
        match blockty {
            BlockType::Empty => (0, 0),
            BlockType::Type(_) => (0, 1),
            BlockType::FuncType(t) => match type_sig(self.module, t) {
                Some(sig) => (sig.params.len(), sig.results.len()),
                None => (0, 0),
            },
        }
    }

    /// The entry state of a structured block: `outer`'s locals, with the top
    /// `param_arity` operand slots carried in as the block's initial stack.
    fn block_entry_state(&self, outer: &State, param_arity: usize) -> State {
        let take = param_arity.min(outer.stack.len());
        let params = outer.stack[outer.stack.len() - take..].to_vec();
        State {
            locals: outer.locals.clone(),
            stack: params,
        }
    }

    /// The state after a structured block exits normally: `outer`'s stack with
    /// the block's params popped and `result_arity` result slots pushed (each the
    /// inner end-state's corresponding result slot, or `NotParam` if the inner
    /// stack is shorter than the declared result arity), and the block's merged
    /// locals.
    fn block_exit_state(
        &self,
        outer: &State,
        inner_end: &State,
        param_arity: usize,
        result_arity: usize,
    ) -> State {
        let mut stack = outer.stack.clone();
        for _ in 0..param_arity.min(stack.len()) {
            stack.pop();
        }
        let results = result_tail(&inner_end.stack, result_arity);
        stack.extend(results);
        State {
            locals: inner_end.locals.clone(),
            stack,
        }
    }

    /// The post-block state when no path reaches the block's end. Control resumes
    /// after the block (carried by branches that skipped it), so the stack shape
    /// must still be correct: pop the params, push `NotParam` results, and widen
    /// every local to `NotParam` (no path's locals are known). Fail closed.
    fn unreachable_exit_state(
        &self,
        outer: &State,
        param_arity: usize,
        result_arity: usize,
    ) -> State {
        let mut stack = outer.stack.clone();
        for _ in 0..param_arity.min(stack.len()) {
            stack.pop();
        }
        stack.extend(std::iter::repeat_n(Prov::NotParam, result_arity));
        State {
            locals: vec![Prov::NotParam; outer.locals.len()],
            stack,
        }
    }

    /// Finds the index of the `else` matching the `if` whose body is
    /// `[body_start, if_end)`, skipping nested structured blocks. `None` when the
    /// `if` has no `else`.
    fn find_else(&self, body_start: usize, if_end: usize) -> Option<usize> {
        let mut nesting = 0usize;
        let mut i = body_start;
        while i < if_end {
            match &self.ops[i] {
                Operator::Block { .. }
                | Operator::Loop { .. }
                | Operator::If { .. }
                | Operator::Forall { .. }
                | Operator::Exists { .. }
                | Operator::Assume { .. }
                | Operator::Unique { .. } => nesting += 1,
                Operator::End => {
                    // An `End` here closes a nested block; the `if`'s own `End` is
                    // at `if_end`, outside this range.
                    nesting = nesting.saturating_sub(1);
                }
                Operator::Else if nesting == 0 => return Some(i),
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Returns the index of the `End` (or, for an `if`, the matching `End`)
    /// closing the structured block opened at `open`, searching within
    /// `[open, limit)`. Fails closed to `limit - 1` semantics by returning a
    /// `Parse` error if the bracket is unbalanced (a valid body never is).
    fn match_end(&self, open: usize, limit: usize) -> Result<usize, LinkError> {
        let mut nesting = 0usize;
        let mut i = open;
        while i < limit {
            match &self.ops[i] {
                Operator::Block { .. }
                | Operator::Loop { .. }
                | Operator::If { .. }
                | Operator::Forall { .. }
                | Operator::Exists { .. }
                | Operator::Assume { .. }
                | Operator::Unique { .. } => nesting += 1,
                Operator::End => {
                    nesting -= 1;
                    if nesting == 0 {
                        return Ok(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        Err(LinkError::Parse(
            "unbalanced structured control flow in function body".to_string(),
        ))
    }
}

/// The outcome of stepping one straight-line operator.
enum StepOutcome {
    /// Control continues to the next operator.
    Continue,
    /// Control becomes unreachable for the rest of the block (`br`/`return`/
    /// `unreachable`/tail call).
    Unreachable,
}

/// Records one memory access's address mask into `summary`. `memarg.offset` is
/// deliberately not consulted: a `param + N` effective address still varies with
/// the caller's pointer and can never reach a caller-independent host location,
/// so the offset cannot turn a trusted base into an untrusted one. An empty mask
/// (an address not provably parameter-derived) is recorded as-is; the verifier
/// rejects it.
fn record_access(summary: &mut FunctionSummary, mask: ParamMask) {
    summary.accesses.push(mask);
}

/// The provenance masks of the top `count` operand-stack slots, deepest-first
/// (so index `j` is the `j`-th call argument). Underflow slots default to the
/// empty mask (fail closed).
fn top_arg_masks(state: &State, count: usize) -> Vec<ParamMask> {
    let depth = state.stack.len();
    (0..count)
        .map(|j| {
            // Argument j sits `count - j` slots below the top of the stack.
            depth
                .checked_sub(count - j)
                .and_then(|idx| state.stack.get(idx))
                .map(|prov| prov.mask())
                .unwrap_or(ParamMask::EMPTY)
        })
        .collect()
}

/// Pops a callee's parameters and pushes one `NotParam` per result, modeling a
/// `call`/`call_indirect` whose results are never trusted. With no resolvable
/// signature the stack is cleared (fail closed: later pops read `NotParam`).
fn apply_call(sig: Option<&FuncSig>, state: &mut State) {
    match sig {
        Some(sig) => {
            for _ in 0..sig.params.len() {
                pop(state);
            }
            for _ in 0..sig.results.len() {
                state.stack.push(Prov::NotParam);
            }
        }
        None => state.stack.clear(),
    }
}

/// The function signature a type index names, if it is a function type in the
/// module's type section.
fn type_sig(module: &ParsedModule, type_index: u32) -> Option<&FuncSig> {
    match module.types.get(type_index as usize)? {
        crate::parse::TypeEntry::Func(sig) => Some(sig),
        crate::parse::TypeEntry::Other => None,
    }
}

/// Pops the top of the operand stack, reading the fail-closed `NotParam` on
/// underflow (which a valid body never produces, but the analysis must survive).
fn pop(state: &mut State) -> Prov {
    state.stack.pop().unwrap_or(Prov::NotParam)
}

/// The provenance of a local, reading the fail-closed `NotParam` for an
/// out-of-range index (which a valid body never produces).
fn local_prov(locals: &[Prov], index: u32) -> Prov {
    locals
        .get(index as usize)
        .copied()
        .unwrap_or(Prov::NotParam)
}

/// Writes `prov` to a local, ignoring an out-of-range index.
fn set_local(locals: &mut [Prov], index: u32, prov: Prov) {
    if let Some(slot) = locals.get_mut(index as usize) {
        *slot = prov;
    }
}

/// Joins two optional states: `Some` only when at least one is `Some`, and the
/// `join` of both when both are present.
fn join_opt(a: Option<State>, b: Option<State>) -> Option<State> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.join(&b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Records a branch's `state` into `acc` at relative `depth`, joining with any
/// branch already targeting that frame. Grows `acc` to cover the depth.
fn accumulate(acc: &mut Vec<Option<State>>, depth: u32, state: &State) {
    let d = depth as usize;
    if acc.len() <= d {
        acc.resize(d + 1, None);
    }
    acc[d] = match acc[d].take() {
        Some(existing) => Some(existing.join(state)),
        None => Some(state.clone()),
    };
}

/// Merges an inner region's outer-frame branch accumulators (already shifted so
/// index 0 is the frame enclosing the inner region) into `outer`, joining
/// per-depth.
fn merge_outer(outer: &mut Vec<Option<State>>, inner: Vec<Option<State>>) {
    if outer.len() < inner.len() {
        outer.resize(inner.len(), None);
    }
    for (slot, contrib) in outer.iter_mut().zip(inner) {
        if let Some(contrib) = contrib {
            *slot = match slot.take() {
                Some(existing) => Some(existing.join(&contrib)),
                None => Some(contrib),
            };
        }
    }
}

/// The top `result_arity` slots of `stack` (the block's result values), padded
/// with `NotParam` when the stack is shorter than the declared arity (fail
/// closed).
fn result_tail(stack: &[Prov], result_arity: usize) -> Vec<Prov> {
    if stack.len() >= result_arity {
        stack[stack.len() - result_arity..].to_vec()
    } else {
        let mut v = vec![Prov::NotParam; result_arity - stack.len()];
        v.extend_from_slice(stack);
        v
    }
}

/// The provenance of `a + b`. `add` is commutative, so the rule is symmetric in
/// its operands.
///
/// - `Param + Param`: two genuine caller values; the result varies with the
///   caller's inputs (`a6`/`a13`). The mask is the **union**: the sum derives
///   from every parameter either operand does. `Param`.
/// - `Param + Const` / `Const + Param`: `caller_base + fixed_offset` provably
///   still varies with the caller's pointer (the struct-field / array-element
///   case `a2`/`a5`). The `Param` mask carries through unchanged.
/// - `Param + NotParam`: **unsound to keep `Param`.** `NotParam` means *not
///   provably parameter-derived*, not *constant*; it may hold `C - p`, and
///   `(C - p) + p == C` is a fixed, caller-independent absolute address. Demote
///   to `NotParam`.
/// - `Const + Const`: a constant. `Const`.
/// - anything else: `NotParam`.
fn add_prov(a: Prov, b: Prov) -> Prov {
    match (a, b) {
        (Prov::Param(x), Prov::Param(y)) => Prov::Param(x.union(y)),
        (Prov::Param(m), Prov::Const) | (Prov::Const, Prov::Param(m)) => Prov::Param(m),
        (Prov::Const, Prov::Const) => Prov::Const,
        _ => Prov::NotParam,
    }
}

/// The provenance of `b - a` (minuend `b`, subtrahend `a`).
///
/// - `Param - Const`: `caller_base - fixed_offset` provably still varies with the
///   caller's pointer (a struct field below the pointer, `a7`). The `Param` mask
///   carries through unchanged.
/// - `Param - NotParam`: **unsound to keep `Param`**, the exact mirror of the
///   `add` cancellation. `NotParam` means *not provably parameter-derived*, not
///   *constant*; the subtrahend may itself hold `p - C`, and `p - (p - C) == C`
///   is a fixed, caller-independent absolute address. Demote to `NotParam`.
/// - `Param - Param`: may be `b - b == 0`, a caller-independent constant
///   (`n1`/`n6`). `NotParam`.
/// - `Const - Const`: a constant. `Const`.
/// - anything else (including `Const - Param`, which negates the caller
///   contribution to `C - p` that a later `add` must not re-promote): `NotParam`.
fn sub_prov(b: Prov, a: Prov) -> Prov {
    match (b, a) {
        (Prov::Param(m), Prov::Const) => Prov::Param(m),
        (Prov::Const, Prov::Const) => Prov::Const,
        _ => Prov::NotParam,
    }
}

/// Whether `op` is an `add`.
fn is_add(op: &Operator) -> bool {
    use Operator::*;
    matches!(op, I32Add | I64Add | F32Add | F64Add)
}

/// Whether `op` is a `sub`.
fn is_sub(op: &Operator) -> bool {
    use Operator::*;
    matches!(op, I32Sub | I64Sub | F32Sub | F64Sub)
}

/// Whether `op` is a two-operand numeric instruction *other than* add/sub: a
/// multiply, divide, remainder, bitwise op, shift, rotate, float min/max/
/// copysign, or any comparison. Each can cancel the caller contribution to a
/// caller-independent value, so its result is unconditionally `NotParam`.
fn is_other_binary(op: &Operator) -> bool {
    use Operator::*;
    matches!(
        op,
        // comparisons
        I32Eq | I32Ne | I32LtS | I32LtU | I32GtS | I32GtU | I32LeS | I32LeU | I32GeS | I32GeU
            | I64Eq | I64Ne | I64LtS | I64LtU | I64GtS | I64GtU | I64LeS | I64LeU | I64GeS
            | I64GeU | F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge | F64Eq | F64Ne | F64Lt
            | F64Gt | F64Le | F64Ge
        // i32 / i64 multiplicative, bitwise, shift, rotate
            | I32Mul | I32DivS | I32DivU | I32RemS | I32RemU | I32And | I32Or | I32Xor | I32Shl
            | I32ShrS | I32ShrU | I32Rotl | I32Rotr | I64Mul | I64DivS | I64DivU | I64RemS
            | I64RemU | I64And | I64Or | I64Xor | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr
        // float multiplicative / min / max / copysign
            | F32Mul | F32Div | F32Min | F32Max | F32Copysign | F64Mul | F64Div | F64Min | F64Max
            | F64Copysign
    )
}

/// Whether `op` is a single-operand numeric instruction: a unary arithmetic, a
/// test, a conversion, a reinterpret, an extend, or a saturating truncation.
/// Every unary op produces `NotParam`: the tagless lattice cannot distinguish a
/// value-preserving width conversion from a value-destroying op like `eqz`
/// (which yields `0`/`1`), so all unary ops fail closed.
fn is_unary(op: &Operator) -> bool {
    use Operator::*;
    matches!(
        op,
        I32Eqz | I64Eqz | I32Clz | I32Ctz | I32Popcnt | I64Clz | I64Ctz | I64Popcnt | F32Abs
            | F32Neg | F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt | F64Abs | F64Neg
            | F64Ceil | F64Floor | F64Trunc | F64Nearest | F64Sqrt | I32WrapI64 | I32TruncF32S
            | I32TruncF32U | I32TruncF64S | I32TruncF64U | I64ExtendI32S | I64ExtendI32U
            | I64TruncF32S | I64TruncF32U | I64TruncF64S | I64TruncF64U | F32ConvertI32S
            | F32ConvertI32U | F32ConvertI64S | F32ConvertI64U | F32DemoteF64 | F64ConvertI32S
            | F64ConvertI32U | F64ConvertI64S | F64ConvertI64U | F64PromoteF32
            | I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64
            | I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S
            | I32TruncSatF32S | I32TruncSatF32U | I32TruncSatF64S | I32TruncSatF64U
            | I64TruncSatF32S | I64TruncSatF32U | I64TruncSatF64S | I64TruncSatF64U
    )
}

#[cfg(test)]
mod tests;

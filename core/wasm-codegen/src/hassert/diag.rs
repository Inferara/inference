//! Diagnostics for the proof-mode specification-to-`hassert` translation.
//!
//! These are their own `P0xx` registry rather than analysis `A0xx` rules for a
//! structural reason: the analysis pass is mode-blind, and every construct these
//! codes reject is legal in proof-mode WASM emission (a `loop` inside a `forall`
//! body compiles fine). They lack only an *assertion* encoding, so they are
//! proof-mode code-generation errors, surfaced in the same
//! `[file_label:]line:col: error[P00x]: message` shape the user already reads
//! from analysis.
//!
//! Every diagnostic is collected — the translator visits all specification
//! functions and records every problem — so a spec with several mistakes
//! surfaces them all at once rather than one round-trip at a time. A diagnostic
//! keeps its function from producing an obligation *and* fails code generation:
//! the caller joins the collected diagnostics into a
//! [`CodegenError::UntranslatableSpec`](crate::errors::CodegenError::UntranslatableSpec),
//! because an obligation is a required proof-mode deliverable.

use std::fmt;

use inference_ast::nodes::{Location, file_label};

/// A proof-mode specification-translation error code.
///
/// The numbering is stable and user-facing; the messages live at the call sites
/// (many carry a construct name or a reason that only the site knows).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PCode {
    /// Spec function body is `assume`-quantified. `assume` is not a
    /// quantifier — with no enclosing `forall` there is no claim to prove.
    P001,
    /// A construct with no assertion encoding: `loop`, `break`, a nested
    /// `unique` block, `**`, a string literal, an array/struct literal read in
    /// scalar term position or written at a shape outside the supported
    /// surface, or an access chain the element encoding cannot pin — one
    /// carrying more than one non-constant index, or one whose non-constant
    /// index lands on an aggregate rather than a scalar leaf.
    ///
    /// Four of these carry their own message instead of the shared template,
    /// because the template's "this has no encoding; move the logic into an
    /// executable helper" is either false or useless for them. `loop`: a
    /// loop's purpose in a specification — saying something about every
    /// element — is precisely what quantifying an index and constraining it
    /// says directly, so its message names that idiom. The out-of-surface
    /// literal: literals encode now, so the restriction is the *shape*, and
    /// the helper remedy dead-ends (a compound result is [`PCode::P005`], a
    /// compound argument [`PCode::P004`]). And each access-chain case, which
    /// states what the encoding can pin and how to get there.
    P002,
    /// Reassignment (`Stmt::Assign`) in a specification body — a permanent
    /// rule, not a pending feature. A specification names values, not storage:
    /// every name stands for one value throughout the claim, which is what
    /// lets the translation read a name as the same term wherever it appears.
    /// Supporting mutation would mean per-branch value versioning across
    /// quantifier scopes for no expressive gain over a fresh `let`.
    P003,
    /// A type with no place in a specification term. Three wordings live under
    /// this code, because three different facts land here:
    ///
    /// - the type is not representable at all — `unit`, a function type, or an
    ///   aggregate outside the representable surface (arrays of scalars at any
    ///   rank, and structs whose fields are scalars or *one-dimensional* scalar
    ///   arrays: the executable aggregate `@` surface, bounded by analysis
    ///   rules A027/A028);
    /// - the type is representable but was read *whole* where a term is
    ///   required — an aggregate call argument, most often. The surface
    ///   enumeration above would contradict this rejection, so the message
    ///   states the real fact instead: an aggregate is not a term;
    /// - the declaration is an *aggregate* parameter of an `exists`/`unique`
    ///   body, which the identical declaration in a `forall` body would
    ///   leaf-expand. The message names the quantifier, since that — not the
    ///   type — is what rules it out: the obligation denotes against the frame
    ///   an actual run reaches, where the parameter is one pointer local. A
    ///   non-scalar, non-aggregate parameter there is unrepresentable for the
    ///   first reason instead, and takes the first wording.
    P004,
    /// A call that cannot be represented as a `T_app` term.
    P005,
    /// `@` outside a `let` right-hand side or a call-argument position.
    P006,
    /// A `forall` block nested inside an `exists` context of an
    /// `exists`/`unique`-quantified body. Inside a universal body the same
    /// nesting translates — the inner block binds a universal logical variable
    /// per `@` — but a reachability body's `@`s are choice parameters its
    /// judgment quantifies operationally, so a universal binder over one has no
    /// representation.
    P007,
    /// `@` at a compound (array/struct) type — in a reachability body, where a
    /// choice arrives as one scalar parameter of the run the obligation talks
    /// about, or in any body at a shape outside the representable surface. A
    /// supported-shape compound `@` in a universal body quantifies one variable
    /// per scalar leaf instead of raising this.
    P008,
    /// A specification *method* carrying a proof obligation the translation
    /// cannot deliver — quantified, or plain but claiming a property. A method
    /// has no free-function fallback path, so it is flagged rather than
    /// silently dropped.
    P009,
    /// A specification function whose obligation is vacuous (`HA_true`).
    P010,
    /// A call from a specification body to an `exists`/`unique`-quantified
    /// spec function. Such a function is the subject of a reachability
    /// judgment about running its own body with its own choices — not a
    /// callable predicate — and its compiled form carries hidden trailing
    /// choice parameters no call site supplies.
    P011,
    /// An anonymous `@` (call-argument position) in a `unique`-bodied spec
    /// function. `unique` compares source-visible exit states, and a choice
    /// nothing names is excluded from that face, which would silently weaken
    /// the judgment; binding it first (`let c: i32 = @;`) makes it count.
    P012,
    /// An aggregate introduction (a compound `@`, a compound parameter, or an
    /// array/struct literal) whose scalar leaves would push the specification
    /// function past its cumulative quantified-leaf budget
    /// (`SPEC_FN_MAX_QUANTIFIED_LEAVES`). A quantified leaf brings a hypothesis
    /// or a binder, and what that costs depends on where it sits: one
    /// assertion-tree level as a universal slot's hypothesis, where a narrow
    /// leaf's declared value domain is grouped into that one level rather than
    /// added beside it, and one as an existential binder — two where a narrow
    /// leaf's bound rides in a conjunct inside it. A literal's leaves are
    /// constants and still nest one level apiece through a leafwise
    /// comparison. Either way the levels accumulate across every introduction
    /// in the function, so the budget is a per-function total, not a
    /// per-introduction cap.
    P013,
    /// A constant-folded array index that is out of bounds for the accessed
    /// array — `const K: i32 = 5; a[K]`, or `a[1 + 4]`, on `[i32; 3]`. States
    /// the same fact analysis rule A037 states for a *direct-literal* index;
    /// A037's pattern requires the literal directly under the access, so an
    /// index named through a constant or computed from constants reaches the
    /// translator even with analysis on, and the no-analysis codegen paths make
    /// this the only guard for any of the spellings.
    P014,
    /// A quantified introduction — a parameter, a `let … = @`, an anonymous
    /// call-argument `@`, or a leaf of an aggregate one — at an `enum` declared
    /// with no variants. The declared type admits no value, so there is nothing
    /// for the claim to range over: an obligation over it either says nothing
    /// or is unprovable for a reason that has nothing to do with the program.
    ///
    /// Analysis rule `A009` already warns about the *declaration*, but only
    /// warns, so such an enum compiles and a `@` over it really does reach this
    /// pass. Executable code generation then treats it three mutually
    /// inconsistent ways: a draw is left unconstrained (`rem_u 0` would trap, so
    /// no normalization is emitted at all), an exported function's entry tag
    /// guard traps on every call (`tag >= 0` is uniformly true), and a memory
    /// round-trip constrains nothing either. There is no consistent behaviour to
    /// mirror in an antecedent, and picking one of the three would put a claim
    /// about an uninhabited type on a footing none of them supports — so the
    /// specification path refuses the introduction instead.
    P015,
    /// A non-constant array index inside the body of an `exists`/`unique`-
    /// quantified specification function.
    ///
    /// Code generation guards every non-constant index with a trap, and a
    /// reachability body is the one specification body the downstream judgment
    /// *reduces*. The judgment fixes an arbitrary typed entry vector and only
    /// then lets the choices range, so a trap does not restrict the claim to
    /// the entries whose index is in range: at every entry the guard rejects,
    /// no choice reaches an exit state, the observation set is empty, and the
    /// theorem is **false** rather than narrowed. That is the same failure an
    /// `assume` over an entry parameter produces, and it is invisible to the
    /// Rocq type-check gate, which admits open proofs and therefore asserts
    /// well-formedness rather than truth.
    ///
    /// The rule reads the index alone, not where its value comes from. Only an
    /// entry-derived index can actually falsify the theorem, but whether a
    /// given index is entry-derived is a dataflow question whose answer is not
    /// visible at the access, and a rule whose reach a reader cannot determine
    /// from the site it fires on is worse than a stricter one they can.
    ///
    /// The rule is also **lexical**, and that is a known limitation rather than
    /// a property of the problem. A retained body calls executable functions,
    /// and the judgment reduces the whole activation — callee frames included —
    /// to a value stack, so a body that calls a function which indexes an array
    /// by a value derived from an entry parameter still carries the trap, still
    /// empties the observation set, and still ships a false obligation. This
    /// rule does not fire there: it sees the access, not the call graph, and
    /// deciding the interprocedural case needs whole-program dataflow it
    /// deliberately does not do. So moving a rejected access behind a call is
    /// not a remedy, and the diagnostic does not offer it as one.
    ///
    /// What an author whose reachability body must reach a dynamic index should
    /// do is keep entry-derived values out of its reachable call graph and let
    /// the index derive from a `@` choice constrained by `assume`. A choice that
    /// traps is simply not the witness the existential needs, so the claim still
    /// holds; an entry that traps has no witness left to offer, so it does not.
    ///
    /// A `forall`-quantified body is untouched: it is omitted from the emitted
    /// module's functions and never reduced, so its non-constant index keeps
    /// the symbolic range bound the element's own definition states, in both
    /// the universal and the existential polarity. So is a constant index
    /// anywhere — it selects an element outright, and [`PCode::P014`] already
    /// rejects the out-of-bounds ones.
    P016,
    /// An arithmetic-mode annotation written where the obligation language has
    /// no mode to select, or arithmetic that traps on overflow written inside a
    /// body the downstream judgment reduces. Two wordings, keyed on which of
    /// those the body is.
    ///
    /// A `forall` or plain specification function — a helper `fn` declared
    /// inside a `spec` block included, since it is translated into an
    /// obligation of its own — becomes a *term*, and a term's `+`, `-` and `*`
    /// are the wrapping machine operators: `HBinop` denotes
    /// `Wasm_int.int_add`/`int_sub`/`int_mul`, modular at the operand width,
    /// and no other operator exists downstream for an annotation to select.
    /// Both spellings would therefore translate to the identical term and be
    /// silently dropped, so both are refused rather than accepted and ignored.
    /// The refusal covers every position in such a body, not only the ones the
    /// term walk reaches: an annotation in an array index or an aggregate
    /// literal is folded through on its way to a value, so a check that lived
    /// on the term path alone would accept exactly the spellings that vanish
    /// most quietly.
    ///
    /// An `exists`/`unique`-quantified body is the opposite case. It is
    /// compiled, and its obligation is a claim about running it, so arithmetic
    /// whose effective mode traps puts a trap on the path the judgment reduces.
    /// The judgment fixes an arbitrary typed entry vector before letting the
    /// choices range, so a trap does not narrow the claim to the entries whose
    /// arithmetic fits: at an entry that overflows, no choice reaches an exit
    /// state, the observation set is empty and the theorem is false. In a
    /// `unique` body it is worse, because a trapping choice shrinks the
    /// successful set and can make a uniqueness claim hold for a reason the
    /// source does not state. `wrapping(...)` is accepted throughout such a
    /// body — including inside its final `assert`, whose arithmetic is both
    /// compiled and translated — and translates to the identical term, which is
    /// what makes it a remedy rather than a second rejection.
    ///
    /// The two wordings are keyed on the *quantifier of the function being
    /// translated*, never on lexical containment in a `spec` block. A nested
    /// `exists { }` block inside a `forall` function is not a reachability
    /// body: nothing reduces it, and its annotation is as meaningless as one
    /// written at the top of the same function.
    P017,
    /// An `exists`/`unique`-quantified specification function whose body
    /// reaches, through calls, a function carrying an overflow guard.
    ///
    /// [`PCode::P017`]'s second wording is lexical, and the trap it describes
    /// does not have to be written in the body to be taken by it: the judgment
    /// reduces the whole activation, callee frames included, so a guard in a
    /// function the body calls empties exactly the same observation set and
    /// makes exactly the same claim false. This rule is the interprocedural
    /// half, computed over code generation's own call resolution so the
    /// function it inspects is the function the call actually lowers to.
    ///
    /// It is deliberately a reachability question and not a dataflow one. Only
    /// an entry-derived operand can really trip a guard, but whether a given
    /// value is entry-derived is not visible at the call, and a rule whose
    /// reach a reader cannot determine from the site it fires on is worse than
    /// a stricter one they can.
    ///
    /// A callee code generation cannot resolve is counted as carrying a guard.
    /// Failing open there would ship a false obligation through a gate that
    /// admits open proofs, which is precisely the outcome the rule exists to
    /// prevent.
    ///
    /// `external fn` callees are skipped. Code generation never receives a
    /// dependency's bytes — they arrive at link time, after this pass has run —
    /// so nothing here can say whether a foreign body traps, and guessing
    /// either way would be a claim about bytes this compiler has not seen. The
    /// linker is the sole judge of a merged body.
    P018,
}

impl fmt::Display for PCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            PCode::P001 => "P001",
            PCode::P002 => "P002",
            PCode::P003 => "P003",
            PCode::P004 => "P004",
            PCode::P005 => "P005",
            PCode::P006 => "P006",
            PCode::P007 => "P007",
            PCode::P008 => "P008",
            PCode::P009 => "P009",
            PCode::P010 => "P010",
            PCode::P011 => "P011",
            PCode::P012 => "P012",
            PCode::P013 => "P013",
            PCode::P014 => "P014",
            PCode::P015 => "P015",
            PCode::P016 => "P016",
            PCode::P017 => "P017",
            PCode::P018 => "P018",
        };
        f.write_str(code)
    }
}

/// One proof-mode translation diagnostic, carrying the source location and the
/// defining file's module path so it renders with the right file label.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct HassertDiagnostic {
    code: PCode,
    location: Location,
    module_path: Vec<String>,
    message: String,
}

impl HassertDiagnostic {
    pub(crate) fn new(
        code: PCode,
        location: Location,
        module_path: Vec<String>,
        message: String,
    ) -> Self {
        Self {
            code,
            location,
            module_path,
            message,
        }
    }
}

impl fmt::Display for HassertDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(label) = file_label(&self.module_path) {
            write!(f, "{label}:")?;
        }
        write!(
            f,
            "{}:{}: error[{}]: {}",
            self.location.start_line, self.location.start_column, self.code, self.message
        )
    }
}

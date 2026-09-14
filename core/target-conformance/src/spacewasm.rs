//! The `SpaceWasm` flight interpreter's envelope, and what an artifact costs it.
//!
//! `SpaceWasm` is a `no_std` WebAssembly 1.0 decoder, validator and interpreter
//! written for on-board use. It has no dynamic allocation it did not ask an
//! embedder for, so its acceptance set is narrower than the standard's in ways
//! the standard does not describe: a function's parameter size is one byte, its
//! local size is two, an import's names are read into a fixed buffer, and the
//! verifier's two stacks are const generics an embedder chooses at compile time.
//!
//! [`check`] answers both questions a build has about that envelope. The first
//! is a verdict: is this module one the decoder would accept, and — in the one
//! place the two part company — one it would execute as written? The second is
//! a measurement: what must the two const generics be for it to fit? An
//! embedder cannot read the second off the standard, and a module that exceeds
//! it fails at load time on hardware, which is the failure this module exists
//! to move to build time.
//!
//! # Where the numbers come from
//!
//! Every constant below cites the upstream file and line it was read from, and
//! names its **unit**, because the two quantities that sound alike are not:
//! `MAX_STACK_DEPTH` bounds a stack with one entry per **value** whatever its
//! width, while a function's recorded stack usage is in **words**, where `i64`
//! and `f64` count two. Sizing an embedder's verifier from the word figure
//! over-allocates; sizing an engine's value stack from the value figure
//! under-allocates. [`Report`] carries both, each labelled.
//!
//! The citations are to `spacewasm` 0.7.1, which is the version
//! [`LIMITS_FROM`] names and the version the test suite decodes against.
//!
//! # Two kinds of limit
//!
//! Some of these are **decode** limits: the decoder refuses the module. Others
//! are **registration** limits: the module decodes, and then no embedder can
//! supply what it imports, so it can never be instantiated. The 31-byte import
//! name and the nine-parameter host function are the second kind, and the
//! refusals say so — a user told "the decoder rejects 32 bytes" would shorten a
//! name to 32 and meet the same wall.
//!
//! # What is not checked
//!
//! [`check`]'s verdict is a claim about the limits below, not a proof of
//! decodability. The interpreter refuses through one enum, `ValidationError`,
//! and this crate's README classifies every variant of it: modelled here,
//! reached first by the WebAssembly 1.0 validation, decided by a host set these
//! bytes do not carry, chosen by an embedder and therefore measured instead, or
//! out of reach on this compiler's path. No variant is left unclassified. An
//! enumeration is the only form that stays honest, because the residue nobody
//! wrote down is the residue nothing turns red about.
//!
//! # Where this is stricter than the decoder
//!
//! One refusal is not a transcription of a `ValidationError`, and it is the
//! only one: [`Violation::IndexTruncated`]. `spacewasm` 0.7.1 narrows a defined
//! function's and a defined global's position into a 16-bit IR word without
//! checking the narrowing, so a module naming definition 65,536 or beyond
//! **loads** and then runs against the definition 65,536 below it. There is no
//! decoder verdict to agree with there, only a wrong execution to inherit, and
//! for a flight target a build that refuses is better than an artifact that
//! flies and calls the wrong function. [`MAX_IR_INDEX`] carries the mechanism
//! and the five places that reach it.

use wasmparser::{
    BinaryReaderError, CompositeInnerType, ElementItems, ElementSectionReader, ExternalKind,
    FuncToValidate, FuncValidator, FuncValidatorAllocations, FunctionBody, MemoryType, Name,
    NameSectionReader, Operator, OperatorsReader, Parser, Payload, TypeRef, ValType, ValidPayload,
    Validator, ValidatorResources,
};

pub use crate::errors::{IndexKind, IndexSpace, NamePart, ReferenceSite, Violation, Violations};

/// The upstream release every constant in this module was read from.
///
/// Carried in [`Report::limits_from`] so a build log says which decoder the
/// numbers describe: they are an implementation's envelope, not the standard's,
/// and a release that widens one makes a report that quoted it stale.
pub const LIMITS_FROM: &str = "spacewasm 0.7.1";

/// Parameter **words** one function may declare (`src/module.rs:583,593`;
/// the field is `Func::parameter_size: u8`, `src/code.rs:72`).
///
/// A word is four bytes: `i64` and `f64` parameters count two, everything else
/// one. The decoder computes the same sum and refuses above `0xFF`.
pub const MAX_PARAM_WORDS: u32 = 255;

/// Local **words** one function may declare (`src/code.rs:142-145`; the field
/// is `Func::local_size: u16`, `src/code.rs:69`).
///
/// Same word convention as [`MAX_PARAM_WORDS`], summed over every locals group
/// in the body. Parameters are not included: the decoder keeps the two sizes in
/// separate fields.
pub const MAX_LOCAL_WORDS: u64 = 65_535;

/// Locals one **group** of a code-section entry may declare
/// (`src/code.rs:126-127`).
///
/// A body declares its locals as run-length pairs, and the decoder stores each
/// run's count in a `u16`. This is a bound on one pair, not on the body: a
/// function may carry many groups, and their words are then bounded by
/// [`MAX_LOCAL_WORDS`].
pub const MAX_LOCALS_GROUP_COUNT: u32 = 65_535;

/// Bytes of an import's module name or field name that an embedder can
/// **register** a host for (`src/host.rs:327,352`: `HOST_FUNCTION_NAME_CAP` and
/// `HOST_MODULE_NAME_CAP`, both 31).
///
/// The decoder itself reads import names into a 32-byte buffer
/// (`src/imports.rs:417,420`), so 32 decodes and 33 does not — but a host module
/// and a host function are named through `HostName<31>`, which is the only way
/// an embedder supplies an import. A 32-byte name therefore decodes into a
/// module that can never be instantiated, which is why the registration cap is
/// the one this crate enforces and the one its refusals name.
pub const MAX_IMPORT_NAME_BYTES: usize = 31;

/// Parameters one imported (host) function may declare (`src/host.rs:168`:
/// `MAX_HOST_FUNCTION_PARAMS`).
///
/// A registration limit, like [`MAX_IMPORT_NAME_BYTES`]: host arguments travel
/// through a fixed-size list of nine (`HostValList`, `src/host.rs:196-199`), so
/// a ten-parameter host function cannot be built and the import can never be
/// bound.
pub const MAX_HOST_FUNCTION_PARAMS: usize = 9;

/// Results one imported (host) function may declare (`src/host.rs:180`:
/// `HostFunctionError::MultiReturnNotAllowed`).
///
/// One, like every WebAssembly 1.0 function — the value is spelled out because
/// the refusal it produces is a registration refusal rather than the validator's
/// multi-value one, and the two arrive for different reasons.
pub const MAX_HOST_FUNCTION_RESULTS: usize = 1;

/// Bytes of a custom section's name the decoder reads (`src/module.rs:483`:
/// `CustomSection::MAX_NAME_LENGTH`).
///
/// The name is read into a fixed 32-byte buffer, and a longer one overflows it
/// before the payload is reached — so a module is refused for the *name* of a
/// section whose contents no runtime would have looked at.
pub const MAX_CUSTOM_SECTION_NAME_BYTES: usize = 32;

/// Linear-memory pages the decoder admits at the default page size
/// (`src/types.rs:332-343`).
///
/// 65,536 pages of 64 KiB is the whole 32-bit address space, so this is the
/// standard's own bound rather than a narrowing of it.
pub const MAX_MEMORY_PAGES: u64 = 65_536;

/// Control frames the verifier's stack holds, as a const generic
/// (`src/text.rs:444`: `control_frames: StaticVec<ControlFrame, MAX_CONTROL_FRAMES>`).
///
/// Counted **including the implicit function-body frame**: `TextBuilder::new`
/// pushes one before reading a single operator (`src/text.rs:505-508`), which is
/// the same convention `wasmparser` uses, so [`FunctionMetrics::max_control_depth`]
/// is comparable with an embedder's const generic with no adjustment.
///
/// The name is the const generic's; this crate does not fix a value for it,
/// because an embedder chooses one. What the reference `spacewasm_std`
/// embedding chooses is [`REFERENCE_MAX_CONTROL_FRAMES`].
pub const REFERENCE_MAX_CONTROL_FRAMES: u32 = 64;

/// Operand **values** the verifier's stack holds, as a const generic
/// (`src/text.rs:445`: `value_stack: StaticVec<OperandType, MAX_STACK_DEPTH>`).
///
/// One entry per value, whatever its width — the same unit as
/// [`FunctionMetrics::max_operand_values`] and *not* the unit of
/// [`FunctionMetrics::max_operand_words`]. The reference `spacewasm_std`
/// embedding chooses 256.
pub const REFERENCE_MAX_STACK_DEPTH: u32 = 256;

/// Words of the frame header the interpreter writes per call
/// (`src/code.rs:157-158`: `2 + local_size + stack_usage`).
const FRAME_HEADER_WORDS: u32 = 2;

/// Words one call frame may occupy (`src/code.rs:157-160`).
///
/// The decoder bounds the whole frame — two words of header, the locals, and the
/// operand stack at the call site — to a 16-bit length, and refuses a function
/// whose worst case is wider. It bites *before* [`MAX_LOCAL_WORDS`] does: a
/// function with no operand stack at all may declare 65,533 local words and no
/// more, because two more words of header are always there. Both bounds are
/// checked, because a function with a large operand stack meets this one with
/// far fewer locals.
pub const MAX_FRAME_WORDS: u64 = 65_535;

/// The largest value one of the interpreter's 16-bit IR immediates holds
/// (`src/text.rs:1099-1102`, `instr_imm_8_or_16`; and the same refusal raised
/// by hand for a `call_indirect` type index at `src/compiler.rs:281-283`).
///
/// A property of the *compiled* form rather than of the WebAssembly encoding,
/// which admits a 32-bit value in both places. Two operators can present a value
/// over it, and they meet the cap in two different places: a `br_table`'s target
/// count goes through the shared emitter (called at `src/compiler.rs:166`), and
/// a `call_indirect`'s type index is checked by hand before its `write_16`.
///
/// The same word also holds a **defined function** and a **defined global**
/// position, and those two are the crate's one refusal that is stricter than
/// the decoder rather than a transcription of it: `Module::get_func_ref`
/// (`src/module.rs:383-389`) and `Module::get_global_ref`
/// (`src/module.rs:423-429`) subtract the imported count, bounds-check the
/// result against the defined table and then narrow it with `as u16` and no
/// check at all. A module naming a definition at 65,536 or beyond therefore
/// loads and runs against the definition 65,536 below it, which is why
/// [`Violation::IndexTruncated`] refuses it here. Five places reach those two
/// accessors: a `call` operand (`src/compiler.rs:202-206`), a `global.get` or
/// `global.set` operand (`src/text.rs:631-634`), an export descriptor
/// (`src/module.rs:753-778`), an element-segment entry
/// (`src/module.rs:870-874`) and the `start` section (`src/module.rs:299-303`).
/// A constant expression's `global.get` is not one: a module-local global is
/// refused there as `InvalidConstantInstruction` before the cast is reached
/// (`src/constant.rs:291-298`). The cross-module link path is not one either
/// (`src/imports.rs:71,169`): it reaches the same two accessors, but with an
/// index it reads back out of the *exporting* module's export descriptor, so a
/// refusal at that module's export section already covers it and it names
/// nothing of its own. An imported function or global is not one
/// either — it resolves to a `Ref::Host` or a `Ref::Extern` carrying no
/// narrowed position — but the imports **shift** every definition after them,
/// which is why the position is the index minus the imported count and not the
/// index.
///
/// The two remaining index kinds are out of reach of either refusal, and each
/// for its own reason:
///
/// - A **local** index is not an index in the compiled form: it is a frame
///   offset, written as `instr_imm_8(op, ty)` plus `write_16(frame_offset)`
///   (`src/compiler.rs:293-312`), and how far that offset can reach is
///   [`MAX_FRAME_WORDS`], which bites first.
/// - A **label** index is refused as `InvalidLabelIndex` when it is past the
///   control stack (`src/text.rs:726-728`), and stock validation refuses it
///   before this crate would; a depth inside the stack is compiled into a jump
///   target rather than written as an index.
pub const MAX_IR_INDEX: u32 = 65_535;

/// Operand **words** one branch may discard (`src/text.rs:741-748`).
///
/// A branch to a block or a loop is compiled with the operands it unwinds in a
/// single byte of its jump-target word, counted in words from the bottom of the
/// live stack up to the target frame's own height — the same word convention
/// and the same stop-at-the-first-untracked-slot rule as
/// [`FunctionMetrics::max_operand_words`]. A branch to the function's own
/// outermost frame takes a different route: it is compiled as an early return
/// (`src/text.rs:733-739`) and is not measured against this at all, and neither
/// is `return`.
///
/// Five instructions reach the compiler's `write_label_target`, and this crate
/// measures four of them: `br`, `br_if`, and each target of a `br_table`
/// including the default (`src/compiler.rs:149,158,171,181`). The fifth is the
/// implicit branch an `else` emits over its own arm
/// (`src/compiler.rs:129-134`), and it cannot reach this cap at the feature set
/// this target decodes: what stands above the `if` frame's floor at an `else` is
/// the block's result type, and WebAssembly 1.0 admits at most one result, so
/// the unwind there is at most two words. A module whose block yields more is
/// outside the 1.0 envelope and refused for that first.
pub const MAX_BRANCH_UNWIND_WORDS: u32 = 255;

/// What one function costs the runtime that loads it.
///
/// Every field is exact rather than an upper bound: the two operand maxima are
/// sampled after every operator of the body, which is the only way to obtain
/// them, since a validator's stack heights are readings on a live validation
/// rather than a summary it reports at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionMetrics {
    /// Index in the module's function index space — imports first, so this is
    /// the number a `call` names.
    pub index: u32,
    /// The function's name from the `name` section, when it carries one.
    pub name: Option<String>,
    /// Declared parameter words (`i64`/`f64` count two). Bounded by
    /// [`MAX_PARAM_WORDS`].
    pub param_words: u32,
    /// Declared local words, parameters excluded. Bounded by
    /// [`MAX_LOCAL_WORDS`].
    pub local_words: u32,
    /// Deepest control-frame nesting reached, **including** the function-body
    /// frame, so the shallowest possible value is 1. Compare against an
    /// embedder's `MAX_CONTROL_FRAMES`.
    pub max_control_depth: u32,
    /// Tallest operand stack reached, in **values**. Compare against an
    /// embedder's `MAX_STACK_DEPTH`.
    pub max_operand_values: u32,
    /// The same peak weighted by width, in **words**: `i64` and `f64` count two
    /// (`src/code.rs:29-32`).
    ///
    /// Counted from the bottom of the stack up to the first operand whose type
    /// the validator no longer tracks — every slot after an `unreachable`,
    /// until the enclosing block ends — which contributes nothing, and neither
    /// does anything above it. That is the interpreter's own rule
    /// (`src/text.rs:789-802`), and it has to be, because the figure it
    /// produces is the one the interpreter checks its frame bound against:
    /// counting an untracked slot at any width would make this crate refuse
    /// functions the decoder loads.
    pub max_operand_words: u32,
}

impl FunctionMetrics {
    /// The call frame this function leaves on the engine's stack, in words:
    /// two words of header plus the locals plus the operand peak. This is the
    /// quantity `Engine::new`'s stack budget must accommodate, and it is a
    /// different question from either verifier bound.
    ///
    /// Computed rather than stored, so it cannot disagree with the two fields
    /// it is the sum of.
    #[must_use]
    pub fn frame_words(&self) -> u32 {
        FRAME_HEADER_WORDS
            .saturating_add(self.local_words)
            .saturating_add(self.max_operand_words)
    }

    /// The name a message calls this function: the `name` section's where it
    /// has one, its index in the function index space otherwise.
    fn named(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("func[{}]", self.index))
    }
}

/// What [`check`] measured about a module that passed.
///
/// The per-function measurements are the record; every module-wide figure is
/// derived from them on demand rather than stored beside them, so a report
/// cannot state a maximum no function in it attains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Every function the module defines, in index order. Imported functions
    /// have no body to measure and do not appear.
    pub functions: Vec<FunctionMetrics>,
    /// The size of the checked module, in bytes.
    pub wasm_size: usize,
    /// The upstream release the limits were read from: [`LIMITS_FROM`].
    pub limits_from: &'static str,
}

impl Report {
    /// The deepest control nesting in the module, and the function that
    /// attains it. A module defining no function answers `(String::new(), 0)`:
    /// it needs no budget at all.
    #[must_use]
    pub fn deepest(&self) -> (String, u32) {
        self.peak(|metrics| metrics.max_control_depth)
    }

    /// The tallest operand stack in **values**, and the function that attains
    /// it. See [`Self::deepest`] for the empty case.
    #[must_use]
    pub fn tallest(&self) -> (String, u32) {
        self.peak(|metrics| metrics.max_operand_values)
    }

    /// The widest operand stack in **words**, and the function that attains it.
    /// Not necessarily the same function as [`Self::tallest`]. See
    /// [`Self::deepest`] for the empty case.
    #[must_use]
    pub fn widest(&self) -> (String, u32) {
        self.peak(|metrics| metrics.max_operand_words)
    }

    /// The highest one axis reaches, and the function reaching it.
    fn peak(&self, pick: fn(&FunctionMetrics) -> u32) -> (String, u32) {
        self.functions
            .iter()
            .max_by_key(|metrics| pick(metrics))
            .map_or_else(
                || (String::new(), 0),
                |metrics| (metrics.named(), pick(metrics)),
            )
    }

    /// The one line a build owes an embedder: what the module needs, in both
    /// units, and the reference configuration to compare it against.
    ///
    /// Both maxima carry their unit, because the two that sound alike are not
    /// the same quantity: `MAX_STACK_DEPTH` counts operand *values* whatever
    /// their width, while the engine's stack is in *words*, where an `i64`
    /// counts two. Sizing either const generic from the other's number is the
    /// deploy-time failure this check exists to move to build time.
    ///
    /// The sentence lives here rather than in each caller because two programs
    /// print it — the compiler about the module it wrote, and the project
    /// driver about the module an optimizer produced — and a build log whose
    /// two lines named their units differently would be worse than one line.
    #[must_use]
    pub fn summary_line(&self) -> String {
        if self.functions.is_empty() {
            return format!(
                "spacewasm: conformant with WebAssembly 1.0; the module defines no function, \
                 so it needs no control-frame or operand-stack budget. Limits from {}.",
                self.limits_from
            );
        }
        let (deepest_fn, depth) = self.deepest();
        let (tallest_fn, values) = self.tallest();
        let (widest_fn, words) = self.widest();
        format!(
            "spacewasm: conformant with WebAssembly 1.0; deepest control nesting {depth} in \
             `{deepest_fn}`, tallest operand stack {values} values in `{tallest_fn}` (peak \
             {words} stack words in `{widest_fn}`). Build the embedder with \
             MAX_CONTROL_FRAMES >= {depth} and MAX_STACK_DEPTH >= {values} (spacewasm_std uses \
             {REFERENCE_MAX_CONTROL_FRAMES} and {REFERENCE_MAX_STACK_DEPTH}); limits from {}.",
            self.limits_from,
        )
    }

    /// The warnings owed when a module needs more than the reference embedder
    /// offers, one per axis that is over.
    ///
    /// A conformant module is not a loadable one: `spacewasm_std` fixes both
    /// const generics, and an artifact above either bound is refused at load by
    /// a configuration most embedders start from. Naming the three functions
    /// that reach highest is what makes the warning actionable — the remedy is
    /// either a bigger const generic or a flatter function, and which one is
    /// affordable depends on which functions they are.
    #[must_use]
    pub fn budget_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let depth = self.deepest().1;
        if depth > REFERENCE_MAX_CONTROL_FRAMES {
            warnings.push(format!(
                "spacewasm: control nesting {depth} exceeds spacewasm_std's MAX_CONTROL_FRAMES \
                 of {REFERENCE_MAX_CONTROL_FRAMES}. Deepest functions: {}. Either raise the \
                 const generic in your embedder or flatten the nesting in these functions.",
                self.leaders(|metrics| metrics.max_control_depth),
            ));
        }
        let values = self.tallest().1;
        if values > REFERENCE_MAX_STACK_DEPTH {
            warnings.push(format!(
                "spacewasm: operand stack {values} values exceeds spacewasm_std's \
                 MAX_STACK_DEPTH of {REFERENCE_MAX_STACK_DEPTH}. Tallest functions: {}. Either \
                 raise the const generic in your embedder or split the expressions in these \
                 functions.",
                self.leaders(|metrics| metrics.max_operand_values),
            ));
        }
        warnings
    }

    /// The three functions that reach highest on one axis, named and numbered,
    /// worst first. Fewer than three functions name however many there are.
    fn leaders(&self, pick: fn(&FunctionMetrics) -> u32) -> String {
        let mut ranked: Vec<&FunctionMetrics> = self.functions.iter().collect();
        ranked.sort_by_key(|metrics| core::cmp::Reverse(pick(metrics)));
        ranked
            .into_iter()
            .take(3)
            .map(|metrics| format!("`{}` ({})", metrics.named(), pick(metrics)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Checks `wasm` against everything a `SpaceWasm` embedder would refuse it for,
/// and measures what loading it costs.
///
/// The walk is the incremental validation loop rather than
/// `Validator::validate_all`, because the two per-function maxima are readings
/// on a live `FuncValidator` and there is no summary to ask for afterwards. The
/// structural scan runs beside it and does not stop when validation does: a
/// module refused for a post-1.0 instruction is still worth telling its author
/// about the 300-word parameter list further down.
///
/// # How a caller reaches this
///
/// Both programs that run it gate on the target *variant* rather than on a
/// predicate of the shared target vocabulary, and deliberately: a predicate
/// answers one question that serves every target answering `true`, while what
/// follows such a gate is one runtime's envelope, measured by one runtime's
/// function into one runtime's [`Report`]. A `bool` meaning "check the
/// envelope" would hand a second checked target the wrong one, so adding a
/// second target is a dispatch on the variant rather than a wider predicate.
///
/// # Errors
///
/// Returns every finding, in a fixed order: the WebAssembly 1.0 verdict first
/// when there is one, then per-function findings in index order, then
/// per-import findings in import order, then what the sections outside a body
/// name — the exports, the `start` section and the element segments, in that
/// order — then the module-shape findings.
pub fn check(wasm: &[u8]) -> Result<Report, Violations> {
    let scan = Scan::run(wasm);
    let violations = scan.violations();
    if !violations.is_empty() {
        return Err(Violations::new(violations));
    }
    Ok(scan.report(wasm.len()))
}

/// One imported function, as both decoders see it before any host is offered.
#[derive(Debug)]
struct ImportFacts {
    module: String,
    field: String,
    params: usize,
    results: usize,
}

/// One defined function's declared widths, read from the sections rather than
/// from the validator, so they survive a validation failure elsewhere.
#[derive(Debug)]
struct DeclaredWidths {
    index: u32,
    param_words: u64,
    local_words: u64,
    largest_locals_group: u32,
}

/// The three maxima one function body reaches.
#[derive(Debug, Default, Clone, Copy)]
struct Maxima {
    control_depth: u32,
    operand_values: u32,
    operand_words: u32,
}

/// The per-body quantities the interpreter's *compiler* bounds, as against the
/// ones its decoder reads out of a section.
///
/// Each is the worst the body reaches rather than the first offender. The
/// interpreter stops at the first and this crate does not, so a body with a
/// thousand over-deep branches would otherwise produce a thousand findings that
/// say the same thing; and where they differ, the worst is the one that decides
/// how far a remedy has to go.
#[derive(Debug, Default, Clone, Copy)]
struct LimitPeaks {
    /// Deepest operand unwind at a branch, in words. Bounded by
    /// [`MAX_BRANCH_UNWIND_WORDS`].
    branch_unwind_words: u32,
    /// Largest type index a `call_indirect` names. Bounded by [`MAX_IR_INDEX`].
    call_indirect_type: u32,
    /// Most non-default targets one `br_table` lists. Bounded by
    /// [`MAX_IR_INDEX`], which the target count shares with an index because
    /// both are one 16-bit word of the interpreter's compiled form.
    br_table_targets: u32,
    /// Largest function index a `call` in the body names, imports included.
    /// Zero when the body calls nothing, which is the same answer as calling
    /// the first function and is below the cap either way.
    called_function: u32,
    /// Largest global index a `global.get` or a `global.set` in the body names,
    /// imports included. Zero when the body names none, as above.
    referenced_global: u32,
}

/// One place outside a function body where a module names a defined function or
/// global, and the largest index it names there.
///
/// Recorded while the sections are read and judged afterwards, because the
/// number of definitions a reference has to be inside is not known until the
/// whole module has been walked, and because the function a body belongs to is
/// named from a `name` section that may come last.
///
/// The largest index per site rather than every index, for the reason
/// [`LimitPeaks`] gives: an element segment naming a thousand functions past
/// the cap is one fault, and the largest is the one that decides how far a
/// remedy has to go.
#[derive(Debug)]
struct Reference {
    space: IndexSpace,
    site: ReferenceSite,
    index: u32,
}

/// What one validated function body yielded: what it costs a runtime, and what
/// it costs the runtime's own compiler.
///
/// One struct rather than two parallel vectors, so a body's measurement and its
/// peaks cannot be read at two different indices.
#[derive(Debug, Default, Clone, Copy)]
struct Measured {
    maxima: Maxima,
    peaks: LimitPeaks,
}

/// Everything one pass over a module learned.
#[derive(Debug, Default)]
struct Scan {
    /// Parameters and results of every type-section entry, in index order.
    type_shapes: Vec<(Vec<ValType>, Vec<ValType>)>,
    imports: Vec<ImportFacts>,
    /// Type index of every defined function, in definition order.
    function_types: Vec<u32>,
    declared: Vec<DeclaredWidths>,
    measured: Vec<Measured>,
    memories: Vec<MemoryType>,
    custom_section_names: Vec<String>,
    names: Vec<(u32, String)>,
    /// The first thing the validator refused, if it refused anything.
    invalid: Option<String>,
    /// How many functions the module imports, which is where the defined
    /// function index space starts.
    imported_functions: u32,
    /// How many globals the module imports, which is where the defined global
    /// index space starts.
    imported_globals: u32,
    /// How many globals the module defines.
    defined_globals: u32,
    /// Every reference to a defined function or global made outside a function
    /// body, in the order the sections are read.
    references: Vec<Reference>,
}

impl Scan {
    /// Walks `wasm` once, validating and measuring as far as the validator gets
    /// and reading structure all the way to the end.
    fn run(wasm: &[u8]) -> Self {
        let mut scan = Self::default();
        let mut validator = Some(Validator::new_with_features(crate::WASM1_ENVELOPE));

        for payload in Parser::new(0).parse_all(wasm) {
            let payload = match payload {
                Ok(payload) => payload,
                Err(err) => {
                    scan.record_invalid(&err);
                    break;
                }
            };
            scan.observe(&payload);

            let Some(active) = validator.as_mut() else {
                continue;
            };
            match active.payload(&payload) {
                Ok(ValidPayload::Func(to_validate, body)) => {
                    match measure(to_validate, &body) {
                        Ok(measured) => scan.measured.push(measured),
                        Err(err) => {
                            scan.record_invalid(&err);
                            validator = None;
                        }
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    scan.record_invalid(&err);
                    validator = None;
                }
            }
        }
        scan
    }

    /// Keeps the first refusal. A validator that has refused once is in no
    /// state to be asked again, so a second message would describe the
    /// harness rather than the module.
    fn record_invalid(&mut self, err: &BinaryReaderError) {
        let _ = self.invalid.get_or_insert_with(|| err.to_string());
    }

    /// Reads out of one payload everything the `SpaceWasm` limits are asked about.
    fn observe(&mut self, payload: &Payload<'_>) {
        match payload {
            Payload::TypeSection(reader) => {
                for group in reader.clone().into_iter().flatten() {
                    for sub_type in group.types() {
                        let shape = match &sub_type.composite_type.inner {
                            CompositeInnerType::Func(func) => {
                                (func.params().to_vec(), func.results().to_vec())
                            }
                            // No other composite type exists in WebAssembly 1.0;
                            // the entry is still counted so later type indices
                            // keep meaning what the module says they mean.
                            _ => (Vec::new(), Vec::new()),
                        };
                        self.type_shapes.push(shape);
                    }
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.clone().into_imports().flatten() {
                    // Spelled without a wildcard: the two counts below are
                    // subtracted from every index the module writes, so a kind
                    // this match guesses at is a shift applied to the wrong
                    // number of definitions rather than a kind left unread.
                    let (params, results) = match import.ty {
                        // Both spellings are a function import — stock
                        // validation resolves them into one index space — so
                        // both are counted and both carry a signature. The
                        // exact spelling is outside WebAssembly 1.0 and the
                        // module is refused for that, but the count still has
                        // to be right or every finding about it names the
                        // wrong position.
                        TypeRef::Func(index) | TypeRef::FuncExact(index) => {
                            self.imported_functions += 1;
                            self.type_shapes
                                .get(index as usize)
                                .map_or((0, 0), |(params, results)| {
                                    (params.len(), results.len())
                                })
                        }
                        // A global import carries no signature to hold to the
                        // host caps, and is counted all the same: it shifts
                        // every defined global after it.
                        TypeRef::Global(_) => {
                            self.imported_globals += 1;
                            (0, 0)
                        }
                        // A memory, a table and a tag import name neither
                        // counted space and carry no signature; each is still
                        // refused for its name like any other import.
                        TypeRef::Table(_) | TypeRef::Memory(_) | TypeRef::Tag(_) => (0, 0),
                    };
                    self.imports.push(ImportFacts {
                        module: import.module.to_string(),
                        field: import.name.to_string(),
                        params,
                        results,
                    });
                }
            }
            Payload::FunctionSection(reader) => {
                self.function_types
                    .extend(reader.clone().into_iter().flatten());
            }
            Payload::MemorySection(reader) => {
                self.memories.extend(reader.clone().into_iter().flatten());
            }
            Payload::GlobalSection(reader) => {
                self.defined_globals += reader.count();
            }
            Payload::ExportSection(reader) => {
                for export in reader.clone().into_iter().flatten() {
                    // Spelled without a wildcard for the same reason the
                    // import match above is: a function or global export is a
                    // reference the narrowing applies to, and a kind guessed at
                    // here is a reference never read.
                    let space = match export.kind {
                        // Both spellings name a function, as stock validation
                        // resolves them; only the plain one arrives, since the
                        // export reader refuses the exact spelling outright,
                        // which is why no fixture can reach the second here.
                        ExternalKind::Func | ExternalKind::FuncExact => IndexSpace::Function,
                        ExternalKind::Global => IndexSpace::Global,
                        // A memory or a table export names neither space, and
                        // the interpreter answers both from a fixed slot rather
                        // than through a narrowing accessor. A tag export names
                        // neither either, and puts the module outside
                        // WebAssembly 1.0, which is what it is refused for.
                        ExternalKind::Table | ExternalKind::Memory | ExternalKind::Tag => continue,
                    };
                    self.references.push(Reference {
                        space,
                        site: ReferenceSite::Export {
                            name: export.name.to_string(),
                        },
                        index: export.index,
                    });
                }
            }
            Payload::StartSection { func, .. } => {
                self.references.push(Reference {
                    space: IndexSpace::Function,
                    site: ReferenceSite::Start,
                    index: *func,
                });
            }
            Payload::ElementSection(reader) => self.observe_elements(reader),
            Payload::CodeSectionEntry(body) => self.observe_body(body),
            Payload::CustomSection(section) => {
                self.custom_section_names.push(section.name().to_string());
                if section.name() == "name" {
                    self.observe_names(section.data(), section.data_offset());
                }
            }
            _ => {}
        }
    }

    /// Records the largest defined function index each element segment names.
    ///
    /// The segments are numbered from the section rather than from the entries
    /// that survive reading it: the finding names the segment a reader is to
    /// open, and that number should not turn on what the reader made of a
    /// neighbour. No module separates the two spellings today — stock
    /// `SectionLimited` ends its iteration at the first entry it cannot read,
    /// so there is never a later segment to misnumber — which is why nothing
    /// here turns red on the difference.
    fn observe_elements(&mut self, reader: &ElementSectionReader<'_>) {
        for (segment, element) in reader.clone().into_iter().enumerate() {
            let Ok(element) = element else {
                continue;
            };
            let ElementItems::Functions(functions) = element.items else {
                // An expression-initialized segment is outside WebAssembly 1.0,
                // and the module is refused for that.
                continue;
            };
            let Some(index) = functions.clone().into_iter().flatten().max() else {
                continue;
            };
            self.references.push(Reference {
                space: IndexSpace::Function,
                site: ReferenceSite::ElementSegment {
                    index: u32::try_from(segment).unwrap_or(u32::MAX),
                },
                index,
            });
        }
    }

    /// Records one code-section entry's declared widths.
    ///
    /// Read from the body rather than from the validator so that a module the
    /// validator refused still reports what its later functions declare.
    fn observe_body(&mut self, body: &FunctionBody<'_>) {
        let defined = u32::try_from(self.declared.len()).unwrap_or(u32::MAX);
        let index = self.imported_functions.saturating_add(defined);
        let param_words = self
            .function_types
            .get(defined as usize)
            .and_then(|ty| self.type_shapes.get(*ty as usize))
            .map_or(0, |(params, _)| {
                params.iter().map(|ty| u64::from(words_of(*ty))).sum()
            });

        let mut local_words: u64 = 0;
        let mut largest_locals_group = 0;
        if let Ok(locals) = body.get_locals_reader() {
            for (count, ty) in locals.into_iter().flatten() {
                largest_locals_group = largest_locals_group.max(count);
                local_words =
                    local_words.saturating_add(u64::from(count) * u64::from(words_of(ty)));
            }
        }

        self.declared.push(DeclaredWidths {
            index,
            param_words,
            local_words,
            largest_locals_group,
        });
    }

    /// Reads the function-name map out of a `name` section.
    ///
    /// A malformed name section is not a conformance question — no decoder
    /// executes it — so a read that fails leaves the functions unnamed rather
    /// than refusing the module.
    fn observe_names(&mut self, data: &[u8], offset: usize) {
        let reader = NameSectionReader::new(wasmparser::BinaryReader::new(data, offset));
        for subsection in reader.into_iter().flatten() {
            if let Name::Function(map) = subsection {
                for naming in map.into_iter().flatten() {
                    self.names.push((naming.index, naming.name.to_string()));
                }
            }
        }
    }

    /// The function at `index` as a message should name it.
    fn name_of(&self, index: u32) -> String {
        self.names
            .iter()
            .find(|(at, _)| *at == index)
            .map_or_else(|| format!("func[{index}]"), |(_, name)| name.clone())
    }

    /// Every finding, in the order [`check`] documents: the feature verdict,
    /// then the functions, then the imports, then what the sections outside a
    /// body name, then the module's own shape.
    ///
    /// The four groups are separate functions because they are four different
    /// questions asked of four different readings, and because a group that
    /// grows an arm should not push the others past what one screen holds.
    fn violations(&self) -> Vec<Violation> {
        let mut found = Vec::new();
        if let Some(detail) = &self.invalid {
            found.push(Violation::OutsideWasm1 {
                detail: detail.clone(),
            });
        }
        self.function_violations(&mut found);
        self.import_violations(&mut found);
        self.reference_violations(&mut found);
        self.module_shape_violations(&mut found);
        found
    }

    /// What each defined function declares and what its body reached, in index
    /// order.
    fn function_violations(&self, found: &mut Vec<Violation>) {
        for (position, declared) in self.declared.iter().enumerate() {
            let function = self.name_of(declared.index);
            if declared.param_words > MAX_PARAM_WORDS.into() {
                found.push(Violation::ParamWordsExceeded {
                    function: function.clone(),
                    words: declared.param_words,
                });
            }
            if declared.local_words > MAX_LOCAL_WORDS {
                found.push(Violation::LocalWordsExceeded {
                    function: function.clone(),
                    words: declared.local_words,
                });
            }
            if declared.largest_locals_group > MAX_LOCALS_GROUP_COUNT {
                found.push(Violation::LocalsGroupTooLarge {
                    function: function.clone(),
                    count: declared.largest_locals_group,
                });
            }
            // Only a body the validator got through has an operand peak, and
            // without one there is no frame to add up. A module that lost its
            // measurements is being refused for that already.
            if let Some(measured) = self.measured.get(position) {
                let words = u64::from(FRAME_HEADER_WORDS)
                    .saturating_add(declared.local_words)
                    .saturating_add(u64::from(measured.maxima.operand_words));
                if words > MAX_FRAME_WORDS {
                    found.push(Violation::FrameWordsExceeded {
                        function: function.clone(),
                        words,
                        local_words: declared.local_words,
                        operand_words: measured.maxima.operand_words,
                    });
                }
                for (kind, value) in [
                    (IndexKind::CallIndirectType, measured.peaks.call_indirect_type),
                    (IndexKind::BranchTableTargets, measured.peaks.br_table_targets),
                ] {
                    if value > MAX_IR_INDEX {
                        found.push(Violation::IndexTooLarge {
                            function: function.clone(),
                            kind,
                            index: value,
                        });
                    }
                }
                for (space, index) in [
                    (IndexSpace::Function, measured.peaks.called_function),
                    (IndexSpace::Global, measured.peaks.referenced_global),
                ] {
                    let site = ReferenceSite::Body {
                        function: function.clone(),
                    };
                    if let Some(violation) = self.truncation(space, site, index) {
                        found.push(violation);
                    }
                }
                if measured.peaks.branch_unwind_words > MAX_BRANCH_UNWIND_WORDS {
                    found.push(Violation::BranchUnwindTooDeep {
                        function,
                        words: measured.peaks.branch_unwind_words,
                    });
                }
            }
        }
    }

    /// What each import asks of an embedder that no embedder could supply, in
    /// import order.
    fn import_violations(&self, found: &mut Vec<Violation>) {
        for import in &self.imports {
            for (which, name) in [
                (NamePart::Module, &import.module),
                (NamePart::Field, &import.field),
            ] {
                if name.len() > MAX_IMPORT_NAME_BYTES {
                    found.push(Violation::ImportNameTooLong {
                        module: import.module.clone(),
                        field: import.field.clone(),
                        which,
                        name: name.clone(),
                        len: name.len(),
                    });
                }
            }
            if import.params > MAX_HOST_FUNCTION_PARAMS {
                found.push(Violation::ImportArityExceeded {
                    module: import.module.clone(),
                    field: import.field.clone(),
                    params: import.params,
                });
            }
            if import.results > MAX_HOST_FUNCTION_RESULTS {
                found.push(Violation::ImportMultipleResults {
                    module: import.module.clone(),
                    field: import.field.clone(),
                    results: import.results,
                });
            }
        }
    }

    /// What the export section, the `start` section and the element section
    /// name, in that order, which is the order a module writes them in and the
    /// order the walk above recorded them in.
    fn reference_violations(&self, found: &mut Vec<Violation>) {
        for reference in &self.references {
            let site = reference.site.clone();
            if let Some(violation) = self.truncation(reference.space, site, reference.index) {
                found.push(violation);
            }
        }
    }

    /// The finding one reference to `index` earns, or `None` when the
    /// interpreter narrows nothing there.
    ///
    /// `None` in three cases. An index below the imported count names an
    /// import, which resolves to a host or an external reference carrying no
    /// narrowed position. An index past what the module defines resolves to
    /// nothing at all, and stock validation refuses the module for that before
    /// this crate would, so answering here would be a second finding about one
    /// mistake. And a position inside the 16-bit word is the ordinary case: the
    /// cast leaves it alone.
    ///
    /// Only the largest index a site names reaches this, so a site whose
    /// largest index resolves to nothing is silent about a smaller index of its
    /// own that the cast would narrow. The module is refused for the
    /// unresolvable reference either way, and that is the edit to make first.
    fn truncation(&self, space: IndexSpace, site: ReferenceSite, index: u32) -> Option<Violation> {
        let (imported, defined) = match space {
            IndexSpace::Function => (
                self.imported_functions,
                u32::try_from(self.function_types.len()).unwrap_or(u32::MAX),
            ),
            IndexSpace::Global => (self.imported_globals, self.defined_globals),
        };
        let position = index.checked_sub(imported)?;
        if position >= defined || position <= MAX_IR_INDEX {
            return None;
        }
        Some(Violation::IndexTruncated {
            space,
            site,
            index,
            position,
        })
    }

    /// What the module declares outside any function or import.
    fn module_shape_violations(&self, found: &mut Vec<Violation>) {
        for memory in &self.memories {
            let pages = memory.maximum.unwrap_or(memory.initial).max(memory.initial);
            if pages > MAX_MEMORY_PAGES {
                found.push(Violation::MemoryTooLarge { pages });
            }
        }
        for name in &self.custom_section_names {
            if name.len() > MAX_CUSTOM_SECTION_NAME_BYTES {
                found.push(Violation::CustomSectionNameTooLong {
                    name: name.clone(),
                    len: name.len(),
                });
            }
        }
    }

    /// The measurement, once nothing has been found to refuse.
    ///
    /// Only reachable with a clean validation, so `declared` and `measured` are
    /// the same functions in the same order.
    fn report(&self, wasm_size: usize) -> Report {
        let functions: Vec<FunctionMetrics> = self
            .declared
            .iter()
            .zip(&self.measured)
            .map(|(declared, measured)| {
                let local_words = u32::try_from(declared.local_words).unwrap_or(u32::MAX);
                FunctionMetrics {
                    index: declared.index,
                    name: self
                        .names
                        .iter()
                        .find(|(at, _)| *at == declared.index)
                        .map(|(_, name)| name.clone()),
                    param_words: u32::try_from(declared.param_words).unwrap_or(u32::MAX),
                    local_words,
                    max_control_depth: measured.maxima.control_depth,
                    max_operand_values: measured.maxima.operand_values,
                    max_operand_words: measured.maxima.operand_words,
                }
            })
            .collect();

        Report {
            functions,
            wasm_size,
            limits_from: LIMITS_FROM,
        }
    }
}

/// How many 32-bit words one value of `ty` occupies.
///
/// Only the four WebAssembly 1.0 types can appear in a module this crate
/// reports on; the remaining arms exist because the enum has them and answer
/// with the width the value would take.
fn words_of(ty: ValType) -> u32 {
    match ty {
        ValType::I32 | ValType::F32 | ValType::Ref(_) => 1,
        ValType::I64 | ValType::F64 => 2,
        ValType::V128 => 4,
    }
}

/// Validates one function body, sampling the two stack heights after every
/// operator and the interpreter's compiler-side bounds at the operators that
/// meet them.
///
/// The heights are instantaneous readings, so the peak exists only while the
/// body is being walked — which is why this drives the operator loop by hand
/// rather than calling `FuncValidator::validate`. The same is true of a
/// branch's unwind, and more sharply: it is a difference between two heights,
/// only one of which survives the branch.
///
/// Each operator is looked at twice, before validation and after, and the split
/// is the interpreter's own ordering rather than a convenience. A branch's
/// unwind is measured against the stack the branch *sees*, which validating the
/// operator destroys — `br` truncates it. An index is a property of the
/// instruction alone, and reading it after validation means an index outside the
/// module's own range has already been refused as a validation failure, which is
/// the order the interpreter reaches the two in as well.
fn measure(
    to_validate: FuncToValidate<ValidatorResources>,
    body: &FunctionBody<'_>,
) -> Result<Measured, BinaryReaderError> {
    let mut validator = to_validate.into_validator(FuncValidatorAllocations::default());
    let mut locals = body.get_binary_reader();
    validator.read_locals(&mut locals)?;

    let mut measured = Measured::default();
    // The state before the first operator is a sample too: the function-body
    // frame is already on the control stack, so a body of nothing but `end`
    // still reports a depth of one.
    sample(&validator, &mut measured.maxima);

    let mut operators = OperatorsReader::new(body.get_binary_reader_for_operators()?);
    while !operators.eof() {
        let offset = operators.original_position();
        let operator = operators.read()?;
        observe_branch(&operator, &validator, &mut measured.peaks);
        validator.op(offset, &operator)?;
        observe_index(&operator, &mut measured.peaks);
        sample(&validator, &mut measured.maxima);
    }
    let tail = operators.get_binary_reader();
    tail.finish_expression(&validator.visitor(tail.original_position()))?;
    Ok(measured)
}

/// Folds the validator's current heights into the running maxima.
fn sample(validator: &FuncValidator<ValidatorResources>, maxima: &mut Maxima) {
    let values = validator.operand_stack_height();
    maxima.control_depth = maxima.control_depth.max(validator.control_stack_height());
    maxima.operand_values = maxima.operand_values.max(values);
    maxima.operand_words = maxima.operand_words.max(words_below(validator, values, values));
}

/// The words the bottom `count` operands of a stack of `height` values occupy.
///
/// The sum walks from the bottom and stops at the first operand whose type is no
/// longer tracked — every slot after an `unreachable`, until the enclosing block
/// ends. That rule is not a choice: the interpreter's own verifier computes both
/// the frame figure and a branch's unwind this way, so counting an untracked
/// slot at any width would refuse functions the decoder loads.
/// `get_operand_type` indexes from the top, hence the arithmetic.
fn words_below(validator: &FuncValidator<ValidatorResources>, height: u32, count: u32) -> u32 {
    let mut words = 0;
    for from_bottom in 0..count.min(height) {
        // `None` is out of bounds, which the depth never is: it is bounded by
        // the height just read. Stopping on it costs nothing and needs no arm
        // of its own.
        let Some(Some(ty)) = validator.get_operand_type((height - 1 - from_bottom) as usize) else {
            break;
        };
        words += words_of(ty);
    }
    words
}

/// Folds a branching operator's unwind into the running peaks.
///
/// Called with the stack as the branch sees it. `br_table` is one operator and
/// as many branches as it lists, the default one included, each leaving a frame
/// of its own — so each is measured rather than the widest guessed at.
///
/// The wildcard is every operator that is not a branch, which is most of the
/// instruction set. The three arms above it are four of the interpreter's five
/// measured unwinds; the fifth, an `else`'s implicit branch over its own arm,
/// is out of reach of the bound at WebAssembly 1.0 and is not modelled — see
/// [`MAX_BRANCH_UNWIND_WORDS`].
fn observe_branch(
    operator: &Operator<'_>,
    validator: &FuncValidator<ValidatorResources>,
    peaks: &mut LimitPeaks,
) {
    let mut note = |words: Option<u32>| {
        if let Some(words) = words {
            peaks.branch_unwind_words = peaks.branch_unwind_words.max(words);
        }
    };
    match operator {
        Operator::Br { relative_depth } => note(unwind_words(validator, *relative_depth, false)),
        Operator::BrIf { relative_depth } => note(unwind_words(validator, *relative_depth, true)),
        Operator::BrTable { targets } => {
            for target in targets
                .targets()
                .flatten()
                .chain(core::iter::once(targets.default()))
            {
                note(unwind_words(validator, target, true));
            }
        }
        _ => {}
    }
}

/// Folds an operator's 16-bit IR immediates into the running peaks.
///
/// Called after validation, so an operator naming something the module does not
/// declare has already been refused for that instead — which is the order the
/// interpreter meets the two in as well, and what lets every index read here be
/// treated as one that resolves.
///
/// The four arms are the whole set rather than the ones that came to mind: they
/// are the only operators that write a value of the module's own into the
/// interpreter's one 16-bit IR word, and [`MAX_IR_INDEX`]'s doc says of each
/// remaining index kind why it is out of reach. The first two are checked
/// against the word upstream and the last two are narrowed into it without a
/// check, which is the difference between [`Violation::IndexTooLarge`] and
/// [`Violation::IndexTruncated`] and not a difference in what is read.
///
/// The two global operators are one arm because they are one accessor: the
/// interpreter reads both through `TextBuilder::get_global`, so a value either
/// can present is a value the other can.
fn observe_index(operator: &Operator<'_>, peaks: &mut LimitPeaks) {
    match operator {
        Operator::CallIndirect { type_index, .. } => {
            peaks.call_indirect_type = peaks.call_indirect_type.max(*type_index);
        }
        Operator::BrTable { targets } => {
            peaks.br_table_targets = peaks.br_table_targets.max(targets.len());
        }
        Operator::Call { function_index } => {
            peaks.called_function = peaks.called_function.max(*function_index);
        }
        Operator::GlobalGet { global_index } | Operator::GlobalSet { global_index } => {
            peaks.referenced_global = peaks.referenced_global.max(*global_index);
        }
        _ => {}
    }
}

/// The operand words a branch to `relative_depth` would discard, or `None` when
/// the interpreter encodes no unwind for it.
///
/// `None` in two cases. A depth past the control stack is not a branch at all —
/// the validator refuses the module for it, and answering here would be a second
/// finding about one mistake. A branch to the *outermost* frame is the function
/// body's own, which the interpreter compiles as an early return carrying the
/// function's result type and no unwind at all.
///
/// `pops_a_selector` is true for the two operators that take an `i32` off the
/// stack before the unwind is read — `br_if` and `br_table`.
fn unwind_words(
    validator: &FuncValidator<ValidatorResources>,
    relative_depth: u32,
    pops_a_selector: bool,
) -> Option<u32> {
    let frames = validator.control_stack_height();
    if relative_depth >= frames || relative_depth + 1 == frames {
        return None;
    }
    let target = validator.get_control_frame(relative_depth as usize)?;
    let below_target = u32::try_from(target.height).unwrap_or(u32::MAX);

    let height = validator.operand_stack_height();
    let live = if pops_a_selector {
        height_after_selector_pop(validator, height)
    } else {
        height
    };
    Some(
        words_below(validator, height, live)
            .saturating_sub(words_below(validator, height, below_target.min(live))),
    )
}

/// The operand height after a branch's `i32` selector is taken off.
///
/// Not `height - 1`: in unreachable code at the innermost frame's own floor the
/// pop yields a synthesized operand and shrinks nothing. Both this validator and
/// the interpreter's do that, and both truncate the stack to the frame floor
/// when a frame becomes unreachable, which is what makes the two figures
/// comparable at all.
fn height_after_selector_pop(validator: &FuncValidator<ValidatorResources>, height: u32) -> u32 {
    let floor = validator
        .get_control_frame(0)
        .map_or(0, |frame| u32::try_from(frame.height).unwrap_or(u32::MAX));
    if height > floor { height - 1 } else { height }
}

#[cfg(test)]
mod spacewasm_tests {
    use super::{
        FunctionMetrics, REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH, Report,
    };

    /// A function reaching `control_depth` frames and `values` operands, named
    /// so the ranking can be read off a warning.
    fn metrics(index: u32, name: &str, control_depth: u32, values: u32) -> FunctionMetrics {
        FunctionMetrics {
            index,
            name: Some(String::from(name)),
            param_words: 0,
            local_words: 0,
            max_control_depth: control_depth,
            max_operand_values: values,
            max_operand_words: values,
        }
    }

    /// A report over `functions`. The module-wide maxima are the type's own
    /// derivation from them, so a hand-built fixture cannot disagree with
    /// itself.
    fn report(functions: Vec<FunctionMetrics>) -> Report {
        Report {
            functions,
            wasm_size: 0,
            limits_from: super::LIMITS_FROM,
        }
    }

    /// A module inside both reference bounds is owed no warning.
    ///
    /// The boundary is *at* the reference value rather than one below it,
    /// because an embedder built with `MAX_CONTROL_FRAMES = 64` loads a module
    /// needing 64 frames; warning there would tell a reader to change a
    /// configuration that already fits.
    ///
    /// Fails if either comparison is relaxed to `>=`.
    #[test]
    fn a_module_at_exactly_the_reference_bounds_is_owed_no_warning() {
        let at_bound = report(vec![metrics(
            0,
            "fits",
            REFERENCE_MAX_CONTROL_FRAMES,
            REFERENCE_MAX_STACK_DEPTH,
        )]);
        assert!(
            at_bound.budget_warnings().is_empty(),
            "a module that fits the reference embedder exactly needs no warning"
        );
    }

    /// Each axis warns on its own, and says which one it is about.
    ///
    /// Both arms are driven separately because they are two copies of one
    /// shape: a warning that reported the control depth under the stack-depth
    /// heading would read as an ordinary over-budget report, and only a module
    /// over one axis and inside the other can tell them apart.
    ///
    /// Fails if an arm is dropped, if one arm reports the other's number, or if
    /// either warning stops naming the const generic to raise.
    #[test]
    fn each_axis_warns_on_its_own_and_names_its_const_generic() {
        let over_depth = report(vec![metrics(
            0,
            "nested",
            REFERENCE_MAX_CONTROL_FRAMES + 7,
            1,
        )]);
        let warnings = over_depth.budget_warnings();
        assert_eq!(warnings.len(), 1, "only the control axis is over: {warnings:?}");
        assert!(
            warnings[0].contains("MAX_CONTROL_FRAMES of 64")
                && warnings[0].contains("control nesting 71"),
            "the control warning must carry both numbers: {}",
            warnings[0]
        );

        let over_stack = report(vec![metrics(
            0,
            "wide",
            1,
            REFERENCE_MAX_STACK_DEPTH + 44,
        )]);
        let warnings = over_stack.budget_warnings();
        assert_eq!(warnings.len(), 1, "only the stack axis is over: {warnings:?}");
        assert!(
            warnings[0].contains("MAX_STACK_DEPTH of 256")
                && warnings[0].contains("operand stack 300 values"),
            "the stack warning must carry both numbers: {}",
            warnings[0]
        );

        let over_both = report(vec![metrics(
            0,
            "both",
            REFERENCE_MAX_CONTROL_FRAMES + 1,
            REFERENCE_MAX_STACK_DEPTH + 1,
        )]);
        assert_eq!(
            over_both.budget_warnings().len(),
            2,
            "a module over both axes is owed both warnings"
        );
    }

    /// The warning names the three worst functions, worst first.
    ///
    /// Ranking is the whole of what makes it actionable: the remedy is either a
    /// bigger const generic or a flatter function, and a reader picking which
    /// function to flatten needs the deepest one named first.
    ///
    /// Fails if the ranking is reversed, if the cut moves off three, or if a
    /// function that is not among the worst is named.
    #[test]
    fn the_warning_names_the_three_deepest_functions_worst_first() {
        let over = report(vec![
            metrics(0, "shallow", 2, 1),
            metrics(1, "blend", REFERENCE_MAX_CONTROL_FRAMES + 2, 1),
            metrics(2, "render_row", REFERENCE_MAX_CONTROL_FRAMES + 7, 1),
            metrics(3, "mix", REFERENCE_MAX_CONTROL_FRAMES + 1, 1),
        ]);
        let warnings = over.budget_warnings();
        assert_eq!(warnings.len(), 1, "only the control axis is over: {warnings:?}");
        assert!(
            warnings[0].contains("Deepest functions: `render_row` (71), `blend` (66), `mix` (65)."),
            "the three deepest must be named in descending order: {}",
            warnings[0]
        );
        assert!(
            !warnings[0].contains("shallow"),
            "a function outside the worst three must not be named: {}",
            warnings[0]
        );
    }

    /// An unnamed function is still named, by its index.
    ///
    /// A module stripped of its `name` section is the ordinary shape of a
    /// release artifact, and a warning that pointed at nothing would be one a
    /// reader could not act on.
    ///
    /// Fails if the `func[N]` fallback is dropped.
    #[test]
    fn an_unnamed_function_is_ranked_by_its_index() {
        let mut anonymous = metrics(4, "", REFERENCE_MAX_CONTROL_FRAMES + 1, 1);
        anonymous.name = None;
        let over = report(vec![anonymous]);
        assert!(
            over.budget_warnings()[0].contains("`func[4]` (65)"),
            "an unnamed function is named by its index: {:?}",
            over.budget_warnings()
        );
    }

    /// The summary carries both units, all three names and the reference pair.
    ///
    /// Each maximum is attained by a different function, which is what makes
    /// the three name slots distinguishable: a module where one function is
    /// worst on every axis is satisfied by a line that printed that one name
    /// three times, and the axes genuinely come apart — a function keeping many
    /// narrow values live is not the one keeping few wide ones.
    ///
    /// Fails if a unit word is dropped, if a slot renders another axis's
    /// function, if the number an embedder is told to build with stops being
    /// the number measured, or if the reference configuration stops being
    /// named.
    #[test]
    fn the_summary_reports_both_units_and_the_reference_configuration() {
        let mut wide = metrics(2, "wide", 1, 8);
        wide.max_operand_words = 14;
        let line = report(vec![
            metrics(0, "deep", 3, 2),
            metrics(1, "tall", 1, 9),
            wide,
        ])
        .summary_line();
        for fragment in [
            "deepest control nesting 3 in `deep`",
            "tallest operand stack 9 values in `tall`",
            "peak 14 stack words in `wide`",
            "MAX_CONTROL_FRAMES >= 3",
            "MAX_STACK_DEPTH >= 9",
            "spacewasm_std uses 64 and 256",
            "limits from spacewasm 0.7.1",
        ] {
            assert!(line.contains(fragment), "the summary must carry `{fragment}`: {line}");
        }
    }

    /// A module defining no function says so rather than reporting zeroes.
    ///
    /// Zeroes would read as a measurement, and an embedder told to build with
    /// `MAX_CONTROL_FRAMES >= 0` has been told nothing: no function means no
    /// call frame, which is a different statement from a small one.
    ///
    /// Fails if the empty arm starts rendering the general sentence.
    #[test]
    fn a_module_with_no_function_needs_no_budget() {
        let line = report(Vec::new()).summary_line();
        assert!(
            line.contains("defines no function")
                && line.contains("Limits from spacewasm 0.7.1")
                && !line.contains("MAX_CONTROL_FRAMES >="),
            "the empty arm says there is no budget rather than reporting one: {line}"
        );
    }
}

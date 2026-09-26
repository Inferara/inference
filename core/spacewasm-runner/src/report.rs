//! The words a run's ending is reported in: a trap, with the host's own
//! detail when a host stopped the program; an exhausted budget; and a module
//! the decoder refused for want of memory. A run is a call of an export or a
//! module's start function, and the reports of both are worded alike, naming
//! the one that ran.
//!
//! The texts here state what the runner knows and nothing about the program
//! that embeds it. Where a sentence has to name that program, the caller
//! passes the name as it should appear, backticks included, so the sentence is
//! the runner's and the name is the caller's. Everything a caller composes
//! around these texts — which artifact, which command-line flag, which remedy —
//! is the caller's to write.

use inference_target_conformance::spacewasm::check;
use spacewasm::{AllocError, MemoryError, ParseError, TrapReason, ValidationError};

use crate::errors::{Limit, LoadError, OverLimit};
use crate::fprime::ReferenceHost;
use crate::module::REFERENCE_STACK_WORDS;

/// Where the instruction that raises a trap can have come from, in a module a
/// build of this compiler produced and [`crate::fprime::load`] loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapGroup {
    /// The code the compiler generates can raise it.
    Compiled,
    /// Only a linked external module can: the compiler never emits the
    /// instruction that raises it.
    LinkedModule,
    /// No module that load accepts can: raising it takes a `memory.grow` the
    /// code builder refuses, a host memory or an imported global the reference
    /// hosts never supply, or a defect in the interpreter.
    Impossible,
}

/// Which [`TrapGroup`] `reason` belongs to.
#[must_use]
pub fn trap_group(reason: TrapReason) -> TrapGroup {
    match reason {
        TrapReason::Unreachable
        | TrapReason::Host
        | TrapReason::DivideByZero
        | TrapReason::IntegerOverflow
        | TrapReason::StackOverflow
        | TrapReason::MemoryOutOfBounds => TrapGroup::Compiled,
        TrapReason::InvalidTableIndex
        | TrapReason::InvalidTableFunctionType
        | TrapReason::UninitializedTableElement
        | TrapReason::UnrepresentableResult
        | TrapReason::BadConversionToInteger => TrapGroup::LinkedModule,
        TrapReason::OutOfMemory
        | TrapReason::MemoryRefNotUnique
        | TrapReason::GlobalGetFailed
        | TrapReason::GlobalSetFailed
        | TrapReason::MalformedIr => TrapGroup::Impossible,
    }
}

/// What `reason` means, in the words a trap's first line gives it.
#[must_use]
pub fn trap_phrase(reason: TrapReason) -> &'static str {
    match reason {
        TrapReason::Unreachable => "a runtime check failed",
        TrapReason::Host => "a host function stopped the call",
        TrapReason::DivideByZero => "division by zero",
        TrapReason::IntegerOverflow => "signed division overflow",
        TrapReason::StackOverflow => "the interpreter's call stack is full",
        TrapReason::MemoryOutOfBounds => "a memory access fell outside linear memory",
        TrapReason::InvalidTableIndex => "an indirect call used an index outside the table",
        TrapReason::InvalidTableFunctionType => {
            "an indirect call's signature did not match the function"
        }
        TrapReason::UninitializedTableElement => "an indirect call reached an empty table slot",
        TrapReason::UnrepresentableResult => "a float-to-integer conversion overflowed",
        TrapReason::BadConversionToInteger => "a float-to-integer conversion of NaN",
        TrapReason::OutOfMemory => "`memory.grow` failed",
        TrapReason::MemoryRefNotUnique => "`memory.grow` failed because a host holds the memory",
        TrapReason::GlobalGetFailed => "an imported global could not be read",
        TrapReason::GlobalSetFailed => "an imported global could not be written",
        TrapReason::MalformedIr => {
            "the interpreter met an instruction of its own compiled form it cannot decode"
        }
    }
}

/// Why an F´ reference host stopped the program, recorded by the host before
/// it trapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostTrap {
    /// `panic` was called, logged its `PANIC` line, and stopped the program,
    /// as it always does.
    Panic {
        /// The host called.
        host: &'static ReferenceHost,
    },
    /// A host was handed a buffer its memory does not hold.
    OutOfBounds {
        /// The host called.
        host: &'static ReferenceHost,
        /// Where the buffer starts, read unsigned.
        address: u32,
        /// How many bytes it was said to hold, read unsigned.
        len: u32,
        /// How many bytes the module's linear memory holds.
        memory_bytes: usize,
    },
    /// A host that prints text was handed bytes that are not UTF-8.
    NotUtf8 {
        /// The host called.
        host: &'static ReferenceHost,
        /// The offset of the first byte that is not part of valid UTF-8.
        first_invalid: usize,
        /// How many bytes it was handed.
        len: u32,
    },
    /// `telemetry` was told its time buffer is shorter than the eleven bytes
    /// it writes.
    ShortTime {
        /// The host called.
        host: &'static ReferenceHost,
        /// The length it was told.
        time_len: i32,
    },
    /// `telemetry`'s eleven-byte time runs past the end of linear memory. The
    /// fields that fit were written before the trap, as the reference embedder
    /// writes them.
    TimeOutOfBounds {
        /// The host called.
        host: &'static ReferenceHost,
        /// Where the time starts, read unsigned.
        address: u32,
        /// How many bytes the module's linear memory holds.
        memory_bytes: usize,
    },
}

impl HostTrap {
    /// What happened, in the words a trap's first line gives it.
    fn fact(self) -> String {
        match self {
            HostTrap::Panic { host } => format!(
                "it called `{}`, which always stops the program; its message is the PANIC line \
                 above",
                host.name()
            ),
            HostTrap::OutOfBounds { host, address, len, memory_bytes } => format!(
                "`{}` was given {} at address {address}, outside the module's {memory_bytes}-byte \
                 linear memory",
                host.name(),
                bytes(len)
            ),
            HostTrap::NotUtf8 { host, first_invalid, len } => format!(
                "`{}` was given bytes that are not UTF-8 (the first invalid byte is at offset \
                 {first_invalid} of {len})",
                host.name()
            ),
            HostTrap::ShortTime { host, time_len } => format!(
                "`{}` writes an 11-byte F Prime time, and `time_len` is {time_len}",
                host.name()
            ),
            HostTrap::TimeOutOfBounds { host, address, memory_bytes } => format!(
                "`{}` writes an 11-byte F Prime time at address {address}, which runs past the \
                 end of the module's {memory_bytes}-byte linear memory",
                host.name()
            ),
        }
    }

    /// What to do about it, when there is something to say.
    fn explanation(self) -> Option<&'static str> {
        match self {
            HostTrap::Panic { .. } => None,
            HostTrap::OutOfBounds { .. } => Some(
                "The host reads `len` bytes from the address it is given; check the length you \
                 pass with the array.",
            ),
            HostTrap::NotUtf8 { .. } => Some("The reference host prints text only."),
            HostTrap::ShortTime { .. } => {
                Some("Declare the parameter `mut time: [u8; 11]` and pass 11.")
            }
            HostTrap::TimeOutOfBounds { .. } => Some(
                "The host writes the time at the address it is given; pass it an array declared \
                 `mut time: [u8; 11]`.",
            ),
        }
    }
}

/// How the reports name a module's start function, where a call's names its
/// export.
const START_FUNCTION: &str = "the module's start function";

/// What a start function's trap is headlined with when a host stopped it
/// without recording why: [`trap_phrase`] says so of a call.
const STOPPED_BY_A_HOST: &str = "a host function stopped it";

/// The explanation of a start function's trap for every reason but an
/// exhausted stack and the reasons no module the loader accepts can raise.
/// It is the one thing about such code that stays true of every module: the
/// explanations of a call's trap describe what this compiler emits.
const NOT_FROM_THIS_COMPILER: &str =
    "Inference never emits a start function, so the code that trapped did not come from this \
     compiler.";

/// The function a report is about: an export a caller invoked, or the
/// module's start function, which runs before any export can be.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ran {
    Export(String),
    StartFunction,
}

impl Ran {
    /// How a sentence names it: `` `main` `` or "the module's start function".
    fn subject(&self) -> String {
        match self {
            Ran::Export(entry) => format!("`{entry}`"),
            Ran::StartFunction => START_FUNCTION.to_string(),
        }
    }
}

/// A call or a start function that trapped, as it is reported: a first line
/// saying which function trapped and why, and one line explaining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapReport {
    ran: Ran,
    reason: TrapReason,
    host: Option<HostTrap>,
    stack_words: usize,
}

impl TrapReport {
    /// The report of a call to `entry` that trapped for `reason`, in an engine
    /// of `stack_words` words of value stack. `host` is the detail a host
    /// recorded, which is read only when the reason is `Host`.
    #[must_use]
    pub fn new(
        entry: &str,
        reason: TrapReason,
        host: Option<HostTrap>,
        stack_words: usize,
    ) -> Self {
        Self::of(Ran::Export(entry.to_string()), reason, host, stack_words)
    }

    /// The report of a module's start function that trapped for `reason`,
    /// whose arguments are read as [`TrapReport::new`] reads a call's. Its
    /// first line names the start function, and its explanation says only
    /// what stays true of code this compiler never emits; see
    /// [`TrapReport::explanation`].
    #[must_use]
    pub fn start_function(reason: TrapReason, host: Option<HostTrap>, stack_words: usize) -> Self {
        Self::of(Ran::StartFunction, reason, host, stack_words)
    }

    /// The report of `ran` trapping for `reason`, keeping `host` only for a
    /// host's trap.
    fn of(ran: Ran, reason: TrapReason, host: Option<HostTrap>, stack_words: usize) -> Self {
        let host = if reason == TrapReason::Host { host } else { None };
        Self { ran, reason, host, stack_words }
    }

    /// The first line: `` `main` trapped: a runtime check failed (Unreachable). ``,
    /// or, when a host recorded why it stopped the program, that reason in its
    /// place, still ending in `(Host).` A start function's names it: `the
    /// module's start function trapped: …`, and a host that stopped one without
    /// recording why is said to have stopped it, since it stopped no call.
    #[must_use]
    pub fn headline(&self) -> String {
        let fact = match (self.host, &self.ran) {
            (Some(detail), _) => detail.fact(),
            (None, Ran::StartFunction) if self.reason == TrapReason::Host => {
                STOPPED_BY_A_HOST.to_string()
            }
            (None, Ran::Export(_) | Ran::StartFunction) => trap_phrase(self.reason).to_string(),
        };
        format!("{} trapped: {fact} ({:?}).", self.ran.subject(), self.reason)
    }

    /// The line after it, when the reason has one.
    ///
    /// `embedder` names the program that loaded the module, as the two
    /// explanations that are about it begin: the stack it gave the
    /// interpreter, and a trap no module it loads can raise.
    ///
    /// A call's trap is explained by what this compiler emits, since the
    /// function called is one it compiled. A start function's is not: this
    /// compiler never emits one, so of every reason but those two it says only
    /// that, and of an exhausted stack only the words the interpreter was
    /// given, which the start function's call chain needs more of.
    #[must_use]
    pub fn explanation(&self, embedder: &str) -> Option<String> {
        match self.ran {
            Ran::Export(_) => self.call_explanation(embedder),
            Ran::StartFunction => Some(self.start_function_explanation(embedder)),
        }
    }

    /// [`TrapReport::explanation`] of a start function's trap.
    fn start_function_explanation(&self, embedder: &str) -> String {
        match self.reason {
            TrapReason::StackOverflow => format!(
                "{}, and the start function's call chain needs more.",
                self.stack_given(embedder)
            ),
            TrapReason::OutOfMemory
            | TrapReason::MemoryRefNotUnique
            | TrapReason::GlobalGetFailed
            | TrapReason::GlobalSetFailed
            | TrapReason::MalformedIr => defect(embedder),
            TrapReason::Unreachable
            | TrapReason::Host
            | TrapReason::DivideByZero
            | TrapReason::IntegerOverflow
            | TrapReason::MemoryOutOfBounds
            | TrapReason::InvalidTableIndex
            | TrapReason::InvalidTableFunctionType
            | TrapReason::UninitializedTableElement
            | TrapReason::UnrepresentableResult
            | TrapReason::BadConversionToInteger => NOT_FROM_THIS_COMPILER.to_string(),
        }
    }

    /// The opening of the explanation of an exhausted stack: "`infs run`
    /// gives the interpreter 1024 words of call stack", naming `embedder` and
    /// the words the module was loaded with, and saying so when they are the
    /// reference embedder's.
    fn stack_given(&self, embedder: &str) -> String {
        let reference = if self.stack_words == REFERENCE_STACK_WORDS {
            ", as spacewasm_std does"
        } else {
            ""
        };
        let words = if self.stack_words == 1 { "word" } else { "words" };
        format!(
            "{embedder} gives the interpreter {} {words} of call stack{reference}",
            self.stack_words
        )
    }

    /// [`TrapReport::explanation`] of a call's trap.
    fn call_explanation(&self, embedder: &str) -> Option<String> {
        let text = match self.reason {
            TrapReason::Unreachable => {
                "Inference compiles each of its runtime checks to a WebAssembly `unreachable`, \
                 so the interpreter cannot say which one failed: an arithmetic overflow (`+`, \
                 `-`, `*` or unary `-` outside `wrapping(...)`), an array index out of bounds, \
                 `MIN / -1` at `i8` or `i16`, a failed `assert`, or an out-of-range enum value \
                 passed to an exported function."
                    .to_string()
            }
            TrapReason::Host => {
                return self.host.and_then(HostTrap::explanation).map(str::to_string);
            }
            TrapReason::DivideByZero => {
                "A `/` or `%` was evaluated with a divisor of 0.".to_string()
            }
            TrapReason::IntegerOverflow => {
                "A signed `/` divided the type's minimum by -1, a quotient an `i32` or `i64` \
                 cannot hold."
                    .to_string()
            }
            TrapReason::StackOverflow => format!(
                "{}, and this call chain's frames need more. Recursion is refused at compile \
                 time (A035), so a deep chain of large frames is the cause.",
                self.stack_given(embedder)
            ),
            TrapReason::MemoryOutOfBounds => {
                "Code the compiler generates keeps its own accesses in bounds, so the address \
                 came from outside it: a command-line argument passed to an array or struct \
                 parameter (which receives an address), or a linked external module."
                    .to_string()
            }
            TrapReason::InvalidTableIndex
            | TrapReason::InvalidTableFunctionType
            | TrapReason::UninitializedTableElement
            | TrapReason::UnrepresentableResult
            | TrapReason::BadConversionToInteger => {
                "Inference never emits the instruction that raises this, so it came from a \
                 linked external module."
                    .to_string()
            }
            TrapReason::OutOfMemory
            | TrapReason::MemoryRefNotUnique
            | TrapReason::GlobalGetFailed
            | TrapReason::GlobalSetFailed
            | TrapReason::MalformedIr => defect(embedder),
        };
        Some(text)
    }
}

/// The explanation of a trap no module `embedder` loads can raise.
fn defect(embedder: &str) -> String {
    format!(
        "{embedder} loads no module that can raise this, so this is a defect in {embedder} or \
         the interpreter; please report it at https://github.com/Inferara/inference/issues with \
         the artifact."
    )
}

/// The core of the report of a call to `entry` that ran out of fuel after
/// `budget` instructions:
///
/// ```text
/// `main` ran out of fuel: the SpaceWasm interpreter stopped it after 7 interpreter instructions
/// ```
///
/// A caller continues the sentence: with the flag that set the budget, with
/// what to do about it, and with the host calls it logged before it stopped.
#[must_use]
pub fn out_of_fuel(entry: &str, budget: usize) -> String {
    ran_out_of_fuel(&Ran::Export(entry.to_string()), budget)
}

/// [`out_of_fuel`] for a module's start function: the same sentence, opening
/// "the module's start function ran out of fuel" where a call's opens with the
/// export's name.
#[must_use]
pub fn start_out_of_fuel(budget: usize) -> String {
    ran_out_of_fuel(&Ran::StartFunction, budget)
}

/// The out-of-fuel sentence's core, naming what ran.
fn ran_out_of_fuel(ran: &Ran, budget: usize) -> String {
    let instructions = if budget == 1 { "instruction" } else { "instructions" };
    format!(
        "{} ran out of fuel: the SpaceWasm interpreter stopped it after {budget} interpreter \
         {instructions}",
        ran.subject()
    )
}

/// The load error the decoder's `verdict` on `wasm` amounts to, for a module
/// loaded with a verifier of `control_frames` frames and `stack_depth` operand
/// values and a code builder of `code_pages` IR pages.
///
/// The target's conformance check is run again on the same bytes, because the
/// decoder's verdict cannot say two things the check can. Refused for want of
/// memory, `AllocError(OutOfMemory)`, a module is one of three things, and the
/// check's measurements tell them apart: too deep a control nesting, too tall
/// an operand stack, or — neither of those being over — too much compiled IR
/// for the code pages. Refused for anything else, a module the check accepts
/// is a gap in the check — a data segment outside linear memory or at a
/// negative offset among them — except for the verdicts the check leaves to
/// others, which keep the decoder's plain verdict: the host set's; an
/// allocation that failed; a `memory.grow`, which the code builder refuses
/// because the load tells it to, as `spacewasm_std` tells its own, and which
/// the check leaves to the embedder for that reason; and the three about the
/// interpreter's compiled form of the module, which the check cannot reproduce
/// without being the interpreter. A module the check refuses keeps the
/// decoder's plain verdict too, since the check has already reported it.
pub(crate) fn explain_refusal(
    wasm: &[u8],
    verdict: ParseError,
    control_frames: u32,
    stack_depth: u32,
    code_pages: usize,
) -> LoadError {
    let Ok(report) = check(wasm) else {
        return LoadError::Decode(verdict);
    };
    match &verdict.err.err {
        ValidationError::AllocError(AllocError::OutOfMemory) => {
            let over = |limit: Limit, (function, needed): (String, u32), allowed: u32| {
                (needed > allowed).then(|| OverLimit {
                    limit,
                    function,
                    needed,
                    allowed,
                    verdict: verdict.clone(),
                })
            };
            if let Some(over) = over(Limit::ControlFrames, report.deepest(), control_frames)
                .or_else(|| over(Limit::OperandStack, report.tallest(), stack_depth))
            {
                return LoadError::OverLimit(over);
            }
            LoadError::CodePagesExhausted { pages: code_pages, verdict }
        }
        ValidationError::FunctionImportNotFound
        | ValidationError::GlobalImportNotFound
        | ValidationError::MemoryImportNotFound
        | ValidationError::TableImportNotFound
        | ValidationError::FunctionImportOutOfRange
        | ValidationError::FunctionImportTypeMismatch
        | ValidationError::GlobalImportTypeMismatch
        | ValidationError::MemoryImportTypeMismatch
        | ValidationError::TableImportTypeMismatch
        | ValidationError::TableImportIncompatibleSize
        | ValidationError::MemoryImportTooLarge
        | ValidationError::TableRefNotUnique
        | ValidationError::DuplicateModuleName
        | ValidationError::InvalidHostStartFunction
        | ValidationError::GuestMemoryAllocationFailure
        | ValidationError::MemoryError(
            MemoryError::OutOfMemory | MemoryError::AllocationFailed | MemoryError::PageTooSmall,
        )
        | ValidationError::AllocError(_)
        | ValidationError::IllegalMemoryGrow
        | ValidationError::LabelJumpTooLarge
        | ValidationError::PageFault
        | ValidationError::PossibleBackpatchCycle => LoadError::Decode(verdict),
        _ => LoadError::ConformanceGap(verdict),
    }
}

/// "1 byte" or "5 bytes".
fn bytes(count: u32) -> String {
    if count == 1 { "1 byte".to_string() } else { format!("{count} bytes") }
}

#[cfg(test)]
mod tests {
    use spacewasm::SectionDecodeError;

    use super::*;
    use crate::errors::{code_pages_exhausted, conformance_gap};
    use crate::fprime::REFERENCE_HOSTS;

    /// Every trap reason the interpreter has, each with the phrase and the
    /// group a report gives it.
    ///
    /// Spelled as a match with no wildcard, so a reason upstream adds is a
    /// compile error here until it has a row, rather than a reason no row
    /// reports.
    fn row(reason: TrapReason) -> (&'static str, TrapGroup) {
        // A reason given a row here is added to `EVERY_REASON` in the same change.
        match reason {
            TrapReason::Unreachable => ("a runtime check failed", TrapGroup::Compiled),
            TrapReason::Host => ("a host function stopped the call", TrapGroup::Compiled),
            TrapReason::DivideByZero => ("division by zero", TrapGroup::Compiled),
            TrapReason::IntegerOverflow => ("signed division overflow", TrapGroup::Compiled),
            TrapReason::StackOverflow => {
                ("the interpreter's call stack is full", TrapGroup::Compiled)
            }
            TrapReason::MemoryOutOfBounds => {
                ("a memory access fell outside linear memory", TrapGroup::Compiled)
            }
            TrapReason::InvalidTableIndex => {
                ("an indirect call used an index outside the table", TrapGroup::LinkedModule)
            }
            TrapReason::InvalidTableFunctionType => (
                "an indirect call's signature did not match the function",
                TrapGroup::LinkedModule,
            ),
            TrapReason::UninitializedTableElement => {
                ("an indirect call reached an empty table slot", TrapGroup::LinkedModule)
            }
            TrapReason::UnrepresentableResult => {
                ("a float-to-integer conversion overflowed", TrapGroup::LinkedModule)
            }
            TrapReason::BadConversionToInteger => {
                ("a float-to-integer conversion of NaN", TrapGroup::LinkedModule)
            }
            TrapReason::OutOfMemory => ("`memory.grow` failed", TrapGroup::Impossible),
            TrapReason::MemoryRefNotUnique => {
                ("`memory.grow` failed because a host holds the memory", TrapGroup::Impossible)
            }
            TrapReason::GlobalGetFailed => {
                ("an imported global could not be read", TrapGroup::Impossible)
            }
            TrapReason::GlobalSetFailed => {
                ("an imported global could not be written", TrapGroup::Impossible)
            }
            TrapReason::MalformedIr => (
                "the interpreter met an instruction of its own compiled form it cannot decode",
                TrapGroup::Impossible,
            ),
        }
    }

    /// Every reason, in the interpreter's declaration order.
    ///
    /// The list is extended by hand: [`row`]'s exhaustive match makes a reason
    /// upstream adds a compile error there, and nothing ties this list to it,
    /// so the change giving the reason a row adds it here too. The dedup check
    /// in `every_reason_has_its_phrase_and_group` proves the sixteen entries
    /// distinct.
    const EVERY_REASON: [TrapReason; 16] = [
        TrapReason::Unreachable,
        TrapReason::Host,
        TrapReason::DivideByZero,
        TrapReason::InvalidTableIndex,
        TrapReason::InvalidTableFunctionType,
        TrapReason::UninitializedTableElement,
        TrapReason::GlobalGetFailed,
        TrapReason::GlobalSetFailed,
        TrapReason::OutOfMemory,
        TrapReason::MemoryRefNotUnique,
        TrapReason::MemoryOutOfBounds,
        TrapReason::StackOverflow,
        TrapReason::UnrepresentableResult,
        TrapReason::IntegerOverflow,
        TrapReason::BadConversionToInteger,
        TrapReason::MalformedIr,
    ];

    /// The report of `reason` in a call to `main` with the reference stack.
    fn report(reason: TrapReason) -> TrapReport {
        TrapReport::new("main", reason, None, REFERENCE_STACK_WORDS)
    }

    /// Every reason has its own phrase and its group, and the first line
    /// names the entry, the phrase and the reason.
    ///
    /// Fails if a phrase or a group drifts from the table, if two reasons
    /// share a phrase, or if the first line loses a part.
    #[test]
    fn every_reason_has_its_phrase_and_group() {
        let mut phrases: Vec<&str> = Vec::new();
        for reason in EVERY_REASON {
            let (phrase, group) = row(reason);
            assert_eq!(trap_phrase(reason), phrase, "{reason:?}");
            assert_eq!(trap_group(reason), group, "{reason:?}");
            assert_eq!(
                report(reason).headline(),
                format!("`main` trapped: {phrase} ({reason:?}).")
            );
            phrases.push(phrase);
        }
        phrases.sort_unstable();
        phrases.dedup();
        assert_eq!(phrases.len(), EVERY_REASON.len(), "two reasons share a phrase");
    }

    /// The reasons compiled code can raise are each explained in their own
    /// words.
    ///
    /// Fails if a sentence drifts, or if the stack explanation stops taking
    /// the number of words from the load, or says `spacewasm_std` gives that
    /// many when it gives another.
    #[test]
    fn each_reason_compiled_code_raises_is_explained_in_its_own_words() {
        let explained = |reason, stack_words| {
            TrapReport::new("main", reason, None, stack_words).explanation("`embedder`")
        };
        assert_eq!(
            explained(TrapReason::Unreachable, REFERENCE_STACK_WORDS).as_deref(),
            Some(
                "Inference compiles each of its runtime checks to a WebAssembly `unreachable`, so \
                 the interpreter cannot say which one failed: an arithmetic overflow (`+`, `-`, \
                 `*` or unary `-` outside `wrapping(...)`), an array index out of bounds, `MIN / \
                 -1` at `i8` or `i16`, a failed `assert`, or an out-of-range enum value passed to \
                 an exported function."
            )
        );
        assert_eq!(
            explained(TrapReason::DivideByZero, REFERENCE_STACK_WORDS).as_deref(),
            Some("A `/` or `%` was evaluated with a divisor of 0.")
        );
        assert_eq!(
            explained(TrapReason::IntegerOverflow, REFERENCE_STACK_WORDS).as_deref(),
            Some(
                "A signed `/` divided the type's minimum by -1, a quotient an `i32` or `i64` \
                 cannot hold."
            )
        );
        assert_eq!(
            explained(TrapReason::MemoryOutOfBounds, REFERENCE_STACK_WORDS).as_deref(),
            Some(
                "Code the compiler generates keeps its own accesses in bounds, so the address \
                 came from outside it: a command-line argument passed to an array or struct \
                 parameter (which receives an address), or a linked external module."
            )
        );
        for (stack_words, given) in [
            (1024, "1024 words of call stack, as spacewasm_std does,"),
            (65_536, "65536 words of call stack,"),
            (1, "1 word of call stack,"),
        ] {
            assert_eq!(
                explained(TrapReason::StackOverflow, stack_words),
                Some(format!(
                    "`embedder` gives the interpreter {given} and this call chain's frames need \
                     more. Recursion is refused at compile time (A035), so a deep chain of large \
                     frames is the cause."
                ))
            );
        }
        assert_eq!(explained(TrapReason::Host, REFERENCE_STACK_WORDS), None);
    }

    /// Every reason only a linked module raises shares one explanation, and
    /// every reason no loaded module can raise another, naming the embedder.
    ///
    /// Fails if a reason's explanation and its group disagree.
    #[test]
    fn the_two_groups_outside_compiled_code_share_their_explanations() {
        for reason in EVERY_REASON {
            let explanation = report(reason).explanation("`embedder`");
            match trap_group(reason) {
                TrapGroup::Compiled => {}
                TrapGroup::LinkedModule => assert_eq!(
                    explanation.as_deref(),
                    Some(
                        "Inference never emits the instruction that raises this, so it came \
                         from a linked external module."
                    ),
                    "{reason:?}"
                ),
                TrapGroup::Impossible => assert_eq!(
                    explanation.as_deref(),
                    Some(
                        "`embedder` loads no module that can raise this, so this is a defect in \
                         `embedder` or the interpreter; please report it at \
                         https://github.com/Inferara/inference/issues with the artifact."
                    ),
                    "{reason:?}"
                ),
            }
        }
    }

    /// The reference host registered as `field`.
    fn host(field: &str) -> &'static ReferenceHost {
        REFERENCE_HOSTS.iter().find(|host| host.field == field).expect("a reference host")
    }

    /// A trap a host recorded is reported by what the host recorded, still
    /// as a `Host` trap, with the remedy it has.
    ///
    /// Fails if a detail's words, numbers or remedy drift, if a count of one
    /// byte is spelled as a plural, or if a detail reaches the report of a
    /// trap that was not the host's.
    #[test]
    fn a_hosts_trap_is_reported_by_what_the_host_recorded() {
        let cases = [
            (
                HostTrap::Panic { host: host("panic") },
                "`main` trapped: it called `fprime_core.panic`, which always stops the program; \
                 its message is the PANIC line above (Host).",
                None,
            ),
            (
                HostTrap::OutOfBounds {
                    host: host("message"),
                    address: 70_000,
                    len: 5,
                    memory_bytes: 65_536,
                },
                "`main` trapped: `fprime_core.message` was given 5 bytes at address 70000, \
                 outside the module's 65536-byte linear memory (Host).",
                Some(
                    "The host reads `len` bytes from the address it is given; check the length \
                     you pass with the array.",
                ),
            ),
            (
                HostTrap::OutOfBounds {
                    host: host("panic"),
                    address: 4_294_967_295,
                    len: 1,
                    memory_bytes: 0,
                },
                "`main` trapped: `fprime_core.panic` was given 1 byte at address 4294967295, \
                 outside the module's 0-byte linear memory (Host).",
                Some(
                    "The host reads `len` bytes from the address it is given; check the length \
                     you pass with the array.",
                ),
            ),
            (
                HostTrap::NotUtf8 { host: host("message"), first_invalid: 3, len: 5 },
                "`main` trapped: `fprime_core.message` was given bytes that are not UTF-8 (the \
                 first invalid byte is at offset 3 of 5) (Host).",
                Some("The reference host prints text only."),
            ),
            (
                HostTrap::ShortTime { host: host("telemetry"), time_len: 8 },
                "`main` trapped: `fprime_core.telemetry` writes an 11-byte F Prime time, and \
                 `time_len` is 8 (Host).",
                Some("Declare the parameter `mut time: [u8; 11]` and pass 11."),
            ),
            (
                HostTrap::TimeOutOfBounds {
                    host: host("telemetry"),
                    address: 65_530,
                    memory_bytes: 65_536,
                },
                "`main` trapped: `fprime_core.telemetry` writes an 11-byte F Prime time at \
                 address 65530, which runs past the end of the module's 65536-byte linear \
                 memory (Host).",
                Some(
                    "The host writes the time at the address it is given; pass it an array \
                     declared `mut time: [u8; 11]`.",
                ),
            ),
        ];
        for (detail, headline, explanation) in cases {
            let hosts = TrapReport::new("main", TrapReason::Host, Some(detail), 1024);
            assert_eq!(hosts.headline(), headline);
            assert_eq!(hosts.explanation("`embedder`").as_deref(), explanation);
            let unrelated = TrapReport::new("main", TrapReason::Unreachable, Some(detail), 1024);
            assert_eq!(unrelated, report(TrapReason::Unreachable), "{detail:?}");
        }
        assert_eq!(
            report(TrapReason::Host).headline(),
            "`main` trapped: a host function stopped the call (Host)."
        );
    }

    /// What a start function's report says of `reason` with no host detail,
    /// in the reference stack, with `` `embedder` `` as the embedder: the
    /// phrase its first line gives, and its explanation.
    ///
    /// Spelled as a match with no wildcard, as [`row`] is, so a reason
    /// upstream adds has to be given its start function's words here.
    fn start_row(reason: TrapReason) -> (&'static str, &'static str) {
        let not_ours = "Inference never emits a start function, so the code that trapped did not \
                        come from this compiler.";
        let defect = "`embedder` loads no module that can raise this, so this is a defect in \
                      `embedder` or the interpreter; please report it at \
                      https://github.com/Inferara/inference/issues with the artifact.";
        match reason {
            TrapReason::Host => ("a host function stopped it", not_ours),
            TrapReason::StackOverflow => (
                row(reason).0,
                "`embedder` gives the interpreter 1024 words of call stack, as spacewasm_std \
                 does, and the start function's call chain needs more.",
            ),
            TrapReason::OutOfMemory
            | TrapReason::MemoryRefNotUnique
            | TrapReason::GlobalGetFailed
            | TrapReason::GlobalSetFailed
            | TrapReason::MalformedIr => (row(reason).0, defect),
            TrapReason::Unreachable
            | TrapReason::DivideByZero
            | TrapReason::IntegerOverflow
            | TrapReason::MemoryOutOfBounds
            | TrapReason::InvalidTableIndex
            | TrapReason::InvalidTableFunctionType
            | TrapReason::UninitializedTableElement
            | TrapReason::UnrepresentableResult
            | TrapReason::BadConversionToInteger => (row(reason).0, not_ours),
        }
    }

    /// A start function's report names the start function where a call's
    /// names its export, and explains only what stays true of code this
    /// compiler never emits: the stack it was given when it ran out, the
    /// defect sentence for a reason no loaded module can raise, and for every
    /// other reason that the code did not come from this compiler.
    ///
    /// Fails if the start function is named as an export would be, in
    /// backticks, if its explanation borrows a call's — which describes what
    /// this compiler emits, a host's remedy included — if a host that stopped
    /// it without recording why is said to have stopped a call, or if a
    /// detail reaches the report of a trap that was not the host's.
    #[test]
    fn a_start_functions_report_names_it_and_explains_only_what_stays_true() {
        for reason in EVERY_REASON {
            let start = TrapReport::start_function(reason, None, REFERENCE_STACK_WORDS);
            let (phrase, explanation) = start_row(reason);
            assert_eq!(
                start.headline(),
                format!("the module's start function trapped: {phrase} ({reason:?}).")
            );
            let explained = start.explanation("`embedder`");
            assert_eq!(explained.as_deref(), Some(explanation), "{reason:?}");
            assert_ne!(start, report(reason), "{reason:?}");
        }
        assert_eq!(
            report(TrapReason::Host).headline(),
            "`main` trapped: a host function stopped the call (Host).",
            "a call's words stay a call's"
        );

        let panic = HostTrap::Panic { host: host("panic") };
        let stopped = TrapReport::start_function(TrapReason::Host, Some(panic), 1024);
        assert_eq!(
            stopped.headline(),
            "the module's start function trapped: it called `fprime_core.panic`, which always \
             stops the program; its message is the PANIC line above (Host)."
        );
        let short = HostTrap::ShortTime { host: host("telemetry"), time_len: 8 };
        assert_eq!(
            TrapReport::start_function(TrapReason::Host, Some(short), 1024)
                .explanation("`embedder`")
                .as_deref(),
            Some(start_row(TrapReason::Host).1),
            "a host's remedy is about Inference declarations"
        );
        assert_eq!(
            TrapReport::start_function(TrapReason::Unreachable, Some(panic), 1024),
            TrapReport::start_function(TrapReason::Unreachable, None, 1024)
        );
        for (stack_words, given) in [(512, "512 words of"), (1, "1 word of")] {
            assert_eq!(
                TrapReport::start_function(TrapReason::StackOverflow, None, stack_words)
                    .explanation("`embedder`"),
                Some(format!(
                    "`embedder` gives the interpreter {given} call stack, and the start \
                     function's call chain needs more."
                ))
            );
        }
    }

    /// The out-of-fuel sentence names the entry and the budget, in the
    /// grammar the count takes.
    #[test]
    fn running_out_of_fuel_names_the_entry_and_the_budget() {
        assert_eq!(
            out_of_fuel("main", 1_000_000),
            "`main` ran out of fuel: the SpaceWasm interpreter stopped it after 1000000 \
             interpreter instructions"
        );
        assert_eq!(
            out_of_fuel("spin", 1),
            "`spin` ran out of fuel: the SpaceWasm interpreter stopped it after 1 interpreter \
             instruction"
        );
    }

    /// A start function's out-of-fuel sentence names the start function, not
    /// an export, and the budget in the grammar the count takes.
    #[test]
    fn a_start_function_running_out_of_fuel_names_the_start_function() {
        assert_eq!(
            start_out_of_fuel(7),
            "the module's start function ran out of fuel: the SpaceWasm interpreter stopped it \
             after 7 interpreter instructions"
        );
        assert_eq!(
            start_out_of_fuel(1),
            "the module's start function ran out of fuel: the SpaceWasm interpreter stopped it \
             after 1 interpreter instruction"
        );
    }

    /// A decoder verdict of `error`, at byte 42.
    fn verdict(error: ValidationError) -> ParseError {
        ParseError::new(42, SectionDecodeError::new(error))
    }

    /// A module the conformance check accepts: one small function.
    fn conformant() -> Vec<u8> {
        wat::parse_str("(module (func (export \"f\") (result i32) i32.const 1))")
            .expect("valid WAT")
    }

    /// Past the three causes of `AllocError(OutOfMemory)`, a verdict about a
    /// module the check accepts is a gap in the check — unless the check
    /// leaves the verdict to others, or refuses the module itself.
    ///
    /// Fails if a verdict the check leaves to others is blamed on it — the
    /// host set's, an allocation that failed, the `memory.grow` the load tells
    /// the code builder to refuse, and the three about the interpreter's
    /// compiled form — if one it should have made is not, a data segment
    /// outside memory included, or if a module the check refuses is explained
    /// at all.
    #[test]
    fn a_refusal_the_check_should_have_made_is_reported_as_a_gap_in_it() {
        for error in [
            ValidationError::TypeMismatch,
            ValidationError::StackTooLarge,
            ValidationError::VecTooLong,
            ValidationError::MemoryError(MemoryError::OutOfBounds),
            ValidationError::InvalidNegativeMemOffset,
        ] {
            let explained = explain_refusal(&conformant(), verdict(error.clone()), 64, 256, 256);
            assert!(matches!(explained, LoadError::ConformanceGap(_)), "{error:?}: {explained:?}");
        }
        assert_eq!(
            explain_refusal(&conformant(), verdict(ValidationError::TypeMismatch), 64, 256, 256)
                .to_string(),
            "the module does not load under the SpaceWasm interpreter: TypeMismatch at byte 42. \
             infc's conformance check accepts this module, so this is a gap in that check; please \
             report it at https://github.com/Inferara/inference/issues with the artifact"
        );
        for error in [
            ValidationError::FunctionImportNotFound,
            ValidationError::FunctionImportTypeMismatch,
            ValidationError::DuplicateModuleName,
            ValidationError::InvalidHostStartFunction,
            ValidationError::GuestMemoryAllocationFailure,
            ValidationError::MemoryError(MemoryError::OutOfMemory),
            ValidationError::MemoryError(MemoryError::AllocationFailed),
            ValidationError::MemoryError(MemoryError::PageTooSmall),
            ValidationError::AllocError(AllocError::AllocationFailed),
            ValidationError::AllocError(AllocError::PageTooSmall),
            ValidationError::IllegalMemoryGrow,
            ValidationError::LabelJumpTooLarge,
            ValidationError::PageFault,
            ValidationError::PossibleBackpatchCycle,
        ] {
            let explained = explain_refusal(&conformant(), verdict(error.clone()), 64, 256, 256);
            assert!(matches!(explained, LoadError::Decode(_)), "{error:?}: {explained:?}");
        }
        let grow = verdict(ValidationError::IllegalMemoryGrow);
        assert_eq!(
            explain_refusal(&conformant(), grow, 64, 256, 256).to_string(),
            "the module does not load under the SpaceWasm interpreter: IllegalMemoryGrow at byte 42"
        );
        let unchecked = explain_refusal(
            b"not wasm",
            verdict(ValidationError::AllocError(AllocError::OutOfMemory)),
            64,
            256,
            256,
        );
        assert!(matches!(unchecked, LoadError::Decode(_)), "{unchecked:?}");
    }

    /// Out of memory inside both bounds is too much IR, naming the pages the
    /// load gave; over a bound, the bound it exceeds, which a tighter bound
    /// than the reference one moves.
    #[test]
    fn running_out_of_memory_is_told_apart_by_the_checks_measurements() {
        let out_of_memory = || verdict(ValidationError::AllocError(AllocError::OutOfMemory));
        let explained = explain_refusal(&conformant(), out_of_memory(), 64, 256, 3);
        assert!(
            matches!(explained, LoadError::CodePagesExhausted { pages: 3, .. }),
            "{explained:?}"
        );
        assert_eq!(
            explained.to_string(),
            "the module does not load under the SpaceWasm interpreter: the interpreter's compiled \
             form of it does not fit the 3 IR code pages the runner provides (the interpreter \
             reported AllocError(OutOfMemory) at byte 42); its control nesting and operand stack \
             are within the limits the runner loads it at"
        );
        let LoadError::OverLimit(frames) =
            explain_refusal(&conformant(), out_of_memory(), 0, 256, 3)
        else {
            panic!("a bound of no frames is exceeded");
        };
        assert_eq!((frames.limit, frames.needed, frames.allowed), (Limit::ControlFrames, 1, 0));
        assert_eq!(frames.function, "func[0]", "a function the name section does not name");
        let LoadError::OverLimit(stack) =
            explain_refusal(&conformant(), out_of_memory(), 64, 0, 3)
        else {
            panic!("a bound of no operand values is exceeded");
        };
        assert_eq!((stack.limit, stack.needed, stack.allowed), (Limit::OperandStack, 1, 0));
        assert_eq!(
            (Limit::ControlFrames.const_generic(), Limit::OperandStack.const_generic()),
            ("MAX_CONTROL_FRAMES", "MAX_STACK_DEPTH")
        );
    }

    /// The two reasons a module the check accepts does not load inside both
    /// verifier bounds are worded once, here: too much IR for the code pages,
    /// naming the embedder given in both places a sentence names it and the
    /// page count in the grammar it takes, and a gap in the check, which names
    /// no embedder. Each variant's own text is that wording naming the runner.
    ///
    /// Fails if a caller's name is dropped, or replaced by the runner's, if a
    /// variant's text drifts from the wording a caller composes from, or if a
    /// page count of one is spelled as a plural.
    #[test]
    fn the_reasons_a_conformant_module_does_not_load_name_the_embedder_given() {
        let out_of_memory = verdict(ValidationError::AllocError(AllocError::OutOfMemory));
        for (embedder, pages, text) in [
            (
                "`infs run`",
                256,
                "the interpreter's compiled form of it does not fit the 256 IR code pages `infs \
                 run` provides (the interpreter reported AllocError(OutOfMemory) at byte 42); its \
                 control nesting and operand stack are within the limits `infs run` loads it at",
            ),
            (
                "the runner",
                1,
                "the interpreter's compiled form of it does not fit the 1 IR code page the runner \
                 provides (the interpreter reported AllocError(OutOfMemory) at byte 42); its \
                 control nesting and operand stack are within the limits the runner loads it at",
            ),
        ] {
            assert_eq!(code_pages_exhausted(pages, &out_of_memory, embedder), text);
        }
        let exhausted = LoadError::CodePagesExhausted { pages: 1, verdict: out_of_memory.clone() };
        assert_eq!(
            exhausted.to_string(),
            format!(
                "the module does not load under the SpaceWasm interpreter: {}",
                code_pages_exhausted(1, &out_of_memory, "the runner")
            )
        );

        let type_mismatch = verdict(ValidationError::TypeMismatch);
        let gap = "TypeMismatch at byte 42. infc's conformance check accepts this module, so this \
                   is a gap in that check; please report it at \
                   https://github.com/Inferara/inference/issues with the artifact";
        assert_eq!(conformance_gap(&type_mismatch), gap);
        assert_eq!(
            LoadError::ConformanceGap(type_mismatch).to_string(),
            format!("the module does not load under the SpaceWasm interpreter: {gap}")
        );
    }
}

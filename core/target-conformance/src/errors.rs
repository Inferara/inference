//! Every way the `SpaceWasm` envelope can refuse an artifact.
//!
//! One enum rather than one per question, because a caller holding bytes wants
//! a single thing to match on and a single rendering to print. Everything here
//! belongs to that one runtime: the variants, the maxima they quote, the
//! authority and remedy sentence behind each, and the header
//! [`Violations::render`] writes. The module is split out of
//! [`crate::spacewasm`] for length rather than because anything in it is
//! target-neutral, and the types are re-exported from there, which is the path
//! a caller reaches them through.
//!
//! The one refusal this crate makes that is not among them is
//! [`crate::check_wasm1`]'s, which is the validator's own message: not this
//! crate's finding but the decoder's, and the caller that knows where the bytes
//! came from is what turns it into a sentence.

use core::fmt;
use core::fmt::Write as _;

use crate::spacewasm::{
    MAX_BRANCH_UNWIND_WORDS, MAX_CUSTOM_SECTION_NAME_BYTES, MAX_FRAME_WORDS,
    MAX_HOST_FUNCTION_PARAMS, MAX_HOST_FUNCTION_RESULTS, MAX_IMPORT_NAME_BYTES, MAX_IR_INDEX,
    MAX_LOCAL_WORDS, MAX_LOCALS_GROUP_COUNT, MAX_MEMORY_PAGES, MAX_PARAM_WORDS,
};

/// Which half of an import's two names a length refusal is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamePart {
    /// The module name — the left half of `(import "m" "f" …)`.
    Module,
    /// The field name — the right half.
    Field,
}

impl fmt::Display for NamePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Module => f.write_str("module"),
            Self::Field => f.write_str("field"),
        }
    }
}

/// Which of the interpreter's two 16-bit IR immediates a body overflowed.
///
/// Both meet the same cap, in two places rather than one: a `br_table`'s target
/// count goes through the shared emitter (`src/text.rs:1099-1102`,
/// `instr_imm_8_or_16`, called at `src/compiler.rs:166`), and a `call_indirect`'s
/// type index is checked by hand before its `write_16` (`src/compiler.rs:281-287`).
/// One is an index into the type section and the other is the width of a jump
/// table, and a reader told the wrong one edits the wrong thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// The type index a `call_indirect` names.
    CallIndirectType,
    /// The number of non-default targets a `br_table` lists.
    BranchTableTargets,
}

impl fmt::Display for IndexKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CallIndirectType => f.write_str("the type index of a `call_indirect`"),
            Self::BranchTableTargets => f.write_str("the target count of a `br_table`"),
        }
    }
}

/// Which of the two index spaces the interpreter narrows without checking.
///
/// Both are narrowed by one accessor each — `Module::get_func_ref`
/// (`src/module.rs:383-389`) and `Module::get_global_ref`
/// (`src/module.rs:423-429`) — and the two are told apart because the count to
/// shrink and the section to look in are different for each, and a finding that
/// named neither would leave the reader to guess which of the two a module of
/// 65,536 definitions had too many of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexSpace {
    /// The module's own functions, numbered after the imported ones.
    Function,
    /// The module's own globals, numbered after the imported ones.
    Global,
}

impl fmt::Display for IndexSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Function => f.write_str("function"),
            Self::Global => f.write_str("global"),
        }
    }
}

/// Where a module names a defined function or a defined global.
///
/// Five places do, and every one of them resolves through the two narrowing
/// accessors: a `call` operand (`src/compiler.rs:202-206`), a `global.get` or
/// `global.set` operand (`src/text.rs:631-634`), an export descriptor
/// (`src/module.rs:753-778`), an element-segment entry (`src/module.rs:870-874`)
/// and the `start` section (`src/module.rs:299-303`). The cross-module link
/// path (`src/imports.rs:71,169`) reaches them too, but with an index it reads
/// back out of the *exporting* module's export descriptor, so a refusal at that
/// module's export section already covers it. The first two are edited in a
/// function body and the rest in three different sections, which is why the
/// refusal names the place: a reader told the wrong one goes looking in a part
/// of the module that names nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceSite {
    /// An instruction in the named function's body.
    Body {
        /// The function, named from the `name` section where it has one.
        function: String,
    },
    /// The descriptor of the named export.
    Export {
        /// The export's name.
        name: String,
    },
    /// An entry of the element segment at this index.
    ElementSegment {
        /// The segment's index in the element section.
        index: u32,
    },
    /// The `start` section.
    Start,
}

impl fmt::Display for ReferenceSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Body { function } => write!(f, "the body of function `{function}`"),
            Self::Export { name } => write!(f, "the export `{name}`"),
            Self::ElementSegment { index } => write!(f, "element segment {index}"),
            Self::Start => f.write_str("the start section"),
        }
    }
}

/// One reason a module is not one a `SpaceWasm` embedder can load and run as
/// written.
///
/// The `Display` here is the finding alone — what, and both numbers. The
/// sentence naming the authority and the sentence naming the remedy are
/// separate, because [`Violations`] renders all three and a caller quoting one
/// violation wants the first.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    /// The module uses something outside WebAssembly 1.0, or is malformed.
    #[error("not WebAssembly 1.0: {detail}")]
    OutsideWasm1 {
        /// The validator's own message, naming the feature and the offset.
        detail: String,
    },
    /// A function declares more parameter words than the decoder's field holds.
    #[error(
        "parameter words exceeded: function `{function}` declares {words} parameter words; \
         SpaceWasm accepts at most {MAX_PARAM_WORDS}"
    )]
    ParamWordsExceeded {
        /// The function, named from the `name` section where it has one.
        function: String,
        /// The declared width, in words.
        words: u64,
    },
    /// A function declares more local words than the decoder's field holds.
    #[error(
        "local words exceeded: function `{function}` declares {words} local words; \
         SpaceWasm accepts at most {MAX_LOCAL_WORDS}"
    )]
    LocalWordsExceeded {
        /// The function, named from the `name` section where it has one.
        function: String,
        /// The declared width, in words.
        words: u64,
    },
    /// A function's worst-case call frame is wider than the decoder's field.
    #[error(
        "call frame too wide: function `{function}` needs {words} stack words \
         ({local_words} local words plus {operand_words} operand words plus 2 of header); \
         SpaceWasm accepts at most {MAX_FRAME_WORDS}"
    )]
    FrameWordsExceeded {
        /// The function, named from the `name` section where it has one.
        function: String,
        /// The whole frame, in words.
        words: u64,
        /// Its locals half.
        local_words: u64,
        /// Its operand half.
        operand_words: u32,
    },
    /// One locals group declares a count wider than the decoder's field.
    #[error(
        "locals group too large: function `{function}` declares a group of {count} locals; \
         SpaceWasm accepts at most {MAX_LOCALS_GROUP_COUNT} per group"
    )]
    LocalsGroupTooLarge {
        /// The function, named from the `name` section where it has one.
        function: String,
        /// The declared group count.
        count: u32,
    },
    /// A body names a value wider than one of the interpreter's 16-bit IR
    /// immediates.
    #[error(
        "IR index too large: in function `{function}`, {kind} is {index}; SpaceWasm holds it \
         in a 16-bit IR immediate and accepts at most {MAX_IR_INDEX}"
    )]
    IndexTooLarge {
        /// The function, named from the `name` section where it has one.
        function: String,
        /// Which immediate it is, since the two are edited differently.
        kind: IndexKind,
        /// The value that has to fit: an index for a `call_indirect`, a count
        /// for a `br_table`, because the decoder writes both into the same
        /// field. The largest one the function names is the one reported.
        index: u32,
    },
    /// A reference to a defined function or global at a position wider than the
    /// 16-bit IR word the interpreter narrows it to without checking.
    ///
    /// The one refusal in this crate that is stricter than the decoder: 0.7.1
    /// loads such a module and runs it against a different definition.
    #[error(
        "defined index truncated: {site} names {space} {index}, which is definition \
         {position} among the module's own; SpaceWasm holds a defined index in one 16-bit \
         IR word and does not check the narrowing, so the reference resolves to definition \
         {} instead",
        .position % (MAX_IR_INDEX + 1)
    )]
    IndexTruncated {
        /// Which index space the reference is in, since the finding has to say
        /// which of the two the module has too many of.
        space: IndexSpace,
        /// Where the module names it.
        site: ReferenceSite,
        /// The index as the module writes it, imported entries included. The
        /// largest one a site names is the one reported, as with
        /// [`Violation::IndexTooLarge`].
        index: u32,
        /// The same reference counted from the module's first defined entry,
        /// which is the number the interpreter narrows.
        position: u32,
    },
    /// A branch discards more operand words than the jump target's field holds.
    #[error(
        "branch unwinds too many operands: a branch in function `{function}` discards \
         {words} operand words; SpaceWasm accepts at most {MAX_BRANCH_UNWIND_WORDS}"
    )]
    BranchUnwindTooDeep {
        /// The function, named from the `name` section where it has one.
        function: String,
        /// The deepest unwind in that function, in words.
        words: u32,
    },
    /// An import name is longer than an embedder can register a host under.
    #[error(
        "import name too long: import {which} name `{name}` on `{module}`.`{field}` is \
         {len} bytes; SpaceWasm accepts at most {MAX_IMPORT_NAME_BYTES}"
    )]
    ImportNameTooLong {
        /// The import's module name.
        module: String,
        /// The import's field name.
        field: String,
        /// Which of the two the length is about.
        which: NamePart,
        /// The offending name, repeated so the finding reads alone.
        name: String,
        /// Its length in bytes.
        len: usize,
    },
    /// An imported function declares more parameters than a host can carry.
    #[error(
        "host function takes too many parameters: import `{module}`.`{field}` declares \
         {params} parameters; SpaceWasm accepts at most {MAX_HOST_FUNCTION_PARAMS}"
    )]
    ImportArityExceeded {
        /// The import's module name.
        module: String,
        /// The import's field name.
        field: String,
        /// The declared parameter count.
        params: usize,
    },
    /// An imported function declares more than one result.
    #[error(
        "host function returns more than one value: import `{module}`.`{field}` declares \
         {results} results; SpaceWasm accepts at most {MAX_HOST_FUNCTION_RESULTS}"
    )]
    ImportMultipleResults {
        /// The import's module name.
        module: String,
        /// The import's field name.
        field: String,
        /// The declared result count.
        results: usize,
    },
    /// A custom section's name is longer than the decoder's name buffer.
    #[error(
        "custom section name too long: `{name}` is {len} bytes; SpaceWasm accepts at most \
         {MAX_CUSTOM_SECTION_NAME_BYTES}"
    )]
    CustomSectionNameTooLong {
        /// The section's name.
        name: String,
        /// Its length in bytes.
        len: usize,
    },
    /// A linear memory declares more pages than the decoder admits.
    #[error(
        "linear memory too large: declares {pages} pages; SpaceWasm addresses at most \
         {MAX_MEMORY_PAGES}"
    )]
    MemoryTooLarge {
        /// The offending page count — the minimum, or the maximum when that is
        /// the half over the bound.
        pages: u64,
    },
}

impl Violation {
    /// The sentence naming what imposes this limit.
    ///
    /// Never "`SpaceWasm` rejects it": a decode limit and a registration limit
    /// are met at different moments and shortened by different edits, and a
    /// user told the wrong one changes the wrong thing.
    #[must_use]
    pub fn authority(&self) -> &'static str {
        match self {
            Self::OutsideWasm1 { .. } => {
                "SpaceWasm decodes WebAssembly 1.0 plus mutable globals, and nothing else. \
                 Inference code generation emits no post-1.0 instruction for this target, so \
                 this arrived from a linked module or a post-build step."
            }
            Self::ParamWordsExceeded { .. } => {
                "SpaceWasm stores a function's parameter size in a single byte. An i64 or f64 \
                 parameter counts as 2 words, every other parameter as 1."
            }
            Self::LocalWordsExceeded { .. } => {
                "SpaceWasm stores a function's local size in a 16-bit field. An i64 or f64 \
                 local counts as 2 words, every other local as 1, and the locals of every \
                 nested block belong to the whole call."
            }
            Self::FrameWordsExceeded { .. } => {
                "SpaceWasm bounds the whole call frame a function leaves on the engine stack — \
                 two words of header, its locals, and its operand stack at the deepest call — \
                 to a 16-bit length. That is a tighter bound on locals alone than the local \
                 size field is, because the header is always there."
            }
            Self::LocalsGroupTooLarge { .. } => {
                "A code section declares locals as run-length groups, and SpaceWasm stores \
                 each group's count in a 16-bit field."
            }
            Self::IndexTooLarge { .. } => {
                "SpaceWasm compiles a module into a bytecode of its own before running it, and \
                 that bytecode holds a `call_indirect`'s type index and a `br_table`'s target \
                 count in one 16-bit immediate. The cap is on the compiled form, not on the \
                 WebAssembly encoding, which admits a 32-bit value for both."
            }
            Self::IndexTruncated { .. } => {
                "This is the one refusal this crate makes that the decoder does not: SpaceWasm \
                 0.7.1 narrows a defined function or global index into the same 16-bit IR word \
                 its other immediates use, and unlike those it does not check the narrowing \
                 (`Module::get_func_ref` and `Module::get_global_ref`). The module loads, and \
                 every reference past the cap runs against the definition 65536 below it. A \
                 build that refuses is better for a flight target than one that loads and \
                 calls the wrong function, so this crate is deliberately stricter here. The \
                 narrowing is reported upstream as nasa/spacewasm#201."
            }
            Self::BranchUnwindTooDeep { .. } => {
                "SpaceWasm encodes the operands a branch discards in a single byte of its jump \
                 target word. The count is in words, so an i64 or f64 held live across the \
                 branch costs two of the 255. A branch to the function's own outermost frame \
                 is compiled as an early return and carries no unwind at all, so this is a \
                 branch to a block or a loop."
            }
            Self::ImportNameTooLong { .. } => {
                "This is a registration limit, not a decode limit: the decoder reads a name \
                 of up to 32 bytes, but an embedder registers host modules and host functions \
                 through a 31-byte name type, so a 32-byte name decodes and can never be bound \
                 to a host. 31 is the cap that matters."
            }
            Self::ImportArityExceeded { .. } => {
                "This is a registration limit, not a decode limit: host arguments are passed \
                 through a fixed-size list of 9, so a host function declaring more cannot be \
                 registered and the module can never instantiate."
            }
            Self::ImportMultipleResults { .. } => {
                "A host function returns at most one value, and a module importing more could \
                 never be bound to one."
            }
            Self::CustomSectionNameTooLong { .. } => {
                "SpaceWasm reads a custom section's name into a fixed 32-byte buffer, before \
                 it reaches the payload it would otherwise skip."
            }
            Self::MemoryTooLarge { .. } => {
                "65536 pages of 64 KiB is the whole 32-bit address space a WebAssembly 1.0 \
                 memory can address."
            }
        }
    }

    /// The one thing to change, in the source where there is one.
    ///
    /// Six of these shapes are not producible by this compiler at all. Their
    /// remedy splits on provenance instead of pointing at a source edit the
    /// author cannot make: either the module was linked in, or a compiler bug
    /// produced it, and only the person holding the build knows which. Four of
    /// the six share one sentence because there is nothing else to say. The
    /// other two, `IndexTooLarge` and `IndexTruncated`, split the same way and
    /// still name the edit, because for those there is one: a narrower
    /// instruction for the first, a smaller module for the second.
    ///
    /// The two `ImportNameTooLong` arms are producible from source and lead
    /// with the source edit, and they carry the linked-module alternative all
    /// the same: this method cannot see which check produced the finding, and
    /// the same sentence renders for a name read off a declaration and for one
    /// read off an artifact's import section — where the import may be one a
    /// linked module dragged in, with no `external fn` anywhere to rename.
    #[must_use]
    pub fn remedy(&self) -> &'static str {
        match self {
            Self::OutsideWasm1 { .. } => {
                "Rebuild the linked module against the WebAssembly 1.0 baseline — a stock Rust \
                 `wasm32-unknown-unknown` build emits sign-extension instructions by default — \
                 or drop it from the build."
            }
            Self::ParamWordsExceeded { .. } => {
                "Pass fewer parameters, or collect them into a struct: a struct or array \
                 parameter is one i32 pointer, which is 1 word."
            }
            Self::LocalWordsExceeded { .. } | Self::FrameWordsExceeded { .. } => {
                "Split the function into smaller functions, or move a large array out of the \
                 frame."
            }
            Self::IndexTooLarge { kind, .. } => match kind {
                IndexKind::CallIndirectType => {
                    "This compiler emits no `call_indirect`, so these bytes came from a linked \
                     module or a post-build step: rebuild that module with fewer function \
                     types, or split it. If `infc` produced it, please report a compiler bug."
                }
                IndexKind::BranchTableTargets => {
                    "This compiler emits no `br_table`, so these bytes came from a linked \
                     module or a post-build step: rebuild that module with a narrower jump \
                     table, or split the dispatch. If `infc` produced it, please report a \
                     compiler bug."
                }
            },
            Self::IndexTruncated { .. } => {
                "This compiler emits one global and nowhere near 65,536 functions, so these \
                 bytes came from a linked module or a post-build step: split that module, or \
                 link fewer of them, so no definition sits at 65,536 or beyond. If `infc` \
                 produced it, please report a compiler bug."
            }
            Self::BranchUnwindTooDeep { .. } => {
                "Hold fewer operands across the branch: bind them to locals before it, or \
                 split the block so the jump crosses less of the stack."
            }
            Self::ImportNameTooLong { which, .. } => match which {
                NamePart::Module => {
                    "Shorten the module name this extern is bound under. If the import came \
                     from a linked module rather than from a declaration in this program, \
                     rebuild that module so it imports from a shorter module name."
                }
                NamePart::Field => {
                    "Rename the `external fn` and the name in the `use { … }` clause that \
                     binds it: the field name an import carries is the declaration's own \
                     name. If the import came from a linked module rather than from a \
                     declaration in this program, rebuild that module so it imports the \
                     function under a shorter name."
                }
            },
            Self::ImportArityExceeded { .. } => {
                "Declare fewer parameters on the `external fn`, or collect them into a struct: \
                 a struct or array argument is one i32 pointer."
            }
            Self::LocalsGroupTooLarge { .. }
            | Self::ImportMultipleResults { .. }
            | Self::CustomSectionNameTooLong { .. }
            | Self::MemoryTooLarge { .. } => {
                "This module shape is not producible by this compiler: if `infc` produced it, \
                 please report a compiler bug; if it was linked in, rebuild the external \
                 module."
            }
        }
    }
}

/// What a set of findings was read from, which is the one thing about them a
/// rendering cannot recover from the findings themselves.
///
/// A refusal opens by saying what was checked, and the two checks in this crate
/// are asked of different things: one is handed a finished module, the other a
/// program's declarations with no module anywhere yet. Told "this is not a
/// module a `SpaceWasm` embedder can load", an author holding a `.inf` file and
/// no artifact has been handed a category error rather than a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Checked {
    /// A finished module's bytes, read by [`crate::spacewasm::check`].
    Artifact,
    /// The imports a program declares, read by
    /// [`crate::spacewasm::check_host_imports`] before any bytes exist.
    Declarations,
}

/// Every reason a check refused what it was asked about, in a fixed order.
///
/// Non-empty by construction: [`crate::spacewasm::check`] over an artifact's
/// bytes and [`crate::spacewasm::check_host_imports`] over the imports a
/// program declares each return `Ok` where they found nothing rather than an
/// empty `Violations`, so the invariant belongs to both of them and a
/// `Violations` in hand always names at least one finding however it was
/// built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violations {
    checked: Checked,
    found: Vec<Violation>,
}

impl Violations {
    /// The refusals read out of a finished module's bytes.
    ///
    /// `pub(crate)` along with its sibling because the non-empty invariant
    /// above is the two builders' to keep: a constructor open to callers would
    /// lose it, and that invariant is what lets a rendered refusal promise at
    /// least one finding under its header.
    pub(crate) fn about_artifact(found: Vec<Violation>) -> Self {
        Self {
            checked: Checked::Artifact,
            found,
        }
    }

    /// The refusals read off the imports a program declares, before any bytes
    /// exist to read.
    pub(crate) fn about_declarations(found: Vec<Violation>) -> Self {
        Self {
            checked: Checked::Declarations,
            found,
        }
    }

    /// The findings, in the order they are rendered.
    #[must_use]
    pub fn as_slice(&self) -> &[Violation] {
        &self.found
    }

    /// The full refusal: a header naming what was checked and what it cost,
    /// then one block per finding giving the two numbers, the authority and one
    /// thing to change.
    ///
    /// `subject` is what the reader knows the checked thing as — a path, or a
    /// phrase like "the linked module" for a build that writes no file, or the
    /// source file a declaration was read from. `consequence` is the one
    /// sentence only the caller can write: what it did about the refusal. Both
    /// are the caller's because this crate is handed the findings and nothing
    /// else.
    ///
    /// The opening sentence is not the caller's, and is chosen by which check
    /// built the value: a declaration-level refusal cannot open by calling the
    /// subject a module, because at that point there is none.
    ///
    /// An empty `consequence` omits that line, which is what the `Display` impl
    /// passes: a reader quoting the violations out of a build has no build to
    /// report the consequence of.
    #[must_use]
    pub fn render(&self, subject: &str, consequence: &str) -> String {
        let mut out = match self.checked {
            Checked::Artifact => format!(
                "SpaceWasm conformance failed: {subject} is not a module a SpaceWasm embedder \
                 can load and run as written.\n"
            ),
            Checked::Declarations => format!(
                "SpaceWasm conformance failed: the host imports {subject} declares cannot all \
                 be registered by a SpaceWasm embedder.\n"
            ),
        };
        if !consequence.is_empty() {
            out.push_str(consequence);
            out.push('\n');
        }
        for violation in &self.found {
            let _ = write!(
                out,
                "\n  {violation}.\n    {}\n    {}\n",
                violation.authority(),
                violation.remedy()
            );
        }
        out
    }
}

impl fmt::Display for Violations {
    /// The rendering a caller with nothing to add falls back to, which is what
    /// `?` on a `Violations` produces. The subject is as specific as a value
    /// that was handed no path can be, and it follows what was checked: naming
    /// a program's declarations "this module" would be the category error the
    /// header above already avoids.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let subject = match self.checked {
            Checked::Artifact => "this module",
            Checked::Declarations => "this program",
        };
        f.write_str(&self.render(subject, ""))
    }
}

impl std::error::Error for Violations {}

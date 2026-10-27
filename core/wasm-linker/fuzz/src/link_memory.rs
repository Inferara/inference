//! The generator, oracle and checker behind the `link_memory` fuzz target.
//!
//! The `link` target feeds raw bytes to the linker, and random bytes almost
//! never form a main module plus externals whose memory sections and import
//! lists interact. That is where #388 lived: the linked memory depended on the
//! order the main module listed its imports in. This module describes a link
//! structurally instead, as a [`LinkSpec`], builds its modules with
//! `wasm-encoder`, links them in three import orders, and checks what the
//! linker produced against the documented memory rule.
//!
//! Two crates include this one file with `#[path]`, so they run literally the
//! same generator and checks: the fuzz target
//! (`core/wasm-linker/fuzz/fuzz_targets/link_memory.rs`), and the stable
//! `cargo test` replay (`core/wasm-linker/tests/link_memory_seeds.rs`). It
//! depends on `wasm_encoder`, `inf_wasmparser` and `inference_wasm_linker`
//! alone; the fuzz target adds the `Arbitrary` impl on its side.
//!
//! ## The byte format
//!
//! [`decode`] reads a fixed-width format, one byte per choice, [`INPUT_LEN`]
//! bytes in all. A missing byte reads as 0 and bytes past the end are ignored,
//! so every byte string decodes to a valid spec. Every field sits at a fixed
//! offset, used or not, so flipping one byte never shifts where later bytes are
//! read. [`encode`] is the inverse.
//!
//! ```text
//! off  0  main slot:        tag lo hi                memory iff tag % 2 == 1
//! off  3  provider count:   n                        P = 1 + n % 4
//! off  4  provider slot ×4: tag lo hi k b0 b1 b2     memory iff tag % 2 == 1;
//!                                                    exports = 1 + k % 3;
//!                                                    body j = BODIES[bj % 5]
//! off 32  import count:     n                        I = n % 9
//! off 33  import slot ×8:   p e                      provider = p % P,
//!                                                    export = e % exports(provider)
//! off 49  shuffle key ×8:   key
//!
//! limits: min = PAGES[lo % 8];
//!         hi % 9 == 8 → no maximum, else max = max(PAGES[hi % 9], min)
//! a memoryless provider's bodies decode as Pure
//! ```
//!
//! `BODIES` is [`Body::ALL`] and `PAGES` is [`PAGES`].
//!
//! ## The generated links
//!
//! Provider `i` is bound under the logical module `m{i}` and exports `f0`, `f1`,
//! … in order, so every provider exports `f0` and fields collide across
//! providers by default. Every function has the signature `(i32) -> i32`. A
//! provider declares at most one memory and nothing else: no data or element
//! segment, table, start function or import. The main module imports
//! `(m{p}, f{e})` for each [`ImportSpec`], in the run's order, repeats
//! included, optionally declares a memory, and exports a `run` that calls each
//! import once.
//!
//! The shapes are chosen so a memory refusal is the only way a link can fail:
//! each [`Body`] is admitted at Tier A or B, and a memoryless provider's bodies
//! are pure. Errors raised per import, which by design name the first failing
//! import in import order, therefore never arise.
//!
//! ## The rule the oracle encodes
//!
//! [`expected`] transcribes the rule documented in the linker README
//! (`core/wasm-linker/README.md`, "Linear-Memory Reconciliation") and in the
//! book (`book/src/the-wasm-linker.md`, "The Shared Linear Memory"). It calls
//! no linker internals.
//!
//! - Every satisfied import whose closure touches memory contributes its
//!   provider's declared limits, once per import entry. An import whose
//!   closure touches no memory contributes nothing, whatever its provider
//!   declares.
//! - **Main declares a memory:** the output keeps main's maximum and widens its
//!   minimum to the largest contributed one. A minimum above main's maximum is
//!   refused.
//! - **Main is memoryless:** with no contributor there is no memory. Otherwise
//!   the output is the largest contributed minimum and the smallest declared
//!   maximum, unbounded when no contributor declares one. A minimum above that
//!   maximum is refused, and so is a growing contributor whose own maximum
//!   differs from it.
//! - **Either mode:** a growing contributor is refused when the result is
//!   pinned (`max == min`).
//!
//! The documents say *when* a link is refused, not which reason the linker
//! reports when several apply, nor which of several tied contributors it
//! names. So on a refusal the oracle gives every applicable [`Refusal`], each
//! with the fields it may name, the numbers and the remedy its message must
//! print, and the sets its message must name one member of: the contributors
//! that declare the shared maximum, when the message names the module that
//! declares it, and the caps of the growers it moves. The linker's report has
//! to match one of them.
//!
//! The warning rule is `LinkWarning::TierBInMultiPageMemory`'s: raised when
//! the output has a memory whose minimum is above one page and some import is
//! admitted at Tier B, which here means its body touches memory. It names the
//! bare fields of those imports, sorted and deduplicated, and the output
//! minimum.
//!
//! ## The properties [`check`] asserts
//!
//! 1. **Round trip:** `decode(encode(spec)) == spec`, so any spec the fuzzer
//!    finds can be written into the seed table as a literal.
//! 2. **Generator:** every generated module passes `inf_wasmparser::validate`,
//!    so a generator bug is reported as one rather than as a linker `Parse`
//!    error.
//! 3. **No panic:** each of the three links returns. A panic propagates: the
//!    fuzz harness reports it as a crash.
//! 4. **Validity:** every `Ok` output passes `inf_wasmparser::validate` and
//!    declares at most one memory.
//! 5. **Invariance:** the three runs observe the same linked memory limits, or
//!    the same error text, and the same warnings. The runs link the imports in
//!    the generated order; reversed, with the externals slice reversed too;
//!    and stably sorted by [`LinkSpec::shuffle`].
//! 6. **Oracle:** an `Ok` is expected, with the expected limits. A refusal is
//!    expected, is a `LinkError::IncompatibleMemory`, and matches exactly one
//!    applicable [`Refusal`]: its message carries that kind's wording, its
//!    remedy, every number the kind prints and a member of each set the kind
//!    names one of, and its `field` is one the kind may name. Any other error
//!    is a violation.
//! 7. **Warnings:** the warnings equal the expected ones.
//!
//! Merged bytes are not compared across orders: function order and indices
//! legitimately follow import order. Only the memory section, the error text
//! and the warnings are order-independent.

use std::collections::BTreeSet;
use std::fmt;

use inf_wasmparser::{Parser, Payload};
use inference_wasm_linker::{LinkError, LinkOutput, LinkWarning, link_with_warnings};
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemArg, MemorySection, MemoryType, Module, TypeSection, ValType,
};

/// The page counts a decoded limit draws from: both ends of the 32-bit range,
/// and small values that collide often, so equal caps, pinned memories and
/// unbounded-beside-bounded links come up constantly.
pub const PAGES: [u64; 8] = [0, 1, 2, 3, 5, 10, 65535, 65536];

/// The length of one encoded spec. [`encode`] writes exactly this many bytes;
/// [`decode`] reads a shorter input as if padded with zeros.
pub const INPUT_LEN: usize = 57;

/// The most providers one spec links.
pub const MAX_PROVIDERS: usize = 4;

/// The most exports one provider has.
pub const MAX_EXPORTS: usize = 3;

/// The most imports the main module declares.
pub const MAX_IMPORTS: usize = 8;

// The offsets of the byte format's fields. A limits slot is `tag lo hi`; a
// provider slot is a limits slot, the export count, and one byte per body.
const MAIN_SLOT: usize = 0;
const PROVIDER_COUNT: usize = MAIN_SLOT + 3;
const PROVIDER_SLOTS: usize = PROVIDER_COUNT + 1;
const PROVIDER_SLOT_LEN: usize = 3 + 1 + MAX_EXPORTS;
const IMPORT_COUNT: usize = PROVIDER_SLOTS + MAX_PROVIDERS * PROVIDER_SLOT_LEN;
const IMPORT_SLOTS: usize = IMPORT_COUNT + 1;
const SHUFFLE_KEYS: usize = IMPORT_SLOTS + 2 * MAX_IMPORTS;
const _: () = assert!(
    SHUFFLE_KEYS + MAX_IMPORTS == INPUT_LEN,
    "the byte format's fields fill exactly INPUT_LEN bytes"
);

/// The `hi` value that decodes to "no maximum".
const UNBOUNDED: u8 = PAGES.len() as u8;

/// One link: a main module and the providers it imports from.
///
/// The fields are plain data: the bounds their documentation states hold in
/// the domain [`decode`] produces and [`encode`] asserts, not for every value
/// of the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSpec {
    /// The main module's memory, if it declares one.
    pub main_memory: Option<Limits>,
    /// One to four providers. Provider `i` is bound under logical module `m{i}`.
    pub providers: Vec<ProviderSpec>,
    /// Up to eight imports, repeats allowed, which the main module declares in
    /// this order.
    pub imports: Vec<ImportSpec>,
    /// One key per import. The third run links the imports stably sorted by it.
    pub shuffle: Vec<u8>,
}

/// A memory's page limits. In the domain [`decode`] produces and [`encode`]
/// asserts, both come from [`PAGES`], and `max`, when present, is at least
/// `min`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub min: u64,
    pub max: Option<u64>,
}

/// One provider module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    /// The provider's memory, if it declares one. In the domain [`decode`]
    /// produces and [`encode`] asserts, a memoryless provider's bodies are all
    /// [`Body::Pure`].
    pub memory: Option<Limits>,
    /// One to three exports. Export `j` is named `f{j}`.
    pub exports: Vec<Body>,
}

/// What an exported function does with memory. Every shape is admitted at
/// Tier A or Tier B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Body {
    /// `local.get 0`: touches no memory (Tier A).
    Pure,
    /// `local.get 0; i32.load`: reads at a caller-supplied address (Tier B).
    Load,
    /// `memory.size`: observes the memory without addressing it (Tier B).
    Size,
    /// `local.get 0; memory.grow`: grows the memory (Tier B).
    Grow,
    /// `local.get 0; call $grow`, where the unexported `$grow` grows the
    /// memory: growth reached through the call closure (Tier B).
    GrowViaHelper,
}

impl Body {
    /// Every body, in the order the byte format indexes them.
    pub const ALL: [Body; 5] = [
        Body::Pure,
        Body::Load,
        Body::Size,
        Body::Grow,
        Body::GrowViaHelper,
    ];

    /// Whether the body's closure touches linear memory, which is what makes
    /// its provider's memory a contribution.
    pub fn uses_memory(self) -> bool {
        self != Body::Pure
    }

    /// Whether the body's closure grows linear memory.
    pub fn grows(self) -> bool {
        matches!(self, Body::Grow | Body::GrowViaHelper)
    }

    fn index(self) -> u8 {
        Body::ALL
            .iter()
            .position(|&body| body == self)
            .expect("Body::ALL lists every body") as u8
    }
}

/// One main-module import: export `f{export}` of provider `m{provider}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSpec {
    pub provider: usize,
    pub export: usize,
}

/// A memory refusal the documented rule describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Refusal {
    /// Main declares a memory, and a contributed minimum exceeds its maximum.
    MainTooSmall,
    /// Main is memoryless, and the largest contributed minimum exceeds the
    /// smallest declared maximum.
    NoFit,
    /// Main is memoryless, and a growing contributor's own maximum differs from
    /// the shared one.
    GrowerCapMoved,
    /// A growing contributor would get a pinned memory (`max == min`).
    GrowthPinned,
}

impl Refusal {
    /// Every refusal kind.
    pub const ALL: [Refusal; 4] = [
        Refusal::MainTooSmall,
        Refusal::NoFit,
        Refusal::GrowerCapMoved,
        Refusal::GrowthPinned,
    ];

    /// The wording that identifies this kind's message. Each is unique to its
    /// kind.
    pub fn wording(self) -> &'static str {
        match self {
            Refusal::MainTooSmall => "exceeds the declared maximum",
            Refusal::NoFit => "no memory satisfies both declarations",
            Refusal::GrowerCapMoved => "does not choose one module's bound over another's",
            Refusal::GrowthPinned => "does not exceed its minimum",
        }
    }

    /// The settings this kind's message names as the remedy, the `[memory]`
    /// key and then the `infc` flag: the page count for a minimum larger than
    /// main's memory can hold, and the maximum for every other refusal.
    pub fn remedy(self) -> [&'static str; 2] {
        match self {
            Refusal::MainTooSmall => ["`pages`", "infc --memory-pages"],
            Refusal::NoFit | Refusal::GrowerCapMoved | Refusal::GrowthPinned => {
                ["`max-pages`", "infc --max-memory-pages"]
            }
        }
    }
}

/// What the documented rule says a spec's link produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    /// The link succeeds with this memory (`(min, max)`, or `None` for no
    /// memory section) and these warnings.
    Linked {
        memory: Option<(u64, Option<u64>)>,
        warnings: Vec<LinkWarning>,
    },
    /// The link is refused. `applicable` is never empty, and holds at most one
    /// entry per kind.
    Refused { applicable: Vec<Applicable> },
}

/// One refusal the rule says applies, with what its message may name and must
/// print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applicable {
    pub kind: Refusal,
    /// The import fields the error's `field` may be: every contributor the
    /// condition implicates, since the documents leave the tie-break open.
    pub fields: BTreeSet<String>,
    /// Substrings the message must contain, each with the number it prints.
    pub numbers: Vec<String>,
    /// Sets the message must contain at least one member of each: the
    /// contributors it may name as the module declaring the shared maximum,
    /// and the caps of the growers it may report.
    pub one_of: Vec<BTreeSet<String>>,
}

/// What [`check`] observed, once every property held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The link succeeded with this memory and these warnings.
    Linked {
        memory: Option<(u64, Option<u64>)>,
        warnings: Vec<LinkWarning>,
    },
    /// The link was refused as `kind`, with this message.
    Refused { kind: Refusal, message: String },
}

/// A property [`check`] found broken, with everything needed to reproduce it.
#[derive(Debug, Clone)]
pub struct Violation {
    property: &'static str,
    detail: String,
    /// Boxed, so a `Result` carrying a `Violation` stays small.
    spec: Box<LinkSpec>,
    /// Each run linked so far: its order, and what it produced.
    runs: Vec<(String, String)>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "link_memory property violated: {}", self.property)?;
        writeln!(f, "{}", self.detail)?;
        writeln!(f, "spec: {:#?}", self.spec)?;
        for (order, outcome) in &self.runs {
            writeln!(f, "run {order}")?;
            writeln!(f, "    {outcome}")?;
        }
        Ok(())
    }
}

/// One link to perform: the main module built in one import order, and the
/// externals slice in one order.
#[derive(Debug, Clone)]
pub struct Run {
    label: String,
    main: Vec<u8>,
    externals: Vec<(String, Vec<u8>)>,
}

/// Decodes a spec from the fixed-width byte format described in the module
/// documentation. Total: every byte string decodes to a spec in [`encode`]'s
/// domain.
pub fn decode(bytes: &[u8]) -> LinkSpec {
    let byte = |offset: usize| bytes.get(offset).copied().unwrap_or(0);
    let limits_at = |slot: usize| decode_limits(byte(slot), byte(slot + 1), byte(slot + 2));

    let main_memory = limits_at(MAIN_SLOT);
    let provider_count = 1 + usize::from(byte(PROVIDER_COUNT)) % MAX_PROVIDERS;
    let providers: Vec<ProviderSpec> = (0..provider_count)
        .map(|i| {
            let slot = PROVIDER_SLOTS + i * PROVIDER_SLOT_LEN;
            let memory = limits_at(slot);
            let export_count = 1 + usize::from(byte(slot + 3)) % MAX_EXPORTS;
            let exports = (0..export_count)
                .map(|j| match memory {
                    Some(_) => Body::ALL[usize::from(byte(slot + 4 + j)) % Body::ALL.len()],
                    None => Body::Pure,
                })
                .collect();
            ProviderSpec { memory, exports }
        })
        .collect();

    let import_count = usize::from(byte(IMPORT_COUNT)) % (MAX_IMPORTS + 1);
    let imports = (0..import_count)
        .map(|i| {
            let slot = IMPORT_SLOTS + 2 * i;
            let provider = usize::from(byte(slot)) % provider_count;
            let export = usize::from(byte(slot + 1)) % providers[provider].exports.len();
            ImportSpec { provider, export }
        })
        .collect();
    let shuffle = (0..import_count).map(|i| byte(SHUFFLE_KEYS + i)).collect();

    LinkSpec {
        main_memory,
        providers,
        imports,
        shuffle,
    }
}

fn decode_limits(tag: u8, lo: u8, hi: u8) -> Option<Limits> {
    if tag.is_multiple_of(2) {
        return None;
    }
    let min = PAGES[usize::from(lo) % PAGES.len()];
    let max = PAGES
        .get(usize::from(hi) % (PAGES.len() + 1))
        .map(|&cap| cap.max(min));
    Some(Limits { min, max })
}

/// Encodes a spec in the byte format [`decode`] reads, so that
/// `decode(&encode(spec)) == *spec`. Always [`INPUT_LEN`] bytes, with zeros in
/// unused slots.
///
/// # Panics
///
/// When `spec` is outside what [`decode`] can produce: a page count not in
/// [`PAGES`], a maximum below its minimum, a non-`Pure` body in a memoryless
/// provider, a provider, export or import count out of range, an import naming
/// a provider or export that does not exist, or a `shuffle` whose length is not
/// the import count. Only a hand-written seed can be out of the domain.
pub fn encode(spec: &LinkSpec) -> Vec<u8> {
    let mut out = vec![0u8; INPUT_LEN];
    if let Some(limits) = spec.main_memory {
        encode_limits(
            &mut out[MAIN_SLOT..MAIN_SLOT + 3],
            limits,
            "the main module",
        );
    }

    let provider_count = spec.providers.len();
    assert!(
        (1..=MAX_PROVIDERS).contains(&provider_count),
        "a spec links 1 to {MAX_PROVIDERS} providers, not {provider_count}"
    );
    out[PROVIDER_COUNT] = (provider_count - 1) as u8;
    for (i, provider) in spec.providers.iter().enumerate() {
        let slot = PROVIDER_SLOTS + i * PROVIDER_SLOT_LEN;
        if let Some(limits) = provider.memory {
            encode_limits(&mut out[slot..slot + 3], limits, &module_name(i));
        }
        let export_count = provider.exports.len();
        assert!(
            (1..=MAX_EXPORTS).contains(&export_count),
            "provider m{i} has 1 to {MAX_EXPORTS} exports, not {export_count}"
        );
        out[slot + 3] = (export_count - 1) as u8;
        for (j, &body) in provider.exports.iter().enumerate() {
            assert!(
                provider.memory.is_some() || body == Body::Pure,
                "provider m{i} declares no memory, so its export f{j} must be Pure, not {body:?}"
            );
            out[slot + 4 + j] = body.index();
        }
    }

    let import_count = spec.imports.len();
    assert!(
        import_count <= MAX_IMPORTS,
        "a spec has at most {MAX_IMPORTS} imports, not {import_count}"
    );
    out[IMPORT_COUNT] = import_count as u8;
    for (i, import) in spec.imports.iter().enumerate() {
        let provider = spec.providers.get(import.provider).unwrap_or_else(|| {
            panic!(
                "import {i} names provider {}, but the spec has {provider_count}",
                import.provider
            )
        });
        assert!(
            import.export < provider.exports.len(),
            "import {i} names export f{} of provider m{}, which has {}",
            import.export,
            import.provider,
            provider.exports.len()
        );
        out[IMPORT_SLOTS + 2 * i] = import.provider as u8;
        out[IMPORT_SLOTS + 2 * i + 1] = import.export as u8;
    }

    assert_eq!(
        spec.shuffle.len(),
        import_count,
        "a spec has one shuffle key per import"
    );
    out[SHUFFLE_KEYS..SHUFFLE_KEYS + import_count].copy_from_slice(&spec.shuffle);
    out
}

/// Writes a declared memory's `tag lo hi` slot. A memoryless slot stays zero.
fn encode_limits(slot: &mut [u8], limits: Limits, owner: &str) {
    let Limits { min, max } = limits;
    slot[0] = 1;
    slot[1] = page_index(min, owner);
    slot[2] = match max {
        None => UNBOUNDED,
        Some(max) => {
            assert!(
                max >= min,
                "{owner} declares a maximum of {max} pages below its minimum of {min}"
            );
            page_index(max, owner)
        }
    };
}

fn page_index(pages: u64, owner: &str) -> u8 {
    PAGES
        .iter()
        .position(|&p| p == pages)
        .unwrap_or_else(|| panic!("{owner} declares {pages} pages, which is not one of {PAGES:?}"))
        as u8
}

/// The logical module provider `index` is bound under.
fn module_name(index: usize) -> String {
    format!("m{index}")
}

/// The name of a provider's export `index`.
fn field_name(index: usize) -> String {
    format!("f{index}")
}

fn memory_type(limits: Limits) -> MemoryType {
    MemoryType {
        minimum: limits.min,
        maximum: limits.max,
        memory64: false,
        shared: false,
        page_size_log2: None,
    }
}

/// The one function type every module uses: `(i32) -> i32`.
fn signature() -> TypeSection {
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], [ValType::I32]);
    types
}

/// Builds a provider: its exports `f0..`, then the unexported grow helper when
/// some export calls it, then nothing else but its optional memory.
fn provider_module(provider: &ProviderSpec) -> Vec<u8> {
    let export_count = provider.exports.len() as u32;
    let needs_helper = provider.exports.contains(&Body::GrowViaHelper);
    let helper = export_count;

    let mut module = Module::new();
    module.section(&signature());

    let mut functions = FunctionSection::new();
    for _ in 0..export_count + u32::from(needs_helper) {
        functions.function(0);
    }
    module.section(&functions);

    if let Some(limits) = provider.memory {
        let mut memories = MemorySection::new();
        memories.memory(memory_type(limits));
        module.section(&memories);
    }

    let mut exports = ExportSection::new();
    for j in 0..provider.exports.len() {
        exports.export(&field_name(j), ExportKind::Func, j as u32);
    }
    module.section(&exports);

    let mut code = CodeSection::new();
    for &body in &provider.exports {
        code.function(&body_function(body, helper));
    }
    if needs_helper {
        let mut grow = Function::new([]);
        grow.instruction(&Instruction::LocalGet(0));
        grow.instruction(&Instruction::MemoryGrow(0));
        grow.instruction(&Instruction::End);
        code.function(&grow);
    }
    module.section(&code);
    module.finish()
}

fn body_function(body: Body, helper: u32) -> Function {
    let mut function = Function::new([]);
    match body {
        Body::Pure => {
            function.instruction(&Instruction::LocalGet(0));
        }
        Body::Load => {
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::I32Load(MemArg {
                offset: 0,
                align: 2,
                memory_index: 0,
            }));
        }
        Body::Size => {
            function.instruction(&Instruction::MemorySize(0));
        }
        Body::Grow => {
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::MemoryGrow(0));
        }
        Body::GrowViaHelper => {
            function.instruction(&Instruction::LocalGet(0));
            function.instruction(&Instruction::Call(helper));
        }
    }
    function.instruction(&Instruction::End);
    function
}

/// Builds the main module with `spec.imports` declared in `order`: the
/// imports, an optional memory, and an exported `run` that calls each import
/// once, so the merge's call rewrite is exercised in every order.
fn main_module(spec: &LinkSpec, order: &[usize]) -> Vec<u8> {
    let mut module = Module::new();
    module.section(&signature());

    let mut imports = ImportSection::new();
    for &i in order {
        let import = spec.imports[i];
        imports.import(
            &module_name(import.provider),
            &field_name(import.export),
            EntityType::Function(0),
        );
    }
    module.section(&imports);

    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);

    if let Some(limits) = spec.main_memory {
        let mut memories = MemorySection::new();
        memories.memory(memory_type(limits));
        module.section(&memories);
    }

    let run_index = order.len() as u32;
    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, run_index);
    module.section(&exports);

    let mut run = Function::new([]);
    for call in 0..run_index {
        run.instruction(&Instruction::LocalGet(0));
        run.instruction(&Instruction::Call(call));
        run.instruction(&Instruction::Drop);
    }
    run.instruction(&Instruction::LocalGet(0));
    run.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&run);
    module.section(&code);
    module.finish()
}

/// Renders an import order as the main module declares it, `m0.f0 m1.f0`.
fn render_order(spec: &LinkSpec, order: &[usize]) -> String {
    if order.is_empty() {
        return "no imports".to_string();
    }
    order
        .iter()
        .map(|&i| {
            let import = spec.imports[i];
            format!("m{}.f{}", import.provider, import.export)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Checks the round trip and builds the three runs' modules, each validated:
/// [`check`]'s first two properties.
///
/// The runs are the generated import order; the reversed order, with the
/// externals slice reversed too; and the order stably sorted by
/// [`LinkSpec::shuffle`]. Every provider has its own logical module, so the
/// slice order should be inert, and folding its reversal into the second run
/// covers it without a fourth link.
pub fn generate(spec: &LinkSpec) -> Result<Vec<Run>, Violation> {
    let violation = |property: &'static str, detail: String| Violation {
        property,
        detail,
        spec: Box::new(spec.clone()),
        runs: Vec::new(),
    };

    let encoded = encode(spec);
    let decoded = decode(&encoded);
    if decoded != *spec {
        return Err(violation(
            "round trip",
            format!("decode(encode(spec)) gave a different spec: {decoded:#?}"),
        ));
    }

    let providers: Vec<(String, Vec<u8>)> = spec
        .providers
        .iter()
        .enumerate()
        .map(|(i, provider)| (module_name(i), provider_module(provider)))
        .collect();
    for (name, bytes) in &providers {
        if let Err(e) = inf_wasmparser::validate(bytes) {
            return Err(violation(
                "generator",
                format!("the generated provider {name} fails validation: {e}"),
            ));
        }
    }

    let generated: Vec<usize> = (0..spec.imports.len()).collect();
    let reversed: Vec<usize> = generated.iter().rev().copied().collect();
    let mut keyed = generated.clone();
    keyed.sort_by_key(|&i| spec.shuffle[i]);
    let reversed_providers: Vec<(String, Vec<u8>)> = providers.iter().rev().cloned().collect();

    let mut runs = Vec::with_capacity(3);
    for (name, order, externals) in [
        ("in the generated order", generated, providers.clone()),
        ("reversed, externals reversed", reversed, reversed_providers),
        ("sorted by shuffle key", keyed, providers),
    ] {
        let main = main_module(spec, &order);
        let label = format!(
            "{name}: imports {}; externals {}",
            render_order(spec, &order),
            externals
                .iter()
                .map(|(module, _)| module.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
        if let Err(e) = inf_wasmparser::validate(&main) {
            return Err(violation(
                "generator",
                format!("the generated main module, {label}, fails validation: {e}"),
            ));
        }
        runs.push(Run {
            label,
            main,
            externals,
        });
    }
    Ok(runs)
}

/// What one run produced, reduced to what must not depend on the order.
#[derive(Debug)]
enum Observation {
    Linked {
        memory: Option<(u64, Option<u64>)>,
        warnings: Vec<LinkWarning>,
    },
    Refused {
        error: LinkError,
        message: String,
    },
}

impl Observation {
    /// Whether two runs agree on what must not depend on the order: the memory
    /// limits and the warnings, or the error text. Two refusals with the same
    /// text agree even if their error data differs.
    fn agrees_with(&self, other: &Observation) -> bool {
        match (self, other) {
            (
                Observation::Linked { memory, warnings },
                Observation::Linked {
                    memory: other_memory,
                    warnings: other_warnings,
                },
            ) => memory == other_memory && warnings == other_warnings,
            (
                Observation::Refused { message, .. },
                Observation::Refused {
                    message: other_message,
                    ..
                },
            ) => message == other_message,
            _ => false,
        }
    }
}

impl fmt::Display for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Observation::Linked { memory, warnings } => {
                write!(
                    f,
                    "linked, {}, warnings {warnings:?}",
                    render_memory(*memory)
                )
            }
            Observation::Refused { message, .. } => write!(f, "refused: {message}"),
        }
    }
}

fn render_memory(memory: Option<(u64, Option<u64>)>) -> String {
    match memory {
        None => "no memory".to_string(),
        Some((min, Some(max))) => format!("memory ({min}, {max})"),
        Some((min, None)) => format!("memory ({min}, unbounded)"),
    }
}

/// Reduces one run's result to what it observed, checking an `Ok` output on
/// the way: it must validate and declare at most one memory. The error names
/// what was wrong with the output.
fn observe(result: &Result<LinkOutput, LinkError>) -> Result<Observation, String> {
    let out = match result {
        Ok(out) => out,
        Err(error) => {
            return Ok(Observation::Refused {
                message: error.to_string(),
                error: error.clone(),
            });
        }
    };
    inf_wasmparser::validate(&out.wasm)
        .map_err(|e| format!("linked a module that fails validation: {e}"))?;
    let memories = declared_memories(&out.wasm)
        .map_err(|e| format!("linked a module whose memory section does not parse: {e}"))?;
    if memories.len() > 1 {
        return Err(format!(
            "linked a module declaring {} memories",
            memories.len()
        ));
    }
    Ok(Observation::Linked {
        memory: memories.first().copied(),
        warnings: out.warnings.clone(),
    })
}

/// Every memory the module's memory sections declare, as `(min, max)`.
fn declared_memories(bytes: &[u8]) -> Result<Vec<(u64, Option<u64>)>, String> {
    let mut memories = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::MemorySection(reader) = payload.map_err(|e| e.to_string())? {
            for memory in reader {
                let memory = memory.map_err(|e| e.to_string())?;
                memories.push((memory.initial, memory.maximum));
            }
        }
    }
    Ok(memories)
}

/// Links `spec` in three import orders and checks the seven properties the
/// module documentation lists, returning what the links produced.
///
/// # Panics
///
/// When the linker panics, which the fuzz harness reports as a crash, and when
/// `spec` is outside [`encode`]'s domain.
pub fn check(spec: &LinkSpec) -> Result<Outcome, Violation> {
    let runs = generate(spec)?;

    let results: Vec<Result<LinkOutput, LinkError>> = runs
        .iter()
        .map(|run| {
            let externals: Vec<(&str, &[u8])> = run
                .externals
                .iter()
                .map(|(module, bytes)| (module.as_str(), bytes.as_slice()))
                .collect();
            link_with_warnings(&run.main, &externals, None)
        })
        .collect();

    let violation = |property: &'static str, detail: String, rendered: &[String]| Violation {
        property,
        detail,
        spec: Box::new(spec.clone()),
        runs: runs
            .iter()
            .zip(rendered)
            .map(|(run, outcome)| (run.label.clone(), outcome.clone()))
            .collect(),
    };

    // Validity: every `Ok` is a valid module with at most one memory.
    let mut observations = Vec::with_capacity(results.len());
    for (run, result) in runs.iter().zip(&results) {
        match observe(result) {
            Ok(observation) => observations.push(observation),
            Err(detail) => {
                let raw: Vec<String> = results
                    .iter()
                    .map(|result| match result {
                        Ok(out) => format!(
                            "linked ({} bytes), memories {:?}, warnings {:?}",
                            out.wasm.len(),
                            declared_memories(&out.wasm),
                            out.warnings
                        ),
                        Err(e) => format!("refused: {e}"),
                    })
                    .collect();
                return Err(violation(
                    "validity",
                    format!("the run {} {detail}", run.label),
                    &raw,
                ));
            }
        }
    }
    let rendered: Vec<String> = observations.iter().map(ToString::to_string).collect();

    // Invariance: the memory or the error text, and the warnings, are the same
    // in every order.
    let first = &observations[0];
    if let Some(other) = observations.iter().position(|o| !o.agrees_with(first)) {
        return Err(violation(
            "invariance",
            format!(
                "the run {} and the run {} disagree",
                runs[0].label, runs[other].label
            ),
            &rendered,
        ));
    }

    // The oracle, and the warnings.
    let outcome = match (expected(spec), first) {
        (
            Expected::Linked { memory, warnings },
            Observation::Linked {
                memory: got,
                warnings: got_warnings,
            },
        ) => {
            if memory != *got {
                return Err(violation(
                    "oracle",
                    format!(
                        "the documented rule gives {}, but the link produced {}",
                        render_memory(memory),
                        render_memory(*got)
                    ),
                    &rendered,
                ));
            }
            if warnings != *got_warnings {
                return Err(violation(
                    "warnings",
                    format!(
                        "the documented rule gives the warnings {warnings:?}, but the link raised \
                         {got_warnings:?}"
                    ),
                    &rendered,
                ));
            }
            Outcome::Linked { memory, warnings }
        }
        (Expected::Linked { memory, .. }, Observation::Refused { message, .. }) => {
            return Err(violation(
                "oracle",
                format!(
                    "the documented rule links this with {}, but the link was refused: {message}",
                    render_memory(memory)
                ),
                &rendered,
            ));
        }
        (Expected::Refused { applicable }, Observation::Linked { memory, .. }) => {
            return Err(violation(
                "oracle",
                format!(
                    "the documented rule refuses this as {:?}, but the link produced {}",
                    applicable.iter().map(|a| a.kind).collect::<Vec<_>>(),
                    render_memory(*memory)
                ),
                &rendered,
            ));
        }
        (Expected::Refused { applicable }, Observation::Refused { error, message }) => {
            let kind = match_refusal(error, message, &applicable)
                .map_err(|detail| violation("oracle", detail, &rendered))?;
            Outcome::Refused {
                kind,
                message: message.clone(),
            }
        }
    };
    Ok(outcome)
}

/// Finds the one applicable refusal `error` reports: its message carries that
/// kind's wording, its remedy, every number the kind prints and a member of
/// each set the kind names one of, and its `field` is one the kind may name.
fn match_refusal(
    error: &LinkError,
    message: &str,
    applicable: &[Applicable],
) -> Result<Refusal, String> {
    let LinkError::IncompatibleMemory { field, .. } = error else {
        return Err(format!(
            "the documented rule refuses this over memory, but the link failed with another \
             error, which no generated link can produce: {error:?}"
        ));
    };
    let worded: Vec<Refusal> = Refusal::ALL
        .into_iter()
        .filter(|kind| message.contains(kind.wording()))
        .collect();
    let kind = match worded[..] {
        [kind] => kind,
        [] => {
            return Err(format!(
                "the memory refusal carries the wording of no documented refusal; the rule \
                 allows {:?}",
                applicable.iter().map(|a| a.kind).collect::<Vec<_>>()
            ));
        }
        _ => {
            return Err(format!(
                "the memory refusal carries the wording of several refusals: {worded:?}"
            ));
        }
    };
    let Some(applies) = applicable.iter().find(|a| a.kind == kind) else {
        return Err(format!(
            "the link reports {kind:?}, but the documented rule makes only {:?} apply",
            applicable.iter().map(|a| a.kind).collect::<Vec<_>>()
        ));
    };
    if !applies.fields.contains(field) {
        return Err(format!(
            "the link reports {kind:?} for `{field}`, but only {:?} meet its condition",
            applies.fields
        ));
    }
    if let Some(missing) = applies
        .numbers
        .iter()
        .map(String::as_str)
        .chain(kind.remedy())
        .find(|s| !message.contains(s))
    {
        return Err(format!(
            "the link reports {kind:?}, but its message lacks `{missing}`"
        ));
    }
    if let Some(missing) = applies
        .one_of
        .iter()
        .find(|set| !set.iter().any(|s| message.contains(s.as_str())))
    {
        return Err(format!(
            "the link reports {kind:?}, but its message names none of {missing:?}"
        ));
    }
    Ok(kind)
}

/// One satisfied import's contribution to the shared memory.
struct Contribution {
    field: String,
    /// How a refusal names this contributor as the module that shares the
    /// memory: `` `m{p}::f{e}` ``.
    name: String,
    limits: Limits,
    grows: bool,
}

impl Contribution {
    /// How a refusal that moves this grower's cap states the cap its module
    /// declares.
    fn own_cap(&self) -> String {
        match self.limits.max {
            Some(own) => format!("up to {own} pages"),
            None => "declares no maximum".to_string(),
        }
    }
}

/// Renders every contribution that meets `condition`, deduplicated.
fn render_where(
    contributions: &[Contribution],
    condition: impl Fn(&Contribution) -> bool,
    render: impl Fn(&Contribution) -> String,
) -> BTreeSet<String> {
    contributions
        .iter()
        .filter(|c| condition(c))
        .map(render)
        .collect()
}

/// What the documented memory rule says linking `spec` produces. See the
/// module documentation for the rule, and the README's "Linear-Memory
/// Reconciliation" and the book's "The Shared Linear Memory" for its source.
pub fn expected(spec: &LinkSpec) -> Expected {
    // Every satisfied import whose closure touches memory contributes its
    // provider's declared memory, once per import entry. A memoryless
    // provider's bodies are pure, so nothing contributes without a memory.
    let contributions: Vec<Contribution> = spec
        .imports
        .iter()
        .filter_map(|import| {
            let provider = &spec.providers[import.provider];
            let body = provider.exports[import.export];
            let limits = provider.memory?;
            body.uses_memory().then(|| Contribution {
                field: field_name(import.export),
                name: format!(
                    "`{}::{}`",
                    module_name(import.provider),
                    field_name(import.export)
                ),
                limits,
                grows: body.grows(),
            })
        })
        .collect();
    let fields_where = |condition: &dyn Fn(&Contribution) -> bool| {
        render_where(&contributions, condition, |c| c.field.clone())
    };
    let largest_min = contributions.iter().map(|c| c.limits.min).max();

    let mut applicable = Vec::new();
    let memory = match spec.main_memory {
        // Main's maximum governs; its minimum widens to every contributor's.
        Some(main) => {
            let min = largest_min.map_or(main.min, |m| m.max(main.min));
            if let Some(max) = main.max
                && min > max
            {
                applicable.push(Applicable {
                    kind: Refusal::MainTooSmall,
                    fields: fields_where(&|c| c.limits.min == min),
                    numbers: vec![
                        format!("minimum ({min} pages)"),
                        format!("maximum ({max} pages)"),
                    ],
                    one_of: Vec::new(),
                });
            }
            Some((min, main.max))
        }
        // The tightest fit to the contributors' own declarations.
        None => largest_min.map(|min| {
            let max = contributions.iter().filter_map(|c| c.limits.max).min();
            if let Some(max) = max {
                // Both refusals name the module that declares the shared
                // maximum, which may be any contributor tied at it.
                let declaring = render_where(
                    &contributions,
                    |c| c.limits.max == Some(max),
                    |c| c.name.clone(),
                );
                if min > max {
                    applicable.push(Applicable {
                        kind: Refusal::NoFit,
                        fields: fields_where(&|c| c.limits.min == min),
                        numbers: vec![
                            format!("at least {min} pages"),
                            format!("maximum of {max} pages"),
                        ],
                        one_of: vec![declaring.clone()],
                    });
                }
                let moved = |c: &Contribution| c.grows && c.limits.max != Some(max);
                if contributions.iter().any(moved) {
                    applicable.push(Applicable {
                        kind: Refusal::GrowerCapMoved,
                        fields: fields_where(&moved),
                        numbers: vec![format!("maximum of {max} pages")],
                        one_of: vec![
                            declaring,
                            render_where(&contributions, moved, Contribution::own_cap),
                        ],
                    });
                }
            }
            (min, max)
        }),
    };

    // In either mode, growth into a pinned memory is refused.
    let growers = fields_where(&|c| c.grows);
    if let Some((min, Some(max))) = memory
        && max == min
        && !growers.is_empty()
    {
        applicable.push(Applicable {
            kind: Refusal::GrowthPinned,
            fields: growers,
            numbers: vec![
                format!("maximum ({max} pages)"),
                format!("minimum ({min} pages)"),
            ],
            one_of: Vec::new(),
        });
    }

    if !applicable.is_empty() {
        return Expected::Refused { applicable };
    }
    Expected::Linked {
        memory,
        warnings: memory.map_or_else(Vec::new, |(pages, _)| expected_warnings(spec, pages)),
    }
}

/// The `TierBInMultiPageMemory` rule for an output memory of `pages` pages:
/// some import is admitted at Tier B (its body touches memory), and `pages` is
/// above one.
fn expected_warnings(spec: &LinkSpec, pages: u64) -> Vec<LinkWarning> {
    let fields: BTreeSet<String> = spec
        .imports
        .iter()
        .filter(|import| spec.providers[import.provider].exports[import.export].uses_memory())
        .map(|import| field_name(import.export))
        .collect();
    if pages <= 1 || fields.is_empty() {
        return Vec::new();
    }
    vec![LinkWarning::TierBInMultiPageMemory {
        fields: fields.into_iter().collect(),
        pages,
    }]
}

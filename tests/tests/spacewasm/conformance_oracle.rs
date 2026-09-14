//! The conformance checker against the runtime it describes.
//!
//! `inference_target_conformance::spacewasm::check` is a second opinion about a
//! module: it reads the artifact with stock `wasmparser` and compares what it
//! finds against numbers transcribed from the interpreter's source. Transcribed
//! numbers rot, and a checker that is wrong in the permissive direction is worse
//! than no checker at all — it turns a build-time refusal into a load failure on
//! flight hardware, which is the failure the whole target exists to prevent.
//!
//! So every case here is a *pair*: one hand-written module, put in front of
//! `check` and in front of the real decoder, with the two verdicts required to
//! be the same. Each limit is pinned on both sides of its boundary, because a
//! refusal alone is satisfied by a checker that refuses everything.
//!
//! # Where "the real decoder's verdict" needs a host
//!
//! An import is bound at decode time against the host modules the embedder
//! registered, so a module with imports is refused by a decoder offered none —
//! whatever its names say. The accepting rows therefore register a host module,
//! and that registration is itself the thing under test for two of the limits:
//! the 31-byte name cap and the nine-parameter cap are properties of
//! `HostName<31>` and `HostValList`, not of the decoder, and the rows that
//! exceed them assert that the registration cannot even be built.
//!
//! # Depth and height
//!
//! The two verifier bounds are const generics, so "the decoder's verdict" about
//! them is a decode under a tightened configuration. Each of those rows decodes
//! twice — once at the bound `check` reported and once one below it — which is
//! what makes the reported number a measurement rather than a convention. It is
//! also how the *units* are pinned: sixteen `i64` values and sixteen `i32`
//! values differ by a factor of two in words and not at all in values, and the
//! pair below shows the decoder counting the same way `check` reports.
//!
//! # The one side that is not a pair
//!
//! `a_branch_table_width_agrees_at_the_ir_immediate`'s *accepting* side is the
//! single exception to the paragraph above. A table of 65,535 targets is inside
//! the immediate and costs 131,070 IR words, which no plausible embedder's code
//! page budget holds, so there is no verdict to match: that side asserts the
//! absence of the refusal under test rather than an equal verdict, and says so
//! in place.
//!
//! # The one class where the two verdicts differ on purpose
//!
//! Everything above is a transcription: `check` refuses what the decoder
//! refuses, and a disagreement is a defect in the transcription. One class is
//! not, and the rows naming a definition past the interpreter's 16-bit IR word
//! are it. `Module::get_func_ref` and `Module::get_global_ref` subtract the
//! imported count, bounds-check the result against the defined table and then
//! narrow it with `as u16` and no check at all, so a module naming definition
//! 65,536 or beyond **loads** and runs against the definition 65,536 below it.
//! `check` refuses it; the decoder does not; and for a flight target a
//! build-time refusal is better than an artifact that flies and calls the wrong
//! function.
//!
//! Those rows therefore state **both** verdicts explicitly, and where the
//! harness can reach it they state what the decoder does *after* loading — which
//! global it reads, where a store lands, which function a call, an export, a
//! table slot or the `start` section reaches. Five places upstream reach the two
//! accessors, and the checker's model spells them as six arms — the two export
//! kinds by an arm each, and the two global operators through one shared arm and
//! one peak, because upstream reads both through `TextBuilder::get_global` —
//! carrying seven references with a site row each: deleting the pattern that
//! reads a reference, an arm or one alternative of the shared arm's or-pattern,
//! turns that reference's row red and no other reference's.
//!
//! Six rows beside them are about the model rather than a site: the two import
//! shifts, one per space, which are the subtraction the position is taken with;
//! the index that resolves to nothing, which must stay one finding rather than
//! two; the second of two element segments, which is the only shape that tells a
//! counted segment index from a constant; the site naming two references, which
//! is the only shape that tells the largest from the first and the last; and the
//! module naming one definition from all three sections outside a body, which is
//! the only shape that reads back the order the findings arrive in. Every other
//! side of every other row is a pair.

use inference_target_conformance::spacewasm::{
    IndexKind, IndexSpace, MAX_BRANCH_UNWIND_WORDS, MAX_FRAME_WORDS, MAX_HOST_FUNCTION_PARAMS,
    MAX_IMPORT_NAME_BYTES, MAX_IR_INDEX, MAX_LOCAL_WORDS, MAX_LOCALS_GROUP_COUNT,
    MAX_MEMORY_PAGES, MAX_PARAM_WORDS, NamePart, ReferenceSite, Violation, check,
};
use spacewasm::{
    AllocError, GlobalValue, GlobalValueError, HostFunction, HostGlobal, HostModule, HostName,
    HostValList, ValidationError, Value,
};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ElementSection, Elements, EntityType, ExportKind,
    ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    Instruction, Module, RefType, StartSection, TableSection, TableType, TypeSection, ValType,
};

use crate::support::{
    EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH, FUEL, LoadedModule, Outcome,
    SpaceWasmSession, decode, decode_with, decode_with_pages,
};

/// A module with no host module registered, which is every row that declares no
/// import.
fn no_hosts() -> spacewasm::Vec<HostModule> {
    spacewasm::Vec::zero()
}

/// Compiles WAT, panicking with the text rather than with a parse error alone.
fn wat(text: &str) -> Vec<u8> {
    wat::parse_str(text).unwrap_or_else(|e| panic!("the fixture is not valid WAT: {e}\n{text}"))
}

/// Whether the real decoder loads `wasm` under the reference embedder
/// configuration with `hosts` registered.
fn decodes(session: &mut SpaceWasmSession, wasm: &[u8], hosts: spacewasm::Vec<HostModule>) -> bool {
    decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(session, wasm, hosts)
        .is_ok()
}

/// Requires `check` and the real decoder to reach the same verdict on `wasm`.
///
/// `label` names the row so a failure says which limit moved, and `expected`
/// is written out rather than derived so that a row cannot pass by both sides
/// being wrong in the same direction.
fn agree(
    session: &mut SpaceWasmSession,
    label: &str,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
    expected: bool,
) {
    let checked = check(wasm).is_ok();
    let decoded = decodes(session, wasm, hosts);
    assert_eq!(
        checked, expected,
        "{label}: the checker said {checked}, the row says {expected}"
    );
    assert_eq!(
        decoded, expected,
        "{label}: the decoder said {decoded}, the row says {expected}"
    );
}

/// The violations `check` reported, or a panic naming the module that passed.
fn refusals(label: &str, wasm: &[u8]) -> Vec<Violation> {
    check(wasm)
        .err()
        .unwrap_or_else(|| panic!("{label}: the checker accepted a module it must refuse"))
        .as_slice()
        .to_vec()
}

/// A host module named `module` exporting one function named `field` with
/// `params` parameters and no result, registered so an importing module can be
/// decoded.
///
/// # Panics
///
/// Panics if either name or the signature is outside what an embedder can
/// register, which is a property of the row rather than of the module under
/// test — the rows that mean to exceed those caps call the constructors
/// directly and assert on the error.
fn host_exporting(module: &str, field: &str, params: usize) -> spacewasm::Vec<HostModule> {
    let signature = "i".repeat(params);
    let function = HostFunction::new(
        HostName::try_from_str(field).expect("the row's field name is registrable"),
        HostValList::try_new(&signature).expect("the row's signature is registrable"),
        HostValList::new(""),
        |_: &mut spacewasm::Engine, _: &[Value]| core::ops::ControlFlow::Continue(None),
    );
    let module = HostModule {
        name: HostName::try_from_str(module).expect("the row's module name is registrable"),
        globals: spacewasm::Vec::zero(),
        functions: spacewasm::Vec::from_array([function]).expect("one host function allocates"),
        memory: spacewasm::Vec::zero(),
        table: spacewasm::Vec::zero(),
    };
    spacewasm::Vec::from_array([module]).expect("one host module allocates")
}

/// A module whose single function declares `params` parameters of `ty`.
fn module_with_params(ty: &str, params: usize) -> Vec<u8> {
    let list = format!("{ty} ").repeat(params);
    wat(&format!("(module (func (param {list})))"))
}

/// A module whose single `[] -> []` function declares `groups` and then runs
/// `body`, to which the terminating `end` is appended.
///
/// Built with the encoder rather than with WAT because the locals counts here
/// reach tens of thousands, and a locals group is a run-length pair the encoder
/// writes directly while WAT would need one declaration per local.
fn module_with_locals_and_body(groups: &[(u32, ValType)], body: &[Instruction<'_>]) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut code = CodeSection::new();
    let mut function = Function::new(groups.iter().copied());
    for instruction in body {
        function.instruction(instruction);
    }
    function.instruction(&Instruction::End);
    code.function(&function);
    module.section(&code);
    module.finish()
}

/// A module whose single function declares the given locals groups and leaves
/// the operand stack empty.
fn module_with_locals(groups: &[(u32, ValType)]) -> Vec<u8> {
    module_with_locals_and_body(groups, &[])
}

/// A module whose single function declares `groups` and keeps `live` `i64`
/// values on the operand stack at its peak.
///
/// The frame bound is a sum over both halves, so a fixture that reaches it on
/// locals alone leaves the operand half of the sum unmeasured.
fn module_with_locals_and_live_i64(groups: &[(u32, ValType)], live: usize) -> Vec<u8> {
    let mut body = vec![Instruction::I64Const(1); live];
    body.extend(core::iter::repeat_n(Instruction::Drop, live));
    module_with_locals_and_body(groups, &body)
}

/// A module importing `module`.`field` as a function of `params` i32 parameters
/// and `results` i32 results.
fn module_importing(module: &str, field: &str, params: usize, results: usize) -> Vec<u8> {
    let params = "i32 ".repeat(params);
    let results = "i32 ".repeat(results);
    wat(&format!(
        "(module (import \"{module}\" \"{field}\" (func (param {params}) (result {results}))))"
    ))
}

// ---------------------------------------------------------------------------
// Function widths
// ---------------------------------------------------------------------------

/// Parameter words, on both sides of the decoder's one-byte field.
///
/// The `i64` row is what makes this a *word* limit rather than a parameter
/// count: 128 `i64` parameters are 256 words and are refused, while 255 `i32`
/// parameters are accepted — a checker counting parameters would accept the
/// first and a decoder does not.
///
/// Fails if `MAX_PARAM_WORDS` moves without the decoder moving with it, or if
/// the i64 weighting is dropped.
#[test]
fn parameter_words_agree_at_the_boundary() {
    let mut session = SpaceWasmSession::acquire();

    let accepted = module_with_params("i32", MAX_PARAM_WORDS as usize);
    agree(&mut session, "255 i32 parameters", &accepted, no_hosts(), true);
    let report = check(&accepted).expect("255 parameter words pass");
    assert_eq!(
        report.functions[0].param_words, MAX_PARAM_WORDS,
        "the report owes the same word sum the refusal one row down is about"
    );

    let wide_accepted = module_with_params("i64", 127);
    agree(&mut session, "127 i64 parameters", &wide_accepted, no_hosts(), true);
    let report = check(&wide_accepted).expect("254 parameter words pass");
    assert_eq!(
        report.functions[0].param_words, 254,
        "127 i64 parameters are 254 words, and a report counting parameters would say 127"
    );

    let refused = module_with_params("i32", MAX_PARAM_WORDS as usize + 1);
    agree(&mut session, "256 i32 parameters", &refused, no_hosts(), false);
    assert!(
        refusals("256 i32 parameters", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::ParamWordsExceeded { words: 256, .. }
            )),
        "the refusal must name the word count, not the parameter count"
    );

    let wide = module_with_params("i64", 128);
    agree(&mut session, "128 i64 parameters", &wide, no_hosts(), false);
    assert!(
        refusals("128 i64 parameters", &wide)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::ParamWordsExceeded { words: 256, .. }
            )),
        "128 i64 parameters are 256 words, and that is the number the refusal owes"
    );
}

/// Local words, on both sides of the boundary that actually bites.
///
/// Two bounds meet here, and which of them a function reaches first is the
/// thing this row exists to keep straight. The decoder holds a function's local
/// size in a 16-bit field, so 65,535 words is the most that field can say — but
/// it also holds the whole *call frame* in a 16-bit length, and the frame is
/// two words of header plus the locals plus the operand stack. So a function
/// with an empty operand stack may declare 65,533 local words and no more, and
/// a checker enforcing only the local-size field would accept two declarations
/// the decoder refuses. That is the permissive direction, which is the one that
/// turns a build-time refusal into a load failure in flight.
///
/// All three sides are built out of `i64` locals so the *local count* stays far
/// below the 50,000 a stock validator admits: the boundaries under test are word
/// sums, and a fixture that tripped a validator limit on the way there would be
/// measuring the wrong thing.
///
/// Fails if either bound is dropped, if the header's two words stop being
/// counted, or if the word weighting is dropped on locals.
#[test]
fn local_words_agree_at_the_boundary() {
    let mut session = SpaceWasmSession::acquire();

    // 32766 × 2 + 1 = 65533 words, which is 65535 once the header is added.
    let accepted = module_with_locals(&[(32_766, ValType::I64), (1, ValType::I32)]);
    agree(&mut session, "65533 local words", &accepted, no_hosts(), true);
    let report = check(&accepted).expect("65533 local words pass");
    assert_eq!(
        (
            report.functions[0].local_words,
            report.functions[0].frame_words()
        ),
        (65_533, MAX_FRAME_WORDS as u32),
        "the report has to say both numbers the boundary is about"
    );

    // 32767 × 2 = 65534 words: inside the local-size field, past the frame.
    let over_frame = module_with_locals(&[(32_767, ValType::I64)]);
    agree(&mut session, "65534 local words", &over_frame, no_hosts(), false);
    let found = refusals("65534 local words", &over_frame);
    assert!(
        found.iter().any(|violation| matches!(
            violation,
            Violation::FrameWordsExceeded { words: 65_536, .. }
        )),
        "the refusal must be about the frame, which is what the decoder refused: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|violation| matches!(violation, Violation::LocalWordsExceeded { .. })),
        "65534 is inside the local-size field, so blaming that field would send the reader \
         to the wrong number: {found:?}"
    );

    // 32768 × 2 = 65536 words: past both.
    let over_field = module_with_locals(&[(32_768, ValType::I64)]);
    agree(&mut session, "65536 local words", &over_field, no_hosts(), false);
    assert!(
        refusals("65536 local words", &over_field)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::LocalWordsExceeded { words: 65_536, .. }
            )),
        "past the local-size field the refusal must name that field too"
    );
    assert_eq!(MAX_LOCAL_WORDS, 65_535, "the field bound is what it says it is");
}

/// The frame bound counts the operand stack, on both sides of its boundary.
///
/// The rows above cross the frame bound on locals alone, with nothing on the
/// operand stack — so the `2 + locals + operands` sum could have been a
/// `2 + locals` one and every one of them would still pass. The operand half
/// is this crate's own reconstruction of a figure the interpreter's verifier
/// computes for itself, and it is the half that carries the word weighting, so
/// it is the half most likely to drift. Here the locals sit two words under the
/// bound and the operand stack is what crosses it.
///
/// Fails if the operand peak stops contributing to the frame, if it stops being
/// weighted by width, or if this crate's reconstruction drifts from the
/// verifier's own high-water mark.
#[test]
fn the_frame_bound_counts_the_operand_stack_too() {
    let mut session = SpaceWasmSession::acquire();

    // 32760 × 2 = 65520 local words, which is 65522 with the header: six live
    // i64 values are 12 more, landing on 65534.
    let accepted = module_with_locals_and_live_i64(&[(32_760, ValType::I64)], 6);
    agree(
        &mut session,
        "65520 local words under six live i64",
        &accepted,
        no_hosts(),
        true,
    );
    let report = check(&accepted).expect("65534 frame words pass");
    assert_eq!(
        (
            report.functions[0].max_operand_values,
            report.functions[0].max_operand_words,
            report.functions[0].frame_words()
        ),
        (6, 12, MAX_FRAME_WORDS as u32 - 1),
        "six live i64 values are six values and twelve words, and the frame is one under \
         the bound"
    );

    // One more value, and only the operand half has moved.
    let refused = module_with_locals_and_live_i64(&[(32_760, ValType::I64)], 7);
    agree(
        &mut session,
        "65520 local words under seven live i64",
        &refused,
        no_hosts(),
        false,
    );
    let found = refusals("65520 local words under seven live i64", &refused);
    assert!(
        found.iter().any(|violation| matches!(
            violation,
            Violation::FrameWordsExceeded {
                words: 65_536,
                local_words: 65_520,
                operand_words: 14,
                ..
            }
        )),
        "the refusal must break the frame into the two halves it is a sum of: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|violation| matches!(violation, Violation::LocalWordsExceeded { .. })),
        "the locals are inside their own field; blaming it would send the reader to the \
         wrong half of the sum: {found:?}"
    );
}

/// A module whose single function declares `groups` and then leaves one operand
/// of untracked type on the stack.
///
/// `unreachable` makes the enclosing frame polymorphic, and `select` in a
/// polymorphic frame yields a value whose type neither validator knows. The
/// trailing `drop` is what lets the body end: the operand is counted at the
/// sample taken after `select`, and a value left standing at `end` would be a
/// type error rather than a measurement.
fn module_with_locals_and_an_untracked_operand(groups: &[(u32, ValType)]) -> Vec<u8> {
    module_with_locals_and_body(
        groups,
        &[Instruction::Unreachable, Instruction::Select, Instruction::Drop],
    )
}

/// An operand whose type is no longer tracked costs the frame nothing.
///
/// This is the one figure in the report that a reasonable checker could get
/// wrong in the *safe* direction and still be wrong: after `unreachable` a
/// slot's width is genuinely unknown, and rounding it up to two words looks
/// conservative. It is not, because the interpreter's verifier stops
/// accumulating at the first untracked slot and the sum it produces is what its
/// own frame bound is checked against. A checker that rounded up would refuse
/// this module, which the decoder loads.
///
/// The locals are placed so the frame lands exactly on the bound with the
/// operand contributing nothing: 65,533 words plus the two-word header. Any
/// width at all attributed to the untracked operand pushes the sum over.
///
/// Fails if the untracked slot is given a width, or if the accepting row stops
/// sitting on the boundary — the companion row one local higher is refused by
/// both sides, which is what proves the accepting row is not merely far from
/// the bound.
#[test]
fn an_untracked_operand_costs_the_frame_nothing() {
    let mut session = SpaceWasmSession::acquire();

    // 32766 × 2 + 1 = 65533 local words, the same boundary the rows above
    // use, split across two groups so the locals *count* stays inside the
    // validator's own per-function cap.
    let accepted =
        module_with_locals_and_an_untracked_operand(&[(32_766, ValType::I64), (1, ValType::I32)]);
    agree(
        &mut session,
        "65533 local words under an untracked operand",
        &accepted,
        no_hosts(),
        true,
    );
    let report = check(&accepted).expect("65533 local words and no operand words pass");
    assert_eq!(
        (
            report.functions[0].max_operand_values,
            report.functions[0].max_operand_words,
            report.functions[0].frame_words()
        ),
        (1, 0, MAX_FRAME_WORDS as u32),
        "the untracked operand is one value and no words, and the frame is on the bound"
    );

    let refused = module_with_locals_and_an_untracked_operand(&[(32_767, ValType::I64)]);
    agree(
        &mut session,
        "65534 local words under an untracked operand",
        &refused,
        no_hosts(),
        false,
    );
}

/// One locals group wider than the decoder's 16-bit run length.
///
/// A group above the cap is necessarily above the word cap too — every local is
/// at least one word — so this row also carries a `LocalWordsExceeded`, and the
/// assertion is that the group violation is *present*, not that it is alone.
///
/// Fails if the group count stops being read, which would leave the shape
/// refused only by the word sum and silent about which declaration is wrong.
#[test]
fn an_oversized_locals_group_is_refused_by_both() {
    let mut session = SpaceWasmSession::acquire();
    let refused = module_with_locals(&[(MAX_LOCALS_GROUP_COUNT + 1, ValType::I32)]);
    agree(&mut session, "a 65536-local group", &refused, no_hosts(), false);
    assert!(
        refusals("a 65536-local group", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::LocalsGroupTooLarge { count: 65_536, .. }
            )),
        "the refusal must name the group, or a reader cannot tell which declaration to split"
    );
}

// ---------------------------------------------------------------------------
// The interpreter's own compiled IR
// ---------------------------------------------------------------------------

/// The verifier configuration these rows decode under.
///
/// Wider than the reference embedder's on the operand axis, because the shapes
/// below keep 256 values live on purpose and `spacewasm_std`'s stack holds
/// exactly 256: a row meant to be refused for its *branch* would otherwise be
/// refused for the stack it stands on, and a row meant to be accepted would not
/// decode at all. The control axis is the reference one — none of these nests.
const WIDE_STACK: usize = 300;

/// Whether the real decoder loads `wasm` with room for the operands these rows
/// keep live.
fn decodes_wide(session: &mut SpaceWasmSession, wasm: &[u8]) -> Result<(), ValidationError> {
    decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, WIDE_STACK>(session, wasm, no_hosts())
        .map(|_| ())
        .map_err(|refusal| refusal.err.err)
}

/// Requires `check` and the real decoder to agree about `wasm` under
/// [`WIDE_STACK`], and — when they refuse — requires the decoder's reason to be
/// `expected_error`.
///
/// The reason is asserted rather than the verdict alone because every shape here
/// is large or deep, and a decoder refusing it for a page budget or a stack
/// bound would satisfy a verdict-only row while saying nothing about the limit
/// under test.
fn agree_wide(
    session: &mut SpaceWasmSession,
    label: &str,
    wasm: &[u8],
    expected_error: Option<ValidationError>,
) {
    let checked = check(wasm).is_ok();
    let decoded = decodes_wide(session, wasm);
    assert_eq!(
        checked,
        expected_error.is_none(),
        "{label}: the checker said {checked}, the row expects {expected_error:?}"
    );
    match expected_error {
        None => assert_eq!(decoded, Ok(()), "{label}: the decoder must load this module"),
        Some(expected) => assert_eq!(
            decoded,
            Err(expected),
            "{label}: the decoder must refuse this module for the limit under test"
        ),
    }
}

/// A module declaring `types` identical `[] -> []` types, a one-entry table, and
/// one function performing `call_indirect` against type `type_index`.
///
/// The element segment is written in the legacy form (no explicit table index),
/// which is the only one the interpreter reads: it takes the first byte of the
/// segment as a table index, so the flag byte of any newer encoding reads as a
/// table this module does not have.
fn module_with_types_and_call_indirect(types_count: u32, type_index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    for _ in 0..types_count {
        types.ty().function([], []);
    }
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        table64: false,
        minimum: 1,
        maximum: Some(1),
        shared: false,
    });
    module.section(&tables);
    let mut elements = ElementSection::new();
    elements.active(
        None,
        &ConstExpr::i32_const(0),
        Elements::Functions([0u32].as_slice().into()),
    );
    module.section(&elements);
    let mut code = CodeSection::new();
    let mut function = Function::new([]);
    function.instruction(&Instruction::I32Const(0));
    function.instruction(&Instruction::CallIndirect {
        type_index,
        table_index: 0,
    });
    function.instruction(&Instruction::End);
    code.function(&function);
    module.section(&code);
    module.finish()
}

/// A `call_indirect` type index, on both sides of the interpreter's 16-bit IR
/// immediate.
///
/// Nothing about this is visible in the WebAssembly encoding, which carries the
/// index as a 32-bit LEB and validates it against the type section alone: both
/// modules below are valid WebAssembly 1.0 and stock validation accepts each.
/// What the interpreter does with them differs, because it compiles the module
/// into a bytecode that holds the index in sixteen bits.
///
/// The 65,535 row is the reason this instruction is the one the boundary is
/// pinned on rather than the `br_table` next to it: both sides of it decode, so
/// the refusal is shown to be about the index and not about the size of the
/// module carrying it.
///
/// Fails if `MAX_IR_INDEX` moves without the interpreter moving with it, if the
/// index stops being read from the instruction, or if the finding stops naming
/// which of the two immediates it is about.
#[test]
fn a_call_indirect_type_index_agrees_at_the_ir_immediate() {
    let mut session = SpaceWasmSession::acquire();
    assert_eq!(
        MAX_IR_INDEX, 65_535,
        "the rows below are built from this constant, so narrowing it would shrink them \
         rather than fail them"
    );

    let accepted = module_with_types_and_call_indirect(MAX_IR_INDEX + 1, MAX_IR_INDEX);
    agree_wide(&mut session, "call_indirect against type 65535", &accepted, None);

    let refused = module_with_types_and_call_indirect(MAX_IR_INDEX + 2, MAX_IR_INDEX + 1);
    agree_wide(
        &mut session,
        "call_indirect against type 65536",
        &refused,
        Some(ValidationError::IdxTooLarge),
    );
    assert!(
        refusals("call_indirect against type 65536", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::IndexTooLarge {
                    kind: IndexKind::CallIndirectType,
                    index: 65_536,
                    ..
                }
            )),
        "the refusal must name the immediate and the value that did not fit"
    );
}

// ---------------------------------------------------------------------------
// The one index the interpreter narrows without checking
// ---------------------------------------------------------------------------

/// What a module's first defined global holds in the rows below.
const FIRST_GLOBAL: i32 = 7;

/// What every other defined global holds.
const OTHER_GLOBAL: i32 = 9;

/// What the imported global in the import-shift row holds.
const IMPORTED_GLOBAL: i32 = 11;

/// What the `global.set` row writes, which is none of the initial values, so a
/// read afterwards says which global the write landed on.
const WRITTEN_GLOBAL: i32 = 13;

/// The witness definition 0 of a function-index module writes.
const FIRST_FUNCTION: i32 = 21;

/// The witness the definition at 65,535 writes.
const OTHER_FUNCTION: i32 = 23;

/// The witness the definition at 65,536 writes.
///
/// Three distinct witnesses, because a row has to tell three outcomes apart:
/// the named definition ran, the one 65,536 below it ran, or neither did.
const LAST_FUNCTION: i32 = 27;

/// IR pages the function-index rows decode under.
///
/// `spacewasm_std`'s budget does not hold 65,537 function bodies, and the rows
/// below measure that rather than assume it: at its 256 pages — 65,536 IR words
/// — the decoder refuses in the code section with `AllocError(OutOfMemory)`, and
/// 257 is the smallest budget measured to load the module, which is the answer
/// to expect when an empty body compiles to one word and there are 65,537 of
/// them. This is four times that, so a change to one row's body shape surfaces
/// as that row's own failure rather than as a page-budget refusal in the row
/// beside it. The budget is an embedder's choice
/// (`CompilerOptions::max_code_pages`), so widening it describes a larger
/// flight computer and narrows nothing else: every other limit is unchanged.
const TRUNCATION_ROW_CODE_PAGES: usize = 1024;

/// A module declaring `imported` imported `i32` globals and `defined` of its
/// own, exporting a `[] -> [i32]` function `get` that returns
/// `global.get index`.
///
/// Defined global 0 holds [`FIRST_GLOBAL`] and every other holds
/// [`OTHER_GLOBAL`], so which global a call actually read can be told from the
/// value it returns. The imported ones hold [`IMPORTED_GLOBAL`] and exist to
/// shift the defined ones: the interpreter narrows the index *after* taking the
/// imported count off it, so the same `index` names a different definition
/// depending on how many imports precede it.
fn module_with_globals_and_getter(imported: u32, defined: u32, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    if imported > 0 {
        let mut imports = ImportSection::new();
        for i in 0..imported {
            imports.import("h", &format!("g{i}"), immutable_i32());
        }
        module.section(&imports);
    }
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    module.section(&globals_holding(defined, false));
    let mut exports = ExportSection::new();
    exports.export("get", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut function = Function::new([]);
    function.instruction(&Instruction::GlobalGet(index));
    function.instruction(&Instruction::End);
    code.function(&function);
    module.section(&code);
    module.finish()
}

/// A module declaring `defined` mutable `i32` globals, an exported `set` that
/// writes [`WRITTEN_GLOBAL`] to global `index`, and an exported `peek` that
/// returns global 0.
///
/// `peek` is what makes the write observable: nothing else reports where a
/// `global.set` landed, and the whole finding is that it lands somewhere other
/// than where the instruction says.
fn module_with_global_setter(defined: u32, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(1);
    module.section(&functions);
    module.section(&globals_holding(defined, true));
    let mut exports = ExportSection::new();
    exports.export("set", ExportKind::Func, 0);
    exports.export("peek", ExportKind::Func, 1);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut setter = Function::new([]);
    setter.instruction(&Instruction::I32Const(WRITTEN_GLOBAL));
    setter.instruction(&Instruction::GlobalSet(index));
    setter.instruction(&Instruction::End);
    code.function(&setter);
    let mut peek = Function::new([]);
    peek.instruction(&Instruction::GlobalGet(0));
    peek.instruction(&Instruction::End);
    code.function(&peek);
    module.section(&code);
    module.finish()
}

/// A module importing `imported` `i32` globals from `h`, declaring `defined` of
/// its own and exporting global `index` under the name `g`, with no function at
/// all.
///
/// No body, deliberately: the export descriptor reaches the same narrowing
/// accessor a `global.get` does, and a module carrying both would not say which
/// of the two the refusal came from.
fn module_exporting_global(imported: u32, defined: u32, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    if imported > 0 {
        let mut imports = ImportSection::new();
        for i in 0..imported {
            imports.import("h", &format!("g{i}"), immutable_i32());
        }
        module.section(&imports);
    }
    module.section(&globals_holding(defined, false));
    let mut exports = ExportSection::new();
    exports.export("g", ExportKind::Global, index);
    module.section(&exports);
    module.finish()
}

/// A global section of `defined` `i32` globals, the first holding
/// [`FIRST_GLOBAL`] and the rest [`OTHER_GLOBAL`].
fn globals_holding(defined: u32, mutable: bool) -> GlobalSection {
    let mut globals = GlobalSection::new();
    for i in 0..defined {
        let value = if i == 0 { FIRST_GLOBAL } else { OTHER_GLOBAL };
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable,
                shared: false,
            },
            &ConstExpr::i32_const(value),
        );
    }
    globals
}

/// The type of an immutable `i32` global, as an import declares it.
fn immutable_i32() -> GlobalType {
    GlobalType {
        val_type: ValType::I32,
        mutable: false,
        shared: false,
    }
}

/// A host module named `module` exporting one immutable `i32` global named
/// `field` and holding [`IMPORTED_GLOBAL`], so a module importing it decodes.
fn host_with_global(module: &str, field: &str) -> spacewasm::Vec<HostModule> {
    let global = HostGlobal {
        name: HostName::try_from_str(field).expect("the row's global name is registrable"),
        value: spacewasm::Box::new(ImportedGlobal)
            .expect("one host global allocates")
            .into_global_value_dyn(),
    };
    let module = HostModule {
        name: HostName::try_from_str(module).expect("the row's module name is registrable"),
        globals: spacewasm::Vec::from_array([global]).expect("one host global allocates"),
        functions: spacewasm::Vec::zero(),
        memory: spacewasm::Vec::zero(),
        table: spacewasm::Vec::zero(),
    };
    spacewasm::Vec::from_array([module]).expect("one host module allocates")
}

/// The immutable `i32` an embedder supplies for the import-shift row.
struct ImportedGlobal;

impl GlobalValue for ImportedGlobal {
    fn write(&self, _value: Value) -> Result<(), GlobalValueError> {
        Err(GlobalValueError)
    }

    fn read(&self) -> Result<Value, GlobalValueError> {
        Ok(Value::I32(IMPORTED_GLOBAL))
    }

    fn ty(&self) -> spacewasm::ValType {
        spacewasm::ValType::I32
    }

    fn mutable(&self) -> bool {
        false
    }
}

/// Where a module names a defined function, other than a body's global operand.
///
/// Four places, and a fifth shape: the two element variants are the same place
/// read twice, one segment and two, because the finding reports *which* segment
/// and a module carrying only one cannot tell that number from a constant.
#[derive(Debug, Clone, Copy)]
enum FunctionRef {
    /// The `call` in the exported `run`.
    Call,
    /// An export descriptor, under the name `wrong`.
    Export,
    /// The one entry of the one element segment, reached by the `call_indirect`
    /// in the exported `run`.
    ElementEntry,
    /// The entry of the **second** of two element segments, reached the same
    /// way through table slot 1. A module may carry several, and the finding
    /// names which — a number a single-segment fixture cannot tell from a
    /// constant.
    LaterElementEntry,
    /// The `start` section.
    Start,
}

impl FunctionRef {
    /// Whether this site needs a table, and how many slots the segments fill.
    fn table_slots(self) -> Option<u64> {
        match self {
            Self::ElementEntry => Some(1),
            Self::LaterElementEntry => Some(2),
            Self::Call | Self::Export | Self::Start => None,
        }
    }
}

/// A module of `defined` functions that names definition `index` from `site`.
///
/// Definition 0 writes [`FIRST_FUNCTION`] to the module's one global,
/// definition `defined - 2` writes [`OTHER_FUNCTION`] and definition
/// `defined - 1` writes [`LAST_FUNCTION`]; definition 1 is the exported `peek`
/// that returns that global, definition 2 is the exported `run` that drives the
/// site where a site needs driving, and every other body is empty so the
/// module costs the interpreter as little IR as the shape allows.
///
/// Three witnesses and not two, because the outcome to rule out is not only
/// "the named definition ran": a row that saw the wrong witness without knowing
/// the third value would not separate the definition 65,536 below from one that
/// never ran at all.
fn module_with_functions_naming(defined: u32, site: FunctionRef, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    for i in 0..defined {
        functions.function(u32::from(i == 1));
    }
    module.section(&functions);
    if let Some(slots) = site.table_slots() {
        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            table64: false,
            minimum: slots,
            maximum: Some(slots),
            shared: false,
        });
        module.section(&tables);
    }
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("peek", ExportKind::Func, 1);
    match site {
        FunctionRef::Call | FunctionRef::ElementEntry | FunctionRef::LaterElementEntry => {
            exports.export("run", ExportKind::Func, 2);
        }
        FunctionRef::Export => {
            exports.export("wrong", ExportKind::Func, index);
        }
        FunctionRef::Start => {}
    }
    module.section(&exports);
    if matches!(site, FunctionRef::Start) {
        module.section(&StartSection {
            function_index: index,
        });
    }
    if site.table_slots().is_some() {
        let mut elements = ElementSection::new();
        if matches!(site, FunctionRef::LaterElementEntry) {
            // Slot 0 is filled from a segment of its own, and by the definition
            // at 65,535 rather than by definition 0: a `run` that reached the
            // wrong slot would then answer with the wrong witness instead of
            // with the one the truncation produces.
            elements.active(
                None,
                &ConstExpr::i32_const(0),
                Elements::Functions([defined - 2].as_slice().into()),
            );
        }
        let offset = i32::from(matches!(site, FunctionRef::LaterElementEntry));
        elements.active(
            None,
            &ConstExpr::i32_const(offset),
            Elements::Functions([index].as_slice().into()),
        );
        module.section(&elements);
    }
    let mut code = CodeSection::new();
    for i in 0..defined {
        let mut function = Function::new([]);
        if i == 0 {
            writes_witness(&mut function, FIRST_FUNCTION);
        } else if i == 1 {
            function.instruction(&Instruction::GlobalGet(0));
        } else if i == 2 {
            match site {
                FunctionRef::Call => function.instruction(&Instruction::Call(index)),
                FunctionRef::ElementEntry => function
                    .instruction(&Instruction::I32Const(0))
                    .instruction(&Instruction::CallIndirect {
                        type_index: 0,
                        table_index: 0,
                    }),
                FunctionRef::LaterElementEntry => function
                    .instruction(&Instruction::I32Const(1))
                    .instruction(&Instruction::CallIndirect {
                        type_index: 0,
                        table_index: 0,
                    }),
                FunctionRef::Export | FunctionRef::Start => &mut function,
            };
        } else if i + 2 == defined {
            writes_witness(&mut function, OTHER_FUNCTION);
        } else if i + 1 == defined {
            writes_witness(&mut function, LAST_FUNCTION);
        }
        function.instruction(&Instruction::End);
        code.function(&function);
    }
    module.section(&code);
    module.finish()
}

/// Appends the two instructions that record `witness` in the module's global.
fn writes_witness(function: &mut Function, witness: i32) {
    function.instruction(&Instruction::I32Const(witness));
    function.instruction(&Instruction::GlobalSet(0));
}

/// The one truncation finding `check` made about `wasm`, and nothing else.
///
/// `check` is required to have found exactly one violation, which is what makes
/// each row a statement about the index rather than about the size of the
/// module carrying it: every fixture here declares tens of thousands of
/// definitions, and a row satisfied by "refused somehow" would pass on a
/// checker that refused them for being large.
fn only_truncation(label: &str, wasm: &[u8]) -> (IndexSpace, ReferenceSite, u32, u32) {
    let found = refusals(label, wasm);
    let [Violation::IndexTruncated {
        space,
        site,
        index,
        position,
    }] = found.as_slice()
    else {
        panic!("{label}: the one finding must be the truncation, got {found:?}");
    };
    (*space, site.clone(), *index, *position)
}

/// The rendered truncation finding for `wasm`, required to predict definition 0.
///
/// The tail of that sentence is arithmetic the message does itself — the
/// position modulo the 16-bit word — and every row below measures what the
/// decoder actually reaches. Pinning the prediction here is what ties the two
/// together: a message that named a definition other than the one the decoder
/// runs would send a reader to the wrong function with both halves of the row
/// still green.
fn predicted_definition(label: &str, wasm: &[u8]) -> String {
    let rendered = refusals(label, wasm)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("resolves to definition 0 instead"),
        "{label}: the finding must say which definition the reference reaches: {rendered}"
    );
    rendered
}

/// The `i32` an export answers with, or a panic naming what it did instead.
fn answered_i32(loaded: &mut LoadedModule<'_>, export: &str) -> i32 {
    match loaded.invoke(export, &[], FUEL) {
        Outcome::Value(Some(Value::I32(value))) => value,
        other => panic!("`{export}` must answer with an i32: {other:?}"),
    }
}

/// What the exported `get` of a global module answers with under the real
/// decoder, with `hosts` registered.
fn value_read_by_get(
    session: &mut SpaceWasmSession,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
) -> i32 {
    let mut loaded =
        decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(session, wasm, hosts)
            .expect("the decoder loads this module");
    answered_i32(&mut loaded, "get")
}

/// Loads a function-index module under a budget that holds its IR, runs
/// `drive` where the site needs driving, and answers with the witness left by
/// whichever definition actually ran.
fn witness_after(session: &mut SpaceWasmSession, wasm: &[u8], drive: Option<&str>) -> i32 {
    let mut loaded = decode_with_pages::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(
        session,
        wasm,
        no_hosts(),
        TRUNCATION_ROW_CODE_PAGES,
    )
    .expect("the decoder loads 65,537 function bodies under this page budget");
    if let Some(export) = drive {
        assert_eq!(
            loaded.invoke(export, &[], FUEL),
            Outcome::Value(None),
            "`{export}` must run to completion"
        );
    }
    answered_i32(&mut loaded, "peek")
}

/// The body of a function names a defined global past the 16-bit IR word: `check`
/// refuses, the decoder loads and reads the wrong global.
///
/// This is the one row in this suite whose two verdicts differ on purpose, and
/// both are stated in full because the difference is the finding. 0.7.1 narrows
/// a defined global's position with `as u16` and no check
/// (`Module::get_global_ref`), so the module loads and the `global.get` reads
/// the global 65,536 below the one it names. Agreeing with the decoder would
/// mean accepting an artifact that flies and reads the wrong word, which for
/// this target is worse than a build-time refusal.
///
/// Three modules, because the boundary alone would not separate the fault from
/// the module's size: the same 65,537 globals with the reference at 65,535 load
/// and answer correctly, and with the reference at 0 they load too. Declaring
/// the globals is not the fault; naming one past the cap is.
///
/// Fails if `check` stops refusing the over side, if it starts refusing either
/// control, if the finding stops carrying the index, the position or the body it
/// is in — or the day 0.7.1 refuses instead of truncating, which is the day this
/// row stops being a deliberate disagreement and becomes a pair.
#[test]
fn a_global_named_past_the_ir_immediate_is_refused_here_and_truncated_there() {
    let mut session = SpaceWasmSession::acquire();
    assert_eq!(
        MAX_IR_INDEX, 65_535,
        "the rows below are built from this constant, so narrowing it would shrink them \
         rather than fail them"
    );
    let declared = MAX_IR_INDEX + 2;

    let inside = module_with_globals_and_getter(0, declared, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "a global at the last position the cast leaves alone is inside every limit"
    );
    assert_eq!(
        value_read_by_get(&mut session, &inside, no_hosts()),
        OTHER_GLOBAL,
        "global 65535 is the one named and the one that must be read"
    );

    let over = module_with_globals_and_getter(0, declared, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("global.get 65536", &over),
        (
            IndexSpace::Global,
            ReferenceSite::Body {
                function: String::from("func[0]")
            },
            65_536,
            65_536
        ),
        "the finding names the space, the body, the index and the position it narrows"
    );
    assert!(
        predicted_definition("global.get 65536", &over)
            .contains("names global 65536, which is definition 65536"),
        "the finding must name the space, the written index and the position in that order"
    );
    assert_eq!(
        value_read_by_get(&mut session, &over, no_hosts()),
        FIRST_GLOBAL,
        "65536 narrowed to u16 is 0, so the decoder reads global 0 rather than the one named"
    );

    let unreferenced = module_with_globals_and_getter(0, declared, 0);
    assert!(
        check(&unreferenced).is_ok(),
        "declaring 65,537 globals is not the fault; naming one past the cap is"
    );
    assert_eq!(
        value_read_by_get(&mut session, &unreferenced, no_hosts()),
        FIRST_GLOBAL,
        "the control module runs, so the refusal above is about the reference alone"
    );
}

/// An imported global shifts where the truncation begins, and the refusal
/// follows it.
///
/// The interpreter subtracts the imported count *before* it narrows
/// (`Module::get_global_ref`), so the index a module writes and the position the
/// cast sees are two different numbers whenever the module imports anything.
/// The pair is the same written index, 65,536, under two modules that differ
/// only by one import: without it the index is position 65,536 and refused, with
/// it the index is position 65,535 and accepted — and the accepted one is run,
/// so the row states that the shift is not merely tolerated but correct.
///
/// Fails if the subtraction is dropped, which turns the accepted side red, and
/// if it is applied twice or to the wrong space, which turns the refused side
/// green.
#[test]
fn an_imported_global_shifts_where_the_truncation_begins() {
    let mut session = SpaceWasmSession::acquire();
    let declared = MAX_IR_INDEX + 2;

    let unshifted = module_with_globals_and_getter(0, declared, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("global.get 65536 with no import", &unshifted),
        (
            IndexSpace::Global,
            ReferenceSite::Body {
                function: String::from("func[0]")
            },
            65_536,
            65_536
        ),
        "with no import, index 65536 is definition 65536 and is refused for the narrowing \
         rather than for anything else the module is"
    );

    let shifted = module_with_globals_and_getter(1, declared, MAX_IR_INDEX + 1);
    assert!(
        check(&shifted).is_ok(),
        "with one imported global, index 65536 is definition 65535 and fits"
    );
    assert_eq!(
        value_read_by_get(&mut session, &shifted, host_with_global("h", "g0")),
        OTHER_GLOBAL,
        "the shifted reference is not merely accepted: it reads the global it names"
    );

    let past = module_with_globals_and_getter(1, declared, MAX_IR_INDEX + 2);
    assert_eq!(
        only_truncation("global.get 65537 behind one import", &past),
        (
            IndexSpace::Global,
            ReferenceSite::Body {
                function: String::from("func[0]")
            },
            65_537,
            65_536
        ),
        "the finding reports the written index and the position it resolves to, which \
         differ by the imported count"
    );
    assert_eq!(
        value_read_by_get(&mut session, &past, host_with_global("h", "g0")),
        FIRST_GLOBAL,
        "definition 65536 narrows to definition 0, whatever the written index was"
    );
}

/// A `global.set` reaches the same narrowing a `global.get` does, and the write
/// lands on the wrong global.
///
/// Both operators resolve through one accessor upstream
/// (`TextBuilder::get_global`), so the model holds them in one peak — and a
/// model that read only the operator it was written for would leave every
/// store unchecked. The write is observed rather than the refusal alone: the
/// module reads global 0 back afterwards, so the row says where the store went
/// and not merely that it was refused here.
///
/// Fails if `global.set` stops being read, and fails on the control if the
/// refusal starts firing at 65,535.
#[test]
fn a_global_set_is_narrowed_where_a_global_get_is() {
    let mut session = SpaceWasmSession::acquire();
    let declared = MAX_IR_INDEX + 2;

    let inside = module_with_global_setter(declared, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "a store to the last position the cast leaves alone is inside every limit"
    );
    let mut loaded = decode(&mut session, &inside).expect("the decoder loads this module");
    assert_eq!(loaded.invoke("set", &[], FUEL), Outcome::Value(None));
    assert_eq!(
        answered_i32(&mut loaded, "peek"),
        FIRST_GLOBAL,
        "a store to global 65535 must leave global 0 as it was"
    );
    drop(loaded);

    let over = module_with_global_setter(declared, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("global.set 65536", &over),
        (
            IndexSpace::Global,
            ReferenceSite::Body {
                function: String::from("func[0]")
            },
            65_536,
            65_536
        ),
        "the finding names the body the store is in"
    );
    let mut loaded = decode(&mut session, &over).expect("0.7.1 loads the module all the same");
    assert_eq!(loaded.invoke("set", &[], FUEL), Outcome::Value(None));
    assert_eq!(
        answered_i32(&mut loaded, "peek"),
        WRITTEN_GLOBAL,
        "the store named global 65536 and landed on global 0"
    );
}

/// An export descriptor names a defined global, and reaches the same narrowing
/// an instruction does.
///
/// The module carries no function at all, so nothing but the descriptor can
/// have earned the refusal. The decoder's side is the verdict alone: an
/// embedder reads an exported global through its own API and this harness
/// exposes only exported functions, so what the truncated descriptor points at
/// is not observable here — which the row states rather than leaves implied.
///
/// Fails if the export section stops being read, or if a global export is read
/// as a function export, which would look for the index in the wrong space and
/// find a module that defines no function.
#[test]
fn a_global_export_is_narrowed_like_an_instruction_operand() {
    let mut session = SpaceWasmSession::acquire();
    let declared = MAX_IR_INDEX + 2;

    let inside = module_exporting_global(0, declared, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "exporting the last global the cast leaves alone is inside every limit"
    );
    assert!(
        decode(&mut session, &inside).is_ok(),
        "the decoder loads it too, so the two agree on this side"
    );

    let over = module_exporting_global(0, declared, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("an export of global 65536", &over),
        (
            IndexSpace::Global,
            ReferenceSite::Export {
                name: String::from("g")
            },
            65_536,
            65_536
        ),
        "the finding names the export, since that is the line to edit"
    );
    assert!(
        decode(&mut session, &over).is_ok(),
        "the decoder loads it, which is the whole reason this crate is stricter here"
    );
}

/// A module importing `imported` `[] -> []` functions from `h`, declaring
/// `defined` of its own and exporting function `index` under the name `f`.
fn module_exporting_function(imported: u32, defined: u32, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    if imported > 0 {
        let mut imports = ImportSection::new();
        for i in 0..imported {
            imports.import("h", &format!("f{i}"), EntityType::Function(0));
        }
        module.section(&imports);
    }
    let mut functions = FunctionSection::new();
    for _ in 0..defined {
        functions.function(0);
    }
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("f", ExportKind::Func, index);
    module.section(&exports);
    module.section(&empty_bodies(defined));
    module.finish()
}

/// A code section of `count` `[] -> []` bodies that do nothing.
fn empty_bodies(count: u32) -> CodeSection {
    let mut code = CodeSection::new();
    for _ in 0..count {
        let mut body = Function::new([]);
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    code
}

/// A module of `defined` `[] -> []` functions whose first body calls each of
/// `indices` in the order written.
///
/// More than one call in one body is the whole point: the model keeps the
/// largest index a body names, and only a body naming two says whether it keeps
/// the largest or merely the first or the last.
fn module_calling_each(defined: u32, indices: &[u32]) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    for _ in 0..defined {
        functions.function(0);
    }
    module.section(&functions);
    let mut code = CodeSection::new();
    let mut caller = Function::new([]);
    for index in indices {
        caller.instruction(&Instruction::Call(*index));
    }
    caller.instruction(&Instruction::End);
    code.function(&caller);
    for _ in 1..defined {
        let mut body = Function::new([]);
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    module.section(&code);
    module.finish()
}

/// A module declaring `defined` `i32` globals whose one body reads each of
/// `indices` in the order written and discards it.
fn module_reading_each_global(defined: u32, indices: &[u32]) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    module.section(&globals_holding(defined, false));
    let mut code = CodeSection::new();
    let mut body = Function::new([]);
    for index in indices {
        body.instruction(&Instruction::GlobalGet(*index));
        body.instruction(&Instruction::Drop);
    }
    body.instruction(&Instruction::End);
    code.function(&body);
    module.section(&code);
    module.finish()
}

/// A module of `defined` `[] -> []` functions with one active element segment
/// listing `entries`, filling a table of exactly that many slots.
fn module_with_element_entries(defined: u32, entries: &[u32]) -> Vec<u8> {
    let slots = u64::try_from(entries.len()).expect("a row lists a handful of entries");
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    for _ in 0..defined {
        functions.function(0);
    }
    module.section(&functions);
    module.section(&table_of(slots));
    let mut elements = ElementSection::new();
    elements.active(
        None,
        &ConstExpr::i32_const(0),
        Elements::Functions(entries.into()),
    );
    module.section(&elements);
    module.section(&empty_bodies(defined));
    module.finish()
}

/// A module of `defined` `[] -> []` functions naming function `index` from each
/// of the three sections outside a body at once: an export, the `start` section
/// and the one entry of one element segment.
fn module_naming_from_every_outside_site(defined: u32, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    for _ in 0..defined {
        functions.function(0);
    }
    module.section(&functions);
    module.section(&table_of(1));
    let mut exports = ExportSection::new();
    exports.export("wrong", ExportKind::Func, index);
    module.section(&exports);
    module.section(&StartSection {
        function_index: index,
    });
    let mut elements = ElementSection::new();
    elements.active(
        None,
        &ConstExpr::i32_const(0),
        Elements::Functions([index].as_slice().into()),
    );
    module.section(&elements);
    module.section(&empty_bodies(defined));
    module.finish()
}

/// A table section holding one `funcref` table of exactly `slots` slots.
fn table_of(slots: u64) -> TableSection {
    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        table64: false,
        minimum: slots,
        maximum: Some(slots),
        shared: false,
    });
    tables
}

/// An index past what the module defines earns one finding, not two.
///
/// A reference the module cannot resolve is not a truncation: the interpreter
/// answers `None` and refuses it as out of range, and WebAssembly 1.0 validation
/// reaches the same conclusion first. Reporting the narrowing as well would be a
/// second sentence about one mistake, and it would quote a position no
/// definition sits at — the two numbers a reader is meant to search the module
/// for would name nothing.
///
/// Both index spaces, because the guard reads a different count for each and a
/// model holding one of them wrong would be invisible in the other. The third
/// module says *which* count: it imports a global, declares 65,537 of its own
/// and names the one index past everything it holds, so a guard comparing the
/// position against the imported and defined counts together would let it
/// through and add a narrowing at a position no definition sits at. The two
/// small modules import nothing and cannot separate the two readings, and a
/// module small enough to import nothing could not reach the narrowing at all.
///
/// Fails if the "does this resolve" guard is dropped, which adds a second
/// finding to each module here while leaving every other row green, and fails
/// on the third module if the guard counts the imports among the definitions.
#[test]
fn an_index_past_what_the_module_defines_is_one_finding_not_two() {
    let mut session = SpaceWasmSession::acquire();
    let unresolvable = 70_000;
    let declared = MAX_IR_INDEX + 2;

    for (label, wasm, hosts, expected) in [
        (
            "an export of global 70000 from a module of three",
            module_exporting_global(0, 3, unresolvable),
            no_hosts(),
            ValidationError::GlobalIdxOutOfRange,
        ),
        (
            "an export of function 70000 from a module of one",
            module_exporting_function(0, 1, unresolvable),
            no_hosts(),
            ValidationError::FunctionIdxOutOfRange,
        ),
        (
            "an export of global 65538 from a module importing one and defining 65,537",
            module_exporting_global(1, declared, declared + 1),
            host_with_global("h", "g0"),
            ValidationError::GlobalIdxOutOfRange,
        ),
    ] {
        let found = refusals(label, &wasm);
        assert!(
            matches!(found.as_slice(), [Violation::OutsideWasm1 { .. }]),
            "{label}: the one finding must be the validator's, got {found:?}"
        );
        let decoded =
            decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, WIDE_STACK>(&mut session, &wasm, hosts)
                .map(|_| ())
                .map_err(|refusal| refusal.err.err);
        assert_eq!(
            decoded,
            Err(expected),
            "{label}: the interpreter refuses it as out of range, not as a narrowing"
        );
    }
}

/// A `call` names a defined function past the cap: `check` refuses, and the
/// decoder loads and calls the function 65,536 below it.
///
/// The function half of the narrowing is the half a description could not reach
/// without a module of 65,537 bodies, which is more compiled IR than the
/// reference embedder's page budget holds. The row measures that refusal in
/// place rather than assuming it, then decodes again under a budget that does
/// hold the module — so the widened budget is a stated step and not a silent
/// one.
///
/// Fails if the `call` operand stops being read, if the control at 65,535 starts
/// being refused, or the day 0.7.1 checks the cast.
#[test]
fn a_call_past_the_cap_is_refused_here_and_calls_the_wrong_function_there() {
    let mut session = SpaceWasmSession::acquire();
    let defined = MAX_IR_INDEX + 2;

    let over = module_with_functions_naming(defined, FunctionRef::Call, MAX_IR_INDEX + 1);
    assert_eq!(
        decodes_wide(&mut session, &over),
        Err(ValidationError::AllocError(AllocError::OutOfMemory)),
        "65,537 bodies are more IR than spacewasm_std's page budget holds, which is why \
         the rows below widen it"
    );
    assert!(
        decode_with_pages::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(
            &mut session,
            &over,
            no_hosts(),
            257,
        )
        .is_ok(),
        "one page past the reference budget holds the module, which is what makes 257 the \
         smallest budget that does and TRUNCATION_ROW_CODE_PAGES four times it rather than \
         a number chosen to be large"
    );

    let inside = module_with_functions_naming(defined, FunctionRef::Call, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "a call to the last definition the cast leaves alone is inside every limit"
    );
    assert_eq!(
        witness_after(&mut session, &inside, Some("run")),
        OTHER_FUNCTION,
        "the call named definition 65535 and must reach it"
    );

    assert_eq!(
        only_truncation("call 65536", &over),
        (
            IndexSpace::Function,
            ReferenceSite::Body {
                function: String::from("func[2]")
            },
            65_536,
            65_536
        ),
        "the finding names the body the call is in"
    );
    assert!(
        predicted_definition("call 65536", &over)
            .contains("the body of function `func[2]` names function 65536"),
        "the rendered finding must name the body and the function space, since the reader \
         opens that body and the remedy splits the two modules"
    );
    assert_eq!(
        witness_after(&mut session, &over, Some("run")),
        FIRST_FUNCTION,
        "definition 65536 narrows to definition 0, so the call runs the wrong function"
    );
}

/// A function export names a definition past the cap, and invoking it runs the
/// function 65,536 below.
///
/// The export descriptor is read at decode time and stored already narrowed
/// (`Export::read`), so the name an embedder calls resolves to the wrong body
/// for the life of the module. That is why the row invokes the export rather
/// than only loading it.
///
/// Fails if the export section stops being read for functions, or if the
/// control at 65,535 starts being refused.
#[test]
fn a_function_export_past_the_cap_resolves_to_the_wrong_function() {
    let mut session = SpaceWasmSession::acquire();
    let defined = MAX_IR_INDEX + 2;

    let inside = module_with_functions_naming(defined, FunctionRef::Export, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "exporting definition 65535 is inside every limit"
    );
    assert_eq!(
        witness_after(&mut session, &inside, Some("wrong")),
        OTHER_FUNCTION,
        "the export named definition 65535 and must reach it"
    );

    let over = module_with_functions_naming(defined, FunctionRef::Export, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("an export of function 65536", &over),
        (
            IndexSpace::Function,
            ReferenceSite::Export {
                name: String::from("wrong")
            },
            65_536,
            65_536
        ),
        "the finding names the export, since that is the line to edit"
    );
    assert_eq!(
        witness_after(&mut session, &over, Some("wrong")),
        FIRST_FUNCTION,
        "the exported name resolves to definition 0 and runs it"
    );
}

/// An element-segment entry names a definition past the cap, and the table holds
/// the function 65,536 below.
///
/// The segment is read into the table already narrowed
/// (`ElementSection::read` writing `TableElement::Func`), so a `call_indirect`
/// through that slot dispatches to the wrong body. The module reaches the slot
/// through exactly that instruction, which is the only way to observe what the
/// table holds.
///
/// Fails if the element section stops being read, if the segment's entries are
/// read as something other than function indices, or if the control at 65,535
/// starts being refused.
#[test]
fn an_element_entry_past_the_cap_fills_the_table_with_the_wrong_function() {
    let mut session = SpaceWasmSession::acquire();
    let defined = MAX_IR_INDEX + 2;

    let inside = module_with_functions_naming(defined, FunctionRef::ElementEntry, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "an element entry naming definition 65535 is inside every limit"
    );
    assert_eq!(
        witness_after(&mut session, &inside, Some("run")),
        OTHER_FUNCTION,
        "the segment named definition 65535 and the table must hold it"
    );

    let over = module_with_functions_naming(defined, FunctionRef::ElementEntry, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("an element entry of function 65536", &over),
        (
            IndexSpace::Function,
            ReferenceSite::ElementSegment { index: 0 },
            65_536,
            65_536
        ),
        "the finding names the segment, since a module may carry several"
    );
    assert_eq!(
        witness_after(&mut session, &over, Some("run")),
        FIRST_FUNCTION,
        "the table slot holds definition 0, so the indirect call reaches it"
    );
}

/// The segment the finding names is read from the section, not assumed.
///
/// Every other element row carries one segment, so `element segment 0` reads
/// the same whether the number is counted or written down. This module carries
/// two, with the safe entry in the first and the over-cap entry in the second,
/// and requires the finding to say `1` — which is the number a reader opens the
/// module with, since a module may carry as many segments as it likes and only
/// one of them is the line to edit.
///
/// Slot 0 is filled by the definition at 65,535 rather than by definition 0, so
/// the witness also says that the `call_indirect` went through slot 1: reaching
/// slot 0 instead would answer [`OTHER_FUNCTION`], which is neither the named
/// definition's witness nor the truncated one's.
///
/// Fails if the segment index becomes a constant, if the segments are counted
/// from one, or if only the first segment of a section is read.
#[test]
fn the_element_segment_a_finding_names_is_the_one_the_entry_is_in() {
    let mut session = SpaceWasmSession::acquire();
    let defined = MAX_IR_INDEX + 2;

    let inside =
        module_with_functions_naming(defined, FunctionRef::LaterElementEntry, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "a second segment naming definition 65535 is inside every limit"
    );
    assert_eq!(
        witness_after(&mut session, &inside, Some("run")),
        OTHER_FUNCTION,
        "slot 1 holds definition 65535, which is also what slot 0 holds, so this side pins \
         the shape and the row below pins the slot"
    );

    let over =
        module_with_functions_naming(defined, FunctionRef::LaterElementEntry, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation(
            "an element entry of function 65536 in the second segment",
            &over
        ),
        (
            IndexSpace::Function,
            ReferenceSite::ElementSegment { index: 1 },
            65_536,
            65_536
        ),
        "the finding names the segment the entry is in, not the first one"
    );
    assert!(
        predicted_definition(
            "an element entry of function 65536 in the second segment",
            &over
        )
        .contains("element segment 1 names function 65536"),
        "the rendered finding must print the segment a reader opens, not only carry it as \
         data the row destructures"
    );
    assert_eq!(
        witness_after(&mut session, &over, Some("run")),
        FIRST_FUNCTION,
        "slot 1 holds definition 0 while slot 0 holds definition 65535, so the witness says \
         the truncated entry was the one reached"
    );
}

/// The `start` section names a definition past the cap, and the module runs the
/// function 65,536 below before an embedder calls anything.
///
/// The start function is the one reference that runs without being invoked, so
/// the witness is read with no call driving it: loading the module is the whole
/// of what the row does before asking which definition ran.
///
/// Fails if the start section stops being read, or if the control at 65,535
/// starts being refused.
#[test]
fn a_start_function_past_the_cap_runs_the_wrong_function_at_load() {
    let mut session = SpaceWasmSession::acquire();
    let defined = MAX_IR_INDEX + 2;

    let inside = module_with_functions_naming(defined, FunctionRef::Start, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "a start section naming definition 65535 is inside every limit"
    );
    assert_eq!(
        witness_after(&mut session, &inside, None),
        OTHER_FUNCTION,
        "the start section named definition 65535 and must run it"
    );

    let over = module_with_functions_naming(defined, FunctionRef::Start, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("a start section naming function 65536", &over),
        (
            IndexSpace::Function,
            ReferenceSite::Start,
            65_536,
            65_536
        ),
        "the finding names the start section, which carries no name of its own"
    );
    assert!(
        predicted_definition("a start section naming function 65536", &over)
            .contains("the start section names function 65536"),
        "the rendered finding must print the section a reader opens, not only carry it as \
         data the row destructures"
    );
    assert_eq!(
        witness_after(&mut session, &over, None),
        FIRST_FUNCTION,
        "the module runs definition 0 at load, before an embedder calls anything"
    );
}

/// An imported function shifts where the truncation begins, as an imported
/// global does.
///
/// The subtraction is per space and reads a different count for each, so a model
/// counting one of the two and not the other would leave that space's boundary a
/// definition out. The pair is the same written index, 65,536, under two modules
/// differing only by one import.
///
/// `check` alone, deliberately: what the decoder does with a truncated function
/// index is measured by the four rows above, and a module importing a function
/// needs a host function registered before it loads at all — a second variable
/// the subtraction is not about.
///
/// Fails if a function import stops being counted, which turns the accepted side
/// red, and if it is counted twice or against the global space, which turns
/// either refused side green.
#[test]
fn an_imported_function_shifts_where_the_truncation_begins() {
    let defined = MAX_IR_INDEX + 2;
    let exported = ReferenceSite::Export {
        name: String::from("f"),
    };

    let unshifted = module_exporting_function(0, defined, MAX_IR_INDEX + 1);
    assert_eq!(
        only_truncation("an export of function 65536 with no import", &unshifted),
        (IndexSpace::Function, exported.clone(), 65_536, 65_536),
        "with no import, index 65536 is definition 65536 and is refused"
    );

    let shifted = module_exporting_function(1, defined, MAX_IR_INDEX + 1);
    assert!(
        check(&shifted).is_ok(),
        "with one imported function, index 65536 is definition 65535 and fits"
    );

    let past = module_exporting_function(1, defined, MAX_IR_INDEX + 2);
    assert_eq!(
        only_truncation("an export of function 65537 behind one import", &past),
        (IndexSpace::Function, exported, 65_537, 65_536),
        "the finding reports the written index and the position it resolves to, which \
         differ by the imported count"
    );
}

/// The index a site reports is the largest one it names, not the first or the
/// last.
///
/// Every other row in this class names exactly one reference per site, which a
/// model keeping the first, the last or the largest answers identically. A body
/// naming two functions, a body naming two globals and a segment listing two
/// entries do not, and each pair is written in both orders because either order
/// alone still leaves two of the three readings agreeing. The shape this
/// protects is real: a body that calls function 65,536 and then function 0 would
/// otherwise be accepted and mis-execute, which is what this class exists to
/// refuse.
///
/// Fails if a peak stops being a maximum and becomes the first or the last
/// reference the site names, and fails on a control if naming two references at
/// one site becomes a fault of its own.
#[test]
fn the_index_a_site_reports_is_the_largest_one_it_names() {
    let defined = MAX_IR_INDEX + 2;
    let over = MAX_IR_INDEX + 1;
    let body = ReferenceSite::Body {
        function: String::from("func[0]"),
    };
    let segment = ReferenceSite::ElementSegment { index: 0 };

    for (label, wasm) in [
        (
            "a body calling 65535 and then 0",
            module_calling_each(defined, &[MAX_IR_INDEX, 0]),
        ),
        (
            "a body reading global 65535 and then global 0",
            module_reading_each_global(defined, &[MAX_IR_INDEX, 0]),
        ),
        (
            "an element segment listing 65535 and then 0",
            module_with_element_entries(defined, &[MAX_IR_INDEX, 0]),
        ),
    ] {
        assert!(
            check(&wasm).is_ok(),
            "{label}: naming two references at one site is not the fault; naming one past \
             the cap is"
        );
    }

    for (label, wasm, space, site) in [
        (
            "a body calling 65536 and then 0",
            module_calling_each(defined, &[over, 0]),
            IndexSpace::Function,
            body.clone(),
        ),
        (
            "a body calling 0 and then 65536",
            module_calling_each(defined, &[0, over]),
            IndexSpace::Function,
            body.clone(),
        ),
        (
            "a body reading global 65536 and then global 0",
            module_reading_each_global(defined, &[over, 0]),
            IndexSpace::Global,
            body.clone(),
        ),
        (
            "a body reading global 0 and then global 65536",
            module_reading_each_global(defined, &[0, over]),
            IndexSpace::Global,
            body,
        ),
        (
            "an element segment listing 65536 and then 0",
            module_with_element_entries(defined, &[over, 0]),
            IndexSpace::Function,
            segment.clone(),
        ),
        (
            "an element segment listing 0 and then 65536",
            module_with_element_entries(defined, &[0, over]),
            IndexSpace::Function,
            segment,
        ),
    ] {
        assert_eq!(
            only_truncation(label, &wasm),
            (space, site, 65_536, 65_536),
            "{label}: the over-cap reference is the one reported, whichever order the site \
             names the two in"
        );
    }
}

/// The three sites outside a body are reported in the order the module writes
/// them in.
///
/// `check` documents the order within the reference group — the exports, the
/// `start` section, then the element segments — and every other row in this
/// class earns exactly one reference finding, so that order is a sentence
/// nothing turns red about. One module naming the same over-cap definition from
/// all three is what reads it back.
///
/// Fails if the references are reported in the discovery order of some other
/// walk, sorted by index or by space, or grouped by site, and fails on the
/// control if naming one definition from three places becomes a fault of its
/// own.
#[test]
fn the_three_sites_outside_a_body_are_reported_in_section_order() {
    let defined = MAX_IR_INDEX + 2;
    let label = "an export, a start section and an element entry of function 65536";

    let inside = module_naming_from_every_outside_site(defined, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "the same three sites naming definition 65535 are inside every limit"
    );

    let wasm = module_naming_from_every_outside_site(defined, MAX_IR_INDEX + 1);

    let sites: Vec<ReferenceSite> = refusals(label, &wasm)
        .into_iter()
        .map(|violation| match violation {
            Violation::IndexTruncated { site, .. } => site,
            other => panic!("{label}: this module earns no other finding, got {other:?}"),
        })
        .collect();
    assert_eq!(
        sites,
        [
            ReferenceSite::Export {
                name: String::from("wrong")
            },
            ReferenceSite::Start,
            ReferenceSite::ElementSegment { index: 0 },
        ],
        "a refusal is read top to bottom, and the documented order is the order the module \
         writes the sections in"
    );
}

/// A module whose single `[] -> []` function branches through a table of
/// `targets` entries, every one of them to the block it sits in.
fn module_with_br_table(targets: u32) -> Vec<u8> {
    let list: Vec<u32> = vec![0; targets as usize];
    let mut body = vec![
        Instruction::Block(BlockType::Empty),
        Instruction::I32Const(0),
        Instruction::BrTable(list.as_slice().into(), 0),
    ];
    body.push(Instruction::End);
    module_with_locals_and_body(&[], &body)
}

/// A `br_table`'s target count meets the same 16-bit immediate an index does.
///
/// The interpreter writes the width of a jump table through the same emitter it
/// writes a type index through, and refuses it at the same cap — which is why
/// one violation covers both and says which it is about.
///
/// Only the refused side of this boundary is decodable, and that is the second
/// thing the row records. A table of 65,535 targets compiles to two IR words
/// each, so the accepting side needs 131,070 words of code page: the decoder
/// refuses it, but for the embedder's page budget rather than for the index, and
/// the assertion is written as "not the index refusal" so that a bigger budget
/// makes it pass rather than making it wrong.
///
/// Fails if the target count stops being read, if it is read as an index into
/// something, or if the refusal is attributed to the `call_indirect` immediate.
#[test]
fn a_branch_table_width_agrees_at_the_ir_immediate() {
    let mut session = SpaceWasmSession::acquire();

    let inside = module_with_br_table(MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "65535 targets are inside the immediate, whatever else the module costs"
    );
    assert_ne!(
        decodes_wide(&mut session, &inside),
        Err(ValidationError::IdxTooLarge),
        "a table of exactly 65535 targets is inside the immediate; whether it decodes at all \
         is the embedder's page budget, which is a different question"
    );

    let refused = module_with_br_table(MAX_IR_INDEX + 1);
    agree_wide(
        &mut session,
        "a br_table of 65536 targets",
        &refused,
        Some(ValidationError::IdxTooLarge),
    );
    assert!(
        refusals("a br_table of 65536 targets", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::IndexTooLarge {
                    kind: IndexKind::BranchTableTargets,
                    index: 65_536,
                    ..
                }
            )),
        "the refusal must name the table, or a reader edits the wrong instruction"
    );
}

/// `count` operands of `ty`, to stand on the stack.
fn live(ty: ValType, count: usize) -> Vec<Instruction<'static>> {
    let push = match ty {
        ValType::I64 => Instruction::I64Const(1),
        _ => Instruction::I32Const(1),
    };
    vec![push; count]
}

/// A module whose single `[] -> []` function stands `groups` of operands inside
/// a block and then executes `br 0` out of it.
fn module_with_branch_over(groups: &[(ValType, usize)]) -> Vec<u8> {
    let mut body = vec![Instruction::Block(BlockType::Empty)];
    for (ty, count) in groups {
        body.extend(live(*ty, *count));
    }
    body.push(Instruction::Br(0));
    body.push(Instruction::End);
    module_with_locals_and_body(&[], &body)
}

/// A branch's unwind, on both sides of the byte the interpreter encodes it in,
/// and in the unit that byte counts.
///
/// The `i64` pair is what makes it a *word* limit rather than an operand count:
/// 128 `i64` values are 128 operands and 256 words, and are refused, while 255
/// `i32` values are 255 of both and are accepted. A checker counting operands
/// would accept the first, and the decoder does not.
///
/// The mixed row sits exactly on the bound with two widths in one sum, which no
/// single-width row can distinguish from a checker that weighted every operand
/// at the width of the first.
///
/// Fails if `MAX_BRANCH_UNWIND_WORDS` moves without the interpreter moving with
/// it, if the word weighting is dropped, or if the unwind is measured after the
/// branch instead of before it — `br` truncates the stack it is measured
/// against.
#[test]
fn a_branch_unwind_agrees_at_the_boundary_and_counts_words() {
    let mut session = SpaceWasmSession::acquire();
    let bound = MAX_BRANCH_UNWIND_WORDS as usize;
    assert_eq!(
        bound, 255,
        "the rows below are built from this constant, so narrowing it would shrink them \
         rather than fail them"
    );

    for (label, groups, expected) in [
        ("255 i32 across a branch", vec![(ValType::I32, bound)], None),
        (
            "256 i32 across a branch",
            vec![(ValType::I32, bound + 1)],
            Some(ValidationError::LabelStackJumpTooDeep),
        ),
        ("127 i64 across a branch", vec![(ValType::I64, 127)], None),
        (
            "128 i64 across a branch",
            vec![(ValType::I64, 128)],
            Some(ValidationError::LabelStackJumpTooDeep),
        ),
        (
            "127 i64 and one i32 across a branch",
            vec![(ValType::I64, 127), (ValType::I32, 1)],
            None,
        ),
        (
            "127 i64 and two i32 across a branch",
            vec![(ValType::I64, 127), (ValType::I32, 2)],
            Some(ValidationError::LabelStackJumpTooDeep),
        ),
    ] {
        let wasm = module_with_branch_over(&groups);
        agree_wide(&mut session, label, &wasm, expected);
    }

    let refused = module_with_branch_over(&[(ValType::I64, 128)]);
    assert!(
        refusals("128 i64 across a branch", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::BranchUnwindTooDeep { words: 256, .. }
            )),
        "128 i64 values are 256 words, and that is the number the refusal owes"
    );
}

/// A module whose single `[] -> []` function stands `count` `i32` operands and
/// then leaves through `exit`, which is placed at the function's own top level.
fn module_leaving_the_function_with(count: usize, exit: Instruction<'static>) -> Vec<u8> {
    let mut body = live(ValType::I32, count);
    body.push(exit);
    module_with_locals_and_body(&[], &body)
}

/// Leaving the *function* carries no unwind, however much is standing on the
/// stack.
///
/// A branch to the outermost frame is the function's own label, and the
/// interpreter compiles it as an early return: a return carries the function's
/// result and discards the rest of the frame wholesale, so there is no unwind
/// byte to overflow. `return` takes the same route by a different name.
///
/// This is the row that makes the limit's shape a measurement rather than a
/// guess: 256 operands live across an *inner* branch are refused one test up,
/// and the same 256 live across these two are loaded. A model that measured
/// every branch alike would refuse a module the decoder runs, which is the
/// direction that turns a working build into a build-time failure.
///
/// Fails if the outermost-frame arm is dropped, or if `return` is ever folded
/// into the branch measurement.
#[test]
fn leaving_the_function_carries_no_unwind() {
    let mut session = SpaceWasmSession::acquire();
    let over = MAX_BRANCH_UNWIND_WORDS as usize + 1;

    for (label, exit) in [
        ("br to the function's own label", Instruction::Br(0)),
        ("return", Instruction::Return),
    ] {
        let wasm = module_leaving_the_function_with(over, exit);
        agree_wide(&mut session, label, &wasm, None);
    }
}

/// A module whose single `[] -> []` function stands `count` `i32` operands
/// inside a block and leaves through `br_if 0`.
fn module_with_conditional_branch_over(count: usize) -> Vec<u8> {
    let mut body = vec![Instruction::Block(BlockType::Empty)];
    body.extend(live(ValType::I32, count));
    body.push(Instruction::I32Const(0));
    body.push(Instruction::BrIf(0));
    body.extend(core::iter::repeat_n(Instruction::Drop, count));
    body.push(Instruction::End);
    module_with_locals_and_body(&[], &body)
}

/// A `br_if`'s own condition is not part of what it unwinds.
///
/// The interpreter takes the `i32` selector off the stack *before* it measures
/// the unwind, so a `br_if` standing on 255 operands plus its condition unwinds
/// 255 words and not 256. A model that measured the whole stack would refuse
/// this module, which the decoder loads — and no unconditional-branch row can
/// tell the two models apart, because `br` has no selector to forget.
///
/// Fails if the selector stops being taken off, and fails in the other direction
/// if two are.
#[test]
fn a_conditional_branch_does_not_unwind_its_own_condition() {
    let mut session = SpaceWasmSession::acquire();
    let bound = MAX_BRANCH_UNWIND_WORDS as usize;

    let accepted = module_with_conditional_branch_over(bound);
    agree_wide(&mut session, "br_if over 255 i32 and a condition", &accepted, None);

    let refused = module_with_conditional_branch_over(bound + 1);
    agree_wide(
        &mut session,
        "br_if over 256 i32 and a condition",
        &refused,
        Some(ValidationError::LabelStackJumpTooDeep),
    );
}

/// A module whose single `[] -> []` function stands `outer` operands inside a
/// block, `inner` more inside a block nested in it, and then executes `br 0` out
/// of the inner one.
fn module_with_nested_branch_over(outer: usize, inner: usize) -> Vec<u8> {
    let mut body = vec![Instruction::Block(BlockType::Empty)];
    body.extend(live(ValType::I32, outer));
    body.push(Instruction::Block(BlockType::Empty));
    body.extend(live(ValType::I32, inner));
    body.push(Instruction::Br(0));
    body.push(Instruction::End);
    body.extend(core::iter::repeat_n(Instruction::Drop, outer));
    body.push(Instruction::End);
    module_with_locals_and_body(&[], &body)
}

/// A branch unwinds down to its target frame's own height and no further.
///
/// Every other branch row here leaves a frame entered at operand height zero, so
/// what is live and what is discarded are the same number and a model reading
/// the first would satisfy all of them. Here the two differ: 250 operands stand
/// below the target frame and 10 above it, so 260 words are live — over the
/// bound — while the branch discards 10, and the decoder loads the module. The
/// refused neighbour keeps the shape and moves the ten to 256.
///
/// Fails if the target frame's height stops being subtracted: the accepting row
/// then becomes a refusal of a module the decoder runs, which is the direction
/// that turns a working build into a build-time failure.
#[test]
fn a_branch_unwinds_only_down_to_its_target_frame() {
    let mut session = SpaceWasmSession::acquire();
    let bound = MAX_BRANCH_UNWIND_WORDS as usize;

    let accepted = module_with_nested_branch_over(250, 10);
    agree_wide(
        &mut session,
        "br 0 discarding 10 words with 250 more standing below its frame",
        &accepted,
        None,
    );

    let refused = module_with_nested_branch_over(40, bound + 1);
    let label = "br 0 discarding 256 words with 40 more standing below its frame";
    agree_wide(
        &mut session,
        label,
        &refused,
        Some(ValidationError::LabelStackJumpTooDeep),
    );
    assert!(
        refusals(label, &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::BranchUnwindTooDeep { words: 256, .. }
            )),
        "what the branch discards is what the refusal owes, not what stands below its frame"
    );
}

/// A module whose single `[] -> []` function runs `unreachable` inside a block
/// and then branches conditionally out of it.
fn module_with_unreachable_conditional_branch() -> Vec<u8> {
    module_with_locals_and_body(
        &[],
        &[
            Instruction::Block(BlockType::Empty),
            Instruction::Unreachable,
            Instruction::BrIf(0),
            Instruction::End,
        ],
    )
}

/// A conditional branch in unreachable code, standing at its own frame's floor,
/// discards nothing.
///
/// `unreachable` truncates the frame to its floor and leaves the validator
/// synthesizing whatever an operator asks for, so the `i32` a `br_if` takes off
/// comes out of nowhere and shrinks nothing. Every other conditional row stands
/// on operands it really has, and none of them can tell a selector pop that asks
/// whether there is anything to pop from one that always subtracts one.
///
/// Fails if the pop stops asking — an unconditional subtraction goes below the
/// floor here, which in the unsigned unit this is counted in is not an answer.
#[test]
fn a_conditional_branch_in_unreachable_code_discards_nothing() {
    let mut session = SpaceWasmSession::acquire();
    let wasm = module_with_unreachable_conditional_branch();
    agree_wide(
        &mut session,
        "br_if after unreachable at its frame's floor",
        &wasm,
        None,
    );
}

/// The inner of the two blocks [`module_with_two_frame_br_table`] builds, as a
/// branch sitting inside it names the frame.
const INNER_FRAME: u32 = 0;

/// The outer of the two, named from the same place.
const OUTER_FRAME: u32 = 1;

/// A module standing `outer` operands in one block and `inner` more in a block
/// inside it, then branching through a one-entry table whose listed target is
/// `listed` and whose default is `default`, each of them [`INNER_FRAME`] or
/// [`OUTER_FRAME`].
///
/// Which of the two frames is the listed target and which the default is a
/// parameter because it is the whole content of the test below: a model that
/// read one of the two and not the other answers correctly for whichever
/// arrangement it happens to be handed.
fn module_with_two_frame_br_table(
    outer: usize,
    inner: usize,
    listed: u32,
    default: u32,
) -> Vec<u8> {
    let targets = [listed];
    let mut body = vec![Instruction::Block(BlockType::Empty)];
    body.extend(live(ValType::I32, outer));
    body.push(Instruction::Block(BlockType::Empty));
    body.extend(live(ValType::I32, inner));
    body.push(Instruction::I32Const(0));
    body.push(Instruction::BrTable(targets.as_slice().into(), default));
    body.push(Instruction::End);
    body.extend(core::iter::repeat_n(Instruction::Drop, outer));
    body.push(Instruction::End);
    module_with_locals_and_body(&[], &body)
}

/// Every target of a `br_table` is measured, against its own frame.
///
/// One `br_table` is as many branches as it lists, each leaving a different
/// frame and therefore unwinding a different amount. The boundary is therefore
/// pinned twice, the second time with the two arranged the other way round: once
/// with the *default* the deep target and the listed one a few words away, and
/// once with the listed target the deep one and the default unwinding nothing. A
/// model reading only one of the two positions answers correctly for one
/// arrangement and accepts a module the decoder refuses in the other, which a
/// single arrangement cannot tell from a model that reads both.
///
/// Fails if the default target stops being measured, if the listed targets stop
/// being measured, if the targets are measured against one frame instead of each
/// against its own, or if the selector is counted here and not in `br_if`.
#[test]
fn every_target_of_a_branch_table_is_measured_against_its_own_frame() {
    let mut session = SpaceWasmSession::acquire();
    let bound = MAX_BRANCH_UNWIND_WORDS as usize;

    let accepted = module_with_two_frame_br_table(bound - 10, 10, INNER_FRAME, OUTER_FRAME);
    agree_wide(
        &mut session,
        "a br_table whose default unwinds 255 words",
        &accepted,
        None,
    );

    let refused = module_with_two_frame_br_table(bound - 10, 11, INNER_FRAME, OUTER_FRAME);
    agree_wide(
        &mut session,
        "a br_table whose default unwinds 256 words",
        &refused,
        Some(ValidationError::LabelStackJumpTooDeep),
    );
    assert!(
        refusals("a br_table whose default unwinds 256 words", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::BranchUnwindTooDeep { words: 256, .. }
            )),
        "the deepest of the table's targets is the one the refusal owes"
    );

    let accepted = module_with_two_frame_br_table(bound, 0, OUTER_FRAME, INNER_FRAME);
    agree_wide(
        &mut session,
        "a br_table whose listed target unwinds 255 words",
        &accepted,
        None,
    );

    let refused = module_with_two_frame_br_table(bound + 1, 0, OUTER_FRAME, INNER_FRAME);
    agree_wide(
        &mut session,
        "a br_table whose listed target unwinds 256 words",
        &refused,
        Some(ValidationError::LabelStackJumpTooDeep),
    );
    assert!(
        refusals("a br_table whose listed target unwinds 256 words", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::BranchUnwindTooDeep { words: 256, .. }
            )),
        "the listed target is the deep one here, and it is the number the refusal owes"
    );
}

// ---------------------------------------------------------------------------
// Imports: the caps an embedder imposes, not the decoder
// ---------------------------------------------------------------------------

/// The two import-name caps, and the reason they are 31 and not 32.
///
/// A 33-byte name overflows the decoder's own 32-byte read buffer, so the
/// decoder refuses it outright. A 32-byte name decodes — and can never be bound,
/// because `HostName<31>` is the only way an embedder names a host module or a
/// host function, which this asserts directly rather than inferring. The
/// accepting row registers a host under 31-byte names and decodes, so the cap is
/// pinned from both sides.
///
/// Fails if `MAX_IMPORT_NAME_BYTES` is relaxed to the decoder's 32, or if the
/// refusal stops distinguishing the two names.
#[test]
fn import_names_agree_at_the_registration_cap() {
    let mut session = SpaceWasmSession::acquire();
    assert_eq!(
        MAX_IMPORT_NAME_BYTES, 31,
        "the cap is `HostName<31>`'s; the accepting row below is built from this constant, \
         so narrowing it would otherwise shorten the row rather than fail it"
    );
    let long = "m".repeat(MAX_IMPORT_NAME_BYTES);
    let field = "f".repeat(MAX_IMPORT_NAME_BYTES);

    let accepted = module_importing(&long, &field, 1, 0);
    agree(
        &mut session,
        "31-byte import names",
        &accepted,
        host_exporting(&long, &field, 1),
        true,
    );

    for (which, module_name, field_name) in [
        (NamePart::Module, "m".repeat(32), String::from("f")),
        (NamePart::Field, String::from("m"), "f".repeat(32)),
    ] {
        let refused = module_importing(&module_name, &field_name, 1, 0);
        // The decoder reads a 32-byte name happily; what no embedder can do is
        // register a host for it, so the honest comparison is against a decode
        // with the best host set that could be built — which is none.
        assert!(
            HostName::<31>::try_from_str(if which == NamePart::Module {
                &module_name
            } else {
                &field_name
            })
            .is_err(),
            "a 32-byte name must be unregistrable, or this row is about nothing"
        );
        agree(&mut session, "a 32-byte import name", &refused, no_hosts(), false);
        assert!(
            refusals("a 32-byte import name", &refused)
                .iter()
                .any(|violation| matches!(
                    violation,
                    Violation::ImportNameTooLong { which: part, len: 32, .. } if *part == which
                )),
            "the refusal must say which of the two names is over"
        );
    }

    // The decoder reads the module name and the field name through two separate
    // 32-byte reads, so a 33-byte name is pinned on each of them. The verdict
    // alone cannot say that: an import against an empty host set is refused for
    // the missing host whatever its names, which would satisfy these two rows
    // without the length ever being read. So each asserts the error the
    // over-long read itself raises — `read_vec_stack::<32, _>` refuses the
    // declared length before it reads a byte of the name.
    for (label, over_decoder) in [
        (
            "a 33-byte import module name",
            module_importing(&"m".repeat(33), "f", 1, 0),
        ),
        (
            "a 33-byte import field name",
            module_importing("m", &"f".repeat(33), 1, 0),
        ),
    ] {
        agree(&mut session, label, &over_decoder, no_hosts(), false);
        let refused = decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(
            &mut session,
            &over_decoder,
            no_hosts(),
        )
        .err()
        .expect("a 33-byte name is refused");
        assert_eq!(
            refused.err.err,
            ValidationError::VecTooLong,
            "{label}: the decoder must refuse the over-long name read, not the absent host"
        );
    }
}

/// Host arity, on both sides of the nine-parameter list.
///
/// Like the name cap this is a registration limit, so the refused side asserts
/// that `HostValList` cannot be built at ten — the module itself decodes as
/// WebAssembly, and what it can never do is find a host.
///
/// Fails if `MAX_HOST_FUNCTION_PARAMS` moves without the embedder's list moving.
#[test]
fn host_arity_agrees_at_the_registration_cap() {
    let mut session = SpaceWasmSession::acquire();

    let accepted = module_importing("host", "call", MAX_HOST_FUNCTION_PARAMS, 0);
    agree(
        &mut session,
        "a 9-parameter import",
        &accepted,
        host_exporting("host", "call", MAX_HOST_FUNCTION_PARAMS),
        true,
    );

    assert!(
        HostValList::try_new(&"i".repeat(MAX_HOST_FUNCTION_PARAMS + 1)).is_err(),
        "a 10-parameter host signature must be unregistrable, or this row is about nothing"
    );
    let refused = module_importing("host", "call", MAX_HOST_FUNCTION_PARAMS + 1, 0);
    agree(&mut session, "a 10-parameter import", &refused, no_hosts(), false);
    assert!(
        refusals("a 10-parameter import", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::ImportArityExceeded { params: 10, .. }
            )),
        "the refusal must name the arity"
    );
}

/// An import declaring two results.
///
/// Refused twice over, and both are worth saying: WebAssembly 1.0 has no
/// multi-value, and a host function returns at most one value whatever the
/// module says. The row asserts the import-shaped finding is present, since the
/// feature verdict alone would leave a reader looking for a post-1.0
/// instruction that is not there.
///
/// The decode half of this row is over-determined — an import against an empty
/// host set is refused for the missing host whatever its signature — so the
/// registration cap is asserted where it actually lives, the way the two rows
/// above do: an embedder cannot build a two-result host function at all. The
/// one-result accepting row further up is the positive control.
///
/// Fails if the import's result count stops being read, or if the embedder
/// starts admitting a second result.
#[test]
fn a_two_result_import_is_refused_by_both() {
    let mut session = SpaceWasmSession::acquire();

    assert!(
        HostFunction::try_new(
            HostName::try_from_str("call").expect("the row's field name is registrable"),
            HostValList::new(""),
            HostValList::new("ii"),
            |_: &mut spacewasm::Engine, _: &[Value]| core::ops::ControlFlow::Continue(None),
        )
        .is_err(),
        "a two-result host function must be unbuildable, or this row is about nothing"
    );

    let refused = module_importing("host", "call", 0, 2);
    agree(&mut session, "a two-result import", &refused, no_hosts(), false);
    assert!(
        refusals("a two-result import", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::ImportMultipleResults { results: 2, .. }
            )),
        "the refusal must name the result count"
    );
}

// ---------------------------------------------------------------------------
// Module shape
// ---------------------------------------------------------------------------

/// A custom section whose *name* is longer than the decoder's name buffer.
///
/// The payload is never reached, which is the surprise worth pinning: a module
/// is refused for the name of a section every runtime would otherwise skip.
/// The 32-byte row is the other side of the boundary.
///
/// Fails if the cap is confused with the 31-byte import cap next to it.
#[test]
fn custom_section_names_agree_at_the_boundary() {
    let mut session = SpaceWasmSession::acquire();

    let accepted = wat(&format!("(module (@custom \"{}\" \"payload\"))", "c".repeat(32)));
    agree(&mut session, "a 32-byte custom name", &accepted, no_hosts(), true);

    let refused = wat(&format!("(module (@custom \"{}\" \"payload\"))", "c".repeat(33)));
    agree(&mut session, "a 33-byte custom name", &refused, no_hosts(), false);
    assert!(
        refusals("a 33-byte custom name", &refused)
            .iter()
            .any(|violation| matches!(
                violation,
                Violation::CustomSectionNameTooLong { len: 33, .. }
            )),
        "the refusal must name the section"
    );
}

/// A linear memory one page past the 32-bit address space.
///
/// Only the refused side is decoded: a 65,536-page memory is four gigabytes,
/// and the decoder allocates a module's guest memory while loading it, so the
/// accepting side of this boundary is not a thing to ask a test machine for.
/// The refusal happens while the memory *type* is read, before any allocation.
///
/// Fails if the page cap is dropped, or if the check starts allocating.
#[test]
fn an_oversized_memory_is_refused_by_both() {
    let mut session = SpaceWasmSession::acquire();
    assert_eq!(
        MAX_MEMORY_PAGES, 65_536,
        "the accepting side of this boundary is never decoded, so the constant is what the \
         refused side is one past"
    );
    let refused = wat("(module (memory 65537))");
    agree(&mut session, "a 65537-page memory", &refused, no_hosts(), false);
    assert!(
        refusals("a 65537-page memory", &refused)
            .iter()
            .any(|violation| matches!(violation, Violation::MemoryTooLarge { pages: 65_537 })),
        "the refusal must name the page count"
    );

    // The declared maximum is the other half of the bound, and the only half a
    // module can put over it while staying small enough to decode the rest of:
    // a checker reading the initial size alone accepts this one and the decoder
    // does not, which is the permissive direction.
    let over_max = wat("(module (memory 1 65537))");
    agree(&mut session, "a memory whose maximum is 65537 pages", &over_max, no_hosts(), false);
    assert!(
        refusals("a memory whose maximum is 65537 pages", &over_max)
            .iter()
            .any(|violation| matches!(violation, Violation::MemoryTooLarge { pages: 65_537 })),
        "the refusal must name the page count the maximum declares"
    );
}

// ---------------------------------------------------------------------------
// Outside WebAssembly 1.0
// ---------------------------------------------------------------------------

/// A module carrying a raw operator stream, for the opcodes no assembler emits.
fn module_with_raw_body(body: &[u8]) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut code = CodeSection::new();
    let mut function = Function::new([]);
    function.raw(body.iter().copied());
    code.function(&function);
    module.section(&code);
    module.finish()
}

/// Three instructions outside WebAssembly 1.0, each refused by both.
///
/// One per reason a post-1.0 byte reaches an artifact: a bulk-memory operator
/// from a linked foreign module, a sign-extension operator from a stock Rust
/// build, and one of this compiler's own `0xfc` verification operators from a
/// proof-mode artifact. All three are `check`'s `OutsideWasm1`, and all three
/// are refused by the interpreter, which knows no `0xfc` at all.
///
/// Fails if the feature envelope widens past WebAssembly 1.0 — the day the
/// checker would start accepting a module the decoder refuses.
#[test]
fn post_webassembly_one_instructions_are_refused_by_both() {
    let mut session = SpaceWasmSession::acquire();

    let bulk = wat(
        "(module (memory 1) (func (i32.const 0) (i32.const 0) (i32.const 0) (memory.copy)))",
    );
    agree(&mut session, "memory.copy", &bulk, no_hosts(), false);

    let sign_extension = wat("(module (func (result i32) (i32.const 0) (i32.extend8_s)))");
    agree(&mut session, "i32.extend8_s", &sign_extension, no_hosts(), false);

    // `0xfc 0x3a` is this compiler's `forall` block opener, which no assembler
    // writes and no standard assigns.
    let verification_operator = module_with_raw_body(&[0xfc, 0x3a, 0x0b]);
    agree(
        &mut session,
        "a 0xfc verification operator",
        &verification_operator,
        no_hosts(),
        false,
    );

    for (label, module) in [
        ("memory.copy", &bulk),
        ("i32.extend8_s", &sign_extension),
        ("a 0xfc verification operator", &verification_operator),
    ] {
        assert!(
            refusals(label, module)
                .iter()
                .any(|violation| matches!(violation, Violation::OutsideWasm1 { .. })),
            "{label} must be refused as a feature question, not as a size one"
        );
    }
}

// ---------------------------------------------------------------------------
// The two verifier bounds, in their own units
// ---------------------------------------------------------------------------

/// Control nesting, measured against a decoder configured at exactly that
/// depth.
///
/// The module nests four blocks; both decoders count the implicit function-body
/// frame as well, so the number is five. Decoding at `<5, _>` succeeds and at
/// `<4, _>` fails, which is what makes "including the function frame" a measured
/// convention rather than a comment beside the constant.
///
/// Fails if either side stops counting the function frame, or if the sampling
/// misses the deepest point.
#[test]
fn control_depth_is_reported_in_the_decoder_s_own_frames() {
    let mut session = SpaceWasmSession::acquire();
    let nested = wat("(module (func (block (block (block (block))))))");

    let report = check(&nested).expect("the module is conformant");
    assert_eq!(
        report.deepest().1, 5,
        "four blocks plus the function-body frame is five"
    );

    assert!(
        decode_with::<5, EMBEDDER_MAX_STACK_DEPTH>(&mut session, &nested, no_hosts()).is_ok(),
        "the decoder must load the module at exactly the depth the report named"
    );
    assert!(
        decode_with::<4, EMBEDDER_MAX_STACK_DEPTH>(&mut session, &nested, no_hosts()).is_err(),
        "one frame below the reported depth the decoder must refuse it, or the number is \
         an over-estimate and nobody would trust it"
    );
}

/// A module whose operand stack peaks at sixteen live values of `ty`.
fn module_with_operand_peak(ty: &str) -> Vec<u8> {
    let pushes = format!("({ty}.const 1) ").repeat(16);
    let drops = "(drop) ".repeat(16);
    wat(&format!("(module (func {pushes}{drops}))"))
}

/// The operand-stack unit, pinned by the pair that separates values from words.
///
/// Sixteen `i32` values and sixteen `i64` values are the same *sixteen* to the
/// verifier's stack and 16 against 32 to the engine's. Both decode at
/// `<_, 16>` and both fail at `<_, 15>`, so `MAX_STACK_DEPTH` is a value count —
/// held here by a decode rather than by a sentence, because an embedder sized
/// from the word figure wastes memory and one sized from the wrong figure the
/// other way fails in flight.
///
/// Fails if the word weighting leaks into the value count, or the value count
/// into the word sum.
#[test]
fn operand_stack_depth_counts_values_and_the_word_sum_counts_widths() {
    let mut session = SpaceWasmSession::acquire();

    for (ty, expected_words) in [("i32", 16), ("i64", 32)] {
        let module = module_with_operand_peak(ty);
        let report = check(&module).expect("the module is conformant");
        assert_eq!(
            report.tallest().1, 16,
            "sixteen live {ty} values are sixteen values"
        );
        assert_eq!(
            report.widest().1, expected_words,
            "sixteen live {ty} values are {expected_words} words"
        );

        assert!(
            decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, 16>(&mut session, &module, no_hosts())
                .is_ok(),
            "the decoder must load the {ty} module at a value-stack of exactly sixteen"
        );
        assert!(
            decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, 15>(&mut session, &module, no_hosts())
                .is_err(),
            "one value below the reported height the decoder must refuse the {ty} module"
        );
    }
}

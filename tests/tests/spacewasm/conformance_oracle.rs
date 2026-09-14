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
//! in place. Every other side of every other row is a pair.

use inference_target_conformance::spacewasm::{
    IndexKind, MAX_BRANCH_UNWIND_WORDS, MAX_FRAME_WORDS, MAX_HOST_FUNCTION_PARAMS,
    MAX_IMPORT_NAME_BYTES, MAX_IR_INDEX, MAX_LOCAL_WORDS, MAX_LOCALS_GROUP_COUNT,
    MAX_MEMORY_PAGES, MAX_PARAM_WORDS, NamePart, Violation, check,
};
use spacewasm::{HostFunction, HostModule, HostName, HostValList, ValidationError, Value};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ElementSection, Elements, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, Instruction, Module, RefType,
    TableSection, TableType, TypeSection, ValType,
};

use crate::support::{
    EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH, FUEL, Outcome, SpaceWasmSession,
    decode_with,
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

/// A module declaring `count` immutable `i32` globals and exporting a
/// `[] -> [i32]` function that returns `global.get index`.
///
/// Global 0 holds [`FIRST_GLOBAL`] and every other global holds
/// [`OTHER_GLOBAL`], so which global a call actually read can be told from the
/// value it returns.
fn module_with_globals_and_getter(count: u32, index: u32) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut globals = GlobalSection::new();
    for i in 0..count {
        let value = if i == 0 { FIRST_GLOBAL } else { OTHER_GLOBAL };
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: false,
                shared: false,
            },
            &ConstExpr::i32_const(value),
        );
    }
    module.section(&globals);
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

/// What global 0 holds in a [`module_with_globals_and_getter`] module.
const FIRST_GLOBAL: i32 = 7;

/// What every other global in one holds.
const OTHER_GLOBAL: i32 = 9;

/// The one residue the README records, held to being what it says: a global
/// index over the 16-bit cap is **truncated** by 0.7.1 rather than refused, and
/// `check` agrees with the runtime by accepting the module.
///
/// This is the only place the crate is deliberately permissive about a module
/// the interpreter mis-executes, and the justification is that the decoder
/// accepts it — so the justification is a measurement, and a measurement no test
/// performs is a sentence nothing turns red about. The row runs the module: the
/// value it answers with says *which* global was read.
///
/// The 65,535 neighbour is the control. It reaches the last index the cast
/// leaves alone, so it proves the truncation and not merely that a large module
/// returns something.
///
/// Fails the day upstream refuses instead of truncating, or narrows somewhere
/// else so the call answers global 65,536's value — either of which is the day
/// `check` stops agreeing with the decoder and this exemption has to become a
/// refusal.
#[test]
fn a_global_index_over_the_ir_immediate_is_truncated_rather_than_refused() {
    let mut session = SpaceWasmSession::acquire();

    let inside = module_with_globals_and_getter(MAX_IR_INDEX + 1, MAX_IR_INDEX);
    assert!(
        check(&inside).is_ok(),
        "a global index at the cap is inside every limit this crate models"
    );
    let mut loaded = decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, WIDE_STACK>(
        &mut session,
        &inside,
        no_hosts(),
    )
    .expect("the decoder loads a module whose global index fits the cast");
    assert_eq!(
        loaded.invoke("get", &[], FUEL),
        Outcome::Value(Some(Value::I32(OTHER_GLOBAL))),
        "global 65535 is the one named and the one that must be read"
    );
    drop(loaded);

    let over = module_with_globals_and_getter(MAX_IR_INDEX + 2, MAX_IR_INDEX + 1);
    assert!(
        check(&over).is_ok(),
        "the decoder accepts this module, so refusing it here would be this crate \
         disagreeing with the runtime it describes"
    );
    let mut loaded = decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, WIDE_STACK>(
        &mut session,
        &over,
        no_hosts(),
    )
    .expect("0.7.1 truncates the index rather than refusing the module");
    assert_eq!(
        loaded.invoke("get", &[], FUEL),
        Outcome::Value(Some(Value::I32(FIRST_GLOBAL))),
        "65536 narrowed to u16 is 0, so the call reads global 0 rather than the one it named"
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

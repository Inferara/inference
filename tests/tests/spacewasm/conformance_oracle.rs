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

use inference_target_conformance::spacewasm::{
    MAX_FRAME_WORDS, MAX_HOST_FUNCTION_PARAMS, MAX_IMPORT_NAME_BYTES, MAX_LOCAL_WORDS,
    MAX_LOCALS_GROUP_COUNT, MAX_MEMORY_PAGES, MAX_PARAM_WORDS, NamePart, Violation, check,
};
use spacewasm::{HostFunction, HostModule, HostName, HostValList, ValidationError, Value};
use wasm_encoder::{
    CodeSection, Function, FunctionSection, Instruction, Module, TypeSection, ValType,
};

use crate::support::{
    EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH, SpaceWasmSession, decode_with,
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

//! The same bytes, two engines, one answer.
//!
//! The decode sweep establishes that a `spacewasm` artifact *loads* on the
//! flight interpreter. That is not the claim the target needs. What a mission
//! needs is that the module it flies computes what the module it proved
//! computes — and the proof, the golden tiers and every execution test in this
//! repository are all written against `wasmtime`.
//!
//! So this sweep compiles each fixture **once**, at the `spacewasm` target, and
//! runs those exact bytes under both engines. Compiling twice would measure
//! byte identity, which the corpus-wide identity sweep already owns; running one
//! module twice measures the only thing left, which is whether two independent
//! implementations of WebAssembly 1.0 agree about it. Agreement is required on
//! the returned value *and* on whether the call trapped, which matters since
//! arithmetic overflow now traps and a guard that fired in one engine and not
//! the other would otherwise read as two different-but-fine results.

use std::path::Path;

use inference_tests::corpus::{
    carries_verification_operator, codegen_for_target_no_analysis, has_import_section,
    relative_to_test_data, single_file_corpus_sources,
};
use inference_wasm_codegen::Target;

use crate::support::{FUEL, Outcome, SpaceWasmSession, decode};

/// The fixtures the `spacewasm` target's own envelope refuses, so no module
/// exists to compare.
///
/// Committed rather than counted, for the reason the sibling sweep's classes
/// are: a floor moves only once enough fixtures have left the compared set at
/// once, and the fixture that left is exactly the one a regression takes. A
/// fixture entering or leaving this list is a reviewed edit.
const ENVELOPE_REFUSED: &[&str] = &[
    "codegen/wasm/algo_converge/algo_converge.inf",
    "codegen/wasm/base/array_nondet/array_nondet.inf",
    "codegen/wasm/base/assign_nondet/assign_nondet.inf",
    "codegen/wasm/base/const_in_forall/const_in_forall.inf",
    "codegen/wasm/base/enum_uzumaki_domain/enum_uzumaki_domain.inf",
    "codegen/wasm/base/i64_uzumaki/i64_uzumaki.inf",
    "codegen/wasm/base/if_nondet/if_nondet.inf",
    "codegen/wasm/base/local_variables/local_variables.inf",
    "codegen/wasm/base/multidim_array_uzumaki/multidim_array_uzumaki.inf",
    "codegen/wasm/base/narrow_uzumaki/narrow_uzumaki.inf",
    "codegen/wasm/base/nondet/nondet.inf",
    "codegen/wasm/base/struct_array_field_nondet/struct_array_field_nondet.inf",
    "codegen/wasm/base/struct_nondet/struct_nondet.inf",
    "codegen/wasm/base/u32_uzumaki/u32_uzumaki.inf",
    "codegen/wasm/loops/loop_in_nondet/loop_in_nondet.inf",
];

/// The fixtures whose emitted module carries a verification operator, which is
/// not WebAssembly, so neither engine can be asked about it. See
/// [`ENVELOPE_REFUSED`] for why the list is committed.
const OPERATOR_EMITTING: &[&str] = &[
    "codegen/wasm/loops/nondet_then_break/nondet_then_break.inf",
];

/// The fixtures whose emitted module declares an import, which resolves against
/// a host-module set this tier leaves empty. See [`ENVELOPE_REFUSED`].
const IMPORT_EMITTING: &[&str] = &[
    "codegen/wasm/extern_import/import_dedup/import_dedup.inf",
    "codegen/wasm/extern_import/import_with_locals/import_with_locals.inf",
    "codegen/wasm/extern_import/multi_import/multi_import.inf",
    "codegen/wasm/extern_import/single_import/single_import.inf",
    "codegen/wasm/param_by_ref/extern_forward/extern_forward.inf",
    "codegen/wasm/param_by_ref/extern_forward_read_only/extern_forward_read_only.inf",
    "codegen/wasm/self_extern_escape/escape_nested_block/escape_nested_block.inf",
    "codegen/wasm/self_extern_escape/escape_nested_expr/escape_nested_expr.inf",
    "codegen/wasm/self_extern_escape/escape_scalar_projection/escape_scalar_projection.inf",
    "codegen/wasm/self_extern_escape/escape_sub_object/escape_sub_object.inf",
    "codegen/wasm/self_extern_escape/escape_whole_self/escape_whole_self.inf",
    "codegen/wasm/self_extern_escape/escape_with_param/escape_with_param.inf",
    "codegen/wasm/self_extern_escape/mut_self_extern/mut_self_extern.inf",
    "codegen/wasm/self_extern_escape/no_escape_self/no_escape_self.inf",
    "codegen/wasm/self_extern_escape/read_only_extern_self/read_only_extern_self.inf",
];

/// A value an exported function gave back, in bits.
///
/// Floating-point results are compared as bit patterns: two engines producing
/// the same NaN must agree, and `f64::NAN != f64::NAN` would make that
/// unstatable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scalar {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
}

/// What one engine did with one call.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    /// It returned, with its result if it declared one.
    Returned(Option<Scalar>),
    /// It trapped. Which trap is not compared: the two engines do not share a
    /// trap vocabulary, and the property under test is that a program either
    /// traps in both or in neither.
    Trapped,
}

impl Scalar {
    /// Reads a `wasmtime` result.
    fn from_wasmtime(value: &wasmtime::Val) -> Self {
        match value {
            wasmtime::Val::I32(v) => Self::I32(*v),
            wasmtime::Val::I64(v) => Self::I64(*v),
            wasmtime::Val::F32(bits) => Self::F32(*bits),
            wasmtime::Val::F64(bits) => Self::F64(*bits),
            other => panic!("no fixture returns {other:?}"),
        }
    }

    /// Reads a SpaceWasm result.
    fn from_spacewasm(value: spacewasm::Value) -> Self {
        match value {
            spacewasm::Value::I32(v) => Self::I32(v),
            spacewasm::Value::I64(v) => Self::I64(v),
            spacewasm::Value::F32(v) => Self::F32(v.to_bits()),
            spacewasm::Value::F64(v) => Self::F64(v.to_bits()),
        }
    }
}

/// One instantiation of `wasm` under `wasmtime`, calling `export` with no
/// arguments.
///
/// A fresh store and instance per call, so a fixture's globals and memory start
/// where the SpaceWasm side starts them: a shared instance would make the second
/// call's answer depend on the first, and the comparison would then be measuring
/// the harness rather than the engines.
///
/// # Panics
///
/// Panics if the module does not instantiate, if `export` is absent, or if the
/// call fails for anything but a trap — none of which is a result to compare.
fn wasmtime_answer(engine: &wasmtime::Engine, module: &wasmtime::Module, export: &str) -> Answer {
    let mut store = wasmtime::Store::new(engine, ());
    let instance = wasmtime::Instance::new(&mut store, module, &[])
        .unwrap_or_else(|e| panic!("the module does not instantiate under wasmtime: {e}"));
    let func = instance
        .get_func(&mut store, export)
        .unwrap_or_else(|| panic!("wasmtime sees no export `{export}`"));
    let signature = func.ty(&store);
    assert!(
        signature.params().len() == 0,
        "`{export}` takes {} parameters under wasmtime; the two decoders disagree about \
         its arity, since it was selected as a zero-parameter export",
        signature.params().len()
    );
    assert!(
        signature.results().len() <= 1,
        "`{export}` returns {} values under wasmtime; the SpaceWasm side reads a single \
         result, so the comparison would narrow to the first one in silence",
        signature.results().len()
    );
    let mut results = vec![wasmtime::Val::I32(0); signature.results().len()];
    match func.call(&mut store, &[], &mut results) {
        Ok(()) => Answer::Returned(results.first().map(Scalar::from_wasmtime)),
        Err(e) if e.downcast_ref::<wasmtime::Trap>().is_some() => Answer::Trapped,
        Err(e) => panic!("`{export}` failed under wasmtime for something other than a trap: {e}"),
    }
}

/// [`wasmtime_answer`]'s counterpart: one decode of `wasm` under the flight
/// interpreter, calling `export` with no arguments.
///
/// # Panics
///
/// Panics if the module does not decode, or if the call exhausts its fuel —
/// which for this corpus means a lowering produced a loop that does not
/// terminate, not that the budget was too small.
fn spacewasm_answer(session: &mut SpaceWasmSession, wasm: &[u8], export: &str) -> Answer {
    let mut module = decode(session, wasm).unwrap_or_else(|e| {
        panic!("the module does not decode under spacewasm: {:?} at offset {}", e.err, e.offset)
    });
    match module.invoke(export, &[], FUEL) {
        Outcome::Value(value) => Answer::Returned(value.map(Scalar::from_spacewasm)),
        Outcome::Trap(_) => Answer::Trapped,
        Outcome::OutOfFuel => panic!("`{export}` exhausted {FUEL} instructions under spacewasm"),
    }
}

/// The two engines answer the same for every exported zero-parameter function of
/// every import-free single-file codegen fixture.
///
/// The unit of coverage is an **invocation**, not a fixture. Filtering on a
/// `main` would have covered nine of the corpus's fixtures and read in a summary
/// as though it covered the corpus; taking every zero-parameter export instead
/// reaches the bodies the fixtures were written to exercise, and `main` becomes
/// one row among them rather than the gate. Zero-parameter is read off the
/// *WebAssembly* signature rather than off the source, because a lowering that
/// returns an aggregate takes a hidden pointer the source never declared.
///
/// Per fixture, the export descriptor's names and the names the flight decoder
/// reads out of the artifact are required to be the same set: two descriptions
/// of one module — what the compiler says it exported, and what the artifact
/// exports — have to agree.
///
/// The three dispositions that keep a fixture out of the comparison — refused
/// by the target's envelope, emitting a verification operator, emitting an
/// import — are each compared against a committed list of the fixtures in them,
/// so a fixture leaving the compared set fails here rather than being absorbed
/// by the headroom under a count floor.
///
/// Fails if the two engines ever disagree about a value or about trapping, if a
/// descriptor and an artifact disagree about what is exported, if a fixture
/// changes disposition without its list moving with it, or if coverage falls
/// below the floors.
#[test]
fn the_two_engines_agree_on_every_zero_parameter_export() {
    let sources = single_file_corpus_sources();
    assert!(
        sources.len() >= 100,
        "expected at least 100 single-file fixtures, found {}; a collector that silently \
         found nothing would pass this test vacuously",
        sources.len()
    );

    let wasmtime_engine = wasmtime::Engine::default();
    let mut session = SpaceWasmSession::acquire();

    let mut fixtures = 0usize;
    let mut invocations = 0usize;
    let mut refused: Vec<String> = Vec::new();
    let mut with_imports: Vec<String> = Vec::new();
    let mut with_operators: Vec<String> = Vec::new();
    let mut trapping: Vec<String> = Vec::new();
    let mut main_rows = 0usize;

    for (label, source) in &sources {
        let name = relative_to_test_data(Path::new(label));
        let output = match codegen_for_target_no_analysis(source, Target::SpaceWasm) {
            Ok(output) => output,
            Err(e) => {
                // The target's own envelope refused it, and the message has to
                // say so: a fixture that stopped compiling for an unrelated
                // reason would otherwise slide out of the compared set and make
                // this sweep quieter rather than redder.
                let message = e.to_string();
                assert!(
                    message.contains(
                        "The `spacewasm` target does not support non-deterministic operations"
                    ),
                    "{label} was refused for `{message}`, which is not the target's envelope"
                );
                refused.push(name);
                continue;
            }
        };
        if carries_verification_operator(output.wasm()) {
            // Checked before imports, the reverse of the sibling sweep's order,
            // and for the reason that sweep states its own: there, a module that
            // is both comes back with the verdict the decoder reaches first;
            // here no verdict is taken at all, and the reason to report is the
            // one about the module rather than the one about this tier's empty
            // host set.
            //
            // The target's gate is a backstop and says so: over a definition it
            // recognizes a bare `@` and the statement kinds it enumerates, and
            // leaves a `forall` nested in a loop body to analysis rule A042 —
            // which this sweep skips on purpose, so the rest of the corpus
            // reaches code generation at all. The module it emitted carries the
            // compiler's custom `0xfc` operators and is not WebAssembly, so
            // neither engine can be asked about it.
            with_operators.push(name);
            continue;
        }
        if has_import_section(output.wasm()) {
            with_imports.push(name);
            continue;
        }

        let module = wasmtime::Module::new(&wasmtime_engine, output.wasm())
            .unwrap_or_else(|e| panic!("{label} is not a valid module for wasmtime: {e}"));

        let mut described: Vec<&str> =
            output.export_signatures().iter().map(|s| s.name.as_str()).collect();
        described.sort_unstable();

        let functions = {
            let decoded = decode(&mut session, output.wasm())
                .unwrap_or_else(|e| panic!("{label} does not decode: {:?}", e.err));
            decoded.exported_functions()
        };
        let zero_parameter: Vec<&str> = functions
            .iter()
            .filter(|f| f.params == 0)
            .map(|f| f.name.as_str())
            .collect();
        let mut exported: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        exported.sort_unstable();
        assert_eq!(
            described, exported,
            "{label}: the export descriptor names {described:?} and the artifact exports \
             {exported:?}"
        );

        fixtures += 1;
        for export in &zero_parameter {
            let under_wasmtime = wasmtime_answer(&wasmtime_engine, &module, export);
            let under_spacewasm = spacewasm_answer(&mut session, output.wasm(), export);
            assert_eq!(
                under_wasmtime, under_spacewasm,
                "{label}::{export}: wasmtime answered {under_wasmtime:?} and spacewasm \
                 answered {under_spacewasm:?}"
            );
            if under_wasmtime == Answer::Trapped {
                trapping.push(format!("{label}::{export}"));
            }
            if *export == "main" {
                main_rows += 1;
            }
            invocations += 1;
        }
    }

    refused.sort();
    with_operators.sort();
    with_imports.sort();

    let trapped = trapping.len();
    println!(
        "spacewasm differential: {invocations} invocations over {fixtures} fixtures \
         ({trapped} trapping, {main_rows} named `main`); {} fixtures refused by \
         the target envelope, {} skipped for verification operators, \
         {} skipped for imports; trapping rows: {trapping:?}",
        refused.len(),
        with_operators.len(),
        with_imports.len()
    );
    assert_eq!(
        refused,
        ENVELOPE_REFUSED,
        "the fixtures the target envelope refuses are not the ones committed to \
         `ENVELOPE_REFUSED`; a fixture entering or leaving the compared set is a reviewed \
         edit, not a silent change in what this sweep covers"
    );
    assert_eq!(
        with_operators,
        OPERATOR_EMITTING,
        "the fixtures emitting a verification operator are not the ones committed to \
         `OPERATOR_EMITTING`; see `ENVELOPE_REFUSED` for why the list is committed"
    );
    assert_eq!(
        with_imports,
        IMPORT_EMITTING,
        "the fixtures emitting an import are not the ones committed to `IMPORT_EMITTING`; \
         see `ENVELOPE_REFUSED` for why the list is committed"
    );
    assert_eq!(
        fixtures + refused.len() + with_operators.len() + with_imports.len(),
        sources.len(),
        "every fixture the walk yields must be compared, refused or skipped for a named \
         reason; a disposition added without a counter would leave the corpus partly \
         unaccounted for"
    );
    assert!(
        invocations >= 270,
        "expected at least 270 invocations, ran {invocations}; a narrowing export filter \
         would otherwise pass this vacuously"
    );
    assert!(
        fixtures >= 100,
        "expected at least 100 fixtures to reach both engines, {fixtures} did"
    );
    assert!(
        refused.len() >= 10,
        "the target envelope refused {} fixtures; a sweep that refuses none is not \
         compiling at this target at all, and every assertion above would hold just as \
         well for the default one",
        refused.len()
    );
    assert!(
        !trapping.is_empty(),
        "no invocation trapped, so trap parity is asserted over nothing here; the row \
         written for it is the overflow test below, and this floor is what says the \
         corpus contributes one too"
    );
    assert!(main_rows > 0, "no fixture's `main` was invoked, so it is not one of the rows");
}

// ---------------------------------------------------------------------------
// The negative control
// ---------------------------------------------------------------------------

/// A module returning `value`, and nothing else.
fn returns(value: i32) -> Vec<u8> {
    wat::parse_str(format!("(module (func (export \"f\") (result i32) i32.const {value}))"))
        .expect("a hand-written module parses")
}

/// A module whose `f` traps.
fn traps() -> Vec<u8> {
    wat::parse_str("(module (func (export \"f\") (result i32) unreachable))")
        .expect("a hand-written module parses")
}

/// The comparison above can fail.
///
/// Every assertion in the sweep is an equality, and an equality between two
/// answers that are always the same constant is satisfied by a harness that
/// reads nothing. This runs the two readers over modules that genuinely differ
/// and requires them to notice, in both of the ways the sweep compares: a
/// different returned value, and a trap where the other side returned.
///
/// It is a committed test rather than a neutralization run because a temporary
/// edit to an emitter on a shared tree is compiled into every concurrent build,
/// and the evidence would die with the pull-request comment.
///
/// Fails if either reader stops reading — if the `wasmtime` side always reported
/// `Returned(None)`, or the SpaceWasm side always reported `Trapped`.
#[test]
fn the_comparison_can_tell_two_modules_apart() {
    let engine = wasmtime::Engine::default();
    let one = returns(1);
    let other = returns(2);
    let trapping = traps();

    let compiled_one = wasmtime::Module::new(&engine, &one).expect("a valid module");
    let compiled_trapping = wasmtime::Module::new(&engine, &trapping).expect("a valid module");

    let mut session = SpaceWasmSession::acquire();

    assert_eq!(
        wasmtime_answer(&engine, &compiled_one, "f"),
        spacewasm_answer(&mut session, &one, "f"),
        "the control: the same module must compare equal across the two engines"
    );
    assert_ne!(
        wasmtime_answer(&engine, &compiled_one, "f"),
        spacewasm_answer(&mut session, &other, "f"),
        "a differing return value must be visible to the comparison"
    );
    assert_ne!(
        wasmtime_answer(&engine, &compiled_one, "f"),
        spacewasm_answer(&mut session, &trapping, "f"),
        "a trap where the other engine returned must be visible to the comparison"
    );
    assert_ne!(
        wasmtime_answer(&engine, &compiled_trapping, "f"),
        spacewasm_answer(&mut session, &one, "f"),
        "the same, with the trap on the wasmtime side"
    );
}

/// The two engines agree that an overflow guard fires, and that its control
/// does not.
///
/// Trap parity is the half of the comparison the corpus barely exercises: the
/// trapping half of `arith_overflow` takes its operands as **parameters**, so
/// almost nothing a zero-parameter sweep reaches can trap. Leaving the property
/// to whichever fixture happens to qualify would be an accident, so it is stated
/// here on a program written for it.
///
/// The overflowing operand is a `let` binding rather than the literal
/// `2147483647 + 1`. Analysis refuses an addition whose operands fold to a
/// constant that leaves the type, so the literal form is not a program this
/// compiler accepts; it would reach code generation here only because this
/// sweep skips analysis, and the row would then be stated on a shape no build
/// produces.
///
/// Fails if the guard stops being emitted, if one engine starts wrapping where
/// the other traps, or if the control stops computing.
#[test]
fn the_two_engines_agree_that_an_overflow_guard_fires() {
    const OVERFLOWS: &str = "\
pub fn over() -> i32 {
    let max: i32 = 2147483647;
    return max + 1;
}

pub fn under() -> i32 {
    let max: i32 = 2147483646;
    return max + 1;
}
";
    let output = codegen_for_target_no_analysis(OVERFLOWS, Target::SpaceWasm)
        .expect("an unannotated addition is an ordinary program");

    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, output.wasm()).expect("a valid module");
    let mut session = SpaceWasmSession::acquire();

    assert_eq!(
        wasmtime_answer(&engine, &module, "over"),
        Answer::Trapped,
        "the guard must fire under wasmtime, or the comparison below is about nothing"
    );
    assert_eq!(
        spacewasm_answer(&mut session, output.wasm(), "over"),
        Answer::Trapped,
        "the guard must fire under spacewasm too"
    );
    assert_eq!(
        wasmtime_answer(&engine, &module, "under"),
        spacewasm_answer(&mut session, output.wasm(), "under"),
        "the control must compute the same value in both engines"
    );
    assert_eq!(
        spacewasm_answer(&mut session, output.wasm(), "under"),
        Answer::Returned(Some(Scalar::I32(2147483647))),
        "the control must not trap, or `over` trapping says nothing about the guard"
    );
}

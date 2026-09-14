//! The committed golden corpus, in front of the real decoder.
//!
//! Two sweeps that must land on opposite sides of the same decoder. The default
//! partition is what a `wasm32` or `spacewasm` build produces, and every one of
//! its members has to load. The opt-in `bulk_memory_golden` partition is what a
//! build produces when the manifest asks for a post-MVP feature, and every one
//! of its members has to be *refused for carrying an instruction the decoder
//! does not know* — not merely refused, which a size or depth limit would also
//! achieve, and which would leave the target's whole bulk-memory argument
//! resting on a refusal that had nothing to do with bulk memory.
//!
//! Both sweeps are total: every golden either gets a verdict from the decoder or
//! is named in an exclusion class, and each class is compared against a
//! **committed** list of the goldens in it. Nothing here is satisfied by a
//! classifier agreeing with itself: a fixture that grows an import, or that
//! starts carrying a verification operator, leaves the swept set and fails
//! against the committed list rather than quietly shrinking what the sweep
//! covers. Nothing is merely skipped either: both of the default partition's
//! exclusion classes are put back in front of the decoder by a test of their
//! own, and the opt-in partition's excluded class is decoded where it stands
//! and only kept out of the count, since a skip is where coverage goes to die.

use std::path::{Path, PathBuf};

use inference_tests::corpus::{
    bulk_memory_golden_artifacts, carries_verification_operator, codegen_for_target_no_analysis,
    golden_wasm_artifacts, has_import_section, relative_to_test_data,
};
use inference_wasm_codegen::Target;
use spacewasm::{AllocError, SectionKind, ValidationError};

use crate::support::{
    EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH, FUEL, Outcome, SpaceWasmSession, decode,
    decode_with, ir_stats,
};

/// Modules the decoder is not being asked about, and why.
///
/// Neither class is a SpaceWasm verdict on the compiler: an import needs a host
/// module this tier registers none of, and a verification operator is a
/// proof-toolchain instruction that is not WebAssembly at all. Membership is
/// decided by reading the module rather than by naming files, and each class is
/// then checked against the committed list of the goldens in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotDecodable {
    /// The module declares an import, which resolves against a host module set
    /// this tier leaves empty.
    HasImports,
    /// The module carries one of the compiler's custom `0xfc` verification
    /// operators, which no WebAssembly decoder accepts.
    HasVerificationOperators,
}

/// Classifies `wasm`, or `None` when the decoder is expected to load it.
fn not_decodable(wasm: &[u8]) -> Option<NotDecodable> {
    if has_import_section(wasm) {
        // Checked first because the decoder reads the import section before the
        // code section: for a module that is both, an unresolved import is the
        // verdict that comes back, and a class whose companion test asserts a
        // verdict the decoder never reaches would be asserting the wrong thing.
        return Some(NotDecodable::HasImports);
    }
    if carries_verification_operator(wasm) {
        return Some(NotDecodable::HasVerificationOperators);
    }
    None
}

/// The default-partition goldens that declare an import.
///
/// Committed rather than derived so that the exclusion is a reviewed fact about
/// the corpus. A classifier compared against itself agrees whatever the corpus
/// does; this list disagrees.
const IMPORT_BEARING: &[&str] = &[
    "codegen/wasm/extern_import/host_import/host_import.wasm",
    "codegen/wasm/extern_import/host_import_fprime/host_import_fprime.wasm",
    "codegen/wasm/extern_import/import_dedup/import_dedup.wasm",
    "codegen/wasm/extern_import/import_with_locals/import_with_locals.wasm",
    "codegen/wasm/extern_import/multi_import/multi_import.wasm",
    "codegen/wasm/extern_import/single_import/single_import.wasm",
    "codegen/wasm/param_by_ref/extern_forward/extern_forward.wasm",
    "codegen/wasm/param_by_ref/extern_forward_read_only/extern_forward_read_only.wasm",
    "codegen/wasm/self_extern_escape/escape_nested_block/escape_nested_block.wasm",
    "codegen/wasm/self_extern_escape/escape_nested_expr/escape_nested_expr.wasm",
    "codegen/wasm/self_extern_escape/escape_scalar_projection/escape_scalar_projection.wasm",
    "codegen/wasm/self_extern_escape/escape_sub_object/escape_sub_object.wasm",
    "codegen/wasm/self_extern_escape/escape_whole_self/escape_whole_self.wasm",
    "codegen/wasm/self_extern_escape/escape_with_param/escape_with_param.wasm",
    "codegen/wasm/self_extern_escape/mut_self_extern/mut_self_extern.wasm",
    "codegen/wasm/self_extern_escape/no_escape_self/no_escape_self.wasm",
    "codegen/wasm/self_extern_escape/read_only_extern_self/read_only_extern_self.wasm",
];

/// The default-partition goldens that carry a verification operator and no
/// import. See [`IMPORT_BEARING`] for why the list is committed.
const OPERATOR_BEARING: &[&str] = &[
    "codegen/wasm/algo_converge/algo_converge.wasm",
    "codegen/wasm/base/array_nondet/array_nondet.wasm",
    "codegen/wasm/base/assign_nondet/assign_nondet.wasm",
    "codegen/wasm/base/const_in_forall/const_in_forall.wasm",
    "codegen/wasm/base/enum_uzumaki_domain/enum_uzumaki_domain.wasm",
    "codegen/wasm/base/i64_uzumaki/i64_uzumaki.wasm",
    "codegen/wasm/base/if_nondet/if_nondet.wasm",
    "codegen/wasm/base/local_variables/local_variables.wasm",
    "codegen/wasm/base/multidim_array_uzumaki/multidim_array_uzumaki.wasm",
    "codegen/wasm/base/narrow_uzumaki/narrow_uzumaki.wasm",
    "codegen/wasm/base/nondet/nondet.wasm",
    "codegen/wasm/base/struct_array_field_nondet/struct_array_field_nondet.wasm",
    "codegen/wasm/base/struct_nondet/struct_nondet.wasm",
    "codegen/wasm/base/u32_uzumaki/u32_uzumaki.wasm",
    "codegen/wasm/loops/loop_in_nondet/loop_in_nondet.wasm",
    "codegen/wasm/loops/nondet_then_break/nondet_then_break.wasm",
];

/// The opt-in `bulk_memory_golden` goldens whose refusal would not be evidence
/// about bulk memory, because they carry a verification operator — also a
/// `0xfc` instruction — as well.
const BULK_MEMORY_OPERATOR_BEARING: &[&str] = &[
    "codegen/wasm/bulk_memory_golden/array_nondet/array_nondet.wasm",
    "codegen/wasm/bulk_memory_golden/const_in_forall/const_in_forall.wasm",
    "codegen/wasm/bulk_memory_golden/enum_uzumaki_domain/enum_uzumaki_domain.wasm",
    "codegen/wasm/bulk_memory_golden/multidim_array_uzumaki/multidim_array_uzumaki.wasm",
    "codegen/wasm/bulk_memory_golden/narrow_uzumaki/narrow_uzumaki.wasm",
    "codegen/wasm/bulk_memory_golden/struct_array_field_nondet/struct_array_field_nondet.wasm",
    "codegen/wasm/bulk_memory_golden/struct_nondet/struct_nondet.wasm",
];

/// Reads a golden, panicking with the path rather than with an io error alone.
fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// [`relative_to_test_data`] over a whole class, sorted.
fn names(paths: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = paths.iter().map(|path| relative_to_test_data(path)).collect();
    names.sort();
    names
}

/// The goldens of `partition` carrying `class`, sorted.
fn classified(partition: &[PathBuf], class: NotDecodable) -> Vec<PathBuf> {
    partition
        .iter()
        .filter(|path| not_decodable(&read(path)) == Some(class))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// The default partition: everything loads
// ---------------------------------------------------------------------------

/// Every WebAssembly 1.0 golden decodes, validates and IR-compiles under the
/// reference embedder configuration.
///
/// This is the tier's central claim. The four existing golden tiers establish
/// that the artifacts are what the compiler emitted, that they read as the
/// expected text, that a stock validator accepts them at the Wasm 1.0 feature
/// level and that `wasmtime` runs them. None of that is evidence about a
/// `no_std` flight interpreter with its own verifier, its own depth bounds and
/// its own IR compiler, which is the runtime the `spacewasm` target names.
///
/// The two exclusion classes are compared against committed lists, and the
/// goldens this sweep skipped are required to be exactly the goldens those
/// lists name, so a skip predicate that widened until it covered nothing would
/// fail rather than pass, and so would a fixture that quietly left the decoded
/// set.
///
/// Fails if a golden stops decoding, if a golden enters or leaves an exclusion
/// class without the committed list moving with it, or if the corpus shrinks
/// below the coverage floor.
#[test]
fn every_wasm_1_0_golden_decodes_under_the_flight_interpreter() {
    let artifacts = golden_wasm_artifacts();
    let mut session = SpaceWasmSession::acquire();

    let mut decoded = Vec::new();
    let mut excluded = Vec::new();
    for path in &artifacts {
        let wasm = read(path);
        if not_decodable(&wasm).is_some() {
            excluded.push(path.clone());
            continue;
        }
        match decode(&mut session, &wasm) {
            Ok(_) => decoded.push(path.clone()),
            Err(e) => panic!(
                "{} does not decode under spacewasm: {:?} at offset {}",
                path.display(),
                e.err,
                e.offset
            ),
        }
    }

    let with_imports = classified(&artifacts, NotDecodable::HasImports);
    let with_operators = classified(&artifacts, NotDecodable::HasVerificationOperators);
    assert_eq!(
        names(&with_imports),
        IMPORT_BEARING,
        "the goldens declaring an import are not the ones committed to `IMPORT_BEARING`; a \
         fixture entering or leaving that class is a reviewed edit, not a silent change in \
         what this sweep covers"
    );
    assert_eq!(
        names(&with_operators),
        OPERATOR_BEARING,
        "the goldens carrying a verification operator are not the ones committed to \
         `OPERATOR_BEARING`; see `IMPORT_BEARING` for why the list is committed"
    );

    let mut committed = [IMPORT_BEARING, OPERATOR_BEARING].concat();
    committed.sort_unstable();
    assert_eq!(
        names(&excluded),
        committed,
        "the goldens this sweep skipped are not the ones the two committed classes name; a \
         third `NotDecodable` variant added without a `classified` call beside it lands here"
    );

    println!(
        "spacewasm decode sweep: {} decoded, {} excluded for imports, {} excluded for \
         verification operators, {} goldens in the partition",
        decoded.len(),
        with_imports.len(),
        with_operators.len(),
        artifacts.len()
    );
    assert!(
        decoded.len() >= 100,
        "expected at least 100 goldens to decode, only {} did; a widening skip predicate \
         would otherwise pass this vacuously",
        decoded.len()
    );
}

/// The import-bearing goldens are refused for their imports and nothing else.
///
/// The sweep above skips them, and a skip is where coverage goes to die: this
/// puts each one in front of the decoder anyway and requires the verdict to be
/// an unresolved import. Without it, "excluded because it has imports" would be
/// a claim about the file rather than about the runtime, and a golden that had
/// also become undecodable for a second reason would keep passing.
///
/// Fails if an import-bearing golden decodes — which would mean the empty host
/// set is not being enforced — or is refused for an unrelated reason.
#[test]
fn an_import_bearing_golden_is_refused_for_its_unresolved_import() {
    let with_imports = classified(&golden_wasm_artifacts(), NotDecodable::HasImports);
    assert!(
        !with_imports.is_empty(),
        "no golden declares an import; this test would pass over an empty corpus"
    );

    let mut session = SpaceWasmSession::acquire();
    for path in &with_imports {
        let error = decode(&mut session, &read(path))
            .err()
            .unwrap_or_else(|| panic!("{} decoded with no host module registered", path.display()));
        assert!(
            matches!(
                error.err.err,
                ValidationError::FunctionImportNotFound
                    | ValidationError::MemoryImportNotFound
                    | ValidationError::TableImportNotFound
                    | ValidationError::GlobalImportNotFound
            ),
            "{} was refused for {:?}, not for an unresolved import",
            path.display(),
            error.err.err
        );
    }
    println!("spacewasm import refusals: {} goldens", with_imports.len());
}

/// The verification-operator goldens are refused for carrying an instruction
/// the decoder does not know.
///
/// The companion of the import test, for the second exclusion class, and it
/// states something the target cares about on its own: a module built for the
/// proof toolchain must not fly. Without it the class would rest on a scan of
/// the file rather than on a verdict, and a compile-mode artifact that wrongly
/// retained a specification body would join the class and be skipped in silence.
///
/// Fails if such a golden decodes, or is refused for a reason that is not an
/// unknown opcode in the code section.
#[test]
fn a_verification_operator_golden_is_refused_as_an_unknown_opcode() {
    let with_operators =
        classified(&golden_wasm_artifacts(), NotDecodable::HasVerificationOperators);
    assert!(
        !with_operators.is_empty(),
        "no golden carries a verification operator; this test would pass over an empty class"
    );

    let mut session = SpaceWasmSession::acquire();
    for path in &with_operators {
        assert_refused_as_unknown_opcode(&mut session, path);
    }
    println!("spacewasm verification-operator refusals: {} goldens", with_operators.len());
}

/// Requires the golden at `path` to be refused with `InvalidOpcode(0xfc)` raised
/// in the code section.
///
/// `0xfc` is the prefix byte rather than the sub-opcode because the decoder
/// never gets as far as reading the sub-opcode: the prefix reaches the opcode
/// dispatch's fallthrough arm, which is what `spacewasm` 0.7.1 implementing
/// WebAssembly 1.0 and no proposal on top of it means. The section matters too —
/// the same verdict raised while reading a data segment would be a different
/// fault.
fn assert_refused_as_unknown_opcode(session: &mut SpaceWasmSession, path: &Path) {
    let error = decode(session, &read(path)).err().unwrap_or_else(|| {
        panic!("{} decoded under spacewasm, which knows no `0xfc` instruction", path.display())
    });
    assert_eq!(
        error.err.err,
        ValidationError::InvalidOpcode(0xfc),
        "{} was refused for {:?}, not for an unknown opcode",
        path.display(),
        error.err.err
    );
    assert_eq!(
        error.err.section,
        Some(SectionKind::Code),
        "{} was refused outside the code section",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// The opt-in partition: nothing loads, and for the stated reason
// ---------------------------------------------------------------------------

/// Every bulk-memory golden is refused, with a verdict naming an opcode the
/// decoder does not know.
///
/// This is the trip-wire behind the target's post-MVP refusal. A `memory.copy`
/// comes back as `InvalidOpcode(0xfc)`; asserting merely "this failed" would be
/// satisfied by a size or depth limit and would keep passing on the day the
/// decoder grew bulk memory, which is exactly the day the refusal in `codegen()`
/// should be revisited.
///
/// The goldens that also carry a verification operator are kept out of the
/// count and committed by name, because their refusal is a `0xfc` too and would
/// be evidence about the proof toolchain rather than about bulk memory. They
/// are still put in front of the decoder and still have to be refused: a skip
/// is where coverage goes to die, and "excluded from the bulk-memory count" is
/// a reason not to count a verdict rather than a reason not to take one. The
/// floor is 50 rather than the house's `>= 100`, which a 57-member partition
/// cannot hold.
///
/// Fails if a bulk-memory golden loads, if one is refused for a reason that is
/// not an unknown opcode, or if the class kept out of the count stops being the
/// committed one.
#[test]
fn every_bulk_memory_golden_is_refused_as_an_unknown_opcode() {
    let artifacts = bulk_memory_golden_artifacts();
    let mut session = SpaceWasmSession::acquire();

    let mut refused = Vec::new();
    let mut with_operators = Vec::new();
    for path in &artifacts {
        assert_refused_as_unknown_opcode(&mut session, path);
        if carries_verification_operator(&read(path)) {
            with_operators.push(path.clone());
            continue;
        }
        refused.push(path.clone());
    }

    assert_eq!(
        names(&with_operators),
        BULK_MEMORY_OPERATOR_BEARING,
        "the opt-in goldens excluded for carrying a verification operator are not the ones \
         committed to `BULK_MEMORY_OPERATOR_BEARING`"
    );
    assert_eq!(
        refused.len() + with_operators.len(),
        artifacts.len(),
        "every opt-in golden must be counted as a bulk-memory refusal or named in the \
         excluded class"
    );

    println!(
        "spacewasm bulk-memory refusals: {} of {} goldens in the opt-in partition; the other \
         {} are refused too, but carry a verification operator as well and so are not \
         counted as evidence about bulk memory",
        refused.len(),
        artifacts.len(),
        with_operators.len()
    );
    assert!(
        refused.len() >= 50,
        "expected at least 50 bulk-memory goldens to be refused, only {} were",
        refused.len()
    );
}

// ---------------------------------------------------------------------------
// A specification is not shipped code
// ---------------------------------------------------------------------------

/// A program carrying a `spec` block, compiled for `spacewasm` in compile mode,
/// decodes and runs — and its module carries no `0xfc` operator at all.
///
/// The source is inline because the corpus cannot state this: every spec-bearing
/// codegen fixture in the tree lives under a multi-file `src` directory, which
/// the single-file walk skips, so no golden this file sweeps contains a `spec`.
///
/// The scan asserting an absence needs a control, or a body that had quietly
/// stopped being non-deterministic would state nothing: the same two functions
/// **outside** a `spec` are refused by the target's own envelope, which is what
/// says the constructs are ones the runtime cannot take and the `spec` wrapper
/// is what removes them.
///
/// Fails if compile mode ever emits a `spec` body, if the emitted module stops
/// loading, or if the specification survives into the export list.
#[test]
fn a_spec_bearing_program_ships_no_specification_instructions() {
    const SOURCE: &str = "\
fn double(n: i32) -> i32 { return wrapping(n + n); }

spec Doubling {
    fn is_modular(lo: i32) forall {
        let n: i32 = @;
        assert(double(n) == wrapping(n + n));
        assert(lo == lo);
    }
}

pub fn twice_of_seven() -> i32 { return double(7); }
";
    const OUTSIDE_A_SPEC: &str = "\
fn double(n: i32) -> i32 { return wrapping(n + n); }

fn is_modular(lo: i32) forall {
    let n: i32 = @;
    assert(double(n) == wrapping(n + n));
    assert(lo == lo);
}

pub fn twice_of_seven() -> i32 { return double(7); }
";

    let refusal = codegen_for_target_no_analysis(OUTSIDE_A_SPEC, Target::SpaceWasm)
        .expect_err("the same body outside a `spec` is not shippable code")
        .to_string();
    assert!(
        refusal.contains("The `spacewasm` target does not support non-deterministic operations"),
        "the control was refused for `{refusal}`, so it does not show that the `spec` \
         wrapper is what makes the program shippable"
    );

    let output = codegen_for_target_no_analysis(SOURCE, Target::SpaceWasm)
        .expect("a `spec` block is specification code, not shipped code");
    assert!(
        !carries_verification_operator(output.wasm()),
        "the compile-mode module carries a specification instruction"
    );

    let mut session = SpaceWasmSession::acquire();
    let mut module = decode(&mut session, output.wasm()).unwrap_or_else(|e| {
        panic!("the spec-bearing program does not decode: {:?} at offset {}", e.err, e.offset)
    });
    let exported: Vec<String> =
        module.exported_functions().into_iter().map(|function| function.name).collect();
    assert_eq!(
        exported,
        ["twice_of_seven"],
        "the artifact must export the shipped half and nothing else; a specification \
         reaching the export list under any name, mangled or not, lands here"
    );
    assert_eq!(
        module.invoke("twice_of_seven", &[], FUEL),
        Outcome::Value(Some(spacewasm::Value::I32(14))),
        "the shipped half of a spec-bearing program must still compute"
    );
}

// ---------------------------------------------------------------------------
// The embedder's own bounds
// ---------------------------------------------------------------------------

/// The verifier's control-frame bound is a real gate, and the reference
/// configuration clears what the compiler emits.
///
/// `MAX_CONTROL_FRAMES` is a const generic on the decoder rather than a runtime
/// field, so "how deep may an embedder's nesting go" can only be asked by
/// decoding again at a different bound. This pins all three directions on two
/// modules: the reference 64 accepts a nest of conditionals, a bound of one
/// still accepts a body with no nested block — so the tight configuration is
/// not simply broken — and a bound of one refuses the nest. The reference pair
/// itself is pinned, since every claim this file makes is made against it.
///
/// The refusal is **not** `ControlFlowTooDeep`, which reads like the variant
/// written for it. In `spacewasm` 0.7.1 nothing ever constructs that variant:
/// the frame stack is a fixed-capacity vector and exceeding it surfaces as its
/// overflow, `AllocError(OutOfMemory)`. Asserting the variant a reader expects
/// rather than the one the decoder produces is how a report ends up promising a
/// diagnostic the runtime cannot emit.
///
/// Fails if the bound stops being enforced, if the reference configuration stops
/// accepting a fixture the compiler emits, or if a future version routes the
/// refusal through a different verdict — which is a signal, not a regression.
#[test]
fn the_control_frame_bound_is_enforced_and_the_reference_bound_clears_the_corpus() {
    const NESTED: &str = "\
pub fn depth(n: i32) -> i32 {
    let mut acc: i32 = 0;
    if n > 0 {
        if n > 1 {
            if n > 2 {
                acc = acc + 1;
            }
        }
    }
    return acc;
}
";
    const FLAT: &str = "pub fn answer() -> i32 { return 42; }";

    assert_eq!(
        (EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH),
        (64, 256),
        "this tier reports against `spacewasm_std`'s own configuration; moving either bound \
         re-measures every claim the sweeps above make"
    );

    let nested = codegen_for_target_no_analysis(NESTED, Target::SpaceWasm)
        .expect("a nest of conditionals is an ordinary program");
    let flat = codegen_for_target_no_analysis(FLAT, Target::SpaceWasm)
        .expect("the simplest program compiles");

    let mut session = SpaceWasmSession::acquire();
    decode(&mut session, nested.wasm()).expect("the reference bound accepts the nest");

    decode_with::<1, EMBEDDER_MAX_STACK_DEPTH>(&mut session, flat.wasm(), spacewasm::Vec::zero())
        .expect("one frame is the implicit function frame, which a flat body needs and no more");

    let error = decode_with::<1, EMBEDDER_MAX_STACK_DEPTH>(
        &mut session,
        nested.wasm(),
        spacewasm::Vec::zero(),
    )
    .err()
    .expect("a one-frame verifier cannot admit a nest of conditionals");
    assert_eq!(
        error.err.err,
        ValidationError::AllocError(AllocError::OutOfMemory),
        "the refusal must be the frame stack overflowing, not something else the tighter \
         run stumbled over"
    );
}

/// A decoded module reports how much IR it compiled to.
///
/// The number is what a flight embedder sizes its code budget from, which is why
/// it is measured here rather than asserted as a constant. Two modules of
/// visibly different size are read rather than one, so the accounting has to
/// track the writer: a footprint that stopped moving with the program — a
/// constant, a zeroed offset — would satisfy any single-module band and fails
/// the comparison below.
///
/// Fails if IR accounting stops working, or if one small function's IR stops
/// fitting on a single page.
#[test]
fn a_decoded_module_reports_its_ir_footprint() {
    const ONE_FUNCTION: &str = "pub fn answer() -> i32 { return 42; }";
    const SEVERAL: &str = "\
pub fn answer() -> i32 { return 42; }

pub fn twice(n: i32) -> i32 { return wrapping(n + n); }

pub fn thrice(n: i32) -> i32 { return wrapping(wrapping(n + n) + n); }

pub fn total() -> i32 { return wrapping(twice(7) + thrice(9)); }
";

    let small = codegen_for_target_no_analysis(ONE_FUNCTION, Target::SpaceWasm)
        .expect("the simplest program compiles");
    let large = codegen_for_target_no_analysis(SEVERAL, Target::SpaceWasm)
        .expect("several functions are an ordinary program");

    let mut session = SpaceWasmSession::acquire();
    let small_stats = {
        let module = decode(&mut session, small.wasm()).expect("the simplest program decodes");
        ir_stats(&module, small.wasm().len())
    };
    let large_stats = {
        let module = decode(&mut session, large.wasm()).expect("several functions decode");
        ir_stats(&module, large.wasm().len())
    };

    assert_eq!(small_stats.code_pages, 1, "one small function's IR fits on one page");
    assert!(small_stats.ir_words > 0, "a module with a body compiles to some IR");
    assert!(
        large_stats.ir_words > small_stats.ir_words,
        "the larger module compiled to {} IR words and the smaller to {}; the footprint is \
         not tracking what the code builder wrote",
        large_stats.ir_words,
        small_stats.ir_words
    );
    assert_eq!(
        (small_stats.wasm_bytes, large_stats.wasm_bytes),
        (small.wasm().len(), large.wasm().len()),
        "the footprint must report the bytes it was compiled from, or the IR-per-byte ratio \
         an embedder reads off it is a quotient of one measured number and one invented one"
    );
}

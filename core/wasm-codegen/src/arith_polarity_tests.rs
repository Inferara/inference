//! The committed golden corpus, compiled at both arithmetic polarities.
//!
//! Every `.wasm` under `tests/test_data/codegen/wasm` is what this crate emits
//! for the fixture beside it at the language's default arithmetic mode. That is
//! asserted here rather than only in the test crate, for two reasons the test
//! crate cannot reach: the fallback mode is crate-private with a test-only
//! override, so only an in-crate test can compile one source at both
//! polarities; and a corpus-wide sweep is what turns "this fixture's golden is
//! current" into "the goldens *are* the shipped default's output", which is the
//! claim a reader of the corpus makes.
//!
//! The second property is what the fallback earns its place with. A source with
//! no operator an annotation governs must produce one module at both
//! polarities, byte for byte — sections, name strings, locals declarations and
//! all. Roughly half the corpus is in that group, and it is the standing
//! evidence that moving the default touches arithmetic and nothing else: a
//! change that leaked into any other lowering would move a fixture that has no
//! arithmetic to move.
//!
//! Only the paired single-file layout is swept — a directory holding
//! `<stem>.inf` beside `<stem>.wasm` — because those are the fixtures whose
//! golden this crate produces on its own. A multi-file project golden is
//! assembled by the driver in the `inference` crate and is that crate's to pin.

use std::path::{Path, PathBuf};

use inference_ast::nodes::ArithMode;
use inference_type_checker::TypeCheckerBuilder;

use crate::compiler::Compiler;
use crate::target::{CompilationMode, EmitFeatures};
use crate::traverse_t_ast_with_compiler;

/// The golden family root, resolved from this crate's manifest directory.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("test_data")
        .join("codegen")
        .join("wasm")
}

/// A fixture in the paired single-file layout: its source and its golden.
struct Fixture {
    /// The fixture's path relative to the corpus root, for failure messages.
    name: String,
    source: String,
    golden: Vec<u8>,
}

/// Every paired single-file fixture under the corpus root, in path order.
///
/// The pairing rule is the corpus's own: a fixture directory holds
/// `<stem>.inf` and `<stem>.wasm` where `<stem>` is the directory's name. The
/// multi-file trees under `src/` do not match it and are skipped, as are the
/// second-copy goldens under `bulk_memory_golden/`, whose sources live with
/// their originals and are reached through them.
fn fixtures() -> Vec<Fixture> {
    let mut found = Vec::new();
    collect(&corpus_root(), &corpus_root(), &mut found);
    found.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(
        found.len() > 100,
        "the corpus walk found only {} paired fixtures, which is too few for the family it \
         is meant to sweep — the layout rule has probably drifted",
        found.len()
    );
    found
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Fixture>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if !path.is_dir() {
            continue;
        }
        let stem = path
            .file_name()
            .expect("a directory has a name")
            .to_string_lossy()
            .into_owned();
        let source_path = path.join(format!("{stem}.inf"));
        let golden_path = path.join(format!("{stem}.wasm"));
        if source_path.is_file() && golden_path.is_file() {
            out.push(Fixture {
                name: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned(),
                source: std::fs::read_to_string(&source_path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", source_path.display())),
                golden: std::fs::read(&golden_path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", golden_path.display())),
            });
        }
        collect(root, &path, out);
    }
}

/// Compiles `source` with the fallback arithmetic mode set to `mode`,
/// reproducing the configuration the golden was produced under: the default
/// target's compile mode, MVP features, bounds checks on, module name
/// `output`.
///
/// Analysis is not run. It does not reach the emitter, and several corpus
/// fixtures exist precisely because a rule refuses them.
fn compile_under(source: &str, mode: ArithMode) -> Option<Vec<u8>> {
    let parsed = inference_parser::parse(source);
    if !parsed.errors.is_empty() {
        return None;
    }
    let ctx = TypeCheckerBuilder::build_typed_context(parsed.arena)
        .ok()?
        .typed_context();
    let mut compiler = Compiler::new("output");
    compiler.set_emit_features(EmitFeatures::default());
    compiler.set_emit_bounds_checks(true);
    compiler.set_default_arith_mode(mode);
    let hspecs =
        traverse_t_ast_with_compiler(&ctx, &mut compiler, CompilationMode::Compile).ok()?;
    Some(compiler.finish_and_take(&hspecs).0)
}

/// Every committed golden is the module this crate emits at the language's
/// default arithmetic mode.
///
/// The test crate compares each fixture against its own golden one at a time;
/// this says the same thing about the corpus as a whole and from inside the
/// crate that produces it, so a golden nobody wrote a test for cannot go stale
/// unnoticed.
#[test]
#[cfg_attr(miri, ignore)]
fn every_committed_golden_is_the_default_polarity_module() {
    let mut skipped = Vec::new();
    for fixture in fixtures() {
        let Some(actual) = compile_under(&fixture.source, ArithMode::DEFAULT) else {
            skipped.push(fixture.name);
            continue;
        };
        assert_eq!(
            actual,
            fixture.golden,
            "`{}` is not the module the shipped default produces",
            fixture.name
        );
    }
    assert!(
        skipped.is_empty(),
        "every paired fixture must reach a module through this crate's own entry points, \
         but these did not: {skipped:?}"
    );
}

/// A fixture with no operator the annotation governs compiles to one module at
/// both polarities.
///
/// This is what "the fallback reaches arithmetic and nothing else" means as a
/// measurement. Comparing whole modules rather than lengths is deliberate: a
/// scratch pool reserved for a guard that is never emitted, or a name section
/// that moved, costs the same bytes at both polarities and would survive a size
/// comparison.
#[test]
#[cfg_attr(miri, ignore)]
fn a_fixture_without_governed_arithmetic_is_one_module_at_both_polarities() {
    let mut ungoverned = 0usize;
    let mut governed = 0usize;
    for fixture in fixtures() {
        let Some(wrapping) = compile_under(&fixture.source, ArithMode::Wrapping) else {
            continue;
        };
        let checked = compile_under(&fixture.source, ArithMode::Checked)
            .expect("a source one polarity compiles must compile at the other");
        if wrapping == checked {
            ungoverned += 1;
        } else {
            governed += 1;
            assert!(
                checked.len() > wrapping.len(),
                "`{}` differs across the polarities, so the checked one carries guards and \
                 must be the larger module",
                fixture.name
            );
        }
    }
    assert!(
        ungoverned > 40 && governed > 40,
        "both groups must be populated or this test is measuring one of them only: \
         {ungoverned} without governed arithmetic, {governed} with"
    );
}

/// The fixture the guard catalogue is written against is guarded throughout at
/// the shipped default and guarded nowhere at the modular one.
///
/// The corpus sweeps above are about the corpus; this one names the fixture a
/// reader goes to for the catalogue, so a change that emptied it would fail
/// with that fixture's name rather than as a count. It is also the file the
/// annotations were removed from when the default moved, and what makes that
/// removal safe is that its arithmetic still reaches the emitter through the
/// fallback — which is exactly the difference measured here.
#[test]
#[cfg_attr(miri, ignore)]
fn the_catalogue_fixture_is_guarded_only_at_the_checked_polarity() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| fixture.name.ends_with("checked_arith"))
        .expect("the catalogue fixture is part of the corpus");
    let wrapping = compile_under(&fixture.source, ArithMode::Wrapping)
        .expect("the catalogue fixture compiles");
    let checked = compile_under(&fixture.source, ArithMode::Checked)
        .expect("the catalogue fixture compiles");
    assert_ne!(
        wrapping, checked,
        "every row of the catalogue is written unannotated, so the fallback is the only thing \
         putting a guard in this module"
    );
    assert_eq!(
        checked, fixture.golden,
        "the catalogue golden is the guarded module"
    );
}


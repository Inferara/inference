//! Integration tests for the driver-side external-module orchestration
//! (`inference::wasm_link::resolve_external_modules`), which ties resolution and
//! validation together and reads the bytes the linker consumes.
//!
//! These tests drive the real front end (`parse` → `type_check`) so the extern
//! provenance the driver enumerates is produced exactly as a build produces it,
//! and resolve against a real temporary directory tree.

use std::path::{Path, PathBuf};

use inference::wasm_link::{
    resolve_external_modules, ExternalResolutionError, ManifestDeps, SearchPath,
};
use inference::{codegen, parse, type_check, TypedContext};

/// A self-cleaning temporary directory rooted under the OS temp dir.
struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(tag: &str) -> Self {
        let unique = format!(
            "inference-wasm-driver-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();
        TempTree { root }
    }

    /// Writes `bytes` at `relative` (creating parent dirs) and returns the path.
    fn write(&self, relative: impl AsRef<Path>, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Compiles `source` to a `.wasm` module via the real codegen path.
fn compile(source: &str, module_name: &str) -> Vec<u8> {
    let arena = parse(source).expect("source parses");
    let typed = type_check(arena).expect("source type-checks");
    codegen(&typed, module_name)
        .expect("codegen succeeds")
        .wasm()
        .to_vec()
}

/// Type-checks `source` into the context the driver enumerates externs from.
fn typed_of(source: &str) -> TypedContext {
    let arena = parse(source).expect("source parses");
    type_check(arena).expect("source type-checks")
}

#[test]
fn resolves_validates_and_reads_a_bound_extern() {
    let lib = compile("pub fn sum(a: i32, b: i32) -> i32 { return a + b; }", "arith");
    let tree = TempTree::new("ok");
    tree.write("arith.wasm", &lib);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let modules =
        resolve_external_modules(&typed, &search, None)
            .expect("resolution succeeds")
            .modules;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].logical_module, "arith");
    assert_eq!(modules[0].bytes, lib);
}

/// A `use … from host::<module>;` binding is refused before resolution, and the
/// refusal does not depend on what is on the search path.
///
/// This is the whole point of the guard. Left to resolve, `env` finds the
/// planted `env.wasm`, validates against the declared signature, merges, and the
/// import is stripped — an artifact that silently replaces the embedder the
/// author asked for with a body this build chose, and not one diagnostic
/// anywhere says so. `env` is the canonical host-module name, so the fixture is
/// the shape a user hits, not a contrived collision.
///
/// Three facts make that claim, rather than one assertion that an error came
/// back:
///
/// 1. The planted module is one this build really would link. The *same* tree
///    and the *same* declared signature, bound by a linked clause, resolve and
///    validate and yield the module's bytes. Without that control the refusal
///    below would be indistinguishable from a resolution that was going to fail
///    anyway — a fixture whose planted file did not validate would pin nothing
///    about the guard.
/// 2. Only the clause differs between the two halves, and the host one is
///    refused by the exact variant that names the module.
/// 3. With the file absent from an otherwise identical search path, the error
///    comes back rendered identically. An outcome that does not move when the
///    filesystem does cannot have been produced by looking at the filesystem,
///    which is what "refused *before* resolution" means — and it is what keeps
///    the refusal honest for the reader whose `env.wasm` is simply missing.
#[test]
fn a_host_binding_is_refused_even_when_a_module_of_that_name_resolves() {
    const DECL: &str = "external fn clock_ms(a: i32) -> i32;";
    const CALL: &str = "pub fn use_it(x: i32) -> i32 { return clock_ms(x); }";

    let lib = compile("pub fn clock_ms(a: i32) -> i32 { return a; }", "env");
    let tree = TempTree::new("host-collision");
    tree.write("env.wasm", &lib);
    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let linked = typed_of(&format!("{DECL}\nuse {{ clock_ms }} from env;\n{CALL}"));
    let resolved = resolve_external_modules(&linked, &search, None)
        .expect("the planted module resolves and validates for a linked clause")
        .modules;
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].logical_module, "env");
    assert_eq!(
        resolved[0].bytes, lib,
        "the guard below has to be refusing a module this build would otherwise have linked"
    );

    let host = typed_of(&format!("{DECL}\nuse {{ clock_ms }} from host::env;\n{CALL}"));
    let err = resolve_external_modules(&host, &search, None)
        .expect_err("a host binding must not be satisfied off the search path");
    assert!(
        matches!(
            &err,
            ExternalResolutionError::HostImportUnsupported { module } if module == "env"
        ),
        "got: {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("`use { … } from host::env;`")
            && rendered.contains("issue #464 is where that work is tracked"),
        "the refusal must quote the clause and name where the work lives, got: {rendered}"
    );
    assert!(
        rendered.contains("silently replacing the provider you asked for"),
        "the reader has to be told why a resolvable module is still refused, got: {rendered}"
    );
    assert!(
        rendered.contains(
            "If you meant the linked `.wasm` module `host::env`, rename it: a module path whose \
             first segment is `host` is reserved in this position"
        ),
        "a two-segment `from host::…;` clause type-checks silently, so this refusal is the only \
         place an author whose linked module was reclassified by the reservation is told to \
         rename it, got: {rendered}"
    );

    let bare = TempTree::new("host-absent");
    let mut empty_search = SearchPath::new();
    empty_search.push_lib_dir(bare.root().to_path_buf());
    let absent = resolve_external_modules(&host, &empty_search, None)
        .expect_err("a host binding is refused with nothing to resolve either");
    assert!(
        matches!(
            &absent,
            ExternalResolutionError::HostImportUnsupported { module } if module == "env"
        ),
        "got: {absent:?}"
    );
    assert_eq!(
        absent.to_string(),
        rendered,
        "the refusal must not move when the search path does, or it was decided after \
         resolution rather than before it"
    );
}

#[test]
fn resolves_a_bound_extern_through_a_manifest_entry() {
    // The manifest binds the logical module to a `.wasm` whose name on disk does
    // not match the logical name — only a manifest entry (not the search path)
    // could resolve it, proving the manifest feeds the driver end to end.
    let lib = compile("pub fn sum(a: i32, b: i32) -> i32 { return a + b; }", "arith");
    let tree = TempTree::new("manifest-ok");
    let on_disk = tree.write("vendor/arith-1.2.3.wasm", &lib);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut manifest = ManifestDeps::new();
    manifest.insert("arith", on_disk);

    let modules = resolve_external_modules(&typed, &SearchPath::new(), Some(&manifest))
        .expect("manifest resolution succeeds")
        .modules;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].logical_module, "arith");
    assert_eq!(modules[0].bytes, lib);
}

#[test]
fn manifest_entry_overrides_a_search_path_directory() {
    // Both the manifest and a `-L` directory carry `arith`, but the search-path
    // copy has the WRONG signature. If the manifest did not win, validation
    // against the search-path module would fail — so a clean resolution proves
    // the manifest took priority.
    let right = compile("pub fn sum(a: i32, b: i32) -> i32 { return a + b; }", "arith");
    let wrong = compile("pub fn sum(a: i32) -> i32 { return a; }", "arith");
    let tree = TempTree::new("manifest-override");
    let manifest_target = tree.write("vendor/arith.wasm", &right);
    tree.write("lib/arith.wasm", &wrong);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().join("lib"));
    let mut manifest = ManifestDeps::new();
    manifest.insert("arith", manifest_target);

    let modules = resolve_external_modules(&typed, &search, Some(&manifest))
        .expect("manifest must override the wrong search-path module")
        .modules;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].bytes, right);
}

#[test]
fn a_program_without_externs_resolves_to_an_empty_set() {
    let typed = typed_of("pub fn double(x: i32) -> i32 { return x + x; }");
    let externals = resolve_external_modules(&typed, &SearchPath::new(), None).unwrap();
    assert!(externals.modules.is_empty());
    assert!(externals.contracts.is_empty());
}

#[test]
fn unresolved_module_is_a_resolve_error() {
    // The extern is bound, but no search directory contains `arith.wasm`.
    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let tree = TempTree::new("missing");
    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    assert!(
        matches!(err, ExternalResolutionError::Resolve(_)),
        "expected a resolve error, got {err:?}"
    );
}

#[test]
fn signature_mismatch_is_a_validate_error() {
    // The library exports `sum` taking two i32s, but the declaration claims a
    // single i32 parameter — validation must reject it distinctly from a miss.
    let lib = compile("pub fn sum(a: i32, b: i32) -> i32 { return a + b; }", "arith");
    let tree = TempTree::new("mismatch");
    tree.write("arith.wasm", &lib);

    let typed = typed_of(
        "external fn sum(a: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    assert!(
        matches!(err, ExternalResolutionError::Validate { .. }),
        "expected a validate error, got {err:?}"
    );
}

#[test]
fn bound_top_level_extern_validates_against_its_own_declaration_not_a_spec_sibling() {
    // H10: a bound top-level `external fn sort(i32)->i32` matches the library,
    // while a same-named spec-inner `external fn sort(i32,i32)->i32` is a
    // distinct, unbound declaration. The driver must validate the resolved
    // library against the *bound* top-level declaration (recovered by DefId),
    // not whichever same-named declaration last won a bare-name map slot. With
    // the prior bare-name keying, the spec's `(i32,i32)` overwrote the slot and
    // this resolved to a bogus signature-mismatch rejection.
    let lib = compile("pub fn sort(a: i32) -> i32 { return a; }", "sorting");
    let tree = TempTree::new("h10");
    tree.write("sorting.wasm", &lib);

    let typed = typed_of(
        "external fn sort(a: i32) -> i32;\n\
         use { sort } from sorting;\n\
         pub fn top(x: i32) -> i32 { return sort(x); }\n\
         spec Ms {\n\
             external fn sort(a: i32, b: i32) -> i32;\n\
         }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let modules = resolve_external_modules(&typed, &search, None)
        .expect("the bound top-level `sort(i32)` must validate against the library")
        .modules;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].logical_module, "sorting");
}

#[test]
fn export_not_found_is_a_validate_error() {
    // The library exports `add`, not the `sum` the program binds.
    let lib = compile("pub fn add(a: i32, b: i32) -> i32 { return a + b; }", "arith");
    let tree = TempTree::new("noexport");
    tree.write("arith.wasm", &lib);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    assert!(
        matches!(err, ExternalResolutionError::Validate { .. }),
        "expected a validate error for the missing export, got {err:?}"
    );
}

#[test]
fn two_externs_from_one_module_dedup_to_a_single_entry() {
    // Both `sum` and `diff` come from the same `arith` library. They resolve to
    // the same `.wasm` path, so the driver must read the bytes once and return a
    // single deduplicated module entry — exercising the by-path cache.
    let lib = compile(
        "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }\n\
         pub fn diff(a: i32, b: i32) -> i32 { return a - b; }",
        "arith",
    );
    let tree = TempTree::new("dedup");
    tree.write("arith.wasm", &lib);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         external fn diff(a: i32, b: i32) -> i32;\n\
         use { sum, diff } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, diff(x, 1)); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let modules = resolve_external_modules(&typed, &search, None)
        .expect("resolution succeeds")
        .modules;
    assert_eq!(
        modules.len(),
        1,
        "two externs from one library must dedup to one module entry"
    );
    assert_eq!(modules[0].logical_module, "arith");
    assert_eq!(modules[0].bytes, lib);
}

#[test]
fn two_distinct_modules_yield_one_entry_each_keyed_by_logical_module() {
    // C4: two libraries bound under distinct logical modules must each produce
    // their own resolved entry, carrying their own logical-module label, so the
    // linker can match each import's recorded `(module, field)` on the right
    // external rather than the first that merely exports the field name.
    let adder = compile("pub fn add_op(a: i32, b: i32) -> i32 { return a + b; }", "adder");
    let subber = compile("pub fn sub_op(a: i32, b: i32) -> i32 { return a - b; }", "subber");
    let tree = TempTree::new("twomods");
    tree.write("adder.wasm", &adder);
    tree.write("subber.wasm", &subber);

    let typed = typed_of(
        "external fn add_op(a: i32, b: i32) -> i32;\n\
         external fn sub_op(a: i32, b: i32) -> i32;\n\
         use { add_op } from adder;\n\
         use { sub_op } from subber;\n\
         pub fn use_it(x: i32) -> i32 { return add_op(x, sub_op(x, 1)); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let modules = resolve_external_modules(&typed, &search, None)
        .expect("resolution succeeds")
        .modules;
    let logical: Vec<&str> = modules.iter().map(|m| m.logical_module.as_str()).collect();
    assert_eq!(
        logical,
        vec!["adder", "subber"],
        "each distinct logical module must get its own entry, sorted deterministically"
    );
}

/// Builds a structurally-decodable module exporting `sum:(i32,i32)->i32` whose
/// body is malformed: it returns nothing while the signature promises an i32, so
/// it decodes (and signature-validates) but fails full WASM validation. This is
/// the H4 shape — a malformed-but-decodable external the body-blind
/// `validate_extern` would otherwise wave through into the linker.
fn malformed_but_decodable_sum() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);
    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    // Empty body: `end` with no value pushed, but the type demands an i32 result.
    let mut func = Function::new([]);
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    module.finish()
}

/// Builds a *valid* external exporting `sum:(i32,i32)->i32` whose body uses a
/// SIMD `v128.const` (immediately dropped). The module is well-formed WebAssembly
/// — it passes the structural validation pass — but SIMD is outside the linker's
/// supported WASM 1.0 subset, so the driver's gate must reject it as an
/// unsupported feature, not as malformed.
fn simd_external_sum() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);
    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut func = Function::new([]);
    func.instruction(&Instruction::V128Const(0));
    func.instruction(&Instruction::Drop);
    func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::LocalGet(1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    module.finish()
}

/// Builds a *valid* external exporting `sum:(i32,i32)->i32` whose body uses a
/// floating-point op (`f32.add` over two `f32.const`, immediately dropped). The
/// module is well-formed WebAssembly — it passes the structural validation pass —
/// but the Inference language has no `f32`/`f64` types and the linker's gate drops
/// the baseline `FLOATS` flag, so the driver's gate must reject it as an
/// unsupported feature naming floating point, not as malformed.
fn float_external_sum() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);
    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut func = Function::new([]);
    func.instruction(&Instruction::F32Const(1.0.into()));
    func.instruction(&Instruction::F32Const(1.0.into()));
    func.instruction(&Instruction::F32Add);
    func.instruction(&Instruction::Drop);
    func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::LocalGet(1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    module.finish()
}

#[test]
fn a_non_wasm1_external_is_rejected_as_unsupported_feature() {
    // Driver alignment: a well-formed external that uses a post-1.0 proposal
    // (SIMD here) must be rejected at the earliest point — when the driver
    // resolves it — with the same feature-named diagnostic the linker's gate
    // produces, distinct from a malformed-module `Invalid`. The gate is a single
    // source of truth: the driver delegates to `inference_wasm_linker`'s
    // `validate_external`.
    let tree = TempTree::new("simd-external");
    tree.write("arith.wasm", &simd_external_sum());

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    match err {
        ExternalResolutionError::UnsupportedFeature {
            logical_module,
            ref path,
            ref reason,
        } => {
            assert_eq!(logical_module, "arith");
            assert!(path.ends_with("arith.wasm"), "names the offending file: {path:?}");
            assert!(
                reason.contains("SIMD"),
                "the diagnostic names the unsupported feature: {reason}"
            );
        }
        other => panic!("expected an UnsupportedFeature error, got {other:?}"),
    }
}

#[test]
fn a_floating_point_external_is_rejected_as_unsupported_feature() {
    // Driver alignment: a well-formed external whose body uses a float op is
    // rejected at resolution time with the same feature-named diagnostic the
    // linker's gate produces. The Inference language has no `f32`/`f64` types, so
    // floating point is outside the supported subset — distinct from a
    // malformed-module `Invalid`.
    let tree = TempTree::new("float-external");
    tree.write("arith.wasm", &float_external_sum());

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    match err {
        ExternalResolutionError::UnsupportedFeature {
            logical_module,
            ref path,
            ref reason,
        } => {
            assert_eq!(logical_module, "arith");
            assert!(path.ends_with("arith.wasm"), "names the offending file: {path:?}");
            assert!(
                reason.contains("floating-point"),
                "the diagnostic names floating point: {reason}"
            );
        }
        other => panic!("expected an UnsupportedFeature error, got {other:?}"),
    }
}

#[test]
fn malformed_but_decodable_external_is_rejected_as_invalid() {
    // H4: the export signature matches, so `validate_extern` alone would accept
    // it. The full-validation gate must reject the malformed body distinctly,
    // before any byte reaches the linker.
    let tree = TempTree::new("invalid-body");
    tree.write("arith.wasm", &malformed_but_decodable_sum());

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    match err {
        ExternalResolutionError::Invalid {
            logical_module,
            ref path,
            ..
        } => {
            assert_eq!(logical_module, "arith");
            assert!(path.ends_with("arith.wasm"), "names the offending file: {path:?}");
        }
        other => panic!("expected an Invalid error, got {other:?}"),
    }
}

#[test]
fn an_oversized_external_is_rejected_before_being_read() {
    // H19: a file larger than the cap must be rejected as TooLarge, never read
    // fully into memory. The fixture is just over the limit by a single byte; a
    // sparse multi-GB bait file would behave the same without the disk cost.
    use inference::wasm_link::MAX_EXTERNAL_MODULE_BYTES;

    let tree = TempTree::new("too-large");
    let path = tree.root().join("arith.wasm");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_EXTERNAL_MODULE_BYTES + 1).unwrap();
    drop(file);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    match err {
        ExternalResolutionError::TooLarge { size, limit, .. } => {
            assert_eq!(limit, MAX_EXTERNAL_MODULE_BYTES);
            assert!(size > limit, "reports the offending size: {size} > {limit}");
        }
        other => panic!("expected a TooLarge error, got {other:?}"),
    }
}

#[test]
fn a_file_at_the_size_limit_is_still_read() {
    // Boundary: exactly at the cap is accepted (the body is then rejected as
    // invalid WASM, proving the read happened rather than tripping TooLarge).
    use inference::wasm_link::MAX_EXTERNAL_MODULE_BYTES;

    let tree = TempTree::new("at-limit");
    let path = tree.root().join("arith.wasm");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_EXTERNAL_MODULE_BYTES).unwrap();
    drop(file);

    let typed = typed_of(
        "external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from arith;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let err = resolve_external_modules(&typed, &search, None).unwrap_err();
    assert!(
        !matches!(err, ExternalResolutionError::TooLarge { .. }),
        "a file exactly at the limit must be read, not rejected as too large: {err:?}"
    );
}

#[test]
fn nested_logical_module_resolves_under_subdirectory() {
    let lib = compile("pub fn hash(a: i32) -> i32 { return a; }", "sha256");
    let tree = TempTree::new("nested");
    tree.write(Path::new("crypto").join("sha256.wasm"), &lib);

    let typed = typed_of(
        "external fn hash(a: i32) -> i32;\n\
         use { hash } from crypto::sha256;\n\
         pub fn use_it(x: i32) -> i32 { return hash(x); }",
    );

    let mut search = SearchPath::new();
    search.push_lib_dir(tree.root().to_path_buf());

    let modules = resolve_external_modules(&typed, &search, None)
        .unwrap()
        .modules;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].logical_module, "crypto::sha256");
}

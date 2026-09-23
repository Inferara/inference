//! Integration tests for the driver-side external-module orchestration
//! (`inference::wasm_link::resolve_external_modules`), which ties resolution and
//! validation together and reads the bytes the linker consumes.
//!
//! These tests drive the real front end (`parse` → `type_check`) so the extern
//! provenance the driver enumerates is produced exactly as a build produces it,
//! and resolve against a real temporary directory tree.

use std::path::{Path, PathBuf};

use inference::wasm_link::{
    resolve_external_modules, DeclaredSignature, ExternalResolutionError, HostImport,
    HostImportError, ManifestDeps, ResolvedExternals, SearchPath, WasmValType,
};
use inference::{codegen, link_resolved, parse, type_check, LinkOptions, TypedContext};

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

/// The `(module, field)` of every function import in `wasm`, in section order.
fn function_imports(wasm: &[u8]) -> Vec<(String, String)> {
    let mut imports = Vec::new();
    for payload in inf_wasmparser::Parser::new(0).parse_all(wasm) {
        if let inf_wasmparser::Payload::ImportSection(reader) = payload.expect("valid payload") {
            for import in reader {
                let import = import.expect("valid import");
                if matches!(import.ty, inf_wasmparser::TypeRef::Func(_)) {
                    imports.push((import.module.to_string(), import.name.to_string()));
                }
            }
        }
    }
    imports
}

/// Type-checks a multi-file program: `(module_path, source)` pairs with the
/// entry file first, folded into one arena as the project front end folds a
/// source tree.
///
/// A cross-*file* fixture is the only way to state a disagreement between two
/// declarations of one imported function: within one file the two would be one
/// name registered twice, which the symbol table rejects long before the driver
/// sees either.
fn typed_of_multi(files: &[(Vec<&str>, &str)]) -> TypedContext {
    let mut arena = inference_ast::arena::AstArena::default();
    for (module_path, source) in files {
        let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
        let parsed = inference_parser::parse_into(arena, source, module_path);
        assert!(
            parsed.errors.is_empty(),
            "multi-file source has syntax errors: {:?}",
            parsed.errors
        );
        arena = parsed.arena;
    }
    type_check(arena).expect("multi-file source type-checks")
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

/// A **linked** extern declared `-> ()` is validated against a library export
/// that returns nothing, and refused against one that returns a value.
///
/// The declaration lowers to the empty result list the import is emitted with,
/// so the library this validation accepts is the one whose export agrees with
/// the import the artifact declares. A lowering that answered `(i32)` here would
/// accept the value-returning library instead, and the merge that followed
/// would splice a value-returning body into a slot typed for none.
#[test]
fn a_linked_extern_returning_unit_is_validated_against_a_void_export() {
    let void_lib = compile("pub fn ping() -> () { return; }", "beeper");
    let valued_lib = compile("pub fn ping() -> i32 { return 1; }", "beeper");

    let typed = typed_of(
        "external fn ping() -> ();\n\
         use { ping } from beeper;\n\
         pub fn use_it(x: i32) -> i32 { ping(); return x; }",
    );

    let void_tree = TempTree::new("unit-void");
    void_tree.write("beeper.wasm", &void_lib);
    let mut void_search = SearchPath::new();
    void_search.push_lib_dir(void_tree.root().to_path_buf());
    let resolved = resolve_external_modules(&typed, &void_search, None)
        .expect("`-> ()` matches an export that returns nothing");
    assert_eq!(resolved.modules.len(), 1);
    assert_eq!(resolved.modules[0].bytes, void_lib);

    let valued_tree = TempTree::new("unit-valued");
    valued_tree.write("beeper.wasm", &valued_lib);
    let mut valued_search = SearchPath::new();
    valued_search.push_lib_dir(valued_tree.root().to_path_buf());
    let err = resolve_external_modules(&typed, &valued_search, None)
        .expect_err("`-> ()` cannot be backed by an export that returns a value");
    let rendered = err.to_string();
    assert!(
        rendered.contains("declared () -> ()") && rendered.contains("found () -> (i32)"),
        "the declaration says the call leaves nothing on the stack, and the library says it \
         leaves an i32; both signatures have to be shown: {rendered}"
    );
}

/// A `use … from host::<module>;` binding is answered without the filesystem
/// being consulted at all, and the answer is the declaration rather than bytes.
///
/// That is the whole point of routing host origins away from resolution. Left to
/// resolve, `env` finds the planted `env.wasm`, validates against the declared
/// signature, merges, and the import is stripped — an artifact that silently
/// replaces the embedder the author asked for with a body this build chose, and
/// not one diagnostic anywhere says so. `env` is the canonical host-module name,
/// so the fixture is the shape a user hits, not a contrived collision.
///
/// Three facts make that claim, rather than one assertion that a host import
/// came back:
///
/// 1. The planted module is one this build really would link. The *same* tree
///    and the *same* declared signature, bound by a linked clause, resolve and
///    validate and yield the module's bytes. Without that control, the host
///    half below would be indistinguishable from a resolution that was going to
///    fail anyway — a fixture whose planted file did not validate would pin
///    nothing about which path was taken.
/// 2. Only the clause differs between the two halves, and the host one yields no
///    modules and no contracts: nothing was read, and nothing is owed to the
///    merge that is not going to happen.
/// 3. With the file absent from an otherwise identical search path, the answer
///    comes back identical. An outcome that does not move when the filesystem
///    does cannot have been produced by looking at the filesystem, which is what
///    "resolved without disk access" means.
#[test]
fn a_host_binding_is_answered_without_reading_the_filesystem() {
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
        "the host half below has to be bypassing a module this build would otherwise have linked"
    );

    let host = typed_of(&format!("{DECL}\nuse {{ clock_ms }} from host::env;\n{CALL}"));
    let with_file = resolve_external_modules(&host, &search, None)
        .expect("a host binding needs nothing on the search path");
    assert!(
        with_file.modules.is_empty() && with_file.contracts.is_empty(),
        "a host import contributes no bytes to merge and no contract to check; got {:?} / {:?}",
        with_file.modules,
        with_file.contracts
    );
    assert_eq!(
        with_file.host_imports,
        vec![HostImport {
            module: "env".into(),
            field: "clock_ms".into(),
            signature: DeclaredSignature {
                params: vec![WasmValType::I32],
                results: vec![WasmValType::I32],
            },
        }],
        "the declaration itself is the answer, under the module string an embedder registers"
    );

    let bare = TempTree::new("host-absent");
    let mut empty_search = SearchPath::new();
    empty_search.push_lib_dir(bare.root().to_path_buf());
    let without_file = resolve_external_modules(&host, &empty_search, None)
        .expect("a host binding resolves with nothing on the search path either");
    assert_eq!(
        without_file.host_imports, with_file.host_imports,
        "the answer must not move when the filesystem does, or it was produced by reading it"
    );
}

/// A host extern that is declared and bound but never called still yields a host
/// import, and still ships as one.
///
/// Surprising, and deliberately pinned rather than left to be discovered. Code
/// generation registers an import per `external fn` **declaration**, not per
/// call site, so a binding written ahead of the code that will use it puts a
/// real import in the artifact — and the module will not instantiate until an
/// embedder offers a function for it. The driver has to agree with that, or the
/// artifact and the declared set part company at the first unused declaration.
#[test]
fn a_bound_but_uncalled_host_extern_is_still_declared_and_still_ships() {
    let typed = typed_of(
        "external fn telemetry(a: i32);\n\
         use { telemetry } from host::fprime_core;\n\
         pub fn use_it(x: i32) -> i32 { return x + 1; }",
    );

    let externals = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect("a host binding resolves with no search path at all");
    assert_eq!(
        externals.host_imports,
        vec![HostImport {
            module: "fprime_core".into(),
            field: "telemetry".into(),
            signature: DeclaredSignature {
                params: vec![WasmValType::I32],
                results: Vec::new(),
            },
        }]
    );

    let artifact = codegen(&typed, "uncalled").expect("codegen succeeds");
    let linked = link_resolved(artifact.wasm(), &externals, &LinkOptions::default())
        .expect("the emitted imports match the declared set");
    assert_eq!(
        linked.wasm,
        artifact.wasm(),
        "a host program's artifact is the codegen output, byte for byte"
    );
    assert_eq!(
        function_imports(&linked.wasm),
        vec![("fprime_core".to_string(), "telemetry".to_string())],
        "an uncalled declaration still ships an import for an embedder to satisfy"
    );
}

/// The whole pipeline for a host program: the codegen output ships unchanged and
/// still carries the import the embedder is asked for.
#[test]
fn a_host_program_ships_the_codegen_bytes_unchanged() {
    let typed = typed_of(
        "external fn clock_ms() -> i64;\n\
         use { clock_ms } from host::env;\n\
         pub fn now() -> i64 { return clock_ms(); }",
    );
    let externals = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect("a host binding resolves with no search path at all");

    let artifact = codegen(&typed, "clock").expect("codegen succeeds");
    let linked = link_resolved(artifact.wasm(), &externals, &LinkOptions::default())
        .expect("the emitted imports match the declared set");
    assert_eq!(
        linked.wasm,
        artifact.wasm(),
        "the link step must not touch a byte of a host program's artifact"
    );
    assert!(
        linked.warnings.is_empty(),
        "nothing was merged, so nothing is owed: {:?}",
        linked.warnings
    );
    assert_eq!(
        function_imports(&linked.wasm),
        vec![("env".to_string(), "clock_ms".to_string())],
        "the import the author asked an embedder for has to survive the link step"
    );
}

/// A program that binds some externs to host modules and others to linked
/// `.wasm` modules is refused, and refused *before* anything is resolved.
///
/// The linked module named here could not resolve under any circumstances — the
/// search path is empty and nothing of that name exists — so a `NotFound` would
/// prove resolution ran first. That ordering is the substance of the test, not a
/// detail: an author who is midway through changing how a module is provided
/// very likely has a missing `.wasm` as well, and being sent to fix a path when
/// the clause is what has to change costs them the whole diagnosis.
#[test]
fn a_mixed_host_and_linked_program_is_refused_before_anything_is_resolved() {
    const COST: &str =
        "the embedder then has to supply every function this program binds to `nowhere_on_disk`";
    const TO_LINKED: &str = "drop `host::` from the `use … from` clauses that bind `env` and \
                             provide a `.wasm` file for each module they then name — one file per \
                             module, never one per function";

    let typed = typed_of(
        "external fn clock_ms() -> i64;\n\
         use { clock_ms } from host::env;\n\
         external fn sum(a: i32, b: i32) -> i32;\n\
         use { sum } from nowhere_on_disk;\n\
         pub fn use_it(x: i32) -> i32 { return sum(x, 1); }",
    );

    let err = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect_err("the two kinds of provider cannot be combined yet");
    assert!(
        matches!(err, ExternalResolutionError::MixedHostAndLinked { .. }),
        "resolution must not have run: {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("`env`.`clock_ms`"),
        "the message names every host import: {rendered}"
    );
    assert!(
        rendered.contains("`nowhere_on_disk`"),
        "the message names every linked module: {rendered}"
    );
    assert!(
        rendered.contains("no import section"),
        "the reader is told why the two cannot be combined: {rendered}"
    );
    assert!(
        rendered.contains(COST),
        "the half that moves everything to host imports must say what it costs, and say it \
         against the modules it is talking about: taking it stops those `.wasm` files being read \
         at all, with no warning anywhere, which is the substitution the reservation exists to \
         prevent: {rendered}"
    );
    assert!(
        rendered.contains(TO_LINKED),
        "the other half must name the clauses it edits by the host modules they bind, and say \
         what to provide instead, counted the way files are actually supplied: two host \
         functions of one module are one `.wasm`, so a remedy counted in functions asks for a \
         file no clause can name: {rendered}"
    );
    assert!(
        !rendered.contains("the clauses above"),
        "the message prints the host imports as pairs and no clause at all, so nothing in it can \
         point back at one: {rendered}"
    );
    assert!(
        rendered.contains("bind every extern to its host spelling (`host::nowhere_on_disk`)"),
        "every linked name here is one flat segment, so each is already a legal host module \
         string and the clause that would have worked can be spelled out; a `<module>` \
         placeholder buries the one fix under a form to fill in: {rendered}"
    );
    assert!(
        !rendered.contains("<module>"),
        "the placeholder must not survive where every real name is in hand: {rendered}"
    );
    assert!(
        !rendered.contains("not a rewrite of the clauses that bind them"),
        "a flat linked name IS a straight rewrite — telling this author their clauses cannot be \
         rewritten sends them renaming a module that needs no new name: {rendered}"
    );
}

/// The mixed refusal does not offer a straight rewrite it knows cannot be
/// written.
///
/// A host module is one flat segment, so there is no host spelling of the linked
/// module `crypto::sha256` at all: an author handed `host::crypto::sha256` would
/// be met by the front end's nested-module refusal, one build later. Where any
/// linked name is a `::` path, the remedy leads with the direction that *is* a
/// straight edit, says the other is a renaming exercise, and spells no host
/// clause — the sibling test above, where every linked name is flat, is where
/// the clauses are written out in full.
#[test]
fn the_mixed_refusal_does_not_offer_a_rewrite_a_nested_module_cannot_take() {
    let typed = typed_of(
        "external fn clock_ms() -> i64;\n\
         use { clock_ms } from host::env;\n\
         external fn digest(a: i32) -> i32;\n\
         use { digest } from crypto::sha256;\n\
         pub fn use_it(x: i32) -> i32 { return digest(x); }",
    );

    let rendered = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect_err("the two kinds of provider cannot be combined yet")
        .to_string();
    assert!(
        rendered.contains("`crypto::sha256`"),
        "the linked module is still named: {rendered}"
    );
    assert!(
        rendered.contains("not a rewrite of the clauses that bind them")
            && rendered.contains("one flat segment, never a `::` path"),
        "an author whose linked module is a path has to be told that moving it to a host import \
         means choosing a new name, not prefixing the old one — and told it about the clauses \
         that bind the linked modules, which are not the `host::` clauses the other direction \
         says to edit: {rendered}"
    );
    assert!(
        !rendered.contains("`host::crypto::sha256`"),
        "the branch exists so that no host spelling of a `::` path is ever printed; one would \
         earn the front end's nested-module refusal on the next build: {rendered}"
    );
    let straight_edit = rendered
        .find("drop `host::` from the `use … from` clauses that bind `env`")
        .unwrap_or_else(|| panic!("the straight edit is offered: {rendered}"));
    let renaming = rendered
        .find("move every extern to a host import")
        .unwrap_or_else(|| panic!("the renaming exercise is offered: {rendered}"));
    assert!(
        straight_edit < renaming,
        "the direction that is a straight edit leads, and the renaming exercise follows: an \
         author who reads only the first instruction must not be sent into a second refusal: \
         {rendered}"
    );
}

/// Two files declaring one host `(module, field)` identically are one import,
/// and the parameter *names* they use are not part of "identically".
///
/// The control the two rejections below are read against, and the only pin on
/// two deliberate decisions. A comparison written on the whole lowered
/// declaration — which derives `PartialEq` and carries the declared parameter
/// names — would reject this program, and nothing else in the suite would go
/// red; names take no part in codegen's interning and cannot reach the artifact,
/// so two files calling one imported argument `a` and `count` have not disagreed
/// about anything. The single entry is the other half: were the second
/// declaration appended rather than folded into the first, the artifact would
/// carry one import against two equal declarations, and the check below would
/// refuse a build for a `Missing` import the module does in fact have.
#[test]
fn two_files_agreeing_on_a_host_import_declare_it_once() {
    let typed = typed_of_multi(&[
        (
            Vec::new(),
            "use sib;\n\
             external fn clock_ms(a: i32) -> i64;\n\
             use { clock_ms } from host::env;\n\
             pub fn now() -> i64 { return clock_ms(1); }\n\
             pub fn sibling() -> i64 { return sib::also_now(); }\n",
        ),
        (
            vec!["sib"],
            "external fn clock_ms(count: i32) -> i64;\n\
             use { clock_ms } from host::env;\n\
             pub fn also_now() -> i64 { return clock_ms(2); }\n",
        ),
    ]);

    let externals = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect("two files may describe one host import, as long as they agree");
    assert_eq!(
        externals.host_imports,
        vec![HostImport {
            module: "env".into(),
            field: "clock_ms".into(),
            signature: DeclaredSignature {
                params: vec![WasmValType::I32],
                results: vec![WasmValType::I64],
            },
        }],
        "one `(module, field)` is one import however many files declare it"
    );

    let artifact = codegen(&typed, "agreeing").expect("codegen succeeds");
    let linked = link_resolved(artifact.wasm(), &externals, &LinkOptions::default())
        .expect("the emitted imports match the declared set");
    assert_eq!(
        linked.wasm,
        artifact.wasm(),
        "a host program's artifact is the codegen output, byte for byte"
    );
    assert_eq!(
        function_imports(&linked.wasm),
        vec![("env".to_string(), "clock_ms".to_string())],
        "the artifact asks the embedder for the function once, as the declared set does"
    );
}

/// Two files declaring one host `(module, field)` at different signatures are
/// rejected.
///
/// Nothing else would catch it. A linked external has a `.wasm` both
/// declarations are checked against; a host import has no such oracle, and code
/// generation would answer the disagreement by emitting *two* imports of one
/// two-level name — an artifact no embedder can satisfy.
#[test]
fn two_files_disagreeing_on_a_host_imports_signature_are_rejected() {
    let typed = typed_of_multi(&[
        (
            Vec::new(),
            "use sib;\n\
             external fn clock_ms() -> i64;\n\
             use { clock_ms } from host::env;\n\
             pub fn now() -> i64 { return clock_ms(); }\n\
             pub fn sibling() -> i32 { return sib::now_small(); }\n",
        ),
        (
            vec!["sib"],
            "external fn clock_ms() -> i32;\n\
             use { clock_ms } from host::env;\n\
             pub fn now_small() -> i32 { return clock_ms(); }\n",
        ),
    ]);

    let err = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect_err("two signatures for one host import must be rejected");
    assert!(
        matches!(err, ExternalResolutionError::ConflictingHostSignature { .. }),
        "got: {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("conflicting signatures for host import `clock_ms`")
            && rendered.contains("module `env`"),
        "the rejection names the import and the module it is bound to: {rendered}"
    );
    assert!(
        rendered.contains("the entry file") && rendered.contains("`sib`"),
        "both declaring files are named, since the fix is to make them agree and neither is \
         wrong on its own: {rendered}"
    );
    assert!(
        rendered.contains("(i64)") && rendered.contains("(i32)"),
        "both signatures are shown, in the lowered form the compiler compares: {rendered}"
    );
}

/// Two files declaring one host `(module, field)` with different `mut` sets are
/// rejected.
///
/// `mut` has no counterpart in a WASM signature, so both declarations lower to
/// the identical signature and the check above passes over them. The
/// disagreement is about what a reader of either file is entitled to believe the
/// imported function may write through, which is a real difference with no
/// reconciliation available: a union licenses the non-`mut` file's calls, an
/// intersection refuses the `mut` file's, and taking the first is a coin toss.
#[test]
fn two_files_disagreeing_on_a_host_imports_write_set_are_rejected() {
    let typed = typed_of_multi(&[
        (
            Vec::new(),
            "use sib;\n\
             external fn sort_pair(mut p: [i32; 2]);\n\
             use { sort_pair } from host::env;\n\
             pub fn direct() -> i32 {\n\
                 let mut arr: [i32; 2] = [5, 2];\n\
                 sort_pair(arr);\n\
                 return arr[0];\n\
             }\n\
             pub fn through_sibling() -> i32 { return sib::via(); }\n",
        ),
        (
            vec!["sib"],
            "external fn sort_pair(p: [i32; 2]);\n\
             use { sort_pair } from host::env;\n\
             pub fn via() -> i32 {\n\
                 let arr: [i32; 2] = [5, 2];\n\
                 sort_pair(arr);\n\
                 return arr[0];\n\
             }\n",
        ),
    ]);

    let err = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect_err("two write sets for one host import must be rejected");
    assert!(
        matches!(err, ExternalResolutionError::ConflictingHostWriteSet { .. }),
        "got: {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("conflicting write sets for host import `sort_pair`")
            && rendered.contains("module `env`"),
        "the rejection names the import and the module it is bound to: {rendered}"
    );
    assert!(
        rendered.contains("the entry file") && rendered.contains("`sib`"),
        "both declaring files are named: {rendered}"
    );
    assert!(
        rendered.contains("no merged body to check a write set against"),
        "the host form says why this is not the linked form's rejection: {rendered}"
    );
}

/// A `()` parameter is refused on a host extern exactly as on a linked one.
///
/// The host path skips resolution, validation and the write-set fold, but it
/// keeps signature lowering, and this is the reason. Lowering is the only
/// refusal of a `()` parameter on the extern path: code generation's import
/// registration drops a unit argument silently, so without this the artifact
/// would declare an import with fewer parameters than the declaration a reader
/// sees, and every call site would push an argument the import does not take.
#[test]
fn a_unit_parameter_is_refused_on_a_host_extern() {
    let typed = typed_of(
        "external fn telemetry(a: ());\n\
         use { telemetry } from host::fprime_core;\n\
         pub fn use_it(x: i32) -> i32 { return x; }",
    );

    let err = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect_err("a `()` parameter has no WASM representation, on either kind of extern");
    assert!(
        matches!(
            &err,
            ExternalResolutionError::Signature { export_field, .. }
                if export_field == "telemetry"
        ),
        "got: {err:?}"
    );
    assert!(
        err.to_string()
            .contains("the unit type `()` has no value representation"),
        "got: {err}"
    );
}

/// A host extern returning `()` builds end to end.
///
/// The declaration is lowered twice — by the driver into the declared host
/// import, and by code generation into the import the artifact carries — and
/// the artifact check compares the two. Both must give the empty result list: a
/// driver answering `() -> (i32)` would have the check refuse a program with
/// nothing wrong in it, telling its author their declaration disagrees with
/// itself.
#[test]
fn a_host_extern_returning_unit_builds_end_to_end() {
    let typed = typed_of(
        "external fn tick() -> ();\n\
         use { tick } from host::env;\n\
         pub fn run(x: i32) -> i32 { tick(); return x; }",
    );

    let externals = resolve_external_modules(&typed, &SearchPath::new(), None)
        .unwrap_or_else(|e| panic!("`-> ()` is a supported return type: {e}"));
    assert_eq!(
        externals.host_imports,
        vec![HostImport {
            module: "env".into(),
            field: "tick".into(),
            signature: DeclaredSignature {
                params: Vec::new(),
                results: Vec::new(),
            },
        }],
        "a `()` return declares no result value"
    );

    let artifact = codegen(&typed, "tick").expect("codegen succeeds");
    let linked = link_resolved(artifact.wasm(), &externals, &LinkOptions::default())
        .unwrap_or_else(|e| panic!("the declaration and the emitted import agree: {e}"));
    assert_eq!(
        linked.wasm,
        artifact.wasm(),
        "a host program's artifact is the codegen output, byte for byte"
    );
}

/// A real two-module host program, resolved and linked end to end, whose
/// declaration order is not sorted order.
///
/// [`inference::link_resolved`] looks each artifact import up with a binary
/// search over the declared set, so sortedness of that set is a precondition.
/// Only the `BTreeMap` the driver collects host imports into supplies it, and
/// nothing else in this file pins it: every other real-bytes test here declares
/// exactly one host import, and every hand-written set is already sorted, so
/// collecting in declaration order instead leaves the whole suite green.
///
/// This shape is what goes red. Code generation emits imports in *declaration*
/// order — `fprime_core.telemetry`, `fprime_core.command`, `env.clock_ms` —
/// which is not `(module, field)` order, so a declaration-order declared set
/// makes the first lookup miss and a correct artifact dies with an undeclared
/// import it does declare. Both halves are asserted because the defect has two
/// ways out: the driver's vector in full, pinning the order where it is decided,
/// and the link against the real artifact, pinning it where it is relied on.
#[test]
fn two_host_modules_resolve_sorted_and_link_against_the_real_artifact() {
    let typed = typed_of(
        "external fn telemetry(channel: i32, value: i32) -> i32;\n\
         external fn command(opcode: i32) -> i32;\n\
         use { telemetry, command } from host::fprime_core;\n\
         external fn clock_ms() -> i64;\n\
         use { clock_ms } from host::env;\n\
         pub fn report(channel: i32) -> i64 {\n\
             let ack: i32 = command(channel);\n\
             let sent: i32 = telemetry(channel, ack);\n\
             if sent == 0 { return 0; }\n\
             return clock_ms();\n\
         }",
    );

    let externals = resolve_external_modules(&typed, &SearchPath::new(), None)
        .expect("host bindings resolve with no search path at all");
    assert_eq!(
        externals.host_imports,
        vec![
            HostImport {
                module: "env".into(),
                field: "clock_ms".into(),
                signature: DeclaredSignature {
                    params: Vec::new(),
                    results: vec![WasmValType::I64],
                },
            },
            HostImport {
                module: "fprime_core".into(),
                field: "command".into(),
                signature: DeclaredSignature {
                    params: vec![WasmValType::I32],
                    results: vec![WasmValType::I32],
                },
            },
            HostImport {
                module: "fprime_core".into(),
                field: "telemetry".into(),
                signature: DeclaredSignature {
                    params: vec![WasmValType::I32, WasmValType::I32],
                    results: vec![WasmValType::I32],
                },
            },
        ],
        "the declared set is sorted by `(module, field)`, which the link step's lookup requires"
    );

    let artifact = codegen(&typed, "report").expect("codegen succeeds");
    assert_eq!(
        function_imports(artifact.wasm()),
        vec![
            ("fprime_core".to_string(), "telemetry".to_string()),
            ("fprime_core".to_string(), "command".to_string()),
            ("env".to_string(), "clock_ms".to_string()),
        ],
        "the artifact lists its imports in declaration order, which is why the declared set \
         cannot also be collected that way"
    );

    let linked = link_resolved(artifact.wasm(), &externals, &LinkOptions::default())
        .expect("the emitted imports are exactly the declared set, in whatever order each lists");
    assert_eq!(
        linked.wasm,
        artifact.wasm(),
        "a host program's artifact is the codegen output, byte for byte"
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

/// The imports a hand-built artifact declares, for the refusals below.
enum Imported {
    /// A function import of the given lowered signature.
    Func(Vec<WasmValType>, Vec<WasmValType>),
    /// A function import at `(f32) -> ()` — a signature built from a value type
    /// no `external fn` declaration can name, so no declaration matches it and
    /// none ever could.
    ///
    /// Spelled as its own row rather than as a `WasmValType` because
    /// `WasmValType` is by construction the set Inference *does* model: there is
    /// no value of it that reaches the reader's "not modelled" answer, and that
    /// answer is the one arm of the signature diagnostic nothing else renders.
    UnmodelledFunc,
    /// A memory import — the shape no `external fn` declaration can describe.
    Memory,
}

/// Builds a module whose import section is exactly `entries`.
///
/// Hand-built rather than compiled, because the point of every test below is a
/// disagreement between the artifact and the declared set, and this compiler
/// cannot be asked to produce one: the two are derived from the same
/// declarations. Writing the artifact by hand is what lets the check be
/// exercised against the states it exists to catch.
fn module_importing(entries: &[(&str, &str, Imported)]) -> Vec<u8> {
    encode_module(0, entries)
}

/// The same module, with `leading_array_types` array types written into the type
/// section ahead of every function type.
///
/// An array type is a composite type that is not a function, so nothing can read
/// a signature out of its slot — and a type index is an index into the whole
/// section, non-function entries included. A reader that skipped those entries
/// rather than holding their places would shift every function type down by one,
/// pairing each import with its neighbour's signature.
fn module_importing_behind_array_types(
    leading_array_types: usize,
    entries: &[(&str, &str, Imported)],
) -> Vec<u8> {
    encode_module(leading_array_types, entries)
}

fn encode_module(leading_array_types: usize, entries: &[(&str, &str, Imported)]) -> Vec<u8> {
    use wasm_encoder::{
        EntityType, ImportSection, MemoryType, Module, StorageType, TypeSection, ValType,
    };

    fn encode(val: WasmValType) -> ValType {
        match val {
            WasmValType::I32 => ValType::I32,
            WasmValType::I64 => ValType::I64,
        }
    }

    let mut types = TypeSection::new();
    let mut imports = ImportSection::new();
    let mut next_type = 0u32;
    for _ in 0..leading_array_types {
        types.ty().array(&StorageType::Val(ValType::I32), false);
        next_type += 1;
    }
    for (module, field, shape) in entries {
        match shape {
            Imported::Func(params, results) => {
                types.ty().function(
                    params.iter().copied().map(encode),
                    results.iter().copied().map(encode),
                );
                imports.import(module, field, EntityType::Function(next_type));
                next_type += 1;
            }
            Imported::UnmodelledFunc => {
                types.ty().function([ValType::F32], []);
                imports.import(module, field, EntityType::Function(next_type));
                next_type += 1;
            }
            Imported::Memory => {
                imports.import(
                    module,
                    field,
                    EntityType::Memory(MemoryType {
                        minimum: 1,
                        maximum: None,
                        memory64: false,
                        shared: false,
                        page_size_log2: None,
                    }),
                );
            }
        }
    }

    let mut module = Module::new();
    module.section(&types);
    module.section(&imports);
    module.finish()
}

/// The declared set every row below is measured against: three host imports of
/// one module — `env.clock_ms` at `() -> (i64)`, `env.log` at `(i32) -> ()` and
/// `env.random` at `() -> (i32)` — sorted by `(module, field)` as resolution
/// returns them.
///
/// More than one entry, and at different signatures, because a set of one cannot
/// tell apart the lookup this check performs from one that does not look at all.
/// Against a single declaration, an implementation matching the i-th import to
/// the i-th declaration agrees with the real one on every case there is —
/// accept, undeclared, missing, wrong signature — so the entire matching rule
/// would be unpinned. The differing signatures are what make the reversed-order
/// row below fail for a positional matcher instead of passing by coincidence.
///
/// The set is shaped so that each half of [`DeclaredSignature`] can be moved on
/// its own. `clock_ms` takes no parameters and returns one value, so an artifact
/// can disagree with it in the *results* alone; `log` takes one parameter and
/// returns none, so an artifact can disagree with it at the same arity, in the
/// *parameter type* alone. Without both, an implementation comparing only
/// parameter counts — or only parameters, dropping results entirely — agrees
/// with the real one on every row there is, while an artifact importing
/// `clock_ms` as `() -> (i32)` against call sites compiled for `() -> (i64)`
/// ships green and traps at the first call.
fn declared_host_imports() -> ResolvedExternals {
    ResolvedExternals {
        host_imports: vec![
            HostImport {
                module: "env".into(),
                field: "clock_ms".into(),
                signature: DeclaredSignature {
                    params: Vec::new(),
                    results: vec![WasmValType::I64],
                },
            },
            HostImport {
                module: "env".into(),
                field: "log".into(),
                signature: DeclaredSignature {
                    params: vec![WasmValType::I32],
                    results: Vec::new(),
                },
            },
            HostImport {
                module: "env".into(),
                field: "random".into(),
                signature: DeclaredSignature {
                    params: Vec::new(),
                    results: vec![WasmValType::I32],
                },
            },
        ],
        ..ResolvedExternals::default()
    }
}

/// The three declared imports as artifact entries, in declared order.
fn declared_entries() -> Vec<(&'static str, &'static str, Imported)> {
    vec![
        (
            "env",
            "clock_ms",
            Imported::Func(Vec::new(), vec![WasmValType::I64]),
        ),
        (
            "env",
            "log",
            Imported::Func(vec![WasmValType::I32], Vec::new()),
        ),
        (
            "env",
            "random",
            Imported::Func(Vec::new(), vec![WasmValType::I32]),
        ),
    ]
}

/// [`declared_entries`] with the entry for `field` replaced by `shape`, so a row
/// below states only the one thing it disagrees with the declaration about.
fn entries_with(field: &str, shape: Imported) -> Vec<(&'static str, &'static str, Imported)> {
    let mut entries = declared_entries();
    let slot = entries
        .iter_mut()
        .find(|(_, name, _)| *name == field)
        .expect("the field is one of the declared set");
    slot.2 = shape;
    entries
}

/// Runs `link_resolved` over [`declared_host_imports`] and returns the
/// refusal it produced, failing the test if the bytes were accepted.
fn refusal(artifact: &[u8]) -> HostImportError {
    let error = link_resolved(artifact, &declared_host_imports(), &LinkOptions::default())
        .expect_err("the artifact must not be shipped");
    error
        .downcast::<HostImportError>()
        .expect("a host-import disagreement surfaces as a `HostImportError`")
}

/// The control the refusals below are read against: an artifact whose imports
/// are exactly the declared set is accepted and shipped unchanged.
///
/// Without it, a check that refused everything would pass every refusal test.
#[test]
fn link_resolved_accepts_an_artifact_whose_imports_are_the_declared_set() {
    let artifact = module_importing(&declared_entries());
    let linked = link_resolved(&artifact, &declared_host_imports(), &LinkOptions::default())
        .expect("the declared set and the import section agree");
    assert_eq!(linked.wasm, artifact);
}

/// An artifact carrying the declared set in the *opposite* order is accepted
/// just the same.
///
/// Import section order is codegen's business, not the declaration's: the
/// declared set is sorted by `(module, field)` and the artifact's entries are
/// emitted in the order the walk reached them, so the two orders agree only by
/// accident. Pairing them by name is what makes the check a statement about
/// which functions ship; pairing them by position would be a statement about
/// nothing, and — since the two declarations here differ in their results — a
/// positional implementation reports a signature mismatch on bytes that are
/// correct.
#[test]
fn link_resolved_pairs_imports_with_declarations_by_name_and_not_by_position() {
    let mut reversed = declared_entries();
    reversed.reverse();
    let artifact = module_importing(&reversed);
    let linked = link_resolved(&artifact, &declared_host_imports(), &LinkOptions::default())
        .expect("the set is the same set whichever order the section lists it in");
    assert_eq!(linked.wasm, artifact);
}

#[test]
fn link_resolved_refuses_an_import_no_declaration_covers() {
    let mut entries = declared_entries();
    entries.push((
        "env",
        "entropy",
        Imported::Func(Vec::new(), vec![WasmValType::I64]),
    ));
    let artifact = module_importing(&entries);
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`entropy`") && rendered.contains("declares no host binding"),
        "an artifact that will not instantiate must say which import nothing asked for: \
         {rendered}"
    );
}

/// One two-level name imported twice is refused, even when the second entry
/// carries the declared signature and the rest of the set is complete.
///
/// Nothing else in the pipeline would catch it. The declared set holds each
/// `(module, field)` once, so a check that only recorded "this pair was seen"
/// would find every declaration matched and ship a module asking an embedder for
/// one name twice — which no embedder can satisfy, and which the source, holding
/// one declaration, does not explain.
#[test]
fn link_resolved_refuses_one_name_imported_twice() {
    let mut entries = declared_entries();
    entries.push((
        "env",
        "clock_ms",
        Imported::Func(Vec::new(), vec![WasmValType::I64]),
    ));
    let artifact = module_importing(&entries);
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`clock_ms`") && rendered.contains("twice"),
        "the repeated name is the whole diagnosis, and a reader has to be told it is a \
         repetition rather than a mismatch: {rendered}"
    );
}

#[test]
fn link_resolved_refuses_a_declaration_the_artifact_does_not_carry() {
    let artifact = module_importing(&[]);
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`clock_ms`") && rendered.contains("does not carry"),
        "a declaration the artifact dropped must be named, or the embedder registers a \
         function nothing calls: {rendered}"
    );
}

#[test]
fn link_resolved_refuses_a_declared_import_emitted_at_another_arity() {
    let artifact = module_importing(&[(
        "env",
        "clock_ms",
        Imported::Func(vec![WasmValType::I32], vec![WasmValType::I64]),
    )]);
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`clock_ms`")
            && rendered.contains("declared `() -> (i64)`")
            && rendered.contains("`(i32) -> (i64)`"),
        "the two signatures are what the reader has to compare, so both are shown: {rendered}"
    );
}

/// A declared import emitted at the declared *arity* and a different result type
/// is refused.
///
/// The half of the comparison with the worst failure mode, and the one an arity
/// check cannot see. An artifact importing `env.clock_ms` as `() -> (i32)` while
/// the program compiled its call sites against `() -> (i64)` instantiates
/// cleanly — an embedder registers whatever the artifact asks for — and then
/// traps at the first call, with nothing in the source to read the trap against.
/// The whole artifact is emitted with its declared parameter list intact, so
/// every other row here leaves an implementation that dropped results entirely
/// looking correct.
#[test]
fn link_resolved_refuses_a_declared_import_emitted_at_another_result_type() {
    let artifact = module_importing(&entries_with(
        "clock_ms",
        Imported::Func(Vec::new(), vec![WasmValType::I32]),
    ));
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`clock_ms`")
            && rendered.contains("declared `() -> (i64)`")
            && rendered.contains("emitted with `() -> (i32)`"),
        "a result-type difference is a signature mismatch and must be reported as one, with \
         both signatures shown: {rendered}"
    );
}

/// A declared import emitted with the declared number of parameters, of another
/// type, is refused.
///
/// `env.log` is declared `(i32) -> ()` and emitted `(i64) -> ()`: same arity,
/// same empty results, one parameter type apart. Nothing else in this file can
/// separate the real comparison from one written on parameter *counts* — the
/// arity row above differs in count, and every accepting row agrees on both.
#[test]
fn link_resolved_refuses_a_declared_import_emitted_at_another_parameter_type() {
    let artifact = module_importing(&entries_with(
        "log",
        Imported::Func(vec![WasmValType::I64], Vec::new()),
    ));
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`log`")
            && rendered.contains("declared `(i32) -> ()`")
            && rendered.contains("emitted with `(i64) -> ()`"),
        "two signatures of one arity are still two signatures, and the embedder registers the \
         emitted one: {rendered}"
    );
}

/// A declared import emitted at a signature built from a value type Inference
/// does not model is refused, and says so rather than printing a signature.
///
/// The one arm of the mismatch diagnostic that has no second signature to show:
/// an `f32` parameter cannot be rendered as a [`DeclaredSignature`], because no
/// declaration can name one. Reporting it as a mismatch against some
/// approximation would send the author comparing two spellings that differ
/// nowhere they can see.
#[test]
fn link_resolved_refuses_a_declared_import_emitted_at_a_signature_that_cannot_be_declared() {
    let artifact = module_importing(&entries_with("clock_ms", Imported::UnmodelledFunc));
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("`env`.`clock_ms`")
            && rendered.contains("a signature using a value type Inference does not model"),
        "with no declarable signature to print, the refusal has to say that is why: {rendered}"
    );
}

/// An artifact whose function types sit behind a composite type that is not a
/// function is accepted, its imports paired with the signatures their type
/// indices actually name.
///
/// A type index indexes the whole type section, non-function entries included,
/// so a reader that collected only the function types would shift every one of
/// them down by a slot — and here that shift is silent in the worst way: each
/// import would be checked against its neighbour's signature, so an artifact
/// that agrees with every declaration is refused for a mismatch it does not
/// have, and an artifact that disagrees can be accepted for one it does.
#[test]
fn link_resolved_reads_a_type_index_past_a_type_that_is_not_a_function() {
    let artifact = module_importing_behind_array_types(1, &declared_entries());
    let linked = link_resolved(&artifact, &declared_host_imports(), &LinkOptions::default())
        .expect("a type index counts every composite type, not only the function ones");
    assert_eq!(linked.wasm, artifact);
}

#[test]
fn link_resolved_refuses_a_non_function_import() {
    let mut entries = declared_entries();
    entries.push(("env", "memory", Imported::Memory));
    let artifact = module_importing(&entries);
    let rendered = refusal(&artifact).to_string();
    assert!(
        rendered.contains("imports a memory") && rendered.contains("`env`.`memory`"),
        "no `external fn` can describe a memory import, so it can never be accounted for: \
         {rendered}"
    );
}

/// Bytes that do not decode are refused, and never read as a module that imports
/// nothing.
///
/// This is the one failure the check could have silently inverted. An empty
/// import list compares equal to an empty declared set, so a reader that
/// answered "no imports" on a decode failure would ship unparseable bytes for
/// any program that declared none — and here it would still have to explain the
/// missing declaration, which is the wrong diagnosis for the wrong file.
#[test]
fn link_resolved_refuses_bytes_that_do_not_decode() {
    let rendered = refusal(b"\x00asm\x01\x00\x00\x00\xff\xff\xff\xff").to_string();
    assert!(
        rendered.contains("does not decode"),
        "a decode failure must be reported as one: {rendered}"
    );
}

/// Resolved modules and host imports arriving together is an internal
/// inconsistency, not a user error: resolution refuses that program before it
/// resolves anything, so a value carrying both was not produced by one
/// resolution.
#[test]
fn link_resolved_reports_a_mixed_resolution_as_an_internal_inconsistency() {
    use inference::wasm_link::ResolvedExternalModule;

    let mut externals = declared_host_imports();
    externals.modules.push(ResolvedExternalModule {
        logical_module: "arith".into(),
        path: PathBuf::from("arith.wasm"),
        bytes: Vec::new(),
    });

    let artifact = module_importing(&[(
        "env",
        "clock_ms",
        Imported::Func(Vec::new(), vec![WasmValType::I64]),
    )]);
    let error = link_resolved(&artifact, &externals, &LinkOptions::default())
        .expect_err("the two halves cannot both be present");
    assert!(
        error.to_string().contains("internal error"),
        "the message must not read as something the author wrote wrong: {error}"
    );
}

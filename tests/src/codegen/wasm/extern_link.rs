//! End-to-end test for external `.wasm` linking (issue #9, Phase 4).
//!
//! Drives the full pipeline an `infc` invocation with `-L` runs: a program that
//! `use`s a function from an external module is compiled, the external module is
//! resolved off a search path and statically merged in, and the result is
//! translated to Rocq. The two end-to-end guarantees the merge makes are
//! asserted directly:
//!
//! - the unified `.wasm` has **no cross-module imports** — the external bytes
//!   are folded in, not referenced; and
//! - the unified `.v` carries the merged function as an **ordinary named
//!   definition** with **no orphan `Mi` import record** for the merged module.
//!
//! The external fixture is itself produced by the Inference compiler: a tiny
//! library exporting `pub fn sum` lowers to a `.wasm` whose `sum` export backs
//! the main program's `external fn sum`.

#[cfg(test)]
mod extern_link_tests {
    use std::path::{Path, PathBuf};

    use inference::wasm_link::{resolve_external_modules, SearchPath};
    use inference::{codegen, link, parse, type_check, wasm_to_v, FxHashMap};
    use inf_wasmparser::{Parser, Payload, TypeRef};

    /// Compiles `source` to a `.wasm` with the default settings, skipping the
    /// analysis phase — this codegen path does not need it. (The library sources
    /// passed here define no externs; in the main programs that do, A024 accepts
    /// the bound externs and rejects only unbound ones.)
    fn compile_wasm(source: &str, module_name: &str) -> Vec<u8> {
        let arena = parse(source).expect("library source parses");
        let typed = type_check(arena).expect("library source type-checks");
        let output = codegen(&typed, module_name).expect("library codegen succeeds");
        output.wasm().to_vec()
    }

    /// A throwaway directory under the system temp dir, unique to this test run,
    /// removed on drop.
    struct TempLibDir {
        path: PathBuf,
    }

    impl TempLibDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "inference-extern-link-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp lib dir");
            TempLibDir { path }
        }

        /// Writes `bytes` to `<dir>/<relative>`, creating parent directories.
        fn write_module(&self, relative: &Path, bytes: &[u8]) {
            let dest = self.path.join(relative);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).expect("create module parent dir");
            }
            std::fs::write(dest, bytes).expect("write external module");
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempLibDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// The `(module, field)` of every function import in `wasm`.
    fn function_imports(wasm: &[u8]) -> Vec<(String, String)> {
        let mut imports = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ImportSection(reader) = payload.expect("valid payload") {
                for import in reader {
                    let import = import.expect("valid import");
                    if matches!(import.ty, TypeRef::Func(_)) {
                        imports.push((import.module.to_string(), import.name.to_string()));
                    }
                }
            }
        }
        imports
    }

    /// Runs the pipeline an `infc -L <lib_dir> main.inf -v` invocation runs and
    /// returns the unified `.wasm` together with its Rocq translation.
    fn compile_and_link(main_source: &str, lib_dir: &Path, module_name: &str) -> (Vec<u8>, String) {
        let arena = parse(main_source).expect("main source parses");
        let typed = type_check(arena).expect("main source type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("external modules resolve and validate");
        let external_bytes: Vec<(&str, &[u8])> = externals
            .iter()
            .map(|m| (m.logical_module.as_str(), m.bytes.as_slice()))
            .collect();

        let codegen_output = codegen(&typed, module_name).expect("main codegen succeeds");

        // Sanity guard: the *unlinked* codegen output carries the import, so its
        // translation contains an `Mi` record. This makes the post-link absence
        // of `Mi` a real difference rather than a vacuous pass.
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let pre_link_rocq = wasm_to_v(
            module_name,
            codegen_output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("unlinked wasm-to-v succeeds");
        assert!(
            pre_link_rocq.contains("Mi "),
            "the unlinked module must still carry an import record; .v was:\n{pre_link_rocq}"
        );

        let unified = link(codegen_output.wasm(), &external_bytes).expect("link succeeds");
        let rocq = wasm_to_v(
            module_name,
            &unified,
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("wasm-to-v succeeds");
        (unified, rocq)
    }

    #[test]
    fn single_extern_links_to_self_contained_wasm_and_v() {
        // The external library exports `sum`; the main program binds it via
        // `use { sum } from arith;` and calls it.
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );

        let lib_dir = TempLibDir::new("single");
        // Logical module `arith` resolves to `<dir>/arith.wasm`.
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }";

        let (unified, rocq) = compile_and_link(main_source, lib_dir.path(), "extern_link");

        // The unified module is valid and self-contained: no import references
        // the external module any more.
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");
        assert!(
            function_imports(&unified).is_empty(),
            "unified module must have no cross-module imports, found {:?}",
            function_imports(&unified)
        );

        // The merged `sum` reads as an ordinary named Rocq definition, prefixed
        // with its logical module so two libraries exporting the same field can
        // never collide in the name section. The linker emits `arith.sum`, which
        // wasm-to-v sanitizes (every non-alphanumeric to `_`) to `arith_sum`.
        assert!(
            rocq.contains("Definition arith_sum"),
            "merged function must be a module-prefixed named Rocq definition; .v was:\n{rocq}"
        );

        // No orphan import record survives for the merged module: the linker
        // removed the import section, so wasm-to-v emits no `Mi` for it.
        assert!(
            !rocq.contains("Mi \"arith\""),
            "merged module must leave no orphan `Mi` import record; .v was:\n{rocq}"
        );
        assert!(
            !rocq.contains("MID_func"),
            "a self-contained module imports nothing, so no `MID_func` should appear; .v was:\n{rocq}"
        );
    }

    #[test]
    fn nested_module_path_resolves_and_links() {
        // A `::`-separated logical module must resolve to a nested file
        // (`crypto::adder` -> `<dir>/crypto/adder.wasm`) and link identically.
        let lib_wasm = compile_wasm(
            "pub fn combine(a: i32, b: i32) -> i32 { return a + b; }",
            "adder",
        );

        let lib_dir = TempLibDir::new("nested");
        lib_dir.write_module(Path::new("crypto").join("adder.wasm").as_path(), &lib_wasm);

        let main_source = "external fn combine(a: i32, b: i32) -> i32;\n\
             use { combine } from crypto::adder;\n\
             pub fn run(x: i32) -> i32 { return combine(x, x); }";

        let (unified, rocq) = compile_and_link(main_source, lib_dir.path(), "nested_link");

        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");
        assert!(
            function_imports(&unified).is_empty(),
            "no cross-module imports may remain, found {:?}",
            function_imports(&unified)
        );
        // The merged `combine` is prefixed with its `::`-separated logical
        // module: the linker emits `crypto::adder.combine`, which wasm-to-v
        // sanitizes (every non-alphanumeric to `_`, then `__` runs collapsed) to
        // `crypto_adder_combine`.
        assert!(
            rocq.contains("Definition crypto_adder_combine"),
            "merged function must be a module-prefixed named Rocq definition; .v was:\n{rocq}"
        );
        assert!(
            !rocq.contains("Mi \"crypto::adder\""),
            "no orphan `Mi` import record for the merged module; .v was:\n{rocq}"
        );
    }

    #[test]
    fn program_without_externs_is_unchanged_by_the_link_step() {
        // A program that binds no externs resolves to an empty external set and
        // the link step is a byte-identical pass-through of codegen output.
        let main_source = "pub fn double(x: i32) -> i32 { return x + x; }";
        let arena = parse(main_source).expect("parses");
        let typed = type_check(arena).expect("type-checks");

        let externals =
            resolve_external_modules(&typed, &SearchPath::new(), None).expect("no externs");
        assert!(externals.is_empty(), "program binds no external modules");

        let codegen_output = codegen(&typed, "plain").expect("codegen succeeds");
        let unified = link(codegen_output.wasm(), &[]).expect("link is a no-op");
        assert_eq!(
            unified,
            codegen_output.wasm(),
            "the link step must not alter an extern-free module"
        );
    }

    #[test]
    fn link_with_no_externals_does_not_silently_pass_through_dangling_imports() {
        // Fail-closed: the empty-externals fast path is keyed on the module being
        // import-free, not merely on the externals slice being empty. A module
        // that still carries an import but is given no externals to satisfy it
        // must error (unsatisfied import), never pass through with the import
        // intact. (In the CLI flow externals are always resolved first; this
        // guards the public `inference::link` contract against misuse.)
        let import_bearing = wat::parse_str(
            r#"(module
                 (import "arith" "sum" (func (param i32 i32) (result i32)))
                 (func (export "run") (result i32)
                   i32.const 1 i32.const 2 call 0))"#,
        )
        .expect("fixture assembles");
        assert!(
            link(&import_bearing, &[]).is_err(),
            "a module with an unsatisfied import must not pass through as Ok"
        );

        // And malformed bytes must surface a parse error, not Ok(garbage).
        assert!(
            link(&[0x00, 0x61, 0x73, 0x6d, 0xff], &[]).is_err(),
            "malformed main bytes must be a link error, not a silent pass-through"
        );
    }

    #[test]
    fn proof_mode_spec_omission_renumbers_the_call_to_the_merged_extern() {
        // C1: a proof-mode program that binds an extern AND declares a spec.
        // Post-link function order is add_three=0, spec `check`=1, merged `sum`=2
        // (the import is removed and the external appended). The spec function is
        // OMITTED from the emitted `.v` module record, so `sum` (index 2) shifts
        // down to instantiated index 1 — and `add_three`'s executable call to
        // `sum` must be renumbered to `BI_call 1%N`. This pins two things at once:
        // that the embedded `inference.spec_funcs` section names `check` (so the
        // *right* function is omitted — omitting `sum` instead would fail-close on
        // `add_three`'s now-dangling call), and that the omission-driven call
        // renumbering is correct. Translating with empty explicit maps adopts the
        // post-link embedded sections as the source of truth.
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );
        let lib_dir = TempLibDir::new("c1_spec");
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        // `check` must be translatable to an obligation, so it does not call the
        // extern (an external call has no verified body — `P005`). `add_three`
        // (executable) carries the extern call whose operand the omission
        // renumbers.
        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }\n\
             spec MySpec {\n\
                 fn check(x: i32) -> i32 { return x; }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("external modules resolve");
        let external_bytes: Vec<(&str, &[u8])> = externals
            .iter()
            .map(|m| (m.logical_module.as_str(), m.bytes.as_slice()))
            .collect();

        let target = inference_wasm_codegen::Target::default();
        let mode = inference_wasm_codegen::CompilationMode::Proof;
        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            target,
            mode,
            target.default_opt_level(),
            "c1prog",
            inference_wasm_codegen::EmitFeatures::default(),
        )
        .expect("proof-mode codegen succeeds");

        let unified = link(codegen_output.wasm(), &external_bytes).expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        // Empty explicit maps: the post-link embedded sections are the source of
        // truth (the pre-link codegen indices would be stale here).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("c1prog", &unified, &empty, &inference::HSpecMap::default())
            .expect("wasm-to-v succeeds");

        // MySpec surfaces with the new obligation-list shape.
        assert!(
            rocq.contains("Definition c1prog__MySpec_specs : list hassert :="),
            "MySpec must surface a per-spec obligation list; .v was:\n{rocq}"
        );
        // The omission of the spec function at index 1 renumbers `sum` from
        // instantiated index 2 to 1, so `add_three`'s call reads `BI_call 1%N`.
        assert!(
            rocq.contains("BI_call 1%N"),
            "add_three's call to the merged `sum` must be renumbered past the \
             omitted spec function (to instantiated index 1); .v was:\n{rocq}"
        );
        // The merged `sum` executable body (its `a + b`) survives in the record.
        assert!(
            rocq.contains("BI_binop T_i32 (Binop_i BOI_add)"),
            "the merged extern `sum` body must survive into the module record; .v was:\n{rocq}"
        );
    }

    #[test]
    fn proof_mode_hspec_t_app_resolves_across_the_link() {
        // The companion to `proof_mode_spec_omission_renumbers_the_call_to_the_merged_extern`:
        // that test pins the *executable* `BI_call` renumbering but its spec is a
        // plain helper (a trivially-true obligation with no cross-call). This one
        // adds the load-bearing half — an obligation whose `T_app` must resolve
        // through the same post-link remap.
        //
        // Post-link function order is add_three=0, is_prime=1, spec `prop`=2,
        // merged `sum`=3. The spec function is OMITTED from the `.v` module record,
        // so both index streams shift: `add_three`'s executable call to `sum`
        // renumbers to `BI_call 2%N`, and the obligation's call to the surviving
        // `is_prime` resolves by its verbatim name-section symbol to defined-fn
        // index 1 (`T_app 1`) — proving the obligation and the executable body
        // agree on the post-link numbering. Translating with empty explicit maps
        // adopts the post-link embedded `inference.spec_funcs` / `inference.hspecs`
        // sections as the source of truth (the CLI-equivalent, defer-to-embedded
        // flow), which is what makes the `T_app` resolution genuinely post-link.
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );
        let lib_dir = TempLibDir::new("hspec_tapp");
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        // `add_three` (executable) carries the extern call the omission renumbers;
        // `is_prime` (executable, defined) is the obligation's `T_app` target — it
        // does not call the extern, since a spec obligation cannot reference an
        // unverified external body (`P005`).
        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }\n\
             fn is_prime(n: i32) -> bool { return n > 1; }\n\
             spec MySpec {\n\
                 fn prop() forall {\n\
                     let n: i32 = @;\n\
                     assume { assert(n > 1); }\n\
                     assert(is_prime(n));\n\
                 }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("external modules resolve");
        let external_bytes: Vec<(&str, &[u8])> = externals
            .iter()
            .map(|m| (m.logical_module.as_str(), m.bytes.as_slice()))
            .collect();

        let target = inference_wasm_codegen::Target::default();
        let mode = inference_wasm_codegen::CompilationMode::Proof;
        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            target,
            mode,
            target.default_opt_level(),
            "hprog",
            inference_wasm_codegen::EmitFeatures::default(),
        )
        .expect("proof-mode codegen succeeds");

        let unified = link(codegen_output.wasm(), &external_bytes).expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        // Empty explicit maps: the post-link embedded sections are the source of
        // truth (the pre-link codegen indices are stale after the merge).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("hprog", &unified, &empty, &inference::HSpecMap::default())
            .expect("wasm-to-v succeeds");

        // The obligation is emitted as a first-class `hassert` for `MySpec`.
        assert!(
            rocq.contains("Definition hprog__MySpec_hspec1 : hassert :="),
            "MySpec must surface a first-class hassert obligation; .v was:\n{rocq}"
        );
        // Executable stream: `add_three`'s call to the merged `sum` renumbers past
        // the omitted spec function to instantiated index 2.
        assert!(
            rocq.contains("BI_call 2%N"),
            "add_three's call to the merged `sum` must renumber to index 2; .v was:\n{rocq}"
        );
        // Obligation stream: the `is_prime` cross-call resolves by its post-link
        // name to defined-fn index 1 — the same numbering the executable bodies use.
        assert!(
            rocq.contains("T_app 1 ((T_local 0%N) :: nil)"),
            "the obligation's `is_prime` call must resolve to defined-fn index 1; .v was:\n{rocq}"
        );
        // The `T_app` target's own body survives in the record (a `> 1` compare).
        assert!(
            rocq.contains("BI_relop T_i32 (Relop_i (ROI_gt SX_S))"),
            "the `is_prime` body must survive into the module record; .v was:\n{rocq}"
        );
    }
}

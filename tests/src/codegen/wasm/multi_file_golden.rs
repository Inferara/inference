//! Four-tier golden tests for multi-file codegen flattening.
//!
//! These complement the execution-only smoke tests in `multi_file.rs` with
//! reproducible golden-file coverage. Each fixture is a real *file tree* under
//! `tests/test_data/codegen/wasm/multi_file_golden/<test>/src/`, compiled through
//! the production project front end ([`inference::parse_project`]) — the same
//! closure walk and canonical file ordering the compiler uses at runtime — so the
//! merged-module bytes are deterministic and committable as goldens.
//!
//! The four verification tiers (per CONTRIBUTING.md) are:
//! 1. **byte compare** against the committed `<test>.wasm`;
//! 2. **WAT compare** against the committed `<test>.wat` (printable modules only);
//! 3. **structural validation** via `inf_wasmparser::validate`;
//! 4. **execution** under Wasmtime of a cross-file call.
//!
//! ## What file-qualification looks like in the WAT
//!
//! Multi-file codegen file-qualifies the function key that keeps two same-named
//! functions in different files apart, and the WASM name section carries that
//! qualification: a non-entry file's `add` is `$lib.arith.add` and its method
//! `Point.dist` is `$lib.geo.Point.dist`. An entry-file item keeps its bare
//! name, so a program that happens to have one file is byte-identical to what
//! single-file codegen produces. Spec-inner functions are the one deliberate
//! exception and stay bare wherever they are defined: spec membership travels
//! as function indices in `inference.spec_funcs`, and the proof translation
//! resolves a reachability obligation by stripping the folded spec prefix and
//! looking the remaining bare name up in the section.
//!
//! The qualification is not cosmetic. The name section is a namespace the proof
//! translation reads back: an obligation names its function by `FnKey::Display`
//! and is resolved against the section by string equality. Qualifying makes the
//! two agree for every file rather than only for the entry, which is what lets
//! a specification call a function in another file at all.
//!
//! Cross-file distinctness therefore surfaces as distinct `(func $…)` names,
//! and — for two files defining a same-named struct — as distinct field offsets
//! baked into each function's loads and stores.

#[cfg(test)]
mod multi_file_golden_codegen_tests {
    use crate::utils::{
        assert_project_wat_equivalence, assert_wasms_modules_equivalence, get_project_test_dir,
        proof_wasm_codegen_project, read_project_golden_wasm, wasm_codegen_project,
    };

    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// Instantiates a module after validating it, returning the store + instance.
    fn instantiate(wasm_bytes: &[u8]) -> (Store<()>, Instance) {
        inf_wasmparser::validate(wasm_bytes)
            .unwrap_or_else(|e| panic!("generated multi-file Wasm module is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes)
            .unwrap_or_else(|e| panic!("failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("failed to instantiate Wasm module: {e}"));
        (store, instance)
    }

    /// Calls a zero-argument exported function returning `i32`.
    fn call_i32(store: &mut Store<()>, instance: &Instance, name: &str) -> i32 {
        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut *store, name)
            .unwrap_or_else(|e| panic!("failed to get '{name}': {e}"));
        f.call(&mut *store, ())
            .unwrap_or_else(|e| panic!("call to '{name}' failed: {e}"))
    }

    /// Renders the module to WAT, panicking with context on failure.
    fn wat_of(wasm_bytes: &[u8], test_name: &str) -> String {
        wasmprinter::print_bytes(wasm_bytes)
            .unwrap_or_else(|e| panic!("failed to print WAT for {test_name}: {e}"))
    }

    /// Tiers 1-3 shared by every printable, deterministic fixture: byte compare,
    /// WAT compare, and structural validation. Returns the freshly generated WASM
    /// so the caller can add tier-4 execution and targeted structural assertions.
    fn golden_bytes_wat_validate(module_path: &str, test_name: &str) -> Vec<u8> {
        let actual = wasm_codegen_project(module_path, test_name);
        let expected = read_project_golden_wasm(module_path, test_name);
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_project_wat_equivalence(&actual, module_path, test_name);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("validate failed for {test_name}: {e}"));
        actual
    }

    /// Two-file program: the entry binds namespace `util` with `use util;` and
    /// reaches `util.inf`'s `pub fn helper` via the 2-segment `util::helper()`
    /// call — the shortest cross-file shape. All four tiers.
    #[test]
    fn two_file_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "two_file");
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 7);
    }

    /// The corrected three-file re-export chain (the normative issue example):
    /// `main` `use math;` → `math` `pub use lib::arith;` → `lib/arith` exposes
    /// `pub fn add`. `main` reaches `math::arith::add` only through the re-export.
    /// All four tiers, plus structural checks that each non-entry function
    /// carries its own defining file in its debug name and that only the entry
    /// `run` is exported.
    #[test]
    fn re_export_chain_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "re_export_chain");
        let wat = wat_of(&wasm, "re_export_chain");
        // Every non-entry function carries its own defining file, not the file
        // that imported it: `add` is qualified by `lib.arith` even though the
        // entry reaches it through `math`'s re-export, and `math`'s own `foo` is
        // qualified by `math`. A bare name here would be resolvable by an
        // obligation only for the entry file, and collidable with every other
        // file's same-named item.
        assert!(
            wat.contains("(func $lib.arith.add "),
            "`add` must be qualified by its defining file, not the re-exporting \
             one; WAT:\n{wat}"
        );
        assert!(
            wat.contains("(func $math.foo "),
            "`math.inf`'s own `foo` must be qualified by `math`; WAT:\n{wat}"
        );
        assert!(
            !wat.contains("(func $add ") && !wat.contains("(func $foo "),
            "no non-entry function may keep an unqualified debug name; WAT:\n{wat}"
        );
        // The entry function is exported by its bare name; the re-exported chain
        // members are not WASM exports.
        assert!(
            wat.contains("(export \"run\" "),
            "entry `run` must be exported; WAT:\n{wat}"
        );
        assert!(
            !wat.contains("(export \"add\" ") && !wat.contains("(export \"foo\" "),
            "re-exported / non-entry pub fns must not be WASM exports; WAT:\n{wat}"
        );
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 3);
    }

    /// Two files each define `struct Pair` with *different* layouts: the entry's
    /// `{ a: i32, b: i32 }` puts `b` at offset 4; the imported `{ a: i64, b: i32 }`
    /// puts `b` at offset 8. The file-qualified type keys keep the layouts
    /// distinct — a single shared layout would mis-read one file. The two distinct
    /// offsets are baked into the WAT, and the two `there_b` functions carry
    /// distinct debug names. All four tiers + structural.
    #[test]
    fn dup_struct_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "dup_struct");
        let wat = wat_of(&wasm, "dup_struct");
        // Entry `Pair.b` at offset 4; imported `Pair.b` at offset 8 — both present
        // proves the two layouts resolved to distinct keys.
        assert!(
            wat.contains("i32.const 4\n    i32.add"),
            "entry Pair must read .b at offset 4; WAT:\n{wat}"
        );
        assert!(
            wat.contains("i32.const 8\n    i32.add"),
            "imported Pair must read .b at offset 8; WAT:\n{wat}"
        );
        // The two `there_b` functions are not same-named in the section: the
        // imported one carries its defining file, so the section holds two
        // distinct strings and wasmprinter has no duplicate to disambiguate.
        assert!(
            wat.contains("(func $there_b ") && wat.contains("(func $lib.shapes.there_b "),
            "the entry and imported `there_b` must be distinctly named; WAT:\n{wat}"
        );
        assert!(
            !wat.contains("(@name "),
            "distinct names leave wasmprinter nothing to disambiguate with \
             `@name`; WAT:\n{wat}"
        );
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "here_b"), 20);
        assert_eq!(call_i32(&mut store, &instance, "there_b"), 200);
    }

    /// Braced item import (`use lib::arith::{add};`) binds `add` directly; the
    /// bare call must resolve to the foreign file's function index. All four tiers.
    #[test]
    fn item_import_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "item_import");
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 42);
    }

    /// A `pub struct` imported by item is used as a function *parameter and return
    /// type*, constructed in its own file, and field-accessed across files. The
    /// struct-return ABI (sret pointer) and cross-file field read execute. Four
    /// tiers.
    #[test]
    fn cross_file_struct_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "cross_file_struct");
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 3);
    }

    /// A cross-file *instance method* (`self` receiver) on an imported struct,
    /// reached via `p.dist()`, plus a cross-file associated constructor
    /// (`Point::origin()`). Both execute. Four tiers.
    #[test]
    fn cross_file_method_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "cross_file_method");
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 5);
    }

    /// Root-only export policy: `lib::arith::add` is `pub` but lives in an imported
    /// file, so it is intra-project visible, not a WASM export. The export section
    /// must contain only the entry `run` (plus the runtime `memory`/stack-pointer
    /// exports), never `add`. Asserted at the export-section level via WAT and at
    /// the instance level via Wasmtime. Four tiers.
    #[test]
    fn root_only_export_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "root_only_export");
        let wat = wat_of(&wasm, "root_only_export");
        assert!(
            wat.contains("(export \"run\" "),
            "entry `run` must be exported; WAT:\n{wat}"
        );
        assert!(
            !wat.contains("(export \"add\" "),
            "non-entry pub fn `add` must NOT be exported (intra-project only); WAT:\n{wat}"
        );
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 5);
        assert!(
            instance.get_func(&mut store, "add").is_none(),
            "imported `pub fn add` must not be a WASM export"
        );
    }

    /// Method name-mangling for an imported struct's method: the WAT debug name
    /// is the defining file joined to the `Struct.method` form
    /// (`lib.geo.Point.dist` / `lib.geo.Point.at`). The qualifier is the
    /// **struct's** file, not the call site's — both calls are made from
    /// `main.inf`. This test pins the observable form so a change to the
    /// mangling that dropped the file prefix, and with it every non-entry
    /// obligation's ability to name the function it means, would be caught.
    /// Four tiers.
    #[test]
    fn method_mangling_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "method_mangling");
        let wat = wat_of(&wasm, "method_mangling");
        assert!(
            wat.contains("(func $lib.geo.Point.dist ") && wat.contains("(func $lib.geo.Point.at "),
            "a method on an imported struct uses the `file.Struct.method` debug \
             name; WAT:\n{wat}"
        );
        assert!(
            !wat.contains("(func $Point."),
            "no method may keep the unqualified `Struct.method` form; WAT:\n{wat}"
        );
        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "run"), 8);
    }

    /// Regression guard: a representative *single-file* program compiled through
    /// the multi-file project path produces byte-identical output to single-file
    /// codegen. This proves the entry-unqualified invariant at the golden level —
    /// the multi-file path must never perturb a program that happens to have one
    /// file. The committed golden here is generated through the project path; the
    /// test additionally asserts equality with the direct single-file pipeline.
    #[test]
    fn single_via_project_test() {
        let wasm = golden_bytes_wat_validate(module_path!(), "single_via_project");

        // The same source through the direct single-file pipeline must match byte
        // for byte (entry items are unqualified, gated on the entry marker).
        let dir = get_project_test_dir(module_path!(), "single_via_project");
        let src = std::fs::read_to_string(dir.join("src").join("main.inf"))
            .expect("failed to read single_via_project entry");
        let arena = inference::parse(&src).expect("single-file parse");
        let tc = inference::type_check(arena).expect("single-file type check");
        inference::analyze(&tc).expect("single-file analysis");
        let single = inference::codegen(&tc, "output")
            .expect("single-file codegen")
            .wasm()
            .to_vec();
        assert_eq!(
            wasm, single,
            "single-file program via the multi-file path must be byte-identical to single-file codegen"
        );

        let (mut store, instance) = instantiate(&wasm);
        assert_eq!(call_i32(&mut store, &instance, "hello_world"), 42);
    }

    /// Proof-mode multi-file spec collection. The module collects specs from
    /// every reachable file; the entry spec stays bare while the imported file's
    /// spec is file-qualified in the `inference.spec_funcs` section (the per-spec
    /// keying is pinned by the smoke test in `multi_file.rs`). Each file's spec
    /// claims a property about that file's own executable function, so the two
    /// obligations differ in both the symbol they apply and the constant they
    /// compare against. This golden pins the *byte* output of that proof module
    /// and structurally validates it. Proof modules may carry opcodes
    /// `wasmprinter`/Wasmtime reject, so this fixture **intentionally skips the WAT
    /// (tier 2) and Wasmtime (tier 4) tiers** — byte compare + validate only.
    ///
    /// The obligation payload is additionally decoded and asserted on, outside
    /// the byte compare: a golden is only ever as good as what it was
    /// regenerated from, and an obligation that collapsed back to the vacuous
    /// `HA_true` would pass a byte compare against a golden regenerated from the
    /// same collapse. Reading the two obligations back proves the section is
    /// carrying claims rather than merely being present.
    #[test]
    fn proof_specs_test() {
        let actual = proof_wasm_codegen_project(module_path!(), "proof_specs");
        let expected = read_project_golden_wasm(module_path!(), "proof_specs");
        assert_wasms_modules_equivalence(&expected, &actual);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("validate failed for proof_specs: {e}"));

        let payload = custom_section(&actual, inference_hassert::HSPECS_SECTION_NAME)
            .expect("a proof module whose specs carry obligations must embed inference.hspecs");
        let hspecs =
            inference_hassert::decode(&payload).expect("the embedded hspecs section must decode");
        for spec in ["EntrySpec", "lib_checks_LibSpec"] {
            let entries = hspecs.get(spec).unwrap_or_else(|| {
                panic!(
                    "no obligations for `{spec}`; have {:?}",
                    hspecs.keys().collect::<Vec<_>>()
                )
            });
            assert_eq!(entries.len(), 1, "`{spec}` declares one spec function");
            assert_ne!(
                entries[0].hassert,
                inference_hassert::HAssert::True,
                "`{spec}`'s obligation is vacuous: any proof discharges it without \
                 reading the program"
            );
        }
    }

    /// A specification may call a function defined in another file: the
    /// obligation's `T_app` symbol resolves against the module the compiler
    /// emits.
    ///
    /// The obligation names its function by `FnKey::Display`
    /// (`lib.checks.lib_value`) and the name section now records the same
    /// file-qualified symbol, so `resolve_app_symbols`' verbatim lookup finds
    /// exactly the one function that carries it. While the section held the
    /// bare `lib_value` the two producers agreed only for the entry file, whose
    /// `module_path` is empty, and every cross-file obligation failed closed.
    ///
    /// The assertion is on the resolved **index**, not merely on translation
    /// succeeding: the entry file's obligation resolved before this change too,
    /// so a test that only checked for an `Ok` would also pass with the
    /// imported file's claim silently pointing at the entry's function.
    #[test]
    fn cross_file_obligation_symbols_resolve() {
        let wasm = proof_wasm_codegen_project(module_path!(), "proof_specs");
        let rocq = inference::wasm_to_v(
            "proof_specs",
            &wasm,
            &inference::FxHashMap::default(),
            &inference::HSpecMap::default(),
        )
        .expect("a cross-file obligation symbol resolves against the name section");

        // `T_app` indexes `mod_funcs`, so pin the order the indices are read
        // against before reading them.
        assert!(
            rocq.contains(
                "  mod_funcs :=\n    entry_value ::\n    main ::\n    \
                 lib_checks_lib_value ::\n    nil;"
            ),
            "the module must list entry_value=0, main=1, lib_checks_lib_value=2; \
             .v was:\n{rocq}"
        );

        // Each spec claims a property of *its own* file's function: the entry's
        // `entry_value` returns 1 and the imported `lib_value` returns 2, so a
        // symbol resolved to the wrong file would state the other file's claim.
        assert!(
            obligation_block(&rocq, "output__EntrySpec_hspec1").contains("T_app 0 nil"),
            "the entry spec must apply `entry_value` (index 0); .v was:\n{rocq}"
        );
        assert!(
            obligation_block(&rocq, "output__lib_checks_LibSpec_hspec1").contains("T_app 2 nil"),
            "the imported file's spec must apply that file's own `lib_value` \
             (index 2), not the entry's function; .v was:\n{rocq}"
        );
    }

    /// Proof-mode byte golden for the `exists`-kind reachability lowering.
    /// The spec function's WASM type carries its hidden trailing choice
    /// parameters (i32, i64, i32 after the declared i32), which changes the
    /// module's TYPE section — the one artifact surface nothing else
    /// byte-pins — so this golden is the byte-level regression pin for the
    /// choice suffix. Like `proof_specs`, the WAT (tier 2) and Wasmtime
    /// (tier 4) tiers are skipped: the module still carries 0xfc proof
    /// scaffolding in general, and the choice parameters have no caller.
    ///
    /// The embedded `inference.hspecs` entry is decoded and asserted on
    /// beyond the byte compare, for the same reason `proof_specs_test`
    /// decodes its payload: a golden regenerated from a kind regression
    /// (say, the entry silently reverting to `Forall`) would pass its own
    /// byte compare.
    #[test]
    fn proof_exists_test() {
        let actual = proof_wasm_codegen_project(module_path!(), "proof_exists");
        let expected = read_project_golden_wasm(module_path!(), "proof_exists");
        assert_wasms_modules_equivalence(&expected, &actual);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("validate failed for proof_exists: {e}"));

        let payload = custom_section(&actual, inference_hassert::HSPECS_SECTION_NAME)
            .expect("the exists golden must embed inference.hspecs");
        let hspecs =
            inference_hassert::decode(&payload).expect("the embedded hspecs section must decode");
        let entries = hspecs
            .get("ReachExists")
            .expect("the spec must carry an obligation entry");
        assert_eq!(entries.len(), 1, "one obligation for the one spec function");
        match &entries[0].kind {
            inference_hassert::SpecKind::Exists(meta) => {
                assert_eq!(meta.entry_arity, 1, "one declared parameter");
                assert_eq!(
                    meta.visible_locs,
                    vec![0, 1, 2],
                    "entry parameter + the two named choices; the anonymous \
                     call-argument choice at slot 3 is excluded"
                );
            }
            other => panic!("the entry must be exists-kind, got {other:?}"),
        }
    }

    /// Proof-mode byte golden for the `unique`-kind reachability lowering —
    /// the unique sibling of [`proof_exists_test`], byte-pinning the same
    /// choice-suffix lowering under the other kind tag. The decoded entry is
    /// where the two goldens genuinely differ: a unique-kind entry whose
    /// visible locs dropped the named choice would weaken the judgment to
    /// agreement-on-parameters, and nothing but this metadata pins that.
    #[test]
    fn proof_unique_test() {
        let actual = proof_wasm_codegen_project(module_path!(), "proof_unique");
        let expected = read_project_golden_wasm(module_path!(), "proof_unique");
        assert_wasms_modules_equivalence(&expected, &actual);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("validate failed for proof_unique: {e}"));

        let payload = custom_section(&actual, inference_hassert::HSPECS_SECTION_NAME)
            .expect("the unique golden must embed inference.hspecs");
        let hspecs =
            inference_hassert::decode(&payload).expect("the embedded hspecs section must decode");
        let entries = hspecs
            .get("ReachUnique")
            .expect("the spec must carry an obligation entry");
        assert_eq!(entries.len(), 1, "one obligation for the one spec function");
        match &entries[0].kind {
            inference_hassert::SpecKind::Unique(meta) => {
                assert_eq!(meta.entry_arity, 1, "one declared parameter");
                assert_eq!(
                    meta.visible_locs,
                    vec![0, 1],
                    "entry parameter + the named choice — the projection \
                     `unique` compares exit states through"
                );
            }
            other => panic!("the entry must be unique-kind, got {other:?}"),
        }
    }

    /// The blank-line-delimited block of emitted Rocq that defines `name`, so a
    /// test can read one obligation's body without matching against the rest of
    /// the file.
    fn obligation_block<'a>(rocq: &'a str, name: &str) -> &'a str {
        rocq.split("\n\n")
            .find(|block| block.contains(&format!("Definition {name} : hassert :=")))
            .unwrap_or_else(|| panic!("no `{name}` obligation in the .v:\n{rocq}"))
    }

    /// The payload of the named custom section, or `None` when the module
    /// carries no such section.
    fn custom_section(wasm: &[u8], name: &str) -> Option<Vec<u8>> {
        inf_wasmparser::Parser::new(0)
            .parse_all(wasm)
            .filter_map(Result::ok)
            .find_map(|payload| match payload {
                inf_wasmparser::Payload::CustomSection(reader) if reader.name() == name => {
                    Some(reader.data().to_vec())
                }
                _ => None,
            })
    }

    /// Regeneration helpers for the multi-file golden `.wasm`/`.wat` files.
    ///
    /// These are `#[ignore]`d by design (per CONTRIBUTING.md): they are not
    /// behavioral tests, they rewrite the committed goldens from current compiler
    /// output. Run explicitly with `--ignored` after an intentional codegen change:
    /// `cargo test -p inference-tests multi_file_golden -- --ignored`.
    #[cfg(test)]
    mod regenerate {
        use crate::utils::{
            proof_wasm_codegen_project, regenerate_project_golden, wasm_codegen_project,
        };

        /// Module path of the parent test module, so `regenerate_project_golden`
        /// resolves the same `test_data/codegen/wasm/multi_file_golden/<test>/`
        /// directories the behavioral tests read.
        fn parent_module_path() -> &'static str {
            // `module_path!()` here ends in `::regenerate`; strip it so the test
            // data resolver sees the same path the behavioral tests use.
            "inference_tests::codegen::wasm::multi_file_golden::multi_file_golden_codegen_tests"
        }

        fn regen(test_name: &str) {
            let wasm = wasm_codegen_project(parent_module_path(), test_name);
            regenerate_project_golden(&wasm, parent_module_path(), test_name);
        }

        #[test]
        #[ignore]
        fn regenerate_two_file() {
            regen("two_file");
        }

        #[test]
        #[ignore]
        fn regenerate_re_export_chain() {
            regen("re_export_chain");
        }

        #[test]
        #[ignore]
        fn regenerate_dup_struct() {
            regen("dup_struct");
        }

        #[test]
        #[ignore]
        fn regenerate_item_import() {
            regen("item_import");
        }

        #[test]
        #[ignore]
        fn regenerate_cross_file_struct() {
            regen("cross_file_struct");
        }

        #[test]
        #[ignore]
        fn regenerate_cross_file_method() {
            regen("cross_file_method");
        }

        #[test]
        #[ignore]
        fn regenerate_root_only_export() {
            regen("root_only_export");
        }

        #[test]
        #[ignore]
        fn regenerate_method_mangling() {
            regen("method_mangling");
        }

        #[test]
        #[ignore]
        fn regenerate_single_via_project() {
            regen("single_via_project");
        }

        /// Proof mode: byte golden only, no WAT (non-det/spec opcodes).
        fn regen_proof(test_name: &str) {
            let wasm = proof_wasm_codegen_project(parent_module_path(), test_name);
            let dir = crate::utils::get_project_test_dir(parent_module_path(), test_name);
            let wasm_path = dir.join(format!("{test_name}.wasm"));
            std::fs::write(&wasm_path, &wasm)
                .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
            println!(
                "Regenerated: {} ({} bytes)",
                wasm_path.display(),
                wasm.len()
            );
        }

        #[test]
        #[ignore]
        fn regenerate_proof_specs() {
            regen_proof("proof_specs");
        }

        #[test]
        #[ignore]
        fn regenerate_proof_exists() {
            regen_proof("proof_exists");
        }

        #[test]
        #[ignore]
        fn regenerate_proof_unique() {
            regen_proof("proof_unique");
        }
    }
}

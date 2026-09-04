//! The WASM `name` section as one namespace, end to end.
//!
//! Two producers write function names into that section and a third reads them
//! back. Code generation writes a compiled function's symbol; the static-merge
//! linker writes a name for every external body it splices in; and the proof
//! translation resolves an obligation's applied function symbol against the
//! merged section by string equality, so *which* function an obligation is
//! about is decided there and nowhere else.
//!
//! The two halves are held apart by a character the Inference identifier
//! grammar cannot produce: a compiled function's symbol joins identifiers with
//! `.` only, and every merged body's name carries `::`. Within the merged half a
//! second mark separates a root — named after the export field an `external fn`
//! declaration binds — from a private callee, whose debug name comes from a
//! foreign module and is unconstrained.
//!
//! Every test here asserts on **which function** an obligation applies, not on
//! the translation succeeding. A symbol that resolves to the wrong body still
//! produces a well-formed `.v` at exit 0, and the claim it carries is then true
//! of a function nobody wrote it about — the failure mode these tests exist for
//! is a passing build, not a red one.

#[cfg(test)]
mod name_section_identity_tests {
    use std::collections::BTreeSet;

    use inf_wasmparser::{Parser, Payload};
    use inference::wasm_link::{SearchPath, resolve_external_modules};
    use inference::{FxHashMap, link, type_check, wasm_to_v};
    use inference_hassert::{HAssert, HSpecMap, HTerm};
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// A merged program plus its proof translation — the two artifacts every
    /// test here reads.
    struct Linked {
        wasm: Vec<u8>,
        rocq: String,
    }

    /// Assembles a `.wasm` from WAT, panicking with the WAT on error.
    ///
    /// The externals below are hand-written rather than compiled from Inference
    /// because the collisions under test are ones Inference source cannot
    /// express: one body exported under two fields, and a module whose private
    /// callee is named exactly like its export. A foreign toolchain produces
    /// both routinely.
    fn wat_module(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).unwrap_or_else(|e| panic!("invalid WAT fixture: {e}\n{wat}"))
    }

    /// Compiles an Inference library to the `.wasm` an `external fn` binds
    /// against.
    fn compile_library(source: &str, module_name: &str) -> Vec<u8> {
        let arena = inference::parse(source).expect("library source parses");
        let typed = type_check(arena).expect("library source type-checks");
        inference::codegen(&typed, module_name)
            .expect("library codegen succeeds")
            .wasm()
            .to_vec()
    }

    /// Runs the pipeline an `infc -L <dir> --proof` invocation runs over a
    /// multi-file program: fold the sources into one arena, type-check, analyze,
    /// resolve each bound `external fn` against a library written to a temporary
    /// search path, generate proof-mode WASM, merge, and translate.
    ///
    /// `files` is `(module_path, source)` with the entry file first, exactly as
    /// the project front end folds a source tree. `libraries` is
    /// `(logical_module, bytes)`, each written to `<dir>/<logical_module>.wasm`
    /// so the real resolver finds it.
    fn build_link_translate(files: &[(Vec<&str>, &str)], libraries: &[(&str, &[u8])]) -> Linked {
        let mut arena = inference_ast::arena::AstArena::default();
        for (module_path, source) in files {
            let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
            let parsed = inference_parser::parse_into(arena, source, module_path);
            assert!(
                parsed.errors.is_empty(),
                "test source has syntax errors: {:?}",
                parsed.errors
            );
            arena = parsed.arena;
        }
        let typed = type_check(arena).expect("test source type-checks");
        inference_analysis::analyze(&typed).expect("test source passes analysis");

        let lib_dir = tempfile::tempdir().expect("create temporary library directory");
        for (logical_module, bytes) in libraries {
            std::fs::write(lib_dir.path().join(format!("{logical_module}.wasm")), bytes)
                .expect("write external module");
        }
        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("external modules resolve and validate");

        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            "output",
            inference_wasm_codegen::CodegenOptions {
                mode: inference_wasm_codegen::CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("proof-mode codegen succeeds");

        let merged = link(
            codegen_output.wasm(),
            &externals.module_bytes(),
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        inf_wasmparser::validate(&merged).expect("merged module is valid wasm");

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("output", &merged, &empty, &HSpecMap::default())
            .expect("the merged module's obligations resolve and translate");
        Linked { wasm: merged, rocq }
    }

    /// Every `(index, name)` the module's `name` section records for a function.
    fn function_names(wasm: &[u8]) -> Vec<(u32, String)> {
        let mut names = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::CustomSection(custom) = payload.expect("valid payload")
                && let inf_wasmparser::KnownCustom::Name(reader) = custom.as_known()
            {
                for sub in reader {
                    if let inf_wasmparser::Name::Function(map) = sub.expect("valid name subsection")
                    {
                        for naming in map {
                            let naming = naming.expect("valid function naming");
                            names.push((naming.index, naming.name.to_string()));
                        }
                    }
                }
            }
        }
        names
    }

    /// The function-name strings the module records, as a set.
    fn name_set(wasm: &[u8]) -> BTreeSet<String> {
        function_names(wasm).into_iter().map(|(_, n)| n).collect()
    }

    /// How many functions the `name` section records under `symbol`.
    fn carriers_of(wasm: &[u8], symbol: &str) -> usize {
        function_names(wasm)
            .iter()
            .filter(|(_, name)| name == symbol)
            .count()
    }

    /// The distinct function symbols the module's obligations *apply* — every
    /// `T_app` head and every `HA_app_ok` head.
    ///
    /// This is the set the proof translation must resolve against the `name`
    /// section, read from the artifact rather than reconstructed, so a test can
    /// compare the two producers' strings directly.
    fn applied_symbols(wasm: &[u8]) -> BTreeSet<String> {
        let data = custom_section(wasm, inference_hassert::HSPECS_SECTION_NAME)
            .expect("the module must carry an inference.hspecs section");
        let map = inference_hassert::decode(&data).expect("the hspecs payload decodes");
        let mut symbols = BTreeSet::new();
        for entries in map.values() {
            for entry in entries {
                collect_applied_in_assert(&entry.hassert, &mut symbols);
            }
        }
        symbols
    }

    /// The assertion half of [`applied_symbols`]. Matched exhaustively so a new
    /// obligation form cannot silently stop being collected.
    fn collect_applied_in_assert(assert: &HAssert, out: &mut BTreeSet<String>) {
        match assert {
            HAssert::True | HAssert::False => {}
            HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
                collect_applied_in_assert(inner, out);
            }
            HAssert::And(left, right) | HAssert::Imp(left, right) | HAssert::Or(left, right) => {
                collect_applied_in_assert(left, out);
                collect_applied_in_assert(right, out);
            }
            HAssert::TermEq(left, right) => {
                collect_applied_in_term(left, out);
                collect_applied_in_term(right, out);
            }
            HAssert::HasType(term, _) | HAssert::Defined(term) => {
                collect_applied_in_term(term, out);
            }
            HAssert::AppOk(symbol, args) => {
                out.insert(symbol.0.clone());
                for arg in args {
                    collect_applied_in_term(arg, out);
                }
            }
        }
    }

    /// The term half of [`applied_symbols`].
    fn collect_applied_in_term(term: &HTerm, out: &mut BTreeSet<String>) {
        match term {
            HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
            HTerm::App(symbol, args) => {
                out.insert(symbol.0.clone());
                for arg in args {
                    collect_applied_in_term(arg, out);
                }
            }
            HTerm::Binop(_, _, left, right) | HTerm::Relop(_, _, left, right) => {
                collect_applied_in_term(left, out);
                collect_applied_in_term(right, out);
            }
        }
    }

    /// The payload of the named custom section, or `None`.
    fn custom_section(wasm: &[u8], name: &str) -> Option<Vec<u8>> {
        Parser::new(0)
            .parse_all(wasm)
            .filter_map(Result::ok)
            .find_map(|payload| match payload {
                Payload::CustomSection(reader) if reader.name() == name => {
                    Some(reader.data().to_vec())
                }
                _ => None,
            })
    }

    /// The emitted module's `mod_funcs` list, in order — the list a `T_app`
    /// index indexes.
    fn mod_funcs(rocq: &str) -> Vec<String> {
        let tail = rocq
            .split_once("  mod_funcs :=\n")
            .unwrap_or_else(|| panic!("the .v must carry a `mod_funcs` list; .v was:\n{rocq}"))
            .1;
        let mut names = Vec::new();
        for line in tail.lines() {
            let line = line.trim();
            if line == "nil;" {
                return names;
            }
            let name = line
                .strip_suffix("::")
                .unwrap_or_else(|| panic!("unexpected `mod_funcs` entry `{line}`"))
                .trim();
            names.push(name.to_string());
        }
        panic!("the `mod_funcs` list is unterminated; .v was:\n{rocq}")
    }

    /// Every `T_app` index the emitted obligations carry, in source order.
    fn applied_indices(rocq: &str) -> Vec<usize> {
        let mut indices = Vec::new();
        let mut rest = rocq;
        while let Some((_, tail)) = rest.split_once("T_app ") {
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            assert!(
                !digits.is_empty(),
                "a `T_app` must be followed by its index; .v was:\n{rocq}"
            );
            indices.push(digits.parse().expect("the index is a number"));
            rest = tail;
        }
        indices
    }

    /// The sole `T_app` index in the emitted obligations.
    fn sole_applied_index(rocq: &str) -> usize {
        match applied_indices(rocq)[..] {
            [one] => one,
            ref many => panic!("expected exactly one application, got {many:?}; .v was:\n{rocq}"),
        }
    }

    /// The `module_func` record `name` defines, so a test can read the body of
    /// the function an obligation resolved to rather than trusting its name.
    fn definition_body(rocq: &str, name: &str) -> String {
        let header = format!("Definition {name} : module_func := {{|");
        let tail = rocq
            .split_once(header.as_str())
            .unwrap_or_else(|| panic!("no `{name}` definition in the .v:\n{rocq}"))
            .1;
        let end = tail
            .find("|}.")
            .unwrap_or_else(|| panic!("`{name}`'s definition is unterminated; .v was:\n{rocq}"));
        tail[..end].to_string()
    }

    /// The body of the function an obligation's sole application resolves to.
    fn applied_body(linked: &Linked) -> String {
        let index = sole_applied_index(&linked.rocq);
        let funcs = mod_funcs(&linked.rocq);
        let name = funcs.get(index).unwrap_or_else(|| {
            panic!(
                "the application names `mod_funcs` index {index}, which the module's {} \
                 functions do not reach; .v was:\n{}",
                funcs.len(),
                linked.rocq
            )
        });
        definition_body(&linked.rocq, name)
    }

    /// Instantiates a merged module and calls `name` with one `i32`.
    fn call_i32(wasm: &[u8], name: &str, argument: i32) -> i32 {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));
        let func: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, name)
            .unwrap_or_else(|e| panic!("merged module must export `{name}`: {e}"));
        func.call(&mut store, argument)
            .unwrap_or_else(|e| panic!("`{name}({argument})` failed: {e}"))
    }

    // -- A linked module named after one of the program's source files -------

    /// A project whose source file is named after a bound logical module, where
    /// the library carries a private function of the same bare name, states its
    /// obligation about **its own** function.
    ///
    /// This is the miscompile the two name spaces exist to prevent, and it is
    /// silent: the program's `mathlib::helper` returns 999 and the library's
    /// private `helper` returns 7, so `assert(mathlib::helper(x) == 7)` is false
    /// of the program. With one separator joining both schemes the obligation's
    /// symbol — the program-side `mathlib.helper` — matched the *library's*
    /// merged callee, and a claim nobody wrote about that body came out true and
    /// dischargeable at exit 0.
    ///
    /// Nothing outside the resolved index distinguishes the two outcomes: both
    /// emit a well-formed `.v` that `coqc` accepts, both name a `Definition`
    /// that sanitizes to `mathlib_helper`, and both exit 0. So the assertion
    /// reads the **body** of the function the application resolved to, which is
    /// the only place 999 and 7 are told apart.
    #[test]
    fn a_linked_module_named_after_a_source_file_cannot_answer_for_its_functions() {
        let library = compile_library(
            "fn helper(x: i32) -> i32 { return 7; }\n\
             pub fn double(x: i32) -> i32 { return helper(x) + x; }",
            "mathlib",
        );
        let linked = build_link_translate(
            &[
                (
                    vec![],
                    "use mathlib;\n\
                     external fn double(x: i32) -> i32;\n\
                     use { double } from mathlib;\n\
                     pub fn twice(x: i32) -> i32 { return double(x); }\n\
                     spec HelperSpec {\n\
                         fn helper_is_seven() forall {\n\
                             let x: i32 = @;\n\
                             assert(mathlib::helper(x) == 7);\n\
                         }\n\
                     }",
                ),
                (
                    vec!["mathlib"],
                    "pub fn helper(x: i32) -> i32 { return 999; }",
                ),
            ],
            &[("mathlib", &library)],
        );

        // The obligation is minted from a source-level call, so it names the
        // program's own function in the program half of the section.
        assert_eq!(
            applied_symbols(&linked.wasm),
            BTreeSet::from(["mathlib.helper".to_string()]),
            "the obligation must apply the program's own file-qualified symbol"
        );

        // The claim is about the body returning 999 — the one the source names —
        // and is therefore false, which is what the author has to be shown. A
        // resolution to the library's `helper` would make the same text true.
        let body = applied_body(&linked);
        assert!(
            body.contains("Vi32 999"),
            "the obligation must apply the program's own `helper`, whose body \
             returns 999; it applied:\n{body}\n.v was:\n{}",
            linked.rocq
        );
        assert!(
            !body.contains("Vi32 7"),
            "the obligation must not apply the library's private `helper`, whose \
             body returns 7; it applied:\n{body}\n.v was:\n{}",
            linked.rocq
        );

        // And the library's body really is in the module, so the assertion above
        // is a choice between two present candidates rather than a merge that
        // quietly dropped one.
        assert!(
            linked
                .rocq
                .split("Definition ")
                .any(|block| block.contains("module_func") && block.contains("Vi32 7")),
            "the library's private `helper` must be merged in, so the resolution \
             is a real choice; .v was:\n{}",
            linked.rocq
        );

        // Three names, all distinct: the program's function, the library's
        // private callee, and the library's root. Under one separator the first
        // two were both `mathlib.helper`.
        let names = name_set(&linked.wasm);
        for symbol in ["mathlib.helper", "mathlib::#helper", "mathlib::double"] {
            assert!(
                names.contains(symbol),
                "the merged section must carry `{symbol}`; it carries {names:?}"
            );
        }

        assert_eq!(
            call_i32(&linked.wasm, "twice", 5),
            12,
            "the executable half is unaffected: `double(5)` runs the library's \
             own `helper(5) + 5`, which is 7 + 5"
        );
    }

    // -- One foreign body satisfying two bound imports -----------------------

    /// Two `external fn` declarations bound to one library that exports a single
    /// body under both fields: both obligations resolve, and to the same
    /// function.
    ///
    /// A WASM name map holds one name per function index, so only one of the two
    /// root names can be recorded. The merge records the least of them and points
    /// any obligation over the other at it; recording whichever import came last
    /// instead would leave the earlier alias naming nothing and fail the
    /// translation of a program that links perfectly well.
    #[test]
    fn one_foreign_body_bound_under_two_names_answers_for_both() {
        let library = wat_module(
            r#"
            (module
              (type (;0;) (func (param i32) (result i32)))
              (func (;0;) (type 0) (param i32) (result i32)
                local.get 0
                local.get 0
                i32.add)
              (export "double" (func 0))
              (export "twice" (func 0)))
            "#,
        );
        let linked = build_link_translate(
            &[(
                vec![],
                "external fn double(x: i32) -> i32;\n\
                 external fn twice(x: i32) -> i32;\n\
                 use { double, twice } from mathlib;\n\
                 pub fn run(x: i32) -> i32 { return double(x) + twice(x); }\n\
                 spec AliasSpec {\n\
                     fn both_are_one_body() forall {\n\
                         let x: i32 = @;\n\
                         assert(double(x) == twice(x));\n\
                     }\n\
                 }",
            )],
            &[("mathlib", &library)],
        );

        // Both aliases were pointed at the one name the section records, so the
        // payload applies a single symbol.
        assert_eq!(
            applied_symbols(&linked.wasm),
            BTreeSet::from(["mathlib::double".to_string()]),
            "an obligation over the unrecorded alias must be pointed at the \
             recorded name"
        );
        assert_eq!(
            carriers_of(&linked.wasm, "mathlib::double"),
            1,
            "exactly one merged body answers for both bindings; the section \
             carries {:?}",
            function_names(&linked.wasm)
        );

        // Both sides of the claim resolve, and to one index — the two `external
        // fn` declarations really are two names for one body.
        let indices = applied_indices(&linked.rocq);
        assert_eq!(
            indices.len(),
            2,
            "both sides of the claim must be applications; .v was:\n{}",
            linked.rocq
        );
        assert_eq!(
            indices[0], indices[1],
            "both aliases must resolve to the same merged body; .v was:\n{}",
            linked.rocq
        );

        assert_eq!(
            call_i32(&linked.wasm, "run", 5),
            20,
            "both calls reach the one merged body: (5+5) + (5+5)"
        );
    }

    // -- A struct named after a bound logical module -------------------------

    /// A program may name a struct after a logical module it links against, and
    /// keep a method of the same name as one of that module's exports.
    ///
    /// The method's symbol is `mathlib.double` and the merged root's is
    /// `mathlib::double`. While both schemes joined over `.` these were one
    /// string, two functions carried it, and the obligation applying the extern
    /// was rejected as ambiguous — a hard failure whose only repair was for the
    /// author to rename their own struct. There is nothing to rename now.
    #[test]
    fn a_struct_named_after_a_bound_module_does_not_collide_with_its_roots() {
        let library = compile_library("pub fn double(a: i32) -> i32 { return a + a; }", "mathlib");
        let linked = build_link_translate(
            &[(
                vec![],
                "external fn double(x: i32) -> i32;\n\
                 use { double } from mathlib;\n\
                 struct mathlib {\n\
                     v: i32;\n\
                     w: i32;\n\
                     fn double(self) -> i32 { return self.v + self.w; }\n\
                 }\n\
                 pub fn twice(x: i32) -> i32 { return double(x); }\n\
                 pub fn via_method(x: i32) -> i32 {\n\
                     let m: mathlib = mathlib { v: x, w: x };\n\
                     return m.double();\n\
                 }\n\
                 spec DoubleSpec {\n\
                     fn doubles() forall {\n\
                         let x: i32 = @;\n\
                         assert(double(x) == x + x);\n\
                     }\n\
                 }",
            )],
            &[("mathlib", &library)],
        );

        let names = name_set(&linked.wasm);
        assert!(
            names.contains("mathlib.double") && names.contains("mathlib::double"),
            "the struct's method and the merged root must be two names; the \
             section carries {names:?}"
        );
        assert_eq!(
            carriers_of(&linked.wasm, "mathlib.double"),
            1,
            "the method's symbol must have one carrier; the section carries {:?}",
            function_names(&linked.wasm)
        );
        assert_eq!(
            carriers_of(&linked.wasm, "mathlib::double"),
            1,
            "the merged root's symbol must have one carrier; the section carries {:?}",
            function_names(&linked.wasm)
        );

        // The obligation is about the extern, so it must resolve to the merged
        // body — which takes its argument in a local and adds it to itself —
        // rather than to the method, which reads two fields out of memory.
        assert_eq!(
            applied_symbols(&linked.wasm),
            BTreeSet::from(["mathlib::double".to_string()]),
            "the obligation applies the bound external's merged root"
        );
        let body = applied_body(&linked);
        assert!(
            !body.contains("BI_load"),
            "the obligation must apply the merged extern, not the struct method \
             that loads its fields; it applied:\n{body}"
        );
        assert!(
            body.contains("BI_binop T_i32 (Binop_i BOI_add)"),
            "the merged extern's body adds its argument to itself; it applied:\n{body}"
        );

        assert_eq!(call_i32(&linked.wasm, "twice", 5), 10);
        assert_eq!(call_i32(&linked.wasm, "via_method", 6), 12);
    }

    // -- A private callee named after its own module's export field ----------

    /// A library whose private callee carries the same debug name as the field
    /// it exports keeps the two apart, so an obligation about the export applies
    /// the export.
    ///
    /// The root is named after the import field, which is an Inference
    /// identifier; the callee keeps the name its own module gave it, which is
    /// unconstrained and here is exactly that field. Without the internal mark
    /// both render `mathlib.double` and the obligation has two carriers.
    #[test]
    fn a_private_callee_named_after_its_export_does_not_answer_for_it() {
        let library = wat_module(
            r#"
            (module
              (type (;0;) (func (param i32) (result i32)))
              (func $root (;0;) (type 0) (param i32) (result i32)
                local.get 0
                call 1)
              (func $double (;1;) (type 0) (param i32) (result i32)
                local.get 0
                local.get 0
                i32.add)
              (export "double" (func 0)))
            "#,
        );
        let linked = build_link_translate(
            &[(
                vec![],
                "external fn double(x: i32) -> i32;\n\
                 use { double } from mathlib;\n\
                 pub fn twice(x: i32) -> i32 { return double(x); }\n\
                 spec DoubleSpec {\n\
                     fn doubles() forall {\n\
                         let x: i32 = @;\n\
                         assert(double(x) == x + x);\n\
                     }\n\
                 }",
            )],
            &[("mathlib", &library)],
        );

        let names = name_set(&linked.wasm);
        assert!(
            names.contains("mathlib::double") && names.contains("mathlib::#double"),
            "the root and the private callee of one module must be two names; \
             the section carries {names:?}"
        );

        // The root forwards to the callee, so the applied body is the one that
        // calls — the callee's own body only adds.
        let body = applied_body(&linked);
        assert!(
            body.contains("BI_call"),
            "the obligation must apply the exported root, which forwards to the \
             private callee; it applied:\n{body}"
        );

        assert_eq!(call_i32(&linked.wasm, "twice", 5), 10);
    }

    // -- Code generation's own half of the namespace -------------------------

    /// For a function outside the entry file, the symbol an obligation applies
    /// and the symbol the `name` section records are one string — before any
    /// link is involved.
    ///
    /// The obligation writes the function's file-qualified key and the section
    /// records what code generation put there; the proof translation matches
    /// them by equality, so a producer that qualified one and not the other
    /// leaves every cross-file obligation resolving to nothing (or, worse,
    /// to some other file's same-named function). Read from the artifact on both
    /// sides so the two producers are compared rather than one of them restated.
    #[test]
    fn an_obligation_symbol_and_its_name_section_entry_are_one_string() {
        let wasm = crate::utils::proof_wasm_codegen_multi_file(&[
            (vec![], "use lib::checks;\n\npub fn main() {}\n"),
            (
                vec!["lib", "checks"],
                "fn lib_value() -> i32 { return 2; }\n\
                 spec LibSpec {\n\
                     fn obligation() forall { assert(lib_value() == 2); }\n\
                 }\n",
            ),
        ]);

        let applied = applied_symbols(&wasm);
        assert_eq!(
            applied,
            BTreeSet::from(["lib.checks.lib_value".to_string()]),
            "the obligation names the imported file's function by its \
             file-qualified key"
        );
        for symbol in &applied {
            assert_eq!(
                carriers_of(&wasm, symbol),
                1,
                "exactly one function must be recorded under `{symbol}`; the \
                 section carries {:?}",
                function_names(&wasm)
            );
        }

        // The bare name is gone from the section, so no other file's `lib_value`
        // could answer for this one either.
        assert!(
            !name_set(&wasm).contains("lib_value"),
            "a non-entry function must not keep its bare name; the section \
             carries {:?}",
            name_set(&wasm)
        );
    }

    /// An entry-file program's symbols are exactly what they were before the
    /// name section carried a defining file, so no single-file artifact moves.
    ///
    /// The entry file's module path is empty, so its functions' symbols are the
    /// bare name and the `Struct.method` form — the strings every single-file
    /// golden, and the Rocq `Definition` names the cross-repo discharge manifest
    /// pins, were already built from. This is the half of the scheme that had to
    /// stay still for the qualification of the other half to be shippable, and
    /// nothing else states it directly.
    #[test]
    fn an_entry_file_programs_symbols_stay_unqualified() {
        let wasm = crate::utils::proof_wasm_codegen_multi_file(&[(
            vec![],
            "struct Point {\n\
                 x: i32;\n\
                 y: i32;\n\
                 fn dist(self) -> i32 { return self.x; }\n\
             }\n\
             fn add(a: i32, b: i32) -> i32 { return a + b; }\n\
             pub fn run(v: i32) -> i32 {\n\
                 let p: Point = Point { x: v, y: 0 };\n\
                 return add(p.dist(), 1);\n\
             }\n\
             spec EntrySpec {\n\
                 fn claim() forall {\n\
                     let x: i32 = @;\n\
                     assert(add(x, 0) == x);\n\
                 }\n\
             }\n",
        )]);

        let names = name_set(&wasm);
        for symbol in ["add", "run", "Point.dist"] {
            assert!(
                names.contains(symbol),
                "an entry-file function keeps its source-level name; the section \
                 carries {names:?}"
            );
        }
        assert!(
            names.iter().all(|name| !name.contains(':')),
            "no compiled function's symbol may carry `:`; the section carries {names:?}"
        );
        assert_eq!(
            applied_symbols(&wasm),
            BTreeSet::from(["add".to_string()]),
            "an entry-file obligation names its callee bare, as it always did"
        );
    }

    /// A specification function is the one deliberate exception: its symbol
    /// stays unqualified wherever its file sits.
    ///
    /// Spec membership travels as indices in `inference.spec_funcs`, and the
    /// proof translation resolves a reachability obligation by stripping the
    /// folded spec prefix off the obligation symbol and looking the remaining
    /// bare name up in the section — so qualifying these would break the lookup
    /// that finds them. The carve-out is safe on the same argument as the rest:
    /// the symbol is still an identifier join and still cannot meet a merged
    /// name.
    #[test]
    fn a_spec_functions_symbol_stays_bare_outside_the_entry_file() {
        let wasm = crate::utils::proof_wasm_codegen_multi_file(&[
            (vec![], "use lib::checks;\n\npub fn main() {}\n"),
            (
                vec!["lib", "checks"],
                "fn lib_value() -> i32 { return 2; }\n\
                 spec LibSpec {\n\
                     fn obligation() forall { assert(lib_value() == 2); }\n\
                 }\n",
            ),
        ]);

        let names = name_set(&wasm);
        assert!(
            names.contains("obligation"),
            "the spec function keeps its bare name; the section carries {names:?}"
        );
        assert!(
            !names.contains("lib.checks.obligation")
                && !names.contains("lib_checks_LibSpec.obligation"),
            "the spec carve-out must leave the symbol unqualified; the section \
             carries {names:?}"
        );
        assert!(
            names.iter().all(|name| !name.contains(':')),
            "no compiled function's symbol may carry `:`, which is what keeps it \
             out of the linker's half of the section; the section carries {names:?}"
        );
    }
}

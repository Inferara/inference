//! End-to-end tests for carrying a linked library's own proof obligations into
//! the program's proof artifact.
//!
//! A library compiled in proof mode ships two custom sections recording what its
//! author proved about its own code. Only the executable closure of a satisfied
//! export crosses a static merge, so those sections describe a module the output
//! is not, and a link either says so or — when the caller asks for it — carries
//! the library's universal obligations across, renamed onto the merged bodies
//! they are about. Both halves are exercised here through the real pipeline:
//! `.inf` source, codegen, the linker, and `wasm-to-v`.
//!
//! Every claim about which body an adopted obligation is about is made by
//! *ordinal*. The obligation names its function by a `T_app` into the module
//! record, and the record entry at that ordinal is identified by a literal only
//! that body carries. The `Definition` name at the ordinal cannot serve: the
//! translator sanitizes the merged symbol's `::` away, so a name that survived
//! the rewrite and a name that never went through it look alike.

#[cfg(test)]
mod extern_link_adopt_tests {
    use crate::rocq_test_support;
    use crate::utils::{get_test_data_path, try_type_check_multi_file};
    use inf_wasmparser::{Parser, Payload};
    use inference::wasm_link::{resolve_external_modules, SearchPath};
    use inference::{ExternalSpecPolicy, LinkOptions, LinkOutput, LinkWarning};
    use inference_wasm_codegen::CompilationMode;

    /// The library whose single specification is universal, so a link adopts all
    /// of it and reports nothing left behind.
    const LIBRARY: &str = "spec_adopted_extern_mathlib.inf";
    /// The same library interface, shipping one universal and one reachability
    /// obligation.
    const REACH_LIBRARY: &str = "spec_adopted_reach_mathlib.inf";
    /// The module name the library is compiled under: deliberately not the
    /// logical module it is bound as, so a confusion between the two cannot
    /// pass unnoticed.
    const LIBRARY_MODULE: &str = "mathlib_impl";
    /// The program that declares no specification of its own.
    const PROGRAM: &str = "spec_adopted_extern.inf";
    const PROGRAM_MODULE: &str = "spec_adopted_extern";
    /// The program that declares one of its own beside the adopted one.
    const BOTH: &str = "spec_adopted_both.inf";
    const BOTH_MODULE: &str = "spec_adopted_both";
    /// The logical module both programs bind the library as, and the namespace
    /// an adopted specification's key is folded under.
    const LOGICAL_MODULE: &str = "mathlib";

    /// The literal the library's `scale` multiplies by, as it appears in a
    /// module record body. No other body in either merged artifact carries it,
    /// which is what makes it an identification rather than a coincidence.
    const LIBRARY_FINGERPRINT: &str = "BI_const_num (Vi32 10007)";
    /// The literal the program's own `scaled_sum` weights its second argument
    /// by. Its role is to be the *wrong* answer: an ordinal that pointed at the
    /// program's body instead of the merged one would land here.
    const PROGRAM_FINGERPRINT: &str = "BI_const_num (Vi32 31)";

    /// The library's `.wasm`, compiled in proof mode — the only compilation that
    /// carries verification sections at all.
    fn library(fixture: &str) -> Vec<u8> {
        rocq_test_support::compile_fixture(fixture, LIBRARY_MODULE, CompilationMode::Proof)
    }

    /// Compiles `program` in proof mode and merges `lib` into it under `policy`.
    fn merge_under(
        program: &str,
        module_name: &str,
        lib: &[u8],
        policy: ExternalSpecPolicy,
    ) -> LinkOutput {
        let main = rocq_test_support::compile_fixture(program, module_name, CompilationMode::Proof);
        inference::link_with_options(
            &main,
            &[(LOGICAL_MODULE, lib)],
            None,
            &LinkOptions {
                external_specs: policy,
            },
        )
        .unwrap_or_else(|e| panic!("link failed for {program}: {e}"))
    }

    /// The `(index, name)` entries of `wasm`'s `name` section function
    /// subsection, in the order it records them.
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

    /// The names of every custom section of `wasm`.
    fn custom_section_names(wasm: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::CustomSection(reader) = payload.expect("valid payload") {
                names.push(reader.name().to_string());
            }
        }
        names
    }

    /// The `module_func` definitions of an emitted `.v`, in record order: the
    /// ordinal a `T_app` carries indexes this list.
    fn module_func_names(v: &str) -> Vec<&str> {
        v.lines()
            .filter_map(|line| {
                line.strip_prefix("Definition ")?
                    .strip_suffix(" : module_func := {|")
            })
            .collect()
    }

    /// The text of the `module_func` definition named `name`, from its
    /// `Definition` line to the `|}.` closing it.
    ///
    /// A needle matched against a whole module says only that *something* in it
    /// has that shape, which is the wrong question when the claim is about one
    /// body.
    fn module_func_body<'v>(v: &'v str, name: &str) -> &'v str {
        let opening = format!("Definition {name} : module_func := {{|");
        let start = v
            .find(&opening)
            .unwrap_or_else(|| panic!("no `module_func` named `{name}` in:\n{v}"));
        let body = &v[start..];
        let end = body
            .find("|}.")
            .unwrap_or_else(|| panic!("unterminated `module_func` `{name}` in:\n{v}"));
        &body[..end]
    }

    /// The term of the first obligation the specification `spec` of module
    /// `module` states.
    ///
    /// The emitter prints one obligation per `Definition … : hassert :=` with
    /// its whole term on the following line.
    fn first_obligation_term<'v>(v: &'v str, module: &str, spec: &str) -> &'v str {
        let header = format!("Definition {module}__{spec}_hspec1 : hassert :=");
        let lines: Vec<&str> = v.lines().collect();
        let at = lines
            .iter()
            .position(|line| *line == header)
            .unwrap_or_else(|| panic!("no obligation `{header}` in:\n{v}"));
        lines
            .get(at + 1)
            .unwrap_or_else(|| panic!("obligation `{header}` has no term in:\n{v}"))
            .trim()
    }

    /// The module record body the first obligation of `spec` applies.
    ///
    /// The applied ordinal is read out of the obligation itself rather than
    /// written down, so a record that reordered would fail as a body carrying
    /// the wrong fingerprint instead of passing on a stale constant.
    fn applied_body<'v>(v: &'v str, module: &str, spec: &str) -> &'v str {
        let term = first_obligation_term(v, module, spec);
        let applied = term
            .find("T_app ")
            .map(|at| &term[at + "T_app ".len()..])
            .unwrap_or_else(|| {
                panic!("obligation `{module}__{spec}_hspec1` applies nothing: {term}")
            });
        let digits: String = applied.chars().take_while(char::is_ascii_digit).collect();
        let index: usize = digits
            .parse()
            .unwrap_or_else(|e| panic!("`T_app {digits}` is not an ordinal ({e}): {term}"));
        let names = module_func_names(v);
        let name = names.get(index).unwrap_or_else(|| {
            panic!(
                "obligation `{module}__{spec}_hspec1` applies ordinal {index}, past the {} \
                 functions of the record: {names:?}",
                names.len()
            )
        });
        module_func_body(v, name)
    }

    /// The premise the whole feature rests on: a library has obligations to
    /// carry only when it was compiled in proof mode, and such a library is a
    /// legitimate link input with its specification functions still in it.
    #[test]
    fn a_proof_mode_library_ships_the_sections_a_link_can_adopt() {
        let proof = library(LIBRARY);
        let sections = custom_section_names(&proof);
        for section in ["inference.spec_funcs", "inference.hspecs"] {
            assert!(
                sections.iter().any(|name| name.as_str() == section),
                "a proof-mode library must ship `{section}`, the section an adoption reads; \
                 {LIBRARY} carries {sections:?}"
            );
        }

        let executable =
            rocq_test_support::compile_fixture(LIBRARY, LIBRARY_MODULE, CompilationMode::Compile);
        let executable_sections = custom_section_names(&executable);
        for section in ["inference.spec_funcs", "inference.hspecs"] {
            assert!(
                !executable_sections.iter().any(|name| name.as_str() == section),
                "a library compiled for execution states no obligations, so `{section}` must be \
                 absent; {LIBRARY} carries {executable_sections:?}"
            );
        }

        let merged = merge_under(PROGRAM, PROGRAM_MODULE, &proof, ExternalSpecPolicy::Ignore);
        let merged_sections = custom_section_names(&merged.wasm);
        for section in ["inference.spec_funcs", "inference.hspecs"] {
            assert!(
                !merged_sections.iter().any(|name| name.as_str() == section),
                "a link that adopts nothing must leave the library's `{section}` behind rather \
                 than copy it into a module it does not describe; the output carries \
                 {merged_sections:?}"
            );
        }
    }

    /// Adoption reaches the emitted proof: the library's specification arrives
    /// under a key namespaced by the logical module, as a `ValidSpec` theorem of
    /// the merged module, and its obligation applies the merged body.
    #[test]
    fn an_adopted_specification_reaches_the_emitted_proof() {
        let merged = merge_under(
            PROGRAM,
            PROGRAM_MODULE,
            &library(LIBRARY),
            ExternalSpecPolicy::Adopt,
        );
        assert!(
            merged.warnings.is_empty(),
            "a library whose every obligation is universal leaves nothing behind, so the link \
             owes the user no report: {:?}",
            merged.warnings
        );

        let v = rocq_test_support::translate(PROGRAM, PROGRAM_MODULE, &merged.wasm);
        let key = format!("{PROGRAM_MODULE}__{LOGICAL_MODULE}_ScaleSpec");
        assert!(
            v.contains(&format!("Definition {key}_specs : list hassert :=")),
            "the adopted specification must reach the proof as a list of its own; .v was:\n{v}"
        );
        assert!(
            v.contains(&format!(
                "Theorem valid_{key} : ValidSpec {PROGRAM_MODULE} {key}_specs."
            )),
            "the adopted specification must reach the proof as a claim about the merged module; \
             .v was:\n{v}"
        );

        let body = applied_body(&v, PROGRAM_MODULE, &format!("{LOGICAL_MODULE}_ScaleSpec"));
        assert!(
            body.contains(LIBRARY_FINGERPRINT),
            "the adopted obligation must apply the merged library body; the record entry at the \
             ordinal it names is:\n{body}"
        );
        assert!(
            !body.contains(PROGRAM_FINGERPRINT),
            "the adopted obligation applied the program's own body instead of the merged one:\n{body}"
        );
    }

    /// A program's own obligations and an adopted one coexist, and they name
    /// different bodies.
    ///
    /// Two obligations in one artifact is the shape that can go wrong silently:
    /// a merge that folded the adopted key into the program's own would leave a
    /// single theorem stating the wrong list, at exit 0.
    #[test]
    fn an_adopted_obligation_and_the_programs_own_name_different_bodies() {
        let merged = merge_under(
            BOTH,
            BOTH_MODULE,
            &library(LIBRARY),
            ExternalSpecPolicy::Adopt,
        );
        let v = rocq_test_support::translate(BOTH, BOTH_MODULE, &merged.wasm);

        for spec in ["SumSpec", "mathlib_ScaleSpec"] {
            assert!(
                v.contains(&format!(
                    "Theorem valid_{BOTH_MODULE}__{spec} : ValidSpec {BOTH_MODULE} \
                     {BOTH_MODULE}__{spec}_specs."
                )),
                "both specifications must reach the proof under their own names, `{spec}` did \
                 not; .v was:\n{v}"
            );
        }

        let adopted = applied_body(&v, BOTH_MODULE, "mathlib_ScaleSpec");
        assert!(
            adopted.contains(LIBRARY_FINGERPRINT) && !adopted.contains(PROGRAM_FINGERPRINT),
            "the adopted obligation must apply the merged library body; it applied:\n{adopted}"
        );
        let own = applied_body(&v, BOTH_MODULE, "SumSpec");
        assert!(
            own.contains(PROGRAM_FINGERPRINT) && !own.contains(LIBRARY_FINGERPRINT),
            "the program's own obligation must still apply the program's own body; it applied:\n{own}"
        );
    }

    /// The default: the library's obligations stay out of the artifact, and the
    /// link says so.
    ///
    /// The partner of [`an_adopted_specification_reaches_the_emitted_proof`]:
    /// without it, that test could be passing on a `.v` that carried the
    /// library's specification no matter what was asked for.
    #[test]
    fn the_default_policy_leaves_the_librarys_obligations_out_of_the_v() {
        let merged = merge_under(
            PROGRAM,
            PROGRAM_MODULE,
            &library(LIBRARY),
            ExternalSpecPolicy::Warn,
        );
        assert_eq!(
            merged.warnings,
            vec![LinkWarning::ExternalSpecsDropped {
                modules: vec![LOGICAL_MODULE.to_string()],
            }],
            "a merge that dropped a library's obligations owes the user exactly one report \
             naming it"
        );

        let v = rocq_test_support::translate(PROGRAM, PROGRAM_MODULE, &merged.wasm);
        assert!(
            !v.contains("ScaleSpec"),
            "the library's specification must be absent from a proof that did not adopt it; \
             .v was:\n{v}"
        );
        assert!(
            !v.contains("ValidSpec"),
            "this program states no obligations of its own, so a `ValidSpec` in its proof came \
             from the library; .v was:\n{v}"
        );
    }

    /// Adoption carries the universal half and names the reachability half it
    /// left behind.
    ///
    /// The library here is the same interface as the one every other test links,
    /// differing only in shipping an `exists` obligation as well — so the report
    /// is attributable to that obligation and to nothing else about the link.
    #[test]
    fn adoption_reports_the_reachability_obligation_it_could_not_carry() {
        let merged = merge_under(
            PROGRAM,
            PROGRAM_MODULE,
            &library(REACH_LIBRARY),
            ExternalSpecPolicy::Adopt,
        );
        let reported: Vec<&LinkWarning> = merged
            .warnings
            .iter()
            .filter(|warning| {
                matches!(
                    warning,
                    LinkWarning::ReachabilityObligationsNotAdopted { .. }
                )
            })
            .collect();
        let [LinkWarning::ReachabilityObligationsNotAdopted {
            module,
            adopted,
            obligations,
        }] = reported.as_slice()
        else {
            panic!(
                "a library shipping one reachability obligation earns exactly one report of it, \
                 got {:?}",
                merged.warnings
            )
        };
        assert_eq!(module.as_str(), LOGICAL_MODULE);
        assert_eq!(
            *adopted, 1,
            "the report must carry what this library did contribute, so its closing clause can \
             tell a partial adoption from one that carried nothing"
        );
        assert_eq!(obligations.len(), 1, "reported obligations: {obligations:?}");
        assert!(
            obligations[0].contains("Bounds") && obligations[0].contains("(exists)"),
            "the report must name the specification and the kind of what it left behind, got \
             `{}`",
            obligations[0]
        );

        let v = rocq_test_support::translate(PROGRAM, PROGRAM_MODULE, &merged.wasm);
        assert!(
            v.contains(&format!(
                "Theorem valid_{PROGRAM_MODULE}__{LOGICAL_MODULE}_Bounds : ValidSpec"
            )),
            "the universal half of the library's specification must still be adopted; .v was:\n{v}"
        );
        assert!(
            !v.contains("ValidExistsSpec"),
            "a reachability judgment reduces a specification function no merged module contains, \
             so none may reach the proof; .v was:\n{v}"
        );
    }

    /// The entry file of a two-file library: it defines the `scale` a program
    /// links against, and states a specification about it.
    const ALIAS_LIBRARY_ENTRY: &str = "\
use lib;

pub fn scale(a: i32) -> i32 {
    return a * 10007;
}

spec Checks {
    fn checks() forall {
        let x: i32 = @;
        assert(scale(x) == x * 10007);
    }
}
";

    /// The library's second file. Its specification declares a function named
    /// `scale` too, which is legal precisely because the two live in different
    /// files: the type checker rejects the shadow only within one file.
    const ALIAS_LIBRARY_SIDE: &str = "\
spec Aliased {
    fn scale() forall {
        let x: i32 = @;
        assert(x == x);
    }
}
";

    /// Compiles the two-file library in proof mode, the way an ordinary
    /// multi-file project is compiled.
    fn alias_library() -> Vec<u8> {
        let typed = try_type_check_multi_file(&[
            (vec![], ALIAS_LIBRARY_ENTRY),
            (vec!["lib"], ALIAS_LIBRARY_SIDE),
        ])
        .expect("the two-file library type-checks");
        inference_analysis::analyze(&typed).expect("the two-file library passes analysis");
        inference_wasm_codegen::codegen(
            &typed,
            LIBRARY_MODULE,
            inference_wasm_codegen::CodegenOptions {
                mode: CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("the two-file library compiles in proof mode")
        .wasm()
        .to_vec()
    }

    /// An ordinary proof-mode library can name two functions `scale`, and a
    /// program must still be able to adopt from it.
    ///
    /// A specification function's `name`-section symbol is unqualified by its
    /// defining file, so a library whose `spec` declares `fn scale` beside its
    /// own top-level `fn scale` records the string twice. Only one of the two is
    /// a body an obligation can be about, and which one is not a judgment the
    /// linker is free to make differently from the proof translator: the
    /// translator narrows the same way when it resolves the library's own
    /// obligations, so a linker that counted the specification function would
    /// refuse a library that translates correctly on its own.
    ///
    /// The shadow is reachable from ordinary source because the type checker
    /// forbids it only within a single file — which is why the library here is
    /// two files rather than hand-built bytes.
    #[test]
    fn a_library_whose_spec_repeats_a_top_level_name_is_still_adoptable() {
        let lib = alias_library();
        let carriers: Vec<u32> = function_names(&lib)
            .into_iter()
            .filter(|(_, name)| name == "scale")
            .map(|(idx, _)| idx)
            .collect();
        assert_eq!(
            carriers.len(),
            2,
            "the premise is that the library names two functions `scale`; it names {:?}",
            function_names(&lib)
        );

        // Linked here rather than through `merge_under`, so a refusal is reported
        // as this test's own claim about which libraries are adoptable.
        let main =
            rocq_test_support::compile_fixture(PROGRAM, PROGRAM_MODULE, CompilationMode::Proof);
        let merged = inference::link_with_options(
            &main,
            &[(LOGICAL_MODULE, lib.as_slice())],
            None,
            &LinkOptions {
                external_specs: ExternalSpecPolicy::Adopt,
            },
        )
        .expect("a library naming a specification function like a top-level one is adoptable");

        let v = rocq_test_support::translate(PROGRAM, PROGRAM_MODULE, &merged.wasm);
        assert!(
            v.contains(&format!(
                "Theorem valid_{PROGRAM_MODULE}__{LOGICAL_MODULE}_Checks : ValidSpec"
            )),
            "the library's specification must be adopted; .v was:\n{v}"
        );

        let body = applied_body(&v, PROGRAM_MODULE, &format!("{LOGICAL_MODULE}_Checks"));
        assert!(
            body.contains(LIBRARY_FINGERPRINT),
            "the adopted obligation must apply the library's executable body, not its \
             specification function; the record entry at the ordinal it names is:\n{body}"
        );
    }

    /// The path a real `infc -L … -v --adopt-external-specs` invocation takes:
    /// the library is a file on a search path, resolved and validated by the
    /// driver before the merge sees it.
    ///
    /// The in-process tests above hand the linker bytes directly, so nothing in
    /// them exercises the resolution gate a proof-mode library has to pass first
    /// — the gate that decodes its whole module, specification functions
    /// included.
    #[test]
    fn adoption_survives_the_driver_resolution_path() {
        let lib_dir = tempfile::tempdir().expect("create a library directory");
        std::fs::write(
            lib_dir.path().join(format!("{LOGICAL_MODULE}.wasm")),
            library(LIBRARY),
        )
        .expect("write the library");

        let path = get_test_data_path().join("inf").join(PROGRAM);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let arena = inference::parse(&source).expect("the program parses");
        let typed = inference::type_check(arena).expect("the program type-checks");
        inference_analysis::analyze(&typed).expect("the program passes analysis");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("a proof-mode library resolves and passes the driver's validation gate");

        let main =
            rocq_test_support::compile_fixture(PROGRAM, PROGRAM_MODULE, CompilationMode::Proof);
        let merged = inference::link_with_options(
            &main,
            &externals.module_bytes(),
            Some(&externals.contracts),
            &LinkOptions {
                external_specs: ExternalSpecPolicy::Adopt,
            },
        )
        .expect("the resolved library links");

        let v = rocq_test_support::translate(PROGRAM, PROGRAM_MODULE, &merged.wasm);
        assert!(
            v.contains(&format!(
                "Theorem valid_{PROGRAM_MODULE}__{LOGICAL_MODULE}_ScaleSpec : ValidSpec"
            )),
            "the library's specification must be adopted on the path the compiler driver takes, \
             not only when the bytes are handed over directly; .v was:\n{v}"
        );
    }
}

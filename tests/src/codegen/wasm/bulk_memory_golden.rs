//! Golden coverage for the `bulk-memory` opt-in instruction level.
//!
//! Requesting `bulk-memory` restores the region fill and copy forms the compiler
//! emitted before it moved to a Wasm 1.0 default: one `memory.fill` for a frame's
//! zero fill and one `memory.copy` for a compound copy, in place of the store
//! sequences and index loops the default level lowers them to. This family pins
//! the bytes that opt-in produces.
//!
//! ## Why the sources are a manifest and not copies
//!
//! Every entry below names a fixture that already exists elsewhere in the corpus.
//! Copying the sources here would fork them: a later edit to `base/struct_copy`
//! would silently stop being the program this family compiles, and the two levels
//! would no longer be two views of one input. Naming the existing path keeps the
//! comparison honest — the same source, compiled twice, differing only in the
//! requested feature set.
//!
//! The entries are exactly the fixtures whose default-level goldens changed when
//! the compiler dropped bulk memory, which is to say exactly the corpus shapes
//! that reach a fill or copy emitter at all.
//!
//! ## What the initial golden bytes are
//!
//! They were captured from the last commit that emitted bulk memory by default,
//! so the first green run of this file was a statement that the opt-in reproduces
//! the older compiler's output byte for byte. That statement is a point in time
//! and does not survive intentional codegen changes: from here these are ordinary
//! regenerable goldens, moved through `mod regenerate` like every other family.
//!
//! Two corpus-wide gates in `validation.rs` cover this directory from the other
//! side — every artifact in it must carry a bulk-memory operator and must
//! validate at Wasm 1.0 plus bulk memory — so a golden regenerated from a build
//! that quietly ignored the opt-in fails there even though it would match itself
//! here.

#[cfg(test)]
mod bulk_memory_golden_tests {
    use crate::utils::{
        AnalysisMode, assert_wasms_modules_equivalence, assert_wat_equivalence,
        codegen_impl_with_features, get_test_data_path, get_test_wasm_path,
        wasm_codegen_project_with_features,
    };
    use inference_wasm_codegen::{CompilationMode, EmitFeatures, Target};

    /// The feature set every golden in this family is compiled with.
    fn bulk_memory() -> EmitFeatures {
        EmitFeatures { bulk_memory: true }
    }

    /// The single-file half of the family: the directory under `codegen/wasm`
    /// holding each fixture's own directory (empty when the fixture directory sits
    /// at the top level), and the fixture name.
    const SINGLE_FILE_SOURCES: &[(&str, &str)] = &[
        ("", "algo_array"),
        ("", "literal_ctx_array_elements"),
        ("", "literal_ctx_nested_array"),
        ("", "short_circuit"),
        ("base", "array_assign"),
        ("base", "array_index"),
        ("base", "array_literal"),
        ("base", "array_nondet"),
        ("base", "array_of_structs"),
        ("base", "array_params"),
        ("base", "array_self_ref_reassign"),
        ("base", "array_zero_literal"),
        ("base", "const_array"),
        ("base", "const_array_sum"),
        ("base", "const_compound_copy"),
        ("base", "const_compound_mixed"),
        ("base", "const_in_forall"),
        ("base", "const_sret_call"),
        ("base", "const_struct"),
        ("base", "enum_array"),
        ("base", "enum_in_struct"),
        ("base", "enum_uzumaki_domain"),
        ("base", "if_else_compound_overlap"),
        ("base", "method_array_return"),
        ("base", "method_assoc"),
        ("base", "method_cross_call"),
        ("base", "method_i64_fields"),
        ("base", "method_instance"),
        ("base", "method_multi_struct"),
        ("base", "method_return_struct"),
        ("base", "method_self_mutate"),
        ("base", "method_three_fields"),
        ("base", "multidim_array_literal"),
        ("base", "multidim_array_uzumaki"),
        ("base", "narrow_uzumaki"),
        ("base", "nested_array_of_structs"),
        ("base", "nested_struct"),
        ("base", "nested_struct_with_array"),
        ("base", "struct_access"),
        ("base", "struct_array_field_nondet"),
        ("base", "struct_assign"),
        ("base", "struct_copy"),
        ("base", "struct_literal"),
        ("base", "struct_nondet"),
        ("base", "struct_params"),
        ("base", "struct_return"),
        ("base", "struct_self_ref_reassign"),
        ("base", "struct_with_array"),
        ("base", "struct_with_array_of_structs"),
        ("base", "struct_with_nested_array"),
        ("loops", "loop_return_array"),
        ("loops", "loop_with_array"),
        ("loops", "loop_zero_init"),
    ];

    /// The project half: fixture trees under `multi_file_golden`, compiled through
    /// the project front end so the merged module is the one under test.
    const PROJECT_SOURCES: &[&str] = &[
        "cross_file_method",
        "cross_file_struct",
        "dup_struct",
        "method_mangling",
    ];

    fn codegen_wasm_dir() -> std::path::PathBuf {
        get_test_data_path().join("codegen").join("wasm")
    }

    /// The existing `.inf` a single-file entry reuses. These paths point *out* of
    /// this family, into whichever module owns the fixture at the default level.
    fn single_file_source_path(parent: &str, name: &str) -> std::path::PathBuf {
        let mut dir = codegen_wasm_dir();
        if !parent.is_empty() {
            dir = dir.join(parent);
        }
        dir.join(name).join(format!("{name}.inf"))
    }

    /// This family's golden `.wasm`, resolved by the shared module-path convention.
    ///
    /// The data directory is named for this module, so the same resolver every
    /// other golden family uses finds these too, and `module_path!()` is written
    /// here rather than passed in — a caller in a nested module would expand it to
    /// its own path and resolve the wrong directory.
    fn golden_wasm_path(name: &str) -> std::path::PathBuf {
        get_test_wasm_path(module_path!(), name)
    }

    /// The family's data directory, spelled out only because listing the family
    /// has no per-fixture path to resolve from.
    fn family_root() -> std::path::PathBuf {
        codegen_wasm_dir().join("bulk_memory_golden")
    }

    /// Module path that resolves the `multi_file_golden` fixture trees, whose
    /// sources the project entries reuse.
    fn project_module_path() -> &'static str {
        "inference_tests::codegen::wasm::multi_file_golden::multi_file_golden_codegen_tests"
    }

    /// Compiles a single-file entry at the opt-in level.
    ///
    /// Analysis is skipped for every entry rather than per fixture: it does not
    /// influence the emitted bytes, several entries deliberately exercise
    /// constructs analysis rejects, and whether each source is *accepted* is
    /// already asserted by the default-level golden test that owns it.
    fn compile_single_file(parent: &str, name: &str, mode: CompilationMode) -> Vec<u8> {
        let path = single_file_source_path(parent, name);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let target = Target::default();
        codegen_impl_with_features(
            &source,
            target,
            mode,
            target.default_opt_level(),
            AnalysisMode::Skip,
            bulk_memory(),
        )
        .unwrap_or_else(|e| panic!("opt-in codegen failed for {name}: {e}"))
        .wasm()
        .to_vec()
    }

    /// Byte compare against the committed golden, then WAT compare through the
    /// shared helper — whose skip-when-absent behaviour is what covers the
    /// non-deterministic entries, which carry custom opcodes `wasmprinter` cannot
    /// render and so have no `.wat`.
    fn assert_matches_golden(name: &str, actual: &[u8]) {
        let path = golden_wasm_path(name);
        let expected = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        assert_wasms_modules_equivalence(&expected, actual);
        assert_wat_equivalence(actual, module_path!(), name);
    }

    /// Names of the goldens present in the family directory.
    fn golden_names_on_disk() -> Vec<String> {
        let root = family_root();
        let entries = std::fs::read_dir(&root)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", root.display()));
        let mut names: Vec<String> = entries
            .map(|entry| entry.expect("failed to read a directory entry").path())
            .filter(|path| path.is_dir())
            .map(|path| {
                path.file_name()
                    .expect("a fixture directory has a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// The manifest names exactly the goldens on disk.
    ///
    /// The two lists are maintained by hand at opposite ends of the repository, so
    /// either can drift: a golden with no entry is never compared against anything
    /// and an entry with no golden would only fail with a missing-file panic
    /// inside whichever test happened to reach it. Comparing the sets names the
    /// discrepancy directly.
    #[test]
    fn manifest_and_family_directory_agree() {
        let mut from_manifest: Vec<String> = SINGLE_FILE_SOURCES
            .iter()
            .map(|(_, name)| (*name).to_string())
            .chain(PROJECT_SOURCES.iter().map(|name| (*name).to_string()))
            .collect();
        from_manifest.sort();

        let mut deduplicated = from_manifest.clone();
        deduplicated.dedup();
        assert_eq!(
            deduplicated, from_manifest,
            "two manifest entries share a name, so they would share one golden"
        );
        assert_eq!(
            from_manifest,
            golden_names_on_disk(),
            "the manifest and the golden directory must name the same fixtures"
        );
    }

    #[test]
    fn single_file_family_matches_goldens() {
        for (parent, name) in SINGLE_FILE_SOURCES {
            let actual = compile_single_file(parent, name, CompilationMode::Compile);
            assert_matches_golden(name, &actual);
        }
    }

    #[test]
    fn project_family_matches_goldens() {
        for name in PROJECT_SOURCES {
            let actual =
                wasm_codegen_project_with_features(project_module_path(), name, bulk_memory());
            assert_matches_golden(name, &actual);
        }
    }

    /// The opt-in applies in proof mode too, across every shape in the family.
    ///
    /// A feature gated on the compilation mode would make the `.v` describe a
    /// different program than the shipped `.wasm`. The end-to-end case is covered
    /// by `infc`'s own integration tests; what this adds is breadth — the fill and
    /// copy emitters are reached from different sites by different shapes, and a
    /// mode check left in one of them would show up in only some of them.
    #[test]
    fn proof_mode_family_carries_bulk_memory_operators() {
        for (parent, name) in SINGLE_FILE_SOURCES {
            let wasm = compile_single_file(parent, name, CompilationMode::Proof);
            assert!(
                contains_bulk_memory_operator(&wasm),
                "proof-mode {name} must honor the requested features"
            );
        }
    }

    fn contains_bulk_memory_operator(wasm: &[u8]) -> bool {
        use inf_wasmparser::{Operator, Parser, Payload};

        for payload in Parser::new(0).parse_all(wasm) {
            let Ok(Payload::CodeSectionEntry(body)) = payload else {
                continue;
            };
            let Ok(operators) = body.get_operators_reader() else {
                continue;
            };
            for op in operators {
                let Ok(op) = op else { continue };
                if matches!(
                    op,
                    Operator::MemoryFill { .. }
                        | Operator::MemoryCopy { .. }
                        | Operator::MemoryInit { .. }
                        | Operator::DataDrop { .. }
                ) {
                    return true;
                }
            }
        }
        false
    }

    /// Extracts one function's WAT text, from its `(func $name ` header to the
    /// closing paren at function indentation.
    fn function_wat(wat: &str, name: &str) -> String {
        let marker = format!("(func ${name} ");
        let start = wat
            .find(&marker)
            .unwrap_or_else(|| panic!("no function ${name} in the printed module"));
        let rest = &wat[start..];
        let end = rest.find("\n  )").map_or(rest.len(), |offset| offset + 4);
        rest[..end].to_string()
    }

    fn printed_module(source: &str, features: EmitFeatures) -> String {
        let target = Target::default();
        let wasm = codegen_impl_with_features(
            source,
            target,
            CompilationMode::Compile,
            target.default_opt_level(),
            AnalysisMode::Skip,
            features,
        )
        .expect("frame_fill compiles at both instruction levels")
        .wasm()
        .to_vec();
        wasmprinter::print_bytes(&wasm).expect("failed to print WAT")
    }

    /// One `memory.fill` replaces the whole prologue, loop and scratch local
    /// included.
    ///
    /// The goldens alone cannot say this. They pin bytes, and a build that emitted
    /// the bulk instruction *in addition to* keeping the loop's induction variable
    /// alive, or that reached the bulk branch only after the scratch allocator had
    /// already claimed a local, would produce a module that runs correctly and
    /// matches a regenerated golden. The 320-byte frame is the sharpest case: it
    /// is far enough past the byte threshold that the default level has no choice
    /// but the looped form.
    ///
    /// Every claim is made against the same source compiled twice, differing only
    /// in the feature set, so the default side doubles as the proof that the
    /// assertion targets something really there to remove.
    ///
    /// The scratch check reads each function's own local declarations rather than
    /// the whole module: a scratch slot is an *anonymous* local, and so is the
    /// temporary a bounds check spills its index into, which no instruction set
    /// removes. The four functions below index their frames by constants, so their
    /// anonymous locals can only be region scratch.
    #[test]
    fn opt_in_frame_fill_drops_the_loop_and_its_scratch_locals() {
        let path = codegen_wasm_dir()
            .join("bulk_free")
            .join("frame_fill")
            .join("frame_fill.inf");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        let default_level = printed_module(&source, EmitFeatures::default());
        let opt_in = printed_module(&source, bulk_memory());

        for name in [
            "fill_144_loop",
            "fill_160_loop",
            "fill_320_loop",
            "fill_multi_slot",
        ] {
            let default_fill = function_wat(&default_level, name);
            let opt_in_fill = function_wat(&opt_in, name);

            assert!(
                default_fill.contains("\n    loop"),
                "{name} must use the fill loop at the default level:\n{default_fill}"
            );
            assert!(
                declares_anonymous_local(&default_fill),
                "...driven by an anonymous scratch local:\n{default_fill}"
            );

            assert!(
                !opt_in_fill.contains("\n    loop"),
                "the opt-in fill must leave no loop in {name}'s prologue:\n{opt_in_fill}"
            );
            assert!(
                !declares_anonymous_local(&opt_in_fill),
                "...and must reach the bulk branch before any scratch is allocated:\n{opt_in_fill}"
            );
            assert!(
                opt_in_fill.contains("memory.fill"),
                "...because one memory.fill replaced both:\n{opt_in_fill}"
            );
        }
    }

    /// Whether a function's local declarations include an unnamed slot. Named
    /// locals are the program's own variables and the frame pointer; the compiler
    /// names neither its region scratch nor its bounds-check temporaries.
    fn declares_anonymous_local(function_wat: &str) -> bool {
        function_wat
            .lines()
            .skip(1)
            .take_while(|line| line.trim_start().starts_with("(local"))
            .any(|line| line.contains("(local i32"))
    }

    /// Regeneration helpers for this family's goldens.
    ///
    /// `#[ignore]`d by design: they are not behavioral tests, they rewrite the
    /// committed goldens from current compiler output. Run explicitly after an
    /// intentional codegen change:
    ///
    /// ```bash
    /// cargo test -p inference-tests bulk_memory_golden::regenerate -- --ignored
    /// ```
    ///
    /// One test per half rather than per fixture: the family is a list, and an
    /// intentional codegen change moves all of it at once.
    #[cfg(test)]
    mod regenerate {
        use crate::utils::regenerate_wat;

        fn write_golden(name: &str, wasm: &[u8]) {
            let path = super::golden_wasm_path(name);
            std::fs::write(&path, wasm)
                .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
            println!("Regenerated: {} ({} bytes)", path.display(), wasm.len());
            let dir = path.parent().expect("a golden path has a parent directory");
            regenerate_wat(wasm, dir, name);
        }

        #[test]
        #[ignore]
        fn regenerate_single_file_goldens() {
            for (parent, name) in super::SINGLE_FILE_SOURCES {
                let wasm = super::compile_single_file(
                    parent,
                    name,
                    inference_wasm_codegen::CompilationMode::Compile,
                );
                write_golden(name, &wasm);
            }
        }

        #[test]
        #[ignore]
        fn regenerate_project_goldens() {
            for name in super::PROJECT_SOURCES {
                let wasm = crate::utils::wasm_codegen_project_with_features(
                    super::project_module_path(),
                    name,
                    super::bulk_memory(),
                );
                write_golden(name, &wasm);
            }
        }
    }
}

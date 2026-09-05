//! Stock-decoder validity of the proof-mode module, fixture by fixture.
//!
//! A proof-mode artifact used to be loadable only by this project's own
//! `wasmparser` fork, because a specification body carried `0xfc`-prefixed
//! opcodes no standard decoder assigns. Choice lowering removed them: a
//! specification function's `@`s arrive as hidden trailing parameters, so its
//! body is ordinary WebAssembly and the module a proof build writes is one any
//! tool can read. This suite is what holds that.
//!
//! It is a per-fixture expected-verdict table rather than a pass count on
//! purpose. A count stays green when one fixture starts validating and another
//! stops compiling, and it stays green when a fixture validates because it
//! silently stopped emitting the construct it exists to cover. The table names
//! the verdict for every `.inf` under `tests/test_data/inf/`, including the ones
//! that reach no module at all and why, and
//! [`every_fixture_in_the_corpus_directory_is_listed`] refuses a new fixture
//! that is not listed — a fixture cannot escape the gate by being added.
//!
//! Nothing here shells out. Validation is stock `wasmparser`, which is a
//! different decoder from the fork the rest of the suite validates with: the
//! fork accepts the custom opcodes and therefore structurally cannot observe the
//! property this file is about.
//!
//! ## Where this gate and the `infc` command line differ, and why
//!
//! The pipeline below is the single-file one — parse, type-check, analyze,
//! generate — which is what every other in-process suite drives. The command
//! line adds two steps around it, and each accounts for one fixture class whose
//! verdict here is deliberately not the command line's:
//!
//! - `example.inf` names source *files* in its `use` clauses. The driver
//!   resolves those against the source root and reports a missing file before
//!   parsing; in process there is no project context, so the same clauses are
//!   reported by the type checker instead. Either way it reaches no module.
//! - `spec_linked_extern.inf`, `spec_linked_toolchain.inf`,
//!   `spec_adopted_extern.inf` and `spec_adopted_both.inf` bind externals the
//!   driver resolves to `.wasm` files through `-L` and merges. Resolution is a
//!   driver step, not a code generation one: the main module compiles and
//!   validates on its own, with its externals as ordinary WebAssembly imports,
//!   which is exactly the property this file asserts about it. The *merged*
//!   artifact is gated by the linked corpus in `rocq_typecheck.rs`.
//!
//! So the table's split is 37 valid and 9 rejected, where `infc --mode proof`
//! run over the same directory reaches 33 modules and refuses 13: the four
//! linked fixtures are the difference, in the direction of this gate covering
//! more rather than less.

#[cfg(test)]
mod gate {
    use crate::utils::{get_test_data_path, try_build_ast};
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{
        CodegenOptions, CodegenOutput, CompilationMode, SPEC_FUNCS_SECTION_NAME,
        SPEC_FUNCS_SECTION_VERSION, Target,
    };

    /// Every `0xfc`-prefixed opcode this compiler can emit: the two uzumaki
    /// draws and the four non-deterministic block wrappers. No specification
    /// body may carry one.
    const CUSTOM_OPCODES: [u8; 6] = [0x31, 0x32, 0x3a, 0x3b, 0x3c, 0x3d];

    /// What the choice lowering has to do for a fixture's proof-mode module to
    /// load in a stock decoder — the strongest of its parts the fixture needs.
    ///
    /// This is documentation the table carries rather than an assertion: the
    /// parts are held to having teeth by the neutralization tests in
    /// `core/wasm-codegen/src/choice_lowering_tests.rs`, each of which removes
    /// one and watches a fixture shape go red. What the column buys is that a
    /// reviewer can see which fixtures stop covering a part if it is reverted,
    /// and that a fixture rewritten into a weaker shape becomes visibly
    /// misfiled.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Lowering {
        /// Nothing: the module carries no `spec`, or no specification function
        /// in it carries a non-deterministic construct.
        NothingToLower,
        /// Nothing new: every non-deterministic body in it is `exists` or
        /// `unique`, and those were already supplied through a choice suffix.
        AlreadyChoiceLowered,
        /// Only the block wrapper had to go: its universal bodies quantify over
        /// declared parameters and draw nothing.
        WrapperSuppression,
        /// At least one scalar `@` in a universal body becomes a choice
        /// parameter.
        ScalarChoice,
        /// At least one aggregate `@` expands to one choice parameter per
        /// scalar leaf.
        AggregateLeaves,
    }

    /// The stage that refuses a fixture, for the fixtures that exist to be
    /// refused.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Stage {
        /// The parser refuses it.
        Parse,
        /// The type checker refuses it.
        TypeCheck,
        /// An analysis rule refuses it. Every reported error must carry this id,
        /// so a fixture that starts failing for an unrelated reason is not
        /// silently accepted as still-covered.
        Analysis(&'static str),
        /// A proof-obligation diagnostic refuses it during code generation. The
        /// message must carry this code.
        Codegen(&'static str),
    }

    /// The verdict a fixture must reach.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Expected {
        /// A module a stock WebAssembly decoder accepts, whose specification
        /// bodies carry no custom opcode.
        StockValid(Lowering),
        /// No module at all: the pipeline stops at the named stage.
        Rejected(Stage),
    }

    /// One corpus entry: the `.inf` file stem, its verdict, and the sentence
    /// that says why that verdict is the right one.
    struct Fixture {
        stem: &'static str,
        expected: Expected,
        why: &'static str,
    }

    use Expected::{Rejected, StockValid};
    use Lowering::{
        AggregateLeaves, AlreadyChoiceLowered, NothingToLower, ScalarChoice, WrapperSuppression,
    };
    use Stage::{Analysis, Codegen, Parse, TypeCheck};

    /// Every `.inf` under `tests/test_data/inf/`, in directory order.
    const CORPUS: &[Fixture] = &[
        Fixture {
            stem: "bad_syntax",
            expected: Rejected(Parse),
            why: "a deliberately malformed file, the parser-error fixture",
        },
        Fixture {
            stem: "example",
            expected: Rejected(TypeCheck),
            why: "a language tour, not a compilable program: its `use inference::std;` clauses \
                  name source files, which need a project context, and it redeclares names on \
                  purpose",
        },
        Fixture {
            stem: "mixed_compile_proof",
            expected: StockValid(WrapperSuppression),
            why: "two `forall` bodies quantifying over nothing but declared parameters; no `@` \
                  anywhere, so the wrapper is the whole of what had to go",
        },
        Fixture {
            stem: "nondet_blocks",
            expected: Rejected(Analysis("A042")),
            why: "the negative fixture for non-deterministic blocks in executable code",
        },
        Fixture {
            stem: "nondet_body_modifiers",
            expected: Rejected(Analysis("A042")),
            why: "the same negative, with the construct as a function-body modifier",
        },
        Fixture {
            stem: "rocq_control_flow",
            expected: StockValid(ScalarChoice),
            why: "a `forall` body drawing one scalar around structured control flow",
        },
        Fixture {
            stem: "rocq_false_certificate",
            expected: StockValid(NothingToLower),
            why: "the false-certificate producer: a universal assertion with no value draws",
        },
        Fixture {
            stem: "rocq_exists_spec",
            expected: StockValid(AlreadyChoiceLowered),
            why: "the corpus producer of the `ValidExistsSpec` grammar; its only \
                  non-deterministic body is the `exists` one, already choice-lowered",
        },
        Fixture {
            stem: "rocq_name_collisions",
            expected: StockValid(ScalarChoice),
            why: "names functions and specs after emitted-`.v` symbols; its universal specs draw \
                  scalars alongside the reachability pair",
        },
        Fixture {
            stem: "rocq_prime_bounded_example",
            expected: StockValid(ScalarChoice),
            why: "the worked primality example with a source-visible signed discharge bound",
        },
        Fixture {
            stem: "rocq_prime_example",
            expected: StockValid(ScalarChoice),
            why: "the worked primality example: a `forall` body over drawn scalars",
        },
        Fixture {
            stem: "rocq_spec_shapes",
            expected: StockValid(ScalarChoice),
            why: "the shape survey: inline blocks and body modifiers, each drawing a scalar",
        },
        Fixture {
            stem: "rocq_unique",
            expected: Rejected(Codegen("P002")),
            why: "a nested `unique` block, which has no encoding in the assertion language, in \
                  any mode",
        },
        Fixture {
            stem: "rocq_unique_spec",
            expected: StockValid(AlreadyChoiceLowered),
            why: "the corpus producer of the `ValidUniqueSpec` grammar; its only \
                  non-deterministic body is the `unique` one, already choice-lowered",
        },
        Fixture {
            stem: "spec_adopted_both",
            expected: StockValid(ScalarChoice),
            why: "a `forall` body drawing two scalars, applying both an own function and a \
                  linked external; the program half of the specification-adoption pair",
        },
        Fixture {
            stem: "spec_adopted_extern",
            expected: StockValid(NothingToLower),
            why: "the program that declares no `spec` at all, so everything its proof artifact \
                  states was adopted from the library it links",
        },
        Fixture {
            stem: "spec_adopted_extern_mathlib",
            expected: StockValid(ScalarChoice),
            why: "the library whose own universal obligation a link adopts; its `forall` body \
                  draws a scalar",
        },
        Fixture {
            stem: "spec_adopted_reach_mathlib",
            expected: StockValid(ScalarChoice),
            why: "the same library shipping a reachability obligation as well; the `exists` body \
                  was already choice-lowered, so the universal one's draw is the strongest part",
        },
        Fixture {
            stem: "spec_aggregate_values",
            expected: StockValid(AggregateLeaves),
            why: "compound `@` at array, matrix and record type — one choice parameter per \
                  scalar leaf",
        },
        Fixture {
            stem: "spec_assume_body_modifier",
            expected: Rejected(Codegen("P001")),
            why: "an `assume` function body states no property, so there is no obligation to \
                  prove",
        },
        Fixture {
            stem: "spec_bitwise_arith",
            expected: StockValid(ScalarChoice),
            why: "the bitwise and shift operator family over drawn scalars",
        },
        Fixture {
            stem: "spec_bounded_iteration",
            expected: StockValid(AggregateLeaves),
            why: "iteration over a drawn record and a drawn row, both leaf-expanded",
        },
        Fixture {
            stem: "spec_bounds_realization",
            expected: StockValid(ScalarChoice),
            why: "the only fixture whose executable bodies carry a bounds guard; its \
                  specification bodies draw scalar indices",
        },
        Fixture {
            stem: "spec_calls_top",
            expected: StockValid(WrapperSuppression),
            why: "a `forall` body whose only content is a call to a top-level helper; it draws \
                  nothing",
        },
        Fixture {
            stem: "spec_linked_extern",
            expected: StockValid(ScalarChoice),
            why: "a `forall` body drawing a scalar and applying a linked external; see the \
                  module documentation on why this gate compiles it unlinked",
        },
        Fixture {
            stem: "spec_linked_extern_mathlib",
            expected: StockValid(NothingToLower),
            why: "the external behind `spec_linked_extern`: one executable function, no `spec`",
        },
        Fixture {
            stem: "spec_linked_toolchain",
            expected: StockValid(ScalarChoice),
            why: "`forall` bodies drawing scalars and applying externals from a foreign \
                  toolchain's artifact",
        },
        Fixture {
            stem: "spec_literal_ctx",
            expected: StockValid(ScalarChoice),
            why: "literal typing contexts inside specification bodies, over a drawn scalar",
        },
        Fixture {
            stem: "spec_method",
            expected: StockValid(NothingToLower),
            why: "a `spec` whose only members are two plain struct methods; no non-determinism \
                  at all",
        },
        Fixture {
            stem: "spec_method_nondet",
            expected: StockValid(ScalarChoice),
            why: "a specification *method* drawing a scalar, one arm of it returning a compound \
                  so the choice sits behind both a result pointer and a receiver",
        },
        Fixture {
            stem: "spec_mixed_kinds",
            expected: StockValid(ScalarChoice),
            why: "all three obligation kinds in one module; the universal ones draw scalars",
        },
        Fixture {
            stem: "spec_narrow_abi",
            expected: StockValid(ScalarChoice),
            why: "the narrow-width ABI survey, one specification function per width",
        },
        Fixture {
            stem: "spec_narrow_discharge",
            expected: StockValid(ScalarChoice),
            why: "the narrow obligations that must be dischargeable, over drawn scalars",
        },
        Fixture {
            stem: "spec_narrow_uzumaki",
            expected: StockValid(AggregateLeaves),
            why: "the declared-value-domain table; its struct and array rows leaf-expand, and \
                  each leaf keeps the store-width round-trip that carries its domain",
        },
        Fixture {
            stem: "spec_negative_consts",
            expected: StockValid(ScalarChoice),
            why: "negative integer constants at every width, compared against drawn scalars",
        },
        Fixture {
            stem: "spec_nondet_blocks",
            expected: StockValid(ScalarChoice),
            why: "inline `forall`/`exists`/`assume` blocks inside plain bodies, each drawing a \
                  scalar",
        },
        Fixture {
            stem: "spec_nondet_body_modifiers",
            expected: StockValid(ScalarChoice),
            why: "a `forall` body and an `exists` body side by side, each drawing a scalar",
        },
        Fixture {
            stem: "spec_operator_matrix",
            expected: StockValid(ScalarChoice),
            why: "every arithmetic and comparison operator the obligation printer can spell, \
                  over drawn scalars",
        },
        Fixture {
            stem: "spec_quantifier_alternation",
            expected: StockValid(AggregateLeaves),
            why: "quantifier alternation at every nesting the language admits, including a \
                  compound `@`",
        },
        Fixture {
            stem: "spec_short_circuit",
            expected: StockValid(ScalarChoice),
            why: "short-circuit operands in assertion position, over drawn scalars",
        },
        Fixture {
            stem: "test_parse_source_file_1",
            expected: Rejected(Parse),
            why: "a parser fixture written against a grammar this parser does not accept",
        },
        Fixture {
            stem: "test_parse_source_file_2",
            expected: Rejected(Parse),
            why: "a parser fixture written against a grammar this parser does not accept",
        },
        Fixture {
            stem: "test_parse_source_file_3",
            expected: Rejected(Parse),
            why: "a parser fixture written against a grammar this parser does not accept",
        },
        Fixture {
            stem: "three_specs",
            expected: StockValid(WrapperSuppression),
            why: "three specs of mixed shapes; the one `forall` body draws nothing",
        },
        Fixture {
            stem: "trivial",
            expected: StockValid(NothingToLower),
            why: "a single executable function and no `spec` at all",
        },
        Fixture {
            stem: "with_spec",
            expected: StockValid(ScalarChoice),
            why: "the smallest universal specification that draws: one `forall` body, one \
                  scalar `@`",
        },
    ];

    /// How far a fixture's proof-mode compilation got.
    enum Outcome {
        ParseFailed,
        TypeCheckFailed,
        /// The rule ids of every reported analysis error.
        AnalysisFailed(Vec<&'static str>),
        /// The rendered code generation error.
        CodegenFailed(String),
        Module(Box<CodegenOutput>),
    }

    /// Runs the proof-mode pipeline over a fixture, reporting the stage it
    /// stopped at rather than panicking, so a stage change is a table mismatch
    /// with a name instead of an unattributed panic.
    ///
    /// The optimization level is the target's own, which is what proof mode uses
    /// regardless of build profile — so this gate measures the artifact a proof
    /// build writes, not a differently optimized one.
    fn compile_proof_mode(stem: &str) -> Outcome {
        let path = get_test_data_path().join("inf").join(format!("{stem}.inf"));
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let Ok(arena) = try_build_ast(source) else {
            return Outcome::ParseFailed;
        };
        let Ok(built) = TypeCheckerBuilder::build_typed_context(arena) else {
            return Outcome::TypeCheckFailed;
        };
        let typed_context = built.typed_context();
        let target = Target::Wasm32;
        let options = CodegenOptions {
            target,
            mode: CompilationMode::Proof,
            opt_level: target.default_opt_level(),
            ..Default::default()
        };
        if let Err(errors) = inference_analysis::analyze_with_options(
            &typed_context,
            inference_analysis::AnalysisOptions {
                stack_budget_bytes: options.layout.stack_size(),
            },
        ) {
            return Outcome::AnalysisFailed(errors.errors().iter().map(|d| d.rule_id()).collect());
        }
        match inference_wasm_codegen::codegen(&typed_context, stem, options) {
            Ok(output) => Outcome::Module(Box::new(output)),
            Err(e) => Outcome::CodegenFailed(e.to_string()),
        }
    }

    /// The stems of every `.inf` in the corpus directory, sorted.
    fn corpus_directory_stems() -> Vec<String> {
        let dir = get_test_data_path().join("inf");
        let mut stems: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("inf"))
            .map(|path| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .expect("fixture file name is UTF-8")
                    .to_string()
            })
            .collect();
        stems.sort();
        stems
    }

    /// Every fixture in the directory carries a verdict, and every verdict names
    /// a fixture that exists.
    ///
    /// Without this the table would be a subset gate: a new fixture that emitted
    /// a custom opcode would ship green because nothing looked at it.
    #[test]
    fn every_fixture_in_the_corpus_directory_is_listed() {
        let on_disk = corpus_directory_stems();
        let mut listed: Vec<String> = CORPUS.iter().map(|f| f.stem.to_string()).collect();
        listed.sort();
        for stem in [
            "rocq_prime_bounded_example",
            "rocq_false_certificate",
            "spec_narrow_discharge",
        ] {
            assert_eq!(
                CORPUS.iter().filter(|fixture| fixture.stem == stem).count(),
                1,
                "selected producer `{stem}` must occur exactly once in the stock-validity table"
            );
        }
        let unlisted: Vec<&String> = on_disk.iter().filter(|s| !listed.contains(s)).collect();
        let missing: Vec<&String> = listed.iter().filter(|s| !on_disk.contains(s)).collect();
        assert!(
            unlisted.is_empty(),
            "these fixtures carry no expected verdict, so nothing gates them: {unlisted:?}"
        );
        assert!(
            missing.is_empty(),
            "these fixtures are listed but no longer exist: {missing:?}"
        );
    }

    /// Each fixture reaches exactly the verdict the table declares, and every
    /// module a stock WebAssembly decoder accepts.
    ///
    /// Failures are accumulated rather than raised at the first mismatch: the
    /// table's value is the whole column, and one fixture regressing must not
    /// hide the twenty behind it.
    #[test]
    fn every_fixture_reaches_its_expected_verdict() {
        let mut failures: Vec<String> = Vec::new();
        for fixture in CORPUS {
            let stem = fixture.stem;
            let outcome = compile_proof_mode(stem);
            match (fixture.expected, &outcome) {
                (StockValid(_), Outcome::Module(output)) => {
                    if let Err(e) = wasmparser::Validator::new().validate_all(output.wasm()) {
                        failures.push(format!(
                            "{stem}: a stock decoder rejected the proof-mode module: {e}"
                        ));
                    }
                }
                (Rejected(Parse), Outcome::ParseFailed)
                | (Rejected(TypeCheck), Outcome::TypeCheckFailed) => {}
                (Rejected(Analysis(rule)), Outcome::AnalysisFailed(reported)) => {
                    if reported.iter().any(|id| *id != rule) {
                        failures.push(format!(
                            "{stem}: expected every analysis error to be {rule}, got {reported:?}"
                        ));
                    }
                }
                (Rejected(Codegen(code)), Outcome::CodegenFailed(message)) => {
                    if !message.contains(&format!("error[{code}]")) {
                        failures.push(format!(
                            "{stem}: expected a {code} diagnostic, got: {message}"
                        ));
                    }
                }
                (expected, actual) => failures.push(format!(
                    "{stem}: expected {expected:?} ({why}), but the pipeline {actual}",
                    why = fixture.why
                )),
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} fixtures missed their expected verdict:\n  {}",
            failures.len(),
            CORPUS.len(),
            failures.join("\n  ")
        );
    }

    /// No body of a function the `inference.spec_funcs` section indexes carries
    /// a `0xfc`-prefixed custom opcode.
    ///
    /// This is narrower than validation and says something validation does not.
    /// Validation is a property of the module: it would also go green if a
    /// specification function stopped being registered, or if the fixture
    /// stopped carrying non-determinism at all. This resolves the section the
    /// Rocq translator actually reads, walks to each indexed body, and looks at
    /// its bytes — so it names the specification and the function index a leaked
    /// opcode belongs to. The section is decoded from the emitted bytes rather
    /// than read out of `CodegenOutput`, and the two are cross-checked, because
    /// downstream sees only the bytes.
    #[test]
    fn no_specification_function_body_carries_a_custom_opcode() {
        let mut failures: Vec<String> = Vec::new();
        for fixture in CORPUS {
            let StockValid(lowering) = fixture.expected else {
                continue;
            };
            let Outcome::Module(output) = compile_proof_mode(fixture.stem) else {
                // The verdict test reports this; here it would be noise.
                continue;
            };
            failures.extend(spec_body_violations(fixture.stem, lowering, &output));
        }
        assert!(
            failures.is_empty(),
            "a specification body must lower to ordinary WebAssembly:\n  {}",
            failures.join("\n  ")
        );
    }

    /// Every complaint about `stem`'s specification bodies: a section that does
    /// not match the map code generation reported, an index with no body, a body
    /// a stock decoder cannot read, a custom opcode inside one — or no body at
    /// all where the table says there is one.
    ///
    /// That last one is what keeps the check from going quietly vacuous. Every
    /// assertion below is over the bodies the section names, so a fixture whose
    /// specification functions stopped being registered would have nothing
    /// looked at and pass. Only a `NothingToLower` entry is allowed to contribute
    /// no body.
    fn spec_body_violations(stem: &str, lowering: Lowering, output: &CodegenOutput) -> Vec<String> {
        let wasm = output.wasm();
        let mut failures = Vec::new();

        let section = decode_spec_funcs_section(wasm);
        let mut from_section: Vec<(String, Vec<u32>)> = section.unwrap_or_default();
        from_section.sort();
        let mut from_output: Vec<(String, Vec<u32>)> = output
            .spec_func_indices_by_spec()
            .iter()
            .map(|(name, indices)| (name.clone(), indices.clone()))
            .collect();
        from_output.sort();
        if from_section != from_output {
            failures.push(format!(
                "{stem}: the emitted `{SPEC_FUNCS_SECTION_NAME}` section says {from_section:?} \
                 but code generation reported {from_output:?}"
            ));
        }

        let bodies = defined_function_bodies(wasm);
        let imported = imported_function_count(wasm);
        let mut inspected = 0_usize;
        for (spec, indices) in &from_section {
            for &index in indices {
                let Some(defined) = index.checked_sub(imported) else {
                    failures.push(format!(
                        "{stem}: spec `{spec}` indexes function {index}, which is an import"
                    ));
                    continue;
                };
                let Some(body) = bodies.get(defined as usize) else {
                    failures.push(format!(
                        "{stem}: spec `{spec}` indexes function {index}, which has no body"
                    ));
                    continue;
                };
                inspected += 1;
                failures.extend(custom_opcodes_in(stem, spec, index, &wasm[body.clone()]));
            }
        }
        if inspected == 0 && lowering != Lowering::NothingToLower {
            failures.push(format!(
                "{stem}: the table says this fixture needs {lowering:?}, but its \
                 `{SPEC_FUNCS_SECTION_NAME}` section names no function body, so nothing here \
                 was checked"
            ));
        }
        failures
    }

    /// The custom opcodes present in one function body.
    ///
    /// Two readings of the same bytes, because neither alone says it well. The
    /// decode is exact — a stock operator reader has no `0xfc 0x31` to decode
    /// into, so a body that reads to its end contains none — but its error names
    /// an offset rather than an opcode. The scan names the opcode. A scan can in
    /// principle match inside an immediate rather than at an opcode boundary, so
    /// it is the message and the decode is the verdict: a scan hit on a body
    /// that decodes cleanly is reported as the immediate it must be.
    fn custom_opcodes_in(stem: &str, spec: &str, index: u32, body: &[u8]) -> Vec<String> {
        let scanned: Vec<String> = CUSTOM_OPCODES
            .iter()
            .filter(|op| body.windows(2).any(|w| w == [0xfc, **op]))
            .map(|op| format!("0xfc {op:#04x}"))
            .collect();
        let decoded = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(body, 0))
            .get_operators_reader()
            .and_then(|mut reader| {
                while !reader.eof() {
                    reader.read()?;
                }
                Ok(())
            });
        match decoded {
            Ok(()) => Vec::new(),
            Err(e) => vec![format!(
                "{stem}: spec `{spec}` function {index} does not decode under a stock reader \
                 ({e}); custom opcodes present: {}",
                if scanned.is_empty() {
                    "none found by scan".to_string()
                } else {
                    scanned.join(", ")
                }
            )],
        }
    }

    /// Decodes the `inference.spec_funcs` custom section, or `None` when the
    /// module carries no such section.
    ///
    /// The payload is `version`, `count`, then `count` pairs of a length-prefixed
    /// UTF-8 name and a counted list of function indices, all LEB128. Reading it
    /// here rather than borrowing the linker's decoder keeps this gate over the
    /// same bytes a downstream consumer sees, with no crate in between.
    fn decode_spec_funcs_section(wasm: &[u8]) -> Option<Vec<(String, Vec<u32>)>> {
        let payload = wasmparser::Parser::new(0)
            .parse_all(wasm)
            .filter_map(Result::ok)
            .find_map(|payload| match payload {
                wasmparser::Payload::CustomSection(section)
                    if section.name() == SPEC_FUNCS_SECTION_NAME =>
                {
                    Some(section.data().to_vec())
                }
                _ => None,
            })?;
        let mut reader = wasmparser::BinaryReader::new(&payload, 0);
        let version = reader.read_var_u32().expect("section version");
        assert_eq!(
            version, SPEC_FUNCS_SECTION_VERSION,
            "`{SPEC_FUNCS_SECTION_NAME}` version"
        );
        let count = reader.read_var_u32().expect("section pair count");
        let mut pairs = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let name = reader.read_string().expect("spec name").to_string();
            let index_count = reader.read_var_u32().expect("index count");
            let indices = (0..index_count)
                .map(|_| reader.read_var_u32().expect("function index"))
                .collect();
            pairs.push((name, indices));
        }
        Some(pairs)
    }

    /// The byte range of each defined function's body, in code-section order.
    fn defined_function_bodies(wasm: &[u8]) -> Vec<std::ops::Range<usize>> {
        wasmparser::Parser::new(0)
            .parse_all(wasm)
            .filter_map(Result::ok)
            .filter_map(|payload| match payload {
                wasmparser::Payload::CodeSectionEntry(body) => Some(body.range()),
                _ => None,
            })
            .collect()
    }

    /// How many function indices the import section consumes, which is the
    /// offset between a function index and its code-section position.
    ///
    /// All three import encodings are counted. This compiler's encoder writes
    /// only the first, but reading the module rather than assuming what wrote it
    /// is what lets this walk stay correct if that ever changes.
    fn imported_function_count(wasm: &[u8]) -> u32 {
        let mut count = 0;
        for payload in wasmparser::Parser::new(0).parse_all(wasm).flatten() {
            let wasmparser::Payload::ImportSection(section) = payload else {
                continue;
            };
            for group in section.into_iter().flatten() {
                let functions = match group {
                    wasmparser::Imports::Single(_, import) => {
                        usize::from(matches!(import.ty, wasmparser::TypeRef::Func(_)))
                    }
                    wasmparser::Imports::Compact1 { items, .. } => items
                        .into_iter()
                        .flatten()
                        .filter(|item| matches!(item.ty, wasmparser::TypeRef::Func(_)))
                        .count(),
                    wasmparser::Imports::Compact2 { ty, names, .. } => {
                        if matches!(ty, wasmparser::TypeRef::Func(_)) {
                            names.into_iter().flatten().count()
                        } else {
                            0
                        }
                    }
                };
                count += u32::try_from(functions).expect("import count fits in u32");
            }
        }
        count
    }

    impl std::fmt::Display for Outcome {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::ParseFailed => f.write_str("stopped at parsing"),
                Self::TypeCheckFailed => f.write_str("stopped at type checking"),
                Self::AnalysisFailed(rules) => write!(f, "stopped at analysis with {rules:?}"),
                Self::CodegenFailed(message) => {
                    write!(f, "stopped at code generation with: {message}")
                }
                Self::Module(_) => f.write_str("produced a module"),
            }
        }
    }
}

//! No program the front end accepts may abort code generation.
//!
//! Where a construct the parser and type checker admitted had no lowering, the
//! only tool at the code generation arm was `todo!()` — a process abort on a
//! program the user had just been told was valid. `return;` in a function that
//! returns nothing was one of them. The repair gives every such construct either
//! a real lowering or a diagnostic, and this sweep is what keeps the next one
//! from shipping: it runs the whole pipeline over every fixture in the
//! repository inside `catch_unwind` and reports an abort as a named failure.
//!
//! ## Why a per-fixture table rather than a count
//!
//! The same reason [`crate::stock_validity`] carries one. A pass count stays
//! green when one fixture starts compiling and another stops, and it stays green
//! when a fixture reaches no code generation at all. So the shape fixtures below
//! declare the stage each stops at, and
//! [`gate::the_corpus_reaches_code_generation`] refuses a corpus fixture that
//! quietly regressed to an earlier stage — a sweep asserting only "did not
//! panic" over a program the type checker now rejects is looking at nothing.
//!
//! ## Why both compilation modes
//!
//! Compile mode never runs choice lowering or the obligation translation, and
//! those hold their own accounting of what a declared parameter costs. An
//! unnamed parameter spends a frame slot the choice suffix has to begin after,
//! and the compiler asserts the two agree; a compile-only sweep would ship a
//! disagreement green, because the assertion sits on a path compile mode does
//! not walk. Every fixture is therefore run twice.
//!
//! ## Why the fixtures are enumerated from disk
//!
//! Two of the three sources are directory walks rather than lists, so a fixture
//! added anywhere in the repository joins this sweep without anyone remembering
//! to add it. The third source is a hand-written table, because its fixtures
//! exist to name specific constructs and a table is what says which construct is
//! missing; [`gate::every_panic_free_fixture_is_listed`] closes it in both
//! directions so a file cannot escape the table by being added beside it.
//!
//! The three sources, 215 fixtures and 430 compilations between them:
//!
//! - `tests/test_data/inf/` — the language corpus, every `.inf` in the
//!   directory: 49.
//! - `tests/test_data/codegen/wasm/` — the canonical paired golden fixtures,
//!   selected by the rule that a fixture's file stem equals its parent directory
//!   name: 147. That rule admits both paired layouts, the one with a module
//!   directory above the fixture directory and the flat one without, and the 22
//!   files it excludes are exactly the multi-file project trees under `src/`,
//!   whose `use` clauses need a project driver this in-process pipeline does not
//!   have.
//! - `tests/test_data/panic_free/` — new here: one minimal program per construct
//!   the repair touched, 19 of them, each a single offence so that the stage it
//!   stops at is attributable to the construct it is named for.
//!
//! ## The single-offence constraint
//!
//! An analysis row declares a *set* of rule ids and both directions are checked:
//! every reported id must be declared, and every declared id must have been
//! reported. That makes a fixture tripping a second, unrelated rule a failure
//! rather than a silent pass, which is why the two constant fixtures declare
//! their `const` inside a function body — a module-scope constant is separately
//! rejected for being top level, and the row would be reporting two rules while
//! naming one.

#[cfg(test)]
mod gate {
    use crate::utils::{get_test_data_path, panic_message, try_build_ast};
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CodegenOptions, CompilationMode, Target};
    use std::path::{Path, PathBuf};

    /// How far one compilation got.
    ///
    /// [`Outcome::Panicked`] is the verdict no row may declare and no fixture
    /// may reach. It is what a `todo!()`, an `unreachable!`, a failed `expect`
    /// or a tripped `assert!` anywhere in the pipeline looks like from outside,
    /// and collapsing all four into one observable is the point: the sweep does
    /// not care which of them it was, only that the process would have died on
    /// a program a user was told is valid.
    enum Outcome {
        ParseFailed,
        TypeCheckFailed,
        /// The rule ids of every reported analysis error.
        AnalysisFailed(Vec<&'static str>),
        /// The rendered code generation diagnostic.
        CodegenFailed(String),
        /// A module was produced. The bytes are not kept: this sweep is about
        /// reaching the end of the pipeline, and the goldens gate what comes out
        /// of it.
        Module,
        /// Some phase aborted, carrying the panic payload.
        Panicked(String),
    }

    /// The verdict a shape fixture must reach in compile mode.
    ///
    /// There is deliberately no code-generation verdict. Every construct in
    /// [`SHAPES`] that cannot be lowered is owned by an analysis rule, and
    /// analysis runs first, so no fixture here can reach a code generation
    /// refusal — the backstop behind each rule is pinned by the negative codegen
    /// tests, which skip analysis in order to get to it. A variant for a stage no
    /// row can reach would read as coverage this sweep does not have.
    #[derive(Debug, Clone, Copy)]
    enum Declared {
        /// The construct has a lowering: the pipeline runs to a module.
        Module,
        /// The type checker refuses it.
        TypeCheck,
        /// Analysis refuses it, reporting exactly these rule ids and no others.
        Analysis(&'static [&'static str]),
    }

    /// One hand-listed shape fixture: the `.inf` stem under
    /// `tests/test_data/panic_free/`, the verdict it must reach, and the
    /// sentence that says why that verdict is the right one for the construct
    /// it is named for.
    struct Shape {
        stem: &'static str,
        declared: Declared,
        why: &'static str,
    }

    use Declared::{Analysis, Module, TypeCheck};

    /// Every `.inf` under `tests/test_data/panic_free/`.
    ///
    /// The first six constructs are the ones that gained a lowering, so they run
    /// to a module; the rest gained a diagnostic, and each names the rule that
    /// owns it. A construct in the second group is refused by analysis rather
    /// than by code generation because analysis runs first — the code generation
    /// backstop behind each of them is pinned separately, by the negative
    /// codegen tests that skip analysis to reach it.
    const SHAPES: &[Shape] = &[
        Shape {
            stem: "bare_type_parameter",
            declared: Analysis(&["A050"]),
            why: "a parameter declared by its type alone binds no name, so the body cannot read \
                  it and a call site cannot label it",
        },
        Shape {
            stem: "generic_type_in_expression",
            declared: TypeCheck,
            why: "a generic name in expression position is the one producer of a type node where \
                  a value belongs; generics are not implemented (#320)",
        },
        Shape {
            stem: "ignored_parameter",
            declared: Module,
            why: "`_: T` is a supported spelling: the parameter occupies its ABI slot and binds \
                  no name",
        },
        Shape {
            stem: "ignored_parameter_in_reachability_spec",
            declared: Module,
            why: "the unnamed parameter spends slot 0 and the reachability body's choice suffix \
                  begins after it, which is the alignment the frame plan asserts",
        },
        Shape {
            stem: "local_type_alias",
            declared: Module,
            why: "an alias is nominal and introduces no value, so the statement contributes no \
                  instruction",
        },
        Shape {
            stem: "string_array_element",
            declared: Analysis(&["A048"]),
            why: "a string has no layout in linear memory, so an array of them has no element \
                  size",
        },
        Shape {
            stem: "string_function_const",
            declared: Analysis(&["A048"]),
            why: "the constant scope where a string type is not also rejected for being top \
                  level",
        },
        Shape {
            stem: "string_literal",
            declared: Analysis(&["A048"]),
            why: "a string literal has no value representation, and the binding it initializes \
                  carries the same type",
        },
        Shape {
            stem: "string_parameter",
            declared: Analysis(&["A048"]),
            why: "there is no WebAssembly type to pass a string in",
        },
        Shape {
            stem: "string_struct_field",
            declared: Analysis(&["A048"]),
            why: "a record has to place every field, and a string field has nothing to place",
        },
        Shape {
            stem: "uninitialized_binding",
            declared: Analysis(&["A025"]),
            why: "a declaration with no initializer reaches code generation with no value to \
                  store",
        },
        Shape {
            stem: "unit_array_element",
            declared: Analysis(&["A049"]),
            why: "a unit value occupies no bytes, so an array of them has no element size",
        },
        Shape {
            stem: "unit_binding",
            declared: Analysis(&["A049"]),
            why: "a unit-typed binding has nothing to store",
        },
        Shape {
            stem: "unit_expression_statement",
            declared: Module,
            why: "a bare `();` is one of the two positions the parser really produces a unit \
                  literal in, and it emits neither a value nor a drop",
        },
        Shape {
            stem: "unit_function_const",
            declared: Analysis(&["A049"]),
            why: "the constant scope where a unit type is not also rejected for being top level",
        },
        Shape {
            stem: "unit_parameter",
            declared: Analysis(&["A049"]),
            why: "a parameter declared unit is given no argument slot",
        },
        Shape {
            stem: "unit_return_type_spelled_unit",
            declared: Module,
            why: "`unit` is a builtin type name where `()` is a simple type kind, and both \
                  declare the empty result list",
        },
        Shape {
            stem: "unit_struct_field",
            declared: Analysis(&["A049"]),
            why: "a unit field occupies no bytes in the record",
        },
        Shape {
            stem: "void_return",
            declared: Module,
            why: "the other position the parser produces a unit literal in: the value occupies \
                  no operand stack slot, so `return;` lowers to the epilogue alone",
        },
    ];

    /// The corpus fixtures that legitimately stop before code generation, and
    /// why.
    ///
    /// This is the anti-vacuity list. Everything not on it must reach
    /// [`Outcome::CodegenFailed`] or [`Outcome::Module`] in compile mode,
    /// because a fixture that stops at parsing or type checking exercises none
    /// of the lowering this sweep exists to protect, and would go on passing
    /// [`no_fixture_panics_the_compiler`] forever while covering nothing. The
    /// list is checked in both directions: an entry whose fixture starts
    /// reaching code generation is a stale reason, and stale reasons are how a
    /// list like this decays into a mute button.
    ///
    /// The shape fixtures are deliberately absent. Each of them declares its
    /// stage exactly in [`SHAPES`], which is a stronger statement than
    /// membership in an allowlist, and repeating them here would be two places
    /// to keep in agreement.
    ///
    /// Every golden fixture below is one whose own test compiles it with
    /// analysis skipped, which is how a lowering for a construct only a
    /// specification may contain gets a golden at all. The reason names the rule
    /// that refuses it here, so an entry stops being true the moment that rule's
    /// judgment about the fixture changes.
    const STOPS_BEFORE_CODEGEN: &[(&str, &str)] = &[
        (
            "inf::bad_syntax",
            "it is the deliberately malformed file, the parser-error fixture",
        ),
        (
            "inf::example",
            "it is a language tour rather than a program: its `use` clauses name source files, \
             which need a project context this single-file pipeline does not have, and it \
             redeclares names on purpose",
        ),
        (
            "inf::nondet_blocks",
            "it is the negative fixture for non-deterministic blocks in executable code, which \
             A042 exists to refuse",
        ),
        (
            "inf::nondet_body_modifiers",
            "it is the same negative with the construct written as a function-body modifier",
        ),
        (
            "inf::test_parse_source_file_1",
            "it is a parser fixture written against a grammar this parser does not accept",
        ),
        (
            "inf::test_parse_source_file_2",
            "it is a parser fixture written against a grammar this parser does not accept",
        ),
        (
            "inf::test_parse_source_file_3",
            "it is a parser fixture written against a grammar this parser does not accept",
        ),
        (
            "codegen::algo_converge",
            "its convergence loop draws inside a `forall`, which A042 refuses in executable code",
        ),
        (
            "codegen::base::array_nondet",
            "it is the array half of the non-deterministic lowering goldens: A042 refuses its \
             blocks and A023 refuses the draw assigned into an existing array element",
        ),
        (
            "codegen::base::assign_nondet",
            "it draws into an already-bound name, which A023 refuses, inside a block A042 refuses",
        ),
        (
            "codegen::base::const_in_forall",
            "it declares constants inside `forall` blocks, which A042 refuses in executable code",
        ),
        (
            "codegen::base::enum_uzumaki_domain",
            "it is the enum draw-domain golden, whose blocks A042 refuses in executable code",
        ),
        (
            "codegen::base::i64_uzumaki",
            "it is a bare `return @;`, which A006 refuses outside a non-deterministic block",
        ),
        (
            "codegen::base::if_nondet",
            "it wraps `if` in non-deterministic blocks, which A042 refuses in executable code",
        ),
        (
            "codegen::base::local_variables",
            "the local-variable survey ends with two drawn bindings, which A006 refuses outside a \
             non-deterministic block",
        ),
        (
            "codegen::base::multidim_array_uzumaki",
            "it draws multi-dimensional arrays inside blocks A042 refuses in executable code",
        ),
        (
            "codegen::base::narrow_uzumaki",
            "it is the narrow-width draw survey, whose blocks A042 refuses in executable code",
        ),
        (
            "codegen::base::nondet",
            "it is the non-deterministic block survey itself, refused by A042 for the blocks and \
             A006 for a draw outside one",
        ),
        (
            "codegen::base::struct_array_field_nondet",
            "it draws into a struct's array field inside blocks A042 refuses in executable code",
        ),
        (
            "codegen::base::struct_nondet",
            "it draws whole structs inside blocks A042 refuses in executable code",
        ),
        (
            "codegen::base::u32_uzumaki",
            "it draws an unsigned value inside a block A042 refuses in executable code",
        ),
        (
            "codegen::loops::loop_in_nondet",
            "it nests loops inside non-deterministic blocks, which A042 refuses in executable code",
        ),
        (
            "codegen::loops::nondet_then_break",
            "it follows a non-deterministic block with `break`, and A042 refuses the block in \
             executable code",
        ),
    ];

    /// A fixture to run: the name failures report it under, and the file to
    /// read.
    struct Case {
        name: String,
        path: PathBuf,
    }

    /// Runs the whole pipeline over `source` and reports the stage it stopped
    /// at, or the payload it aborted with.
    ///
    /// The guard is around every phase, not around code generation alone,
    /// because the property is about the compiler and not about one crate of it:
    /// a `todo!()` newly reachable in the type checker would abort a build just
    /// as visibly.
    ///
    /// The pipeline runs on the reserved compiler stack for the same reason
    /// [`crate::robustness::deep_syntax`] does: the phases recurse once per
    /// level of a fixture's nesting, and the test harness gives a thread far
    /// less stack than they are built for, so a deep fixture would overflow on
    /// the harness rather than reach the behaviour under test. A stack overflow
    /// is not catchable, so it would abort the whole test binary rather than
    /// report a fixture. [`inference::with_compiler_stack`] re-raises a worker
    /// panic on this thread, which is what leaves the guard below still able to
    /// see one.
    fn compile(source: &str, name: &str, mode: CompilationMode) -> Outcome {
        let guarded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inference::with_compiler_stack(|| run_pipeline(source, name, mode))
        }));
        match guarded {
            Ok(outcome) => outcome,
            Err(payload) => Outcome::Panicked(panic_message(&*payload)),
        }
    }

    /// Parse, type check, analyze, generate — the single-file pipeline every
    /// other in-process suite drives, reporting the stage it stopped at.
    ///
    /// Analysis measures the program against the artifact these options
    /// describe, which is the pairing a real build makes: the stack budget a
    /// rule clears a call chain against is the stack code generation is about to
    /// emit.
    fn run_pipeline(source: &str, name: &str, mode: CompilationMode) -> Outcome {
        let Ok(arena) = try_build_ast(source.to_string()) else {
            return Outcome::ParseFailed;
        };
        let Ok(built) = TypeCheckerBuilder::build_typed_context(arena) else {
            return Outcome::TypeCheckFailed;
        };
        let typed_context = built.typed_context();
        let target = Target::Wasm32;
        let options = CodegenOptions {
            target,
            mode,
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
        match inference_wasm_codegen::codegen(&typed_context, name, options) {
            Ok(_) => Outcome::Module,
            Err(error) => Outcome::CodegenFailed(error.to_string()),
        }
    }

    /// The `.inf` files directly under `dir`, as `(stem, path)` pairs sorted by
    /// stem.
    fn inf_files_in(dir: &Path) -> Vec<(String, PathBuf)> {
        let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("inf"))
            .map(|path| {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .expect("fixture file name is UTF-8")
                    .to_string();
                (stem, path)
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }

    /// The language corpus: every `.inf` under `tests/test_data/inf/`.
    fn language_corpus() -> Vec<Case> {
        inf_files_in(&get_test_data_path().join("inf"))
            .into_iter()
            .map(|(stem, path)| Case {
                name: format!("inf::{stem}"),
                path,
            })
            .collect()
    }

    /// The canonical paired golden fixtures under `tests/test_data/codegen/wasm/`.
    ///
    /// Selected by the rule that the file stem equals its parent directory name,
    /// which is what the paired layout means: a fixture directory named for the
    /// test, holding the source and the artifacts it is compared against. The
    /// rule admits the flat layout too, where the fixture directory sits
    /// directly under `wasm/` because its test module carries the fixture's own
    /// name. What it excludes is the multi-file project trees, whose sources sit
    /// under a `src/` directory and whose `use` clauses need a project driver
    /// this pipeline does not have.
    fn golden_corpus() -> Vec<Case> {
        let root = get_test_data_path().join("codegen").join("wasm");
        let mut cases = Vec::new();
        collect_paired_fixtures(&root, &root, &mut cases);
        cases.sort_by(|a, b| a.name.cmp(&b.name));
        cases
    }

    /// Walks `dir` for paired fixtures, naming each by its path below `root`.
    fn collect_paired_fixtures(root: &Path, dir: &Path, cases: &mut Vec<Case>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry").path());
        for path in entries {
            if path.is_dir() {
                collect_paired_fixtures(root, &path, cases);
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("inf") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str());
            let parent = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str());
            if stem.is_none() || stem != parent {
                continue;
            }
            let relative = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .expect("a paired fixture sits below the golden root");
            let name: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            cases.push(Case {
                name: format!("codegen::{}", name.join("::")),
                path,
            });
        }
    }

    /// The hand-listed shape fixtures under `tests/test_data/panic_free/`.
    fn shape_corpus() -> Vec<Case> {
        inf_files_in(&get_test_data_path().join("panic_free"))
            .into_iter()
            .map(|(stem, path)| Case {
                name: format!("panic_free::{stem}"),
                path,
            })
            .collect()
    }

    /// Reads a fixture, failing loudly rather than reporting a stage it never
    /// reached.
    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// No fixture anywhere in the repository aborts the compiler, in either
    /// compilation mode.
    ///
    /// This is the gate the issue's acceptance criterion names. It asserts
    /// nothing about *which* verdict a fixture reaches — the other three gates
    /// do that — only that reaching one is what happened, rather than the
    /// process dying with a stock panic message on a program the front end had
    /// already accepted.
    ///
    /// Failures accumulate. A new abort in a shared lowering path shows up on
    /// dozens of fixtures at once, and the list of which ones is how a reader
    /// tells a shared path from a construct-specific one.
    #[test]
    fn no_fixture_panics_the_compiler() {
        let mut cases = language_corpus();
        cases.extend(golden_corpus());
        cases.extend(shape_corpus());
        assert!(
            cases.len() > 200,
            "the corpus walk found only {} fixtures, so it is looking in the wrong place",
            cases.len()
        );

        let mut failures: Vec<String> = Vec::new();
        for case in &cases {
            let source = read(&case.path);
            for mode in [CompilationMode::Compile, CompilationMode::Proof] {
                if let Outcome::Panicked(payload) = compile(&source, &case.name, mode) {
                    failures.push(format!(
                        "{name} in {mode:?} mode: {payload}",
                        name = case.name
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} fixtures aborted the compiler instead of reaching a verdict:\n  {}",
            failures.len(),
            cases.len() * 2,
            failures.join("\n  ")
        );
    }

    /// Each shape fixture reaches exactly the verdict its row declares.
    ///
    /// Compile mode, because that is the mode the declared stages describe: a
    /// proof build adds obligation translation whose own refusals would put a
    /// second stage column in this table with nothing to say about the
    /// constructs it is written for.
    #[test]
    fn every_panic_free_shape_reaches_its_declared_verdict() {
        let mut failures: Vec<String> = Vec::new();
        for shape in SHAPES {
            let path = get_test_data_path()
                .join("panic_free")
                .join(format!("{stem}.inf", stem = shape.stem));
            let source = read(&path);
            let outcome = compile(&source, shape.stem, CompilationMode::Compile);
            match (shape.declared, &outcome) {
                (Module, Outcome::Module) | (TypeCheck, Outcome::TypeCheckFailed) => {}
                (Analysis(declared), Outcome::AnalysisFailed(reported)) => {
                    let undeclared: Vec<&&str> =
                        reported.iter().filter(|id| !declared.contains(id)).collect();
                    let unreported: Vec<&&str> = declared
                        .iter()
                        .filter(|id| !reported.contains(id))
                        .collect();
                    if !undeclared.is_empty() || !unreported.is_empty() {
                        failures.push(format!(
                            "{stem}: declared {declared:?} but analysis reported {reported:?} \
                             (undeclared: {undeclared:?}, never reported: {unreported:?})",
                            stem = shape.stem
                        ));
                    }
                }
                (declared, actual) => failures.push(format!(
                    "{stem}: declared {declared:?} ({why}), but the pipeline {actual}",
                    stem = shape.stem,
                    why = shape.why
                )),
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} shape fixtures missed their declared verdict:\n  {}",
            failures.len(),
            SHAPES.len(),
            failures.join("\n  ")
        );
    }

    /// Every shape fixture on disk carries a row, and every row names a fixture
    /// that exists.
    ///
    /// Without this the table would be a subset gate. A new fixture added beside
    /// the others would run in [`no_fixture_panics_the_compiler`], where the
    /// only thing asserted is that it did not abort, and would never have to say
    /// which construct it covers or where it is supposed to stop.
    #[test]
    fn every_panic_free_fixture_is_listed() {
        let on_disk: Vec<String> = shape_corpus()
            .iter()
            .map(|case| {
                case.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .expect("fixture file name is UTF-8")
                    .to_string()
            })
            .collect();
        let listed: Vec<&str> = SHAPES.iter().map(|shape| shape.stem).collect();

        let unlisted: Vec<&String> = on_disk
            .iter()
            .filter(|stem| !listed.contains(&stem.as_str()))
            .collect();
        let missing: Vec<&&str> = listed
            .iter()
            .filter(|stem| !on_disk.iter().any(|s| s == *stem))
            .collect();
        assert!(
            unlisted.is_empty(),
            "these shape fixtures declare no verdict, so nothing says where they stop: {unlisted:?}"
        );
        assert!(
            missing.is_empty(),
            "these rows name shape fixtures that no longer exist: {missing:?}"
        );
    }

    /// Every corpus fixture reaches code generation, except the ones
    /// [`STOPS_BEFORE_CODEGEN`] names and says why.
    ///
    /// This is what keeps [`no_fixture_panics_the_compiler`] from going quietly
    /// vacuous. A fixture that regressed to a parse or type error still passes
    /// that gate — it did not abort, because it never got near the lowering that
    /// could — and would go on doing so while covering none of the code it was
    /// written to cover. So the two disk-enumerated corpora have to say, fixture
    /// by fixture, that they still arrive.
    ///
    /// The list is checked in both directions. An entry that starts reaching
    /// code generation is reported as a stale reason rather than passed over,
    /// because a reason nobody has to keep true is a reason nobody reads.
    #[test]
    fn the_corpus_reaches_code_generation() {
        let mut cases = language_corpus();
        cases.extend(golden_corpus());

        let mut failures: Vec<String> = Vec::new();
        let mut allowed_but_arrived: Vec<String> = Vec::new();
        for case in &cases {
            let source = read(&case.path);
            let outcome = compile(&source, &case.name, CompilationMode::Compile);
            let arrived = matches!(outcome, Outcome::Module | Outcome::CodegenFailed(_));
            let allowance = STOPS_BEFORE_CODEGEN
                .iter()
                .find(|(name, _)| *name == case.name);
            match (allowance, arrived) {
                (None, true) | (Some(_), false) => {}
                (None, false) => failures.push(format!(
                    "{name}: {outcome}, so it exercises no code generation and is not listed as \
                     a fixture that stops earlier",
                    name = case.name
                )),
                (Some((_, reason)), true) => allowed_but_arrived.push(format!(
                    "{name}: listed as stopping earlier because {reason}, but it reached code \
                     generation",
                    name = case.name
                )),
            }
        }
        let unknown: Vec<&(&str, &str)> = STOPS_BEFORE_CODEGEN
            .iter()
            .filter(|(name, _)| !cases.iter().any(|case| case.name == *name))
            .collect();

        assert!(
            failures.is_empty() && allowed_but_arrived.is_empty() && unknown.is_empty(),
            "the corpus must reach code generation, or say why not:\n  {}\n  {}\n  {}",
            failures.join("\n  "),
            allowed_but_arrived.join("\n  "),
            unknown
                .iter()
                .map(|(name, _)| format!("{name}: listed but no such fixture exists"))
                .collect::<Vec<_>>()
                .join("\n  ")
        );
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
                Self::Module => f.write_str("produced a module"),
                Self::Panicked(payload) => write!(f, "aborted the compiler with: {payload}"),
            }
        }
    }
}

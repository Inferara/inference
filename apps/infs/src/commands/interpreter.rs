//! Executing a `spacewasm` build in process under the `SpaceWasm` flight
//! interpreter, the route `infs run` takes for that target.
//!
//! The artifact is loaded through `inference-spacewasm-runner`, against the F´
//! (F Prime) reference hosts of `spacewasm_std`, upstream's reference embedder,
//! and at that embedder's configuration: its two verifier bounds, its IR code
//! pages and its 1,024 words of value stack ([`EngineConfig::REFERENCE`]). The
//! runner checks every import against the reference hosts before it decodes a
//! byte, so a program importing anything else, or a reference host at another
//! signature, is refused with nothing executed.
//!
//! What a run prints follows the `wasmtime` route where the two can agree: the
//! build log, then `Invoking '<entry>' with the SpaceWasm interpreter (…)...`,
//! then the value the entry returned as the last line on stdout, and nothing for
//! a function that returns nothing. The hosts log each call to stderr as it is
//! made, in the style of the reference embedder's lines. A refused load, a
//! refused argument, a trap and an exhausted `--fuel` budget are errors, which
//! `infs` reports and exits with status 1; a call that returns exits 0, whatever
//! it returned.
//!
//! Every sentence here that the runner can state, it states: the import lines,
//! the reference table, a trap's first line and its explanation, the over-limit
//! facts, why a conformant module inside both verifier bounds did not load, the
//! argument refusals, what a function takes and the core of the out-of-fuel
//! sentence are the runner's, with `` `infs run` `` passed where a sentence names
//! the program embedding it. What is composed here is the framing only `infs`
//! knows — the artifact's path, the `--fuel` flag, project mode, and the remedy.
//!
//! The whole run is synchronous and happens under one [`Session`], which is
//! acquired and released inside [`run`]: nothing awaits while it is held.

use std::num::NonZeroUsize;
use std::path::Path;

use anyhow::{Result, anyhow};
use inference_spacewasm_runner::fprime::{self, LOWERING_NOTE, reference_table_lines};
use inference_spacewasm_runner::{
    ArgumentError, EngineConfig, ExportedFunction, Fuel, HostLog, ImportProblem, InvokeError,
    LIMITS_FROM, Limit, LoadError, Outcome, OverLimit, REFERENCE_MAX_CONTROL_FRAMES,
    REFERENCE_MAX_STACK_DEPTH, Session, UnsupportedImports, code_pages_exhausted,
    coerce_arguments, conformance_gap, out_of_fuel, render, type_name,
};

/// How the runner's sentences name the program embedding it.
const EMBEDDER: &str = "`infs run`";

/// One run of a built artifact under the interpreter.
pub(crate) struct Invocation<'a> {
    /// The artifact's bytes.
    pub(crate) wasm: &'a [u8],
    /// The artifact as the messages name it: `out/main.wasm` in project mode,
    /// `out/<stem>.wasm` in single-file mode.
    pub(crate) shown_as: &'a Path,
    /// The export to invoke.
    pub(crate) entry_point: &'a str,
    /// What the invoked function is given.
    pub(crate) arguments: Arguments<'a>,
    /// The instruction budget of the whole run, from `--fuel`.
    pub(crate) fuel: Option<NonZeroUsize>,
}

/// What the invoked function is given, which the two modes answer differently.
#[derive(Clone, Copy)]
pub(crate) enum Arguments<'a> {
    /// Single-file mode: the trailing arguments, as written, read as the
    /// function's parameters.
    Given(&'a [String]),
    /// Project mode, which passes nothing. `entry_file` is the source file a
    /// refusal of a function that takes arguments tells the reader to run
    /// instead.
    Project {
        /// The project's entry file, as the reader would type it.
        entry_file: &'a Path,
    },
}

/// Loads `invocation.wasm` under the interpreter, invokes its entry point, and
/// prints what it returned.
///
/// # Errors
///
/// A refusal to load the module — an import the reference hosts do not provide
/// as declared, a module over the reference configuration, any other decoder
/// verdict — or to call the entry point — an export it does not have, an
/// argument its parameters cannot take, a function taking arguments in project
/// mode — and every way the call can end other than returning: a trap, and an
/// exhausted `--fuel` budget. Each is worded for the reader, and nothing is
/// executed before any refusal of the first two kinds.
pub(crate) fn run(invocation: &Invocation<'_>) -> Result<()> {
    let fuel = match invocation.fuel {
        Some(budget) => Fuel::Limited(budget),
        None => Fuel::Unbounded,
    };
    let entry = invocation.entry_point;
    let mut session = Session::acquire();
    let mut module = fprime::load(
        &mut session,
        invocation.wasm,
        HostLog::stderr(),
        fuel,
        EngineConfig::REFERENCE,
    )
    .map_err(|error| load_refusal(error, invocation.shown_as))?;
    let function = module
        .module()
        .function(entry)
        .map_err(|error| invoke_refusal(error, invocation.shown_as))?;
    let args = match invocation.arguments {
        Arguments::Given(raw) => {
            coerce_arguments(&function, raw).map_err(|error| argument_refusal(&error, raw))?
        }
        Arguments::Project { entry_file } => {
            if !function.params.is_empty() {
                return Err(anyhow!(main_takes_arguments_in_project_mode(
                    &function, entry_file
                )));
            }
            Vec::new()
        }
    };

    println!("{}", announcement(entry, invocation.fuel));
    match module
        .invoke(entry, &args)
        .map_err(|error| invoke_refusal(error, invocation.shown_as))?
    {
        Outcome::Returned(value) => {
            if let Some(value) = value {
                println!("{}", render(value));
            }
            Ok(())
        }
        Outcome::Trapped(reason) => {
            let report = module.trap_report(entry, reason);
            let mut message = report.headline();
            if let Some(explanation) = report.explanation(EMBEDDER) {
                message.push('\n');
                message.push_str(&explanation);
            }
            Err(anyhow!(message))
        }
        Outcome::OutOfFuel { budget } => Err(anyhow!(out_of_fuel_refusal(
            entry,
            budget,
            module.log()
        ))),
    }
}

/// The line printed before the call: which function, under which interpreter
/// release, and the budget when there is one.
fn announcement(entry: &str, fuel: Option<NonZeroUsize>) -> String {
    let budget = fuel.map(|budget| format!(", fuel {budget}")).unwrap_or_default();
    format!("Invoking '{entry}' with the SpaceWasm interpreter ({LIMITS_FROM}{budget})...")
}

/// The refusal of a function that takes arguments in project mode, which
/// passes none: what it takes, in the runner's words, and the single-file
/// command that passes them, with one placeholder per parameter —
/// `` `main` takes 1 argument (i32), and project mode passes none. Run the entry
/// file with its arguments after the path: `infs run src/main.inf <i32>`. ``
///
/// Both routes refuse such a `main` in these words: this one reads its
/// parameters off the loaded module, and the `wasmtime` route off the artifact.
pub(crate) fn main_takes_arguments_in_project_mode(
    main: &ExportedFunction,
    entry_file: &Path,
) -> String {
    let placeholders: Vec<String> = main
        .params
        .iter()
        .map(|ty| format!("<{}>", type_name(*ty)))
        .collect();
    format!(
        "{}, and project mode passes none. Run the entry file with its arguments after the path: \
         `infs run {} {}`.",
        main.arity_clause(),
        entry_file.display(),
        placeholders.join(" ")
    )
}

/// The error a load that did not succeed is reported as.
///
/// Every variant is named, so a refusal the runner adds has to be worded here
/// before `infs` compiles.
fn load_refusal(error: LoadError, shown_as: &Path) -> anyhow::Error {
    let artifact = shown_as.display();
    match error {
        LoadError::UnsupportedImports(unsupported) => {
            anyhow!(unsupported_imports(&unsupported, shown_as))
        }
        LoadError::OverLimit(over) => anyhow!(over_limit(&over, shown_as)),
        LoadError::CodePagesExhausted { pages, verdict } => anyhow!(
            "`infs run` cannot load {artifact}: {}.",
            code_pages_exhausted(pages, &verdict, EMBEDDER)
        ),
        LoadError::ConformanceGap(verdict) => {
            anyhow!("`infs run` cannot load {artifact}: {}.", conformance_gap(&verdict))
        }
        error @ (LoadError::Hosts(_)
        | LoadError::Decode(_)
        | LoadError::Resource { .. }
        | LoadError::StartTrapped(_)
        | LoadError::StartOutOfFuel { .. }
        | LoadError::StartPaused
        | LoadError::StartRefused { .. }) => {
            anyhow!("`infs run` cannot load {artifact}: {error}.")
        }
    }
}

/// The refusal of a module whose imports the reference hosts do not provide as
/// it declares them: each import and what is wrong with it, the note on
/// WebAssembly signatures when one is shown, the reference table when a name is
/// unknown, and the remedy.
fn unsupported_imports(unsupported: &UnsupportedImports, shown_as: &Path) -> String {
    let imports = unsupported.imports();
    let only_functions = imports
        .iter()
        .all(|import| !matches!(import.problem, ImportProblem::NotAFunction { .. }));
    let noun = match (imports.len(), only_functions) {
        (1, true) => "function",
        (_, true) => "functions",
        (1, false) => "import",
        (_, false) => "imports",
    };
    let mut lines = vec![format!(
        "`infs run` cannot execute this program: {} imports {} {noun} that `infs run` does not \
         provide as declared.",
        shown_as.display(),
        imports.len()
    )];
    lines.extend(unsupported.import_lines(EMBEDDER));
    if unsupported.shows_signatures() {
        lines.push(LOWERING_NOTE.to_string());
    }
    if unsupported.names_an_unknown_function() {
        lines.push(
            "A `spacewasm` build runs with the F Prime reference hosts of spacewasm_std, the \
             reference embedder in NASA's spacewasm repository, and with no others:"
                .to_string(),
        );
        lines.extend(reference_table_lines());
    }
    let remedy = if imports.len() == 1 {
        "Change the declaration to match, or run the program from an embedder that supplies it."
    } else {
        "Change each declaration to match, or run the program from an embedder that supplies \
         these functions."
    };
    lines.push(format!(
        "Nothing was executed. {remedy} See the book's Compilation Targets chapter (\"Running a \
         SpaceWasm build\")."
    ));
    lines.join("\n")
}

/// The refusal of a conformant module that needs more of one verifier stack
/// than the reference configuration `infs run` loads every module at.
fn over_limit(over: &OverLimit, shown_as: &Path) -> String {
    let remedy = match over.limit {
        Limit::ControlFrames => "flatten the nesting in the functions that warning names",
        Limit::OperandStack => "split the expressions in the functions that warning names",
    };
    format!(
        "`infs run` cannot load {}: its {} that `infs run` gives the SpaceWasm interpreter.\n\
         `infs run` loads every module at the spacewasm_std reference configuration \
         ({REFERENCE_MAX_CONTROL_FRAMES} control frames, {REFERENCE_MAX_STACK_DEPTH} \
         operand-stack values, {} IR code pages), and the build warned above that this module \
         needs more. The module is conformant, and an embedder built with {} >= {} loads it; to \
         run it here, {remedy}.",
        shown_as.display(),
        over.fact(),
        EngineConfig::REFERENCE.max_code_pages,
        over.limit.const_generic(),
        over.needed,
    )
}

/// The error a call that could not be made is reported as.
fn invoke_refusal(error: InvokeError, shown_as: &Path) -> anyhow::Error {
    let artifact = shown_as.display();
    match error {
        InvokeError::NoSuchExport { export, exports } => {
            let exported = if exports.is_empty() {
                "It exports no function.".to_string()
            } else {
                let names: Vec<String> = exports.iter().map(|name| format!("`{name}`")).collect();
                format!("It exports: {}.", names.join(", "))
            };
            anyhow!(
                "{artifact} exports no function named `{export}`. {exported} A function is \
                 exported when it is declared `pub` at the top level of the entry file."
            )
        }
        InvokeError::NotAFunction { export, kind } => anyhow!(
            "`{export}` is exported by {artifact}, but it is {}, not a function.",
            kind.description()
        ),
        error @ (InvokeError::HostReexport { .. }
        | InvokeError::Arity { .. }
        | InvokeError::Argument { .. }
        | InvokeError::Busy { .. }
        | InvokeError::Paused { .. }
        | InvokeError::NoResult { .. }) => anyhow!("{error}."),
    }
}

/// The error an argument the function's parameters cannot take is reported
/// as: the runner's clause closed as a sentence, with a pointer at the option
/// it most likely was when `raw` carries one — except for a floating-point
/// parameter, which no argument fills wherever the options stand.
fn argument_refusal(error: &ArgumentError, raw: &[String]) -> anyhow::Error {
    let message = format!("{}.", error.clause(EMBEDDER));
    let function = match error {
        ArgumentError::FloatingPoint { .. } => return anyhow!(message),
        ArgumentError::Count { function, .. }
        | ArgumentError::NotAnInteger { function, .. }
        | ArgumentError::OutOfRange { function, .. } => function,
    };
    match (raw.first(), raw.iter().find(|text| looks_like_an_option(text))) {
        (Some(first), Some(option)) => anyhow!(
            "{message} Options go before the first argument: everything from `{first}` on is \
             passed to `{function}` unparsed, so move `{option}` before it."
        ),
        _ => anyhow!(message),
    }
}

/// Whether an argument is spelled like an option rather than like a number: a
/// dash and something after it that is not a decimal integer, as `--fuel` and
/// `-L` are and `-5` is not.
fn looks_like_an_option(text: &str) -> bool {
    text.len() > 1 && text.starts_with('-') && text.parse::<i128>().is_err()
}

/// The error a call that ran out of fuel is reported as, saying that the host
/// calls it logged were made when it logged any.
fn out_of_fuel_refusal(entry: &str, budget: usize, log: &HostLog) -> String {
    let logged = if log.line_count() > 0 {
        " The host calls logged above had already been made."
    } else {
        ""
    };
    format!(
        "{} (`--fuel {budget}`) before it returned. Either it never returns or it needs a larger \
         budget: raise `--fuel`, or leave it out to run without one.{logged}",
        out_of_fuel(entry, budget)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_spacewasm_runner::{ExportKind, ValType};
    use wasm_encoder::{
        BlockType, CodeSection, EntityType, ExportSection, Function, FunctionSection,
        ImportSection, Instruction, MemorySection, MemoryType, Module, TypeSection,
        ValType as Wasm,
    };

    /// One function a test module defines: its export name, its signature, the
    /// `i32` locals it declares, and its body, `end` included.
    struct Defined<'a> {
        name: &'a str,
        params: &'a [Wasm],
        results: &'a [Wasm],
        locals: u32,
        body: Vec<Instruction<'a>>,
    }

    /// A module importing `imports` — each `(module, field, params, results)` —
    /// and defining `functions`, each exported under its name, with a one-page
    /// memory exported as `memory`.
    fn module(imports: &[(&str, &str, &[Wasm], &[Wasm])], functions: &[Defined<'_>]) -> Vec<u8> {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        let mut import_section = ImportSection::new();
        let mut funcs = FunctionSection::new();
        let mut exports = ExportSection::new();
        let mut code = CodeSection::new();
        let mut index = 0_u32;
        for &(module_name, field, params, results) in imports {
            types.ty().function(params.iter().copied(), results.iter().copied());
            import_section.import(module_name, field, EntityType::Function(index));
            index += 1;
        }
        for function in functions {
            types
                .ty()
                .function(function.params.iter().copied(), function.results.iter().copied());
            funcs.function(index);
            exports.export(function.name, wasm_encoder::ExportKind::Func, index);
            let mut body = Function::new([(function.locals, Wasm::I32)]);
            for instruction in &function.body {
                body.instruction(instruction);
            }
            code.function(&body);
            index += 1;
        }
        exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
        module.section(&types);
        if !imports.is_empty() {
            module.section(&import_section);
        }
        module.section(&funcs);
        let mut memories = MemorySection::new();
        memories.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memories);
        module.section(&exports);
        module.section(&code);
        module.finish()
    }

    /// A function of no parameters and one `i32` result running `body`.
    fn main_running(body: Vec<Instruction<'static>>) -> Defined<'static> {
        Defined { name: "main", params: &[], results: &[Wasm::I32], locals: 0, body }
    }

    /// `pub fn add(a: i32, b: i32) -> i32`, compiled.
    fn add() -> Defined<'static> {
        Defined {
            name: "add",
            params: &[Wasm::I32, Wasm::I32],
            results: &[Wasm::I32],
            locals: 0,
            body: vec![
                Instruction::LocalGet(0),
                Instruction::LocalGet(1),
                Instruction::I32Add,
                Instruction::End,
            ],
        }
    }

    /// A body counting a local up to a million, then returning it: long
    /// enough that any small budget runs out, and finite, so a run that is
    /// given no budget still ends.
    fn counting_loop() -> Vec<Instruction<'static>> {
        vec![
            Instruction::Loop(BlockType::Empty),
            Instruction::LocalGet(0),
            Instruction::I32Const(1),
            Instruction::I32Add,
            Instruction::LocalTee(0),
            Instruction::I32Const(1_000_000),
            Instruction::I32LtS,
            Instruction::BrIf(0),
            Instruction::End,
            Instruction::LocalGet(0),
            Instruction::End,
        ]
    }

    /// Runs `wasm`'s `entry` with `arguments` under `fuel`, named as project
    /// mode names its artifact.
    fn run_module(
        wasm: &[u8],
        entry: &str,
        arguments: Arguments<'_>,
        fuel: Option<usize>,
    ) -> Result<()> {
        let shown_as = Path::new("out").join("main.wasm");
        run(&Invocation {
            wasm,
            shown_as: &shown_as,
            entry_point: entry,
            arguments,
            fuel: fuel.and_then(NonZeroUsize::new),
        })
    }

    /// The refusal a run of `wasm`'s `entry` ends in.
    fn refusal(wasm: &[u8], entry: &str, arguments: Arguments<'_>, fuel: Option<usize>) -> String {
        run_module(wasm, entry, arguments, fuel)
            .expect_err("the run is refused")
            .to_string()
    }

    /// Single-file arguments, as written.
    fn given(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    /// `out/main.wasm`, as the refusals name it.
    fn artifact() -> String {
        Path::new("out").join("main.wasm").display().to_string()
    }

    /// The announcement names the entry and the interpreter release, and the
    /// budget when there is one.
    ///
    /// Fails if the release is spelled here instead of read from the limits the
    /// runner was built against, or if a budget is dropped from the line.
    #[test]
    fn the_announcement_names_the_entry_the_release_and_the_budget() {
        assert_eq!(
            announcement("main", None),
            format!("Invoking 'main' with the SpaceWasm interpreter ({LIMITS_FROM})...")
        );
        assert_eq!(
            announcement("add", NonZeroUsize::new(1000)),
            format!("Invoking 'add' with the SpaceWasm interpreter ({LIMITS_FROM}, fuel 1000)...")
        );
        assert_eq!(LIMITS_FROM, "spacewasm 0.7.1", "the release the texts above were read at");
    }

    /// An argument spelled like an option is a dash and something that is not
    /// a decimal integer.
    #[test]
    fn an_option_is_told_apart_from_a_negative_number() {
        for text in ["--fuel", "-L", "--", "-x1", "--entry-point"] {
            assert!(looks_like_an_option(text), "{text}");
        }
        for text in ["-5", "-0", "-", "5", "fuel", "-2147483648", ""] {
            assert!(!looks_like_an_option(text), "{text}");
        }
    }

    /// Each refused argument is reported in the runner's words closed as a
    /// sentence, with the pointer at the option it most likely was when one of
    /// the arguments is spelled like an option, and a floating-point parameter
    /// in the runner's clause with `infs run` named as the embedder.
    ///
    /// Fails if a refusal loses its full stop, if the pointer names the wrong
    /// token or fires on a negative number, or if the float refusal names the
    /// runner.
    #[test]
    fn a_refused_argument_is_worded_for_the_command_line() {
        let add = || "add".to_string();
        let pair = vec![ValType::I32, ValType::I32];
        let count = ArgumentError::Count {
            function: add(),
            params: pair.clone(),
            given: given(&["2", "40", "--fuel", "5"]),
        };
        assert_eq!(
            argument_refusal(&count, &given(&["2", "40", "--fuel", "5"])).to_string(),
            "`add` takes 2 arguments (i32, i32), and 4 were given: 2 40 --fuel 5. Options go \
             before the first argument: everything from `2` on is passed to `add` unparsed, so \
             move `--fuel` before it."
        );
        let count = ArgumentError::Count {
            function: add(),
            params: pair.clone(),
            given: given(&["2", "-40", "7"]),
        };
        assert_eq!(
            argument_refusal(&count, &given(&["2", "-40", "7"])).to_string(),
            "`add` takes 2 arguments (i32, i32), and 3 were given: 2 -40 7."
        );
        let not_an_integer = ArgumentError::NotAnInteger {
            function: add(),
            position: 2,
            params: pair.clone(),
            text: "--fuel".to_string(),
        };
        assert_eq!(
            argument_refusal(&not_an_integer, &given(&["2", "--fuel"])).to_string(),
            "argument 2 for `add` is `--fuel`, which is not a decimal integer; `add` takes (i32, \
             i32). Options go before the first argument: everything from `2` on is passed to \
             `add` unparsed, so move `--fuel` before it."
        );
        let not_an_integer = ArgumentError::NotAnInteger {
            function: add(),
            position: 2,
            params: pair,
            text: "4x".to_string(),
        };
        assert_eq!(
            argument_refusal(&not_an_integer, &given(&["2", "4x"])).to_string(),
            "argument 2 for `add` is `4x`, which is not a decimal integer; `add` takes (i32, i32)."
        );
        let out_of_range = ArgumentError::OutOfRange {
            function: add(),
            position: 1,
            ty: ValType::I32,
            text: "5000000000".to_string(),
        };
        assert_eq!(
            argument_refusal(&out_of_range, &given(&["5000000000", "1"])).to_string(),
            "argument 1 for `add` is 5000000000, which does not fit an i32 (-2147483648 to \
             4294967295; a value above 2147483647 is taken as its unsigned bit pattern)."
        );
        let float = ArgumentError::FloatingPoint {
            function: "f".to_string(),
            position: 1,
            ty: ValType::F32,
        };
        assert_eq!(
            argument_refusal(&float, &given(&["1"])).to_string(),
            "`f` takes an f32 argument, which `infs run` cannot pass."
        );
    }

    /// A call that cannot be made names the artifact and what the name is
    /// instead of a function.
    #[test]
    fn a_call_that_cannot_be_made_names_the_artifact() {
        let shown_as = Path::new("out").join("main.wasm");
        let refused = |error| invoke_refusal(error, &shown_as).to_string();
        assert_eq!(
            refused(InvokeError::NoSuchExport {
                export: "helper".to_string(),
                exports: vec!["main".to_string(), "add".to_string()],
            }),
            format!(
                "{} exports no function named `helper`. It exports: `main`, `add`. A function is \
                 exported when it is declared `pub` at the top level of the entry file.",
                artifact()
            )
        );
        assert_eq!(
            refused(InvokeError::NoSuchExport { export: "main".to_string(), exports: Vec::new() }),
            format!(
                "{} exports no function named `main`. It exports no function. A function is \
                 exported when it is declared `pub` at the top level of the entry file.",
                artifact()
            )
        );
        assert_eq!(
            refused(InvokeError::NotAFunction {
                export: "memory".to_string(),
                kind: ExportKind::Memory,
            }),
            format!(
                "`memory` is exported by {}, but it is the module's linear memory, not a function.",
                artifact()
            )
        );
        assert_eq!(
            refused(InvokeError::Busy { export: "main".to_string() }),
            "`main` could not be invoked: the interpreter is still running an earlier call."
        );
    }

    /// A name the module does not export as a function is refused through the
    /// runner's own resolution, before anything runs, both when it is nothing
    /// and when it is the memory.
    #[test]
    fn an_entry_that_is_no_function_is_refused_before_anything_runs() {
        let wasm = module(&[], &[add()]);
        let arguments = Arguments::Given(&[]);
        assert_eq!(
            refusal(&wasm, "helper", arguments, None),
            format!(
                "{} exports no function named `helper`. It exports: `add`. A function is exported \
                 when it is declared `pub` at the top level of the entry file.",
                artifact()
            )
        );
        assert_eq!(
            refusal(&wasm, "memory", arguments, None),
            format!(
                "`memory` is exported by {}, but it is the module's linear memory, not a function.",
                artifact()
            )
        );
        run_module(&wasm, "add", Arguments::Given(&given(&["2", "40"])), None)
            .expect("`add 2 40` returns");
    }

    /// Single-file arguments are read against the compiled parameters, and
    /// project mode refuses a function that takes any with the command that
    /// passes them.
    ///
    /// Fails if a count mismatch reaches the interpreter, or if project mode
    /// calls a function that takes arguments with none.
    #[test]
    fn arguments_are_read_against_the_compiled_parameters() {
        let wasm = module(&[], &[add()]);
        assert_eq!(
            refusal(&wasm, "add", Arguments::Given(&given(&["2", "40", "7"])), None),
            "`add` takes 2 arguments (i32, i32), and 3 were given: 2 40 7."
        );
        let entry_file = Path::new("src").join("main.inf");
        assert_eq!(
            refusal(&wasm, "add", Arguments::Project { entry_file: &entry_file }, None),
            format!(
                "`add` takes 2 arguments (i32, i32), and project mode passes none. Run the entry \
                 file with its arguments after the path: `infs run {} <i32> <i32>`.",
                entry_file.display()
            )
        );
        let unit = Defined {
            name: "main",
            params: &[],
            results: &[],
            locals: 0,
            body: vec![Instruction::End],
        };
        let project = Arguments::Project { entry_file: &entry_file };
        run_module(&module(&[], &[unit]), "main", project, None)
            .expect("a `main` taking nothing runs in project mode");
    }

    /// A trap is reported by its first line and its explanation, the
    /// explanation naming `infs run` where it names the embedder.
    #[test]
    fn a_trap_is_reported_with_its_explanation() {
        let wasm = module(&[], &[main_running(vec![Instruction::Unreachable, Instruction::End])]);
        assert_eq!(
            refusal(&wasm, "main", Arguments::Given(&[]), None),
            "`main` trapped: a runtime check failed (Unreachable).\nInference compiles each of its \
             runtime checks to a WebAssembly `unreachable`, so the interpreter cannot say which \
             one failed: an arithmetic overflow (`+`, `-`, `*` or unary `-` outside \
             `wrapping(...)`), an array index out of bounds, `MIN / -1` at `i8` or `i16`, a failed \
             `assert`, or an out-of-range enum value passed to an exported function."
        );
    }

    /// The interpreter runs with the reference embedder's 1,024 words of value
    /// stack: a frame of 2,000 words does not fit it, and the trap says how many
    /// words `infs run` gave.
    ///
    /// Fails if the load is given any other stack — the test tier's 65,536
    /// words, which the frame fits, among them.
    #[test]
    fn the_interpreter_is_given_the_reference_stack() {
        let wide = Defined {
            name: "main",
            params: &[],
            results: &[Wasm::I32],
            locals: 2000,
            body: vec![Instruction::LocalGet(1999), Instruction::End],
        };
        assert_eq!(
            refusal(&module(&[], &[wide]), "main", Arguments::Given(&[]), None),
            "`main` trapped: the interpreter's call stack is full (StackOverflow).\n`infs run` \
             gives the interpreter 1024 words of call stack, as spacewasm_std does, and this call \
             chain's frames need more. Recursion is refused at compile time (A035), so a deep \
             chain of large frames is the cause."
        );
    }

    /// A call that runs out of fuel names the flag that set the budget, and
    /// says the host calls it logged were made only when it logged any.
    ///
    /// Fails if the budget is not forwarded to the load, if the flag is not
    /// named, or if the host-call clause appears without a host call or goes
    /// missing after one.
    #[test]
    fn running_out_of_fuel_names_the_budget_and_the_host_calls_made() {
        let quiet = module(&[], &[Defined { locals: 1, ..main_running(counting_loop()) }]);
        let core = "`main` ran out of fuel: the SpaceWasm interpreter stopped it after 100 \
                    interpreter instructions (`--fuel 100`) before it returned. Either it never \
                    returns or it needs a larger budget: raise `--fuel`, or leave it out to run \
                    without one.";
        assert_eq!(refusal(&quiet, "main", Arguments::Given(&[]), Some(100)), core);
        run_module(&quiet, "main", Arguments::Given(&[]), None)
            .expect("with no budget the loop runs to its end");

        let mut body = vec![Instruction::I64Const(1), Instruction::Call(0)];
        body.extend(counting_loop());
        let logging = module(
            &[("fprime_core", "rsleep", &[Wasm::I64], &[])],
            &[Defined { locals: 1, ..main_running(body) }],
        );
        assert_eq!(
            refusal(&logging, "main", Arguments::Given(&[]), Some(100)),
            format!("{core} The host calls logged above had already been made.")
        );
    }

    /// Imports the reference hosts do not provide as declared are refused
    /// before anything is decoded or run, each with what is wrong, the note on
    /// WebAssembly signatures when a signature is shown, the reference table
    /// when a name is unknown, and the remedy in the grammar of the count.
    #[test]
    fn unsupported_imports_are_refused_before_anything_runs() {
        let wasm = module(
            &[
                ("fprime_core", "message", &[Wasm::I32], &[]),
                ("fprime_core", "beep", &[], &[]),
            ],
            &[main_running(vec![Instruction::I32Const(0), Instruction::End])],
        );
        let table = fprime::reference_table_lines().join("\n");
        assert_eq!(
            refusal(&wasm, "main", Arguments::Given(&[]), None),
            format!(
                "`infs run` cannot execute this program: {} imports 2 functions that `infs run` \
                 does not provide as declared.\n  fprime_core.beep\n    not a host function \
                 `infs run` provides\n  fprime_core.message\n    this program declares  (i32)\n    \
                 the host provides      (ptr: i32, len: i32)\n    a declaration that matches: \
                 external fn message(text: [u8; N], len: i32);   (N: your buffer's length)\n\
                 {LOWERING_NOTE}\nA `spacewasm` build runs with the F Prime reference hosts of \
                 spacewasm_std, the reference embedder in NASA's spacewasm repository, and with no \
                 others:\n{table}\nNothing was executed. Change each declaration to match, or run \
                 the program from an embedder that supplies these functions. See the book's \
                 Compilation Targets chapter (\"Running a SpaceWasm build\").",
                artifact()
            )
        );

        let mismatch = module(
            &[("env", "clock_ms", &[], &[Wasm::I32])],
            &[main_running(vec![Instruction::I32Const(0), Instruction::End])],
        );
        let text = refusal(&mismatch, "main", Arguments::Given(&[]), None);
        assert!(
            text.starts_with(&format!(
                "`infs run` cannot execute this program: {} imports 1 function that `infs run` \
                 does not provide as declared.\n  env.clock_ms\n    this program declares  () -> \
                 i32\n",
                artifact()
            )),
            "got: {text}"
        );
        assert!(text.contains(LOWERING_NOTE), "a signature is shown, so the note is owed: {text}");
        assert!(
            !text.contains("and with no others"),
            "no name is unknown, so the table is not owed: {text}"
        );
        assert!(
            text.ends_with(
                "Nothing was executed. Change the declaration to match, or run the program from \
                 an embedder that supplies it. See the book's Compilation Targets chapter \
                 (\"Running a SpaceWasm build\")."
            ),
            "got: {text}"
        );
    }

    /// A conformant module nesting deeper than the reference embedder's 64
    /// control frames is refused naming the nesting, the function, the
    /// reference configuration and the const generic an embedder would raise.
    #[test]
    fn a_module_over_the_reference_configuration_is_refused_with_its_measurement() {
        let mut body = vec![Instruction::Block(BlockType::Empty); 70];
        body.extend(vec![Instruction::End; 70]);
        body.extend([Instruction::I32Const(0), Instruction::End]);
        let wasm = module(&[], &[main_running(body)]);
        assert_eq!(
            refusal(&wasm, "main", Arguments::Given(&[]), None),
            format!(
                "`infs run` cannot load {}: its control nesting of 71 frames (in `func[0]`) \
                 exceeds the 64 that `infs run` gives the SpaceWasm interpreter.\n`infs run` \
                 loads every module at the spacewasm_std reference configuration (64 control \
                 frames, 256 operand-stack values, 256 IR code pages), and the build warned above \
                 that this module needs more. The module is conformant, and an embedder built \
                 with MAX_CONTROL_FRAMES >= 71 loads it; to run it here, flatten the nesting in \
                 the functions that warning names.",
                artifact()
            )
        );
    }

    /// A conformant module whose operand stack peaks past the reference
    /// embedder's 256 values is refused naming the peak, the function, the
    /// reference configuration, the const generic an embedder would raise,
    /// and the remedy that fits a tall stack rather than deep nesting.
    ///
    /// Fails if the operand-stack refusal borrows the nesting's words or
    /// remedy, or names the other const generic.
    #[test]
    fn a_module_over_the_reference_operand_stack_is_refused_with_its_measurement() {
        let mut body = vec![Instruction::I32Const(0); 300];
        body.extend(vec![Instruction::Drop; 300]);
        body.extend([Instruction::I32Const(0), Instruction::End]);
        let wasm = module(&[], &[main_running(body)]);
        assert_eq!(
            refusal(&wasm, "main", Arguments::Given(&[]), None),
            format!(
                "`infs run` cannot load {}: its tallest operand stack of 300 values (in \
                 `func[0]`) exceeds the 256 that `infs run` gives the SpaceWasm interpreter.\n\
                 `infs run` loads every module at the spacewasm_std reference configuration (64 \
                 control frames, 256 operand-stack values, 256 IR code pages), and the build \
                 warned above that this module needs more. The module is conformant, and an \
                 embedder built with MAX_STACK_DEPTH >= 300 loads it; to run it here, split the \
                 expressions in the functions that warning names.",
                artifact()
            )
        );
    }

    /// The refusal of a `main` that takes arguments says what it takes, in the
    /// runner's clause, and the single-file command with one placeholder per
    /// parameter.
    ///
    /// Fails if a placeholder is dropped or reordered, or if the clause stops
    /// counting the compiled parameters.
    #[test]
    fn a_main_taking_arguments_is_refused_with_the_command_that_passes_them() {
        use inference_spacewasm_runner::ValType::{I32, I64};
        let entry_file = Path::new("src").join("main.inf");
        let rows: [(&[ValType], &str); 3] = [
            (&[I32], "`main` takes 1 argument (i32), and project mode passes none. Run the \
                      entry file with its arguments after the path: `infs run {} <i32>`."),
            (&[I32, I64], "`main` takes 2 arguments (i32, i64), and project mode passes none. \
                           Run the entry file with its arguments after the path: `infs run {} \
                           <i32> <i64>`."),
            (&[I64, I32, I32], "`main` takes 3 arguments (i64, i32, i32), and project mode \
                                passes none. Run the entry file with its arguments after the \
                                path: `infs run {} <i64> <i32> <i32>`."),
        ];
        for (params, text) in rows {
            let main = ExportedFunction {
                name: "main".to_string(),
                params: params.to_vec(),
                result: None,
            };
            assert_eq!(
                main_takes_arguments_in_project_mode(&main, &entry_file),
                text.replace("{}", &entry_file.display().to_string())
            );
        }
    }
}

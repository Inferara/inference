#![warn(clippy::pedantic)]

//! # Inference Compiler CLI (infc)
//!
//! Standalone command line interface for the Inference programming language compiler.
//!
//! This is the legacy compiler CLI. For most users, the unified `infs` toolchain
//! CLI is recommended. Use `infc` directly when you need fine-grained control over
//! compilation phases or are integrating Inference compilation into build systems.
//!
//! ## Compilation Phases
//!
//! The Inference compiler operates in three distinct phases:
//!
//! 1. **Parse** (`--parse`) – Builds the typed AST using the custom parser
//!    - Reads the source file
//!    - Runs the Inference parser
//!    - Constructs arena-allocated AST nodes
//!    - Validates syntax and basic structure
//!    - Reports parsing errors if any
//!
//! 2. **Analyze** (`--analyze`) – Performs type checking and semantic validation
//!    - Type inference and checking
//!    - Symbol resolution
//!    - Semantic validation
//!    - Reports type errors and semantic issues
//!
//! 3. **Codegen** (`--codegen`) – Emits WebAssembly binary
//!    - Generates WebAssembly binary from typed AST
//!    - Supports non-deterministic instructions (uzumaki, forall, exists, assume, unique)
//!    - Optionally translates to Rocq (.v) format for formal verification
//!
//! ## Phase Execution
//!
//! Phases execute in canonical order (parse → analyze → codegen) regardless of
//! the order flags appear on the command line. Each phase depends on the previous:
//!
//! - `--parse` runs standalone
//! - `--analyze` automatically runs parse first
//! - `--codegen` automatically runs parse and analyze first
//!
//! ## Default Behavior
//!
//! When no phase flags are given, `infc` defaults to full compilation and writes
//! the WASM binary to disk — equivalent to `--codegen -o`. This matches
//! conventional compiler UX (e.g. `gcc foo.c`).
//!
//! ```bash
//! infc example.inf              # parse → codegen → write out/example.wasm
//! infc example.inf -v           # implies --mode proof → both out/example.wasm and out/example.v
//! infc example.inf --mode proof # proof mode (keeps specs); implies -v → writes both files
//! infc example.inf --mode compile -v # opt back into stripped-spec V output
//! infc example.inf -v --adopt-external-specs # also carry linked libraries' obligations
//! ```
//!
//! Supplying any explicit phase flag overrides the default:
//!
//! ```bash
//! infc example.inf --parse    # parse only, no output files
//! infc example.inf --analyze  # parse + analyze only, no output files
//! ```
//!
//! ## Output Artifacts
//!
//! By default, all output files are written to an `out/` directory relative to
//! the current working directory:
//!
//! - `out/<source_name>.wasm` – WebAssembly binary (when `-o` is specified)
//! - `out/<source_name>.v` – Rocq translation (when `-v` is specified)
//!
//! The `--out-dir <path>` flag overrides the directory (still relative to CWD
//! unless an absolute path is given); it applies to both the `.wasm` and the
//! `.v`. The output directory is created automatically if it doesn't exist.
//!
//! ## Linked Libraries' Proof Obligations
//!
//! A library compiled with `--mode proof` ships the obligations its author
//! stated about its own code. Linking it folds in only the executable bodies a
//! satisfied import reaches, so those obligations are not part of the merged
//! module and a build that writes a `.v` reports that they were left behind.
//!
//! `--adopt-external-specs` carries each contributing library's universal
//! (`forall`) obligations into the program's own verification sections instead,
//! namespaced under the logical module the library is bound as, and reports the
//! reachability (`exists`/`unique`) obligations it could not carry. An adopted
//! obligation is a claim to be re-proved against the merged module, never a
//! proof imported from the library. The flag requires proof mode; pairing it
//! with a compile-mode build is an error rather than a silent no-op.
//!
//! ## Host Imports
//!
//! A `use { f } from host::<module>;` clause binds `f` to a function the
//! embedder registers rather than to one this build links, so the artifact
//! ships the import and instantiates only against a host that offers it. Every
//! build that ships one prints a `host imports:` line naming each pair, which
//! is the only place a clause silently reinterpreted by the `host` reservation
//! surfaces at compile time.
//!
//! `--host-imports=<list>` is the policy those bindings are held to: the pairs
//! a build may bind, written `module.field` and comma separated. The `=` is
//! required so that `--host-imports=` — a declared, empty allowlist admitting
//! none — is spellable; omitting the flag applies no policy at all. A build
//! that writes a `.v` is refused outright, because a function the embedder
//! supplies lies outside the artifact a proof is written about.
//!
//! See `book/src/external-functions-and-wasm-linking.md` for the surface, what
//! each layer refuses, and what `mut` does and does not prove at a host
//! position.
//!
//! ## Error Handling
//!
//! The compiler reports errors to stderr with descriptive messages:
//!
//! - **Parse errors**: Syntax errors, malformed AST nodes
//! - **Type errors**: Type mismatches, undefined symbols
//! - **Codegen errors**: WebAssembly generation failures
//! - **IO errors**: File not found, permission issues
//!
//! All errors cause the process to exit with code 1.
//!
//! ## Exit Codes
//!
//! | Code | Meaning                                    |
//! |------|--------------------------------------------|
//! | 0    | Success - all requested phases completed   |
//! | 1    | Failure - usage, IO, or compilation error  |
//!
//! ## Examples
//!
//! Parse and validate syntax:
//! ```bash
//! infc example.inf --parse
//! ```
//!
//! Type check without generating code:
//! ```bash
//! infc example.inf --analyze
//! ```
//!
//! Full compilation to WebAssembly (default — no flags needed):
//! ```bash
//! infc example.inf
//! ```
//!
//! Compile and generate Rocq translation:
//! ```bash
//! infc example.inf -v
//! ```
//!
//! Full compilation with explicit flags (equivalent to the default):
//! ```bash
//! infc example.inf --codegen -o
//! ```
//!
//! Only generate Rocq (no WASM file):
//! ```bash
//! infc example.inf --codegen -v
//! ```
//!
//! ## Relationship to `infs`
//!
//! The Inference ecosystem provides two CLI tools:
//!
//! - **`infc`** (this binary) - Standalone compiler
//! - **`infs`** - Unified toolchain CLI with project management and toolchain installation
//!
//! See `apps/infs/README.md` for the full-featured toolchain interface.
//!
//! ## Current Limitations
//!
//! - Single-file compilation only (multi-file projects not yet supported)
//! - Output directory defaults to `out/` relative to CWD (not the source file
//!   location); override with `--out-dir <path>`
//! - Analysis phase is work-in-progress
//!
//! ## Tests
//!
//! Integration tests in `tests/cli_integration.rs` verify:
//! - Flag validation and error handling
//! - Phase execution correctness
//! - Output file generation
//! - Error message formatting
//!
//! See `README.md` in this crate for comprehensive usage documentation.

mod parser;
pub(crate) mod toolchain;
use clap::Parser;
use inference::wasm_link::{
    host_import_label, resolve_external_modules, HostImport, ManifestDeps, ResolvedExternalModule,
    ResolvedExternals, SearchPath,
};
use inference::{
    AnalysisOptions, ExternKind, ExternalSpecPolicy, HOST_SEGMENT, LinkOptions,
    analyze_with_options, link_resolved, parse_project, type_check, wasm_to_v,
};
use inference_wasm_codegen::{EmitFeatures, MemoryLayout, MemoryLayoutSource};
use parser::{Cli, CliMode};
use std::{
    fs,
    path::PathBuf,
    process::{self},
};
use toolchain::BuildProfile;

/// Environment variable holding a `PATH`-style list of directories to search
/// for external `.wasm` modules, after any `-L` directories.
const WASM_LIB_PATH_ENV: &str = "INFERENCE_WASM_LIB_PATH";

/// Builds the manifest-declared dependency map from `--wasm-dep <name>=<path>`
/// entries.
///
/// `infs build` forwards one entry per `Inference.toml [wasm-dependencies]`
/// declaration; these bind a logical module name directly to a `.wasm` file and
/// take precedence over every search directory. A malformed entry (no `=`, or an
/// empty name) is a hard error so a typo never silently falls through to the
/// search path.
///
/// A name whose first segment is [`HOST_SEGMENT`] is refused here. Unrefused,
/// `--wasm-dep host=x.wasm` would bind a file under the name a `use … from
/// host::…;` clause reserves for the embedder — the search-path substitution the
/// reservation exists to make impossible, reached through the one door that
/// skips the search path entirely. `infs` refuses an `Inference.toml
/// [wasm-dependencies]` key under the segment itself, before it asks `infc` to
/// build, so this refusal is the backstop: it is what a direct `--wasm-dep`
/// caller meets, and what a manifest key meets when it is forwarded by an `infs`
/// that predates that check. The manifest-facing copy of the rule is
/// `validate_wasm_dependency_key` in `apps/infs/src/project/manifest.rs`, whose
/// refusal follows this one, shortened, ending on its closing clause verbatim.
///
/// The refusal carries the two source-level reservations' "reserved in this
/// position and no longer resolves to a file" clause verbatim, and that clause
/// is the whole of what makes the three one rule rather than three rules. Its
/// reader may have typed nothing new — a script that has passed `--wasm-dep
/// host=…` for months, a project an older `infs` built yesterday — and without
/// the clause they read a rule they broke instead of a migration they owe.
///
/// The remedy is two branches because this is the only one of the three that can
/// fire on a program whose *source* is already correct. An author with a working
/// `use { clock_ms } from host::env;` who also writes a `host::env` dependency
/// entry beside it — plausible, since every other `from` clause needs one — is
/// not mistaken about the clause: the entry is the redundant half, and deleting
/// it is the fix. Issuing the rename unconditionally would tell them to turn a
/// working host binding into a linked module, which is the provider substitution
/// this refusal exists to prevent. The delete branch is phrased on what the
/// entry was written *for* rather than on the key's own spelling, because a bare
/// `host` key has no host clause to correspond to — `use … from host;` is
/// refused by the front end — and a sentence keyed on the name would read as an
/// invitation to write one. The rename branch is kept for the reader who did
/// mean a linked module, and is stated second because it is the one that changes
/// a program that works.
///
/// The match is on the first segment and is exact: not a prefix, so `hostlib`
/// binds; not case-insensitive, so `Host` binds; not a substring, so `a::host`
/// binds.
fn parse_manifest_deps(entries: &[String]) -> anyhow::Result<ManifestDeps> {
    let mut deps = ManifestDeps::new();
    for entry in entries {
        let (name, path) = entry.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid --wasm-dep `{entry}`: expected `<name>=<path>`")
        })?;
        if name.is_empty() {
            anyhow::bail!("invalid --wasm-dep `{entry}`: module name is empty");
        }
        if name.split("::").next() == Some(HOST_SEGMENT) {
            anyhow::bail!(
                "invalid --wasm-dep `{entry}`: a dependency key names the module a `use … from` \
                 clause refers to, and `{HOST_SEGMENT}` is reserved as the first segment there \
                 for imports the embedder supplies, so no linked module can be named under it. If \
                 this entry was written for a host import, delete it: a `use … from \
                 {HOST_SEGMENT}::<module>;` clause needs no dependency entry at all, because the \
                 embedder supplies the function and no file is named. Rename the module — and the \
                 `use … from` clause that binds it — to a name outside `{HOST_SEGMENT}` only if \
                 you meant a linked `.wasm` module named `{name}`: a module path whose first \
                 segment is `{HOST_SEGMENT}` is reserved in this position and no longer resolves \
                 to a file"
            );
        }
        deps.insert(name, PathBuf::from(path));
    }
    Ok(deps)
}

/// Resolves and validates every external `.wasm` module the program binds.
///
/// Resolution priority, highest first:
/// 1. manifest dependencies (`--wasm-dep`, forwarded from
///    `Inference.toml [wasm-dependencies]`),
/// 2. `-L` / `--wasm-lib-dir` directories,
/// 3. `INFERENCE_WASM_LIB_PATH` environment directories.
fn resolve_externals(
    typed_context: &inference::TypedContext,
    lib_dirs: &[PathBuf],
    manifest_deps: &ManifestDeps,
) -> anyhow::Result<ResolvedExternals> {
    let mut search_path = SearchPath::new();
    for dir in lib_dirs {
        search_path.push_lib_dir(dir.clone());
    }
    if let Some(env_path) = std::env::var_os(WASM_LIB_PATH_ENV) {
        for dir in env_search_dirs(&env_path) {
            search_path.push_env_dir(dir);
        }
    }
    Ok(resolve_external_modules(
        typed_context,
        &search_path,
        Some(manifest_deps),
    )?)
}

/// Splits an `INFERENCE_WASM_LIB_PATH`-style value into search directories,
/// dropping empty entries.
///
/// An empty entry (a leading/trailing/interior separator, or a wholly-empty
/// value) would otherwise yield an empty `PathBuf` whose `join(relative)`
/// resolves against the process CWD — silently turning the build directory into
/// a `.wasm` search root. Dropping it makes `""` and `":"` behave exactly like
/// the variable being unset.
fn env_search_dirs(env_path: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(env_path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect()
}

/// Applies default phase normalization to parsed CLI arguments.
///
/// When no phase flag (`--parse`, `--analyze`, `--codegen`) is given, defaults
/// to full pipeline + WASM output — equivalent to `--codegen -o`.
///
/// Mode/`-v` resolution rules (symmetric):
/// - `--mode proof` implies `-v` because the `.v` artifact is what proof mode
///   is for; emitting only `.wasm` in proof mode would silently waste the
///   unoptimized spec preservation work.
/// - `-v` with no explicit `--mode` implies `--mode proof` because `compile`
///   mode strips spec functions and would produce a near-empty `.v` (no
///   per-spec definitions or theorems). Users who legitimately want V output
///   from a spec-stripped WASM can pass `--mode compile -v` explicitly.
///
/// After this function, `args.mode` is always `Some(..)`.
pub(crate) fn normalize_args(args: &mut Cli) {
    // Detect explicit proof-mode combined with a non-codegen phase BEFORE the
    // default-normalization runs, so we can warn that the .v output will not
    // be produced. The warning is purely informational; exit code is unchanged.
    if matches!(args.mode, Some(CliMode::Proof)) && (args.parse || args.analyze) && !args.codegen {
        let flag = if args.parse { "--parse" } else { "--analyze" };
        eprintln!("warning: --mode proof is ignored when {flag} is set; no .v will be written");
    }
    if !args.parse && !args.analyze && !args.codegen {
        args.codegen = true;
        args.generate_wasm_output = true;
    }
    let effective_mode = match (args.mode, args.generate_v_output) {
        (Some(m), _) => m,
        (None, true) => CliMode::Proof,
        (None, false) => CliMode::Compile,
    };
    args.mode = Some(effective_mode);
    if matches!(effective_mode, CliMode::Proof) {
        args.generate_v_output = true;
    }
}

/// The external-specification policy a build runs the merge under.
///
/// The warning is keyed on whether this build writes a proof artifact rather
/// than on the mode alone, because `--mode compile -v` is explicitly supported
/// and `.v` generation is gated on `generate_v_output` alone: such a build does
/// write a `.v` that silently omits a linked library's obligations, which is the
/// defect this policy exists to retire. A build that writes no `.v` gets
/// [`ExternalSpecPolicy::Ignore`], because a report about obligations nothing
/// would have consumed is noise on every compile of every program that links a
/// proof-mode library.
///
/// The compile-mode `adopt` pair is unreachable — [`run`] refuses it before any
/// phase — and `Ignore` is the fail-safe reading of it regardless, since writing
/// verification sections into an artifact that carries none of its own is the
/// one outcome that must not happen by accident. It keeps its own arm rather
/// than being folded into the catch-all it agrees with today: the two say
/// different things, and merging them would leave the fail-safe reading of an
/// unreachable request depending on an unrelated arm's value.
#[allow(clippy::match_same_arms)]
fn external_spec_policy(
    mode: CliMode,
    generate_v_output: bool,
    adopt: bool,
) -> ExternalSpecPolicy {
    match (mode, adopt) {
        (CliMode::Proof, true) => ExternalSpecPolicy::Adopt,
        (CliMode::Compile, true) => ExternalSpecPolicy::Ignore,
        (_, false) if generate_v_output => ExternalSpecPolicy::Warn,
        (_, false) => ExternalSpecPolicy::Ignore,
    }
}

/// Resolves the `--wasm-features` entries into the emission flags code generation
/// takes.
///
/// Validation is [`inference_compiler_interface::resolve_wasm_features`] — the
/// same vocabulary and the same wording `infs` uses for the manifest key, so a
/// name rejected in one place is rejected identically in the other. Its
/// `WasmFeatureError` carries the whole diagnostic, so this surfaces it unchanged.
///
/// The mapping is an exhaustive match with no wildcard arm: a feature name cannot
/// be added to the shared vocabulary without a codegen effect being decided for
/// it here, which is why there is no "recognized but unsupported" state to
/// report.
///
/// # Errors
///
/// Returns the shared diagnostic for the first entry that is not a valid,
/// not-yet-seen feature name.
fn resolve_emit_features(entries: &[String]) -> anyhow::Result<EmitFeatures> {
    use inference_compiler_interface::{WasmFeatureName, WasmFeatureSource};

    let requested =
        inference_compiler_interface::resolve_wasm_features(entries, WasmFeatureSource::Flag)?;
    let mut features = EmitFeatures::default();
    for name in requested {
        match name {
            WasmFeatureName::BulkMemory => features.bulk_memory = true,
        }
    }
    Ok(features)
}

/// Resolves the `--target` name into the emission target code generation takes.
///
/// Validation is [`inference_compiler_interface::resolve_target`] — the same
/// vocabulary and the same wording `infs` uses for the `[build] target` key, so a
/// name rejected in one place is rejected identically in the other. Its
/// `TargetError` carries the whole diagnostic, so this surfaces it unchanged.
///
/// The mapping is an exhaustive match with no wildcard arm: a name cannot be
/// added to the shared vocabulary without an emission target being decided for it
/// here, which is why there is no "recognized but unsupported" state to report.
/// A wildcard would let a new name silently resolve to the default and ship an
/// artifact built for the wrong runtime.
///
/// # Errors
///
/// Returns the shared diagnostic when `requested` names no target this compiler
/// builds for.
fn resolve_target_flag(requested: Option<&str>) -> anyhow::Result<inference_wasm_codegen::Target> {
    use inference_compiler_interface::{TargetName, TargetSource};

    let name = match requested {
        Some(entry) => inference_compiler_interface::resolve_target(entry, TargetSource::Flag)?,
        None => TargetName::DEFAULT,
    };
    Ok(match name {
        TargetName::Wasm32 => inference_wasm_codegen::Target::Wasm32,
        TargetName::Stellar => inference_wasm_codegen::Target::Stellar,
        TargetName::SpaceWasm => inference_wasm_codegen::Target::SpaceWasm,
    })
}

/// The one line a Stellar build prints beside its progress lines: what the
/// contract exposes, at which environment protocol, and how large it is.
///
/// Printed only by a build that writes the module. The size is the size of a
/// file, and a build asked for no file has none to report — while the rewrite
/// itself still runs, so a phase-only build is held to everything a writing one
/// is held to.
///
/// The arity is the *value* arity — the number of 64-bit words a host passes —
/// which is the source parameter count, since every admissible parameter takes
/// exactly one word and a returned value takes none. Printing it is what makes
/// a wrong call site diagnosable from the build log: a Soroban host reports an
/// arity mismatch without naming the arity it expected, and it reports a wrong
/// argument type as an undiscriminated trap.
fn stellar_summary(exports: &[inference_wasm_codegen::ExportSignature], size: usize) -> String {
    let methods = exports
        .iter()
        .map(|export| format!("{}/{}", export.name, export.params.len()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Stellar contract: {methods}; env protocol {}; {size} bytes",
        inference_stellar_abi::STELLAR_ENV_PROTOCOL,
    )
}

/// The refusal owed to a foreign module a build for `target` cannot carry, or
/// `None` when every external clears the instruction set that target is held
/// to.
///
/// A target that answers
/// [`inference_wasm_codegen::Target::requires_wasm1_externals`] ships an
/// artifact that is WebAssembly 1.0 throughout, and that artifact is the
/// program *and* every external merged into it. The linker is deliberately
/// more permissive — it accepts sign extension, bulk memory and mutable
/// globals, which is what an ordinary foreign toolchain emits — so without
/// this the first thing to notice a post-1.0 external is a whole-artifact
/// check, by which point the offending instruction sits at an offset into
/// merged bytes that came from no single file, and the message can neither
/// name the artifact nor suggest anything to do about it.
///
/// Asked of each external before the merge, the same question names the module,
/// the file it resolved to and a remedy, and the validator's offset points into
/// that file.
///
/// The sentence is target-neutral because the fact is: what each of these
/// targets does with the merged module differs — one rewrites it into a
/// contract, another hands it to a flight interpreter — but neither can carry
/// an instruction its runtime does not decode, and a reason that named one
/// runtime would be wrong about the other.
fn foreign_module_refusal(
    target: inference_wasm_codegen::Target,
    externals: &[ResolvedExternalModule],
) -> Option<String> {
    externals.iter().find_map(|external| {
        let reason = inference_target_conformance::check_wasm1(&external.bytes).err()?;
        let logical = &external.logical_module;
        Some(format!(
            "The `{}` target: the external module `{logical}`, resolved to {}, is not a \
             WebAssembly 1.0 module: {reason}. This target's artifact is WebAssembly 1.0 \
             throughout and this module is merged into it, so the artifact is refused as a \
             whole rather than in part. Rebuild `{logical}` for the WebAssembly 1.0 \
             instruction set — a stock Rust `wasm32-unknown-unknown` build emits \
             sign-extension instructions by default — or build this program at --target {}, \
             which links the module as it is.",
            target.as_str(),
            external.path.display(),
            inference_compiler_interface::TargetName::DEFAULT.as_str(),
        ))
    })
}

/// The refusal owed to a `.v` request whose target rewrites the module after the
/// translation would have read it, or `None` when the pairing is sound.
///
/// The Stellar target ships a module the Val-ABI rewriter produced, and that
/// pass runs after linking — after the only bytes `wasm_to_v` ever sees. A proof
/// artifact written from those bytes would describe a module with a different
/// export section, different function bodies and a metadata section it does not
/// mention, while claiming to be about the artifact beside it on disk. That is
/// worse than no `.v` at all, so the pairing is refused rather than qualified.
///
/// Only compile mode reaches here. `--mode proof` and a bare `-v` both resolve
/// to proof mode in [`normalize_args`], and a proof-mode build for a target that
/// does not support it is refused by code generation in its own words; adding a
/// second refusal for the same thing would be a second wording to keep in step.
/// What is left is `--mode compile -v`, the one spelling that keeps compile mode
/// and still asks for a translation.
///
/// The condition names one target rather than asking a predicate, and that is
/// deliberate: what it refuses is a *rewrite*, and only that target performs
/// one. A target that adds nothing to the module falls through and gets the `.v`
/// a `wasm32` compile-mode build of the same source writes — byte for byte,
/// because that is what the bytes on disk are. Widening this to "any non-default
/// target" would refuse the pairing that is most worth having, in a message
/// about a rewrite that never happened.
///
/// Its neighbour [`foreign_module_refusal`] *is* asked through a predicate, and
/// the difference is the whole point: that one asks what a runtime decodes, a
/// question every narrowed target answers the same way, while this one asks
/// what this compiler does to the module after the translation has read it —
/// which is nothing, for every target but one. A reader generalizing the two
/// Stellar-gated refusals in this file together deletes a legitimate build.
fn proof_artifact_refusal(
    target: inference_wasm_codegen::Target,
    mode: Option<CliMode>,
    generate_v_output: bool,
) -> Option<String> {
    if target != inference_wasm_codegen::Target::Stellar
        || !generate_v_output
        || !matches!(mode, Some(CliMode::Compile))
    {
        return None;
    }
    Some(format!(
        "Error: -v cannot be combined with --target {}. The module this target \
         ships is rewritten into the Stellar value ABI after linking, and the \
         Rocq translation reads the pre-rewrite bytes: the .v would describe a \
         module with different exports, different bodies and no environment \
         metadata, not the .wasm written beside it. Build the same source at \
         --target {} to obtain a .v — code generation is target-blind, so the \
         pre-rewrite bytes of the two builds are the same module.",
        inference_compiler_interface::TargetName::Stellar.as_str(),
        inference_compiler_interface::TargetName::DEFAULT.as_str(),
    ))
}

/// The host imports a build is allowed to bind, keyed by import module.
///
/// A map of modules to fields rather than a flat set of pairs, because the
/// refusal asks two questions of it and only one of them is about a pair. "Is
/// `env`.`clock_ms` listed?" decides whether to refuse; "does `env` have an
/// entry at all?" decides which edit the refusal asks for, and a flat set can
/// answer the second only by scanning. The ordering is the rendering order: a
/// build is told what it was given as a sorted list, so two invocations that
/// name the same imports in different orders read alike.
type HostImportAllowlist = std::collections::BTreeMap<String, std::collections::BTreeSet<String>>;

/// Reads `--host-imports` into the policy a build runs under.
///
/// Each entry is one host import written `module.field` — the two strings an
/// embedder registers a function under, joined by a dot. The dot is the only
/// separator: an import module string is flat, so a name with a second dot in
/// it could never match a declaration and is a typo rather than a nested path.
///
/// `::` is that same mistake made with the source's own spelling, and it gets
/// a sentence of its own rather than the shared one. It is the likeliest entry
/// anybody writes, because the clause being transcribed is
/// `use { clock_ms } from host::env;` and both halves of it mislead:
/// `env::clock_ms` keeps the path separator, and `host::env.clock_ms` keeps a
/// segment that names the provider and is never part of an import name.
/// Neither can match a declaration — the type checker strips `host` and keeps
/// exactly one segment — so under the shared message the first reads as
/// malformed for no stated reason, while the second is taken for a module
/// named `host::env`, refused by the allowlist two phases later, and read as
/// the allowlist mechanism being broken rather than as a typo. The sentence is
/// [`path_separator_refusal`]'s, which also spells the entry its author meant.
///
/// An empty entry is the flag's third state and reaches clap as one empty
/// value, not as none: `--host-imports=` parses to `[""]`. That spelling is a
/// *declared and empty* allowlist, which admits nothing, and it is the whole
/// reason the flag is an `Option` — see [`Cli::host_imports`]. Only a
/// single-entry list means it. An empty entry inside a list that names
/// something is a stray or trailing comma, and is told so in its own words:
/// the refusal below quotes the entry it rejected, and quoting an empty one
/// would name neither the comma nor the empty-allowlist spelling it is one
/// character away from.
///
/// Entries are trimmed before they are read, because the inventory line a
/// successful build prints exists to be copied back into this flag and renders
/// its pairs `env.clock_ms, fprime_core.telemetry`. A paste inside quotes keeps
/// the separator's space — an unquoted one never reaches this function, since
/// the shell splits it into separate arguments first — and a refusal whose
/// offending character is an invisible leading space inside backticks is one
/// nobody can act on. The single-empty-entry check above reads the values as
/// given, so trimming cannot turn a list of blanks into the empty-allowlist
/// policy.
///
/// # Errors
///
/// Returns the shared diagnostic for the first entry that is not two non-empty
/// names joined by exactly one dot, the `::` diagnostic for the first entry
/// written with the source's path separator, or the stray-comma diagnostic for
/// the first entry that is empty once trimmed.
fn parse_host_import_allowlist(entries: &[String]) -> anyhow::Result<HostImportAllowlist> {
    let mut allowed = HostImportAllowlist::new();
    if entries.len() == 1 && entries[0].is_empty() {
        return Ok(allowed);
    }
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            anyhow::bail!(
                "invalid `--host-imports`: the list has an empty entry, which is a stray or \
                 trailing comma. `--host-imports=` on its own is the empty allowlist; a list \
                 names `module.field` pairs, as in \
                 `--host-imports=env.clock_ms,fprime_core.command`."
            );
        }
        if entry.contains("::") {
            anyhow::bail!("{}", path_separator_refusal(entry));
        }
        let pair = entry.split_once('.').filter(|(module, field)| {
            !module.is_empty() && !field.is_empty() && !field.contains('.')
        });
        let Some((module, field)) = pair else {
            anyhow::bail!(
                "invalid `--host-imports` entry `{entry}`: each entry is a host import in \
                 `module.field` form, as in `--host-imports=env.clock_ms,fprime_core.command`."
            );
        };
        allowed
            .entry(module.to_string())
            .or_default()
            .insert(field.to_string());
    }
    Ok(allowed)
}

/// The refusal for an allowlist entry written with the source's `::`, naming
/// the entry its author meant wherever one can be read out of it.
///
/// A rule and an unrelated example are less than such a reader needs. The entry
/// is almost always a transcription of one `use` clause, and a transcription
/// has exactly one right spelling: so a leading `host::` is stripped — it names
/// the provider and is never part of an import name — and what is left is
/// split once, on `::` or failing that on the dot, and rendered back as
/// `module.field`. That is a correction only where it yields two non-empty
/// names with no separator left in either. `a::b::c` holds three names, and
/// picking two of them would be a guess presented as a fix, so it gets the
/// generic example instead.
///
/// The fault is named by what the entry carries. `host::env.clock_ms` already
/// separates its pair with a dot, and telling its author the separator is wrong
/// would send them to the one part of it that is right.
fn path_separator_refusal(entry: &str) -> String {
    let unprefixed = entry
        .strip_prefix(HOST_SEGMENT)
        .and_then(|rest| rest.strip_prefix("::"));
    let rest = unprefixed.unwrap_or(entry);
    let fault = match (unprefixed, rest.contains("::")) {
        (None, _) => "the separator here is a dot, not `::`".to_string(),
        (Some(_), false) => format!("the `{HOST_SEGMENT}::` prefix is not part of an import name"),
        (Some(_), true) => format!(
            "the separator here is a dot, not `::`, and the `{HOST_SEGMENT}::` prefix is not \
             part of an import name"
        ),
    };
    let meant = rest
        .split_once("::")
        .or_else(|| rest.split_once('.'))
        .filter(|&(module, field)| {
            [module, field]
                .iter()
                .all(|name| !name.is_empty() && !name.contains("::") && !name.contains('.'))
        });
    let correction = match meant {
        Some((module, field)) => format!("Write it `{module}.{field}`."),
        None => format!(
            "For example, `use {{ clock_ms }} from {HOST_SEGMENT}::env;` is written \
             `env.clock_ms`."
        ),
    };
    format!(
        "invalid `--host-imports` entry `{entry}`: {fault}. An entry names the WebAssembly \
         import module and field an embedder registers a function under: an import module \
         string is flat, and `{HOST_SEGMENT}` names the provider, so neither `::` nor a \
         `{HOST_SEGMENT}` segment ever appears in the artifact. {correction}"
    )
}

/// Every host import the program binds, as the label a diagnostic names one by,
/// sorted and deduplicated.
///
/// Read off the typed context rather than off the linker driver's answer,
/// because the one refusal that needs it runs before resolution. Two
/// declarations of one `(module, field)` — the same function reached from two
/// files — are one import to an embedder and must be one name in a message, so
/// the list is deduplicated here exactly as resolution deduplicates its own.
fn host_import_labels(typed_context: &inference::TypedContext) -> Vec<String> {
    let origins = typed_context.extern_origins();
    let mut labels: Vec<String> = origins
        .iter()
        .filter(|origin| origin.kind == ExternKind::Host)
        .map(|origin| host_import_label(&origin.logical_module, &origin.export_field))
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

/// The refusal owed to a build that writes a proof artifact and binds a host
/// import, or `None` when it binds none.
///
/// A host import has no body in this build: the embedder supplies it at
/// instantiation, and nothing the translation reads describes what it does. A
/// `.v` written anyway would be a proof about a module with a hole in it, and
/// the hole is invisible in the artifact — which is worse than no proof, so the
/// pairing is refused rather than qualified.
///
/// Code generation refuses the same pairing in its own words, and this is not
/// that refusal moved: this one runs before external resolution, and the
/// ordering is what it is for. A program that binds both a host import and a
/// linked module is refused by resolution as mixed, and that refusal's first
/// remedy is to bind every extern to `host::…` — advice a proof build must not
/// be given, since following it lands the author here having rewritten every
/// `use` clause for nothing. Asked first, the reader hears the refusal that
/// actually governs the build they asked for.
///
/// The remedy names the requests rather than a mode, because the gate does not
/// read the mode: `-v` requests a `.v` on its own, so "build with `--mode
/// compile`" clears `--mode proof` and sends `--mode compile -v` round the same
/// refusal having changed nothing. It asks for every request to go, since a
/// build can pass both and a reader who drops one of two meets this refusal
/// again. And it names the one request a reader may not have typed: `infs
/// build` passes `--mode proof` itself for a project whose manifest sets that
/// mode, and a reader told only to drop flags would search a command line that
/// holds none.
///
/// Takes the rendered labels rather than the typed context because that is the
/// whole of what the message needs, and because the plural agreement is the
/// only decision in it: one import is what the wording was written for, and a
/// program binding three must not be told about "its" behavior.
fn host_import_proof_refusal(host: &[String]) -> Option<String> {
    if host.is_empty() {
        return None;
    }
    let (subject, verb, possessive, object) = if host.len() == 1 {
        (host[0].clone(), "is", "its", "the host import")
    } else {
        (host.join(", "), "are", "their", "the host imports")
    };
    Some(format!(
        "Error: Host imports are not yet modeled in the proof translation. {subject} {verb} \
         satisfied by the embedder, so {possessive} behavior is outside the artifact a proof is \
         written about, and the translation has no way to state an assumption about it. Build \
         this program without a proof artifact, or remove {object} from the program. `-v` and \
         `--mode proof` each request a `.v` on their own, so drop every one of them this build \
         passed; `infs build` passes `--mode proof` on a project's behalf when its \
         Inference.toml sets `[build] mode = \"proof\"`, and there the request to drop is that \
         setting: make it `\"compile\"`."
    ))
}

/// The `Inference.toml [host-imports]` edit that would admit `fields` under
/// `module`.
///
/// The table holds one array per module — the shape the Host Imports section of
/// `book/src/external-functions-and-wasm-linking.md` documents — so the edit is
/// not one shape but two: a module with no entry needs its key written, and a
/// module that has one needs names added to the array already there. Deciding
/// which is the compiler's job, because the compiler is the one holding the
/// allowlist; a message that made the reader go and check would be asking for
/// work already done. Reading the table's keys off the flag is sound because
/// `infs` refuses an empty array on load, so every key it forwards arrives as at
/// least one pair.
///
/// `fields` is every unadmitted field of `module`; why they arrive together is
/// [`host_import_allowlist_refusal`]'s.
fn host_import_table_edit(
    module: &str,
    fields: &std::collections::BTreeSet<&str>,
    has_entry: bool,
) -> String {
    if has_entry {
        let quoted = fields
            .iter()
            .map(|field| format!("`\"{field}\"`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("add {quoted} to the `{module}` entry")
    } else {
        let array = fields
            .iter()
            .map(|field| format!("\"{field}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("add `{module} = [{array}]`")
    }
}

/// The refusal owed to a build whose allowlist does not admit every host import
/// it resolved, or `None` when no allowlist was given or every import is in it.
///
/// Asked of what resolution produced rather than of the declarations, so a pair
/// two files declare is judged once, under the one signature they had to agree
/// on to get this far.
///
/// One paragraph per import *module*, naming every unadmitted field of it, and
/// all of them printed together. Reporting one finding per build would make a
/// program binding four unadmitted functions four builds to correct; reporting
/// one paragraph per *import* would hand the reader edits that collide, because
/// both mechanisms hold one list per module. Two paragraphs asking separately
/// for `env` ask for a duplicate key in a table and for two halves of one list
/// on a command line, and applying either pair literally loses an import or
/// fails to parse. Grouped, a paragraph asks for the whole edit for its module
/// and the paragraphs can be applied in any order.
///
/// The source-level remedy names the module and the unadmitted fields rather
/// than quoting a `use` clause: `fields` is the unadmitted subset of a set
/// deduplicated across the program, so a clause spelled from it need not exist
/// in any file, and the refusal carries no file or line to navigate by instead.
/// For the same reason it asks for "every" binding of those names rather than
/// "the" one: a field two files bind is one import here, and the refusal sees
/// the deduplicated name, never the clauses behind it, so it cannot count them.
/// "Drop every `host::env` binding of `clock_ms` and `sleep_ms`" holds of one
/// clause carrying both names, of one per file, and of any mix of the two.
///
/// `infc` is handed a flag and has no manifest, so the flag is named as the
/// mechanism and the manifest as where `infs` fills it from — not the other way
/// round, and in both branches. Inverting it would describe a file this
/// invocation never read, to a caller who may not have one, and the edit it
/// asked for would be in a syntax the flag refuses: `--host-imports` takes
/// `module.field`, never a TOML array. The manifest sentence is there for the
/// other reader, who reached this refusal through `infs build` or `infs run`
/// and never typed the flag at all: `infs` filled it from their `[host-imports]`
/// table, and the table is the thing they edit, in its own syntax. That holds
/// for the empty policy as much as for a listed one — `infs` forwards
/// `--host-imports=` from a header with no keys — so both branches spell the
/// table edit, and the empty one spells it as a new key, since that table has
/// none.
fn host_import_allowlist_refusal(
    allowlist: Option<&HostImportAllowlist>,
    declared: &[HostImport],
) -> Option<String> {
    let allowed = allowlist?;
    let mut unadmitted: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    for import in declared {
        if allowed
            .get(&import.module)
            .is_some_and(|fields| fields.contains(&import.field))
        {
            continue;
        }
        unadmitted
            .entry(import.module.as_str())
            .or_default()
            .insert(import.field.as_str());
    }
    if unadmitted.is_empty() {
        return None;
    }
    let given = allowed
        .iter()
        .flat_map(|(module, fields)| fields.iter().map(move |field| format!("{module}.{field}")))
        .collect::<Vec<_>>()
        .join(", ");
    let mut blocks = Vec::new();
    for (module, fields) in &unadmitted {
        let labels = fields
            .iter()
            .map(|field| host_import_label(module, field))
            .collect::<Vec<_>>()
            .join(", ");
        let (noun, verb, pronoun) = if fields.len() == 1 {
            ("host import", "is", "it")
        } else {
            ("host imports", "are", "them")
        };
        let pairs = fields
            .iter()
            .map(|field| format!("{module}.{field}"))
            .collect::<Vec<_>>()
            .join(",");
        let names = serial_list(
            &fields
                .iter()
                .map(|field| format!("`{field}`"))
                .collect::<Vec<_>>(),
        );
        let remedy = format!(
            "Add `{pairs}` to `--host-imports` to admit {pronoun}, or drop every \
             `{HOST_SEGMENT}::{module}` binding of {names}."
        );
        if allowed.is_empty() {
            let table_edit = host_import_table_edit(module, fields, false);
            blocks.push(format!(
                "Error: {noun} {labels} {verb} not in this build's host-import allowlist: the \
                 allowlist is present and empty, which forbids every host import. The allowlist \
                 (`--host-imports`) names every host function the program may bind, and \
                 `--host-imports=` declares that it binds none. {remedy} `infs` fills the flag \
                 from the `[host-imports]` table in Inference.toml, where this empty policy is \
                 the table with no keys and the same edit is to {table_edit} under it."
            ));
        } else {
            let table_edit = host_import_table_edit(module, fields, allowed.contains_key(*module));
            blocks.push(format!(
                "Error: {noun} {labels} {verb} not in this build's host-import allowlist. The \
                 allowlist (`--host-imports`) names every host function the program may bind, \
                 and this build was given `{given}`. {remedy} `infs` fills the flag from the \
                 `[host-imports]` table in Inference.toml, where the same edit is to \
                 {table_edit}."
            ));
        }
    }
    Some(blocks.join("\n\n"))
}

/// `items` as an English list: `a`, `a and b`, `a, b, and c`.
fn serial_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
    }
}

/// The one line a build that ships host imports prints: which functions the
/// artifact will ask an embedder for, and whether a reviewed policy admitted
/// them.
///
/// The parenthetical is the load-bearing half. Reserving `host` positionally
/// means a two-segment `use { io_write } from host::io;` clause that used to
/// link `host/io.wasm` now declares a host import of `io` instead — the file is
/// no longer read, the embedder becomes responsible for the function, and
/// nothing else in the toolchain says so. This line is the only compile-time
/// surface that will ever tell that author, so it names every import rather
/// than counting them.
///
/// The qualifier separates an import admitted by a policy from one admitted by
/// the absence of one, which is the distinction a reviewer reading a build log
/// needs and cannot otherwise recover: the two builds print the same names.
///
/// The pairs are rendered `env.clock_ms`, not the backticked
/// [`host_import_label`] a diagnostic uses. A build-log line is read as plain
/// text and its names are copied into `--host-imports`, which spells them this
/// way; a diagnostic is read as prose, where the two halves have to be told
/// apart from the dot joining them.
///
/// Sorted by `(module, field)`, which `declared` already is.
fn host_import_inventory(declared: &[HostImport], allowlisted: bool) -> Option<String> {
    if declared.is_empty() {
        return None;
    }
    let names = declared
        .iter()
        .map(|import| format!("{}.{}", import.module, import.field))
        .collect::<Vec<_>>()
        .join(", ");
    let qualifier = if allowlisted { "" } else { " (no allowlist)" };
    Some(format!("host imports{qualifier}: {names}"))
}

/// Renders a `wasm_to_v` failure with the user-facing diagnostic shape
/// described in plan §6: a dedicated message for Rocq-stdlib shadowing,
/// dedicated guidance for the `__` collision, and a generic invalid-Rocq-identifier
/// fallthrough for the remaining reasons.
///
/// The rejected name can be either the source-derived module name OR a spec
/// name declared in the source (since `translate()` now validates each spec
/// name up-front). The diagnostic uses neutral phrasing because the CLI does
/// not currently have a way to tell which source the name came from —
/// labelling it "source filename" when the offender was a spec name was a
/// wrong guess.
fn eprint_translation_error(e: &anyhow::Error) {
    use inference::{InvalidIdentifierReason, WasmToVError};
    if let Some(wte) = e.downcast_ref::<WasmToVError>() {
        match wte {
            WasmToVError::RocqStdlibShadow { name } => {
                eprintln!(
                    "error: '{name}' would shadow the Rocq stdlib type '{name}'. \
                     Rename the source file or spec to avoid the collision (e.g. \
                     'list_ops', 'my_list')."
                );
                return;
            }
            WasmToVError::InvalidRocqIdentifier {
                name,
                reason: InvalidIdentifierReason::ContainsDoubleUnderscore,
            } => {
                eprintln!(
                    "error: '{name}' contains '__' which is reserved as the \
                     module/spec name separator in the emitted Rocq output. \
                     Use a single underscore or a different name."
                );
                return;
            }
            WasmToVError::InvalidRocqIdentifier {
                reason: InvalidIdentifierReason::EmptyName,
                ..
            } => {
                eprintln!(
                    "error: empty Rocq identifier — the source filename has no \
                     usable stem (e.g. \".inf\" with no name), or a spec block has \
                     no name.\n\n  Rename the source file."
                );
                return;
            }
            WasmToVError::InvalidRocqIdentifier { name, reason } => {
                eprintln!(
                    "error: '{name}' is not a valid Rocq identifier.\n\n  \
                     A Rocq identifier (used for both module names and spec \
                     names) must:\n    \
                     - start with a letter (A-Z or a-z)\n    \
                     - contain only letters, digits, and underscores\n    \
                     - not contain '__' (reserved as the module/spec name separator)\n    \
                     - not collide with Rocq stdlib types or reserved keywords\n\n  \
                     Rename the source file or the spec block (e.g. 'list_utils') \
                     and re-run.\n  (specifically: {reason})"
                );
                return;
            }
            WasmToVError::EmbeddedSpecMismatch { .. } => {
                eprintln!(
                    "error: internal inconsistency — the codegen-emitted spec map \
                     and the embedded `inference.spec_funcs` section disagree.\n\n  \
                     This is a compiler bug; please file an issue with the .inf \
                     source attached."
                );
                return;
            }
            WasmToVError::SpecNameReservesSeparator {
                offender_kind,
                offender,
                joined,
                fix_hint,
            } => {
                eprint_spec_join_boundary_error(offender_kind, offender, joined, fix_hint);
                return;
            }
            WasmToVError::ModuleNameShadowsPreambleHelper { name, fix_hint } => {
                eprint_module_name_shadow_error(name, fix_hint);
                return;
            }
            WasmToVError::WasmParse(msg) => {
                eprintln!(
                    "error: malformed WebAssembly binary: {msg}\n\n  \
                     The WASM input could not be parsed. If this binary was \
                     produced by `infc`, please file a bug. If it came from \
                     another source, the file may be corrupted or use an \
                     unsupported extension."
                );
                return;
            }
            WasmToVError::UnsupportedFeature { description } => {
                eprintln!(
                    "error: this module cannot be translated to Rocq: {description}\n\n  \
                     The proof model a .v targets describes a subset of \
                     WebAssembly, and this construct falls outside it. That is a \
                     property of the model rather than unfinished work, so no \
                     flag enables it.\n\n  \
                     The module can still be compiled and run — drop '-v' (and \
                     any explicit '--mode proof') to build the .wasm without a \
                     proof artifact."
                );
                return;
            }
            // WasmToVError is #[non_exhaustive]; the wildcard handles future
            // variants by falling through to the generic message below.
            _ => {}
        }
    }
    eprintln!("WASM->V translation failed: {e}");
}

/// Renders the educational diagnostic for a spec/module name whose trailing `_`
/// fabricates Rocq's reserved `__` separator when joined into the proof grammar.
/// The fix differs by which component offended (rename the source file vs. the
/// spec block); both are surfaced as a concrete rename.
fn eprint_spec_join_boundary_error(
    offender_kind: &str,
    offender: &str,
    joined: &str,
    fix_hint: &str,
) {
    let rename = if offender_kind == "output module name" {
        format!("Rename the source file: '{offender}.inf' -> '{fix_hint}.inf'.")
    } else {
        format!("Rename the spec: 'spec {offender}' -> 'spec {fix_hint}'.")
    };
    eprintln!(
        "error: the {offender_kind} '{offender}' ends with '_', so it joins \
         into the reserved '__' run in the Rocq proof name '{joined}'.\n\n  \
         The emitted proof grammar is '<module>__<spec>_specs', where '__' \
         separates the module from the spec; a trailing '_' on either side \
         fabricates that separator. {rename}\n\n  \
         Why not auto-encode: proof-mode names appear verbatim in your .v \
         file, so they are kept readable rather than escaped into noise."
    );
}

/// Renders the diagnostic for an output module name the generated `.v` already
/// spends on one of its preamble helpers. The module name comes from the source
/// file's stem, so the concrete fix is a file rename.
fn eprint_module_name_shadow_error(name: &str, fix_hint: &str) {
    eprintln!(
        "error: the output module name '{name}' is one of the helper definitions \
         every generated .v opens with, so '{name}' would name two top-level \
         definitions in one file.\n\n  \
         The module name comes from the source filename, and the .v spends it \
         again on 'Definition {name}' for the module record and on 'Theorem \
         valid_{name}'. Rocq definitions are not overloadable, so a name claimed \
         twice makes the whole file fail to compile. Rename the source file: \
         '{name}.inf' -> '{fix_hint}.inf'.\n\n  \
         Why not auto-rename: proof-mode names appear verbatim in your .v file, \
         so a proof that imports '{name}' would silently lose its subject."
    );
}

/// Removes any pre-existing output artifacts a prior build left, so a compile
/// that is later rejected leaves no runnable stale file behind, and a plain
/// compile does not leave a stale proof describing a since-changed program.
///
/// Both the `.wasm` and the `.v` are cleared together whenever the run will write
/// *at least one* artifact, regardless of which one. A run that writes any output
/// recompiles this source name and may be rejected at codegen or `wasm_to_v`; the
/// would-be `.wasm` and `.v` for that name are stale the moment such a run starts,
/// so a leftover from an earlier build must not survive — a `--codegen -v` run
/// (which writes only `.v`) must still invalidate the `.wasm` an earlier
/// `--codegen -o` or default build wrote, or a rejection would leave a runnable
/// artifact describing the old program. The success path rewrites whichever
/// artifacts this invocation requests, so clearing both up front costs nothing
/// there. A no-output dry run (`--codegen` with neither `-o` nor `-v`) writes
/// nothing, so its caller does not invoke this — a dry run leaves existing
/// artifacts untouched.
///
/// A missing file is not an error (nothing to clear); a removal failure is
/// ignored because the subsequent write would surface any genuine IO problem
/// with a precise message, and a transient failure must not abort an
/// otherwise-valid build. The directory itself is left untouched — it is created
/// on the success path exactly as before.
fn clear_stale_outputs(output_dir: &std::path::Path, source_fname: &str) {
    let _ = fs::remove_file(output_dir.join(format!("{source_fname}.wasm")));
    let _ = fs::remove_file(output_dir.join(format!("{source_fname}.v")));
}

/// Runs the compiler driver on an explicitly sized stack.
///
/// The compiler's phases recurse with the input's syntactic nesting depth, and the
/// platform's default main-thread stack is too small to survive input the front end
/// is expected to accept. Exit codes are what they were: `process::exit` terminates
/// the process identically from the scoped worker thread, and a panic inside the
/// driver is re-raised on the main thread with its original payload, printed once.
/// The only stderr difference is that a panic header now names the compile thread
/// rather than `main`.
fn main() {
    inference::with_compiler_stack(run);
}

/// The Inference compiler CLI driver, run by [`main`] on a compiler-sized stack.
///
/// ## Execution Flow
///
/// 1. **Parse command line arguments** using clap
/// 2. **Validate input**: verify source file exists
/// 3. **Apply default normalization**: when no phase flags are given, defaults
///    to full pipeline (`--codegen -o`) so that `infc file.inf` just works
/// 4. **Execute compilation phases** in canonical order:
///    - Parse: Build typed AST from source using the custom parser
///    - Analyze: Type check and semantic validation
///    - Codegen: Generate WebAssembly binary from typed AST
/// 5. **Generate output files** (if requested):
///    - Write WASM binary with `-o` flag (set by default when no flags given)
///    - Write Rocq translation with `-v` flag
///
/// ## Error Handling
///
/// All errors are reported to stderr with descriptive messages and cause
/// process exit with code 1. Error categories:
///
/// - **Usage errors**: Invalid arguments
/// - **IO errors**: File not found, permission denied, output write failures
/// - **Compilation errors**: Parse errors, type errors, codegen failures
///
/// ## Phase Coordination
///
/// The function ensures correct phase dependencies:
/// - Parse phase always runs first when any phase is requested
/// - Analyze phase requires parse output (typed AST)
/// - Codegen phase requires analyze output (typed context)
///
/// Phase outputs are stored in `Option` types and unwrapped only when
/// guaranteed to be present by prior validation logic.
///
/// ## Output Management
///
/// Output files are written to the output directory (`out/` by default, relative
/// to CWD; overridable via `--out-dir <path>`):
/// - Directory is created if it doesn't exist
/// - File names are derived from source file stem
/// - Both `-o` and `-v` flags can be used simultaneously
/// - `--out-dir` redirects both the `.wasm` and the `.v`
///
/// ## Implementation Notes
///
/// - Uses `anyhow::Result` for error propagation from library functions
/// - Reports every phase failure as a diagnostic and calls `process::exit(1)`
///   explicitly, rather than aborting the process. Code generation is fail-closed
///   for this reason: a construct that reaches it without a lowering is recorded
///   as a `CodegenError` and returned, and this function renders it as
///   `Codegen failed: …`. The property is enforced by the `panic_free` sweep in
///   `tests/src/panic_free.rs`, which runs the whole pipeline over every fixture
///   in the repository in both compilation modes inside `catch_unwind`
/// - Reads entire source file into memory (limitation: no streaming)
/// - Phase execution is sequential (no parallelization)
#[allow(clippy::too_many_lines)]
fn run() {
    let mut args = Cli::parse();

    if args.commit_hash {
        println!("{}", env!("INFC_GIT_COMMIT"));
        process::exit(0);
    }

    if args.abi_version {
        println!(
            "{}.{}",
            inference_compiler_interface::COMPILER_ABI_MAJOR,
            inference_compiler_interface::COMPILER_ABI_MINOR,
        );
        process::exit(0);
    }

    let Some(path) = args.path.clone() else {
        eprintln!("Error: source file argument required");
        process::exit(1);
    };
    if !path.exists() {
        eprintln!("Error: path not found");
        process::exit(1);
    }

    normalize_args(&mut args);

    // Refuse a proof-only request against a compile-mode build here, where the
    // effective mode is already resolved, so `--adopt-external-specs` with
    // neither `-v` nor `--mode` is refused exactly like `--mode compile
    // --adopt-external-specs`. Before any phase runs, so nothing is written.
    if args.adopt_external_specs && matches!(args.mode, Some(CliMode::Compile)) {
        eprintln!(
            "Error: --adopt-external-specs requires proof mode; this build resolves to compile \
             mode, which strips the program's own specification functions and emits no \
             verification section for a linked library's obligations to join. Pass -v (or --mode \
             proof), or drop --adopt-external-specs."
        );
        process::exit(1);
    }

    // Resolve the requested instruction set before any phase runs: a misspelled
    // feature is a mistake about the artifact, and reporting it after a full
    // parse and type check would bury it under work the user has to discard
    // anyway.
    let emit_features = match resolve_emit_features(&args.wasm_features) {
        Ok(features) => features,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    // Resolve the target for the same reason and at the same point: naming a
    // runtime this compiler does not build for is a mistake about the artifact,
    // and it must be reported before a parse and a type check the user then has
    // to discard.
    let target = match resolve_target_flag(args.target.as_deref()) {
        Ok(target) => target,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    // Refuse a proof artifact for a target whose shipped module is not the one
    // the translation would read, before any phase runs. See
    // `proof_artifact_refusal` for why compile mode is the only spelling this
    // has to catch.
    if let Some(message) = proof_artifact_refusal(target, args.mode, args.generate_v_output) {
        eprintln!("{message}");
        process::exit(1);
    }

    // Resolve the memory layout here for the same reason, and because both the
    // analysis phase and code generation need it: A036 measures call chains
    // against this stack size and the emitter lays every frame out in it, so the
    // two must be handed one value rather than each reaching for a default.
    let layout = match MemoryLayout::resolve(
        args.memory_pages,
        args.stack_size,
        MemoryLayoutSource::Flag,
    ) {
        Ok(layout) => layout,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    // Read the host-import policy here for the reason the three resolutions
    // above are here: a malformed entry is a mistake about the build, not about
    // the program, and a policy reported after the parse and type check it was
    // written to gate is a policy the user has to discard work to act on.
    let host_allowlist = match args
        .host_imports
        .as_deref()
        .map(parse_host_import_allowlist)
        .transpose()
    {
        Ok(allowlist) => allowlist,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let output_path = args
        .out_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("out"));
    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    let source_fname = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("module")
        .to_string();

    // Clear any artifact a previous build left in the output directory before
    // this build runs, so a compile that is later rejected (by type check,
    // analysis, external resolution, codegen, or `wasm_to_v`) never leaves a
    // runnable stale `.wasm` (or its `.v`) on disk for `wasmtime` to execute.
    // Both artifacts are cleared whenever this run will write at least one of
    // them — independent of which one — because the run recompiles this source
    // name and any leftover for it is already stale; a `--codegen -v` run that
    // writes only `.v` must still drop an earlier build's `.wasm`. Clearing up
    // front means every rejection path exits with no artifact without each
    // `process::exit(1)` site having to clean up. A run that writes no output —
    // a parse/analyze-only run, or a `--codegen` dry run with neither `-o` nor
    // `-v` — must not disturb a previous build's artifacts, so clearing is gated
    // on this run actually emitting something.
    if need_codegen && (args.generate_wasm_output || args.generate_v_output) {
        clear_stale_outputs(&output_path, &source_fname);
    }

    let mut t_ast = None;
    if need_codegen || need_analyze || need_parse {
        // Drive the multi-file front end. A single file with no path-form `use`
        // imports yields a one-file arena identical to the legacy single-file
        // parse, so existing single-file behavior is preserved; reachable
        // imported files are folded into the same arena.
        match parse_project(&path) {
            Ok(project) => {
                println!("Parsed: {}", path.display());
                for warning in &project.warnings {
                    eprintln!("{warning}");
                }
                t_ast = Some(project.arena);
            }
            Err(e) => {
                eprintln!("Parse error: {e}");
                process::exit(1);
            }
        }
    }

    let Some(arena) = t_ast else {
        eprintln!("Internal error: parse phase did not produce AST");
        process::exit(1);
    };

    let mut typed_context = None;

    if need_codegen || need_analyze {
        match type_check(arena) {
            Err(e) => {
                eprintln!("Type checking failed: {e}");
                process::exit(1);
            }
            Ok(tctx) => {
                // A036's budget is the stack this build will actually emit, not
                // the default one. Analysis runs ahead of code generation on
                // every `infc` path that reaches it, which is what keeps a
                // program whose frame exceeds the stack a diagnostic here rather
                // than a panic in frame layout later.
                match analyze_with_options(&tctx, AnalysisOptions { stack_budget_bytes: layout.stack_size() }) {
                    Err(e) => {
                        eprintln!("{e}");
                        process::exit(1);
                    }
                    Ok(result) => {
                        if result.has_findings() {
                            eprintln!("{result}");
                        }
                    }
                }
                typed_context = Some(tctx);
                println!("Analyzed: {}", path.display());
            }
        }
    }

    // Before external resolution, which is what makes this the refusal a mixed
    // proof build hears rather than the resolver's mixed-program one. See
    // `host_import_proof_refusal` for why that ordering is worth stating.
    //
    // The predicate is "this build writes a `.v`", which takes both halves: the
    // artifact is requested by `generate_v_output` and written only under
    // `need_codegen`. On the request alone it would also refuse `--analyze
    // --mode proof`, a run `normalize_args` has just warned writes no `.v` — an
    // artifact that is never written is not one this refusal is owed about.
    //
    // The request is read rather than the mode, for the reason
    // `external_spec_policy` gives: `--mode compile -v` is a supported spelling
    // that does write a `.v`, and a gate on the mode alone would let it through
    // with the imports quietly assumed away. `normalize_args` has already set
    // the flag for every proof-mode spelling, so one predicate covers both.
    if need_codegen
        && args.generate_v_output
        && let Some(tctx) = &typed_context
        && let Some(refusal) = host_import_proof_refusal(&host_import_labels(tctx))
    {
        eprintln!("{refusal}");
        process::exit(1);
    }

    // The target's refusal of a host import, asked before resolution for the
    // reason the proof refusal is: resolution and the allowlist below each have
    // a refusal whose remedy leads into this one — bind every extern to
    // `host::…`, admit the import — and at a target that binds no host, a
    // reader who follows either buys another build and, for the allowlist, a
    // policy entry nothing can use. The words are code generation's, which
    // still asks the same question as its own backstop.
    if need_codegen
        && let Some(tctx) = &typed_context
        && let Err(refusal) =
            inference_wasm_codegen::check_host_import_target_support(tctx, target)
    {
        eprintln!("Error: {refusal}");
        process::exit(1);
    }

    // Resolve every external `.wasm` the program binds, ahead of codegen, so a
    // resolution or validation failure aborts before any output is produced.
    let manifest_deps = match parse_manifest_deps(&args.wasm_deps) {
        Ok(deps) => deps,
        Err(e) => {
            eprintln!("External module resolution failed: {e}");
            process::exit(1);
        }
    };
    let externals = match &typed_context {
        Some(tctx) if need_codegen => {
            match resolve_externals(tctx, &args.wasm_lib_dirs, &manifest_deps) {
                Ok(externals) => externals,
                Err(e) => {
                    eprintln!("External module resolution failed: {e}");
                    process::exit(1);
                }
            }
        }
        _ => ResolvedExternals::default(),
    };

    // The registration caps, asked of the declarations the program made rather
    // than of the import section they become. `spacewasm::check` over the
    // finished bytes remains the backstop and the two run one body, so this can
    // only bring the same finding forward — to a point where it can still name
    // the `external fn` the author wrote instead of a two-level string in a
    // module no file holds.
    //
    // Caps before policy, because an allowlist entry has to spell the import's
    // final name. Asked the other way round, the allowlist tells the reader to
    // admit a name no SpaceWasm embedder can register, the rename the caps then
    // ask for leaves that entry matching nothing, and the renamed import is
    // refused by the allowlist again: three builds, and a dead policy entry.
    //
    // Variant equality rather than a predicate: the reason is on
    // `spacewasm::check`, which is also where the post-link caller's identical
    // gate points.
    if target == inference_wasm_codegen::Target::SpaceWasm {
        let declared = externals
            .host_imports
            .iter()
            .map(
                |import| inference_target_conformance::spacewasm::DeclaredImportFacts {
                    module: &import.module,
                    field: &import.field,
                    params: import.signature.params.len(),
                    results: import.signature.results.len(),
                },
            )
            .collect::<Vec<_>>();
        if let Err(violations) =
            inference_target_conformance::spacewasm::check_host_imports(&declared)
        {
            // The subject completes "the host imports … declares", which the
            // declaration-level header opens with, so it names the program and
            // not a file: at this point in the build there is no file, and the
            // declarations are in the source whatever the output was to be
            // called.
            eprintln!("{}", violations.render("this program", "No file was written."));
            process::exit(1);
        }
    }

    // The allowlist is applied to what resolution produced, not to the
    // declarations: a `(module, field)` two files declare is one import to an
    // embedder, and a policy that refused it twice would ask for the same edit
    // twice.
    if let Some(refusal) =
        host_import_allowlist_refusal(host_allowlist.as_ref(), &externals.host_imports)
    {
        eprintln!("{refusal}");
        process::exit(1);
    }

    let external_modules = &externals.modules;
    if need_codegen {
        let Some(tctx) = typed_context else {
            eprintln!("Internal error: type check phase did not produce typed context");
            process::exit(1);
        };
        let profile = BuildProfile::default();
        let mode: inference_wasm_codegen::CompilationMode =
            args.mode.unwrap_or(CliMode::Compile).into();
        let opt_level = profile.resolve_opt_level(target, mode);
        let source_fname = source_fname.as_str();
        let codegen_output = match inference_wasm_codegen::codegen(
            &tctx,
            source_fname,
            inference_wasm_codegen::CodegenOptions {
                target,
                mode,
                opt_level,
                features: emit_features,
                layout,
            },
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Codegen failed: {e}");
                process::exit(1);
            }
        };
        println!("Codegen complete");

        // Held to the target's instruction set one file at a time, while each
        // artifact is still its own file and a refusal can say which one it is
        // about. After the merge there is only one module.
        if target.requires_wasm1_externals()
            && let Some(refusal) = foreign_module_refusal(target, external_modules)
        {
            eprintln!("{refusal}");
            process::exit(1);
        }

        // Fold the resolved external modules into the codegen output, or — for a
        // program whose externs are host imports — ship the codegen bytes with
        // the imports intact. The merge branch produces a single self-contained
        // module with no cross-module imports: each external is paired with the
        // logical module it was bound under, so the merge matches each import's
        // recorded `(module, field)` against the right external. With no externs
        // this is a byte-identical pass-through.
        //
        // Through `link_resolved` rather than the linker entry point directly,
        // because resolution's answer decides which of two paths the artifact
        // takes: a program whose externs are host imports has nothing to merge
        // and must keep the imports it emitted, checked against the declarations
        // that asked for them.
        //
        // The warning-carrying entry point, not the discarding `link`: the merge
        // reports where its own guarantee stops short of what a reader would
        // assume, and a diagnostic nobody prints is one nobody acts on.
        //
        // The checked write-set mode: every declaration this program made is in
        // `externals.contracts`, so each merged body is held to what its
        // `external fn` says it may write through — and an import no declaration
        // covers is held to writing nothing at all.
        let external_specs = external_spec_policy(
            args.mode.unwrap_or(CliMode::Compile),
            args.generate_v_output,
            args.adopt_external_specs,
        );
        let linked = match link_resolved(
            codegen_output.wasm(),
            &externals,
            &LinkOptions { external_specs },
        ) {
            Ok(linked) => linked,
            Err(e) => {
                eprintln!("Link step failed: {e}");
                process::exit(1);
            }
        };
        for warning in &linked.warnings {
            eprintln!("warning: {warning}");
        }
        let wasm_owned = linked.wasm;
        if !external_modules.is_empty() {
            println!("Linked {} external module(s)", external_modules.len());
        }
        // Mutually exclusive with the line above: a program that binds both
        // kinds is refused at resolution, so an artifact either merged modules
        // or kept imports and never both. Two independent `if`s rather than an
        // `else`, because the exclusivity is the resolver's invariant and not
        // this block's — an `else` would silently hide the second line if that
        // ever changed.
        if let Some(inventory) =
            host_import_inventory(&externals.host_imports, host_allowlist.is_some())
        {
            println!("{inventory}");
        }

        // The Stellar value ABI is a post-link rewrite, not an emission: the
        // bytes above are the same module the default target produces, and this
        // is the only step that makes them a contract. It runs before any file
        // is written so a refusal here leaves no artifact, exactly like the
        // translation below.
        //
        // The rewrite runs whether or not a file is asked for, so no refusal it
        // owns is skipped by a build that writes nothing; only the summary is
        // withheld, because it describes an artifact and reports its size.
        let wasm_owned = if target == inference_wasm_codegen::Target::Stellar {
            match inference_stellar_abi::rewrite(
                &wasm_owned,
                codegen_output.export_signatures(),
                inference_stellar_abi::STELLAR_ENV_PROTOCOL,
            ) {
                Ok(contract) => {
                    if args.generate_wasm_output {
                        println!(
                            "{}",
                            stellar_summary(codegen_output.export_signatures(), contract.len())
                        );
                    }
                    contract
                }
                Err(e) => {
                    eprintln!("Stellar contract rewrite failed: {e}");
                    process::exit(1);
                }
            }
        } else {
            wasm_owned
        };
        let wasm_bytes = wasm_owned.as_slice();

        // The conformance check runs on the bytes that ship: after the rewrite
        // branch above, so a target that both rewrites and checks checks what
        // it produced, and before the translation and every file write, so a
        // refusal leaves nothing on disk. SpaceWasm performs no rewrite, which
        // is what makes the two orderings indistinguishable here — the
        // placement is for the target that comes after it.
        //
        // The summary is printed whatever the build was asked to write, unlike
        // `stellar_summary` next door, which is withheld from a build that
        // writes no file because it reports that file's size. This line reports
        // properties of the module, which a `--codegen` run has just as much as
        // a writing one.
        //
        // Variant equality rather than a predicate: the reason is on
        // `spacewasm::check`, which is also where `infs`'s identical gate in
        // `wasm_opt.rs` points.
        if target == inference_wasm_codegen::Target::SpaceWasm {
            match inference_target_conformance::spacewasm::check(wasm_bytes) {
                Ok(report) => {
                    println!("{}", report.summary_line());
                    for warning in report.budget_warnings() {
                        eprintln!("warning: {warning}");
                    }
                }
                Err(violations) => {
                    let subject = if args.generate_wasm_output {
                        output_path
                            .join(format!("{source_fname}.wasm"))
                            .display()
                            .to_string()
                    } else {
                        String::from("the linked module")
                    };
                    eprintln!("{}", violations.render(&subject, "No file was written."));
                    process::exit(1);
                }
            }
        }

        // Run the Rocq translation *before* writing any file: a `wasm_to_v`
        // rejection (e.g. a spec or file named after a Rocq stdlib type) must not
        // leave a runnable `.wasm` on disk at a non-zero exit. The translation
        // output is held in memory and the artifacts are written only once the
        // whole requested pipeline has succeeded, so every rejection path exits
        // with no partial artifact. When `-v` is not requested this is skipped and
        // the `.wasm` write below is the first and only output step.
        let v_output = if args.generate_v_output {
            // The spec-function indices codegen records are in the *pre-link*
            // space; the linker rewrote the embedded `inference.spec_funcs`
            // section into the post-link space. When externals were merged the
            // pre-link map is stale, so defer entirely to the embedded post-link
            // section (an empty explicit map makes the translator adopt it).
            // With no externals the merge is a byte-identical pass-through and
            // the explicit map still cross-checks against the embedded one.
            let empty_spec_funcs = inference::FxHashMap::default();
            let explicit_spec_funcs = if external_modules.is_empty() {
                codegen_output.spec_func_indices_by_spec()
            } else {
                &empty_spec_funcs
            };
            // Same policy for the `inference.hspecs` obligations as for
            // `inference.spec_funcs`: with externals the pre-link map is stale
            // (the linker rewrote the embedded section), so pass an empty map
            // and defer to the embedded post-link section; without externals
            // the merge is a byte-identical pass-through and the explicit map
            // still cross-checks against the embedded one.
            let empty_hspecs = inference::HSpecMap::default();
            let explicit_hspecs = if external_modules.is_empty() {
                codegen_output.hspecs()
            } else {
                &empty_hspecs
            };
            match wasm_to_v(
                source_fname,
                wasm_bytes,
                explicit_spec_funcs,
                explicit_hspecs,
            ) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprint_translation_error(&e);
                    process::exit(1);
                }
            }
        } else {
            None
        };

        if args.generate_wasm_output {
            let wasm_file_path = output_path.join(format!("{source_fname}.wasm"));
            if let Err(e) = fs::create_dir_all(&output_path) {
                eprintln!("Failed to create output directory: {e}");
                process::exit(1);
            }
            if let Err(e) = fs::write(&wasm_file_path, wasm_bytes) {
                eprintln!("Failed to write WASM file: {e}");
                process::exit(1);
            }
            println!("WASM generated at: {}", wasm_file_path.to_string_lossy());
        }
        if let Some(v_output) = v_output {
            let v_file_path = output_path.join(format!("{source_fname}.v"));
            if let Err(e) = fs::create_dir_all(&output_path) {
                eprintln!("Failed to create output directory: {e}");
                process::exit(1);
            }
            if let Err(e) = fs::write(&v_file_path, v_output) {
                eprintln!("Failed to write V file: {e}");
                process::exit(1);
            }
            println!("V generated at: {}", v_file_path.to_string_lossy());
        }
    }
    process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_wasm_codegen::MemoryLayoutError;
    use std::path::{Path, PathBuf};

    fn make_args(parse: bool, analyze: bool, codegen: bool) -> Cli {
        Cli {
            path: Some(PathBuf::from("test.inf")),
            out_dir: None,
            parse,
            analyze,
            codegen,
            generate_wasm_output: false,
            generate_v_output: false,
            mode: None,
            wasm_lib_dirs: Vec::new(),
            wasm_deps: Vec::new(),
            host_imports: None,
            target: None,
            wasm_features: Vec::new(),
            memory_pages: None,
            stack_size: None,
            adopt_external_specs: false,
            commit_hash: false,
            abi_version: false,
        }
    }

    /// Every cell of the external-specification policy, so no arm can be
    /// widened or narrowed without a failure naming the pairing it changed.
    ///
    /// The two `false` rows are the ones that carry the design: the report is
    /// owed to a build that writes a `.v`, whatever mode produced it, and to no
    /// other. Keying the policy on the mode instead would silence
    /// `--mode compile -v`, which does write a `.v` that omits the library's
    /// obligations, and would report on a plain compile that consumes nothing.
    #[test]
    fn external_spec_policy_maps_every_cell() {
        assert_eq!(
            external_spec_policy(CliMode::Proof, true, true),
            ExternalSpecPolicy::Adopt,
            "a proof build that asked to adopt must adopt"
        );
        assert_eq!(
            external_spec_policy(CliMode::Proof, true, false),
            ExternalSpecPolicy::Warn,
            "a proof build that did not ask must be told what it did not get"
        );
        assert_eq!(
            external_spec_policy(CliMode::Compile, true, false),
            ExternalSpecPolicy::Warn,
            "`--mode compile -v` writes a .v, so it is owed the same report"
        );
        assert_eq!(
            external_spec_policy(CliMode::Compile, false, false),
            ExternalSpecPolicy::Ignore,
            "a build that writes no .v must not report obligations nothing would consume"
        );
        assert_eq!(
            external_spec_policy(CliMode::Proof, false, false),
            ExternalSpecPolicy::Ignore,
            "the report follows the artifact, not the mode"
        );
        assert_eq!(
            external_spec_policy(CliMode::Compile, false, true),
            ExternalSpecPolicy::Ignore,
            "the unreachable compile-mode adopt pairing must fail safe"
        );
    }

    /// A resolved host import with the empty signature, for the message tests
    /// below: nothing they assert reads the signature, and spelling one out
    /// would suggest it does.
    fn host_import(module: &str, field: &str) -> HostImport {
        HostImport {
            module: module.to_string(),
            field: field.to_string(),
            signature: inference::wasm_link::DeclaredSignature {
                params: Vec::new(),
                results: Vec::new(),
            },
        }
    }

    /// The three states `--host-imports` has to distinguish, read back off clap
    /// rather than assumed.
    ///
    /// The middle row is the one the `Option` exists for and the one clap's
    /// shape decides: `--host-imports=` arrives as a single empty value, not as
    /// no values, so "declared and empty" is `Some([""])` on the way in and has
    /// to be recognised rather than filtered. Pinning it here means a clap
    /// upgrade that starts yielding `Some([])` — or `None` — fails on the parser
    /// instead of silently turning the strictest policy into the absent one.
    ///
    /// The last case is what `require_equals` buys: without it clap reads the
    /// source path as the flag's value, and the build then fails on a missing
    /// file rather than on the policy the author was writing.
    #[test]
    fn host_imports_flag_round_trips_its_three_states() {
        let absent = Cli::try_parse_from(["infc", "a.inf"]).unwrap();
        assert_eq!(absent.host_imports, None, "no flag is no policy");

        let empty = Cli::try_parse_from(["infc", "a.inf", "--host-imports="]).unwrap();
        assert_eq!(
            empty.host_imports,
            Some(vec![String::new()]),
            "`--host-imports=` must stay distinguishable from an absent flag"
        );

        let listed = Cli::try_parse_from(["infc", "a.inf", "--host-imports=env.clock_ms,f.command"])
            .unwrap();
        assert_eq!(
            listed.host_imports,
            Some(vec!["env.clock_ms".to_string(), "f.command".to_string()]),
            "one occurrence carries a comma separated list"
        );

        assert!(
            Cli::try_parse_from(["infc", "--host-imports", "a.inf"]).is_err(),
            "a bare --host-imports must not swallow the source path"
        );
    }

    /// The same three states after parsing, plus the entries that are not a host
    /// import at all.
    ///
    /// The empty-allowlist row is asserted as an empty *map*, not as an error:
    /// the single empty entry is a policy and not a typo. The malformed rows are
    /// what separate it from a stray comma — an entry with no dot, an empty
    /// half, or a second dot could never match an import module string.
    ///
    /// The paste row is what trimming is for: the flag's own list separator is
    /// a comma, but the build-log line these names are read off of separates
    /// them with `, `, and a policy that refused a quoted paste of it would
    /// refuse it over a character the reader cannot see inside the backticks
    /// quoting it.
    ///
    /// The `::` rows are transcriptions of a `use { f } from host::m;` clause,
    /// and none may reach the shared message. `env::clock_ms` would be called
    /// malformed without being told what is wrong with it, and
    /// `host::env.clock_ms` is not malformed at all under the shared rule — it
    /// parses as a module named `host::env`, which no declaration can produce,
    /// so it would be accepted here and refused two phases later by the
    /// allowlist, reading as a broken mechanism rather than as a typo.
    ///
    /// Each row pins the fault the entry actually carries and the one spelling
    /// its author meant, whole: a refusal that gave every author the same
    /// `env.clock_ms` example would pass a test asserting only the rule. The
    /// `host::env.clock_ms` row is the one where naming the separator would be
    /// wrong, since that entry already uses a dot; the three-name row is the
    /// one where a correction would be a guess, and falls back to the example.
    #[test]
    fn parse_host_import_allowlist_reads_the_three_states() {
        assert!(
            parse_host_import_allowlist(&[String::new()])
                .unwrap()
                .is_empty(),
            "`--host-imports=` is a declared and empty policy, not a malformed entry"
        );

        let listed = parse_host_import_allowlist(&[
            "env.clock_ms".to_string(),
            "env.sleep_ms".to_string(),
            "fprime_core.telemetry".to_string(),
        ])
        .unwrap();
        assert_eq!(listed.len(), 2, "entries are grouped by import module");
        assert_eq!(
            listed["env"].iter().cloned().collect::<Vec<_>>(),
            vec!["clock_ms".to_string(), "sleep_ms".to_string()]
        );

        let pasted =
            parse_host_import_allowlist(&["env.clock_ms".to_string(), " env.sleep_ms".to_string()])
                .unwrap();
        assert_eq!(
            pasted["env"].iter().cloned().collect::<Vec<_>>(),
            vec!["clock_ms".to_string(), "sleep_ms".to_string()],
            "a list pasted out of the inventory line keeps its `, ` separators"
        );

        for malformed in ["env", ".clock_ms", "env.", "a.b.c"] {
            let err = parse_host_import_allowlist(&[
                "fprime_core.telemetry".to_string(),
                malformed.to_string(),
            ])
            .expect_err("an entry that is not a `module.field` pair must be refused");
            let rendered = err.to_string();
            assert!(
                rendered.contains(&format!("entry `{malformed}`")),
                "the refusal quotes the whole entry it rejected: {rendered}"
            );
            assert!(
                rendered.contains("`module.field` form"),
                "the refusal states the form an entry takes: {rendered}"
            );
        }

        let separator = "the separator here is a dot, not `::`.";
        let prefix = "the `host::` prefix is not part of an import name.";
        let example = "For example, `use { clock_ms } from host::env;` is written `env.clock_ms`.";
        for (transcribed, fault, correction) in [
            ("env::clock_ms", separator, "Write it `env.clock_ms`."),
            ("fprime_core::telemetry", separator, "Write it `fprime_core.telemetry`."),
            (
                "host::fprime_core::telemetry",
                "the separator here is a dot, not `::`, and the `host::` prefix is not part of \
                 an import name.",
                "Write it `fprime_core.telemetry`.",
            ),
            ("host::env.clock_ms", prefix, "Write it `env.clock_ms`."),
            ("host::env", prefix, example),
            ("a::b::c", separator, example),
        ] {
            let err = parse_host_import_allowlist(&[
                "fprime_core.telemetry".to_string(),
                transcribed.to_string(),
            ])
            .expect_err("an entry written with the source's path separator must be refused");
            let rendered = err.to_string();
            assert!(
                rendered.starts_with(&format!(
                    "invalid `--host-imports` entry `{transcribed}`: {fault} "
                )),
                "the refusal quotes the whole entry and names the fault it carries: {rendered}"
            );
            assert!(
                rendered.ends_with(correction),
                "the refusal ends at the spelling `{transcribed}` was meant to be: {rendered}"
            );
        }

        for stray in ["", "  "] {
            let err = parse_host_import_allowlist(&[
                "fprime_core.telemetry".to_string(),
                stray.to_string(),
            ])
            .expect_err("a list that names something and also carries a blank entry is a typo");
            let rendered = err.to_string();
            assert!(
                rendered.contains("the list has an empty entry, which is a stray or trailing \
                                   comma"),
                "a blank entry is named rather than quoted back as nothing: {rendered}"
            );
            assert!(
                rendered.contains("`--host-imports=` on its own is the empty allowlist"),
                "the refusal separates the typo from the policy it looks like: {rendered}"
            );
        }
    }

    /// One host import reads as one, two read as two.
    ///
    /// The wording was written for a single import, and a program binding
    /// several must not be told about "its" behavior — nor asked to remove "the
    /// host imports" when it binds one. Nothing else in the message varies, so
    /// the agreement is the whole of what this function decides, and both ends
    /// of the sentence are asserted at both arities.
    ///
    /// The remedy is asserted whole at both, too. It asks for *every* request
    /// for a `.v` to go, since a build passing both `-v` and `--mode proof` that
    /// drops one is refused again; and it names the manifest setting, since a
    /// project build receives `--mode proof` from `infs` and its author typed no
    /// flag to drop.
    #[test]
    fn host_import_proof_refusal_agrees_with_its_subject() {
        assert_eq!(
            host_import_proof_refusal(&[]),
            None,
            "no host import, no refusal"
        );

        let one = host_import_proof_refusal(&["`env`.`clock_ms`".to_string()]).unwrap();
        assert!(
            one.contains("`env`.`clock_ms` is satisfied by the embedder, so its behavior"),
            "{one}"
        );
        assert!(
            one.contains("or remove the host import from the program."),
            "one import is removed in the singular: {one}"
        );

        let many = host_import_proof_refusal(&[
            "`env`.`clock_ms`".to_string(),
            "`fprime_core`.`telemetry`".to_string(),
        ])
        .unwrap();
        assert!(
            many.contains(
                "`env`.`clock_ms`, `fprime_core`.`telemetry` are satisfied by the embedder, so \
                 their behavior"
            ),
            "{many}"
        );
        assert!(
            many.contains("or remove the host imports from the program."),
            "two imports are removed in the plural: {many}"
        );

        for refusal in [&one, &many] {
            assert!(
                refusal.contains(
                    "Build this program without a proof artifact, or remove the host import"
                ),
                "the remedy is the artifact, not a mode: {refusal}"
            );
            assert!(
                refusal.contains(
                    "`-v` and `--mode proof` each request a `.v` on their own, so drop every one \
                     of them this build passed;"
                ),
                "the remedy asks for every request to go, not one of two: {refusal}"
            );
            assert!(
                refusal.ends_with(
                    "`infs build` passes `--mode proof` on a project's behalf when its \
                     Inference.toml sets `[build] mode = \"proof\"`, and there the request to \
                     drop is that setting: make it `\"compile\"`."
                ),
                "a project build's author is told where the request they never typed comes \
                 from: {refusal}"
            );
            assert!(
                !refusal.contains("whichever"),
                "a build passing both requests must not be told to drop one: {refusal}"
            );
        }
    }

    /// Asserts that `rendered` names the `[host-imports]` table `infs` fills the
    /// flag from, in the present tense, and spells the table edit — the half of
    /// the remedy a project build's reader acts on — in the empty policy's own
    /// sentence when the allowlist is empty.
    fn assert_names_the_host_imports_table(rendered: &str, empty_policy: bool) {
        let table_sentence = if empty_policy {
            "`infs` fills the flag from the `[host-imports]` table in Inference.toml, \
             where this empty policy is the table with no keys and the same edit is to add"
        } else {
            "`infs` fills the flag from the `[host-imports]` table in Inference.toml, \
             where the same edit is to add"
        };
        assert!(
            rendered.contains(table_sentence),
            "every branch names the table `infs` fills the flag from, and its edit: {rendered}"
        );
        for retired in ["table exists", "today `infs`"] {
            assert!(
                !rendered.contains(retired),
                "the table exists, so `{retired}` must not come back: {rendered}"
            );
        }
    }

    /// The edit an allowlist refusal asks for follows the allowlist it was
    /// given: a module with no entry needs the key written, a module that has
    /// one needs names added to the array it already has.
    ///
    /// The compiler is holding the allowlist, so a message that made the reader
    /// check would be asking for work already done — and the two edits are not
    /// interchangeable in a TOML table.
    ///
    /// Both branches are asserted to name `--host-imports` and to spell the
    /// flag's own `module.field` edit, because that is the only remedy a direct
    /// `infc` caller can perform: they have no manifest, and `--host-imports`
    /// refuses the TOML array spelling the same sentence hands an `infs`
    /// reader. Both are asserted to name the `[host-imports]` table and its edit
    /// too, in the present tense, for the `infs` reader whose flag was filled
    /// from it — and neither may carry the retired sentence that told that
    /// reader the table did not exist yet.
    ///
    /// The source-level half of the remedy is asserted to name the bindings
    /// rather than to quote a `use` clause. The partial-admission row is what
    /// separates the two: `fields` is the unadmitted subset of a deduplicated
    /// set, so a clause rendered from it can be one no file contains, and a
    /// reader who greps for it finds nothing. For the same reason every row
    /// asserts "every … binding" and none "the … binding": a deduplicated name
    /// may stand for a clause in each of several files, which the refusal
    /// cannot count. The two- and three-name rows pin the names as one list
    /// under that quantifier.
    #[test]
    fn host_import_allowlist_refusal_branches_on_the_module_entry() {
        let declared = [host_import("env", "clock_ms")];

        assert_eq!(
            host_import_allowlist_refusal(None, &declared),
            None,
            "no allowlist is no policy"
        );

        let listed = parse_host_import_allowlist(&["env.clock_ms".to_string()]).unwrap();
        assert_eq!(
            host_import_allowlist_refusal(Some(&listed), &declared),
            None,
            "an admitted import is not refused"
        );

        let other_module =
            parse_host_import_allowlist(&["fprime_core.telemetry".to_string()]).unwrap();
        let refusal = host_import_allowlist_refusal(Some(&other_module), &declared).unwrap();
        assert!(
            refusal.contains("add `env = [\"clock_ms\"]`"),
            "a module with no entry needs the key written: {refusal}"
        );

        let same_module = parse_host_import_allowlist(&["env.sleep_ms".to_string()]).unwrap();
        let refusal = host_import_allowlist_refusal(Some(&same_module), &declared).unwrap();
        assert!(
            refusal.contains("add `\"clock_ms\"` to the `env` entry"),
            "a module that already has an entry needs a name added to it: {refusal}"
        );

        let empty = parse_host_import_allowlist(&[String::new()]).unwrap();
        let refusal = host_import_allowlist_refusal(Some(&empty), &declared).unwrap();
        assert!(
            refusal.contains("the allowlist is present and empty, which forbids every host import"),
            "an empty allowlist says so rather than reporting an empty list: {refusal}"
        );
        assert!(
            refusal.contains("the same edit is to add `env = [\"clock_ms\"]` under it."),
            "the empty policy's table has no keys, so its edit writes one: {refusal}"
        );

        for allowlist in [&other_module, &same_module, &empty] {
            let rendered = host_import_allowlist_refusal(Some(allowlist), &declared).unwrap();
            assert!(
                rendered.contains(
                    "Add `env.clock_ms` to `--host-imports` to admit it, or drop every \
                     `host::env` binding of `clock_ms`."
                ),
                "every branch names the one mechanism this build has, in its own syntax: \
                 {rendered}"
            );
            assert_names_the_host_imports_table(&rendered, allowlist.is_empty());
        }

        let both = [host_import("env", "clock_ms"), host_import("env", "sleep_ms")];
        let rendered = host_import_allowlist_refusal(Some(&same_module), &both).unwrap();
        assert!(
            rendered.contains("or drop every `host::env` binding of `clock_ms`."),
            "only the unadmitted name is named, and not as a clause: {rendered}"
        );
        assert!(
            !rendered.contains("the `host::env` binding"),
            "a deduplicated name must not be told it has exactly one binding: {rendered}"
        );
        assert!(
            !rendered.contains("use {"),
            "no `use` clause is quoted, since the one rendered here is in no file: {rendered}"
        );

        let two = [host_import("env", "clock_ms"), host_import("env", "sleep_ms")];
        let grouped = host_import_allowlist_refusal(Some(&other_module), &two).unwrap();
        assert!(
            !grouped.contains("\n\n"),
            "one module is one paragraph however many of its fields are unadmitted: {grouped}"
        );
        for fragment in [
            "host imports `env`.`clock_ms`, `env`.`sleep_ms` are not in this build's",
            "Add `env.clock_ms,env.sleep_ms` to `--host-imports` to admit them",
            "drop every `host::env` binding of `clock_ms` and `sleep_ms`.",
            "add `env = [\"clock_ms\", \"sleep_ms\"]`",
        ] {
            assert!(
                grouped.contains(fragment),
                "the grouped refusal must carry `{fragment}`:\n{grouped}"
            );
        }

        let three = [
            host_import("env", "clock_ms"),
            host_import("env", "sleep_ms"),
            host_import("env", "wake"),
        ];
        let listed = host_import_allowlist_refusal(Some(&other_module), &three).unwrap();
        assert!(
            listed.contains(
                "drop every `host::env` binding of `clock_ms`, `sleep_ms`, and `wake`."
            ),
            "three names read as a list, not as one clause carrying them: {listed}"
        );

        // A second module is a second paragraph, which is what makes the
        // grouping a grouping rather than one block for everything.
        let across = [host_import("env", "clock_ms"), host_import("io", "write")];
        let split = host_import_allowlist_refusal(Some(&other_module), &across).unwrap();
        assert_eq!(
            split.matches("\n\n").count(),
            1,
            "two modules are two paragraphs: {split}"
        );
    }

    /// The inventory line names every pair and says whether a policy admitted
    /// them.
    ///
    /// The qualifier is the only difference between the two lines a reviewer
    /// sees, and the pairs are rendered the way `--host-imports` spells them so
    /// a name read out of a build log can be pasted straight back into the flag.
    #[test]
    fn host_import_inventory_marks_an_unpoliced_build() {
        assert_eq!(host_import_inventory(&[], false), None);

        let declared = [
            host_import("env", "clock_ms"),
            host_import("fprime_core", "telemetry"),
        ];
        assert_eq!(
            host_import_inventory(&declared, true).unwrap(),
            "host imports: env.clock_ms, fprime_core.telemetry"
        );
        assert_eq!(
            host_import_inventory(&declared, false).unwrap(),
            "host imports (no allowlist): env.clock_ms, fprime_core.telemetry"
        );
    }

    #[test]
    fn normalize_sets_full_pipeline_when_no_flags() {
        let mut args = make_args(false, false, false);
        normalize_args(&mut args);
        assert!(args.codegen);
        assert!(args.generate_wasm_output);
        assert!(!args.generate_v_output);
    }

    #[test]
    fn normalize_does_not_override_explicit_parse() {
        let mut args = make_args(true, false, false);
        normalize_args(&mut args);
        assert!(!args.codegen);
        assert!(!args.generate_wasm_output);
    }

    #[test]
    fn normalize_does_not_override_explicit_analyze() {
        let mut args = make_args(false, true, false);
        normalize_args(&mut args);
        assert!(!args.codegen);
    }

    #[test]
    fn normalize_does_not_override_explicit_codegen() {
        let mut args = make_args(false, false, true);
        normalize_args(&mut args);
        assert!(args.codegen);
        assert!(!args.generate_wasm_output);
    }

    #[test]
    fn normalize_proof_mode_implies_v_output() {
        let mut args = make_args(false, false, false);
        args.mode = Some(CliMode::Proof);
        normalize_args(&mut args);
        assert!(
            args.codegen,
            "proof mode should still trigger default codegen"
        );
        assert!(
            args.generate_wasm_output,
            "proof mode should still emit wasm"
        );
        assert!(
            args.generate_v_output,
            "proof mode must imply -v so the .v artifact is written"
        );
        assert_eq!(
            args.mode,
            Some(CliMode::Proof),
            "explicit proof mode must be preserved"
        );
    }

    #[test]
    fn normalize_dash_v_implies_proof_mode() {
        let mut args = make_args(false, false, false);
        args.generate_v_output = true;
        normalize_args(&mut args);
        assert_eq!(
            args.mode,
            Some(CliMode::Proof),
            "-v alone must promote effective mode to proof so specs survive codegen"
        );
        assert!(args.generate_v_output);
    }

    #[test]
    fn normalize_explicit_compile_plus_v_keeps_compile() {
        let mut args = make_args(false, false, false);
        args.mode = Some(CliMode::Compile);
        args.generate_v_output = true;
        normalize_args(&mut args);
        assert_eq!(
            args.mode,
            Some(CliMode::Compile),
            "explicit --mode compile must not be overridden by -v"
        );
        assert!(
            args.generate_v_output,
            "explicit -v must be preserved even in compile mode"
        );
    }

    #[test]
    fn normalize_no_flags_resolves_mode_to_compile() {
        let mut args = make_args(false, false, false);
        normalize_args(&mut args);
        assert_eq!(
            args.mode,
            Some(CliMode::Compile),
            "absence of --mode and -v must resolve to compile"
        );
        assert!(!args.generate_v_output);
    }

    /// Returns the path to the test data directory.
    #[allow(dead_code)]
    pub(crate) fn get_test_data_path() -> std::path::PathBuf {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .parent() // inference
            .unwrap()
            .join("test_data")
    }

    /// Returns the path to the output directory for test artifacts.
    #[allow(dead_code)]
    fn get_out_path() -> std::path::PathBuf {
        get_test_data_path().parent().unwrap().join("out")
    }

    #[test]
    fn parse_manifest_deps_binds_name_to_path() {
        let deps =
            parse_manifest_deps(&["arith=/libs/arith.wasm".to_string()]).expect("should parse");
        assert_eq!(deps.get("arith"), Some(Path::new("/libs/arith.wasm")));
    }

    #[test]
    fn parse_manifest_deps_accepts_multiple_entries() {
        let deps = parse_manifest_deps(&[
            "arith=/libs/arith.wasm".to_string(),
            "crypto=/vendor/sha256.wasm".to_string(),
        ])
        .expect("should parse");
        assert_eq!(deps.get("arith"), Some(Path::new("/libs/arith.wasm")));
        assert_eq!(deps.get("crypto"), Some(Path::new("/vendor/sha256.wasm")));
    }

    #[test]
    fn parse_manifest_deps_preserves_path_with_equals() {
        // Only the first `=` separates name from path; later ones belong to the
        // path so values like `a=b=c` survive intact.
        let deps = parse_manifest_deps(&["arith=/odd=dir/arith.wasm".to_string()])
            .expect("should parse");
        assert_eq!(deps.get("arith"), Some(Path::new("/odd=dir/arith.wasm")));
    }

    #[test]
    fn parse_manifest_deps_rejects_missing_separator() {
        let err = parse_manifest_deps(&["arith".to_string()]).unwrap_err();
        assert!(err.to_string().contains("expected `<name>=<path>`"));
    }

    #[test]
    fn parse_manifest_deps_rejects_empty_name() {
        let err = parse_manifest_deps(&["=/libs/arith.wasm".to_string()]).unwrap_err();
        assert!(err.to_string().contains("module name is empty"));
    }

    #[test]
    fn parse_manifest_deps_empty_input_yields_empty_map() {
        let deps = parse_manifest_deps(&[]).expect("should parse");
        assert!(deps.get("anything").is_none());
    }

    /// `infs` refuses a `[wasm-dependencies]` key under the contract crate's
    /// copy of the reserved segment, and this binary refuses the `--wasm-dep`
    /// under the type checker's. This crate is the one that sees both, so it is
    /// where they are held equal: the tests on either side spell `host` out, and
    /// a rename of one copy alone would pass every one of them.
    #[test]
    fn the_host_segment_infs_refuses_is_the_one_the_compiler_reserves() {
        assert_eq!(
            HOST_SEGMENT,
            inference_compiler_interface::HOST_SEGMENT,
            "the reserved segment has one spelling on both sides of the infs/infc contract"
        );
    }

    #[test]
    fn empty_wasm_lib_path_resolves_like_unset() {
        // H5: a wholly-empty value, and a lone separator, must each yield zero
        // search directories — identical to the variable being unset — rather
        // than injecting the process CWD as a silent `.wasm` search root.
        use std::ffi::OsString;

        let empty = env_search_dirs(&OsString::from(""));
        assert!(empty.is_empty(), "empty value yields no dirs: {empty:?}");

        // A lone PATH list separator (`:` on Unix, `;` on Windows) splits into
        // two empty entries; both must be dropped.
        let list_sep = if cfg!(windows) { ";" } else { ":" };
        let bare = env_search_dirs(&OsString::from(list_sep));
        assert!(bare.is_empty(), "a lone list separator yields no dirs: {bare:?}");
    }

    #[test]
    fn wasm_lib_path_keeps_real_dirs_and_drops_empties() {
        // `"/real/dir<sep>"` (a trailing separator) must keep the real directory
        // and drop only the empty trailing entry.
        use std::ffi::OsString;

        let list_sep = if cfg!(windows) { ";" } else { ":" };
        let value = OsString::from(format!("real{list_sep}"));
        let dirs = env_search_dirs(&value);
        assert_eq!(dirs, [PathBuf::from("real")]);
    }

    fn features(raw: &[&str]) -> anyhow::Result<EmitFeatures> {
        resolve_emit_features(&raw.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn feature_error(raw: &[&str]) -> String {
        features(raw)
            .expect_err("the request must be rejected")
            .to_string()
    }

    #[test]
    fn no_wasm_features_flag_emits_wasm_1_0() {
        assert_eq!(features(&[]).unwrap(), EmitFeatures::default());
    }

    #[test]
    fn bulk_memory_sets_its_emission_flag() {
        assert_eq!(
            features(&["bulk-memory"]).unwrap(),
            EmitFeatures { bulk_memory: true }
        );
    }

    #[test]
    fn unknown_feature_is_rejected_with_the_shared_wording() {
        let err = feature_error(&["simd"]);
        assert!(err.contains("unknown WebAssembly feature"), "{err}");
        // The flag, not the manifest key, is what an `infc` caller must edit.
        assert!(err.contains("`--wasm-features`"), "{err}");
    }

    #[test]
    fn instruction_name_is_rejected_with_the_did_you_mean() {
        let err = feature_error(&["memory.fill"]);
        assert!(err.contains("is an instruction, not a feature"), "{err}");
        assert!(err.contains("write `bulk-memory`"), "{err}");
    }

    #[test]
    fn duplicate_feature_is_rejected() {
        let err = feature_error(&["bulk-memory", "bulk-memory"]);
        assert!(err.contains("listed more than once"), "{err}");
    }

    /// Omitting `--target` must land on the same emission target the flag's
    /// default name selects, not on a second idea of what the default is.
    #[test]
    fn an_absent_target_flag_resolves_like_the_default_name() {
        use inference_compiler_interface::TargetName;

        let absent = resolve_target_flag(None).expect("no target named");
        let named = resolve_target_flag(Some(TargetName::DEFAULT.as_str()))
            .expect("the default name is requestable");
        assert_eq!(absent, named);
        assert_eq!(absent, inference_wasm_codegen::Target::Wasm32);
    }

    /// Every requestable name must reach an emission target through this
    /// function, or the flag accepts a name the compiler then cannot build for.
    #[test]
    fn every_requestable_target_resolves_through_the_flag() {
        use inference_compiler_interface::TargetName;

        for name in TargetName::ALL {
            let target = resolve_target_flag(Some(name.as_str()))
                .unwrap_or_else(|e| panic!("`{}` must resolve: {e}", name.as_str()));
            assert_eq!(name.as_str(), target.as_str());
        }
    }

    #[test]
    fn an_unknown_target_is_rejected_with_the_shared_wording() {
        let err = resolve_target_flag(Some("wasm64"))
            .expect_err("`wasm64` names no target")
            .to_string();
        assert!(err.contains("unknown compilation target"), "{err}");
        // The flag, not the manifest key, is what an `infc` caller must edit.
        assert!(err.contains("`--target`"), "{err}");
    }

    /// Both accepted spellings reach the same flags: `--wasm-features a,b` is the
    /// canonical form and repetition is accepted, so `infs` can forward one comma
    /// list without callers who repeat the flag getting different output.
    #[test]
    fn comma_list_and_repetition_parse_alike() {
        let comma = Cli::try_parse_from(["infc", "x.inf", "--wasm-features", "bulk-memory"])
            .expect("comma form parses");
        let repeated = Cli::try_parse_from([
            "infc",
            "x.inf",
            "--wasm-features",
            "bulk-memory",
            "--wasm-features",
            "bulk-memory",
        ])
        .expect("repetition parses");
        assert_eq!(comma.wasm_features, ["bulk-memory"]);
        assert_eq!(repeated.wasm_features, ["bulk-memory", "bulk-memory"]);
        assert_eq!(
            resolve_emit_features(&comma.wasm_features).unwrap(),
            EmitFeatures { bulk_memory: true }
        );
        // The same name twice is a duplicate however it was spelled.
        assert!(resolve_emit_features(&repeated.wasm_features).is_err());
    }

    #[test]
    fn comma_separated_entries_split_into_separate_names() {
        let cli = Cli::try_parse_from(["infc", "x.inf", "--wasm-features", "bulk-memory,simd"])
            .expect("a comma list parses into entries");
        assert_eq!(cli.wasm_features, ["bulk-memory", "simd"]);
    }

    // Memory layout flags ---

    /// The layout the flags on `argv` resolve to, or the rejection.
    fn layout_from(argv: &[&str]) -> Result<MemoryLayout, MemoryLayoutError> {
        let mut full = vec!["infc", "x.inf"];
        full.extend_from_slice(argv);
        let cli = Cli::try_parse_from(full).expect("the flags under test parse");
        MemoryLayout::resolve(cli.memory_pages, cli.stack_size, MemoryLayoutSource::Flag)
    }

    #[test]
    fn no_memory_flags_yields_the_default_layout() {
        assert_eq!(layout_from(&[]), Ok(MemoryLayout::default()));
    }

    /// Each flag is independently settable, and the unset one keeps its default.
    /// Without this a user who wants a larger memory would have to restate the
    /// stack size — and would silently get a different stack if they got it wrong.
    #[test]
    fn each_memory_flag_can_be_given_alone() {
        let pages_only = layout_from(&["--memory-pages", "4"]).expect("four pages is admissible");
        assert_eq!(pages_only.pages(), 4);
        assert_eq!(
            pages_only.stack_size(),
            MemoryLayout::default().stack_size(),
            "an unset --stack-size must keep the default stack"
        );

        let stack_only =
            layout_from(&["--stack-size", "32768"]).expect("half a page of stack is admissible");
        assert_eq!(
            stack_only.pages(),
            MemoryLayout::default().pages(),
            "an unset --memory-pages must keep the default memory"
        );
        assert_eq!(stack_only.stack_size(), 32_768);
    }

    #[test]
    fn both_memory_flags_together_reach_the_layout() {
        let layout =
            layout_from(&["--memory-pages", "2", "--stack-size", "32768"]).expect("admissible");
        assert_eq!(layout.pages(), 2);
        assert_eq!(layout.stack_size(), 32_768);
    }

    /// A rejection names the flag spelling rather than the manifest one, which is
    /// the only thing the surface changes — the verdict itself is shared.
    #[test]
    fn an_unusable_layout_is_rejected_naming_the_flags() {
        let err = layout_from(&["--memory-pages", "0"]).expect_err("a zero-page memory is refused");
        let rendered = err.to_string();
        assert!(rendered.contains("`--memory-pages`"), "{rendered}");
        assert!(rendered.contains("at least one 64 KiB page"), "{rendered}");

        let err = layout_from(&["--stack-size", "1000"])
            .expect_err("a stack off the frame-alignment grid is refused");
        let rendered = err.to_string();
        assert!(rendered.contains("`--stack-size`"), "{rendered}");
        assert!(
            rendered.contains("multiple of the 16-byte frame alignment"),
            "{rendered}"
        );
    }

    /// A flag value legal on its own is still judged against the layout it
    /// completes to, so the fill-then-check order survives the flag surface.
    #[test]
    fn a_stack_larger_than_the_default_memory_is_rejected_alone() {
        let err = layout_from(&["--stack-size", "131072"])
            .expect_err("a 128 KiB stack does not fit the default single page");
        assert!(
            err.reason.contains("does not fit in the linear memory"),
            "{err}"
        );
        assert!(
            layout_from(&["--memory-pages", "4", "--stack-size", "131072"]).is_ok(),
            "the same stack is fine once --memory-pages makes room for it"
        );
    }

    /// A non-numeric or negative value is refused by the parser rather than
    /// reaching the resolver, so the two flags cannot carry a nonsense value.
    #[test]
    fn memory_flags_reject_a_non_numeric_value() {
        for argv in [
            ["--memory-pages", "many"],
            ["--stack-size", "-16"],
            ["--memory-pages", "1.5"],
        ] {
            assert!(
                Cli::try_parse_from(["infc", "x.inf", argv[0], argv[1]]).is_err(),
                "`{} {}` must not parse",
                argv[0],
                argv[1]
            );
        }
    }
}

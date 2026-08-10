//! Build command for the infs CLI.
//!
//! Compiles Inference source files by delegating to the `infc` compiler
//! via subprocess. This module acts as a lightweight bootstrapper, validating
//! arguments and forwarding them to infc.
//!
//! ## Behavior
//!
//! `infs build` always performs full compilation (parse, analyze, codegen)
//! and writes the WASM binary to disk. The `-v` flag additionally generates
//! a Rocq (.v) translation file. `infs` forwards `-v` and `--mode` to `infc`
//! exactly as the user passed them; it does not synthesize one flag from the
//! other. The `-v` ⇄ `--mode proof` implication lives in
//! `infc::normalize_args`: `--mode proof` makes `infc` enable `-v`, and `-v`
//! alone makes `infc` derive proof mode (so the `.v` keeps the per-spec
//! definitions that `compile` mode strips). Keeping the implication in one
//! place avoids a second source of truth that could drift.
//!
//! ## Single-file vs. project mode
//!
//! The positional path is optional. When a path is given, `build` operates in
//! **single-file mode** (the historical behavior): it compiles exactly that
//! file with `infc` inheriting the current working directory. When the path is
//! omitted, `build` operates in **project mode**: it discovers the project's
//! `Inference.toml` by walking up from the current directory, compiles
//! `<root>/src/main.inf` with `infc`'s working directory set to the project
//! root (so `out/` always lands at the root). `infc` follows the
//! import-reachable closure from `src/main.inf`, compiling every imported file
//! and warning about any unreachable `src/**/*.inf` files itself.
//!
//! ```bash
//! infs build                             # project mode: build <root>/src/main.inf
//! infs build example.inf                 # single-file: parse -> codegen -> out/example.wasm
//! infs build example.inf -v              # also writes out/example.v (proof mode)
//! infs build example.inf --mode proof    # proof mode; writes both .wasm and .v
//! infs build example.inf --mode compile -v   # compile mode + .v (specs stripped)
//! ```
//!
//! ## Project-mode manifest semantics
//!
//! In project mode the manifest's `[build] mode` and `[verification]
//! output-dir` become consumed configuration, with CLI flags overriding:
//!
//! - **Effective mode** = CLI `--mode` if present, else manifest `[build]
//!   mode`. Manifest `proof` forwards `--mode proof`; manifest `compile`
//!   (explicit or defaulted) forwards *nothing* so that `infc`'s `-v` ⇄ proof
//!   implication stays the single source of truth in `infc::normalize_args`.
//!   `infs` never forwards `--mode compile`.
//! - **`output-dir`** is honored *only in effective-proof mode* and is
//!   normalized (relative-only, trailing separator stripped) before forwarding
//!   as `--out-dir`, which moves both `.wasm` and `.v`. In compile mode it is
//!   ignored entirely — the default `proofs/` must never relocate
//!   `out/main.wasm`. The default proof-mode `output-dir` is `proofs/`, so a
//!   default proof build writes both artifacts under `<root>/proofs/`.
//! - **`-v` alone** (no `--mode`, compile-mode manifest) is *not* treated as
//!   proof by `infs`: only the explicitly-owned mode signal triggers
//!   effective-proof mode. `infs build -v` forwards just `-v`; `infc` derives
//!   proof internally and writes both artifacts to `out/` (no `--out-dir`).
//! - **`--out-dir` is forwarded only to an `infc` that supports it**;
//!   pairing a non-default `output-dir` with an older `infc` hard-errors with
//!   remediation rather than failing opaquely in the subprocess.
//! - **`[wasm-dependencies]`** is forwarded as one `--wasm-dep <name>=<path>`
//!   per declaration (paths resolved against the project root), alongside every
//!   `-L`/`--wasm-lib-dir` the user passed. `infc` resolves `use { … } from
//!   <module>` from those two sources only, so without them a project binding
//!   externals cannot link — in proof mode, cannot emit its `.v` at all.
//!
//! ## Manifest settings honored in single-file mode
//!
//! Two `[build]`-adjacent settings are read from the *enclosing* project even
//! when a source path is given, by walking up to the nearest `Inference.toml`:
//! `[wasm-dependencies]`, and `[build] wasm-features`. The latter is not an
//! optional nicety — `infs build`, `infs build src/main.inf`, and `infs run
//! src/main.inf` all write `out/main.wasm` for the same project, so they must not
//! disagree about its WebAssembly instruction level; the feature request, its
//! validation, and its ABI gate are identical on all three paths (see
//! [`crate::commands::run`] for the third). A file outside any project takes the
//! defaults and never errors.
//!
//! `[wasm-dependencies]` is resolved on every path but one: single-file `build`
//! (here) and both project paths forward it. That single-file `run` does not
//! resolve it predates this and is tracked separately (#367).

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::project_build::{
    forward_wasm_features, mode_flag, probe_compiler_compatibility, run_project_build,
};
use crate::errors::InfsError;
use crate::project::manifest::{InferenceToml, MANIFEST_FILE_NAME, find_manifest_dir};
use crate::project::{self, ProjectContext};
use crate::toolchain::resolver::find_infc_with_source;
use inference_compiler_interface::WasmFeatureName;

/// Compilation mode forwarded to `infc --mode <…>`.
///
/// Mirrors `inference_wasm_codegen::CompilationMode` locally so the `infs`
/// binary does not need to depend on the codegen crate just to parse a CLI
/// flag it only forwards as a string.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildMode {
    Compile,
    Proof,
}

/// Arguments for the build command.
///
/// Always performs full compilation (parse, analyze, codegen) and writes
/// the WASM binary to disk. Use `-v` to also generate a Rocq (.v) file.
///
/// `mode` is `Option<BuildMode>` so the absence of `--mode` is distinguishable
/// from `--mode compile`; this lets `-v` alone forward `--mode proof` to `infc`
/// while `--mode compile -v` is left untouched.
#[derive(Args, Clone)]
pub struct BuildArgs {
    /// Path to the source file to compile.
    ///
    /// When omitted, `build` runs in project mode: it discovers the project's
    /// `Inference.toml` by walking up from the current directory and compiles
    /// `<root>/src/main.inf`. Provide a path to compile a single file directly.
    pub path: Option<PathBuf>,

    /// Generate Rocq (.v) translation file in addition to the WASM binary.
    ///
    /// When `--mode` is omitted, `-v` also forwards `--mode proof` to `infc`
    /// (specs are preserved). Pass `--mode compile -v` to opt out.
    #[clap(short = 'v', action = clap::ArgAction::SetTrue)]
    pub generate_v_output: bool,

    /// Compilation mode (`compile` or `proof`). When omitted, defaults to
    /// `compile` unless `-v` is also passed, in which case it resolves to
    /// `proof` (the `.v` artifact requires preserved specs to be useful).
    /// `proof` preserves spec functions unoptimized for Rocq translation
    /// and implies `-v`.
    #[clap(long = "mode", value_enum)]
    pub mode: Option<BuildMode>,

    /// Directory to search for external `.wasm` modules referenced by
    /// `use { … } from <module>;`. Repeatable; forwarded as `--wasm-lib-dir` in
    /// both single-file and project mode. A relative dir always means what it
    /// meant at the shell: single-file `infc` inherits the invocation directory,
    /// and the project path anchors the dir to that directory before forwarding,
    /// because it moves `infc` to the project root.
    #[clap(short = 'L', long = "wasm-lib-dir", value_name = "DIR")]
    pub wasm_lib_dirs: Vec<PathBuf>,

    /// Skip the `[build.wasm-opt]` post-build optimization for this build.
    ///
    /// Project mode only: when the manifest declares `[build.wasm-opt]`, this
    /// leaves `out/main.wasm` exactly as `infc` emitted it. No effect in
    /// single-file mode or when no `[build.wasm-opt]` table is present.
    #[clap(long = "no-wasm-opt")]
    pub no_wasm_opt: bool,
}

/// Executes the build command with the given arguments.
///
/// Dispatches on the presence of a positional path:
/// - `Some(path)` → [`execute_single_file`] (the historical behavior).
/// - `None` → [`execute_project`]: discover `Inference.toml` from the current
///   directory upward and build `<root>/src/main.inf`.
///
/// ## Errors
///
/// Propagates errors from the selected mode (missing file, compiler lookup,
/// ABI handshake, non-zero infc exit, or — in project mode — discovery and
/// entry-point resolution failures).
pub fn execute(args: &BuildArgs) -> Result<()> {
    if let Some(path) = &args.path {
        return execute_single_file(path, args);
    }

    let cwd =
        std::env::current_dir().context("Failed to determine the current working directory")?;
    let ctx = project::discover_and_load(&cwd)?;
    execute_project(&ctx, args)
}

/// Compiles a single explicit source file (single-file mode).
///
/// ## Execution Flow
///
/// 1. Validates that the source file exists
/// 2. Locates the infc compiler binary
/// 3. Runs the `infc` compatibility handshake (git hash + ABI version)
/// 4. Builds and executes the infc command, forwarding `-v` if requested
/// 5. Propagates exit code from infc
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - infc compiler cannot be found
/// - infc reports a *major* ABI version mismatch (hard error with remediation)
/// - the enclosing manifest requests `wasm-features` the resolved `infc` cannot
///   honor
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
fn execute_single_file(path: &Path, args: &BuildArgs) -> Result<()> {
    if !path.exists() {
        bail!("Path not found: {}", path.display());
    }

    let enclosing = enclosing_manifest(path)?;
    let features = manifest_wasm_features(enclosing.as_ref().map(|(_, manifest)| manifest))?;

    let (infc_path, infc_source) = find_infc_with_source()?;
    let compat = probe_compiler_compatibility(&infc_path, infc_source)?;

    let mut cmd = Command::new(&infc_path);
    cmd.arg(path);

    if args.generate_v_output {
        cmd.arg("-v");
    }

    // Forward only what the user explicitly passed. `infc::normalize_args`
    // owns the `-v` ↔ `--mode proof` implication; mirroring it here would
    // create a second source of truth that could silently drift.
    if let Some(mode) = args.mode {
        cmd.arg("--mode").arg(mode_flag(mode));
    }

    for dir in &args.wasm_lib_dirs {
        cmd.arg("--wasm-lib-dir").arg(dir);
    }

    for (name, path) in manifest_wasm_dependencies(enclosing.as_ref())? {
        cmd.arg("--wasm-dep")
            .arg(format_wasm_dep_arg(&name, &path)?);
    }

    let manifest_path = enclosing
        .as_ref()
        .map(|(dir, _)| dir.join(MANIFEST_FILE_NAME));
    forward_wasm_features(&mut cmd, compat, &features, manifest_path.as_deref())?;

    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute infc at {}", infc_path.display()))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        Err(InfsError::process_exit_code(code).into())
    }
}

/// Formats one resolved manifest dependency as the `<name>=<path>` argument
/// forwarded to `infc --wasm-dep`.
///
/// Shared by the single-file path here and the project path in
/// [`crate::commands::project_build`]: the two must spell a dependency the same
/// way, or one project would bind its externals differently depending on how the
/// build was invoked.
///
/// `name` is already validated against the logical-name grammar in
/// [`crate::project::manifest::validate_wasm_dependency_key`], so it never
/// contains `=`. The receiver splits on the FIRST `=`, which is therefore always
/// the name/path boundary — a path that itself contains `=` is preserved intact.
///
/// The argument is a single UTF-8 `String`, so the path must round-trip through
/// UTF-8. Using `Path::display()` would lossily substitute U+FFFD for any
/// non-UTF-8 component and silently forward a corrupted path that resolves to the
/// wrong file (or none). The manifest declares its paths as UTF-8 strings, so a
/// non-UTF-8 *resolved* path can only come from a non-UTF-8 manifest directory.
/// Reject it with an actionable error instead of corrupting it. (An
/// OsString-preserving argument channel would lift this restriction, but is out
/// of scope for this pass.)
///
/// ## Errors
///
/// Returns an error when `path` is not valid UTF-8.
pub(crate) fn format_wasm_dep_arg(name: &str, path: &Path) -> Result<String> {
    let Some(path) = path.to_str() else {
        bail!(
            "wasm dependency `{name}` resolves to a path that is not valid UTF-8 ({}); \
             rename the containing directory to a UTF-8 path so it can be forwarded to \
             the compiler",
            path.display()
        );
    };
    Ok(format!("{name}={path}"))
}

/// Loads the manifest of the project enclosing `source_path`, paired with the
/// directory that holds it.
///
/// Walks up from the source file to the nearest `Inference.toml`. `None` means
/// the source lives outside any project, which is a valid manifest-free build —
/// every manifest-derived setting then takes its default.
///
/// Each single-file path calls this exactly once and derives every manifest
/// setting from the result, so a build cannot read one file for one setting and a
/// different file for another, and a malformed manifest is reported once.
/// `commands::run` shares it for the same reason.
///
/// The path is made absolute against the current directory before the walk. The
/// walk ascends by taking parents, and a shallow relative path runs out of them
/// immediately — `main.inf` has only `""` above it — so `cd src && infs build
/// main.inf` would otherwise find no manifest at all and silently take every
/// default, even though `infs build` from the same directory finds the project.
///
/// ## Errors
///
/// Returns an error if the current directory cannot be determined, or if a
/// manifest exists but cannot be read or parsed.
pub(crate) fn enclosing_manifest(source_path: &Path) -> Result<Option<(PathBuf, InferenceToml)>> {
    let source_path = std::env::current_dir()
        .context("Failed to determine the current working directory")?
        .join(source_path);
    let Some(manifest_dir) = find_manifest_dir(&source_path) else {
        return Ok(None);
    };
    let manifest = InferenceToml::from_file(&manifest_dir.join(MANIFEST_FILE_NAME))?;
    Ok(Some((manifest_dir, manifest)))
}

/// Resolves the `[wasm-dependencies]` of an already-loaded enclosing manifest.
///
/// Returns each declared dependency's logical name paired with its absolute
/// `.wasm` path (relative entries resolved against the manifest directory).
/// `None` — a source outside any project — yields an empty list.
///
/// ## Errors
///
/// Returns an error if any `[wasm-dependencies]` key is not a well-formed logical
/// module name.
fn manifest_wasm_dependencies(
    enclosing: Option<&(PathBuf, InferenceToml)>,
) -> Result<Vec<(String, PathBuf)>> {
    let Some((manifest_dir, manifest)) = enclosing else {
        return Ok(Vec::new());
    };
    manifest.resolved_wasm_dependencies(manifest_dir)
}

/// Resolves the `[build] wasm-features` of an already-loaded enclosing manifest.
///
/// `None` — a source outside any project — requests nothing, which is the pure
/// WebAssembly 1.0 default. Centralizing that default is why both single-file
/// paths call this rather than reaching into `build.wasm_features` themselves.
///
/// ## Errors
///
/// Returns an error if the manifest requests a feature that is not a supported
/// proposal name. (A manifest loaded through [`enclosing_manifest`] has already
/// been validated, so this is the programmatic-construction path.)
pub(crate) fn manifest_wasm_features(
    manifest: Option<&InferenceToml>,
) -> Result<Vec<WasmFeatureName>> {
    manifest.map_or_else(|| Ok(Vec::new()), |m| m.build.resolved_wasm_features())
}

/// Compiles the entry point of a discovered project (project mode).
///
/// Resolves the *effective* build configuration from the CLI flags and the
/// manifest's `[build] mode` / `[verification] output-dir`, then delegates to
/// [`run_project_build`] (which owns the shared spawn, handshake, and exit-code
/// propagation). The forwarding rules are documented on
/// [`resolve_effective_mode`] and [`resolve_out_dir`].
///
/// ## Errors
///
/// Propagates every error [`run_project_build`] can return (missing entry
/// point, compiler lookup, ABI handshake, `--out-dir` capability gate, non-zero
/// infc exit), plus `output-dir` normalization failures.
fn execute_project(ctx: &ProjectContext, args: &BuildArgs) -> Result<()> {
    let effective_mode = resolve_effective_mode(args.mode, &ctx.manifest.build.mode);
    let out_dir = resolve_out_dir(effective_mode, &ctx.manifest.verification)?;

    run_project_build(
        ctx,
        args.generate_v_output,
        effective_mode,
        out_dir.as_deref(),
        &args.wasm_lib_dirs,
        args.no_wasm_opt,
    )
}

/// Resolves the effective `--mode` to forward to `infc`, or `None` to forward
/// nothing.
///
/// Precedence:
/// - CLI `--mode` always wins when present (`compile` or `proof`).
/// - Otherwise, manifest `[build] mode = "proof"` forwards `--mode proof`.
/// - Manifest `"compile"` (explicit or defaulted) forwards **nothing**.
///
/// Why never forward `--mode compile`: `infs` does not own the `-v` ⇄ `--mode
/// proof` implication — `infc::normalize_args` does, and it is the single
/// source of truth. Forwarding an explicit `--mode compile` when the user
/// passed only `-v` would suppress that implication inside `infc` (turning
/// `-v`-alone into a spec-stripped `.v`), reintroducing exactly the drift that
/// single source of truth avoids. Forwarding nothing for the compile case
/// leaves `infc` free to derive proof from `-v`.
///
/// The manifest string is already validated to `compile`/`proof` on load, so
/// the fallback maps any non-`proof` value to "forward nothing".
fn resolve_effective_mode(cli_mode: Option<BuildMode>, manifest_mode: &str) -> Option<BuildMode> {
    if let Some(mode) = cli_mode {
        return Some(mode);
    }
    if manifest_mode == "proof" {
        return Some(BuildMode::Proof);
    }
    None
}

/// Resolves the `--out-dir` to forward, honoring `[verification] output-dir`
/// **only in effective-proof mode**.
///
/// In compile mode (or when no explicit proof mode is in effect) the manifest
/// `output-dir` is ignored entirely and `None` is returned — a default-manifest
/// build must never relocate `out/main.wasm` into the `proofs/` default, and
/// `--out-dir` cannot isolate the `.v` from the `.wasm` anyway (it moves both).
///
/// In effective-proof mode the manifest string is normalized through `PathBuf`
/// (relative-only, trailing separator stripped) and returned for forwarding.
/// The default `"proofs/"` normalizes to `proofs`, so a default proof-mode
/// build writes both artifacts under `<root>/proofs/`.
///
/// Note: this keys off the mode `infs` explicitly owns (CLI `--mode proof` or
/// manifest `mode = "proof"`). It deliberately does **not** treat `-v`-alone as
/// proof: that implication belongs to `infc::normalize_args`, so `infs build -v`
/// on a compile-mode manifest forwards only `-v` and lets `infc` write both
/// artifacts to `out/` — `output-dir` is not consulted.
///
/// # Errors
///
/// Returns an error if the manifest `output-dir` is empty or absolute.
fn resolve_out_dir(
    effective_mode: Option<BuildMode>,
    verification: &crate::project::manifest::VerificationConfig,
) -> Result<Option<PathBuf>> {
    if effective_mode == Some(BuildMode::Proof) {
        Ok(Some(verification.normalized_output_dir()?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod cli_surface_tests {
    use super::*;
    use clap::Parser;

    /// A minimal parser wrapping [`BuildArgs`], standing in for the real CLI so
    /// the flag surface can be exercised without spawning the binary.
    #[derive(Parser)]
    struct BuildCli {
        #[command(flatten)]
        args: BuildArgs,
    }

    /// Both spellings of the lib-dir flag parse, repeat, mix, and preserve the
    /// order given. The order is contractual, not cosmetic: `infc` searches the
    /// directories in the order received and the first hit wins, so a parse
    /// that reordered them would change which `.wasm` a module resolves to.
    #[test]
    fn lib_dir_flag_accepts_both_spellings_and_preserves_order() {
        let cli = BuildCli::try_parse_from([
            "build",
            "-L",
            "first",
            "--wasm-lib-dir",
            "second",
            "-L",
            "third",
            "main.inf",
        ])
        .expect("both spellings of the lib-dir flag must parse");
        assert_eq!(
            cli.args.wasm_lib_dirs,
            [
                PathBuf::from("first"),
                PathBuf::from("second"),
                PathBuf::from("third")
            ]
        );
        assert_eq!(cli.args.path.as_deref(), Some(Path::new("main.inf")));
    }
}

#[cfg(test)]
mod manifest_dep_tests {
    use super::*;
    use assert_fs::prelude::*;

    #[test]
    fn forwards_declared_wasm_dependencies_as_absolute_paths() {
        let temp = assert_fs::TempDir::new().unwrap();
        let manifest = temp.child("Inference.toml");
        manifest
            .write_str(
                "[package]\n\
                 name = \"demo\"\n\
                 version = \"0.1.0\"\n\
                 infc_version = \"0.1.0\"\n\n\
                 [wasm-dependencies]\n\
                 arith = { path = \"libs/arith.wasm\" }\n",
            )
            .unwrap();
        let source = temp.child("src").child("main.inf");
        source.write_str("").unwrap();

        let deps = deps_for(source.path()).expect("should resolve");

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].0, "arith");
        assert_eq!(deps[0].1, temp.path().join("libs/arith.wasm"));
    }

    #[test]
    fn no_manifest_yields_no_dependencies() {
        let temp = assert_fs::TempDir::new().unwrap();
        let source = temp.child("main.inf");
        source.write_str("").unwrap();

        let deps = deps_for(source.path()).expect("should succeed");
        assert!(deps.is_empty());
    }

    #[test]
    fn manifest_without_wasm_dependencies_yields_none() {
        let temp = assert_fs::TempDir::new().unwrap();
        let manifest = temp.child("Inference.toml");
        manifest
            .write_str(
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n",
            )
            .unwrap();
        let source = temp.child("main.inf");
        source.write_str("").unwrap();

        let deps = deps_for(source.path()).expect("should succeed");
        assert!(deps.is_empty());
    }

    #[test]
    fn formats_utf8_dependency_path() {
        let arg = format_wasm_dep_arg("arith", Path::new("/libs/arith.wasm"))
            .expect("a UTF-8 path must format");
        assert_eq!(arg, "arith=/libs/arith.wasm");
    }

    #[test]
    fn preserves_equals_sign_in_path() {
        // The receiver splits on the first `=`, so a `=` inside the path is
        // preserved intact (the name is `=`-free by grammar validation).
        let arg = format_wasm_dep_arg("arith", Path::new("/a=b/arith.wasm"))
            .expect("a path containing `=` must format");
        assert_eq!(arg, "arith=/a=b/arith.wasm");
        assert_eq!(arg.split_once('=').map(|(n, _)| n), Some("arith"));
    }

    /// Writes a manifest carrying `build_body` under `[build]`, plus a source
    /// file nested one directory below it, and returns the source path.
    fn project_with_build_body(temp: &assert_fs::TempDir, build_body: &str) -> std::path::PathBuf {
        temp.child("Inference.toml")
            .write_str(&format!(
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\
                 infc_version = \"0.1.0\"\n\n[build]\n{build_body}"
            ))
            .unwrap();
        let source = temp.child("src").child("main.inf");
        source.write_str("").unwrap();
        source.path().to_path_buf()
    }

    /// The features a single-file path would request for `source`, going through
    /// the same load-then-derive sequence the command does.
    fn features_for(source: &Path) -> Result<Vec<WasmFeatureName>> {
        let enclosing = enclosing_manifest(source)?;
        manifest_wasm_features(enclosing.as_ref().map(|(_, manifest)| manifest))
    }

    /// The dependencies a single-file build would forward for `source`, through
    /// the same load-then-derive sequence.
    fn deps_for(source: &Path) -> Result<Vec<(String, PathBuf)>> {
        let enclosing = enclosing_manifest(source)?;
        manifest_wasm_dependencies(enclosing.as_ref())
    }

    #[test]
    fn file_outside_any_project_requests_no_wasm_features() {
        // Manifest-free is a valid build, not an error.
        let temp = assert_fs::TempDir::new().unwrap();
        let source = temp.child("main.inf");
        source.write_str("").unwrap();
        assert!(enclosing_manifest(source.path()).unwrap().is_none());
        assert!(features_for(source.path()).unwrap().is_empty());
    }

    /// A *relative* path given from a *subdirectory* must still find the project.
    ///
    /// The walk ascends by taking parents, and `main.inf` has none — only `""`,
    /// whose parent is `None`. Without absolutizing first, `cd src && infs build
    /// main.inf` finds no manifest and silently takes every default, while
    /// `infs build` from that same directory finds the project. Serialized
    /// because it moves the process working directory.
    #[test]
    #[serial_test::serial]
    fn relative_source_path_from_a_subdirectory_finds_the_enclosing_manifest() {
        let temp = assert_fs::TempDir::new().unwrap();
        project_with_build_body(&temp, "wasm-features = [\"bulk-memory\"]\n");

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path().join("src")).unwrap();
        let resolved = features_for(Path::new("main.inf"));
        std::env::set_current_dir(original).unwrap();

        assert_eq!(
            resolved.unwrap(),
            vec![WasmFeatureName::BulkMemory],
            "a bare relative filename must resolve against the current \
             directory before the walk"
        );
    }

    #[test]
    fn enclosing_manifest_wasm_features_are_honored_from_a_nested_source() {
        // The walk that finds `[wasm-dependencies]` finds this too, so building
        // one file of a project sees the project's instruction level.
        let temp = assert_fs::TempDir::new().unwrap();
        let source = project_with_build_body(&temp, "wasm-features = [\"bulk-memory\"]\n");
        assert_eq!(
            features_for(&source).unwrap(),
            vec![WasmFeatureName::BulkMemory]
        );
    }

    #[test]
    fn manifest_without_the_key_requests_no_wasm_features() {
        let temp = assert_fs::TempDir::new().unwrap();
        let source = project_with_build_body(&temp, "mode = \"compile\"\n");
        assert!(features_for(&source).unwrap().is_empty());
    }

    #[test]
    fn invalid_enclosing_wasm_feature_fails_the_single_file_build() {
        // The manifest is rejected on load, so single-file mode surfaces the same
        // diagnostic project mode would — it does not quietly build at 1.0.
        let temp = assert_fs::TempDir::new().unwrap();
        let source = project_with_build_body(&temp, "wasm-features = [\"memory.fill\"]\n");
        let err = features_for(&source).expect_err("an instruction name is not a feature");
        assert!(
            format!("{err:#}").contains("is an instruction, not a feature"),
            "got: {err:#}"
        );
    }

    #[test]
    fn one_manifest_read_serves_both_derived_settings() {
        // The load happens once and both settings come off that one value, so a
        // build cannot read one file for its features and another for its deps.
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("Inference.toml")
            .write_str(
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n\n\
                 [build]\nwasm-features = [\"bulk-memory\"]\n\n\
                 [wasm-dependencies]\narith = { path = \"libs/arith.wasm\" }\n",
            )
            .unwrap();
        let source = temp.child("src").child("main.inf");
        source.write_str("").unwrap();

        let enclosing = enclosing_manifest(source.path())
            .unwrap()
            .expect("the manifest must be found");
        assert_eq!(enclosing.0, temp.path());
        assert_eq!(
            manifest_wasm_features(Some(&enclosing.1)).unwrap(),
            vec![WasmFeatureName::BulkMemory]
        );
        let deps = manifest_wasm_dependencies(Some(&enclosing)).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].1, temp.path().join("libs/arith.wasm"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_dependency_path() {
        use std::os::unix::ffi::OsStrExt;

        // A path component with an invalid UTF-8 byte (0xFF) cannot round-trip
        // through the single-`String` `<name>=<path>` argument.
        let bytes = b"/libs/\xFF/arith.wasm";
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(bytes));
        let err = format_wasm_dep_arg("arith", &path)
            .expect_err("a non-UTF-8 path must be rejected, not lossily forwarded");
        let msg = err.to_string();
        assert!(
            msg.contains("arith") && msg.contains("not valid UTF-8"),
            "diagnostic should name the dependency and the UTF-8 cause; got: {msg}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::manifest::VerificationConfig;

    #[test]
    fn cli_mode_overrides_manifest() {
        // CLI proof wins over manifest compile, and CLI compile wins over
        // manifest proof — the CLI is always authoritative when present.
        assert_eq!(
            resolve_effective_mode(Some(BuildMode::Proof), "compile"),
            Some(BuildMode::Proof)
        );
        assert_eq!(
            resolve_effective_mode(Some(BuildMode::Compile), "proof"),
            Some(BuildMode::Compile)
        );
    }

    #[test]
    fn manifest_proof_forwards_proof() {
        assert_eq!(
            resolve_effective_mode(None, "proof"),
            Some(BuildMode::Proof)
        );
    }

    #[test]
    fn manifest_compile_forwards_nothing() {
        // Compile (explicit or defaulted) must forward nothing so infc's
        // `-v` ⇄ proof implication stays the single source of truth.
        assert_eq!(resolve_effective_mode(None, "compile"), None);
    }

    #[test]
    fn compile_mode_ignores_output_dir() {
        // Even a non-default output-dir must be ignored in compile mode,
        // so out/main.wasm is never relocated into proofs/.
        let verification = VerificationConfig {
            output_dir: String::from("artifacts"),
        };
        assert_eq!(resolve_out_dir(None, &verification).unwrap(), None);
        assert_eq!(
            resolve_out_dir(Some(BuildMode::Compile), &verification).unwrap(),
            None
        );
    }

    #[test]
    fn proof_mode_forwards_default_output_dir_normalized() {
        // The default "proofs/" must normalize to `proofs` and be forwarded.
        let verification = VerificationConfig::default();
        assert_eq!(
            resolve_out_dir(Some(BuildMode::Proof), &verification).unwrap(),
            Some(PathBuf::from("proofs"))
        );
    }

    #[test]
    fn proof_mode_forwards_custom_output_dir() {
        let verification = VerificationConfig {
            output_dir: String::from("artifacts/"),
        };
        assert_eq!(
            resolve_out_dir(Some(BuildMode::Proof), &verification).unwrap(),
            Some(PathBuf::from("artifacts"))
        );
    }

    #[test]
    fn proof_mode_propagates_output_dir_validation_error() {
        // An absolute output-dir is rejected — but only when proof mode actually
        // consults it (in compile mode the bad value is never read).
        let abs = if cfg!(windows) { r"C:\x" } else { "/x" };
        let verification = VerificationConfig {
            output_dir: String::from(abs),
        };
        assert!(resolve_out_dir(Some(BuildMode::Proof), &verification).is_err());
        assert!(
            resolve_out_dir(None, &verification).is_ok(),
            "compile mode must not even read a malformed output-dir"
        );
    }
}

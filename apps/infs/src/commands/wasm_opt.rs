//! Post-build optimization of the compile-mode artifact via Binaryen `wasm-opt`.
//!
//! When a project's `Inference.toml` declares `[build.wasm-opt]`, `infs` runs
//! the external `wasm-opt` binary over `<root>/out/main.wasm` after `infc` exits
//! successfully, replacing the artifact in place with a smaller/faster
//! equivalent. This is an opt-in, project-level step: absent the table the
//! default pipeline is byte-identical, and `infc`/core are never touched. It
//! mirrors the rustc-vs-cargo split, where the wrapping tool (wasm-pack, trunk)
//! runs `wasm-opt`, not the compiler.
//!
//! Only executable artifacts are optimized. Proof-mode builds — and any `-v`
//! build, which `infs` treats conservatively as a verification workflow — are
//! skipped silently: their WASM carries verification-only opcodes (the `0xfc`
//! non-det/uzumaki family) that `wasm-opt` cannot process, and they are a
//! different artifact class. As a backstop, a compile-mode artifact that still
//! carries such an opcode is a hard error with remediation rather than a
//! confusing `wasm-opt` parse failure.
//!
//! It lives under `commands/` for the same reason as
//! [`crate::commands::project_build`]: spawning an external tool and propagating
//! its outcome is command-execution logic, not manifest/filesystem logic.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use inf_wasmparser::{Operator, Parser, Payload, WasmFeatures};

use crate::commands::build::BuildMode;
use crate::project::ProjectContext;

/// Environment variable that overrides `wasm-opt` resolution, taking priority
/// over a PATH lookup.
const WASM_OPT_PATH_ENV: &str = "WASM_OPT_PATH";

/// Minimum supported Binaryen major version. The forwarded flags
/// (`--mvp-features` plus the mutable-globals / bulk-memory enables) and the
/// `-Os`/`-Oz` levels are stable from Binaryen 116 onward.
const MIN_WASM_OPT_VERSION: u32 = 116;

/// Optimizes `<root>/out/main.wasm` in place when `[build.wasm-opt]` is enabled.
///
/// Called from [`crate::commands::project_build::run_project_build`] after a
/// successful `infc` exit. Returns `Ok(())` — a no-op — whenever optimization is
/// not requested: no `[build.wasm-opt]` table, `enabled = false`, or
/// `cli_disabled` (the `--no-wasm-opt` flag).
///
/// Proof-mode builds and any `-v` build are skipped silently. `infs` does not
/// own the `-v` ⇄ proof implication (that lives in `infc::normalize_args`), so
/// it treats `--mode proof` *or* a `-v` build as a verification workflow and
/// leaves the artifact alone. Because a verification build is the only thing
/// that forwards `--out-dir`, reaching the optimization path guarantees the
/// artifact is at the conventional `<root>/out/main.wasm`.
///
/// # Errors
///
/// When optimization is active, errors if the artifact is missing, still
/// carries a verification-only opcode, `wasm-opt` cannot be resolved or is too
/// old, or the optimization/re-validation fails. Every failure leaves the
/// original `out/main.wasm` untouched.
pub(crate) fn post_build_optimize(
    ctx: &ProjectContext,
    generate_v_output: bool,
    mode: Option<BuildMode>,
    cli_disabled: bool,
) -> Result<()> {
    let Some(config) = &ctx.manifest.build.wasm_opt else {
        return Ok(());
    };
    if !config.enabled || cli_disabled {
        return Ok(());
    }

    if mode == Some(BuildMode::Proof) || generate_v_output {
        // Same INFS_VERBOSE semantics as the toolchain resolver: set, non-empty,
        // and not "0".
        if std::env::var_os("INFS_VERBOSE").is_some_and(|v| !v.is_empty() && v != "0") {
            eprintln!(
                "wasm-opt: skipping a verification build (proof mode or -v); \
                 only executable artifacts are optimized."
            );
        }
        return Ok(());
    }

    let wasm_path = ctx.root.join("out").join("main.wasm");
    if !wasm_path.is_file() {
        bail!(
            "Compilation succeeded but WASM file not found at: {}",
            wasm_path.display()
        );
    }

    let wasm_bytes = std::fs::read(&wasm_path)
        .with_context(|| format!("Failed to read {} for optimization", wasm_path.display()))?;

    if let Some(construct) = find_verification_construct(&wasm_bytes)? {
        bail!(
            "`[build.wasm-opt]` is enabled but `out/main.wasm` contains the \
             verification-only construct `{construct}`, which wasm-opt cannot \
             process. Verification constructs (forall/exists/assume/unique and \
             `@`/uzumaki) belong in `spec` blocks, which compile-mode builds \
             strip. Move the construct into a `spec` block, or disable \
             optimization (`enabled = false` under `[build.wasm-opt]`, or pass \
             `--no-wasm-opt`)."
        );
    }

    let wasm_opt = resolve_wasm_opt()?;
    check_wasm_opt_version(&wasm_opt)?;

    let before = wasm_bytes.len() as u64;
    optimize_in_place(&wasm_opt, &config.level, &wasm_path)?;
    let after = std::fs::metadata(&wasm_path)
        .with_context(|| format!("Failed to stat optimized {}", wasm_path.display()))?
        .len();

    println!(
        "wasm-opt -O{}: main.wasm {before} -> {after} bytes",
        config.level
    );
    Ok(())
}

/// Resolves the `wasm-opt` binary: the `WASM_OPT_PATH` override if set,
/// otherwise a PATH lookup.
///
/// # Errors
///
/// Errors when `WASM_OPT_PATH` is set but does not name a file, or when no
/// `wasm-opt` is found on PATH (with install remediation).
fn resolve_wasm_opt() -> Result<PathBuf> {
    resolve_wasm_opt_from(std::env::var_os(WASM_OPT_PATH_ENV).as_deref())
}

/// The testable core of [`resolve_wasm_opt`], with the environment lookup lifted
/// into `override_path` so tests need not mutate process-global state.
///
/// `Some` is the `WASM_OPT_PATH` override: it must name an existing file or the
/// call errors (naming the env var and the path). `None` falls back to a PATH
/// lookup, erroring with install hints when `wasm-opt` is absent.
///
/// # Errors
///
/// See [`resolve_wasm_opt`].
fn resolve_wasm_opt_from(override_path: Option<&OsStr>) -> Result<PathBuf> {
    if let Some(raw) = override_path {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "`{WASM_OPT_PATH_ENV}` is set to `{}`, which is not a file. Point it \
             at a `wasm-opt` executable, or unset it to search PATH.",
            path.display()
        );
    }

    which::which("wasm-opt").map_err(|_| missing_wasm_opt_error())
}

/// The install-hint error for a missing `wasm-opt`, mirroring the wasmtime hint
/// style in [`crate::commands::run`].
fn missing_wasm_opt_error() -> anyhow::Error {
    anyhow::anyhow!(
        "wasm-opt not found in PATH.\n\n\
         `[build.wasm-opt]` is enabled but the Binaryen `wasm-opt` optimizer \
         could not be located. To install Binaryen:\n  \
         - macOS: brew install binaryen\n  \
         - Linux: apt install binaryen  (or your distribution's package)\n  \
         - npm: npm install -g binaryen\n  \
         - Or download a release: https://github.com/WebAssembly/binaryen/releases\n\n\
         Then ensure `wasm-opt` is in PATH, or set {WASM_OPT_PATH_ENV} to its \
         full path. To build without optimization, set `enabled = false` under \
         `[build.wasm-opt]`, or pass `--no-wasm-opt`."
    )
}

/// Verifies the resolved `wasm-opt` is new enough ([`MIN_WASM_OPT_VERSION`]).
///
/// A parsed version below the minimum is a hard error. If `--version` cannot be
/// run, exits unsuccessfully, or emits output that does not parse, this warns
/// (quoting the raw output) and proceeds: a best-effort check must never block a
/// build over an unrecognized — but possibly perfectly good — binary.
///
/// # Errors
///
/// Errors only when a version is successfully parsed and is below the minimum.
fn check_wasm_opt_version(wasm_opt: &Path) -> Result<()> {
    let output = match Command::new(wasm_opt).arg("--version").output() {
        Ok(output) => output,
        Err(err) => {
            eprintln!(
                "warning: could not run `{} --version` ({err}); proceeding \
                 without a wasm-opt version check.",
                wasm_opt.display()
            );
            return Ok(());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        eprintln!(
            "warning: `{} --version` exited unsuccessfully (output: {:?}); \
             proceeding without a wasm-opt version check.",
            wasm_opt.display(),
            stdout.trim()
        );
        return Ok(());
    }

    let Some(version) = parse_wasm_opt_version(&stdout) else {
        eprintln!(
            "warning: could not parse a wasm-opt version from {:?}; proceeding \
             without a version check.",
            stdout.trim()
        );
        return Ok(());
    };

    if version < MIN_WASM_OPT_VERSION {
        bail!(
            "wasm-opt version {version} is too old: `[build.wasm-opt]` requires \
             Binaryen {MIN_WASM_OPT_VERSION} or newer. Update Binaryen, or set \
             {WASM_OPT_PATH_ENV} to a newer `wasm-opt`."
        );
    }
    Ok(())
}

/// Parses the major version from `wasm-opt --version` output, whose first line
/// reads like `wasm-opt version 116 (version_116-...)`. Returns the first
/// whitespace-delimited token that parses as a `u32`, or `None` if there is
/// none.
fn parse_wasm_opt_version(stdout: &str) -> Option<u32> {
    stdout
        .split_whitespace()
        .find_map(|token| token.parse::<u32>().ok())
}

/// Scans `wasm_bytes` for a verification-only opcode and returns its source
/// spelling (e.g. `"forall"`, `"i32.uzumaki"`) if one is present.
///
/// Compile-mode builds strip `spec` blocks, so a well-formed executable artifact
/// carries none of these. Finding one means a verification construct leaked into
/// an ordinary function — `wasm-opt` would reject the unknown `0xfc` opcode with
/// an opaque error, so this pre-scan surfaces it with remediation instead.
///
/// # Errors
///
/// Errors if the artifact cannot be parsed as WebAssembly.
fn find_verification_construct(wasm_bytes: &[u8]) -> Result<Option<&'static str>> {
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload.map_err(|err| {
            anyhow::anyhow!("failed to scan out/main.wasm for verification constructs: {err}")
        })?;
        let Payload::CodeSectionEntry(body) = payload else {
            continue;
        };
        let operators = body.get_operators_reader().map_err(|err| {
            anyhow::anyhow!("failed to read a function body while scanning out/main.wasm: {err}")
        })?;
        for op in operators {
            let op = op.map_err(|err| {
                anyhow::anyhow!("failed to decode an operator while scanning out/main.wasm: {err}")
            })?;
            if let Some(name) = verification_construct_name(&op) {
                return Ok(Some(name));
            }
        }
    }
    Ok(None)
}

/// The source spelling of a verification-only operator, or `None` for an
/// ordinary executable one.
///
/// This is the local mirror of `is_verification_only` in
/// `core/wasm-linker/src/safety.rs` — the linker's fail-closed predicate over
/// the same six opcodes. Both consume the same `inf-wasmparser` fork, so a new
/// verification opcode requires touching that fork (where the mirrored-predicate
/// note lives); a wasm-linker dependency for six match arms is not worth the
/// coupling.
fn verification_construct_name(op: &Operator) -> Option<&'static str> {
    use Operator::{Assume, Exists, Forall, I32Uzumaki, I64Uzumaki, Unique};
    match op {
        Forall { .. } => Some("forall"),
        Exists { .. } => Some("exists"),
        Assume { .. } => Some("assume"),
        Unique { .. } => Some("unique"),
        I32Uzumaki { .. } => Some("i32.uzumaki"),
        I64Uzumaki { .. } => Some("i64.uzumaki"),
        _ => None,
    }
}

/// Builds the `wasm-opt` argument vector.
///
/// `--mvp-features` pins the baseline feature set so the result is stable across
/// Binaryen versions; the two `--enable-*` flags re-admit exactly the proposals
/// Inference codegen relies on — a mutable exported `__stack_pointer` global
/// (mutable-globals) and `memory.copy`/`memory.fill` (bulk-memory) — matching
/// the linker's supported-feature envelope. `-O<level>` works uniformly for
/// every value `WasmOptConfig` validates, so there is no second mapping table.
fn wasm_opt_args(level: &str, input: &Path, output: &Path) -> Vec<OsString> {
    vec![
        OsString::from(format!("-O{level}")),
        OsString::from("--mvp-features"),
        OsString::from("--enable-mutable-globals"),
        OsString::from("--enable-bulk-memory"),
        input.as_os_str().to_os_string(),
        OsString::from("-o"),
        output.as_os_str().to_os_string(),
    ]
}

/// Runs `wasm-opt` over `wasm_path`, replacing it in place only if the optimized
/// bytes re-validate.
///
/// The optimizer writes to a sibling temp file (`main.wasm.opt`, same directory
/// and filesystem) and the original is swapped in via an atomic
/// [`std::fs::rename`] only after the result validates. Every failure path — a
/// nonzero `wasm-opt` exit, unreadable output, or failed re-validation — leaves
/// the original artifact untouched and makes a best-effort attempt to remove the
/// temp file.
///
/// # Errors
///
/// Errors if `wasm-opt` cannot be spawned, exits nonzero, produces output that
/// cannot be read or fails re-validation, or if the final rename fails.
fn optimize_in_place(wasm_opt: &Path, level: &str, wasm_path: &Path) -> Result<()> {
    let tmp_path = optimized_tmp_path(wasm_path);
    let args = wasm_opt_args(level, wasm_path, &tmp_path);

    let output = Command::new(wasm_opt)
        .args(&args)
        .output()
        .with_context(|| format!("Failed to execute wasm-opt at {}", wasm_opt.display()))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let code = output.status.code().unwrap_or(1);
        bail!(
            "wasm-opt failed (exit code {code}): {}\nThe unoptimized artifact at \
             {} is unchanged.",
            String::from_utf8_lossy(&output.stderr).trim(),
            wasm_path.display()
        );
    }

    let optimized = match std::fs::read(&tmp_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(anyhow::Error::from(err).context(format!(
                "wasm-opt reported success but its output at {} could not be read",
                tmp_path.display()
            )));
        }
    };

    if let Err(err) = validate_optimized(&optimized) {
        let _ = std::fs::remove_file(&tmp_path);
        bail!(
            "wasm-opt produced an artifact that failed re-validation: {err}. The \
             original {} is unchanged; try `--no-wasm-opt`, or a different \
             Binaryen version.",
            wasm_path.display()
        );
    }

    std::fs::rename(&tmp_path, wasm_path).map_err(|err| {
        let _ = std::fs::remove_file(&tmp_path);
        anyhow::Error::from(err).context(format!(
            "Failed to replace {} with the optimized artifact",
            wasm_path.display()
        ))
    })?;

    Ok(())
}

/// The sibling temp path `wasm-opt` writes to: the artifact path with `.opt`
/// appended (`out/main.wasm` → `out/main.wasm.opt`). Kept in the same directory
/// so the final [`std::fs::rename`] stays on one filesystem and is atomic.
fn optimized_tmp_path(wasm_path: &Path) -> PathBuf {
    let mut tmp = wasm_path.as_os_str().to_os_string();
    tmp.push(".opt");
    PathBuf::from(tmp)
}

/// Re-validates `bytes` against the same feature envelope the linker enforces
/// (`GC_TYPES | MUTABLE_GLOBAL | BULK_MEMORY`), guarding against a `wasm-opt`
/// that emits something outside the executable subset the pipeline supports.
///
/// # Errors
///
/// Errors with the validator's message when `bytes` is not valid WebAssembly
/// within that feature set.
fn validate_optimized(bytes: &[u8]) -> Result<()> {
    let features = WasmFeatures::GC_TYPES
        .union(WasmFeatures::MUTABLE_GLOBAL)
        .union(WasmFeatures::BULK_MEMORY);
    inf_wasmparser::Validator::new_with_features(features)
        .validate_all(bytes)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    /// Wraps a raw code-section body (an operator stream) into a one-function
    /// module and returns the finished bytes. `wat` cannot assemble the custom
    /// `0xfc`-prefixed Inference opcodes, so bodies exercising them are built
    /// byte-by-byte (recipe mirrored from `core/wasm-linker/src/safety.rs`).
    /// `Function::new([])` emits the empty-locals byte, so `body` is the
    /// instruction stream that follows it.
    fn module_with_raw_body(body: &[u8]) -> Vec<u8> {
        use wasm_encoder::{CodeSection, Function, FunctionSection, Module, TypeSection};
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        f.raw(body.iter().copied());
        code.function(&f);
        module.section(&code);
        module.finish()
    }

    #[test]
    fn parse_wasm_opt_version_reads_release_output() {
        assert_eq!(
            parse_wasm_opt_version("wasm-opt version 116 (version_116)"),
            Some(116)
        );
    }

    #[test]
    fn parse_wasm_opt_version_reads_git_suffix_output() {
        assert_eq!(
            parse_wasm_opt_version("wasm-opt version 123 (version_123-4-gdeadbee)"),
            Some(123)
        );
    }

    #[test]
    fn parse_wasm_opt_version_rejects_garbage() {
        assert_eq!(parse_wasm_opt_version("banana"), None);
        assert_eq!(parse_wasm_opt_version("wasm-opt version vNext"), None);
    }

    #[test]
    fn parse_wasm_opt_version_rejects_empty() {
        assert_eq!(parse_wasm_opt_version(""), None);
    }

    #[test]
    fn wasm_opt_args_are_exact_for_level_z() {
        let args = wasm_opt_args("z", Path::new("in.wasm"), Path::new("out.wasm.opt"));
        let expected: Vec<OsString> = [
            "-Oz",
            "--mvp-features",
            "--enable-mutable-globals",
            "--enable-bulk-memory",
            "in.wasm",
            "-o",
            "out.wasm.opt",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(args, expected);
    }

    #[test]
    fn wasm_opt_args_are_exact_for_level_3() {
        let args = wasm_opt_args("3", Path::new("a.wasm"), Path::new("b.wasm.opt"));
        let expected: Vec<OsString> = [
            "-O3",
            "--mvp-features",
            "--enable-mutable-globals",
            "--enable-bulk-memory",
            "a.wasm",
            "-o",
            "b.wasm.opt",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(args, expected);
    }

    #[test]
    fn resolve_wasm_opt_from_hits_existing_override() {
        let dir = assert_fs::TempDir::new().unwrap();
        let fake = dir.child("wasm-opt");
        fake.write_str("#!/bin/sh\n").unwrap();
        let resolved = resolve_wasm_opt_from(Some(fake.path().as_os_str())).unwrap();
        assert_eq!(resolved, fake.path());
    }

    #[test]
    fn resolve_wasm_opt_from_rejects_nonexistent_override() {
        let dir = assert_fs::TempDir::new().unwrap();
        let missing = dir.path().join("nope-wasm-opt");
        let err = resolve_wasm_opt_from(Some(missing.as_os_str())).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(WASM_OPT_PATH_ENV) && msg.contains("not a file"),
            "an override that is not a file must name the env var and the failure, got: {msg}"
        );
    }

    #[test]
    fn find_verification_construct_detects_each_nondet_block() {
        for (sub_opcode, name) in [
            (0x3a_u8, "forall"),
            (0x3b, "exists"),
            (0x3c, "assume"),
            (0x3d, "unique"),
        ] {
            // `<nondet> (empty blocktype) end; end`.
            let body = [0x00, 0xfc, sub_opcode, 0x40, 0x0b, 0x0b];
            let module = module_with_raw_body(&body);
            assert_eq!(
                find_verification_construct(&module).unwrap(),
                Some(name),
                "sub-opcode {sub_opcode:#x} must be reported as `{name}`"
            );
        }
    }

    #[test]
    fn find_verification_construct_detects_uzumaki() {
        // `<uzumaki> drop; end`, for both the i32 and i64 forms.
        let i32_body = [0x00, 0xfc, 0x31, 0x1a, 0x0b];
        assert_eq!(
            find_verification_construct(&module_with_raw_body(&i32_body)).unwrap(),
            Some("i32.uzumaki")
        );
        let i64_body = [0x00, 0xfc, 0x32, 0x1a, 0x0b];
        assert_eq!(
            find_verification_construct(&module_with_raw_body(&i64_body)).unwrap(),
            Some("i64.uzumaki")
        );
    }

    #[test]
    fn find_verification_construct_ignores_plain_body() {
        // An ordinary executable body (just `end`) carries no verification-only
        // opcode.
        let module = module_with_raw_body(&[0x0b]);
        assert_eq!(find_verification_construct(&module).unwrap(), None);
    }

    // Spawns a real executable stub, so it is gated to unix (mirroring the
    // stub-based tests in `project_build.rs`); executing a script through
    // `Command` is not portable to Windows.
    #[cfg(unix)]
    #[test]
    fn optimize_in_place_leaves_original_on_wasm_opt_failure() {
        // A `wasm-opt` that exits nonzero must not touch the original artifact,
        // and must not leave the temp file behind.
        let dir = assert_fs::TempDir::new().unwrap();
        let wasm_path = dir.path().join("main.wasm");
        let original = b"original artifact bytes";
        std::fs::write(&wasm_path, original).unwrap();

        let fake = write_failing_wasm_opt(&dir);
        let err = optimize_in_place(&fake, "z", &wasm_path).unwrap_err();
        assert!(
            err.to_string().contains("wasm-opt failed"),
            "a nonzero exit must surface as a wasm-opt failure, got: {err}"
        );
        assert_eq!(
            std::fs::read(&wasm_path).unwrap(),
            original,
            "the original artifact must be unchanged after a wasm-opt failure"
        );
        assert!(
            !optimized_tmp_path(&wasm_path).exists(),
            "the temp file must be cleaned up after a failure"
        );
    }

    #[test]
    fn post_build_optimize_is_noop_without_table() {
        // No [build.wasm-opt] table: a no-op even when out/main.wasm is absent.
        let dir = assert_fs::TempDir::new().unwrap();
        let ctx = ctx_with_wasm_opt(dir.path(), None);
        assert!(post_build_optimize(&ctx, false, None, false).is_ok());
    }

    #[test]
    fn post_build_optimize_is_noop_when_disabled_by_config() {
        let dir = assert_fs::TempDir::new().unwrap();
        let ctx = ctx_with_wasm_opt(dir.path(), Some((false, "3")));
        assert!(post_build_optimize(&ctx, false, None, false).is_ok());
    }

    #[test]
    fn post_build_optimize_is_noop_when_disabled_by_cli() {
        let dir = assert_fs::TempDir::new().unwrap();
        let ctx = ctx_with_wasm_opt(dir.path(), Some((true, "3")));
        // cli_disabled short-circuits before the missing-artifact check.
        assert!(post_build_optimize(&ctx, false, None, true).is_ok());
    }

    #[test]
    fn post_build_optimize_skips_proof_and_v_builds() {
        let dir = assert_fs::TempDir::new().unwrap();
        let ctx = ctx_with_wasm_opt(dir.path(), Some((true, "3")));
        // Proof mode and any -v build skip before touching the (absent) artifact.
        assert!(post_build_optimize(&ctx, false, Some(BuildMode::Proof), false).is_ok());
        assert!(post_build_optimize(&ctx, true, None, false).is_ok());
    }

    #[test]
    fn post_build_optimize_errors_on_missing_artifact_when_active() {
        // Active config, compile mode: the missing artifact is a hard error
        // (the skip short-circuits above do not apply).
        let dir = assert_fs::TempDir::new().unwrap();
        let ctx = ctx_with_wasm_opt(dir.path(), Some((true, "3")));
        let err = post_build_optimize(&ctx, false, None, false).unwrap_err();
        assert!(
            err.to_string().contains("WASM file not found"),
            "an active config with no artifact must error, got: {err}"
        );
    }

    /// Builds a [`ProjectContext`] rooted at `root` whose manifest carries the
    /// given `[build.wasm-opt]` setting (`Some((enabled, level))`) or none.
    fn ctx_with_wasm_opt(root: &Path, wasm_opt: Option<(bool, &str)>) -> ProjectContext {
        use crate::project::manifest::{InferenceToml, WasmOptConfig};
        let mut manifest = InferenceToml::new("demo");
        manifest.build.wasm_opt = wasm_opt.map(|(enabled, level)| WasmOptConfig {
            enabled,
            level: level.to_string(),
        });
        ProjectContext {
            root: root.to_path_buf(),
            manifest,
            entry_point: root.join("src").join("main.inf"),
        }
    }

    /// Writes an executable stub at `<dir>/wasm-opt` that exits nonzero for any
    /// non-`--version` invocation, used to exercise the failure path.
    #[cfg(unix)]
    fn write_failing_wasm_opt(dir: &assert_fs::TempDir) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let stub = dir.child("wasm-opt");
        stub.write_str("#!/bin/sh\necho 'fake failure' 1>&2\nexit 1\n")
            .unwrap();
        let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(stub.path(), perms).unwrap();
        stub.path().to_path_buf()
    }
}

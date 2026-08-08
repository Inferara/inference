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
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use inf_wasmparser::{Operator, Parser, Payload, WasmFeatures};

use crate::commands::build::BuildMode;
use crate::project::ProjectContext;
use crate::toolchain::binaryen;
use crate::toolchain::doctor::DoctorCheck;
use crate::toolchain::{Platform, ToolchainPaths};

/// Environment variable that overrides `wasm-opt` resolution, taking priority
/// over a PATH lookup.
const WASM_OPT_PATH_ENV: &str = "WASM_OPT_PATH";

/// Minimum supported Binaryen major version. The forwarded flags
/// (`--mvp-features` plus the `--enable-*` feature flags) and the `-Os`/`-Oz`
/// levels are stable from Binaryen 116 onward.
const MIN_WASM_OPT_VERSION: u32 = 116;

/// Identifies which precedence tier resolved `wasm-opt`.
///
/// [`WasmOptSource::label`] emits the exact strings used in both the
/// `INFS_VERBOSE` trace line and the `infs doctor` line, so those two surfaces
/// stay in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WasmOptSource {
    /// Resolved via the `WASM_OPT_PATH` environment variable (highest priority).
    EnvOverride,
    /// Resolved via `which::which("wasm-opt")` against the system `PATH`.
    SystemPath,
    /// Resolved via the infs-managed Binaryen under `<root>/tools/binaryen/`.
    ManagedTools,
}

impl WasmOptSource {
    /// The human-readable label for this resolution tier.
    fn label(self) -> &'static str {
        match self {
            Self::EnvOverride => "WASM_OPT_PATH env",
            Self::SystemPath => "PATH",
            Self::ManagedTools => "managed tools",
        }
    }
}

/// Whether `INFS_VERBOSE` is set to a non-empty, non-"0" value. Mirrors the
/// toolchain resolver's predicate so build traces and `infs doctor` agree.
fn verbose() -> bool {
    std::env::var_os("INFS_VERBOSE").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Emits a resolution trace line to stderr under `INFS_VERBOSE`.
fn trace_resolved(source: WasmOptSource, path: &Path) {
    if verbose() {
        eprintln!(
            "infs: resolved wasm-opt via {}: {}",
            source.label(),
            path.display()
        );
    }
}

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
        if verbose() {
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

    let uses_bulk_memory = match scan_artifact(&wasm_bytes)? {
        ArtifactScan::VerificationConstruct(construct) => bail!(
            "`[build.wasm-opt]` is enabled but `out/main.wasm` contains the \
             verification-only construct `{construct}`, which wasm-opt cannot \
             process. Verification constructs (forall/exists/assume/unique and \
             `@`/uzumaki) belong in `spec` blocks, which compile-mode builds \
             strip. Move the construct into a `spec` block, or disable \
             optimization (`enabled = false` under `[build.wasm-opt]`, or pass \
             `--no-wasm-opt`)."
        ),
        ArtifactScan::Executable { uses_bulk_memory } => uses_bulk_memory,
    };

    let wasm_opt = match resolve_wasm_opt_with_source()? {
        Some((path, _)) => path,
        None if config.auto_install => auto_install_wasm_opt()?,
        None => return Err(missing_wasm_opt_error()),
    };
    check_wasm_opt_version(&wasm_opt)?;

    let before = wasm_bytes.len() as u64;
    optimize_in_place(&wasm_opt, &config.level, &wasm_path, uses_bulk_memory)?;
    let after = std::fs::metadata(&wasm_path)
        .with_context(|| format!("Failed to stat optimized {}", wasm_path.display()))?
        .len();

    println!(
        "wasm-opt -O{}: main.wasm {before} -> {after} bytes",
        config.level
    );
    Ok(())
}

/// Resolves the `wasm-opt` binary and reports which precedence tier fired: the
/// `WASM_OPT_PATH` override, then `PATH`, then the infs-managed Binaryen under
/// `<root>/tools/binaryen/`.
///
/// `Ok(None)` means `wasm-opt` was found nowhere — the caller decides whether
/// that is a hard error or triggers `auto-install`. An invalid `WASM_OPT_PATH`
/// override is an `Err`, never a silent fallthrough: a user who set the variable
/// gets their mistake surfaced rather than papered over (in particular, an
/// `auto-install` build must not download over a typo'd override).
///
/// Emits an `INFS_VERBOSE` trace naming the winning tier and path.
///
/// # Errors
///
/// Errors when `WASM_OPT_PATH` is set but does not name a file.
fn resolve_wasm_opt_with_source() -> Result<Option<(PathBuf, WasmOptSource)>> {
    let managed = ToolchainPaths::new()
        .ok()
        .and_then(|paths| binaryen::installed_wasm_opt(&paths));
    let resolved = resolve_wasm_opt_from(std::env::var_os(WASM_OPT_PATH_ENV).as_deref(), managed)?;
    if let Some((path, source)) = &resolved {
        trace_resolved(*source, path);
    }
    Ok(resolved)
}

/// The testable core of [`resolve_wasm_opt_with_source`], with the environment
/// override and the managed-install lookup lifted into parameters so tests need
/// not mutate process-global state.
///
/// Precedence: a `WASM_OPT_PATH` override (`override_path`) wins, then a `PATH`
/// lookup, then `managed`. A `Some` override must name an existing file or the
/// call errors (naming the env var and the path) — an explicit override is
/// never discarded in favor of a lower tier. `Ok(None)` means nothing resolved
/// in any tier.
///
/// # Errors
///
/// Errors when `override_path` is `Some` but does not name a file.
fn resolve_wasm_opt_from(
    override_path: Option<&OsStr>,
    managed: Option<PathBuf>,
) -> Result<Option<(PathBuf, WasmOptSource)>> {
    if let Some(raw) = override_path {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(Some((path, WasmOptSource::EnvOverride)));
        }
        bail!(
            "`{WASM_OPT_PATH_ENV}` is set to `{}`, which is not a file. Point it \
             at a `wasm-opt` executable, or unset it to search PATH.",
            path.display()
        );
    }

    if let Ok(path) = which::which("wasm-opt") {
        return Ok(Some((path, WasmOptSource::SystemPath)));
    }

    Ok(managed.map(|path| (path, WasmOptSource::ManagedTools)))
}

/// Downloads the pinned Binaryen `wasm-opt` at build time for a
/// `[build.wasm-opt] auto-install = true` project whose `wasm-opt` resolved
/// nowhere, returning the path to the freshly installed binary.
///
/// # Errors
///
/// Errors if the toolchain directory cannot be prepared, the platform cannot be
/// detected, or the download / verification / install fails — with remediation
/// naming a retry, the manual `infs component add wasm-opt`, and disabling the
/// optimizer.
fn auto_install_wasm_opt() -> Result<PathBuf> {
    println!("wasm-opt not found; [build.wasm-opt] auto-install is enabled.");
    let paths = ToolchainPaths::new()?;
    paths.ensure_directories()?;
    let platform = Platform::detect()?;
    binaryen::install_blocking(&paths, platform).with_context(|| {
        format!(
            "Failed to auto-install the Binaryen `wasm-opt` optimizer ({}). \
             Retry the build, install it manually with `infs component add \
             wasm-opt`, or disable optimization (`enabled = false` under \
             `[build.wasm-opt]`, or pass `--no-wasm-opt`).",
            binaryen::BINARYEN_PIN
        )
    })
}

/// The install-hint error for a `wasm-opt` that resolved in no tier. Leads with
/// the infs-managed option, then the system package managers, and points at
/// `auto-install` for a hands-off setup.
fn missing_wasm_opt_error() -> anyhow::Error {
    anyhow::anyhow!(
        "wasm-opt not found.\n\n\
         `[build.wasm-opt]` is enabled but the Binaryen `wasm-opt` optimizer \
         could not be located. To install Binaryen:\n  \
         - Managed by infs: infs component add wasm-opt\n  \
         - macOS: brew install binaryen\n  \
         - Linux: apt install binaryen  (or your distribution's package)\n  \
         - npm: npm install -g binaryen\n  \
         - Or download a release: https://github.com/WebAssembly/binaryen/releases\n\n\
         Set `auto-install = true` under `[build.wasm-opt]` to download it \
         automatically at build time. Then ensure `wasm-opt` is in PATH, or set \
         {WASM_OPT_PATH_ENV} to its full path. To build without optimization, \
         set `enabled = false` under `[build.wasm-opt]`, or pass `--no-wasm-opt`."
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

/// Runs `wasm-opt --version` and returns the parsed major version, or `None` if
/// the probe cannot be run, exits unsuccessfully, or emits an unparseable
/// banner. Unlike [`check_wasm_opt_version`] this makes no judgement and emits
/// no warnings — it is the classifier [`doctor_check`] uses to pick OK vs WARN.
fn probe_wasm_opt_version(wasm_opt: &Path) -> Option<u32> {
    let output = Command::new(wasm_opt).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_wasm_opt_version(&String::from_utf8_lossy(&output.stdout))
}

/// The `infs doctor` health check for `wasm-opt`.
///
/// Reports the resolved binary and its precedence tier when one is found and its
/// version parses; warns when a resolved binary's `--version` fails (a managed
/// copy gets a repair hint — this catches the macOS missing-dylib case); and
/// reports the optional never-installed state as OK so a project that does not
/// use `[build.wasm-opt]` is never alarmed. A `tools/binaryen` directory without
/// the pinned binary, and an invalid `WASM_OPT_PATH`, both warn with remediation.
///
/// The message is always a single line, per the `[OK|WARN|FAIL] name: message`
/// contract the VS Code extension parses.
#[must_use]
pub(crate) fn doctor_check() -> DoctorCheck {
    const NAME: &str = "wasm-opt";
    let paths = ToolchainPaths::new().ok();
    let managed = paths.as_ref().and_then(binaryen::installed_wasm_opt);

    match resolve_wasm_opt_from(
        std::env::var_os(WASM_OPT_PATH_ENV).as_deref(),
        managed.clone(),
    ) {
        Ok(Some((path, source))) => wasm_opt_doctor_found(NAME, &path, source, managed.as_deref()),
        Ok(None) => wasm_opt_doctor_absent(NAME, paths.as_ref()),
        Err(err) => DoctorCheck::warning(NAME, err.to_string()),
    }
}

/// Builds the doctor line for a resolved `wasm-opt`: OK with the source tier and
/// Binaryen version when `--version` parses (noting a managed copy shadowed by a
/// `PATH` hit), or WARN when the version probe fails.
fn wasm_opt_doctor_found(
    name: &str,
    path: &Path,
    source: WasmOptSource,
    managed: Option<&Path>,
) -> DoctorCheck {
    let Some(version) = probe_wasm_opt_version(path) else {
        let hint = if source == WasmOptSource::ManagedTools {
            "run 'infs component add wasm-opt' to repair"
        } else {
            "check the installation"
        };
        return DoctorCheck::warning(
            name,
            format!(
                "Found at {} (source: {}) but `wasm-opt --version` failed; {hint}.",
                path.display(),
                source.label()
            ),
        );
    };

    let mut message = format!(
        "Found at {} (source: {}, Binaryen {version})",
        path.display(),
        source.label()
    );
    if source == WasmOptSource::SystemPath
        && let Some(managed) = managed
    {
        let _ = write!(
            message,
            "; managed copy at {} is shadowed by PATH",
            managed.display()
        );
    }
    DoctorCheck::ok(name, message)
}

/// Builds the doctor line when no `wasm-opt` resolved: WARN when a managed
/// Binaryen directory is present but missing its binary (a broken install to
/// repair), otherwise the optional never-installed OK line that leaves
/// non-users unalarmed.
fn wasm_opt_doctor_absent(name: &str, paths: Option<&ToolchainPaths>) -> DoctorCheck {
    if let Some(paths) = paths {
        let dir = paths.binaryen_dir(binaryen::BINARYEN_PIN);
        if dir.exists() {
            return DoctorCheck::warning(
                name,
                format!(
                    "A managed Binaryen install at {} is missing its wasm-opt \
                     binary; run 'infs component add wasm-opt' to repair.",
                    dir.display()
                ),
            );
        }
    }
    DoctorCheck::ok(
        name,
        "Not installed (optional — needed only for [build.wasm-opt]; install \
         with 'infs component add wasm-opt')",
    )
}

/// What the pre-optimization scan found in `out/main.wasm`.
///
/// The two states are mutually exclusive by construction, so a caller can never
/// read a bulk-memory verdict off an artifact the scan rejected outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactScan {
    /// A verification-only construct leaked into an ordinary function; the
    /// payload is its source spelling (e.g. `"forall"`, `"i32.uzumaki"`).
    VerificationConstruct(&'static str),
    /// An ordinary executable artifact, and whether it carries any bulk-memory
    /// operator.
    Executable { uses_bulk_memory: bool },
}

/// Scans `wasm_bytes` once for both facts the optimizer needs up front: whether
/// a verification-only construct leaked into the artifact, and whether the
/// artifact carries bulk memory.
///
/// Compile-mode builds strip `spec` blocks, so a well-formed executable artifact
/// carries no verification construct. Finding one means it leaked into an
/// ordinary function — `wasm-opt` would reject the unknown `0xfc` opcode with an
/// opaque error, so the scan stops there and lets the caller surface it with
/// remediation instead.
///
/// # Errors
///
/// Errors if the artifact cannot be parsed as WebAssembly.
fn scan_artifact(wasm_bytes: &[u8]) -> Result<ArtifactScan> {
    let mut uses_bulk_memory = false;
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload =
            payload.map_err(|err| anyhow::anyhow!("failed to scan out/main.wasm: {err}"))?;
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
                return Ok(ArtifactScan::VerificationConstruct(name));
            }
            uses_bulk_memory |= is_bulk_memory(&op);
        }
    }
    Ok(ArtifactScan::Executable { uses_bulk_memory })
}

/// Whether `op` belongs to the bulk-memory proposal.
///
/// Two sanctioned sources put one of these in a built artifact: a project that
/// opts in with `[build] wasm-features = ["bulk-memory"]`, in which case codegen
/// emits `memory.copy`/`memory.fill` directly; and a statically merged external
/// module, which the linker's supported-feature envelope admits regardless of
/// what the project requested. Neither is distinguishable here, and neither needs
/// to be — the predicate answers what the bytes contain. The segment-indexed
/// forms are included even though the merge rejects them today and codegen never
/// emits them, so that a widened linker or codegen cannot silently produce an
/// artifact Binaryen is not told to parse.
fn is_bulk_memory(op: &Operator) -> bool {
    use Operator::{DataDrop, MemoryCopy, MemoryFill, MemoryInit};
    matches!(
        op,
        MemoryFill { .. } | MemoryCopy { .. } | MemoryInit { .. } | DataDrop { .. }
    )
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
/// Binaryen versions, and `--enable-mutable-globals` re-admits the one proposal
/// Inference codegen always relies on: the exported mutable `__stack_pointer`
/// global.
///
/// Bulk memory reaches an artifact from either of two sanctioned sources: a
/// project that opts in with `[build] wasm-features = ["bulk-memory"]`, or a
/// statically merged external module (the linker accepts
/// `memory.copy`/`memory.fill` from one whatever the project requested). Binaryen
/// hard-rejects those bytes unless told to parse them, so
/// `--enable-bulk-memory` is forwarded exactly when `enable_bulk_memory` reports
/// that the *input* carries such an operator. Keying on the input rather than on
/// either source keeps one rule for both, and withholding the flag everywhere
/// else is what stops Binaryen from introducing bulk memory into an artifact that
/// had none.
///
/// `-O<level>` works uniformly for every value `WasmOptConfig` validates, so
/// there is no second mapping table.
fn wasm_opt_args(
    level: &str,
    input: &Path,
    output: &Path,
    enable_bulk_memory: bool,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from(format!("-O{level}")),
        OsString::from("--mvp-features"),
        OsString::from("--enable-mutable-globals"),
    ];
    if enable_bulk_memory {
        args.push(OsString::from("--enable-bulk-memory"));
    }
    args.push(input.as_os_str().to_os_string());
    args.push(OsString::from("-o"));
    args.push(output.as_os_str().to_os_string());
    args
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
/// `uses_bulk_memory` is the pre-scan's verdict on the input, and governs both
/// the forwarded feature flags and the re-validation envelope so the two cannot
/// drift apart.
///
/// # Errors
///
/// Errors if `wasm-opt` cannot be spawned, exits nonzero, produces output that
/// cannot be read or fails re-validation, or if the final rename fails.
fn optimize_in_place(
    wasm_opt: &Path,
    level: &str,
    wasm_path: &Path,
    uses_bulk_memory: bool,
) -> Result<()> {
    let tmp_path = optimized_tmp_path(wasm_path);
    let args = wasm_opt_args(level, wasm_path, &tmp_path, uses_bulk_memory);

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

    if let Err(err) = validate_optimized(&optimized, uses_bulk_memory) {
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

/// Re-validates `bytes` against the narrowest envelope the input artifact
/// justified, guarding against a `wasm-opt` that emits something outside the
/// executable subset the pipeline supports.
///
/// The baseline is WebAssembly 1.0 plus the mutable `__stack_pointer` global
/// (`GC_TYPES` is the parser fork's value-type flag, not a proposal opt-in).
/// `BULK_MEMORY` joins it only when `allow_bulk_memory` records that the
/// pre-optimization artifact already carried bulk operators — which it would be
/// wrong to reject after a project opted into them or the linker accepted them
/// from an external module. For the ordinary bulk-free artifact, leaving
/// `BULK_MEMORY` out is precisely what makes this a guard: an optimizer that
/// introduced `memory.copy` or `memory.fill` fails here instead of shipping.
///
/// # Errors
///
/// Errors with the validator's message when `bytes` is not valid WebAssembly
/// within that feature set.
fn validate_optimized(bytes: &[u8], allow_bulk_memory: bool) -> Result<()> {
    let mut features = WasmFeatures::GC_TYPES.union(WasmFeatures::MUTABLE_GLOBAL);
    if allow_bulk_memory {
        features = features.union(WasmFeatures::BULK_MEMORY);
    }
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

    /// Like [`module_with_raw_body`] but the module also declares a one-page
    /// memory, so a body exercising memory operators can be *validated* rather
    /// than merely parsed.
    fn module_with_memory_and_raw_body(body: &[u8]) -> Vec<u8> {
        use wasm_encoder::{
            CodeSection, Function, FunctionSection, MemorySection, MemoryType, Module, TypeSection,
        };
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut memories = MemorySection::new();
        memories.memory(MemoryType {
            minimum: 1,
            maximum: Some(1),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memories);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        f.raw(body.iter().copied());
        code.function(&f);
        module.section(&code);
        module.finish()
    }

    /// `i32.const 0` three times, `memory.fill 0`, `end` — a well-typed
    /// bulk-memory body over the single shared memory.
    const MEMORY_FILL_BODY: &[u8] = &[0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0b, 0x00, 0x0b];

    /// `i32.const 0` three times, `memory.copy 0 0`, `end`.
    const MEMORY_COPY_BODY: &[u8] = &[
        0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0a, 0x00, 0x00, 0x0b,
    ];

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
        // A bulk-free input: no --enable-bulk-memory, so Binaryen cannot
        // introduce an instruction family the artifact did not already use.
        let args = wasm_opt_args("z", Path::new("in.wasm"), Path::new("out.wasm.opt"), false);
        let expected: Vec<OsString> = [
            "-Oz",
            "--mvp-features",
            "--enable-mutable-globals",
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
        let args = wasm_opt_args("3", Path::new("a.wasm"), Path::new("b.wasm.opt"), false);
        let expected: Vec<OsString> = [
            "-O3",
            "--mvp-features",
            "--enable-mutable-globals",
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
    fn wasm_opt_args_enable_bulk_memory_for_a_bulk_bearing_input() {
        // An artifact that carries bulk memory — whether from a project that
        // opted in or from a linked external module — must be parseable by
        // Binaryen, so the flag is appended after the always-on enables and
        // before the input path.
        let args = wasm_opt_args("z", Path::new("in.wasm"), Path::new("out.wasm.opt"), true);
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
    fn resolve_from_override_wins_over_managed() {
        // A valid WASM_OPT_PATH override short-circuits before the PATH lookup
        // and the managed tier, and reports the EnvOverride source.
        let dir = assert_fs::TempDir::new().unwrap();
        let fake = dir.child("wasm-opt");
        fake.write_str("#!/bin/sh\n").unwrap();
        let managed = dir.path().join("managed-wasm-opt");

        let (path, source) = resolve_wasm_opt_from(Some(fake.path().as_os_str()), Some(managed))
            .unwrap()
            .expect("a valid override must resolve");
        assert_eq!(path, fake.path());
        assert_eq!(source, WasmOptSource::EnvOverride);
    }

    #[test]
    fn resolve_from_rejects_nonexistent_override_even_with_managed() {
        // An invalid override is an error even when a managed copy is available:
        // a user's mistake is surfaced, never silently papered over by a lower
        // tier (an auto-install build must not download over a typo).
        let dir = assert_fs::TempDir::new().unwrap();
        let missing = dir.path().join("nope-wasm-opt");
        let managed = dir.path().join("managed-wasm-opt");
        let err = resolve_wasm_opt_from(Some(missing.as_os_str()), Some(managed)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(WASM_OPT_PATH_ENV) && msg.contains("not a file"),
            "an override that is not a file must name the env var and the failure, got: {msg}"
        );
    }

    #[test]
    fn resolve_from_reports_nothing_when_no_tier_resolves() {
        // No override, no managed, and a PATH lookup that (in the vanishing
        // chance a real wasm-opt is present) would be the only hit — the
        // contract is that a genuine miss is Ok(None). Guarded by clearing the
        // managed tier; the PATH-dependent cases are covered by the serial and
        // integration tests.
        let resolved = resolve_wasm_opt_from(None, None);
        assert!(
            matches!(resolved, Ok(None | Some((_, WasmOptSource::SystemPath)))),
            "with no override and no managed tier, resolution is either a PATH \
             hit or Ok(None), got: {resolved:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn resolve_from_uses_managed_when_path_misses() {
        // With no override and an empty PATH, the managed tier is the fallback
        // and is reported as ManagedTools.
        let dir = assert_fs::TempDir::new().unwrap();
        let managed = dir.path().join("managed-wasm-opt");
        std::fs::write(&managed, b"managed").unwrap();

        let original = std::env::var_os("PATH");
        // SAFETY: serialized test; PATH restored immediately below.
        unsafe {
            std::env::set_var("PATH", "");
        }
        let resolved = resolve_wasm_opt_from(None, Some(managed.clone()));
        // SAFETY: restore regardless of the assertion outcome.
        unsafe {
            match original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let (path, source) = resolved
            .unwrap()
            .expect("managed must resolve when PATH misses");
        assert_eq!(path, managed);
        assert_eq!(source, WasmOptSource::ManagedTools);
    }

    // Requires an executable stub on PATH; `which` only accepts an executable
    // file, so this is gated to unix where a chmod is portable.
    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn resolve_from_path_wins_over_managed() {
        use std::os::unix::fs::PermissionsExt;

        let path_dir = assert_fs::TempDir::new().unwrap();
        let on_path = path_dir.path().join("wasm-opt");
        std::fs::write(&on_path, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&on_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let managed_dir = assert_fs::TempDir::new().unwrap();
        let managed = managed_dir.path().join("managed-wasm-opt");
        std::fs::write(&managed, b"managed").unwrap();

        let original = std::env::var_os("PATH");
        // SAFETY: serialized test; PATH restored immediately below.
        unsafe {
            std::env::set_var("PATH", path_dir.path());
        }
        let resolved = resolve_wasm_opt_from(None, Some(managed.clone()));
        // SAFETY: restore regardless of the assertion outcome.
        unsafe {
            match original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        // If `which` located our stub, PATH must win over managed. In a
        // restricted sandbox where `which` cannot see it, fall through to the
        // managed tier — both are acceptable; what must never happen is PATH
        // losing to managed when PATH did resolve.
        let (path, source) = resolved.unwrap().expect("a tier must resolve");
        if source == WasmOptSource::SystemPath {
            assert_eq!(
                path.canonicalize().unwrap(),
                on_path.canonicalize().unwrap(),
                "the PATH hit must be the stub, not the managed copy"
            );
        } else {
            assert_eq!(source, WasmOptSource::ManagedTools);
        }
    }

    #[test]
    fn wasm_opt_source_labels_are_stable() {
        // The labels are a contract shared by the INFS_VERBOSE trace and the
        // doctor line; they must not drift.
        assert_eq!(WasmOptSource::EnvOverride.label(), "WASM_OPT_PATH env");
        assert_eq!(WasmOptSource::SystemPath.label(), "PATH");
        assert_eq!(WasmOptSource::ManagedTools.label(), "managed tools");
    }

    #[test]
    fn doctor_absent_reports_optional_ok_without_managed_residue() {
        // No managed Binaryen directory: the never-installed state is optional
        // OK so a project that does not use `[build.wasm-opt]` is never alarmed.
        let root = assert_fs::TempDir::new().unwrap();
        let paths = ToolchainPaths::with_root(root.path().to_path_buf());
        let check = wasm_opt_doctor_absent("wasm-opt", Some(&paths));
        assert_eq!(check.prefix(), "[OK]");
        assert!(check.message.contains("Not installed (optional"));
        assert!(check.message.contains("infs component add wasm-opt"));
    }

    #[test]
    fn doctor_absent_warns_on_broken_managed_dir() {
        // A managed directory without the binary is a broken install: WARN with
        // the repair hint.
        let root = assert_fs::TempDir::new().unwrap();
        let paths = ToolchainPaths::with_root(root.path().to_path_buf());
        std::fs::create_dir_all(paths.binaryen_dir(binaryen::BINARYEN_PIN)).unwrap();
        let check = wasm_opt_doctor_absent("wasm-opt", Some(&paths));
        assert_eq!(check.prefix(), "[WARN]");
        assert!(check.message.contains("repair"));
        assert!(check.message.contains("infs component add wasm-opt"));
    }

    /// Writes an executable stub at `<dir>/wasm-opt` printing `version_output`
    /// (empty means print nothing) and exiting `exit_code`.
    ///
    /// The stub is settled before it is returned: `probe_wasm_opt_version` maps
    /// a failed spawn to `None`, so an `ETXTBSY` there would surface as a
    /// missing version rather than an error a retry could see.
    #[cfg(unix)]
    fn write_version_stub(
        dir: &assert_fs::TempDir,
        version_output: &str,
        exit_code: i32,
    ) -> PathBuf {
        use assert_fs::prelude::*;
        use std::os::unix::fs::PermissionsExt;
        let stub = dir.child("wasm-opt");
        stub.write_str(&format!(
            "#!/bin/sh\necho '{version_output}'\nexit {exit_code}\n"
        ))
        .unwrap();
        std::fs::set_permissions(stub.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        crate::testing::settle_executable(stub.path(), &["--version"]);
        stub.path().to_path_buf()
    }

    #[cfg(unix)]
    #[test]
    fn doctor_found_warns_when_version_probe_fails() {
        // A resolved binary whose `--version` exits nonzero warns; a managed
        // copy gets the repair hint (this is the macOS missing-dylib case),
        // everything else the generic "check the installation".
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_version_stub(&dir, "", 1);

        let managed = wasm_opt_doctor_found("wasm-opt", &stub, WasmOptSource::ManagedTools, None);
        assert_eq!(managed.prefix(), "[WARN]");
        assert!(
            managed
                .message
                .contains("run 'infs component add wasm-opt' to repair")
        );

        let system = wasm_opt_doctor_found("wasm-opt", &stub, WasmOptSource::SystemPath, None);
        assert_eq!(system.prefix(), "[WARN]");
        assert!(system.message.contains("check the installation"));
    }

    #[cfg(unix)]
    #[test]
    fn doctor_found_ok_notes_managed_shadowed_by_path() {
        // A PATH hit that parses a version, with a managed copy also present,
        // is OK and notes the managed copy is shadowed by PATH.
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_version_stub(&dir, "wasm-opt version 118 (test)", 0);
        let managed = dir.path().join("managed-wasm-opt");

        let check =
            wasm_opt_doctor_found("wasm-opt", &stub, WasmOptSource::SystemPath, Some(&managed));
        assert_eq!(check.prefix(), "[OK]");
        assert!(
            check.message.contains("source: PATH, Binaryen 118"),
            "the OK line must name the tier and version, got: {}",
            check.message
        );
        assert!(
            check.message.contains("shadowed by PATH"),
            "a shadowed managed copy must be noted, got: {}",
            check.message
        );
    }

    #[test]
    fn scan_artifact_detects_each_nondet_block() {
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
                scan_artifact(&module).unwrap(),
                ArtifactScan::VerificationConstruct(name),
                "sub-opcode {sub_opcode:#x} must be reported as `{name}`"
            );
        }
    }

    #[test]
    fn scan_artifact_detects_uzumaki() {
        // `<uzumaki> drop; end`, for both the i32 and i64 forms.
        let i32_body = [0x00, 0xfc, 0x31, 0x1a, 0x0b];
        assert_eq!(
            scan_artifact(&module_with_raw_body(&i32_body)).unwrap(),
            ArtifactScan::VerificationConstruct("i32.uzumaki")
        );
        let i64_body = [0x00, 0xfc, 0x32, 0x1a, 0x0b];
        assert_eq!(
            scan_artifact(&module_with_raw_body(&i64_body)).unwrap(),
            ArtifactScan::VerificationConstruct("i64.uzumaki")
        );
    }

    #[test]
    fn scan_artifact_reports_a_plain_body_as_bulk_free() {
        // An ordinary executable body (just `end`) carries neither a
        // verification-only opcode nor bulk memory.
        let module = module_with_raw_body(&[0x0b]);
        assert_eq!(
            scan_artifact(&module).unwrap(),
            ArtifactScan::Executable {
                uses_bulk_memory: false
            }
        );
    }

    #[test]
    fn scan_artifact_detects_each_bulk_memory_operator() {
        // The four bulk-memory operators, each in an otherwise ordinary body.
        // `memory.init 0 0` and `data.drop 0` decode without their segments;
        // the scan reads operators, it does not validate.
        let memory_init: &[u8] = &[
            0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x08, 0x00, 0x00, 0x0b,
        ];
        let data_drop: &[u8] = &[0xfc, 0x09, 0x00, 0x0b];
        for (body, name) in [
            (MEMORY_FILL_BODY, "memory.fill"),
            (MEMORY_COPY_BODY, "memory.copy"),
            (memory_init, "memory.init"),
            (data_drop, "data.drop"),
        ] {
            let module = module_with_raw_body(body);
            assert_eq!(
                scan_artifact(&module).unwrap(),
                ArtifactScan::Executable {
                    uses_bulk_memory: true
                },
                "{name} must be reported as bulk memory"
            );
        }
    }

    #[test]
    fn scan_artifact_reports_verification_construct_ahead_of_bulk_memory() {
        // A leaked construct is a hard error, so it wins over the bulk verdict
        // even when both are present — the caller never has to choose.
        let mut body = vec![0xfc, 0x31, 0x1a];
        body.extend_from_slice(MEMORY_FILL_BODY);
        assert_eq!(
            scan_artifact(&module_with_raw_body(&body)).unwrap(),
            ArtifactScan::VerificationConstruct("i32.uzumaki")
        );
    }

    #[test]
    fn validate_optimized_rejects_bulk_memory_the_input_did_not_have() {
        // The guard that matters: an optimizer that introduced bulk memory into
        // a clean artifact fails re-validation instead of shipping.
        let module = module_with_memory_and_raw_body(MEMORY_FILL_BODY);
        assert!(
            validate_optimized(&module, false).is_err(),
            "bulk memory must not validate when the input artifact carried none"
        );
        assert!(
            validate_optimized(&module, true).is_ok(),
            "the same module must validate once the input justified bulk memory"
        );
    }

    #[test]
    fn validate_optimized_accepts_a_plain_module_under_the_strict_envelope() {
        // Wasm 1.0 output validates without any bulk-memory opt-in.
        let module = module_with_memory_and_raw_body(&[0x0b]);
        assert!(validate_optimized(&module, false).is_ok());
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
        let err = crate::testing::retry_while_exec_busy(|| {
            optimize_in_place(&fake, "z", &wasm_path, false)
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("wasm-opt failed"),
            "a nonzero exit must surface as a wasm-opt failure, got: {err:#}"
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
            auto_install: false,
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

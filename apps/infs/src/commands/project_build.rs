//! Shared project-build helper for the infs CLI.
//!
//! Both `infs build` (project mode) and `infs run` (project mode) need to
//! perform the *same* project compilation: resolve the conventional
//! `src/main.inf` entry point, run the `infc` compatibility handshake, spawn
//! `infc` with its working directory set to the project root so `out/` lands at
//! the root, and apply the optional `[build.wasm-opt]` post-build optimization
//! to the resulting executable. This module owns that shared logic so the two
//! command modules do not duplicate it (and so `run` inherits both the
//! handshake and the optimizer "for free", running exactly what it ships).
//!
//! `infc` compiles the whole import-reachable closure starting at
//! `src/main.inf` and is the sole authority on which files are part of the
//! build: it warns about genuinely-unreachable `src/**/*.inf` files itself.
//! Because `infc` is spawned with inherited stdio, those warnings (and any
//! errors) reach the user directly. `infs` therefore adds no file-discovery or
//! warning logic of its own.
//!
//! It lives under `commands/` rather than `project/` because it is
//! command-execution logic (subprocess spawning, exit-code propagation, the
//! ABI handshake), the same category as [`crate::commands::build`] and
//! [`crate::commands::run`]. The `project/` module is deliberately scoped to
//! filesystem walking and manifest parsing; placing subprocess-spawning code
//! there would blur that boundary.
//!
//! The compatibility handshake ([`probe_compiler_compatibility`]) also lives
//! here: it is part of "running a project build", and keeping it beside the
//! single spawning site keeps the coupling tight. Every caller wants the probed
//! capability, not just the pass/fail — the additive flags `infs` forwards
//! (`--out-dir`, `--wasm-features`) are each gated on it.
//!
//! ## Which settings are parameters and which are read off the context
//!
//! [`run_project_build`] takes `mode` and `out_dir` as parameters but reads
//! `[build] wasm-features` straight off `ctx`. The rule: a setting a CLI flag can
//! override, or that a caller must be able to suppress, is threaded so the
//! caller stays the single place that resolves it (`run` deliberately passes
//! `mode = None` to force compile mode). A setting only the manifest can express,
//! with no flag and nothing to suppress, is read from `ctx` — threading it would
//! let two callers disagree about a property of the project itself. An
//! instruction-set request is the latter: `build` and `run` emitting different
//! instruction levels for one project is a bug, not a configuration.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::commands::build::BuildMode;
use crate::errors::InfsError;
use crate::project::ProjectContext;
use crate::project::manifest::MANIFEST_FILE_NAME;
use crate::toolchain::find_infc;
use inference_compiler_interface::{
    COMPILER_ABI_MAJOR, COMPILER_ABI_MINOR, WasmFeatureName, render_feature_list,
};

/// Compiles the entry point of a discovered project (project mode).
///
/// Shared by `infs build` and `infs run`. Resolves the conventional
/// `src/main.inf` entry point, runs the `infc` compatibility handshake, then
/// spawns `infc` with its working directory set to the project root so that
/// `out/` lands at the root regardless of where the command was invoked. `infc`
/// follows the import-reachable closure from `src/main.inf` and reports
/// unreachable files itself; those messages reach the user through inherited
/// stdio.
///
/// The entry-point source is passed *relative to the root* (`src/main.inf`),
/// matching the CWD-relativity contract between `infs` and `infc`: `infc`
/// resolves both its source argument and `out/` against the inherited CWD.
///
/// The forwarded flags are passed explicitly rather than as a `BuildArgs` so
/// `run` need not depend on `build`'s argument struct. Only what the caller
/// resolved is forwarded; `infc::normalize_args` owns the `-v` ↔ `--mode proof`
/// implication, so mirroring it here would create a second source of truth that
/// could drift. The single forwarding/spawn site lives here so the ABI gate on
/// `--out-dir` sits next to the spawn.
///
/// `out_dir`, when `Some`, is forwarded as `--out-dir <dir>`; this is only ever
/// supplied by `build`'s effective-proof-mode path. The shared helper gates the
/// forward on the resolved `infc` actually supporting the flag and hard-errors
/// with remediation otherwise — it never emits the flag blind.
/// `infs run` always passes `out_dir = None` (and `mode = None`), so project
/// `run` always builds an executable in `out/`.
///
/// The manifest's `[build] wasm-features` is read straight off `ctx` rather than
/// passed in, so `build` and `run` cannot disagree about the instruction level of
/// the module they produce, and it applies in both compile and proof mode — a
/// `.v` that described a different instruction set than the shipped `.wasm` would
/// be worthless. The forward is gated exactly like `--out-dir`, and the resolved
/// set is echoed to stdout.
///
/// After a successful `infc` exit, the optional `[build.wasm-opt]` post-build
/// optimization is applied to `<root>/out/main.wasm` (see
/// [`crate::commands::wasm_opt::post_build_optimize`]). `no_wasm_opt` (the
/// `--no-wasm-opt` flag) suppresses it, as do proof/`-v` builds; when no
/// `[build.wasm-opt]` table is present it is a no-op.
///
/// ## Errors
///
/// Returns an error if:
/// - The entry point `<root>/src/main.inf` does not exist
/// - infc compiler cannot be found
/// - infc reports a *major* ABI version mismatch (hard error with remediation)
/// - `out_dir` is requested but the resolved `infc` does not support `--out-dir`
/// - the manifest requests `wasm-features` the resolved `infc` cannot honor
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
/// - post-build optimization is active and fails (missing/invalid artifact,
///   `wasm-opt` resolution, or the optimization itself)
pub(crate) fn run_project_build(
    ctx: &ProjectContext,
    generate_v_output: bool,
    mode: Option<BuildMode>,
    out_dir: Option<&Path>,
    no_wasm_opt: bool,
) -> Result<()> {
    if !ctx.entry_point.is_file() {
        bail!(
            "Missing entry point: expected `{}`. Project mode compiles \
             `src/main.inf` by convention; create it, or pass a source file \
             path (`infs build path/to/file.inf`).",
            ctx.entry_point.display()
        );
    }

    let infc_path = find_infc()?;
    let compat = probe_compiler_compatibility(&infc_path)?;

    let entry_relative = ProjectContext::entry_relative();

    let mut cmd = Command::new(&infc_path);
    cmd.current_dir(&ctx.root).arg(&entry_relative);

    if generate_v_output {
        cmd.arg("-v");
    }

    // Forward only what the caller resolved (see module docs).
    if let Some(mode) = mode {
        cmd.arg("--mode").arg(mode_flag(mode));
    }

    // Forward `--out-dir` only to an infc known to support it; never blind.
    if let Some(dir) = out_dir {
        if !compat.supports_out_dir() {
            bail!(
                "the resolved infc does not support `--out-dir` (requires infc \
                 ABI ≥ 1.1); update the toolchain or remove `[verification] \
                 output-dir` from Inference.toml."
            );
        }
        cmd.arg("--out-dir").arg(dir);
    }

    let features = ctx.manifest.build.resolved_wasm_features()?;
    forward_wasm_features(
        &mut cmd,
        compat,
        &features,
        Some(&ctx.root.join(MANIFEST_FILE_NAME)),
    )?;

    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute infc at {}", infc_path.display()))?;

    if status.success() {
        crate::commands::wasm_opt::post_build_optimize(ctx, generate_v_output, mode, no_wasm_opt)?;
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        Err(InfsError::process_exit_code(code).into())
    }
}

/// Maps a [`BuildMode`] to the `infc --mode` flag value.
///
/// Shared by the project path and the single-file path in
/// [`crate::commands::build`].
pub(crate) fn mode_flag(mode: BuildMode) -> &'static str {
    match mode {
        BuildMode::Proof => "proof",
        BuildMode::Compile => "compile",
    }
}

/// The capability of a resolved `infc`, as established by the handshake.
///
/// Distinguishes *tolerated* drift (which the handshake only warns about) from
/// *actively used* features such as `--out-dir`, which must only be sent to an
/// `infc` known to support them. `commit_matched` is the strongest signal — the
/// binaries were built from the same tree — and short-circuits the ABI probe
/// entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompilerCompat {
    /// `infc --commit-hash` matched `infs`'s build commit.
    pub commit_matched: bool,

    /// The probed `(major, minor)` ABI, or `None` if the binary does not
    /// understand `--abi-version` (old/unknown).
    pub abi: Option<(u32, u32)>,
}

impl CompilerCompat {
    /// Whether the resolved `infc` is known to support the additive `--out-dir`
    /// flag: either it is the same build (`commit_matched`) or it advertises an
    /// ABI minor ≥ 1 within the supported major. An unknown/old ABI is treated
    /// as unsupported.
    pub fn supports_out_dir(self) -> bool {
        self.supports_abi_minor(1)
    }

    /// Whether the resolved `infc` is known to support the additive
    /// `--wasm-features` flag, which landed at ABI minor 2.
    ///
    /// The conservative reading matters more here than for `--out-dir`: an `infc`
    /// that predates the flag cannot honor an instruction-set request, and
    /// nothing in the ABI lets it say so. Refusing to build beats shipping a
    /// module at an instruction level the manifest did not ask for.
    pub fn supports_wasm_features(self) -> bool {
        self.supports_abi_minor(2)
    }

    /// Whether the resolved `infc` is known to have the additive feature
    /// introduced at `minor`: either it is the same build (`commit_matched`, the
    /// strongest signal) or it advertises at least that minor within the
    /// supported major. An unknown/old ABI is treated as unsupported.
    ///
    /// One predicate per flag is deliberate rather than a single "minimum minor"
    /// accessor: each capability names the minor its flag landed at exactly once,
    /// so a caller cannot accidentally gate a newer flag on an older floor.
    fn supports_abi_minor(self, minor: u32) -> bool {
        if self.commit_matched {
            return true;
        }
        matches!(self.abi, Some((major, advertised)) if major == COMPILER_ABI_MAJOR && advertised >= minor)
    }
}

/// Appends `--wasm-features <list>` to `cmd` when `features` is non-empty, after
/// confirming the resolved `infc` can honor the request, and echoes the resolved
/// set to stdout.
///
/// Every path that spawns `infc` on behalf of a project routes through here:
/// project `build`/`run`, and both single-file paths. The remediation message and
/// the echo format are user-visible contract text, so they exist once.
///
/// The empty check lives inside rather than at the call sites so no caller can
/// emit a bare `--wasm-features ""`, which the flag's comma-splitting would read
/// as a single empty feature name and reject.
///
/// `manifest_path` names the file the remediation tells the user to edit, which
/// matters as soon as a walk was involved: single-file mode may have found a
/// manifest several directories up. `None` can only accompany an empty request —
/// a feature can only have been requested by some manifest — and the fallback
/// keeps the message well-formed regardless.
///
/// # Errors
///
/// Returns a remediation-bearing error when `features` is non-empty and the
/// resolved `infc` predates the flag. The flag is never emitted blind.
pub(crate) fn forward_wasm_features(
    cmd: &mut Command,
    compat: CompilerCompat,
    features: &[WasmFeatureName],
    manifest_path: Option<&Path>,
) -> Result<()> {
    if features.is_empty() {
        return Ok(());
    }
    if !compat.supports_wasm_features() {
        let manifest = manifest_path.map_or_else(
            || String::from(MANIFEST_FILE_NAME),
            |path| path.display().to_string(),
        );
        bail!(
            "the resolved infc does not support `--wasm-features` (requires \
             infc ABI ≥ 1.2); update the toolchain or remove `[build] \
             wasm-features` from {manifest}."
        );
    }
    let list = render_feature_list(features);
    println!("wasm-features: {list}");
    cmd.arg("--wasm-features").arg(list);
    Ok(())
}

/// Runs the `infc` compatibility handshake and returns its capability.
///
/// Sequence (unchanged from the original boolean handshake — same messages):
/// 1. Query `infc --commit-hash`. If it equals `INFS_GIT_COMMIT`, short-circuit —
///    the two binaries were built from the same source tree and the ABI is
///    guaranteed compatible (`commit_matched = true`).
/// 2. Otherwise query `infc --abi-version` and compare against the major/minor
///    constants from `inference-compiler-interface`. Major mismatch is a hard
///    error; minor mismatch is a warning; exact match is silent.
///
/// Old binaries that do not understand the flags (non-zero exit, empty output,
/// or the literal `unknown`) are treated as graceful skips — we neither warn
/// nor error on them, and the returned `abi` is `None`. The L1/L2 resolver
/// fixes remain the correctness guarantee; this handshake is a safety net
/// against residual drift.
///
/// # Errors
///
/// Hard-errors only on a *major* ABI mismatch (with remediation).
pub(crate) fn probe_compiler_compatibility(infc_path: &Path) -> Result<CompilerCompat> {
    // --commit-hash / --abi-version print and exit 0 immediately; no timeout needed.
    let local_commit = env!("INFS_GIT_COMMIT");
    let remote_commit = probe_flag(infc_path, "--commit-hash");

    if let Some(hash) = &remote_commit
        && hash == local_commit
    {
        return Ok(CompilerCompat {
            commit_matched: true,
            abi: None,
        });
    }

    let Some(abi_raw) = probe_flag(infc_path, "--abi-version") else {
        return Ok(CompilerCompat {
            commit_matched: false,
            abi: None,
        });
    };

    let Some((infc_major, infc_minor)) = parse_abi_version(&abi_raw) else {
        return Ok(CompilerCompat {
            commit_matched: false,
            abi: None,
        });
    };

    let local_major = COMPILER_ABI_MAJOR;
    let local_minor = COMPILER_ABI_MINOR;

    if infc_major != local_major {
        bail!(
            "infs ABI {local_major}.{local_minor} but infc reported ABI \
             {infc_major}.{infc_minor}; rebuild the workspace or set \
             INFC_PATH to a matching binary."
        );
    }

    match infc_minor.cmp(&local_minor) {
        std::cmp::Ordering::Greater => {
            eprintln!(
                "warning: infc ABI {infc_major}.{infc_minor} is newer than \
                 infs ABI {local_major}.{local_minor}; infs may not \
                 recognize features emitted by infc."
            );
        }
        std::cmp::Ordering::Less => {
            eprintln!(
                "warning: infs ABI {local_major}.{local_minor} is newer \
                 than infc ABI {infc_major}.{infc_minor}; infs may request \
                 features infc does not provide."
            );
        }
        std::cmp::Ordering::Equal => {}
    }

    Ok(CompilerCompat {
        commit_matched: false,
        abi: Some((infc_major, infc_minor)),
    })
}

/// Spawn attempts for [`probe_flag`] when exec reports `ETXTBSY`. Five tries
/// with linear backoff (10–50 ms) comfortably outlast the sub-millisecond
/// fork/exec window that triggers the race.
const PROBE_BUSY_RETRIES: u32 = 5;

/// Runs `<infc_path> <flag>` with stdin/stderr suppressed and returns the
/// trimmed stdout on success. Returns `None` for any failure mode that an old
/// `infc` lacking the flag would produce: an unspawnable binary, non-zero exit,
/// empty stdout, or the literal `unknown`.
///
/// A freshly built `infc` can transiently fail to exec with `ETXTBSY` ("text
/// file busy") while another process still holds a writable handle to it across
/// that process's fork/exec window. The condition clears within milliseconds, so
/// the spawn is retried a few times rather than misread as a missing flag.
fn probe_flag(infc_path: &Path, flag: &str) -> Option<String> {
    let mut attempt = 0;
    let output = loop {
        match Command::new(infc_path)
            .arg(flag)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) => break output,
            Err(err)
                if err.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && attempt < PROBE_BUSY_RETRIES =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(10 * u64::from(attempt)));
            }
            Err(_) => return None,
        }
    };
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() || value == "unknown" {
        return None;
    }
    Some(value)
}

/// Parses a `"<major>.<minor>"` string into `(major, minor)`. Returns `None`
/// on any parse failure — callers treat that as "skip the ABI check".
fn parse_abi_version(raw: &str) -> Option<(u32, u32)> {
    let (major, minor) = raw.split_once('.')?;
    let major: u32 = major.parse().ok()?;
    let minor: u32 = minor.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod project_tests {
    use super::*;
    use crate::project::manifest::InferenceToml;

    /// `run_project_build` must fail with a remediation error before doing any
    /// compiler lookup when `src/main.inf` is absent. This is platform
    /// independent — it errors before spawning `infc`.
    #[test]
    fn run_project_build_errors_when_entry_missing() {
        let dir = assert_fs::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        // Manifest present, but no src/main.inf.
        let ctx = ProjectContext {
            root: root.clone(),
            manifest: InferenceToml::new("demo"),
            entry_point: root.join("src").join("main.inf"),
        };

        let err = run_project_build(&ctx, false, None, None, false).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Missing entry point") && msg.contains("main.inf"),
            "expected missing-entry remediation, got: {msg}"
        );
    }

    /// A *directory* named `main.inf` must not satisfy the entry-point guard:
    /// `is_file` rejects it with the same remediation rather than letting `infc`
    /// fail opaquely on a directory argument.
    #[test]
    fn run_project_build_errors_when_entry_is_a_directory() {
        let dir = assert_fs::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        // Create src/main.inf as a directory, not a file.
        let entry = root.join("src").join("main.inf");
        std::fs::create_dir_all(&entry).unwrap();
        let ctx = ProjectContext {
            root: root.clone(),
            manifest: InferenceToml::new("demo"),
            entry_point: entry,
        };

        let err = run_project_build(&ctx, false, None, None, false).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Missing entry point") && msg.contains("main.inf"),
            "a directory at the entry-point path must be rejected, got: {msg}"
        );
    }

    /// `CompilerCompat::supports_out_dir` is the `--out-dir` capability
    /// predicate: commit match OR same-major ABI minor ≥ 1. Unknown/old ABIs are
    /// unsupported. Pure logic — no subprocess needed.
    #[test]
    fn supports_out_dir_capability_matrix() {
        // Commit match alone is sufficient (ABI not even probed).
        assert!(
            CompilerCompat {
                commit_matched: true,
                abi: None,
            }
            .supports_out_dir()
        );

        // Same major, minor >= 1 → supported.
        assert!(
            CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR, 1)),
            }
            .supports_out_dir()
        );
        assert!(
            CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR, 5)),
            }
            .supports_out_dir()
        );

        // Same major, minor 0 → not supported (the flag landed at minor 1).
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR, 0)),
            }
            .supports_out_dir()
        );

        // Different major → not supported even at minor >= 1.
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR + 1, 9)),
            }
            .supports_out_dir()
        );

        // Unknown ABI → not supported.
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: None,
            }
            .supports_out_dir()
        );
    }

    /// `CompilerCompat::supports_wasm_features` is the same predicate one minor
    /// later: commit match OR same-major ABI minor ≥ 2. In particular a minor-1
    /// `infc` — which supports `--out-dir` — must NOT be sent `--wasm-features`.
    #[test]
    fn supports_wasm_features_capability_matrix() {
        // Commit match alone is sufficient (ABI not even probed).
        assert!(
            CompilerCompat {
                commit_matched: true,
                abi: None,
            }
            .supports_wasm_features(),
            "a same-build infc supports every flag this infs knows"
        );

        // Same major, minor >= 2 → supported.
        for minor in [2, 7] {
            assert!(
                CompilerCompat {
                    commit_matched: false,
                    abi: Some((COMPILER_ABI_MAJOR, minor)),
                }
                .supports_wasm_features(),
                "minor {minor} must support --wasm-features"
            );
        }

        // The flag landed at minor 2, so 0 and 1 are both unsupported. Minor 1 is
        // the interesting one: the two capabilities must not be conflated.
        for minor in [0, 1] {
            let compat = CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR, minor)),
            };
            assert!(
                !compat.supports_wasm_features(),
                "minor {minor} predates --wasm-features"
            );
        }
        assert!(
            CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR, 1)),
            }
            .supports_out_dir(),
            "minor 1 still supports --out-dir; only the newer flag is gated out"
        );

        // Different major → not supported even at a high minor.
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR + 1, 9)),
            }
            .supports_wasm_features(),
            "a foreign major is incompatible regardless of its minor"
        );

        // Unknown ABI → not supported.
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: None,
            }
            .supports_wasm_features(),
            "an infc that cannot report its ABI must not be sent the flag"
        );
    }

    /// The arguments accumulated on `cmd`, as owned strings.
    fn args_of(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// An empty request appends nothing — and specifically not a bare
    /// `--wasm-features ""`, which the flag's comma-splitting would read as a
    /// single empty feature name and reject. The guard is inside the forwarder so
    /// no call site can get this wrong; an old `infc` is irrelevant when nothing
    /// is being requested of it.
    #[test]
    fn forward_wasm_features_appends_nothing_for_an_empty_request() {
        let mut cmd = Command::new("infc");
        let predates_the_flag = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 0)),
        };
        forward_wasm_features(&mut cmd, predates_the_flag, &[], None)
            .expect("an empty request asks nothing of the compiler");
        assert!(
            args_of(&cmd).is_empty(),
            "an empty request must not put a flag on the command"
        );
    }

    #[test]
    fn forward_wasm_features_appends_the_canonical_rendering() {
        let mut cmd = Command::new("infc");
        let same_build = CompilerCompat {
            commit_matched: true,
            abi: None,
        };
        forward_wasm_features(&mut cmd, same_build, &[WasmFeatureName::BulkMemory], None)
            .expect("a same-build infc supports the flag");
        assert_eq!(args_of(&cmd), ["--wasm-features", "bulk-memory"]);
    }

    /// The gate refuses rather than forwards, and leaves the command untouched so
    /// a caller that mishandled the error could not still spawn with the flag.
    #[test]
    fn forward_wasm_features_refuses_an_infc_that_predates_the_flag() {
        let mut cmd = Command::new("infc");
        let minor_one = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 1)),
        };
        let manifest = Path::new("/projects/demo").join(MANIFEST_FILE_NAME);
        let err = forward_wasm_features(
            &mut cmd,
            minor_one,
            &[WasmFeatureName::BulkMemory],
            Some(&manifest),
        )
        .expect_err("ABI minor 1 predates --wasm-features");
        let msg = err.to_string();
        assert!(
            msg.contains("--wasm-features") && msg.contains("1.2"),
            "the error must name the flag and the required ABI, got: {msg}"
        );
        assert!(
            msg.contains("update the toolchain") && msg.contains("[build] wasm-features"),
            "the error must offer both remediations, got: {msg}"
        );
        assert!(
            msg.contains(&manifest.display().to_string()),
            "the error must name which manifest to edit — single-file mode may \
             have found one several directories up; got: {msg}"
        );
        assert!(
            args_of(&cmd).is_empty(),
            "a refused request must leave no flag on the command"
        );
    }

    /// The entry point is resolved as `<root>/src/main.inf` using path joins,
    /// so the resolved path always ends in the platform-correct components.
    #[test]
    fn entry_point_resolves_to_src_main_inf() {
        let dir = assert_fs::TempDir::new().unwrap();
        InferenceToml::new("demo")
            .write_to_file(
                &dir.path()
                    .join(crate::project::manifest::MANIFEST_FILE_NAME),
            )
            .unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.inf"), "pub fn main() -> i32 { return 0; }\n").unwrap();

        let ctx = crate::project::discover_and_load(dir.path()).unwrap();
        assert_eq!(ctx.entry_point, ctx.root.join("src").join("main.inf"));
        assert!(ctx.entry_point.exists());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// Writes an executable `infc` stub that prints fixed strings for
    /// `--commit-hash` and `--abi-version`. The stub exits 0 by default but
    /// can be configured to exit 1 instead.
    fn write_stub(
        dir: &assert_fs::TempDir,
        commit_stdout: &str,
        abi_stdout: &str,
        exit_nonzero: bool,
    ) -> PathBuf {
        let stub = dir.child("infc");
        let exit_code = i32::from(exit_nonzero);
        let script = format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
               --commit-hash)\n\
                 printf '%s\\n' \"{commit_stdout}\"\n\
                 exit {exit_code}\n\
                 ;;\n\
               --abi-version)\n\
                 printf '%s\\n' \"{abi_stdout}\"\n\
                 exit {exit_code}\n\
                 ;;\n\
               *)\n\
                 exit 0\n\
                 ;;\n\
             esac\n",
        );
        stub.write_str(&script).unwrap();
        let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(stub.path(), perms).unwrap();
        stub.path().to_path_buf()
    }

    #[test]
    fn abi_major_mismatch_is_hard_error() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "nottherightcommit", "2.0", false);
        let err = probe_compiler_compatibility(&stub).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("ABI") && msg.contains("rebuild"),
            "expected remediation message, got: {msg}"
        );
    }

    #[test]
    fn abi_minor_difference_warns_only() {
        let dir = assert_fs::TempDir::new().unwrap();
        // Exercise the "infc minor newer than infs" path. The stub reports a
        // far-future minor ("1.5") so the comparison resolves to Greater for
        // any plausible local COMPILER_ABI_MINOR; the branch warns but does not
        // hard-error.
        let stub = write_stub(&dir, "nottherightcommit", "1.5", false);
        let result = probe_compiler_compatibility(&stub);
        assert!(result.is_ok(), "minor mismatch should not hard-error");
    }

    #[test]
    fn abi_minor_infs_newer_than_infc_warns_only() {
        let dir = assert_fs::TempDir::new().unwrap();
        // Reverse branch: infs newer than infc. This path only became reachable
        // once COMPILER_ABI_MINOR was bumped above 0. A stub reporting the same
        // major but minor "0" (one below the local minor of 1) exercises the
        // Less arm — it must warn, not hard-error. Constructed against the live
        // major constant so it stays valid across future major bumps.
        let abi = format!("{COMPILER_ABI_MAJOR}.0");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let result = probe_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "infs-newer-than-infc minor mismatch must warn, not hard-error; got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn exact_abi_match_with_differing_commit_is_silent_ok() {
        let dir = assert_fs::TempDir::new().unwrap();
        // Commit hashes differ (so the short-circuit in step 1 does NOT fire),
        // forcing the ABI comparison to run. The stub reports EXACTLY the local
        // major.minor, so the comparison resolves to `Ordering::Equal` — the
        // arm that neither warns nor errors. Constructed from the live
        // constants so it tracks future bumps. This is the only direct test of
        // the equal-minor branch; the warn-only tests cover Greater and Less.
        let abi = format!("{COMPILER_ABI_MAJOR}.{COMPILER_ABI_MINOR}");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let result = probe_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "exact ABI match (differing commit) must be a silent Ok via the Equal arm; got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn matching_commit_hash_skips_abi_check() {
        let dir = assert_fs::TempDir::new().unwrap();
        // ABI "9.9" would trigger a major mismatch if the ABI check ran.
        // A matching commit hash must short-circuit before that.
        let stub = write_stub(&dir, env!("INFS_GIT_COMMIT"), "9.9", false);
        let result = probe_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "matching commit hash must short-circuit ABI check, got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn unknown_commit_and_unknown_abi_is_silent() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "unknown", "unknown", false);
        let result = probe_compiler_compatibility(&stub);
        assert!(result.is_ok(), "unknown outputs must be graceful");
    }

    #[test]
    fn old_infc_returns_nonzero_for_flags_is_graceful() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "anything", "anything", true);
        let result = probe_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "non-zero exit from flag probes must be graceful"
        );
    }

    #[test]
    fn probe_capability_commit_match_sets_commit_matched() {
        let dir = assert_fs::TempDir::new().unwrap();
        // ABI "9.9" would major-mismatch if probed; commit match must short-circuit.
        let stub = write_stub(&dir, env!("INFS_GIT_COMMIT"), "9.9", false);
        let compat = probe_compiler_compatibility(&stub).unwrap();
        assert!(
            compat.commit_matched,
            "matching commit must set commit_matched"
        );
        assert_eq!(
            compat.abi, None,
            "commit match short-circuits the ABI probe"
        );
        assert!(compat.supports_out_dir());
    }

    #[test]
    fn probe_capability_minor_1_supports_out_dir() {
        let dir = assert_fs::TempDir::new().unwrap();
        let abi = format!("{COMPILER_ABI_MAJOR}.1");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let compat = probe_compiler_compatibility(&stub).unwrap();
        assert!(!compat.commit_matched);
        assert_eq!(compat.abi, Some((COMPILER_ABI_MAJOR, 1)));
        assert!(
            compat.supports_out_dir(),
            "ABI minor 1 with matching major must support --out-dir"
        );
    }

    #[test]
    fn probe_capability_minor_0_rejects_out_dir() {
        let dir = assert_fs::TempDir::new().unwrap();
        let abi = format!("{COMPILER_ABI_MAJOR}.0");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let compat = probe_compiler_compatibility(&stub).unwrap();
        assert_eq!(compat.abi, Some((COMPILER_ABI_MAJOR, 0)));
        assert!(
            !compat.supports_out_dir(),
            "ABI minor 0 must not advertise --out-dir support"
        );
    }

    #[test]
    fn probe_capability_minor_2_supports_wasm_features() {
        let dir = assert_fs::TempDir::new().unwrap();
        let abi = format!("{COMPILER_ABI_MAJOR}.2");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let compat = probe_compiler_compatibility(&stub).unwrap();
        assert!(!compat.commit_matched);
        assert_eq!(compat.abi, Some((COMPILER_ABI_MAJOR, 2)));
        assert!(
            compat.supports_wasm_features(),
            "ABI minor 2 with matching major must support --wasm-features"
        );
    }

    #[test]
    fn probe_capability_minor_1_rejects_wasm_features() {
        // The pairing that motivates a per-flag predicate: minor 1 supports
        // `--out-dir` but predates `--wasm-features`.
        let dir = assert_fs::TempDir::new().unwrap();
        let abi = format!("{COMPILER_ABI_MAJOR}.1");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let compat = probe_compiler_compatibility(&stub).unwrap();
        assert!(compat.supports_out_dir());
        assert!(
            !compat.supports_wasm_features(),
            "ABI minor 1 must not advertise --wasm-features support"
        );
    }

    #[test]
    fn probe_capability_unknown_abi_rejects_out_dir() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "unknown", "unknown", false);
        let compat = probe_compiler_compatibility(&stub).unwrap();
        assert!(!compat.commit_matched);
        assert_eq!(compat.abi, None);
        assert!(
            !compat.supports_out_dir(),
            "an old/unknown infc must not be sent --out-dir"
        );
    }

    // The end-to-end out-dir capability gate (old infc + out_dir → hard error)
    // is covered by the `cli_integration` test using INFC_PATH in a subprocess,
    // which avoids mutating this process's environment (find_infc reads
    // INFC_PATH globally).

    #[test]
    fn parse_abi_version_accepts_valid() {
        assert_eq!(parse_abi_version("1.0"), Some((1, 0)));
        assert_eq!(parse_abi_version("2.7"), Some((2, 7)));
    }

    #[test]
    fn parse_abi_version_rejects_garbage() {
        assert_eq!(parse_abi_version(""), None);
        assert_eq!(parse_abi_version("1"), None);
        assert_eq!(parse_abi_version("1.x"), None);
        assert_eq!(parse_abi_version("x.1"), None);
        assert_eq!(parse_abi_version("1.0.0"), None);
    }
}

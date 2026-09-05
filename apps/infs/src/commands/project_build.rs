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
//! capability, not just the pass/fail — the additive flags `infs` gates
//! (`--out-dir`, `--wasm-features`) are each checked against it.
//!
//! ## Which settings are parameters and which are read off the context
//!
//! [`run_project_build`] takes `mode`, `out_dir`, and `wasm_lib_dirs` as
//! parameters but reads `[build] wasm-features` and `[wasm-dependencies]`
//! straight off `ctx`. The rule: a setting a CLI flag can override, or that a
//! caller must be able to suppress, is threaded so the caller stays the single
//! place that resolves it — `build` and `run` each own their `-L` entries and
//! pass them through, and `run` deliberately passes `mode = None` to force
//! compile mode. A setting only the manifest can express, with no flag and
//! nothing to suppress, is read from `ctx` — threading it would let two callers
//! disagree about a property of the project itself. An instruction-set request
//! is the latter, as is the set of external modules the project links against:
//! `build` and `run` emitting different instruction levels — or resolving one
//! project's `use { … } from <module>` against different `.wasm` files — is a
//! bug, not a configuration.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::commands::build::{BuildMode, format_wasm_dep_arg};
use crate::errors::InfsError;
use crate::project::ProjectContext;
use crate::project::manifest::{MANIFEST_FILE_NAME, MemoryConfig, VerificationConfig};
use crate::toolchain::resolver::{ResolutionSource, find_infc_with_source};
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
/// External-module resolution reaches `infc` as two flag families, so a project
/// whose sources bind `use { … } from <module>` can link: each `wasm_lib_dirs`
/// entry becomes `--wasm-lib-dir <dir>`, and each `[wasm-dependencies]` entry
/// becomes `--wasm-dep <name>=<path>` with the declared path resolved against
/// the project root. Neither is capability-gated, matching the single-file path:
/// both flags arrived with external-module support itself rather than at a
/// distinguishable ABI minor, so the handshake has nothing to check — an `infc`
/// old enough to lack them could not compile a `use { … } from <module>` binding
/// at all.
///
/// A lib dir is anchored to the *invocation* directory before it is forwarded,
/// precisely because this helper re-anchors the child process to the project
/// root: `-L ../libs` typed in `<root>/src` names `<root>/src/../libs`, and
/// forwarding it verbatim would have `infc` read it as `<root>/../libs` instead
/// — failing to link, or silently linking a same-named module that happens to
/// sit at the root-anchored path. Joining onto the invocation directory leaves
/// an absolute dir unchanged. Neither single-file path needs that treatment:
/// single-file `build` and `run` both let `infc` inherit the working directory,
/// so a relative dir already keeps its meaning and is forwarded verbatim.
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
/// - the manifest declares a `[memory]` table the resolved `infc` cannot honor
/// - the manifest asks to adopt external specifications on a proof-artifact
///   build and the resolved `infc` cannot honor the request
/// - a `[wasm-dependencies]` key is not a well-formed logical module name
/// - a resolved `[wasm-dependencies]` path is not valid UTF-8
/// - lib dirs were passed and the current working directory cannot be determined
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
/// - post-build optimization is active and fails (missing/invalid artifact,
///   `wasm-opt` resolution, or the optimization itself)
pub(crate) fn run_project_build(
    ctx: &ProjectContext,
    generate_v_output: bool,
    mode: Option<BuildMode>,
    out_dir: Option<&Path>,
    wasm_lib_dirs: &[PathBuf],
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

    let (infc_path, infc_source) = find_infc_with_source()?;
    let compat = probe_compiler_compatibility(&infc_path, infc_source)?;

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

    if !wasm_lib_dirs.is_empty() {
        let cwd =
            std::env::current_dir().context("Failed to determine the current working directory")?;
        for dir in wasm_lib_dirs {
            cmd.arg("--wasm-lib-dir").arg(cwd.join(dir));
        }
    }

    for (name, path) in ctx.manifest.resolved_wasm_dependencies(&ctx.root)? {
        cmd.arg("--wasm-dep")
            .arg(format_wasm_dep_arg(&name, &path)?);
    }

    let manifest_path = ctx.root.join(MANIFEST_FILE_NAME);
    let features = ctx.manifest.build.resolved_wasm_features()?;
    forward_wasm_features(&mut cmd, compat, &features, Some(&manifest_path))?;
    forward_memory_layout(&mut cmd, compat, &ctx.manifest.memory, Some(&manifest_path))?;
    forward_adopt_external_specs(
        &mut cmd,
        compat,
        &ctx.manifest.verification,
        generate_v_output,
        mode,
        Some(&manifest_path),
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

    /// Whether the resolved `infc` is known to support the additive
    /// `--memory-pages` / `--stack-size` flags, which landed together at ABI
    /// minor 3.
    ///
    /// One predicate covers both flags because they are one capability: a layout
    /// is the pair of numbers, they shipped in the same minor, and no request
    /// forwards one without the gate having cleared the other. Splitting them
    /// would name the same minor twice with nothing to distinguish the two.
    ///
    /// The conservative reading is the same as for `--wasm-features`: an `infc`
    /// that predates the flags cannot honor a layout request, and refusing to
    /// build beats emitting a module whose memory is not the one the manifest
    /// asked for.
    pub fn supports_memory_layout(self) -> bool {
        self.supports_abi_minor(3)
    }

    /// Whether the resolved `infc` is known to support the additive
    /// `--adopt-external-specs` flag, which landed at ABI minor 4.
    ///
    /// The conservative reading matters most here of the four. An `infc` that
    /// predates the flag cannot carry a library's obligations and cannot say it
    /// did not, and the difference is which theorems the `.v` states — a missing
    /// one looks exactly like a proof artifact that was never asked for the
    /// obligation. Refusing to build beats writing a proof artifact whose
    /// contents are not the ones the manifest asked for.
    pub fn supports_adopt_external_specs(self) -> bool {
        self.supports_abi_minor(4)
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

/// Appends `--memory-pages` / `--stack-size` to `cmd` for the keys the project
/// actually declared, after confirming the resolved `infc` can honor them, and
/// echoes the resolved layout to stdout.
///
/// Every path that spawns `infc` on behalf of a project routes through here, for
/// the same reason [`forward_wasm_features`] exists once: a project must get the
/// same memory whether it was built, run, or built from a bare source path.
///
/// Only the declared keys are forwarded. Sending the resolved layout instead
/// would be simpler and wrong: a project with no `[memory]` table would forward
/// the defaults, which turns every build into a layout request and refuses to
/// build against an `infc` that the project never needed anything from. What is
/// forwarded is therefore the request, not its resolution — and since `infc`
/// fills an omitted flag from the same default, the layout it resolves is the one
/// echoed here.
///
/// The declared-nothing check lives inside rather than at the call sites so no
/// caller can reach the ABI gate on behalf of a project that asked for nothing.
///
/// `manifest_path` names the file the remediation tells the user to edit, which
/// matters as soon as a walk was involved: single-file mode may have found a
/// manifest several directories up. `None` can only accompany an empty request —
/// a memory can only have been requested by some manifest — and the fallback
/// keeps the message well-formed regardless.
///
/// # Errors
///
/// Returns the layout diagnostic when the declared keys do not describe a usable
/// memory, or a remediation-bearing error when they do and the resolved `infc`
/// predates the flags. Neither flag is ever emitted blind.
pub(crate) fn forward_memory_layout(
    cmd: &mut Command,
    compat: CompilerCompat,
    memory: &MemoryConfig,
    manifest_path: Option<&Path>,
) -> Result<()> {
    if memory.is_default() {
        return Ok(());
    }
    let layout = memory.resolved_layout()?;
    if !compat.supports_memory_layout() {
        let manifest = manifest_path.map_or_else(
            || String::from(MANIFEST_FILE_NAME),
            |path| path.display().to_string(),
        );
        bail!(
            "the resolved infc does not support `--memory-pages` / `--stack-size` \
             (requires infc ABI ≥ 1.3); update the toolchain or remove the \
             `[memory]` table from {manifest}."
        );
    }
    println!(
        "memory: {} page(s), {}-byte stack",
        layout.pages(),
        layout.stack_size()
    );
    if let Some(pages) = memory.pages {
        cmd.arg("--memory-pages").arg(pages.to_string());
    }
    if let Some(stack_size) = memory.stack_size {
        cmd.arg("--stack-size").arg(stack_size.to_string());
    }
    Ok(())
}

/// Appends `--adopt-external-specs` to `cmd` when the project asked for it and
/// the command line being built asks `infc` for a proof artifact in a mode that
/// can carry one, after confirming the resolved `infc` can honor the request,
/// and echoes the decision to stdout.
///
/// The gate reads the two arguments `infs` itself forwards, which is the only
/// thing `infs` knows about the build: the flag goes on when `-v` or `--mode
/// proof` is on the command line, and never when `--mode compile` is. Those two
/// clauses are not redundant. `-v` alone leaves the mode to `infc`, which
/// resolves it to proof; an explicit `--mode compile -v` is a supported
/// spelling that writes a `.v` for the executable module, and `infc` refuses
/// `--adopt-external-specs` on it outright — so forwarding there would turn a
/// manifest key into a hard build failure for a command line the user is
/// entitled to run.
///
/// Withholding it is the right answer rather than a concession: a compile-mode
/// build strips the program's own specification functions and emits no
/// verification section, so there is nothing for a library's adopted
/// obligations to join. The request is echoed as *not applied* whenever a `.v`
/// is being written, because that is the build whose theorems the key asked to
/// change, and the link's own dropped-obligations warning on it ends by
/// prescribing the very key the project already set.
///
/// It deliberately keys on the wider signal than `[verification] output-dir`
/// does, and the difference is not an inconsistency. Forwarding `--out-dir`
/// under `-v`-alone would relocate `out/main.wasm` for every existing project —
/// a change to where artifacts land — which is why `resolve_out_dir` refuses to.
/// Forwarding this changes only which theorems the `.v` states, which is
/// precisely what the key asked for, and withholding it is invisible in the
/// output.
///
/// The mode test lives inside rather than at the call site so no caller can
/// forward a proof-only flag onto a compile-mode command line: a project that
/// sets the key must still be able to run a plain `infs build`, and every
/// spelling of a compile-mode one.
///
/// # Errors
///
/// Returns a remediation-bearing error when the project asked for adoption on a
/// proof-artifact build and the resolved `infc` predates the flag. The flag is
/// never emitted blind.
pub(crate) fn forward_adopt_external_specs(
    cmd: &mut Command,
    compat: CompilerCompat,
    verification: &VerificationConfig,
    generate_v_output: bool,
    mode: Option<BuildMode>,
    manifest_path: Option<&Path>,
) -> Result<()> {
    if !verification.adopt_external_specs {
        return Ok(());
    }
    if mode == Some(BuildMode::Compile) {
        if generate_v_output {
            println!("external-spec adoption: not applied to a compile-mode build");
        }
        return Ok(());
    }
    if !generate_v_output && mode != Some(BuildMode::Proof) {
        return Ok(());
    }
    if !compat.supports_adopt_external_specs() {
        let manifest = manifest_path.map_or_else(
            || String::from(MANIFEST_FILE_NAME),
            |path| path.display().to_string(),
        );
        bail!(
            "the resolved infc does not support `--adopt-external-specs` \
             (requires infc ABI ≥ 1.4); update the toolchain or remove \
             `adopt-external-specs` from the `[verification]` table in {manifest}."
        );
    }
    println!("external-spec adoption: on");
    cmd.arg("--adopt-external-specs");
    Ok(())
}

/// The pairing warning owed to a sibling-resolved `infc`, or `None` when
/// there is nothing to report.
///
/// Resolution tier 2 claims an adjacent `infc` is this `infs`'s pair on the
/// strength of adjacency alone. Nothing enforces that claim, so a stale
/// `infc` left behind in a build directory keeps winning silently. Comparing
/// the two build commits is what turns the tier's implicit claim into a
/// checked one.
///
/// Deliberately restricted to [`ResolutionSource::ExecutableSibling`]. For
/// `INFC_PATH`, `PATH`, and the managed toolchain a differing commit is the
/// normal state — a released `infc` is routinely built from a different
/// commit than the `infs` invoking it — so warning there would fire on every
/// build for every end user. Only adjacency asserts pairing, so only
/// adjacency can have that assertion falsified.
///
/// This detects *cross-commit* drift only. Two binaries built from the same
/// commit but different working-tree states report the same hash and are
/// indistinguishable here.
fn sibling_pairing_warning(
    source: ResolutionSource,
    local_commit: &str,
    remote_commit: Option<&str>,
) -> Option<String> {
    if source != ResolutionSource::ExecutableSibling {
        return None;
    }
    // Both build scripts stamp the literal "unknown" when git is unavailable,
    // so an infs built outside a checkout has no commit to compare against.
    // The infc side needs no such guard: `probe_flag` already maps "unknown"
    // and empty output to `None`.
    if local_commit == "unknown" {
        return None;
    }
    let remote = remote_commit?;
    if remote == local_commit {
        return None;
    }
    Some(format!(
        "warning: the infc beside infs is from a different build (infs \
         {local_commit}, infc {remote}); adjacent binaries are assumed to be \
         built together. Rebuild the workspace, or set INFC_PATH to pin the \
         compiler you want."
    ))
}

/// Runs the `infc` compatibility handshake and returns its capability.
///
/// Sequence:
/// 1. Query `infc --commit-hash`. If it equals `INFS_GIT_COMMIT`, short-circuit —
///    the two binaries were built from the same source tree and the ABI is
///    guaranteed compatible (`commit_matched = true`).
/// 2. Otherwise, when `source` is the sibling tier, report the build-pairing
///    mismatch ([`sibling_pairing_warning`]). An exact ABI match is silent and
///    the ABI rarely moves, so without this the common case of a stale
///    neighbour says nothing at all.
/// 3. Query `infc --abi-version` and compare against the major/minor constants
///    from `inference-compiler-interface`. Major mismatch is a hard error;
///    minor mismatch is a warning; exact match is silent.
///
/// Old binaries that do not understand the flags (non-zero exit, empty output,
/// or the literal `unknown`) are treated as graceful skips — we neither warn
/// nor error on them, and the returned `abi` is `None`. The resolver's
/// priority order remains the correctness guarantee; this handshake is a
/// safety net against residual drift, and it sees only *cross-commit* drift:
/// two binaries built from one commit with different working trees are
/// identical to it.
///
/// # Errors
///
/// Hard-errors only on a *major* ABI mismatch (with remediation).
pub(crate) fn probe_compiler_compatibility(
    infc_path: &Path,
    source: ResolutionSource,
) -> Result<CompilerCompat> {
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

    if let Some(warning) = sibling_pairing_warning(source, local_commit, remote_commit.as_deref()) {
        eprintln!("{warning}");
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

        let err = run_project_build(&ctx, false, None, None, &[], false).unwrap_err();
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

        let err = run_project_build(&ctx, false, None, None, &[], false).unwrap_err();
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

    // Memory layout forwarding ---

    #[test]
    fn supports_memory_layout_capability_matrix() {
        // Commit match alone is sufficient (ABI not even probed).
        assert!(
            CompilerCompat {
                commit_matched: true,
                abi: None,
            }
            .supports_memory_layout(),
            "a same-build infc supports every flag this infs knows"
        );

        // Same major, minor >= 3 → supported.
        for minor in [3, 9] {
            assert!(
                CompilerCompat {
                    commit_matched: false,
                    abi: Some((COMPILER_ABI_MAJOR, minor)),
                }
                .supports_memory_layout(),
                "minor {minor} must support the memory flags"
            );
        }

        // The flags landed at minor 3, so 0..=2 are unsupported. Minor 2 is the
        // interesting one: the capabilities must not be conflated.
        for minor in [0, 1, 2] {
            assert!(
                !CompilerCompat {
                    commit_matched: false,
                    abi: Some((COMPILER_ABI_MAJOR, minor)),
                }
                .supports_memory_layout(),
                "minor {minor} predates the memory flags"
            );
        }
        let minor_two = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 2)),
        };
        assert!(
            minor_two.supports_wasm_features() && minor_two.supports_out_dir(),
            "minor 2 still supports the older flags; only the newer pair is gated out"
        );

        // Different major → not supported even at a high minor.
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: Some((COMPILER_ABI_MAJOR + 1, 9)),
            }
            .supports_memory_layout(),
            "a foreign major is incompatible regardless of its minor"
        );

        // Unknown ABI → not supported.
        assert!(
            !CompilerCompat {
                commit_matched: false,
                abi: None,
            }
            .supports_memory_layout(),
            "an infc that cannot report its ABI must not be sent the flags"
        );
    }

    /// A project that declared no `[memory]` table forwards nothing, and so never
    /// reaches the ABI gate. This is what keeps every existing project buildable
    /// against an older `infc`: without it, adding the table to the schema would
    /// have turned every build into a layout request.
    #[test]
    fn forward_memory_layout_appends_nothing_when_nothing_was_declared() {
        let mut cmd = Command::new("infc");
        let predates_the_flags = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 0)),
        };
        forward_memory_layout(&mut cmd, predates_the_flags, &MemoryConfig::default(), None)
            .expect("an undeclared memory asks nothing of the compiler");
        assert!(
            args_of(&cmd).is_empty(),
            "an undeclared memory must not put a flag on the command"
        );
    }

    /// Only the declared keys are forwarded. Sending the resolved layout would
    /// send both flags always, which is the same mistake as forwarding defaults
    /// for a project that declared nothing — one step smaller.
    #[test]
    fn forward_memory_layout_appends_only_the_declared_keys() {
        let same_build = CompilerCompat {
            commit_matched: true,
            abi: None,
        };

        let mut cmd = Command::new("infc");
        forward_memory_layout(
            &mut cmd,
            same_build,
            &MemoryConfig {
                pages: Some(4),
                stack_size: None,
            },
            None,
        )
        .expect("a same-build infc supports the flags");
        assert_eq!(args_of(&cmd), ["--memory-pages", "4"]);

        let mut cmd = Command::new("infc");
        forward_memory_layout(
            &mut cmd,
            same_build,
            &MemoryConfig {
                pages: None,
                stack_size: Some(32_768),
            },
            None,
        )
        .expect("a same-build infc supports the flags");
        assert_eq!(args_of(&cmd), ["--stack-size", "32768"]);

        let mut cmd = Command::new("infc");
        forward_memory_layout(
            &mut cmd,
            same_build,
            &MemoryConfig {
                pages: Some(2),
                stack_size: Some(32_768),
            },
            None,
        )
        .expect("a same-build infc supports the flags");
        assert_eq!(
            args_of(&cmd),
            ["--memory-pages", "2", "--stack-size", "32768"]
        );
    }

    /// The gate refuses rather than forwards, and leaves the command untouched so
    /// a caller that mishandled the error could not still spawn with the flags.
    #[test]
    fn forward_memory_layout_refuses_an_infc_that_predates_the_flags() {
        let mut cmd = Command::new("infc");
        let minor_two = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 2)),
        };
        let manifest = Path::new("/projects/demo").join(MANIFEST_FILE_NAME);
        let err = forward_memory_layout(
            &mut cmd,
            minor_two,
            &MemoryConfig {
                pages: Some(2),
                stack_size: None,
            },
            Some(&manifest),
        )
        .expect_err("ABI minor 2 predates the memory flags");
        let msg = err.to_string();
        assert!(
            msg.contains("--memory-pages") && msg.contains("--stack-size") && msg.contains("1.3"),
            "the error must name both flags and the required ABI, got: {msg}"
        );
        assert!(
            msg.contains("update the toolchain") && msg.contains("[memory]"),
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

    /// A project that asked for adoption on a command line `infc` would not
    /// resolve to a proof build forwards nothing, and never reaches the ABI
    /// gate.
    ///
    /// This is what keeps `infs build` and `infs run` usable on a project that
    /// set the key: `infc` refuses the flag outright on a compile-mode command
    /// line, so forwarding it unconditionally would make the key break every
    /// non-proof build of the project that declared it.
    ///
    /// `--mode compile -v` is the cell that makes the two clauses of the gate
    /// separately load-bearing. It *is* a proof-artifact request — `infs` puts
    /// `-v` on the command line — and it is still not a proof build, so a gate
    /// that read only `generate_v_output` would hand `infc` a flag it rejects
    /// and turn a manifest key into a failed build.
    #[test]
    fn forward_adopt_external_specs_appends_nothing_without_a_proof_request() {
        let asked = VerificationConfig {
            adopt_external_specs: true,
            ..VerificationConfig::default()
        };

        // An infc that supports the flag: what withholds it is the mode this
        // command line resolves to, not the toolchain.
        let supports = CompilerCompat {
            commit_matched: true,
            abi: None,
        };
        for (generate_v_output, mode) in [
            (false, None),
            (false, Some(BuildMode::Compile)),
            (true, Some(BuildMode::Compile)),
        ] {
            let mut cmd = Command::new("infc");
            forward_adopt_external_specs(&mut cmd, supports, &asked, generate_v_output, mode, None)
                .expect("a build that resolves to compile mode asks nothing of the compiler");
            assert!(
                args_of(&cmd).is_empty(),
                "a compile-mode command line must not carry a proof-only flag                  (generate_v_output = {generate_v_output}, mode = {mode:?})"
            );
        }

        // And the gate is never reached, so an infc that predates the flag is
        // not consulted and cannot refuse the build.
        let predates_the_flag = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 3)),
        };
        for (generate_v_output, mode) in [
            (false, None),
            (false, Some(BuildMode::Compile)),
            (true, Some(BuildMode::Compile)),
        ] {
            let mut cmd = Command::new("infc");
            forward_adopt_external_specs(
                &mut cmd,
                predates_the_flag,
                &asked,
                generate_v_output,
                mode,
                None,
            )
            .expect("an unforwarded request asks nothing of the compiler");
            assert!(args_of(&cmd).is_empty());
        }
    }

    /// A project that did not ask forwards nothing even on a proof build, and
    /// so never reaches the ABI gate either.
    #[test]
    fn forward_adopt_external_specs_appends_nothing_when_the_project_did_not_ask() {
        let mut cmd = Command::new("infc");
        let supports = CompilerCompat {
            commit_matched: true,
            abi: None,
        };
        forward_adopt_external_specs(
            &mut cmd,
            supports,
            &VerificationConfig::default(),
            true,
            Some(BuildMode::Proof),
            None,
        )
        .expect("a project that asked for nothing asks nothing of the compiler");
        assert!(
            args_of(&cmd).is_empty(),
            "adoption is opt-in; the default must put no flag on the command"
        );

        // And the ABI gate is never reached, so a project that asked for
        // nothing still builds against an infc that predates the flag.
        let mut cmd = Command::new("infc");
        let predates_the_flag = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 3)),
        };
        forward_adopt_external_specs(
            &mut cmd,
            predates_the_flag,
            &VerificationConfig::default(),
            true,
            Some(BuildMode::Proof),
            None,
        )
        .expect("an unforwarded request must not consult the toolchain");
        assert!(args_of(&cmd).is_empty());
    }

    /// Both spellings of a proof-artifact request forward the flag, and so does
    /// a same-build `infc` whose ABI was never probed.
    ///
    /// `-v` alone counts deliberately: such a build writes a `.v` whose theorems
    /// are exactly what the key asked to change, and withholding the flag there
    /// would leave the manifest silently unhonored.
    #[test]
    fn forward_adopt_external_specs_appends_on_every_proof_artifact_request() {
        let asked = VerificationConfig {
            adopt_external_specs: true,
            ..VerificationConfig::default()
        };
        let supports = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 4)),
        };

        let mut cmd = Command::new("infc");
        forward_adopt_external_specs(&mut cmd, supports, &asked, true, None, None)
            .expect("ABI minor 4 supports the flag");
        assert_eq!(args_of(&cmd), ["--adopt-external-specs"]);

        let mut cmd = Command::new("infc");
        forward_adopt_external_specs(
            &mut cmd,
            supports,
            &asked,
            false,
            Some(BuildMode::Proof),
            None,
        )
        .expect("ABI minor 4 supports the flag");
        assert_eq!(args_of(&cmd), ["--adopt-external-specs"]);

        let same_build = CompilerCompat {
            commit_matched: true,
            abi: None,
        };
        let mut cmd = Command::new("infc");
        forward_adopt_external_specs(&mut cmd, same_build, &asked, true, None, None)
            .expect("a same-build infc supports the flag");
        assert_eq!(args_of(&cmd), ["--adopt-external-specs"]);
    }

    /// The gate refuses rather than forwards, and leaves the command untouched so
    /// a caller that mishandled the error could not still spawn with the flag.
    ///
    /// Refusing is the whole point: an `infc` that predates the flag would write
    /// a `.v` missing the theorems the manifest asked for, and nothing in the
    /// artifact would say a request had been dropped.
    #[test]
    fn forward_adopt_external_specs_refuses_an_infc_that_predates_the_flag() {
        let mut cmd = Command::new("infc");
        let minor_three = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 3)),
        };
        let manifest = Path::new("/projects/demo").join(MANIFEST_FILE_NAME);
        let err = forward_adopt_external_specs(
            &mut cmd,
            minor_three,
            &VerificationConfig {
                adopt_external_specs: true,
                ..VerificationConfig::default()
            },
            true,
            None,
            Some(&manifest),
        )
        .expect_err("ABI minor 3 predates --adopt-external-specs");
        let msg = err.to_string();
        assert!(
            msg.contains("--adopt-external-specs") && msg.contains("1.4"),
            "the error must name the flag and the required ABI, got: {msg}"
        );
        assert!(
            msg.contains("update the toolchain") && msg.contains("adopt-external-specs")
                && msg.contains("[verification]"),
            "the error must offer both remediations, got: {msg}"
        );
        assert!(
            msg.contains(&manifest.display().to_string()),
            "the error must name which manifest to edit, got: {msg}"
        );
        assert!(
            args_of(&cmd).is_empty(),
            "a refused request must leave no flag on the command"
        );
    }

    /// An unusable memory is refused before the ABI gate, so a user with an old
    /// toolchain and a bad value is told about the value — which they must fix
    /// either way — rather than being sent to upgrade first.
    #[test]
    fn forward_memory_layout_reports_a_bad_value_ahead_of_the_abi_gate() {
        let mut cmd = Command::new("infc");
        let minor_two = CompilerCompat {
            commit_matched: false,
            abi: Some((COMPILER_ABI_MAJOR, 2)),
        };
        let err = forward_memory_layout(
            &mut cmd,
            minor_two,
            &MemoryConfig {
                pages: Some(0),
                stack_size: None,
            },
            None,
        )
        .expect_err("a zero-page memory is unusable");
        let msg = err.to_string();
        assert!(
            msg.contains("at least one 64 KiB page"),
            "the value must be diagnosed, not the toolchain, got: {msg}"
        );
        assert!(args_of(&cmd).is_empty());
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

    const LOCAL: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const REMOTE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn sibling_from_a_different_build_warns_and_names_both_hashes() {
        let warning =
            sibling_pairing_warning(ResolutionSource::ExecutableSibling, LOCAL, Some(REMOTE))
                .expect("a differing sibling commit must warn");
        assert!(
            warning.contains(LOCAL) && warning.contains(REMOTE),
            "the warning must name both builds so the drift is diagnosable: {warning}"
        );
        assert!(
            warning.contains("INFC_PATH"),
            "the warning must name the pinning escape hatch: {warning}"
        );
    }

    #[test]
    fn sibling_from_the_same_build_is_silent() {
        assert_eq!(
            sibling_pairing_warning(ResolutionSource::ExecutableSibling, LOCAL, Some(LOCAL)),
            None
        );
    }

    #[test]
    fn sibling_that_reports_no_commit_is_silent() {
        // `probe_flag` maps an old infc's empty output and its literal
        // "unknown" to `None`, so this is the shape those reach us in.
        assert_eq!(
            sibling_pairing_warning(ResolutionSource::ExecutableSibling, LOCAL, None),
            None
        );
    }

    #[test]
    fn sibling_is_silent_when_infs_itself_has_no_commit() {
        // An `infs` built outside a git checkout is stamped "unknown" and has
        // nothing to compare, so every sibling would otherwise look stale.
        assert_eq!(
            sibling_pairing_warning(ResolutionSource::ExecutableSibling, "unknown", Some(REMOTE)),
            None
        );
    }

    #[test]
    fn non_sibling_tiers_never_warn_about_a_differing_commit() {
        // The anti-noise guarantee. A released `infc` is routinely built from
        // a different commit than the `infs` invoking it, so warning on these
        // tiers would fire on every build for every end user. Covers all
        // three: only adjacency claims the two binaries are a pair.
        for source in [
            ResolutionSource::InfcPathEnv,
            ResolutionSource::SystemPath,
            ResolutionSource::ManagedToolchain,
        ] {
            assert_eq!(
                sibling_pairing_warning(source, LOCAL, Some(REMOTE)),
                None,
                "{} must not warn about a differing commit",
                source.label()
            );
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// The resolution tier the ABI-handshake tests report.
    ///
    /// Any tier but the sibling one keeps [`sibling_pairing_warning`] silent,
    /// so these tests observe the ABI handshake alone. The sibling tier's own
    /// behaviour is covered separately, against the pure function.
    const ABI_PROBE_SOURCE: ResolutionSource = ResolutionSource::InfcPathEnv;

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
        let err = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap_err();
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
        let result = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE);
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
        let result = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE);
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
        let result = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE);
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
        let result = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE);
        assert!(
            result.is_ok(),
            "matching commit hash must short-circuit ABI check, got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn sibling_tier_keeps_the_commit_match_short_circuit() {
        // The pairing warning is inserted on the commit-mismatch path only.
        // A sibling built from this very commit must still short-circuit
        // before the ABI probe, so the "9.9" major mismatch stays unreached.
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, env!("INFS_GIT_COMMIT"), "9.9", false);
        let compat =
            probe_compiler_compatibility(&stub, ResolutionSource::ExecutableSibling).unwrap();
        assert!(compat.commit_matched);
        assert_eq!(compat.abi, None);
    }

    #[test]
    fn sibling_tier_with_differing_commit_still_reports_abi() {
        // The warning is advisory: it must not disturb the returned
        // capability, which the flag gates depend on.
        let dir = assert_fs::TempDir::new().unwrap();
        let abi = format!("{COMPILER_ABI_MAJOR}.{COMPILER_ABI_MINOR}");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let compat =
            probe_compiler_compatibility(&stub, ResolutionSource::ExecutableSibling).unwrap();
        assert!(!compat.commit_matched);
        assert_eq!(compat.abi, Some((COMPILER_ABI_MAJOR, COMPILER_ABI_MINOR)));
    }

    #[test]
    fn unknown_commit_and_unknown_abi_is_silent() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "unknown", "unknown", false);
        let result = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE);
        assert!(result.is_ok(), "unknown outputs must be graceful");
    }

    #[test]
    fn old_infc_returns_nonzero_for_flags_is_graceful() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "anything", "anything", true);
        let result = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE);
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
        let compat = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap();
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
        let compat = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap();
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
        let compat = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap();
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
        let compat = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap();
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
        let compat = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap();
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
        let compat = probe_compiler_compatibility(&stub, ABI_PROBE_SOURCE).unwrap();
        assert!(!compat.commit_matched);
        assert_eq!(compat.abi, None);
        assert!(
            !compat.supports_out_dir(),
            "an old/unknown infc must not be sent --out-dir"
        );
    }

    // The end-to-end out-dir capability gate (old infc + out_dir → hard error)
    // is covered by the `cli_integration` test using INFC_PATH in a subprocess,
    // which avoids mutating this process's environment (the resolver reads
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

#![warn(clippy::pedantic)]

//! Integration tests for the infs unified CLI toolchain.
//!
//! These tests exercise the `infs` binary in a realistic environment by spawning
//! the compiled executable and validating its behavior through stdout, stderr,
//! and exit codes.
//!
//! ## Test Strategy
//!
//! The test suite verifies:
//!
//! ### Phase 1: Build Command
//!
//! 1. **Error handling**: File existence, no panics on error paths
//! 2. **Build command**: Full compilation, `-v` flag for Rocq output
//! 3. **Output generation**: WASM and Rocq file creation
//! 4. **Version and help**: CLI metadata display
//! 5. **Headless mode**: Display info without TUI
//! 6. **Compatibility**: Byte-identical output compared to `infc`
//!
//! ### Phase 2: Toolchain Management
//!
//! 7. **Install command**: Help display, error handling without network
//! 8. **Uninstall command**: Help display, nonexistent version handling
//! 9. **List command**: Success on empty state, appropriate messaging
//! 10. **Default command**: Help display, argument validation, error handling
//! 11. **Doctor command**: Health checks execution, output verification
//! 12. **Self update command**: Help display, subcommand validation, error handling
//!
//! ### Phase 3: Project Scaffolding
//!
//! 13. **New command**: Project creation, validation, directory structure
//! 14. **Init command**: In-place initialization, manifest generation
//!
//! ### Phase 4-5: Verify Command
//!
//! 15. **Verify command**: Help display, path validation, coqc availability check
//!
//! ### Phase 6: Run Command
//!
//! 16. **Run command**: Help display, path validation, wasmtime availability check
//!
//! ## Test Infrastructure
//!
//! - Uses `assert_cmd` for spawning and asserting on command execution
//! - Uses `assert_fs` for temporary filesystem operations
//! - Uses `predicates` for flexible output matching
//! - Test data located in `tests/test_data/` at workspace root
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test -p infs
//! ```
//!
//! Tests run in parallel and use temporary directories to avoid interference.

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::process::Command;

/// Resolves the path to a test fixture file in the `tests/fixtures/` directory.
///
/// ## Path Resolution
///
/// ```text
/// env!("CARGO_MANIFEST_DIR")  // apps/infs/
///   .join("tests")
///   .join("fixtures")
///   .join(name)
/// ```
fn fixture_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Resolves the path to a test data file (alias for `fixture_file`).
fn example_file(name: &str) -> std::path::PathBuf {
    fixture_file(name)
}

/// Resolves the path to a codegen test data file (alias for `fixture_file`).
///
/// These files are simpler examples that successfully compile through all phases.
fn codegen_test_file(name: &str) -> std::path::PathBuf {
    fixture_file(name)
}

/// Returns a PATH that excludes wasmtime and coqc but preserves essential
/// system directories and runtime DLLs needed for the process to run.
///
/// On Windows, setting PATH="" prevents the process from finding essential DLLs
/// (like MinGW runtime when compiled with GNU toolchain), causing `STATUS_DLL_NOT_FOUND`.
/// This function uses `which` to find the exact directories containing the tools
/// and excludes only those, preserving all other paths.
///
/// On non-Windows platforms, we can safely use an empty PATH since there are no
/// DLL loading issues.
fn path_without_tools() -> String {
    // On non-Windows, empty PATH is safe and ensures tools aren't found
    #[cfg(not(windows))]
    {
        String::new()
    }

    // On Windows, we must preserve system directories and MinGW runtime DLLs
    #[cfg(windows)]
    {
        use std::path::PathBuf;

        let current_path = std::env::var("PATH").unwrap_or_default();

        // Find directories containing the tools we want to exclude
        let tool_dirs: Vec<PathBuf> = ["wasmtime", "coqc"]
            .iter()
            .filter_map(|tool| {
                which::which(tool)
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            })
            .collect();

        current_path
            .split(';')
            .filter(|dir| {
                let dir_path = std::path::Path::new(dir);
                // Keep directories that don't contain any of the tools
                !tool_dirs.iter().any(|tool_dir| dir_path == tool_dir)
            })
            .collect::<Vec<_>>()
            .join(";")
    }
}

// Error Path Tests

/// Verifies that the build command fails gracefully when the input file doesn't exist.
///
/// **Expected behavior**: Exit with non-zero code and print "Path not found" to stderr.
#[test]
fn build_fails_when_file_missing() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("build").arg("this-file-does-not-exist.inf");

    cmd.assert().failure().stderr(
        predicate::str::contains("Path not found").or(predicate::str::contains("path not found")),
    );
}

/// Verifies that `infs build` with no phase flags defaults to full compilation.
///
/// **Expected behavior**: Exit with code 0 and produce a `.wasm` file in `out/`.
/// This is equivalent to `infs build file.inf --codegen -o`.
#[test]
fn build_no_flags_produces_wasm() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let wasm_output = temp.child("out").child("trivial.wasm");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
}

/// Verifies that `infs build -v` (no explicit phase flags) defaults to full compilation
/// and produces both `.wasm` and `.v` output files.
///
/// **Expected behavior**: Exit with code 0, produce `out/trivial.wasm` and `out/trivial.v`.
#[test]
fn build_v_flag_alone_produces_both_outputs() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    let wasm_output = temp.child("out").child("trivial.wasm");
    let v_output = temp.child("out").child("trivial.v");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
    assert!(
        v_output.path().exists(),
        "Expected V file at: {:?}",
        v_output.path()
    );
}

// Success Path Tests

/// Verifies that the full pipeline with Rocq output works correctly.
///
/// **Expected behavior**: The compilation succeeds and produces both .wasm and .v files.
#[test]
fn build_full_pipeline_with_v_output() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated at:"));

    let wasm_output = temp.child("out").child("trivial.wasm");
    let v_output = temp.child("out").child("trivial.v");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
    assert!(
        v_output.path().exists(),
        "Expected V file at: {:?}",
        v_output.path()
    );
}

// Version and Help Tests

/// Verifies that the `--version` flag displays the correct version information.
///
/// **Expected behavior**: Exit with code 0 and print the version string to stdout.
#[test]
fn version_flag_shows_version() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

/// Verifies that the `--help` flag displays usage information.
///
/// **Expected behavior**: Exit with code 0 and print help text including available commands.
#[test]
fn help_shows_available_commands() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("--headless"));
}

/// `infs build --help` documents the external-lib-dir flag, including the
/// relative-directory semantics: a relative `-L` always means what it meant at
/// the shell, in project mode as much as in single-file mode. That sentence is
/// the user-visible contract behind the invocation-directory anchoring in
/// `run_project_build`, so it is pinned here — rendering the help is also the
/// only invocation that materializes the flag's help text at all.
///
/// clap re-wraps help text to the terminal width, so the assertions run against
/// a whitespace-normalized copy of the output rather than the raw bytes.
#[test]
fn build_help_documents_the_external_lib_dir_flag() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("build").arg("--help");

    let assert = cmd.assert().success();
    let normalized = stdout_of(&assert)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for expected in [
        "--wasm-lib-dir",
        "-L",
        "Directory to search for external `.wasm` modules",
        "A relative dir always means what it meant at the shell",
    ] {
        assert!(
            normalized.contains(expected),
            "`infs build --help` must document {expected:?}; normalized output was:\n{normalized}"
        );
    }
}

// Headless Mode Tests

/// Verifies that headless mode without a command shows informational output.
///
/// **Expected behavior**: Exit with code 0 and display guidance about available commands.
#[test]
fn headless_mode_without_command_shows_info() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("--headless");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("infs"))
        .stdout(predicate::str::contains("--help").or(predicate::str::contains("build")));
}

/// Verifies that the TUI is skipped when `INFS_NO_TUI` environment variable is set.
///
/// **Test setup**: Sets `INFS_NO_TUI=1` environment variable and runs infs without subcommand.
/// This is the dedicated way to disable the interactive TUI in non-interactive environments.
///
/// **Expected behavior**: Exit with code 0 and display informational output (same as headless mode).
/// The TUI should NOT be launched because the headless detection recognizes the `INFS_NO_TUI` setting.
#[test]
fn tui_detects_infs_no_tui_environment() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_NO_TUI", "1");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("infs"))
        .stdout(predicate::str::contains("--help").or(predicate::str::contains("build")));
}

// infc Resolution Tests

/// Environment variable that pins the `infc` binary these tests spawn,
/// overriding the probe.
///
/// Deliberately distinct from `INFC_PATH`, which the gated tests set on the
/// *child* `infs` process. Honouring an ambient `INFC_PATH` here would let a
/// developer's installed compiler stand in for the one built from this working
/// tree, and the byte-identity comparison would then be against the wrong
/// binary.
///
/// An empty value counts as unset and falls through to the probe. A workflow
/// renders a falsy `${{ }}` expression as an empty-but-set variable, so a
/// conditional pin that does not fire must not abort the suite.
const INFC_OVERRIDE_ENV: &str = "INFERENCE_TEST_INFC";

/// Derives the cargo profile directory that holds uplifted package binaries
/// from the path of the running test executable.
///
/// Cargo builds integration tests into `<profile-dir>/deps/` and uplifts each
/// package's binaries into `<profile-dir>` itself, so dropping the file name
/// and a trailing `deps` component names the directory a sibling package's
/// binary was written to. Deriving it from the running executable encodes no
/// layout knowledge of its own, so it holds equally for `target/<profile>`, for
/// `target/<triple>/<profile>` under `--target`, for a redirected
/// `CARGO_TARGET_DIR`, and for the private target directory a coverage run
/// builds into.
fn cargo_profile_dir(test_exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut profile_dir = test_exe.parent()?.to_path_buf();
    if profile_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        profile_dir.pop();
    }
    Some(profile_dir)
}

/// The `infc` location probed for a test executable at `test_exe`, when
/// [`INFC_OVERRIDE_ENV`] is unset.
///
/// Resolution is derived from the running test binary instead of from a fixed
/// workspace path because only the running binary carries this run's profile
/// and target triple. A fixed `target/debug` path either names the same
/// directory, and is then dead weight, or names a different one, and then
/// holds a binary from another profile or target that a `--release` or
/// `--target` run would silently compare against instead of skipping.
///
/// `None` when `test_exe` has no directory component, so a degenerate path
/// cannot turn into a bare `infc` probed against the working directory.
fn infc_candidate_for(test_exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let profile_dir = cargo_profile_dir(test_exe)?;
    Some(profile_dir.join(format!("infc{}", std::env::consts::EXE_SUFFIX)))
}

/// [`infc_candidate_for`] applied to the running test executable.
///
/// The environment read lives here rather than in the derivation so that the
/// derivation stays a pure function of a path, exercisable against the layouts
/// this host cannot produce.
fn infc_candidate() -> Option<std::path::PathBuf> {
    let test_exe = std::env::current_exe().ok()?;
    infc_candidate_for(&test_exe)
}

/// Resolves the `infc` binary the end-to-end tests spawn, or `None` when the
/// probed location holds no file.
///
/// # Panics
///
/// When [`INFC_OVERRIDE_ENV`] names something other than an existing file. A
/// set-but-wrong override is a configuration mistake; probing past it would
/// bury that mistake under a skipped test, and accepting a directory would
/// defer it to an opaque spawn failure in every gated test.
fn infc_binary() -> Option<std::path::PathBuf> {
    if let Some(pinned) = std::env::var_os(INFC_OVERRIDE_ENV).filter(|value| !value.is_empty()) {
        let pinned = std::path::PathBuf::from(pinned);
        assert!(
            pinned.is_file(),
            "{INFC_OVERRIDE_ENV} is set to `{}`, but no file exists there",
            pinned.display()
        );
        return Some(pinned);
    }

    infc_candidate().filter(|candidate| candidate.is_file())
}

/// Renders the probed `infc` locations as an indented one-per-line list for the
/// diagnostics that both the CI abort and the local skip notice print.
fn probed_infc_paths() -> String {
    match infc_candidate() {
        Some(candidate) => format!("  {}", candidate.display()),
        None => "  (none: the running test executable has no directory)".to_owned(),
    }
}

/// True when `CI` carries a value that claims an automated run.
fn is_ci() -> bool {
    std::env::var_os("CI").is_some_and(|value| ci_value_is_meaningful(&value))
}

/// The value predicate behind [`is_ci`]: a shell that exports `CI=` or `CI=0`
/// has not opted into automated-run behavior.
///
/// A test that writes an environment variable races every concurrent read of
/// the environment in the process, so no test in this file writes `CI`; the
/// predicate is exercised directly instead.
fn ci_value_is_meaningful(value: &std::ffi::OsStr) -> bool {
    !value.is_empty() && value != "0"
}

/// Resolves `infc` for a test that spawns the real compiler, or reports it as
/// unavailable.
///
/// Locally a missing `infc` skips the caller, because `cargo test -p infs` on
/// its own never builds it. Under CI the run aborts instead: every workflow leg
/// builds the compiler with this run's profile and target before invoking the
/// suite, so a missing binary there means resolution is broken — and cargo
/// captures the skip notice, so the skip would otherwise be indistinguishable
/// from a pass.
///
/// # Panics
///
/// When no `infc` resolves during a CI run.
fn require_infc() -> Option<std::path::PathBuf> {
    if let Some(infc_path) = infc_binary() {
        return Some(infc_path);
    }

    let probed = probed_infc_paths();
    assert!(
        !is_ci(),
        "infc binary not found; a CI run must not silently skip the tests that spawn \
         the real compiler.\nProbed:\n{probed}\nBuild it with `cargo build -p inference-cli` \
         using this run's profile and target, or point {INFC_OVERRIDE_ENV} at an existing \
         binary (unset CI or set CI=0 to restore the local skip)."
    );
    eprintln!(
        "Skipping test: infc binary not found.\nProbed:\n{probed}\n\
         Build with `cargo build -p inference-cli` first."
    );
    None
}

/// Builds a path from its segments so the expectations below carry no
/// platform-specific separator.
fn joined(segments: &[&str]) -> std::path::PathBuf {
    segments.iter().collect()
}

/// The profile directory is the test executable's own directory with cargo's
/// `deps` component dropped, which is where cargo uplifts package binaries.
#[test]
fn profile_dir_drops_a_deps_component() {
    assert_eq!(
        cargo_profile_dir(&joined(&[
            "target",
            "debug",
            "deps",
            "cli_integration-1a2b3c"
        ])),
        Some(joined(&["target", "debug"]))
    );
}

/// Only a component named exactly `deps` is dropped; any other parent is the
/// profile directory itself.
#[test]
fn profile_dir_keeps_a_parent_that_is_not_named_deps() {
    assert_eq!(
        cargo_profile_dir(&joined(&["target", "debug", "dependencies", "harness"])),
        Some(joined(&["target", "debug", "dependencies"]))
    );
}

/// A cross-compiled run nests the profile under the target triple; the derived
/// directory must keep that segment instead of assuming `target/debug`.
#[test]
fn profile_dir_resolves_the_target_triple_layout() {
    assert_eq!(
        cargo_profile_dir(&joined(&[
            "target",
            "x86_64-pc-windows-gnu",
            "debug",
            "deps",
            "cli_integration-1a2b3c.exe"
        ])),
        Some(joined(&["target", "x86_64-pc-windows-gnu", "debug"]))
    );
}

/// The probe follows the test binary into its own profile directory, keeping
/// the target triple and the release profile of a cross-compiled run. Driving
/// it with the exact layout of the Windows leg fails the moment anyone
/// reintroduces a fixed `target/debug` candidate, which is what left that leg
/// comparing against a stale host compiler.
#[test]
fn infc_candidate_follows_the_test_binary_into_its_profile_directory() {
    let test_exe = joined(&[
        "target",
        "x86_64-pc-windows-gnu",
        "release",
        "deps",
        "cli_integration-1a2b3c.exe",
    ]);
    let expected = joined(&["target", "x86_64-pc-windows-gnu", "release"])
        .join(format!("infc{}", std::env::consts::EXE_SUFFIX));

    assert_eq!(infc_candidate_for(&test_exe), Some(expected));
}

/// A test executable path with no directory component yields no candidate, so
/// the unresolvable case reaches the skip-or-abort gate instead of probing a
/// bare `infc` against the working directory.
#[test]
fn infc_candidate_is_absent_without_a_directory_component() {
    assert_eq!(infc_candidate_for(&joined(&[])), None);
}

/// CI is claimed only by a meaningful `CI` value, so a shell that exports
/// `CI=0` or `CI=` keeps the local skip behavior. An unset `CI` is absent by
/// construction, since [`is_ci`] reaches the predicate only through
/// `Option::is_some_and`.
#[test]
fn ci_predicate_accepts_only_a_meaningful_value() {
    assert!(ci_value_is_meaningful(std::ffi::OsStr::new("true")));
    assert!(ci_value_is_meaningful(std::ffi::OsStr::new("1")));
    assert!(!ci_value_is_meaningful(std::ffi::OsStr::new("0")));
    assert!(!ci_value_is_meaningful(std::ffi::OsStr::new("")));
}

// Byte-Identical Output Tests

/// Verifies that `infs build` produces byte-identical WASM output as `infc`.
///
/// This test ensures backward compatibility and correctness by comparing
/// the output from both CLI tools when compiling the same source file.
#[test]
fn build_produces_identical_wasm_as_infc() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp_new = assert_fs::TempDir::new().unwrap();
    let temp_legacy = assert_fs::TempDir::new().unwrap();

    let src = codegen_test_file("trivial.inf");

    let dest_new = temp_new.child("trivial.inf");
    std::fs::copy(&src, dest_new.path()).unwrap();

    let dest_legacy = temp_legacy.child("trivial.inf");
    std::fs::copy(&src, dest_legacy.path()).unwrap();

    let mut cmd_new = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_new
        .env("INFC_PATH", &infc_path)
        .current_dir(temp_new.path())
        .arg("build")
        .arg(dest_new.path());

    cmd_new.assert().success();

    let mut cmd_legacy = Command::new(&infc_path);
    cmd_legacy
        .current_dir(temp_legacy.path())
        .arg(dest_legacy.path());

    cmd_legacy.assert().success();

    let wasm_new = temp_new.child("out").child("trivial.wasm");
    let wasm_legacy = temp_legacy.child("out").child("trivial.wasm");

    assert!(wasm_new.path().exists(), "infs did not produce WASM output");
    assert!(
        wasm_legacy.path().exists(),
        "infc did not produce WASM output"
    );

    let new_bytes = std::fs::read(wasm_new.path()).expect("Failed to read infs WASM");
    let legacy_bytes = std::fs::read(wasm_legacy.path()).expect("Failed to read infc WASM");

    assert_eq!(
        new_bytes, legacy_bytes,
        "WASM output from infs and infc should be byte-identical"
    );
}

// Project-mode Build Tests

/// Source used as `src/main.inf` for project-mode tests. Must define a `main`
/// entry point so it compiles cleanly and (for the run tests later) is
/// invokable.
const PROJECT_MAIN_SRC: &str = "pub fn main() -> i32 {\n    return 0;\n}\n";

/// Scaffolds a minimal project under `dir`: an `Inference.toml` manifest and a
/// `src/main.inf` with the given source. Returns nothing; `dir` is mutated in
/// place. Paths are built with `join` so they are platform-correct.
fn scaffold_project(dir: &assert_fs::TempDir, name: &str, main_src: &str) {
    let manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n");
    dir.child("Inference.toml").write_str(&manifest).unwrap();
    dir.child("src")
        .child("main.inf")
        .write_str(main_src)
        .unwrap();
}

/// Project mode: `infs build` (no path) invoked from the project root discovers
/// the manifest and writes `<root>/out/main.wasm`.
#[test]
fn project_build_from_root_produces_wasm() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().success();

    let wasm = temp.child("out").child("main.wasm");
    assert!(
        wasm.path().exists(),
        "expected project build to produce {:?}",
        wasm.path()
    );
}

/// Project mode: `infs build` invoked from a nested subdirectory still walks up
/// to the manifest and lands `out/` at the project root, not the subdir.
#[test]
fn project_build_from_subdir_lands_out_at_root() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    // Invoke from <root>/src (a directory that exists below the manifest).
    let subdir = temp.child("src");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(subdir.path())
        .arg("build");

    cmd.assert().success();

    // out/ must be at the root, regardless of the invocation CWD.
    let wasm_at_root = temp.child("out").child("main.wasm");
    assert!(
        wasm_at_root.path().exists(),
        "out/main.wasm should land at the project root: {:?}",
        wasm_at_root.path()
    );
    // And NOT under the subdirectory we invoked from.
    let wasm_in_subdir = subdir.child("out").child("main.wasm");
    assert!(
        !wasm_in_subdir.path().exists(),
        "out/ must not land in the invocation subdir: {:?}",
        wasm_in_subdir.path()
    );
}

/// Project mode with no `Inference.toml` anywhere up the tree must fail with a
/// clear, remediation-style message naming the manifest file.
#[test]
fn project_build_without_manifest_errors() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Inference.toml"));
}

/// Project mode whose manifest exists but `src/main.inf` is missing must fail
/// with a remediation message naming the expected entry point.
#[test]
fn project_build_missing_entry_point_errors() {
    let temp = assert_fs::TempDir::new().unwrap();
    temp.child("Inference.toml")
        .write_str("[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n")
        .unwrap();
    // No src/main.inf created.

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("entry point").and(predicate::str::contains("main.inf")));
}

// Project-mode Multi-file Build Tests (#63)

/// The stale `infs`-side warning that predated multi-file support: it claimed
/// project mode compiled only `src/main.inf`. `infc` now compiles the whole
/// import-reachable closure, so this text must never appear again. Every
/// multi-file test asserts its absence.
const STALE_PENDING_WARNING_FRAGMENT: &str = "multi-file support is pending";

/// Writes an extra source file at `relative` (a slash-separated path under
/// `src/`, e.g. `"lib/util.inf"`), creating intermediate directories. The path
/// is split on `/` and rejoined with `Path::join` so the on-disk layout is
/// platform-correct.
fn write_src_file(dir: &assert_fs::TempDir, relative: &str, src: &str) {
    let mut path = dir.child("src").path().to_path_buf();
    for segment in relative.split('/') {
        path = path.join(segment);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, src).unwrap();
}

/// A valid multi-file project (`src/main.inf` importing `src/lib/util.inf`)
/// builds successfully: exit 0 and a `<root>/out/main.wasm` artifact. The
/// imported file is part of the build — `infc` follows the import closure — so
/// no unreachable-file warning and no stale "pending" text appears.
#[test]
fn project_build_multi_file_succeeds() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(
        &temp,
        "demo",
        "use lib::util;\n\npub fn main() -> i32 {\n    return util::add(1, 2);\n}\n",
    );
    write_src_file(
        &temp,
        "lib/util.inf",
        "pub fn add(a: i32, b: i32) -> i32 {\n    return a + b;\n}\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .success()
        .stderr(predicate::str::contains(STALE_PENDING_WARNING_FRAGMENT).not())
        .stderr(predicate::str::contains("not imported by any reachable file").not());

    let wasm = temp.child("out").child("main.wasm");
    assert!(
        wasm.path().exists(),
        "multi-file build must produce {:?}",
        wasm.path()
    );
}

/// A genuinely-unreachable extra `src/**/*.inf` file surfaces the compiler's
/// unreachable-file warning (passed through `infc`'s inherited stderr), names
/// the file, and still builds successfully — and the stale "pending" text is
/// absent. This is the contradiction the old `infs`-side warning created: the
/// build now lies neither about what is compiled nor double-warns.
#[test]
fn project_build_unreachable_file_warns_without_stale_text() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);
    // Not imported by main.inf -> genuinely unreachable.
    write_src_file(
        &temp,
        "lib/orphan.inf",
        "pub fn orphan() -> i32 {\n    return 9;\n}\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("orphan.inf"))
        .stderr(predicate::str::contains(
            "not imported by any reachable file",
        ))
        .stderr(predicate::str::contains(STALE_PENDING_WARNING_FRAGMENT).not());

    let wasm = temp.child("out").child("main.wasm");
    assert!(wasm.path().exists(), "build should still succeed");
}

/// A `use` of a file that does not exist fails with a non-zero exit and the
/// compiler's missing-import-file error, which names the expected path and (for
/// a near-miss sibling) offers a "did you mean" suggestion. The suggestion is
/// produced by `infc` and reaches the user through inherited stderr.
#[test]
fn project_build_missing_import_errors_with_suggestion() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    // `utill` is a one-character typo of the sibling `util`.
    scaffold_project(
        &temp,
        "demo",
        "use lib::utill;\n\npub fn main() -> i32 {\n    return 0;\n}\n",
    );
    write_src_file(
        &temp,
        "lib/util.inf",
        "pub fn add(a: i32, b: i32) -> i32 {\n    return a + b;\n}\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().failure().stderr(
        predicate::str::contains("imported file not found")
            .and(predicate::str::contains("did you mean `util`")),
    );

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "a missing import must abort before any WASM is written"
    );
}

/// `infs build -v` on a valid multi-file project produces both the `.wasm` and
/// the `.v` proof output — confirming the proof flow is wired through `infs` for
/// multi-file projects, not just single files.
#[test]
fn project_build_multi_file_v_flag_produces_proof_output() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(
        &temp,
        "demo",
        "use lib::util;\n\npub fn main() -> i32 {\n    return util::add(1, 2);\n}\n",
    );
    write_src_file(
        &temp,
        "lib/util.inf",
        "pub fn add(a: i32, b: i32) -> i32 {\n    return a + b;\n}\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "multi-file `-v` build must produce out/main.wasm"
    );
    assert!(
        temp.child("out").child("main.v").path().exists(),
        "multi-file `-v` build must produce out/main.v"
    );
}

/// Single-file `infs build file.inf` must behave exactly as before the
/// project-mode addition (regression guard for the optional-path change).
#[test]
fn single_file_build_still_works() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let wasm = temp.child("out").child("trivial.wasm");
    assert!(wasm.path().exists());
}

/// Four-tier byte comparison: project-mode WASM must be byte-identical to what
/// single-file `infc` produces for the same source. The control is critical —
/// `infc src/main.inf` is run with CWD = project root so the `main` stem (and
/// therefore the WASM name section) matches; comparing against a differently
/// named source would diverge in the name section even with identical codegen.
#[test]
fn project_build_wasm_byte_identical_to_infc() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp_project = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp_project, "demo", PROJECT_MAIN_SRC);

    // Project-mode build.
    let mut cmd_project = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_project
        .env("INFC_PATH", &infc_path)
        .current_dir(temp_project.path())
        .arg("build");
    cmd_project.assert().success();

    // Reference: infc compiling src/main.inf with CWD = the project root, so
    // the source stem ("main") and out/ location match the project build.
    let temp_ref = assert_fs::TempDir::new().unwrap();
    temp_ref
        .child("src")
        .child("main.inf")
        .write_str(PROJECT_MAIN_SRC)
        .unwrap();

    let mut cmd_ref = Command::new(&infc_path);
    cmd_ref
        .current_dir(temp_ref.path())
        .arg(std::path::Path::new("src").join("main.inf"));
    cmd_ref.assert().success();

    let project_wasm = temp_project.child("out").child("main.wasm");
    let ref_wasm = temp_ref.child("out").child("main.wasm");
    assert!(
        project_wasm.path().exists(),
        "project build produced no WASM"
    );
    assert!(ref_wasm.path().exists(), "reference infc produced no WASM");

    let project_bytes = std::fs::read(project_wasm.path()).unwrap();
    let ref_bytes = std::fs::read(ref_wasm.path()).unwrap();
    assert_eq!(
        project_bytes, ref_bytes,
        "project-mode WASM must be byte-identical to single-file infc output"
    );
}

// Project-mode Run Tests

/// A `main` returning a nonzero constant, used to assert wasmtime surfaces the
/// return value (printed to stdout by `--invoke`).
const PROJECT_MAIN_NONZERO_SRC: &str = "pub fn main() -> i32 {\n    return 42;\n}\n";

/// `main.inf` that fails to compile (undefined identifier), used to assert a
/// compile error propagates as a non-zero exit before wasmtime is invoked.
const PROJECT_MAIN_BROKEN_SRC: &str = "pub fn main() -> i32 {\n    return nope;\n}\n";

/// Both `infc` and `wasmtime` are required to execute a project end-to-end.
/// Returns the `infc` path when both are present; otherwise defers to
/// [`require_wasmtime`] and [`require_infc`], which skip locally and abort
/// under CI.
fn require_infc_and_wasmtime() -> Option<std::path::PathBuf> {
    if !require_wasmtime() {
        return None;
    }
    require_infc()
}

/// Project `run` from the project root: builds `<root>/out/main.wasm` and
/// invokes `main`, which returns 0 → exit 0.
#[test]
fn project_run_from_root_invokes_main() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run");

    cmd.assert().success();

    // The build must have landed the WASM at the project root.
    let wasm = temp.child("out").child("main.wasm");
    assert!(
        wasm.path().exists(),
        "project run should have produced {:?}",
        wasm.path()
    );
}

/// Project `run` surfaces `main`'s return value: wasmtime `--invoke` prints the
/// returned i32 to stdout. A `main` returning 42 prints `42` with exit 0.
#[test]
fn project_run_prints_main_return_value() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_NONZERO_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

/// Project `run` invoked from a nested subdir still builds at the root and runs
/// `<root>/out/main.wasm`.
#[test]
fn project_run_from_subdir_runs_root_wasm() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);
    let subdir = temp.child("src");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(subdir.path())
        .arg("run");

    cmd.assert().success();

    let wasm_at_root = temp.child("out").child("main.wasm");
    assert!(
        wasm_at_root.path().exists(),
        "out/main.wasm should land at the project root: {:?}",
        wasm_at_root.path()
    );
}

/// `infs run` with no manifest anywhere up the tree fails with the remediation
/// error naming `Inference.toml`.
///
/// wasmtime availability is checked first (fail-fast parity with single-file
/// mode), so the discovery error is only reachable when wasmtime is present.
#[test]
fn project_run_without_manifest_errors() {
    if !require_wasmtime() {
        return;
    }

    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("run");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Inference.toml"));
}

/// A project whose manifest exists but `src/main.inf` is missing fails with the
/// entry-point remediation error. Gated on wasmtime (checked before discovery).
#[test]
fn project_run_missing_entry_point_errors() {
    if !require_wasmtime() {
        return;
    }

    let temp = assert_fs::TempDir::new().unwrap();
    temp.child("Inference.toml")
        .write_str("[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n")
        .unwrap();
    // No src/main.inf created.

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("run");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("entry point").and(predicate::str::contains("main.inf")));
}

/// `--entry-point` with a non-`main` value in project mode is rejected with
/// guidance to use single-file mode. This is an argument-validation error, so
/// it fires before the wasmtime check and needs no external tools.
#[test]
fn project_run_rejects_non_main_entry_point() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("run")
        .arg("--entry-point")
        .arg("helper");

    cmd.assert().failure().stderr(
        predicate::str::contains("Project mode always invokes `main`")
            .and(predicate::str::contains("infs run path/to/file.inf")),
    );
}

/// `--entry-point main` (the explicit default) in project mode is allowed — it
/// must not be treated as a custom entry point. Full run, so gated on tools.
#[test]
fn project_run_allows_explicit_main_entry_point() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run")
        .arg("--entry-point")
        .arg("main");

    cmd.assert().success();
}

/// Project mode is structurally arg-free: the first bare token on the command
/// line binds to the positional `path`, which selects *single-file* mode. So
/// `infs run -- ignored-arg` is not "project mode with trailing args"
/// — it is single-file mode with `path = ignored-arg`, which does not exist.
/// This pins the parsing contract that makes the in-code trailing-args warning
/// unreachable through the CLI (the warning is retained as a defensive guard).
#[test]
fn project_run_token_selects_single_file_mode() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("run")
        .arg("--")
        .arg("ignored-arg");

    // Single-file mode: the token is treated as the source path, which is
    // missing -> "Path not found", proving it never entered project mode.
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Path not found: ignored-arg"));
}

/// A compile error in `main.inf` propagates as a non-zero exit, and wasmtime is
/// never invoked (no "Invoking 'main'" line on stdout).
#[test]
fn project_run_propagates_compile_error() {
    if !require_wasmtime() {
        return;
    }

    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_BROKEN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run");

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("Invoking 'main'").not());
}

// Project-mode Manifest Semantics Tests

/// Scaffolds a project whose `Inference.toml` embeds the given `[build]` /
/// `[verification]` body (appended after `[package]`). `manifest_extra` is raw
/// TOML, e.g. `"[build]\nmode = \"proof\"\n"`.
fn scaffold_project_with_manifest(
    dir: &assert_fs::TempDir,
    name: &str,
    main_src: &str,
    manifest_extra: &str,
) {
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n\n{manifest_extra}"
    );
    dir.child("Inference.toml").write_str(&manifest).unwrap();
    dir.child("src")
        .child("main.inf")
        .write_str(main_src)
        .unwrap();
}

/// Default-manifest (compile) build writes `<root>/out/main.wasm` and creates
/// NO `proofs/` directory — the `proofs/` manifest default must never be
/// forwarded as `--out-dir` in compile mode.
#[test]
fn project_build_default_manifest_no_proofs_dir() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");
    cmd.assert().success();

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "compile build must write out/main.wasm"
    );
    assert!(
        !temp.child("proofs").path().exists(),
        "compile build must NOT create proofs/ (no --out-dir forwarded)"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "compile build must not emit a .v"
    );
}

/// Manifest `[build] mode = "proof"` (default output-dir) produces BOTH the
/// `.wasm` and `.v` under `<root>/proofs/` (the default output-dir is honored
/// in proof mode and `--out-dir` moves both artifacts).
#[test]
fn project_build_manifest_proof_writes_under_proofs() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nmode = \"proof\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");
    cmd.assert().success();

    assert!(
        temp.child("proofs").child("main.wasm").path().exists(),
        "proof build: .wasm must land under proofs/"
    );
    assert!(
        temp.child("proofs").child("main.v").path().exists(),
        "proof build: .v must land under proofs/"
    );
    // And NOT in out/ (the default location for compile mode).
    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "proof build must not also write out/main.wasm"
    );
}

/// Manifest `[verification] output-dir = "artifacts"` in proof mode redirects
/// BOTH artifacts under `<root>/artifacts/`.
#[test]
fn project_build_proof_honors_custom_output_dir() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nmode = \"proof\"\n\n[verification]\noutput-dir = \"artifacts\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");
    cmd.assert().success();

    assert!(
        temp.child("artifacts").child("main.wasm").path().exists(),
        "custom output-dir: .wasm must land under artifacts/"
    );
    assert!(
        temp.child("artifacts").child("main.v").path().exists(),
        "custom output-dir: .v must land under artifacts/"
    );
    assert!(
        !temp.child("proofs").path().exists(),
        "custom output-dir must not also create the default proofs/"
    );
}

/// CLI `--mode compile` overrides a manifest `mode = "proof"` AND ignores
/// `output-dir`: the build writes only `out/main.wasm`, no proofs/, no .v.
#[test]
fn project_build_cli_compile_overrides_manifest_proof() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nmode = \"proof\"\n\n[verification]\noutput-dir = \"artifacts\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg("--mode")
        .arg("compile");
    cmd.assert().success();

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "CLI compile override must write out/main.wasm"
    );
    assert!(
        !temp.child("proofs").path().exists() && !temp.child("artifacts").path().exists(),
        "CLI compile override must ignore output-dir entirely"
    );
    assert!(
        !temp.child("out").child("main.v").path().exists(),
        "CLI compile override must not emit a .v"
    );
}

/// CLI `--mode proof` on a DEFAULT (compile) manifest uses the default
/// `output-dir` (`proofs/`): both artifacts land under `<root>/proofs/`.
#[test]
fn project_build_cli_proof_on_default_manifest_uses_proofs() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg("--mode")
        .arg("proof");
    cmd.assert().success();

    assert!(
        temp.child("proofs").child("main.wasm").path().exists()
            && temp.child("proofs").child("main.v").path().exists(),
        "CLI --mode proof must honor the default output-dir (proofs/)"
    );
}

/// `-v` alone on a default (compile) manifest is NOT treated as effective-proof
/// by `infs`: it forwards only `-v`, no `--out-dir`. `infc` derives proof
/// internally and writes BOTH artifacts to `out/` (output-dir is not consulted
/// — the `-v` ⇄ proof implication belongs to `infc::normalize_args`).
#[test]
fn project_build_v_alone_writes_both_to_out_not_proofs() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg("-v");
    cmd.assert().success();

    assert!(
        temp.child("out").child("main.wasm").path().exists()
            && temp.child("out").child("main.v").path().exists(),
        "`-v` alone must write both .wasm and .v to out/ (infc's implication)"
    );
    assert!(
        !temp.child("proofs").path().exists(),
        "`-v` alone must NOT trigger output-dir forwarding"
    );
}

/// Project `run` always builds in compile mode regardless of `[build] mode =
/// "proof"`: it executes fine, the wasm is in `out/`, and no `proofs/` dir is
/// created (proof-mode wasm would carry non-executable custom opcodes).
#[test]
fn project_run_forces_compile_ignoring_manifest_proof() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nmode = \"proof\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run");
    cmd.assert().success();

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "project run must build an executable in out/ even with manifest proof mode"
    );
    assert!(
        !temp.child("proofs").path().exists(),
        "project run must ignore [build] mode/output-dir (no proofs/)"
    );
}

/// `infs new` scaffolds a manifest with an explicit `[build] mode = "compile"`.
/// The full parse+validate round-trip through `from_toml` is unit-tested in
/// `scaffold.rs`; here we assert the user-facing CLI emits the load-bearing
/// field, and that a subsequent project `build` from the scaffold succeeds
/// (proving the loader accepts it end-to-end).
#[test]
fn scaffolded_project_manifest_has_compile_mode() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();

    let mut new_cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    new_cmd
        .current_dir(temp.path())
        .arg("new")
        .arg("demo")
        .arg("--no-git");
    new_cmd.assert().success();

    let project = temp.child("demo");
    let manifest_path = project.child("Inference.toml");
    assert!(
        manifest_path.path().exists(),
        "new must scaffold a manifest"
    );

    let content = std::fs::read_to_string(manifest_path.path()).unwrap();
    assert!(
        content.contains("[build]") && content.contains("mode = \"compile\""),
        "scaffolded manifest must carry an explicit [build] mode = compile"
    );

    // End-to-end: the scaffolded project must build (the loader accepts it).
    let mut build_cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    build_cmd
        .env("INFC_PATH", &infc_path)
        .current_dir(project.path())
        .arg("build");
    build_cmd.assert().success();
    assert!(project.child("out").child("main.wasm").path().exists());
}

/// Old-infc out-dir gate: a stub `infc` reporting ABI `1.0` (no `--out-dir`)
/// paired with a manifest that needs `output-dir` (proof mode) must hard-error
/// with remediation mentioning the required ABI — never emit the flag blind.
///
/// Unix-only: relies on an executable shell stub. The stub cannot actually
/// compile, but the gate fires *before* the spawn, so the hard error is
/// reached deterministically.
#[cfg(unix)]
#[test]
fn project_build_old_infc_with_output_dir_hard_errors() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nmode = \"proof\"\n",
    );

    // Stub infc: reports a non-matching commit and ABI "1.0" (minor 0 → no
    // --out-dir support), exits 0 for the probes.
    let stub = temp.child("infc_stub");
    stub.write_str(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
           --abi-version) printf '1.0\\n'; exit 0 ;;\n\
           *) exit 0 ;;\n\
         esac\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(stub.path(), perms).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", stub.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().failure().stderr(
        predicate::str::contains("--out-dir")
            .and(predicate::str::contains("ABI"))
            .and(predicate::str::contains("output-dir")),
    );
}

// Project-mode `[build] wasm-features` Tests
//
// These pin the observable end of the opt-in: the manifest key must change the
// instruction set of the emitted artifact, be visible in the build log, reach the
// post-build optimizer's feature flags, and be refused rather than forwarded to
// an `infc` that cannot honor it.

/// `src/main.inf` whose struct local forces a stack frame. The frame's zero-fill
/// lowers to `memory.fill` when bulk memory is enabled and to an explicit store
/// sequence when it is not, which makes the instruction-level choice directly
/// observable in the artifact bytes.
const PROJECT_MAIN_STRUCT_SRC: &str = "struct P {\n    x: i32;\n    y: i32;\n}\n\n\
     pub fn main() -> i32 {\n    let p: P = P { x: 1, y: 2 };\n    return p.x + p.y;\n}\n";

/// Validates `bytes` at the pure WebAssembly 1.0 level.
///
/// `WasmFeatures::WASM1` is the MVP plus mutable globals, which is exactly
/// Inference's baseline (every module that allocates a frame exports a mutable
/// `__stack_pointer`). Crucially it excludes `BULK_MEMORY`, so this failing while
/// [`wasm_is_valid`] succeeds *is* the proof that an artifact uses bulk memory —
/// a module valid under `… | BULK_MEMORY` but not under `WASM1` must use
/// something from that proposal.
///
/// Deciding the question with the validator rather than by matching operator
/// names means these tests cannot drift from the compiler's notion of the
/// proposal the way a hand-copied opcode list would.
fn wasm_is_valid_at_wasm_1_0(bytes: &[u8]) -> bool {
    inf_wasmparser::Validator::new_with_features(inf_wasmparser::WasmFeatures::WASM1)
        .validate_all(bytes)
        .is_ok()
}

/// Asserts `bytes` is a valid module that uses bulk memory.
///
/// Stated as the conjunction that pins it exactly: valid under the bulk-inclusive
/// envelope, invalid without it.
fn assert_artifact_uses_bulk_memory(bytes: &[u8], context: &str) {
    assert!(
        wasm_is_valid(bytes),
        "{context}: the artifact must validate under the bulk-inclusive envelope"
    );
    assert!(
        !wasm_is_valid_at_wasm_1_0(bytes),
        "{context}: the artifact validates at pure WebAssembly 1.0, so it does \
         NOT use bulk memory — the feature request did not reach codegen"
    );
}

/// Asserts `bytes` is a valid module that stays within WebAssembly 1.0.
fn assert_artifact_is_wasm_1_0(bytes: &[u8], context: &str) {
    assert!(
        wasm_is_valid_at_wasm_1_0(bytes),
        "{context}: the artifact must validate at pure WebAssembly 1.0"
    );
}

/// The echo line `infs` prints for a resolved non-empty feature set.
const WASM_FEATURES_ECHO: &str = "wasm-features: bulk-memory";

/// Asserts the feature echo appears exactly once in `stdout`, as a complete line,
/// and that the superseded bare `features:` wording appears nowhere.
///
/// A `contains` assertion cannot pin the wording here: `"wasm-features:
/// bulk-memory"` *contains* `"features: bulk-memory"`, so a substring match is
/// satisfied by either spelling and would silently accept a rename in either
/// direction. Whole-line equality fixes exactly one spelling — a regression to the
/// bare wording yields zero matches and fails — and counting pins that one build
/// echoes once. The explicit negative additionally catches a second echo being
/// added alongside the first rather than replacing it.
fn assert_echoes_features_once(stdout: &str) {
    let matches = stdout
        .lines()
        .filter(|line| line.trim() == WASM_FEATURES_ECHO)
        .count();
    assert_eq!(
        matches, 1,
        "expected exactly one `{WASM_FEATURES_ECHO}` line, found {matches} in:\n{stdout}"
    );
    assert!(
        !stdout
            .lines()
            .any(|line| line.trim_start().starts_with("features:")),
        "no line may use the superseded bare `features:` wording, got:\n{stdout}"
    );
}

/// The stdout of a successful `infs` invocation, as a string.
fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

/// Reads `<root>/out/main.wasm`, asserting it exists.
fn read_project_artifact(temp: &assert_fs::TempDir) -> Vec<u8> {
    let path = temp.child("out").child("main.wasm");
    assert!(
        path.path().exists(),
        "the build must have produced out/main.wasm"
    );
    std::fs::read(path.path()).expect("read out/main.wasm")
}

/// The baseline this feature is measured against: with no `wasm-features` key the
/// artifact is pure WebAssembly 1.0 (no bulk operator) and nothing is echoed.
#[test]
fn project_build_without_wasm_features_emits_no_bulk_memory() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_STRUCT_SRC);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("features:").not());

    assert_artifact_is_wasm_1_0(
        &read_project_artifact(&temp),
        "a build with no wasm-features key",
    );
}

/// `[build] wasm-features = ["bulk-memory"]` reaches `infc` and changes the
/// emitted instruction set, and the resolved set is echoed once to stdout.
#[test]
fn project_build_with_bulk_memory_emits_a_bulk_operator() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");
    assert_echoes_features_once(&stdout_of(&cmd.assert().success()));

    assert_artifact_uses_bulk_memory(&read_project_artifact(&temp), "an opted-in project build");
}

/// The same opted-in project with `[build.wasm-opt]`: the pre-optimization scan
/// sees the bulk operator codegen now emits, so the optimizer invocation carries
/// `--enable-bulk-memory`. Without it Binaryen would refuse to parse the input.
#[test]
fn wasm_opt_enables_bulk_memory_for_a_feature_opted_project() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n\n[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");
    cmd.assert().success();

    let invocations = optimizer_invocations(log.path());
    assert_eq!(
        invocations.len(),
        1,
        "wasm-opt must be spawned exactly once, got: {invocations:?}"
    );
    let args = &invocations[0];
    assert_eq!(args.len(), 7, "unexpected argument count: {args:?}");
    assert_eq!(args[0], "-Oz");
    assert_eq!(args[1], "--mvp-features");
    assert_eq!(args[2], "--enable-mutable-globals");
    assert_eq!(
        args[3], "--enable-bulk-memory",
        "the input carries bulk memory, so Binaryen must be told to parse it"
    );
    assert_eq!(file_name_of(&args[4]), Some("main.wasm"));
    assert_eq!(args[5], "-o");
    assert_eq!(file_name_of(&args[6]), Some("main.wasm.opt"));
}

/// Old-infc wasm-features gate: a stub `infc` reporting ABI `1.1` — new enough
/// for `--out-dir`, too old for `--wasm-features` — paired with a manifest that
/// requests a feature must hard-error naming the required ABI and both
/// remediations. It must never forward the flag blind, and must not claim what
/// the older `infc` would emit instead (nothing in the ABI reveals that).
///
/// Unix-only: relies on an executable shell stub. The stub cannot compile
/// anything, but the gate fires before the spawn, so the error is deterministic.
#[cfg(unix)]
#[test]
fn project_build_old_infc_with_wasm_features_hard_errors() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let stub = temp.child("infc_stub");
    stub.write_str(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
           --abi-version) printf '1.1\\n'; exit 0 ;;\n\
           *) exit 0 ;;\n\
         esac\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(stub.path(), perms).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", stub.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().failure().stderr(
        predicate::str::contains("--wasm-features")
            .and(predicate::str::contains("1.2"))
            .and(predicate::str::contains("update the toolchain"))
            .and(predicate::str::contains("[build] wasm-features")),
    );
}

/// Single-file mode honors the enclosing project's `wasm-features`: building
/// `src/main.inf` by path must not silently emit a module at a different
/// instruction level than `infs build` does for the same project.
#[test]
fn single_file_build_honors_enclosing_manifest_wasm_features() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(std::path::Path::new("src").join("main.inf"));
    assert_echoes_features_once(&stdout_of(&cmd.assert().success()));

    assert_artifact_uses_bulk_memory(
        &read_project_artifact(&temp),
        "a single-file build inside an opted-in project",
    );
}

/// A source outside any project takes the defaults: no manifest walk result, no
/// feature request, no error, and a plain WebAssembly 1.0 artifact.
#[test]
fn single_file_build_outside_a_project_defaults_to_wasm_1_0() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let source = temp.child("standalone.inf");
    source.write_str(PROJECT_MAIN_STRUCT_SRC).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg("standalone.inf");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("features:").not());

    let bytes = std::fs::read(temp.child("out").child("standalone.wasm").path())
        .expect("read out/standalone.wasm");
    assert_artifact_is_wasm_1_0(&bytes, "a manifest-free single-file build");
}

/// Mode-independence, the invariant that keeps proof artifacts meaningful: a
/// proof-mode build with `wasm-features` emits the same instruction set a
/// compile-mode build would, and the Rocq translation describes it. A `.v` that
/// disagreed with the shipped `.wasm` would be worthless as a proof artifact, so
/// no code path may gate the feature on the build mode.
///
/// Proof mode also forwards `--out-dir`, so both artifacts land under `proofs/`.
#[test]
fn proof_build_with_bulk_memory_agrees_between_the_wasm_and_the_v() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nmode = \"proof\"\nwasm-features = [\"bulk-memory\"]\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");
    assert_echoes_features_once(&stdout_of(&cmd.assert().success()));

    let wasm = std::fs::read(temp.child("proofs").child("main.wasm").path())
        .expect("proof mode must write proofs/main.wasm");
    assert_artifact_uses_bulk_memory(
        &wasm,
        "a proof-mode build (the feature must not be gated on mode)",
    );

    let translation = std::fs::read_to_string(temp.child("proofs").child("main.v").path())
        .expect("proof mode must write proofs/main.v");
    assert!(
        translation.contains("BI_memory_fill"),
        "the Rocq translation must describe the operator the .wasm carries"
    );
}

/// Project-mode `infs run` shares the project-build path, so the manifest's
/// `wasm-features` governs the artifact it executes too. The bulk-bearing module
/// must also still execute: `main` returns `1 + 2`. (Single-file `run` is covered
/// separately — it reaches `infc` by a different route.)
#[test]
fn project_run_honors_wasm_features_and_executes() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run");
    let stdout = stdout_of(&cmd.assert().success());
    assert_echoes_features_once(&stdout);
    assert!(
        stdout.contains('3'),
        "wasmtime must print main's return value (1 + 2), got:\n{stdout}"
    );

    assert_artifact_uses_bulk_memory(
        &read_project_artifact(&temp),
        "the artifact project-mode `run` executed",
    );
}

/// The wire spelling, against an `infc` reached through the ABI branch.
///
/// Every real-`infc` test here takes the `commit_matched` short-circuit, because
/// `infs` and `infc` are built from the same tree in dev and CI — so none of them
/// exercises the `minor >= 2` positive branch, and none observes what actually
/// lands on the command line. This stub reports a mismatched commit and ABI 1.2,
/// forcing that branch, and logs its argv so the flag and its value can be
/// checked as adjacent argv entries (a single `--wasm-features=…` token or a
/// space-split value would both fail this).
///
/// The stub cannot compile, so no artifact appears; with no `[build.wasm-opt]`
/// table the post-build step is a no-op and the build still succeeds.
#[cfg(unix)]
#[test]
fn wasm_features_reaches_infc_as_two_adjacent_argv_entries() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );
    let argv_log = temp.child("argv.log");

    let stub = temp.child("infc_stub");
    stub.write_str(&format!(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
           --abi-version) printf '{}.2\\n'; exit 0 ;;\n\
           *) printf '%s\\n' \"$@\" >> '{}'; exit 0 ;;\n\
         esac\n",
        inference_compiler_interface::COMPILER_ABI_MAJOR,
        argv_log.path().display()
    ))
    .unwrap();
    let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(stub.path(), perms).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", stub.path())
        .current_dir(temp.path())
        .arg("build");
    assert_echoes_features_once(&stdout_of(&cmd.assert().success()));

    let logged = std::fs::read_to_string(argv_log.path()).expect("the stub must log its argv");
    let argv: Vec<&str> = logged.lines().collect();
    let flag_positions: Vec<usize> = argv
        .iter()
        .enumerate()
        .filter(|(_, arg)| **arg == "--wasm-features")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        flag_positions.len(),
        1,
        "`--wasm-features` must be forwarded exactly once, got argv: {argv:?}"
    );
    assert_eq!(
        argv.get(flag_positions[0] + 1),
        Some(&"bulk-memory"),
        "the flag's value must be the next argv entry, got argv: {argv:?}"
    );
}

/// The ABI gate on the single-file `build` route. After the shared forwarder
/// extraction there is one gate, but each route into it is pinned separately —
/// a route that forgot to probe would fail here and nowhere else.
#[cfg(unix)]
#[test]
fn single_file_build_old_infc_with_wasm_features_hard_errors() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let stub = temp.child("infc_stub");
    stub.write_str(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
           --abi-version) printf '1.1\\n'; exit 0 ;;\n\
           *) exit 0 ;;\n\
         esac\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(stub.path(), perms).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", stub.path())
        .current_dir(temp.path())
        .arg("build")
        .arg(std::path::Path::new("src").join("main.inf"));

    cmd.assert().failure().stderr(
        predicate::str::contains("--wasm-features")
            .and(predicate::str::contains("1.2"))
            .and(predicate::str::contains("Inference.toml")),
    );
}

/// Single-file `run` honors the enclosing manifest's `wasm-features` too.
///
/// This is the invocation that would otherwise reopen the hole: `infs run
/// src/main.inf` compiles through a different `infc` invocation than `infs build`
/// does and overwrites the same `out/main.wasm`, so a version of this path that
/// ignored the key would leave the project's shipped artifact at whichever
/// instruction level the last command happened to use.
#[test]
fn single_file_run_honors_enclosing_manifest_wasm_features() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run")
        .arg(std::path::Path::new("src").join("main.inf"));
    let stdout = stdout_of(&cmd.assert().success());
    assert_echoes_features_once(&stdout);
    assert!(
        stdout.contains('3'),
        "wasmtime must print main's return value (1 + 2), got:\n{stdout}"
    );

    assert_artifact_uses_bulk_memory(
        &read_project_artifact(&temp),
        "the artifact single-file `run` executed",
    );
}

/// The ABI gate covers single-file `run` as well: a stub `infc` reporting 1.1 is
/// refused rather than handed a flag it cannot honor. Unix-only (executable stub);
/// wasmtime is never reached because the gate fires during compilation.
#[cfg(unix)]
#[test]
fn single_file_run_old_infc_with_wasm_features_hard_errors() {
    use std::os::unix::fs::PermissionsExt;

    if !require_wasmtime() {
        return;
    }

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n",
    );

    let stub = temp.child("infc_stub");
    stub.write_str(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
           --abi-version) printf '1.1\\n'; exit 0 ;;\n\
           *) exit 0 ;;\n\
         esac\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(stub.path(), perms).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", stub.path())
        .current_dir(temp.path())
        .arg("run")
        .arg(std::path::Path::new("src").join("main.inf"));

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--wasm-features").and(predicate::str::contains("1.2")));
}

/// Real-binary end-to-end: a genuine Binaryen `wasm-opt` accepts and optimizes a
/// bulk-bearing artifact.
///
/// The fake-optimizer test proves the flag is *passed*; only a real Binaryen
/// proves it is *sufficient*. Binaryen hard-rejects `0xFC` bulk opcodes unless
/// told to parse them, so without `--enable-bulk-memory` this fails at the
/// optimizer rather than in any assertion. The result must still validate, and
/// must still use bulk memory — `wasm-opt` is not permitted to quietly lower it
/// away into something the re-validation envelope would also accept.
#[test]
fn wasm_opt_real_binary_optimizes_a_bulk_bearing_artifact() {
    let Some(infc_path) = require_infc() else {
        return;
    };
    if !require_wasm_opt() {
        return;
    }

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_STRUCT_SRC,
        "[build]\nwasm-features = [\"bulk-memory\"]\n\n[build.wasm-opt]\nlevel = \"z\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("wasm-opt -Oz: main.wasm"));

    assert_artifact_uses_bulk_memory(
        &read_project_artifact(&temp),
        "the real-Binaryen optimized artifact",
    );
}

/// A manifest key no table knows is a build error naming the offending key —
/// previously-ignored junk is now surfaced rather than silently doing nothing.
#[test]
fn project_build_rejects_an_unknown_manifest_key() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nwasm_features = [\"bulk-memory\"]\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("build");

    cmd.assert().failure().stderr(
        predicate::str::contains("wasm_features").and(predicate::str::contains("wasm-features")),
    );
}

// Project-mode `[wasm-dependencies]` and `-L` Forwarding Tests
//
// `infc` resolves `use { … } from <module>` from `--wasm-dep`, `-L` and
// `INFERENCE_WASM_LIB_PATH` alone, so a project build that forwards none of them
// cannot link an external at all — and in proof mode cannot emit its `.v`. These
// pin both ends: the wire spelling each project route puts on the command line,
// and the end-to-end link through a real `infc` with no environment variable in
// play.

/// `src/main.inf` that binds an external: the declaration, the `use` that binds
/// it to the logical module `arith`, and a `main` whose value only the linked
/// function can produce. Unlinkable unless the dependency reaches `infc`.
const PROJECT_MAIN_EXTERN_SRC: &str = "external fn sum(a: i32, b: i32) -> i32;\n\
     use { sum } from arith;\n\n\
     pub fn main() -> i32 {\n    return sum(40, 2);\n}\n";

/// The library [`PROJECT_MAIN_EXTERN_SRC`] binds against, compiled on its own
/// and vendored into the dependent project as `libs/arith.wasm`.
const ARITH_LIB_SRC: &str = "pub fn sum(a: i32, b: i32) -> i32 {\n    return a + b;\n}\n";

/// The manifest table declaring that dependency. The declared path is written in
/// the manifest's own platform-independent spelling; `infs` resolves it against
/// the project root before forwarding it.
const ARITH_DEPENDENCY_TABLE: &str =
    "[wasm-dependencies]\narith = { path = \"libs/arith.wasm\" }\n";

/// Writes an executable `infc` stub under `dir` that reports a mismatched commit
/// and the current ABI, and appends the argv of every non-probe invocation —
/// everything but the `--commit-hash` and `--abi-version` handshake calls, one
/// entry per line — to `log`. Returns the stub's path.
///
/// The mismatched commit is what makes the stub useful: every real-`infc` test
/// here takes the `commit_matched` short-circuit, so none of them observes what
/// actually lands on the command line. The stub cannot compile, so no artifact
/// appears; with no `[build.wasm-opt]` table the post-build step is a no-op and a
/// `build` through it still succeeds.
///
/// Unix-only: relies on an executable shell script.
#[cfg(unix)]
fn write_argv_logging_infc_stub(
    dir: &assert_fs::TempDir,
    log: &std::path::Path,
) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let stub = dir.child("infc_stub");
    stub.write_str(&format!(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
           --abi-version) printf '{}.{}\\n'; exit 0 ;;\n\
           *) printf '%s\\n' \"$@\" >> '{}'; exit 0 ;;\n\
         esac\n",
        inference_compiler_interface::COMPILER_ABI_MAJOR,
        inference_compiler_interface::COMPILER_ABI_MINOR,
        log.display()
    ))
    .unwrap();
    let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(stub.path(), perms).unwrap();
    stub.path().to_path_buf()
}

/// The argv entry immediately following each occurrence of `flag`, in order of
/// appearance.
///
/// Adjacency is the property under test, and this expresses it exactly: a fused
/// `--flag=value` token never matches `flag` and yields no entry at all, while a
/// value split across entries leaves only its first word here — an expectation on
/// the result fails for either spelling. A flag in final position (its value
/// dropped) yields `None` rather than vanishing.
#[cfg(unix)]
fn argv_values_after<'a>(argv: &[&'a str], flag: &str) -> Vec<Option<&'a str>> {
    argv.iter()
        .enumerate()
        .filter(|(_, entry)| **entry == flag)
        .map(|(index, _)| argv.get(index + 1).copied())
        .collect()
}

/// The raw newline-separated argv a stub logged.
///
/// Returned as one owned `String` rather than the split entries because the
/// entries borrow from it: a caller keeps this alive and splits it in place, so
/// the `&str` slices it feeds to [`argv_values_after`] outlive the call.
#[cfg(unix)]
fn logged_argv(log: &std::path::Path) -> String {
    std::fs::read_to_string(log).expect("the stub must log its argv")
}

/// Compiles [`ARITH_LIB_SRC`] with the real `infc` in a temp directory of its own
/// and returns the emitted module bytes.
///
/// The dependency is a genuine compiler artifact rather than a checked-in blob:
/// what these tests link must be what `infc` emits today, or a change to the
/// module shape would leave them linking a stale fixture.
fn build_arith_library(infc_path: &std::path::Path) -> Vec<u8> {
    let lib = assert_fs::TempDir::new().unwrap();
    lib.child("arith.inf").write_str(ARITH_LIB_SRC).unwrap();

    let mut cmd = Command::new(infc_path);
    cmd.current_dir(lib.path()).arg("arith.inf");
    cmd.assert().success();

    std::fs::read(lib.child("out").child("arith.wasm").path())
        .expect("infc must produce out/arith.wasm for the library")
}

/// Scaffolds a project that binds the `arith` external: the manifest declares the
/// dependency, `src/main.inf` binds `sum`, and `libs/arith.wasm` holds `library`.
fn scaffold_project_with_arith_dependency(dir: &assert_fs::TempDir, library: &[u8]) {
    scaffold_project_with_manifest(dir, "demo", PROJECT_MAIN_EXTERN_SRC, ARITH_DEPENDENCY_TABLE);
    dir.child("libs")
        .child("arith.wasm")
        .write_binary(library)
        .unwrap();
}

/// The wire spelling of external-module forwarding on the project `build` route.
///
/// Three properties at once, none of them observable through a real `infc`: both
/// spellings of the lib-dir flag reach `infc` as `--wasm-lib-dir` plus an
/// adjacent value, in the order given; the manifest dependency is forwarded
/// exactly once; and its value is the declared path resolved against the project
/// *root*, absolute — the resolution the manifest promises, independent of any
/// working directory.
#[cfg(unix)]
#[test]
fn project_build_forwards_lib_dirs_and_manifest_dep_to_infc() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_EXTERN_SRC,
        ARITH_DEPENDENCY_TABLE,
    );
    let argv_log = temp.child("argv.log");
    let stub = write_argv_logging_infc_stub(&temp, argv_log.path());

    // Two real directories, passed through the two spellings of the same flag.
    let first = temp.child("vendor-a");
    first.create_dir_all().unwrap();
    let second = temp.child("vendor-b");
    second.create_dir_all().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &stub)
        .current_dir(temp.path())
        .arg("build")
        .arg("-L")
        .arg(first.path())
        .arg("--wasm-lib-dir")
        .arg(second.path());
    cmd.assert().success();

    let logged = logged_argv(argv_log.path());
    let argv: Vec<&str> = logged.lines().collect();

    assert_eq!(
        argv_values_after(&argv, "--wasm-lib-dir"),
        vec![
            Some(first.path().to_str().unwrap()),
            Some(second.path().to_str().unwrap()),
        ],
        "both lib-dir spellings must reach infc as `--wasm-lib-dir`, in the \
         order given, got argv: {argv:?}"
    );

    let expected_dep = format!(
        "arith={}",
        temp.path()
            .canonicalize()
            .unwrap()
            .join("libs")
            .join("arith.wasm")
            .display()
    );
    assert_eq!(
        argv_values_after(&argv, "--wasm-dep"),
        vec![Some(expected_dep.as_str())],
        "the manifest dependency must be forwarded exactly once, as \
         `<name>=<path resolved against the project root>`, got argv: {argv:?}"
    );
}

/// A *relative* lib dir keeps the meaning it had at the shell.
///
/// The project route spawns `infc` with its working directory set to the project
/// root, so a lib dir forwarded verbatim would be re-read against the root rather
/// than against the directory the user typed it in. Here `-L ../libs` is passed
/// from `<root>/src`, where it names `<root>/libs`; re-anchored to the root it
/// would name `<root>/../libs` — outside the project, and a same-named `.wasm`
/// sitting there would link silently in place of the intended one. The absolute
/// dir alongside it pins the other half: joining must leave it untouched.
///
/// The expectation is built from the *canonicalized* subdirectory because
/// `std::env::current_dir()` in the child reports the symlink-resolved path,
/// which on macOS differs from the temp path this test holds.
#[cfg(unix)]
#[test]
fn project_build_anchors_a_relative_lib_dir_to_the_invocation_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_EXTERN_SRC,
        ARITH_DEPENDENCY_TABLE,
    );
    let argv_log = temp.child("argv.log");
    let stub = write_argv_logging_infc_stub(&temp, argv_log.path());

    // `<root>/libs` is what `../libs` names from `<root>/src`; the root-anchored
    // reading, `<root>/../libs`, is the temp directory's parent.
    temp.child("libs").create_dir_all().unwrap();
    let absolute = temp.child("vendor");
    absolute.create_dir_all().unwrap();

    let invocation_dir = temp.child("src");
    let relative = std::path::Path::new("..").join("libs");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &stub)
        .current_dir(invocation_dir.path())
        .arg("build")
        .arg("-L")
        .arg(&relative)
        .arg("-L")
        .arg(absolute.path());
    cmd.assert().success();

    let logged = logged_argv(argv_log.path());
    let argv: Vec<&str> = logged.lines().collect();

    let anchored = invocation_dir
        .path()
        .canonicalize()
        .unwrap()
        .join(&relative)
        .display()
        .to_string();
    assert_eq!(
        argv_values_after(&argv, "--wasm-lib-dir"),
        vec![
            Some(anchored.as_str()),
            Some(absolute.path().to_str().unwrap()),
        ],
        "a relative lib dir must reach infc anchored at the invocation \
         directory (`{anchored}`), and an absolute one unchanged, got argv: \
         {argv:?}"
    );
}

/// Every declared dependency is forwarded, not just the first, and in the
/// manifest resolution's name order — so two manifests differing only in the
/// order they list the same dependencies produce the same `infc` invocation.
/// Declared here in reverse of the expected order, which a straight iteration
/// over the declarations would preserve.
#[cfg(unix)]
#[test]
fn project_build_forwards_every_manifest_dependency_in_name_order() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_EXTERN_SRC,
        "[wasm-dependencies]\n\
         zeta = { path = \"libs/zeta.wasm\" }\n\
         alpha = { path = \"libs/alpha.wasm\" }\n",
    );
    let argv_log = temp.child("argv.log");
    let stub = write_argv_logging_infc_stub(&temp, argv_log.path());

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &stub)
        .current_dir(temp.path())
        .arg("build");
    cmd.assert().success();

    let logged = logged_argv(argv_log.path());
    let argv: Vec<&str> = logged.lines().collect();

    let root = temp.path().canonicalize().unwrap();
    let alpha = format!("alpha={}", root.join("libs").join("alpha.wasm").display());
    let zeta = format!("zeta={}", root.join("libs").join("zeta.wasm").display());
    assert_eq!(
        argv_values_after(&argv, "--wasm-dep"),
        vec![Some(alpha.as_str()), Some(zeta.as_str())],
        "every declaration must be forwarded, in name order, got argv: {argv:?}"
    );
}

/// Neutrality: a project that declares no dependency and passes no lib dir gets
/// neither flag on the command line. The forwarding is inert for the projects
/// that do not bind externals, which is nearly all of them.
#[cfg(unix)]
#[test]
fn project_build_without_dependencies_forwards_no_external_flags() {
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);
    let argv_log = temp.child("argv.log");
    let stub = write_argv_logging_infc_stub(&temp, argv_log.path());

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &stub)
        .current_dir(temp.path())
        .arg("build");
    cmd.assert().success();

    let logged = logged_argv(argv_log.path());
    let argv: Vec<&str> = logged.lines().collect();
    assert!(
        argv_values_after(&argv, "--wasm-dep").is_empty()
            && argv_values_after(&argv, "--wasm-lib-dir").is_empty(),
        "a project with no externals must get neither flag, got argv: {argv:?}"
    );
}

/// The same wire spelling on the project `run` route, which has no lib-dir flag
/// of its own: the manifest is its only source of externals, so a route that
/// dropped it would leave every project binding `use { … } from <module>`
/// unrunnable.
///
/// wasmtime is checked before the build, hence the gate. The stub compiles
/// nothing, so `out/main.wasm` never appears and the command fails *after* the
/// build step — that exact failure is what proves the build ran at all.
#[cfg(unix)]
#[test]
fn project_run_forwards_manifest_dep_to_infc() {
    if !require_wasmtime() {
        return;
    }

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_EXTERN_SRC,
        ARITH_DEPENDENCY_TABLE,
    );
    let argv_log = temp.child("argv.log");
    let stub = write_argv_logging_infc_stub(&temp, argv_log.path());

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &stub)
        .current_dir(temp.path())
        .arg("run");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("WASM file not found"));

    let logged = logged_argv(argv_log.path());
    let argv: Vec<&str> = logged.lines().collect();

    let expected_dep = format!(
        "arith={}",
        temp.path()
            .canonicalize()
            .unwrap()
            .join("libs")
            .join("arith.wasm")
            .display()
    );
    assert_eq!(
        argv_values_after(&argv, "--wasm-dep"),
        vec![Some(expected_dep.as_str())],
        "project `run` must forward the manifest dependency, got argv: {argv:?}"
    );
    assert!(
        argv_values_after(&argv, "--wasm-lib-dir").is_empty(),
        "`run` has no lib-dir flag to forward, got argv: {argv:?}"
    );
}

/// The end-to-end acceptance: a project whose sources bind an external links in
/// proof mode and emits both artifacts, with no environment workaround.
///
/// `INFERENCE_WASM_LIB_PATH` is explicitly removed — it is `infc`'s last-resort
/// search tier and the very workaround this replaces, so an inherited value would
/// let a passing run prove nothing about the manifest. The `.v` is the load-bearing
/// half: external resolution runs before code generation, so a proof build that
/// cannot resolve produces no translation at all.
#[test]
fn project_proof_build_links_a_manifest_dependency_without_env() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let library = build_arith_library(&infc_path);
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_arith_dependency(&temp, &library);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env_remove("INFERENCE_WASM_LIB_PATH")
        .current_dir(temp.path())
        .arg("build")
        .arg("--mode")
        .arg("proof");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Linked 1 external module"));

    let wasm = std::fs::read(temp.child("proofs").child("main.wasm").path())
        .expect("a linked proof build must write proofs/main.wasm");
    assert!(!wasm.is_empty(), "the linked .wasm must not be empty");

    let translation = std::fs::read_to_string(temp.child("proofs").child("main.v").path())
        .expect("a linked proof build must write proofs/main.v");
    assert!(!translation.is_empty(), "the linked .v must not be empty");
}

/// The linked external actually executes: project `run` on the same project
/// prints `sum(40, 2)`. Resolution reaching `infc` is necessary but not
/// sufficient — this pins that what `run` executes is a module with the
/// dependency's code merged in, not one left with an unresolved import.
#[test]
fn project_run_executes_a_linked_manifest_dependency() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let library = build_arith_library(&infc_path);
    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_arith_dependency(&temp, &library);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env_remove("INFERENCE_WASM_LIB_PATH")
        .current_dir(temp.path())
        .arg("run");
    let stdout = stdout_of(&cmd.assert().success());
    assert!(
        stdout.contains("42"),
        "wasmtime must print the linked call's result (40 + 2), got:\n{stdout}"
    );
}

// Phase 2: Toolchain Management Command Tests

// Install Command Tests

/// Verifies that `infs install --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show version argument and usage.
#[test]
fn install_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("install").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Install"))
        .stdout(predicate::str::contains("VERSION"));
}

/// Verifies that `infs install` shows a helpful error when network is unavailable.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory to avoid affecting the system.
///
/// **Expected behavior**: Exit with non-zero code and print an error message
/// (not panic) when the manifest cannot be fetched.
#[test]
fn install_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .arg("install")
        .arg("0.0.0-nonexistent");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error").or(predicate::str::contains("error")));
}

// Uninstall Command Tests

/// Verifies that `infs uninstall --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show version argument.
#[test]
fn uninstall_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("uninstall").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Uninstall"))
        .stdout(predicate::str::contains("VERSION"));
}

/// Verifies that uninstalling a nonexistent version shows a helpful message.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains installed.
///
/// **Expected behavior**: Exit with non-zero code and indicate the version is not installed.
#[test]
fn uninstall_nonexistent_shows_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .arg("uninstall")
        .arg("0.0.0-nonexistent");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not installed"));
}

// List Command Tests

/// Verifies that `infs list` runs successfully even with no toolchains installed.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with code 0 (not a failure state).
#[test]
fn list_runs_successfully() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("list");

    cmd.assert().success();
}

/// Verifies that `infs list` shows appropriate message when no toolchains are installed.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with code 0 and display "No toolchains installed".
#[test]
fn list_shows_no_toolchains_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No toolchains installed"));
}

// Versions Command Tests

/// Verifies that `infs versions --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show stable and json flags.
#[test]
fn versions_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("versions").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("List available"))
        .stdout(predicate::str::contains("--stable"))
        .stdout(predicate::str::contains("--json"));
}

/// Verifies that `infs versions` shows an error when no network is available.
///
/// **Test setup**: Uses a non-existent distribution server (`INFS_DIST_SERVER`) and
/// isolated `INFERENCE_HOME` to ensure no cached manifest is used.
///
/// **Expected behavior**: Exit with non-zero code and display a network error.
#[test]
fn versions_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_DIST_SERVER", "http://localhost:1")
        .env("INFERENCE_HOME", temp.path())
        .arg("versions")
        .arg("--headless");

    cmd.assert().failure().stderr(
        predicate::str::contains("Failed")
            .or(predicate::str::contains("error"))
            .or(predicate::str::contains("Error")),
    );
}

/// Verifies that `infs versions --stable` flag is accepted.
///
/// **Test setup**: Uses a non-existent distribution server and isolated `INFERENCE_HOME`.
///
/// **Expected behavior**: The flag is parsed correctly (failure is from network, not flag parsing).
#[test]
fn versions_stable_flag_is_accepted() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_DIST_SERVER", "http://localhost:1")
        .env("INFERENCE_HOME", temp.path())
        .arg("versions")
        .arg("--stable")
        .arg("--headless");

    // Should fail due to network, not argument parsing
    cmd.assert().failure();
}

/// Verifies that `infs versions --json` flag is accepted.
///
/// **Test setup**: Uses a non-existent distribution server and isolated `INFERENCE_HOME`.
///
/// **Expected behavior**: The flag is parsed correctly (failure is from network, not flag parsing).
#[test]
fn versions_json_flag_is_accepted() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFS_DIST_SERVER", "http://localhost:1")
        .env("INFERENCE_HOME", temp.path())
        .arg("versions")
        .arg("--json")
        .arg("--headless");

    // Should fail due to network, not argument parsing
    cmd.assert().failure();
}

// Default Command Tests

/// Verifies that `infs default --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show version argument.
#[test]
fn default_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("default").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Set the default"))
        .stdout(predicate::str::contains("VERSION"));
}

/// Verifies that `infs default` requires a version argument.
///
/// **Expected behavior**: Exit with non-zero code when no version is provided.
#[test]
fn default_requires_version_argument() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("default");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("VERSION").or(predicate::str::contains("required")));
}

/// Verifies that setting a nonexistent version as default shows a helpful error.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory with no toolchains.
///
/// **Expected behavior**: Exit with non-zero code and indicate version is not installed
/// or does not exist (depending on whether the version exists in the release manifest).
#[test]
fn default_nonexistent_version_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .arg("default")
        .arg("0.0.0-nonexistent");

    cmd.assert().failure().stderr(
        predicate::str::contains("not installed").or(predicate::str::contains("does not exist")),
    );
}

// Doctor Command Tests

/// Verifies that `infs doctor` runs successfully even with no toolchains installed.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory.
///
/// **Expected behavior**: Exit with code 0 (doctor reports issues but doesn't fail).
#[test]
fn doctor_runs_successfully() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert().success();
}

/// Verifies that `infs doctor` shows platform check in output.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory.
///
/// **Expected behavior**: Output contains "Platform" check.
#[test]
fn doctor_shows_platform_check() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Platform"));
}

/// Verifies that `infs doctor` shows multiple health checks.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory.
///
/// **Expected behavior**: Output contains multiple check sections (Platform, Toolchain, etc.).
#[test]
fn doctor_shows_all_checks() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Platform"))
        .stdout(predicate::str::contains("Toolchain directory"))
        .stdout(predicate::str::contains("Default toolchain"))
        .stdout(predicate::str::contains("infc"))
        .stdout(predicate::str::contains("inference-lsp"));
}

/// Verifies that `infs doctor` output respects the VS Code extension's line contract.
///
/// The VS Code extension at `editors/vscode/src/toolchain/doctor.ts:32` parses check
/// lines with the regex `/^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)/`. Any change to
/// the line shape breaks the extension's doctor rendering. This test locks the
/// format in place so drift is caught in CI before it reaches editors/vscode.
///
/// **Test setup**: Runs `infs doctor` with an isolated `INFERENCE_HOME` so the
/// user's real toolchain state does not influence the output, and with
/// `INFC_PATH` removed so resolver priorities behave deterministically.
#[test]
fn doctor_output_respects_vscode_check_line_contract() {
    let check_pattern = regex::Regex::new(r"^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)").unwrap();

    let temp = assert_fs::TempDir::new().unwrap();
    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .arg("doctor")
        .env("INFERENCE_HOME", temp.path())
        .env_remove("INFC_PATH")
        .output()
        .expect("doctor should run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let check_lines: Vec<_> = stdout
        .lines()
        .filter(|l| l.trim_start().starts_with('['))
        .collect();

    assert!(
        !check_lines.is_empty(),
        "doctor produced zero check lines. Output was:\n{stdout}"
    );

    for line in &check_lines {
        assert!(
            check_pattern.is_match(line),
            "line violates VS Code contract (editors/vscode/src/toolchain/doctor.ts): {line:?}"
        );
    }
}

/// Regression test for the PATH-conflict block in `infs doctor`.
///
/// The baseline `doctor_output_respects_vscode_check_line_contract` test
/// does not trigger the conflict branch — with a pristine `INFERENCE_HOME`,
/// `detect_path_conflicts` returns empty. This test builds a layout where
/// `INFERENCE_HOME/bin/infc` exists *and* a differently-located `infc` is
/// visible on `PATH`, so `detect_path_conflicts` reports a mismatch and
/// the `[WARN] PATH conflict: …` header is actually emitted. Then the same
/// VS Code regex must still match every bracketed line.
///
/// Gated on `unix` because the stub invocation relies on a `#!/bin/sh`
/// shebang with chmod +x — Windows would need a distinct stub builder.
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn doctor_output_respects_vscode_contract_on_path_conflict() {
    use std::os::unix::fs::PermissionsExt;

    let check_pattern = regex::Regex::new(r"^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)").unwrap();

    // Build a managed toolchain layout: INFERENCE_HOME/bin/infc must exist
    // so `detect_path_conflicts` considers the expected location "real".
    let home = assert_fs::TempDir::new().unwrap();
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let managed_infc = bin_dir.join("infc");
    std::fs::write(&managed_infc, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&managed_infc).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&managed_infc, perms).unwrap();

    // Build a separate PATH dir with its own infc stub; pointing PATH here
    // makes `which::which` resolve to a path that differs from `managed_infc`.
    let stub_dir = assert_fs::TempDir::new().unwrap();
    let stub_infc = stub_dir.path().join("infc");
    std::fs::write(&stub_infc, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&stub_infc).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&stub_infc, perms).unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .arg("doctor")
        .env("INFERENCE_HOME", home.path())
        .env("PATH", stub_dir.path())
        .env_remove("INFC_PATH")
        .output()
        .expect("doctor should run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let check_lines: Vec<_> = stdout
        .lines()
        .filter(|l| l.trim_start().starts_with('['))
        .collect();

    assert!(
        check_lines.iter().any(|l| l.contains("PATH conflict:")),
        "expected a `[WARN] PATH conflict: …` line. Output was:\n{stdout}"
    );

    for line in &check_lines {
        assert!(
            check_pattern.is_match(line),
            "line violates VS Code contract (editors/vscode/src/toolchain/doctor.ts): {line:?}"
        );
    }
}

/// Verifies that `infs doctor` shows the checking message.
///
/// **Expected behavior**: Output contains the initial "Checking" message.
#[test]
fn doctor_shows_checking_message() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Checking Inference toolchain"));
}

// Self Update Command Tests

/// Verifies that `infs self --help` displays the available subcommands.
///
/// **Expected behavior**: Exit with code 0 and show the update subcommand.
#[test]
fn self_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("self").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("update").or(predicate::str::contains("Update")));
}

/// Verifies that `infs self update --help` displays usage information.
///
/// **Expected behavior**: Exit with code 0 and show help text.
#[test]
fn self_update_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("self").arg("update").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Update").or(predicate::str::contains("update")));
}

/// Verifies that `infs self update` shows a helpful error when network is unavailable.
///
/// **Test setup**: Uses an isolated `INFERENCE_HOME` directory and points to an invalid
/// distribution server via `INFS_DIST_SERVER` environment variable.
///
/// **Expected behavior**: Exit with non-zero code and print an error message
/// (not panic) when the manifest cannot be fetched.
#[test]
fn self_update_without_network_shows_error() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .env("INFS_DIST_SERVER", "http://invalid-test-server.localhost")
        .arg("self")
        .arg("update");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error").or(predicate::str::contains("error")));
}

/// Verifies that `INFS_DIST_SERVER` environment variable is used for manifest fetching.
///
/// **Test setup**: Sets `INFS_DIST_SERVER` to an invalid test server URL and runs install.
/// The cache TTL is set to 0 to force a network fetch.
///
/// **Expected behavior**: Exit with non-zero code and the error message should contain
/// the custom server URL, proving the environment variable was used.
#[test]
fn install_uses_custom_dist_server() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path())
        .env("INFS_DIST_SERVER", "http://invalid-test-server.localhost")
        .arg("install");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid-test-server"));
}

/// Verifies that `infs self` without a subcommand shows an error.
///
/// **Expected behavior**: Exit with non-zero code when no subcommand is provided.
#[test]
fn self_requires_subcommand() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("self");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("subcommand").or(predicate::str::contains("required")));
}

// Phase 3: Project Scaffolding Command Tests

// New Command Tests

/// Verifies that `infs new --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show NAME argument, --no-git flag, and path option.
#[test]
fn new_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("new").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("NAME"))
        .stdout(predicate::str::contains("--no-git"))
        .stdout(predicate::str::contains("PATH").or(predicate::str::contains("path")));
}

/// Verifies that `infs new` requires a name argument.
///
/// **Expected behavior**: Exit with non-zero code when no name is provided.
#[test]
fn new_requires_name_argument() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("new");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("NAME").or(predicate::str::contains("required")));
}

/// Verifies that `infs new` creates the complete project structure.
///
/// **Test setup**: Uses a temporary directory and --no-git to avoid git dependency.
///
/// **Expected behavior**: Creates Inference.toml, src/main.inf, .gitignore, tests/, and proofs/.
#[test]
fn new_creates_project_structure() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("myproject")
        .arg("--no-git");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created project"));

    let project_dir = temp.child("myproject");
    assert!(
        project_dir.path().exists(),
        "Project directory should exist"
    );
    assert!(
        project_dir.child("Inference.toml").path().exists(),
        "Inference.toml should exist"
    );
    assert!(
        project_dir.child("src").child("main.inf").path().exists(),
        "src/main.inf should exist"
    );
    // With --no-git, .gitignore should NOT be created
    assert!(
        !project_dir.child(".gitignore").path().exists(),
        ".gitignore should NOT exist with --no-git"
    );
    assert!(
        project_dir.child("tests").path().exists(),
        "tests/ directory should exist"
    );
    assert!(
        project_dir.child("proofs").path().exists(),
        "proofs/ directory should exist"
    );
}

/// Verifies that `infs new` validates project names and rejects invalid ones.
///
/// **Test cases**:
/// - Names starting with numbers (e.g., "123invalid")
/// - Reserved keywords (e.g., "fn")
///
/// **Expected behavior**: Exit with non-zero code and display an error message.
#[test]
fn new_validates_project_name() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("123invalid")
        .arg("--no-git");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("start with"));

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("fn")
        .arg("--no-git");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

/// Verifies that `infs new` fails when the target directory already exists.
///
/// **Test setup**: Pre-creates a directory with the same name.
///
/// **Expected behavior**: Exit with non-zero code and indicate directory exists.
#[test]
fn new_fails_if_directory_exists() {
    let temp = assert_fs::TempDir::new().unwrap();
    let existing_dir = temp.child("existing_project");
    std::fs::create_dir_all(existing_dir.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("existing_project")
        .arg("--no-git");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

/// Verifies that `infs new` generates a valid Inference.toml manifest.
///
/// **Expected behavior**: The manifest contains the correct project name and version.
#[test]
fn new_generates_valid_manifest() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("test_manifest_project")
        .arg("--no-git");

    cmd.assert().success();

    let manifest_path = temp.child("test_manifest_project").child("Inference.toml");
    let manifest_content =
        std::fs::read_to_string(manifest_path.path()).expect("Failed to read Inference.toml");

    assert!(
        manifest_content.contains("name = \"test_manifest_project\""),
        "Manifest should contain project name"
    );
    assert!(
        manifest_content.contains("version = \"0.1.0\""),
        "Manifest should contain default version"
    );
}

/// Verifies that `infs new --no-git` skips git initialization.
///
/// **Expected behavior**: Project is created successfully without .git directory.
#[test]
fn new_with_no_git_flag() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("nogit_project")
        .arg("--no-git");

    cmd.assert().success();

    let project_dir = temp.child("nogit_project");
    assert!(
        project_dir.path().exists(),
        "Project directory should exist"
    );
    assert!(
        !project_dir.child(".git").path().exists(),
        ".git directory should not exist when --no-git is used"
    );
}

// Init Command Tests

/// Verifies that `infs init --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show the name option.
#[test]
fn init_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("init").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("NAME").or(predicate::str::contains("name")));
}

/// Verifies that `infs init` creates the manifest and source files.
///
/// **Test setup**: Uses a temporary directory.
///
/// **Expected behavior**: Creates Inference.toml and src/main.inf.
#[test]
fn init_creates_manifest() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("init")
        .arg("init_test_project");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(
        temp.child("Inference.toml").path().exists(),
        "Inference.toml should exist"
    );
    assert!(
        temp.child("src").child("main.inf").path().exists(),
        "src/main.inf should exist"
    );
}

/// Verifies that `infs init` uses a custom name when provided.
///
/// **Expected behavior**: The manifest contains the specified project name.
#[test]
fn init_uses_custom_name() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("init")
        .arg("custom_name_project");

    cmd.assert().success();

    let manifest_content = std::fs::read_to_string(temp.child("Inference.toml").path())
        .expect("Failed to read Inference.toml");

    assert!(
        manifest_content.contains("name = \"custom_name_project\""),
        "Manifest should contain the custom project name"
    );
}

/// Verifies that `infs init` fails when Inference.toml already exists.
///
/// **Test setup**: Pre-creates an Inference.toml file.
///
/// **Expected behavior**: Exit with non-zero code and indicate manifest exists.
#[test]
fn init_fails_if_manifest_exists() {
    let temp = assert_fs::TempDir::new().unwrap();
    std::fs::write(
        temp.child("Inference.toml").path(),
        "[package]\nname = \"existing\"",
    )
    .expect("Failed to create existing manifest");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("init").arg("newproject");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

/// Verifies that `infs init` validates custom project names.
///
/// **Expected behavior**: Exit with non-zero code for reserved keywords.
#[test]
fn init_validates_custom_name() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("init").arg("fn");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

/// Verifies that `infs init` uses the directory name as default project name.
///
/// **Test setup**: Creates a directory with a specific name and runs init without arguments.
///
/// **Expected behavior**: The manifest contains the directory name as project name.
#[test]
fn init_uses_directory_name_as_default() {
    let temp = assert_fs::TempDir::new().unwrap();
    let project_dir = temp.child("my_default_name_project");
    std::fs::create_dir_all(project_dir.path()).expect("Failed to create project directory");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(project_dir.path()).arg("init");

    cmd.assert().success();

    let manifest_content = std::fs::read_to_string(project_dir.child("Inference.toml").path())
        .expect("Failed to read Inference.toml");

    assert!(
        manifest_content.contains("name = \"my_default_name_project\""),
        "Manifest should contain the directory name as project name"
    );
}

// File Permission and Error Handling Tests

/// Verifies that file permissions are handled correctly for created project files.
///
/// **Test setup**: Creates a project with git enabled and checks file permissions.
///
/// **Expected behavior**: All generated files should be readable.
#[test]
fn new_creates_files_with_correct_permissions() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("permission_test_project");
    // Note: not using --no-git so .gitignore will be created

    cmd.assert().success();

    let project_dir = temp.child("permission_test_project");

    // Verify all files are readable
    let manifest = project_dir.child("Inference.toml");
    assert!(
        std::fs::read_to_string(manifest.path()).is_ok(),
        "Inference.toml should be readable"
    );

    let main_inf = project_dir.child("src").child("main.inf");
    assert!(
        std::fs::read_to_string(main_inf.path()).is_ok(),
        "src/main.inf should be readable"
    );

    let gitignore = project_dir.child(".gitignore");
    assert!(
        std::fs::read_to_string(gitignore.path()).is_ok(),
        ".gitignore should be readable"
    );
}

/// Verifies that `infs new` handles permission denied errors gracefully.
///
/// **Test setup**: On Unix, creates a read-only directory where project creation should fail.
/// Uses an explicit path argument to create the project in the read-only directory.
///
/// **Expected behavior**: Exit with non-zero code and display a meaningful error message.
#[test]
#[cfg(unix)]
fn new_handles_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    let readonly_dir = temp.child("readonly_parent");
    std::fs::create_dir_all(readonly_dir.path()).expect("Failed to create directory");

    // Make the directory read-only (no write permission)
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to set permissions");

    // Run from temp directory but try to create project in the read-only subdirectory
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .arg("new")
        .arg("should_fail_project")
        .arg(readonly_dir.path())
        .arg("--no-git");

    cmd.assert().failure().stderr(
        predicate::str::contains("Failed")
            .or(predicate::str::contains("Permission denied"))
            .or(predicate::str::contains("permission")),
    );

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to restore permissions");
}

/// Verifies that `infs init` handles permission denied errors gracefully.
///
/// **Test setup**: On Unix, we test that init properly reports errors when it cannot
/// write files. We do this by making the target directory read-only after creation.
///
/// **Expected behavior**: Exit with non-zero code and display a meaningful error message.
#[test]
#[cfg(unix)]
fn init_handles_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let temp = assert_fs::TempDir::new().unwrap();
    let work_dir = temp.child("work_dir");
    std::fs::create_dir_all(work_dir.path()).expect("Failed to create directory");

    // Create a read-only subdirectory that we'll try to init
    let readonly_dir = work_dir.child("readonly_init_dir");
    std::fs::create_dir_all(readonly_dir.path()).expect("Failed to create directory");

    // Make the directory read-only (no write permission) - execute bit needed to cd into it
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to set permissions");

    // Run from work_dir but use -C or pass path to init in the readonly dir
    // Note: infs init takes name as positional arg, not path, so we need to cd into it
    // The issue is that cd into a read-only dir works, but writing fails
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(readonly_dir.path())
        .arg("init")
        .arg("should_fail");

    cmd.assert().failure().stderr(
        predicate::str::contains("Failed")
            .or(predicate::str::contains("Permission denied"))
            .or(predicate::str::contains("permission")),
    );

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(readonly_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(readonly_dir.path(), perms).expect("Failed to restore permissions");
}

// Run Command Tests

/// Verifies that `infs run --help` displays the available options.
///
/// **Expected behavior**: Exit with code 0 and show path argument and usage.
#[test]
fn run_help_shows_options() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PATH").or(predicate::str::contains("path")))
        .stdout(predicate::str::contains("Run").or(predicate::str::contains("run")));
}

/// Verifies that `infs run` with no path enters project mode.
///
/// Before the path became optional, a missing path was a clap "PATH required"
/// usage error. Now an absent path selects project mode, so it must reach the
/// runtime pipeline instead of being rejected by the argument parser. From a
/// directory with no `Inference.toml`, the project pipeline fails — either at
/// the fail-fast wasmtime check (`wasmtime not found`) or at manifest discovery
/// (`Inference.toml`), depending on whether wasmtime is installed. The
/// regression guard is that it is *not* a clap usage error.
#[test]
fn run_without_path_enters_project_mode() {
    let temp = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("run");

    cmd.assert().failure().stderr(
        // Reached the runtime pipeline (one of these two), not clap's parser.
        predicate::str::contains("Inference.toml")
            .or(predicate::str::contains("wasmtime not found"))
            // And explicitly NOT a clap "required argument" usage error.
            .and(predicate::str::contains("required arguments").not()),
    );
}

/// Verifies that `infs run` fails when source file doesn't exist.
///
/// **Expected behavior**: Exit with non-zero code and print "Path not found".
#[test]
fn run_fails_when_file_missing() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run").arg("this-file-does-not-exist.inf");

    cmd.assert().failure().stderr(
        predicate::str::contains("Path not found").or(predicate::str::contains("path not found")),
    );
}

/// Verifies that `infs run` shows a helpful error when wasmtime is not available.
///
/// **Test setup**: Uses PATH override to ensure wasmtime is not found.
///
/// **Expected behavior**: Exit with non-zero code and display installation instructions.
#[test]
fn run_shows_wasmtime_not_found_message() {
    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path())
        .env("PATH", path_without_tools())
        .arg("run")
        .arg(dest.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("wasmtime not found"))
        .stderr(
            predicate::str::contains("wasmtime.dev")
                .or(predicate::str::contains("brew install wasmtime")),
        );
}

/// Verifies that `infs run` accepts trailing arguments for the WASM program.
///
/// **Expected behavior**: The help shows that arguments can be passed to the WASM program.
#[test]
fn run_accepts_trailing_args() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("run").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ARGS").or(predicate::str::contains("args")));
}

// Conditional Tests: Full Workflow (Require External Tools)

/// Helper function to check if wasmtime is available in PATH.
fn is_wasmtime_available() -> bool {
    std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Gate for the tests that execute compiled WASM: true when wasmtime is usable.
///
/// Mirrors [`require_infc`]. A local run without wasmtime skips with a notice,
/// while CI aborts: every workflow leg installs wasmtime unconditionally before
/// invoking this suite, so a silent skip there would report unexecuted tests as
/// green.
///
/// # Panics
///
/// When wasmtime is missing during a CI run.
fn require_wasmtime() -> bool {
    if is_wasmtime_available() {
        return true;
    }

    assert!(
        !is_ci(),
        "wasmtime not found on PATH; a CI run must not silently skip the tests that \
         execute compiled WASM. Every workflow leg installs wasmtime before running \
         this suite, so its absence means the installation step failed \
         (unset CI or set CI=0 to restore the local skip)."
    );
    eprintln!("Skipping test: wasmtime not available");
    false
}

/// Verifies full `infs run` workflow with wasmtime.
///
/// **Prerequisites**: wasmtime must be installed and in PATH, and infc must be built.
///
/// **Test setup**: Compiles a trivial Inference program and runs it.
///
/// **Expected behavior**: Program compiles, runs with wasmtime, exits successfully.
#[test]
fn run_full_workflow_with_wasmtime() {
    if !require_wasmtime() {
        return;
    }

    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("run")
        .arg(dest.path())
        .arg("--entry-point")
        .arg("hello_world");

    cmd.assert().success();
}

/// Returns the path to the syntax_`error.inf` test file.
fn syntax_error_file() -> std::path::PathBuf {
    fixture_file("syntax_error.inf")
}

/// **Expected behavior**: Exit with non-zero code, meaningful error message, NO PANIC.
#[test]
fn run_fails_gracefully_on_syntax_error() {
    let syntax_error_file = syntax_error_file();

    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("run")
        .arg(&syntax_error_file);

    cmd.assert().failure().stderr(
        predicate::str::contains("error")
            .or(predicate::str::contains("Error"))
            .or(predicate::str::contains("Syntax")),
    );
}

/// **Expected behavior**: Exit with non-zero code, meaningful error message, NO PANIC.
#[test]
fn build_fails_gracefully_on_syntax_error() {
    let syntax_error_file = syntax_error_file();

    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("build")
        .arg(&syntax_error_file);

    cmd.assert().failure().stderr(
        predicate::str::contains("error")
            .or(predicate::str::contains("Error"))
            .or(predicate::str::contains("Syntax")),
    );
}

// Helper Functions for QA Test Files

/// Returns the path to `empty.inf` test file.
fn empty_file() -> std::path::PathBuf {
    example_file("empty.inf")
}

/// Returns the path to `uzumaki.inf` test file.
#[allow(dead_code)]
fn uzumaki_file() -> std::path::PathBuf {
    example_file("uzumaki.inf")
}

/// Returns the path to `forall_test.inf` test file.
#[allow(dead_code)]
fn forall_test_file() -> std::path::PathBuf {
    example_file("forall_test.inf")
}

/// Returns the path to `exists_test.inf` test file.
#[allow(dead_code)]
fn exists_test_file() -> std::path::PathBuf {
    example_file("exists_test.inf")
}

/// Returns the path to `assume_test.inf` test file.
#[allow(dead_code)]
fn assume_test_file() -> std::path::PathBuf {
    example_file("assume_test.inf")
}

/// Returns the path to `unique_test.inf` test file.
#[allow(dead_code)]
fn unique_test_file() -> std::path::PathBuf {
    example_file("unique_test.inf")
}

// QA Test Coverage: Migrated from docs/qa-test-suite.md

/// QA: TC-2.10 - Verify empty file is handled gracefully.
///
/// **Expected behavior**: Exit with code 0 or specific empty-file error, no crash/panic.
#[test]
fn build_handles_empty_file() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .arg("build")
        .arg(empty_file());

    // Empty file should either succeed or fail gracefully (no panic)
    let output = cmd.output().expect("Failed to execute command");

    // Check that stderr doesn't contain panic messages
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panic") && !stderr.contains("RUST_BACKTRACE"),
        "Empty file should not cause panic. Got: {stderr}"
    );
}

/// QA: TC-5.9c - Verify `infs init` does not overwrite existing .gitignore.
///
/// **Expected behavior**: Exit with code 0, .gitignore contains original content.
#[test]
fn init_preserves_existing_gitignore() {
    let temp = assert_fs::TempDir::new().unwrap();

    // Create .git directory to trigger git file creation
    std::fs::create_dir_all(temp.child(".git").path()).unwrap();

    // Create custom .gitignore with specific content
    let gitignore = temp.child(".gitignore");
    std::fs::write(gitignore.path(), "custom-ignore-pattern\n").unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.current_dir(temp.path()).arg("init").arg("test_project");

    cmd.assert().success();

    // Verify .gitignore still contains original content
    let content = std::fs::read_to_string(gitignore.path()).expect("Failed to read .gitignore");

    assert!(
        content.contains("custom-ignore-pattern"),
        ".gitignore should preserve existing content. Got: {content}"
    );
}

/// QA: TC-10.5 - Verify graceful handling of invalid `INFC_PATH`.
///
/// **Expected behavior**: Exit with non-zero code, clear error message.
#[test]
fn build_with_invalid_infc_path_shows_error() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", "/nonexistent/path/to/infc")
        .arg("build")
        .arg(example_file("example.inf"));

    cmd.assert().failure().stderr(
        predicate::str::contains("not found")
            .or(predicate::str::contains("No such file"))
            .or(predicate::str::contains("does not exist"))
            .or(predicate::str::contains("compiler not found")),
    );
}

/// QA: TC-12.3 - Verify recovery from corrupted toolchain metadata.
///
/// **Expected behavior**: No crash, warning about corrupted metadata.
#[test]
fn list_handles_corrupted_metadata() {
    let temp = assert_fs::TempDir::new().unwrap();

    // Create toolchain directory structure with corrupted metadata
    let toolchain_dir = temp.child("toolchains").child("0.1.0");
    std::fs::create_dir_all(toolchain_dir.path()).unwrap();

    // Create a corrupted .metadata.json file
    std::fs::write(
        toolchain_dir.child(".metadata.json").path(),
        "{ invalid json content",
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", temp.path()).arg("list");

    // Should not panic, may show warning or skip corrupted entry
    let output = cmd.output().expect("Failed to execute command");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("panic") && !stderr.contains("RUST_BACKTRACE"),
        "Corrupted metadata should not cause panic. Got: {stderr}"
    );
}

/// QA: TC-13.1 - Verify a project whose non-deterministic constructs (including
/// the uzumaki operator `@`) live in a `spec` builds in compile mode: the
/// proof-only spec is stripped and the executable `main` is compiled.
///
/// **Expected behavior**: Exit code 0, WASM binary generated.
#[test]
fn build_compiles_uzumaki_operator() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("nondet.inf");
    let dest = temp.child("nondet.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let wasm_output = temp.child("out").child("nondet.wasm");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
}

/// QA: TC-13.6 - Verify a `-v` build of the TRANSLATABLE non-deterministic
/// feature set (a `forall` spec function with the uzumaki `@`, an `assume`
/// filter, and a nested `exists` block).
///
/// **Expected behavior**: Exit code 0, both WASM and Rocq outputs generated —
/// the spec lowers to the custom WASM opcodes and its `hassert` obligation is
/// emitted into the `.v` (the spec function is omitted from the module record).
#[test]
fn build_compiles_translatable_nondet_features() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("nondet.inf");
    let dest = temp.child("nondet.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("-v");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"))
        .stdout(predicate::str::contains("V generated"));

    let wasm_output = temp.child("out").child("nondet.wasm");
    let v_output = temp.child("out").child("nondet.v");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
    assert!(
        v_output.path().exists(),
        "Expected V file at: {:?}",
        v_output.path()
    );
}

/// QA: TC-13.7 - Verify a `-v` (proof-mode) build of a spec containing a
/// `unique` block is rejected. `unique` has no `hassert` encoding, so codegen
/// aborts with a fatal `P002` diagnostic. Pins that `infs` propagates a fatal
/// hassert diagnostic from `infc` to stderr with a non-zero exit, and that no
/// partial `.v` or `.wasm` artifact is left behind.
///
/// **Expected behavior**: Non-zero exit, stderr names the `P002` `unique`
/// rejection, and neither a `.v` nor a `.wasm` is written.
#[test]
fn build_v_rejects_unique_block_with_p002() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("nondet_unique.inf");
    let dest = temp.child("nondet_unique.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path())
        .arg("-v");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("P002").and(predicate::str::contains("unique")));

    // A fatal codegen error must leave no partial artifact behind.
    let v_output = temp.child("out").child("nondet_unique.v");
    let wasm_output = temp.child("out").child("nondet_unique.wasm");
    assert!(
        !v_output.path().exists(),
        "a rejected proof-mode build must write no V file, found: {:?}",
        v_output.path()
    );
    assert!(
        !wasm_output.path().exists(),
        "a rejected proof-mode build must write no WASM file, found: {:?}",
        wasm_output.path()
    );
}

/// QA: TC-13.8 - Verify the same `unique`-only fixture compiles in compile mode
/// (no `-v`): the proof-only spec is stripped and the executable `main` is
/// compiled, so the mode boundary — rejected under `-v`, accepted without it —
/// is pinned.
///
/// **Expected behavior**: Exit code 0, WASM binary generated.
#[test]
fn build_compiles_unique_block_without_v() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("nondet_unique.inf");
    let dest = temp.child("nondet_unique.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("WASM generated"));

    let wasm_output = temp.child("out").child("nondet_unique.wasm");
    assert!(
        wasm_output.path().exists(),
        "Expected WASM file at: {:?}",
        wasm_output.path()
    );
}

/// QA: TC-1.6 - Verify graceful error on unknown subcommand.
///
/// **Expected behavior**: Exit code non-zero, error message indicates unknown subcommand.
#[test]
fn unknown_subcommand_shows_error() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.arg("unknown-command");

    cmd.assert().failure().stderr(
        predicate::str::contains("unrecognized")
            .or(predicate::str::contains("not found"))
            .or(predicate::str::contains("unknown")),
    );
}

/// QA: TC-1.9 - Verify --version flag and version subcommand produce consistent output.
///
/// **Expected behavior**: Both commands exit with code 0, both show same version.
#[test]
fn version_flag_and_subcommand_are_consistent() {
    let mut cmd_flag = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_flag.arg("--version");

    let mut cmd_subcmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd_subcmd.arg("version");

    let flag_output = cmd_flag.output().expect("Failed to run --version");
    let subcmd_output = cmd_subcmd.output().expect("Failed to run version");

    assert!(flag_output.status.success(), "--version should succeed");
    assert!(
        subcmd_output.status.success(),
        "version subcommand should succeed"
    );

    let flag_stdout = String::from_utf8_lossy(&flag_output.stdout);
    let subcmd_stdout = String::from_utf8_lossy(&subcmd_output.stdout);

    // Both should contain the version number
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        flag_stdout.contains(version),
        "--version should contain {version}"
    );
    assert!(
        subcmd_stdout.contains(version),
        "version subcommand should contain {version}"
    );
}

/// QA: TC-12.1 - Verify error when output directory not writable.
///
/// **Expected behavior**: Exit code non-zero, error indicates permission denied.
#[test]
#[cfg(unix)]
fn build_fails_on_readonly_output_directory() {
    use std::os::unix::fs::PermissionsExt;

    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    let src = codegen_test_file("trivial.inf");
    let dest = temp.child("trivial.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    // Create read-only output directory
    let out_dir = temp.child("out");
    std::fs::create_dir_all(out_dir.path()).unwrap();
    let mut perms = std::fs::metadata(out_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(out_dir.path(), perms).expect("Failed to set permissions");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path());

    cmd.assert().failure().stderr(
        predicate::str::contains("permission")
            .or(predicate::str::contains("Permission"))
            .or(predicate::str::contains("denied"))
            .or(predicate::str::contains("Failed")),
    );

    // Restore permissions for cleanup
    let mut perms = std::fs::metadata(out_dir.path())
        .expect("Failed to get metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(out_dir.path(), perms).expect("Failed to restore permissions");
}

/// QA: TC-11.4 - Verify correct path handling with subdirectories.
///
/// **Expected behavior**: Path resolved correctly, build succeeds.
#[test]
fn build_handles_nested_paths() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();

    // Create nested directory structure
    let nested_dir = temp.child("subdir").child("nested");
    std::fs::create_dir_all(nested_dir.path()).unwrap();

    let src = codegen_test_file("trivial.inf");
    let dest = nested_dir.child("test.inf");
    std::fs::copy(&src, dest.path()).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build")
        .arg(dest.path());

    cmd.assert().success();
}

// Project-mode wasm-opt Post-Build Optimization Tests
//
// These exercise the optional `[build.wasm-opt]` post-build step end-to-end
// through the `infs` binary. Almost all are hermetic: a dependency-free fake
// `wasm-opt` (tests/fixtures/fake_wasm_opt.rs) is compiled once per test process
// and injected per-child via `WASM_OPT_PATH`, so they run on machines without
// Binaryen installed and never mutate global process state (no `serial_test`
// needed). The handful of real-binary end-to-end tests skip-gate on
// `require_wasm_opt`.

/// The invocation marker the fake `wasm-opt` writes before each logged call.
/// Kept in sync with `tests/fixtures/fake_wasm_opt.rs`.
const WASM_OPT_INVOCATION_MARKER: &str = "--- wasm-opt invocation ---";

/// `src/main.inf` that leaks a verification-only construct (a `forall` block
/// with an uzumaki `@`) into an ordinary function. Analysis rule A042 rejects a
/// non-deterministic block outside a `spec`, so the build fails before codegen —
/// wasm-opt is never reached, which is the guarantee the optimization step relies
/// on. (The post-build `0xfc` scan itself is unit-tested in `commands::wasm_opt`.)
const PROJECT_MAIN_NONDET_SRC: &str =
    "pub fn main() -> i32 {\n    forall {\n        let x: i32 = @;\n    }\n    return 0;\n}\n";

/// Compiles the dependency-free fake `wasm-opt` fixture once per test process
/// and returns the path to the built binary.
///
/// The binary stands in for a real Binaryen `wasm-opt`, letting the hermetic
/// optimization tests run anywhere `rustc` is available (which is everywhere
/// `cargo test` runs). The temp directory holding it is kept alive for the life
/// of the process via `OnceLock`, so the returned path stays valid across every
/// test. The `.exe` suffix is applied on Windows so the produced binary is
/// spawnable.
fn fake_wasm_opt_binary() -> std::path::PathBuf {
    static FAKE: std::sync::OnceLock<(assert_fs::TempDir, std::path::PathBuf)> =
        std::sync::OnceLock::new();
    FAKE.get_or_init(|| {
        let dir = assert_fs::TempDir::new().unwrap();
        let source = fixture_file("fake_wasm_opt.rs");
        let binary = dir
            .path()
            .join(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX));
        let status = Command::new("rustc")
            .arg("--edition")
            .arg("2024")
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .status()
            .expect("failed to spawn rustc to build the fake wasm-opt fixture");
        assert!(
            status.success(),
            "rustc failed to compile the fake wasm-opt fixture"
        );
        (dir, binary)
    })
    .1
    .clone()
}

/// Skip-gate for the real-binary end-to-end tests: returns `true` only when a
/// genuine Binaryen `wasm-opt` is on PATH.
///
/// Unlike [`require_infc`] and [`require_wasmtime`], this gate stays a soft
/// skip even under CI: no workflow installs Binaryen, so aborting here would
/// fail every run. The fake-binary tests cover the invocation contract; these
/// only add real-optimizer confirmation where Binaryen happens to be present.
fn require_wasm_opt() -> bool {
    if which::which("wasm-opt").is_ok() {
        true
    } else {
        eprintln!("Skipping test: wasm-opt (Binaryen) not found in PATH");
        false
    }
}

/// Parses the fake `wasm-opt` log into one argument vector per recorded
/// invocation. A missing log file — the optimizer was never spawned — yields an
/// empty list. The version probe is not logged, so a single optimization run
/// produces exactly one entry.
fn optimizer_invocations(log_path: &std::path::Path) -> Vec<Vec<String>> {
    let Ok(contents) = std::fs::read_to_string(log_path) else {
        return Vec::new();
    };
    contents
        .split(WASM_OPT_INVOCATION_MARKER)
        .map(str::trim)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| chunk.lines().map(str::to_string).collect())
        .collect()
}

/// Validates `bytes` against the same feature envelope the linker and the
/// optimizer's re-validation enforce (`GC_TYPES | MUTABLE_GLOBAL |
/// BULK_MEMORY`), using the workspace `inf-wasmparser` fork.
fn wasm_is_valid(bytes: &[u8]) -> bool {
    let features = inf_wasmparser::WasmFeatures::GC_TYPES
        .union(inf_wasmparser::WasmFeatures::MUTABLE_GLOBAL)
        .union(inf_wasmparser::WasmFeatures::BULK_MEMORY);
    inf_wasmparser::Validator::new_with_features(features)
        .validate_all(bytes)
        .is_ok()
}

/// The file name of `raw`, as a `&str`, for asserting an argument path points at
/// the expected artifact regardless of the (absolute, platform-specific) parent.
fn file_name_of(raw: &str) -> Option<&str> {
    std::path::Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
}

/// With no `[build.wasm-opt]` table the optimizer is never spawned even when
/// `WASM_OPT_PATH` is set — the default pipeline is untouched — and the ordinary
/// `out/main.wasm` is still produced. Pins the default-off neutrality.
#[test]
fn wasm_opt_absent_table_never_invokes_optimizer() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project(&temp, "demo", PROJECT_MAIN_SRC);
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().success();

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "absent [build.wasm-opt] must never spawn wasm-opt"
    );
    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "the ordinary artifact must still be produced"
    );
}

/// An enabled `[build.wasm-opt] level = "z"` forwards exactly the expected
/// argument vector — the level flag, the two feature flags, the input, then
/// `-o` and the sibling temp target, in order — leaves the artifact in place,
/// and prints the one-line size summary.
#[test]
fn wasm_opt_enabled_forwards_exact_args_and_prints_summary() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("wasm-opt -Oz: main.wasm"))
        .stdout(predicate::str::contains(" -> "))
        .stdout(predicate::str::contains("bytes"));

    let invocations = optimizer_invocations(log.path());
    assert_eq!(
        invocations.len(),
        1,
        "wasm-opt must be spawned exactly once for optimization, got: {invocations:?}"
    );
    let args = &invocations[0];
    assert_eq!(args.len(), 6, "unexpected argument count: {args:?}");
    assert_eq!(args[0], "-Oz");
    assert_eq!(args[1], "--mvp-features");
    assert_eq!(args[2], "--enable-mutable-globals");
    assert_eq!(
        file_name_of(&args[3]),
        Some("main.wasm"),
        "input must be out/main.wasm, got: {}",
        args[3]
    );
    assert_eq!(args[4], "-o");
    assert_eq!(
        file_name_of(&args[5]),
        Some("main.wasm.opt"),
        "output must be the sibling temp file, got: {}",
        args[5]
    );

    assert!(
        temp.child("out").child("main.wasm").path().exists(),
        "the optimized artifact must remain at out/main.wasm"
    );
}

/// `[build.wasm-opt] enabled = false` keeps the optimizer off even though the
/// table is present.
#[test]
fn wasm_opt_enabled_false_skips_optimizer() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nenabled = false\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().success();

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "`enabled = false` must not spawn wasm-opt"
    );
    assert!(temp.child("out").child("main.wasm").path().exists());
}

/// `infs build --no-wasm-opt` suppresses the step even when `[build.wasm-opt]`
/// is enabled, leaving the artifact exactly as infc emitted it.
#[test]
fn wasm_opt_no_wasm_opt_flag_skips_optimizer_on_build() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build")
        .arg("--no-wasm-opt");

    cmd.assert().success();

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "--no-wasm-opt must not spawn wasm-opt"
    );
    assert!(temp.child("out").child("main.wasm").path().exists());
}

/// A proof-mode manifest skips optimization silently: proof artifacts are a
/// different class (they carry the non-det opcodes wasm-opt cannot process), so
/// the optimizer is never spawned and the build still succeeds.
#[test]
fn wasm_opt_skipped_for_manifest_proof_mode() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build]\nmode = \"proof\"\n\n[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().success();

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "a proof-mode manifest must skip wasm-opt"
    );
}

/// `infs build --mode proof` on a compile-mode manifest that enables the
/// optimizer skips optimization: the explicitly-owned proof signal wins and the
/// artifact is left unoptimized.
#[test]
fn wasm_opt_skipped_for_cli_proof_mode() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build")
        .arg("--mode")
        .arg("proof");

    cmd.assert().success();

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "`--mode proof` must skip wasm-opt"
    );
}

/// A plain `-v` build is treated conservatively as a verification workflow and
/// skips optimization, even on a compile-mode manifest that enables it. Both
/// `out/main.wasm` and `out/main.v` are still produced.
#[test]
fn wasm_opt_skipped_for_v_flag() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build")
        .arg("-v");

    cmd.assert().success();

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "a -v build must skip wasm-opt"
    );
    assert!(
        temp.child("out").child("main.wasm").path().exists()
            && temp.child("out").child("main.v").path().exists(),
        "-v must still write both out/main.wasm and out/main.v"
    );
}

/// A verification construct that leaks into an ordinary function is a hard error:
/// analysis rule A042 rejects the non-deterministic `forall` block outside a
/// `spec` before codegen runs, so the build fails naming the construct and
/// pointing at `spec`, no artifact is written, and the optimizer is never
/// spawned. This preserves the optimization step's precondition (a leaked
/// verification construct never reaches wasm-opt) at the earliest possible gate.
#[test]
fn wasm_opt_rejects_leaked_verification_construct() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_NONDET_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("'forall' block").and(predicate::str::contains("spec")));

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "analysis must reject before wasm-opt is spawned"
    );
    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "no artifact is written when analysis rejects the program before codegen"
    );
}

/// A non-deterministic block in an *imported* module file (not just the entry
/// file) is rejected end-to-end through project-mode `infs build`: analysis rule
/// A042 fires on the `forall` in `src/lib.inf`, the rendered diagnostic names the
/// offending module by its file label (`lib`), and no artifact is written. This
/// pins that the whole import closure — not only `src/main.inf` — is held to the
/// rule.
#[test]
fn build_rejects_nondet_in_imported_module_file() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        "use lib;\n\npub fn main() -> i32 {\n    return lib::helper();\n}\n",
        "",
    );
    temp.child("src")
        .child("lib.inf")
        .write_str(
            "pub fn helper() -> i32 {\n    forall {\n        let x: i32 = 1;\n    }\n    return 7;\n}\n",
        )
        .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().failure().stderr(
        predicate::str::contains("A042")
            .and(predicate::str::contains("'forall' block"))
            .and(predicate::str::contains("lib")),
    );

    assert!(
        !temp.child("out").child("main.wasm").path().exists(),
        "no artifact must be written when analysis rejects the build"
    );
}

/// A `WASM_OPT_PATH` that does not name a file is a hard error naming the
/// environment variable — an explicit override is never silently discarded in
/// favor of a PATH search.
#[test]
fn wasm_opt_missing_binary_via_env_names_env_var() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let missing = temp.child("no-such-wasm-opt");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", missing.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("WASM_OPT_PATH"));
}

/// With no `WASM_OPT_PATH` override, a PATH containing no `wasm-opt`, and no
/// managed install, the missing-binary error leads with the infs-managed option
/// (`infs component add wasm-opt`) and still carries the Binaryen package hints.
/// `INFERENCE_HOME` is isolated to an empty directory so a managed `wasm-opt` on
/// the developer's real machine cannot resolve and mask the error. Unix-only: an
/// empty PATH is the portable way to guarantee the lookup fails, and infc is
/// still reached through `INFC_PATH`.
#[cfg(unix)]
#[test]
fn wasm_opt_missing_binary_via_path_hints_install() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let home = assert_fs::TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("INFERENCE_HOME", home.path())
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().failure().stderr(
        predicate::str::contains("infs component add wasm-opt")
            .and(predicate::str::contains("brew install binaryen")),
    );
}

/// A nonzero `wasm-opt` exit is a hard error carrying its stderr, and the
/// original artifact is left intact — the optimizer writes to a sibling temp
/// file that is cleaned up on failure, so the in-place artifact never sees a
/// partial result.
#[test]
fn wasm_opt_nonzero_exit_fails_and_preserves_artifact() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_EXIT", "1")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("fake failure"));

    let artifact = temp.child("out").child("main.wasm");
    let bytes = std::fs::read(artifact.path()).expect("original artifact must remain readable");
    assert!(
        !bytes.is_empty() && wasm_is_valid(&bytes),
        "the unoptimized artifact must be left valid after a wasm-opt failure"
    );
    assert!(
        !temp.child("out").child("main.wasm.opt").path().exists(),
        "the temp file must be cleaned up after a failure"
    );
}

/// A `wasm-opt` older than the supported minimum is a hard error naming both the
/// found version and the required minimum.
#[test]
fn wasm_opt_version_too_old_errors() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_VERSION", "wasm-opt version 90 (version_90)")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("90").and(predicate::str::contains("116")));
}

/// An unparseable `--version` banner is a warning, not a blocker: the build
/// proceeds to optimization and succeeds. A possibly-fine binary must never be
/// rejected over an unrecognized version string.
#[test]
fn wasm_opt_unparseable_version_warns_and_succeeds() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_VERSION", "banana")
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().success().stderr(predicate::str::contains(
        "could not parse a wasm-opt version",
    ));

    assert_eq!(
        optimizer_invocations(log.path()).len(),
        1,
        "the build must proceed to optimization despite the unparseable version"
    );
}

/// Optimized bytes that fail re-validation are a hard error; the original
/// `out/main.wasm` is left valid and the temp file is cleaned up. This guards
/// against a `wasm-opt` that emits something outside the executable subset.
#[test]
fn wasm_opt_garbage_output_fails_revalidation_and_preserves_artifact() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_GARBAGE", "1")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("failed re-validation"));

    let artifact = temp.child("out").child("main.wasm");
    let bytes = std::fs::read(artifact.path()).expect("original artifact must remain readable");
    assert!(
        wasm_is_valid(&bytes),
        "the original artifact must stay valid when the optimized output is rejected"
    );
    assert!(
        !temp.child("out").child("main.wasm.opt").path().exists(),
        "the temp file must be cleaned up after a re-validation failure"
    );
}

/// Project `run` applies the same optimization `build` does — it runs exactly
/// what it ships — so an enabled table spawns the optimizer, and `main`'s return
/// value (42) still surfaces. Gated on wasmtime.
#[test]
fn wasm_opt_run_enabled_invokes_optimizer_and_runs() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_NONZERO_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("42"));

    assert_eq!(
        optimizer_invocations(log.path()).len(),
        1,
        "project run must apply the optimization it ships"
    );
}

/// `infs run --no-wasm-opt` executes the artifact exactly as infc emitted it:
/// the optimizer is skipped and `main` still runs. Gated on wasmtime.
#[test]
fn wasm_opt_run_no_wasm_opt_flag_skips_optimizer() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_NONZERO_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("WASM_OPT_PATH", fake_wasm_opt_binary())
        .env("FAKE_WASM_OPT_LOG", log.path())
        .current_dir(temp.path())
        .arg("run")
        .arg("--no-wasm-opt");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("42"));

    assert!(
        optimizer_invocations(log.path()).is_empty(),
        "--no-wasm-opt must skip optimization on run too"
    );
}

/// Real-binary end-to-end: an optimized artifact validates under the workspace
/// feature envelope and is no larger than the unoptimized one built from the
/// same source (`wasm-opt -Oz` at minimum drops the names section). Gated on a
/// real Binaryen `wasm-opt`.
#[test]
fn wasm_opt_real_binary_produces_valid_no_larger_artifact() {
    let Some(infc_path) = require_infc() else {
        return;
    };
    if !require_wasm_opt() {
        return;
    }

    let optimized = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &optimized,
        "demo",
        PROJECT_MAIN_NONZERO_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let mut opt_cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    opt_cmd
        .env("INFC_PATH", &infc_path)
        .current_dir(optimized.path())
        .arg("build");
    opt_cmd.assert().success();

    let unoptimized = assert_fs::TempDir::new().unwrap();
    scaffold_project(&unoptimized, "demo", PROJECT_MAIN_NONZERO_SRC);
    let mut plain_cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    plain_cmd
        .env("INFC_PATH", &infc_path)
        .current_dir(unoptimized.path())
        .arg("build");
    plain_cmd.assert().success();

    let optimized_bytes = std::fs::read(optimized.child("out").child("main.wasm").path()).unwrap();
    let unoptimized_bytes =
        std::fs::read(unoptimized.child("out").child("main.wasm").path()).unwrap();

    assert!(
        wasm_is_valid(&optimized_bytes),
        "the optimized artifact must validate under the workspace feature set"
    );
    assert!(
        optimized_bytes.len() <= unoptimized_bytes.len(),
        "optimized ({}) must be no larger than unoptimized ({})",
        optimized_bytes.len(),
        unoptimized_bytes.len()
    );
}

/// Real-binary end-to-end: running an optimized project yields the same
/// observable result (`main` returns 42) as running the unoptimized one —
/// optimization preserves behavior. Gated on both a real `wasm-opt` and
/// wasmtime.
#[test]
fn wasm_opt_real_binary_run_matches_unoptimized() {
    let Some(infc_path) = require_infc_and_wasmtime() else {
        return;
    };
    if !require_wasm_opt() {
        return;
    }

    let optimized = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &optimized,
        "demo",
        PROJECT_MAIN_NONZERO_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let mut opt_run = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    opt_run
        .env("INFC_PATH", &infc_path)
        .current_dir(optimized.path())
        .arg("run");
    opt_run
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));

    let unoptimized = assert_fs::TempDir::new().unwrap();
    scaffold_project(&unoptimized, "demo", PROJECT_MAIN_NONZERO_SRC);
    let mut plain_run = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    plain_run
        .env("INFC_PATH", &infc_path)
        .current_dir(unoptimized.path())
        .arg("run");
    plain_run
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

// `infs component` Tests
//
// These exercise the managed-component install path end to end without touching
// the network: a `TcpListener` stub serves an in-test Binaryen tarball, and each
// test isolates `INFERENCE_HOME` so nothing leaks into the real toolchain. The
// tarball's `bin/wasm-opt` payload reuses the compiled `fake_wasm_opt_binary()`
// fixture (a real, spawnable binary), so an install is byte-for-byte realistic.

/// The pinned Binaryen version, mirrored from `toolchain::binaryen::BINARYEN_PIN`
/// (an integration test cannot import the `infs` binary crate).
const BINARYEN_PIN: &str = "version_130";

/// Spawns a throwaway HTTP/1.1 server on `127.0.0.1:0` that answers every request
/// with `body`, and returns its base URL (`http://127.0.0.1:<port>`).
///
/// The download client (`reqwest`) issues a single `GET`; the stub drains the
/// request headers before responding so the client never sees a reset, then
/// closes the connection (`Connection: close`). The accept loop lives in a
/// detached thread for the life of the test process.
fn spawn_binaryen_stub(body: Vec<u8>) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub listener");
    let base = format!("http://{}", listener.local_addr().expect("stub local addr"));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while let Ok(1) = stream.read(&mut byte) {
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") || request.len() > 8192 {
                    break;
                }
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(header.as_bytes()).is_err() {
                continue;
            }
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    base
}

/// Builds a gzip-compressed tar from `(archive_path, bytes)` entries, each with
/// mode `0o755`. Archive paths use `/` separators (the tar convention).
fn build_tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    let mut buf = Vec::new();
    {
        let mut builder = Builder::new(GzEncoder::new(&mut buf, Compression::default()));
        for &(path, bytes) in entries {
            let mut header = Header::new_gnu();
            header.set_size(u64::try_from(bytes.len()).expect("entry size fits in u64"));
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, path, bytes)
                .expect("append tar entry");
        }
        builder
            .into_inner()
            .expect("finish tar")
            .finish()
            .expect("finish gzip");
    }
    buf
}

/// Builds a Binaryen release tarball matching the host platform's required
/// layout: `binaryen-<pin>/bin/wasm-opt[.exe]`, plus the `lib/libbinaryen.dylib`
/// sibling the installer requires on macOS.
fn build_host_binaryen_tarball() -> Vec<u8> {
    let wasm_opt = std::fs::read(fake_wasm_opt_binary()).expect("read compiled fake wasm-opt");
    let bin_path = format!(
        "binaryen-{BINARYEN_PIN}/bin/wasm-opt{}",
        std::env::consts::EXE_SUFFIX
    );

    #[cfg(target_os = "macos")]
    let dylib_path = format!("binaryen-{BINARYEN_PIN}/lib/libbinaryen.dylib");
    #[cfg(target_os = "macos")]
    let entries: Vec<(&str, &[u8])> = vec![
        (bin_path.as_str(), wasm_opt.as_slice()),
        (dylib_path.as_str(), b"fake libbinaryen dylib"),
    ];
    #[cfg(not(target_os = "macos"))]
    let entries: Vec<(&str, &[u8])> = vec![(bin_path.as_str(), wasm_opt.as_slice())];

    build_tarball(&entries)
}

/// The lowercase-hex SHA256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut out = String::with_capacity(64);
    for b in hasher.finalize() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Whether `dir` contains an entry whose name starts with `prefix`. Returns
/// `false` when `dir` does not exist.
fn dir_has_prefixed_entry(dir: &std::path::Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().starts_with(prefix))
}

/// The install path of the managed `wasm-opt` under `home`.
fn managed_wasm_opt(home: &assert_fs::TempDir) -> assert_fs::fixture::ChildPath {
    home.child("tools")
        .child("binaryen")
        .child(BINARYEN_PIN)
        .child("bin")
        .child(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX))
}

/// Builds an `infs component add wasm-opt` command wired to `home` and the stub.
fn component_add_cmd(home: &assert_fs::TempDir, base: &str, sha: &str) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", home.path())
        .env("INFS_BINARYEN_BASE_URL", base)
        .env("INFS_BINARYEN_SHA256", sha)
        .args(["component", "add", "wasm-opt"]);
    cmd
}

/// Installs the managed component into `home` via a fresh stub, asserting success.
fn install_managed_component(home: &assert_fs::TempDir) {
    let tarball = build_host_binaryen_tarball();
    let sha = sha256_hex(&tarball);
    let base = spawn_binaryen_stub(tarball);
    component_add_cmd(home, &base, &sha).assert().success();
}

/// `component add wasm-opt` downloads from the stub, verifies the checksum,
/// installs an executable `wasm-opt`, and cleans up the archive and temp dir.
#[test]
fn component_add_downloads_verifies_and_installs() {
    let home = assert_fs::TempDir::new().unwrap();
    let tarball = build_host_binaryen_tarball();
    let sha = sha256_hex(&tarball);
    let base = spawn_binaryen_stub(tarball);

    component_add_cmd(&home, &base, &sha)
        .assert()
        .success()
        .stdout(predicate::str::contains("Installing component 'wasm-opt'"));

    let installed = managed_wasm_opt(&home);
    assert!(installed.path().exists(), "wasm-opt must be installed");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(installed.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "installed wasm-opt must be executable");
    }

    assert!(
        !dir_has_prefixed_entry(home.child("downloads").path(), "binaryen-"),
        "the downloaded archive must be cleaned up"
    );
    assert!(
        !dir_has_prefixed_entry(home.child("tools").child("binaryen").path(), ".tmp-"),
        "no per-process temp dir should remain"
    );
}

/// A second `add` with the base URL pointed at a dead port still succeeds — an
/// already-installed component short-circuits before any network access.
#[test]
fn component_add_is_idempotent_without_network() {
    let home = assert_fs::TempDir::new().unwrap();
    let tarball = build_host_binaryen_tarball();
    let sha = sha256_hex(&tarball);
    let base = spawn_binaryen_stub(tarball);

    component_add_cmd(&home, &base, &sha).assert().success();

    // Nothing listens on port 1; reaching the network here would fail.
    let mut second = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    second
        .env("INFERENCE_HOME", home.path())
        .env("INFS_BINARYEN_BASE_URL", "http://localhost:1")
        .env("INFS_BINARYEN_SHA256", &sha)
        .args(["component", "add", "wasm-opt"]);
    second
        .assert()
        .success()
        .stdout(predicate::str::contains("already installed"));
}

/// A checksum mismatch rejects the download, installs nothing, and deletes the
/// downloaded archive.
#[test]
fn component_add_rejects_checksum_mismatch_and_cleans_up() {
    let home = assert_fs::TempDir::new().unwrap();
    let tarball = build_host_binaryen_tarball();
    let base = spawn_binaryen_stub(tarball);
    let wrong_sha = "0".repeat(64);

    component_add_cmd(&home, &base, &wrong_sha)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Checksum verification failed"));

    assert!(
        !home
            .child("tools")
            .child("binaryen")
            .child(BINARYEN_PIN)
            .path()
            .exists(),
        "a checksum mismatch must install nothing"
    );
    assert!(
        !dir_has_prefixed_entry(home.child("downloads").path(), "binaryen-"),
        "the rejected archive must be deleted"
    );
}

/// A directory present at the pinned path but missing the binary is a broken
/// install; `add` repairs it.
#[test]
fn component_add_repairs_broken_install() {
    let home = assert_fs::TempDir::new().unwrap();
    // Pre-seed a broken install: the version dir exists, but has no binary.
    home.child("tools")
        .child("binaryen")
        .child(BINARYEN_PIN)
        .child("bin")
        .create_dir_all()
        .unwrap();

    install_managed_component(&home);

    assert!(
        managed_wasm_opt(&home).path().exists(),
        "a broken install must be repaired"
    );
}

/// An unknown component name is rejected, naming the known components. No network
/// or platform work happens first.
#[test]
fn component_add_unknown_component_errors() {
    let home = assert_fs::TempDir::new().unwrap();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", home.path())
        .args(["component", "add", "wasm-optimizer"]);
    cmd.assert().failure().stderr(
        predicate::str::contains("Unknown component 'wasm-optimizer'")
            .and(predicate::str::contains("Known components: wasm-opt")),
    );
}

/// `component remove wasm-opt` deletes an installed component.
#[test]
fn component_remove_present_deletes_install() {
    let home = assert_fs::TempDir::new().unwrap();
    install_managed_component(&home);
    let dir = home.child("tools").child("binaryen").child(BINARYEN_PIN);
    assert!(dir.path().exists(), "precondition: component installed");

    let mut rm = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    rm.env("INFERENCE_HOME", home.path())
        .args(["component", "remove", "wasm-opt"]);
    rm.assert()
        .success()
        .stdout(predicate::str::contains("Removed component 'wasm-opt'"));

    assert!(!dir.path().exists(), "remove must delete the install dir");
}

/// Removing an absent component bails.
#[test]
fn component_remove_absent_bails() {
    let home = assert_fs::TempDir::new().unwrap();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", home.path())
        .args(["component", "remove", "wasm-opt"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not installed"));
}

/// Removing an unknown component name is rejected before touching the filesystem.
#[test]
fn component_remove_unknown_component_errors() {
    let home = assert_fs::TempDir::new().unwrap();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", home.path())
        .args(["component", "remove", "binaryen"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown component 'binaryen'"));
}

/// `component list` reports the not-installed state with the add hint.
#[test]
fn component_list_reports_not_installed() {
    let home = assert_fs::TempDir::new().unwrap();
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", home.path())
        .args(["component", "list"]);
    cmd.assert().success().stdout(
        predicate::str::contains("wasm-opt")
            .and(predicate::str::contains("not installed"))
            .and(predicate::str::contains("infs component add wasm-opt")),
    );
}

/// `component list` reports the installed state with the version and path.
#[test]
fn component_list_reports_installed() {
    let home = assert_fs::TempDir::new().unwrap();
    install_managed_component(&home);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFERENCE_HOME", home.path())
        .args(["component", "list"]);
    cmd.assert().success().stdout(predicate::str::contains(
        "installed: Binaryen version_130 at",
    ));
}

/// macOS installs the `libbinaryen.dylib` sibling alongside the binary — the
/// dynamic `wasm-opt` there links against it at runtime.
#[cfg(target_os = "macos")]
#[test]
fn component_add_installs_dylib_sibling_on_macos() {
    let home = assert_fs::TempDir::new().unwrap();
    install_managed_component(&home);

    let install_root = home.child("tools").child("binaryen").child(BINARYEN_PIN);
    assert!(
        install_root.child("bin").child("wasm-opt").path().exists(),
        "the wasm-opt binary must be installed"
    );
    assert!(
        install_root
            .child("lib")
            .child("libbinaryen.dylib")
            .path()
            .exists(),
        "the libbinaryen.dylib sibling must be installed on macOS"
    );
}

/// An archive missing a required file surfaces a layout-drift error and installs
/// nothing.
#[test]
fn component_add_reports_layout_drift_on_missing_file() {
    let home = assert_fs::TempDir::new().unwrap();
    // The binary is under an unexpected name, so the required `bin/wasm-opt`
    // (required on every platform) is absent.
    let entry = format!("binaryen-{BINARYEN_PIN}/bin/not-wasm-opt");
    let tarball = build_tarball(&[(entry.as_str(), b"not the binary")]);
    let sha = sha256_hex(&tarball);
    let base = spawn_binaryen_stub(tarball);

    component_add_cmd(&home, &base, &sha)
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing the expected file"));

    assert!(
        !home
            .child("tools")
            .child("binaryen")
            .child(BINARYEN_PIN)
            .path()
            .exists(),
        "a layout-drift failure must install nothing"
    );
}

// Managed-tier resolution, auto-install, and doctor Tests
//
// These exercise the three-tier `wasm-opt` resolver (WASM_OPT_PATH -> PATH ->
// managed), the manifest `auto-install` build-time provisioning, and the
// `infs doctor` check. They are unix-gated: an empty PATH is the portable way to
// force the PATH lookup to miss, and infc is still reached via `INFC_PATH`. Each
// isolates `INFERENCE_HOME` so nothing leaks into (or resolves from) the
// developer's real toolchain.

/// Seeds a managed `wasm-opt` under `home` by copying the compiled fake into the
/// pinned Binaryen layout (`tools/binaryen/<pin>/bin/wasm-opt`) and making it
/// executable, so the resolver's managed tier finds a runnable binary without a
/// network install. Returns the installed path.
#[cfg(unix)]
fn seed_managed_wasm_opt(home: &assert_fs::TempDir) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = home
        .child("tools")
        .child("binaryen")
        .child(BINARYEN_PIN)
        .child("bin");
    bin_dir.create_dir_all().unwrap();

    let managed = bin_dir.child(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(fake_wasm_opt_binary(), managed.path()).expect("copy fake wasm-opt into managed");
    let mut perms = std::fs::metadata(managed.path()).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(managed.path(), perms).unwrap();
    managed.path().to_path_buf()
}

/// A `wasm-opt` on PATH takes precedence over a managed copy: with both present,
/// resolution reports the PATH tier (the `INFS_VERBOSE` trace names it), matching
/// the `infc` resolver's PATH-over-managed precedence.
#[cfg(unix)]
#[test]
fn wasm_opt_path_beats_managed() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let home = assert_fs::TempDir::new().unwrap();
    let managed = seed_managed_wasm_opt(&home);
    assert!(managed.exists(), "precondition: managed copy seeded");

    // The compiled fake's own directory becomes the sole PATH entry, so
    // `which::which("wasm-opt")` resolves to it.
    let path_dir = fake_wasm_opt_binary()
        .parent()
        .expect("fake wasm-opt has a parent dir")
        .to_path_buf();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("INFERENCE_HOME", home.path())
        .env("PATH", &path_dir)
        .env("INFS_VERBOSE", "1")
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("resolved wasm-opt via PATH:"));
}

/// The managed tier is the fallback when PATH has no `wasm-opt`: with an empty
/// PATH and no override, resolution uses the managed copy (the `INFS_VERBOSE`
/// trace names the tier) and the optimizer actually runs.
#[cfg(unix)]
#[test]
fn wasm_opt_managed_tier_used_when_path_empty() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nlevel = \"z\"\n",
    );
    let home = assert_fs::TempDir::new().unwrap();
    seed_managed_wasm_opt(&home);
    let log = temp.child("wasm-opt.log");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("INFERENCE_HOME", home.path())
        .env("PATH", "")
        .env("INFS_VERBOSE", "1")
        .env("FAKE_WASM_OPT_LOG", log.path())
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert().success().stderr(predicate::str::contains(
        "resolved wasm-opt via managed tools:",
    ));

    assert_eq!(
        optimizer_invocations(log.path()).len(),
        1,
        "the managed wasm-opt must run the optimization"
    );
}

/// `auto-install = true` downloads the pinned Binaryen from the stub when no
/// `wasm-opt` resolves, then optimizes with it: the console announces the
/// auto-install, the managed binary lands under `INFERENCE_HOME`, and the size
/// summary confirms the optimizer ran.
#[cfg(unix)]
#[test]
fn wasm_opt_auto_install_downloads_then_optimizes() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nauto-install = true\nlevel = \"z\"\n",
    );
    let home = assert_fs::TempDir::new().unwrap();
    let log = temp.child("wasm-opt.log");

    let tarball = build_host_binaryen_tarball();
    let sha = sha256_hex(&tarball);
    let base = spawn_binaryen_stub(tarball);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("INFERENCE_HOME", home.path())
        .env("INFS_BINARYEN_BASE_URL", &base)
        .env("INFS_BINARYEN_SHA256", &sha)
        .env("FAKE_WASM_OPT_LOG", log.path())
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "[build.wasm-opt] auto-install is enabled.",
        ))
        .stdout(predicate::str::contains("wasm-opt -Oz: main.wasm"));

    assert!(
        managed_wasm_opt(&home).path().exists(),
        "auto-install must leave the managed wasm-opt installed"
    );
    assert_eq!(
        optimizer_invocations(log.path()).len(),
        1,
        "the freshly installed wasm-opt must run the optimization"
    );
}

/// `auto-install = false` (the default) with no resolvable `wasm-opt` is a hard
/// error whose remediation leads with the managed component command *before* the
/// `brew` package hint — the managed path is the recommended first option.
#[cfg(unix)]
#[test]
fn wasm_opt_auto_install_false_errors_with_component_hint_before_brew() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nauto-install = false\nlevel = \"z\"\n",
    );
    let home = assert_fs::TempDir::new().unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .env("INFC_PATH", &infc_path)
        .env("INFERENCE_HOME", home.path())
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build")
        .output()
        .expect("infs build should run");

    assert!(
        !output.status.success(),
        "a missing wasm-opt with auto-install off must fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let component_at = stderr
        .find("infs component add wasm-opt")
        .expect("the managed component hint must be present");
    let brew_at = stderr
        .find("brew install binaryen")
        .expect("the brew hint must be present");
    assert!(
        component_at < brew_at,
        "the managed component hint must precede the brew hint; stderr:\n{stderr}"
    );
}

/// An offline `auto-install = true` build surfaces the auto-install failure with
/// remediation context rather than a bare network error: the download against a
/// dead port fails, and the error names the retry / manual-install path.
#[cfg(unix)]
#[test]
fn wasm_opt_auto_install_offline_reports_context() {
    let Some(infc_path) = require_infc() else {
        return;
    };

    let temp = assert_fs::TempDir::new().unwrap();
    scaffold_project_with_manifest(
        &temp,
        "demo",
        PROJECT_MAIN_SRC,
        "[build.wasm-opt]\nauto-install = true\nlevel = \"z\"\n",
    );
    let home = assert_fs::TempDir::new().unwrap();
    let sha = "0".repeat(64);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("infs"));
    cmd.env("INFC_PATH", &infc_path)
        .env("INFERENCE_HOME", home.path())
        .env("INFS_BINARYEN_BASE_URL", "http://localhost:1")
        .env("INFS_BINARYEN_SHA256", &sha)
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .current_dir(temp.path())
        .arg("build");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to auto-install"));
}

/// `infs doctor` reports a `wasm-opt` that resolves nowhere as an OK optional
/// line — a project that does not use `[build.wasm-opt]` is never alarmed, and
/// the line still names the component command for users who do want it.
#[cfg(unix)]
#[test]
fn doctor_reports_missing_wasm_opt_as_optional_ok() {
    let home = assert_fs::TempDir::new().unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .arg("doctor")
        .env("INFERENCE_HOME", home.path())
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .output()
        .expect("doctor should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[OK] wasm-opt: Not installed (optional"),
        "doctor must report a never-installed wasm-opt as optional OK; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("infs component add wasm-opt"),
        "the optional OK line must name the component command; stdout:\n{stdout}"
    );
}

/// `infs doctor` warns when a managed Binaryen directory exists but its binary
/// is missing (a broken/interrupted install), pointing at the component command
/// to repair it.
#[cfg(unix)]
#[test]
fn doctor_warns_on_broken_managed_wasm_opt() {
    let home = assert_fs::TempDir::new().unwrap();
    // A managed version dir without the binary — a broken/interrupted install.
    home.child("tools")
        .child("binaryen")
        .child(BINARYEN_PIN)
        .child("bin")
        .create_dir_all()
        .unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .arg("doctor")
        .env("INFERENCE_HOME", home.path())
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .output()
        .expect("doctor should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[WARN] wasm-opt:") && stdout.contains("infs component add wasm-opt"),
        "a broken managed install must warn with the repair hint; stdout:\n{stdout}"
    );
}

/// `infs doctor` reports an invalid `WASM_OPT_PATH` as a WARN naming the
/// environment variable, rather than silently ignoring the user's override.
#[cfg(unix)]
#[test]
fn doctor_warns_on_invalid_wasm_opt_path() {
    let home = assert_fs::TempDir::new().unwrap();
    let bogus = home.child("not-a-wasm-opt");

    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .arg("doctor")
        .env("INFERENCE_HOME", home.path())
        .env("WASM_OPT_PATH", bogus.path())
        .output()
        .expect("doctor should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[WARN] wasm-opt:") && stdout.contains("WASM_OPT_PATH"),
        "an invalid WASM_OPT_PATH must warn naming the env var; stdout:\n{stdout}"
    );
}

/// `infs doctor` reports a healthy managed `wasm-opt` as OK, naming the managed
/// tier and the Binaryen version its `--version` reports.
#[cfg(unix)]
#[test]
fn doctor_reports_managed_wasm_opt_as_ok() {
    let home = assert_fs::TempDir::new().unwrap();
    seed_managed_wasm_opt(&home);

    let output = Command::new(assert_cmd::cargo::cargo_bin!("infs"))
        .arg("doctor")
        .env("INFERENCE_HOME", home.path())
        .env("PATH", "")
        .env_remove("WASM_OPT_PATH")
        .output()
        .expect("doctor should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[OK] wasm-opt: Found at") && stdout.contains("source: managed tools"),
        "a healthy managed wasm-opt must be OK naming the managed tier; stdout:\n{stdout}"
    );
}

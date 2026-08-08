//! Provisioning for Binaryen's `wasm-opt`, the optimizer the `[build.wasm-opt]`
//! manifest table drives.
//!
//! `infs` installs a **pinned, sha256-verified** Binaryen release into
//! `~/.inference/tools/binaryen/<version>/`, so projects that opt into
//! optimization work out of the box and teams get version-consistent artifacts
//! (`wasm-opt` output is deterministic only per Binaryen version). This is a
//! distinct install tier from `toolchains/`: the infc-specific
//! `MANAGED_BINARY`/symlink machinery is untouched, and resolution precedence
//! lives in [`crate::commands::wasm_opt`].
//!
//! ## Platform layout
//!
//! Binaryen ships one `.tar.gz` per platform with a versioned root directory
//! (`binaryen-<version>/`). On Linux and Windows `wasm-opt` is statically
//! linked, so only `bin/wasm-opt[.exe]` is installed. On macOS it is dynamically
//! linked against `@rpath/libbinaryen.dylib` with an rpath of
//! `@loader_path/../lib`, so `lib/libbinaryen.dylib` must be installed as a
//! sibling of `bin/` or the binary will not launch. [`required_files`] encodes
//! this per platform.
//!
//! ## Bumping the pin
//!
//! 1. Change [`BINARYEN_PIN`] to the new release tag.
//! 2. Refresh the three [`pinned_sha256`] constants. Download each asset and run
//!    `shasum -a 256`, cross-checking against the release's `.sha256` sidecars.
//!    (The sidecars are a pin-time cross-check only; they are never trusted at
//!    install time.)
//! 3. Confirm `MIN_WASM_OPT_VERSION` in [`crate::commands::wasm_opt`] is still
//!    less than or equal to the new pin.
//! 4. Add a CHANGELOG entry.
//!
//! Quarantine note: `reqwest` downloads do not set the macOS
//! `com.apple.quarantine` attribute, so the extracted `wasm-opt` runs without a
//! Gatekeeper prompt.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::toolchain::archive::set_executable_file;
use crate::toolchain::{Platform, ToolchainPaths, download_file, extract_archive, verify_checksum};

/// The pinned Binaryen release tag.
pub const BINARYEN_PIN: &str = "version_130";

/// The user-facing name of the component this module provisions.
pub const COMPONENT_NAME: &str = "wasm-opt";

/// Base URL for release downloads. Overridable via [`BINARYEN_BASE_URL_ENV`].
const DEFAULT_BASE_URL: &str = "https://github.com/WebAssembly/binaryen/releases/download";

/// Environment variable overriding the download base URL, for mirrors and as the
/// hermetic test seam.
pub const BINARYEN_BASE_URL_ENV: &str = "INFS_BINARYEN_BASE_URL";

/// Environment variable overriding the expected checksum. Honored **only** when
/// [`BINARYEN_BASE_URL_ENV`] is also set — see [`sha_seam`].
const BINARYEN_SHA256_OVERRIDE_ENV: &str = "INFS_BINARYEN_SHA256";

/// Pinned checksum of `binaryen-version_130-x86_64-linux.tar.gz`.
const SHA256_LINUX_X64: &str = "0a18362361ad05465118cd8eeb72edaeec89de6894bc283576ef4e07aa3babcc";

/// Pinned checksum of `binaryen-version_130-arm64-macos.tar.gz`.
const SHA256_MACOS_ARM64: &str = "79d3ab9f417d9e215f15f598f523d001a7d9ac1e59367e5c869fbdabd1cba72e";

/// Pinned checksum of `binaryen-version_130-x86_64-windows.tar.gz`.
const SHA256_WINDOWS_X64: &str = "cc09c874f4332d00aa32ab72745a9b98c9a172f795762f21d03e70638a3f7f4c";

/// The install status of a managed component.
#[derive(Debug, Clone)]
pub struct ComponentStatus {
    /// The component's user-facing name (e.g. `wasm-opt`).
    pub name: &'static str,
    /// Whether the component is installed and usable.
    pub installed: bool,
    /// The pinned version the component would install/report.
    pub version: &'static str,
}

/// The Binaryen release-asset target triple for `platform`.
fn release_target(platform: Platform) -> &'static str {
    match platform {
        Platform::LinuxX64 => "x86_64-linux",
        Platform::MacosArm64 => "arm64-macos",
        Platform::WindowsX64 => "x86_64-windows",
    }
}

/// The release asset file name for `platform` at the pinned version.
#[must_use = "returns the asset name without side effects"]
pub fn asset_name(platform: Platform) -> String {
    format!(
        "binaryen-{BINARYEN_PIN}-{}.tar.gz",
        release_target(platform)
    )
}

/// The full download URL for `platform`, joining `base`, `version`, and the
/// asset name. A trailing slash on `base` is trimmed so the join never doubles.
#[must_use = "returns the URL without side effects"]
pub fn download_url(base: &str, version: &str, platform: Platform) -> String {
    format!(
        "{}/{version}/{}",
        base.trim_end_matches('/'),
        asset_name(platform)
    )
}

/// The pinned sha256 of `platform`'s release asset.
#[must_use = "returns the checksum without side effects"]
pub fn pinned_sha256(platform: Platform) -> &'static str {
    match platform {
        Platform::LinuxX64 => SHA256_LINUX_X64,
        Platform::MacosArm64 => SHA256_MACOS_ARM64,
        Platform::WindowsX64 => SHA256_WINDOWS_X64,
    }
}

/// The archive-relative files that must be installed for `platform`, in
/// POSIX-separated form (the archive's own layout). macOS additionally requires
/// the `libbinaryen.dylib` sibling `wasm-opt` links against at runtime.
fn required_files(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::LinuxX64 => &["bin/wasm-opt"],
        Platform::MacosArm64 => &["bin/wasm-opt", "lib/libbinaryen.dylib"],
        Platform::WindowsX64 => &["bin/wasm-opt.exe"],
    }
}

/// The download base URL: the [`BINARYEN_BASE_URL_ENV`] override if set,
/// otherwise [`DEFAULT_BASE_URL`].
fn base_url() -> String {
    std::env::var(BINARYEN_BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// The checksum to verify a download against, applying the test/mirror seam.
fn expected_sha256(platform: Platform) -> String {
    sha_seam(
        pinned_sha256(platform),
        std::env::var_os(BINARYEN_BASE_URL_ENV).is_some(),
        std::env::var(BINARYEN_SHA256_OVERRIDE_ENV).ok(),
    )
}

/// Pure core of the checksum seam: the override replaces the pinned value only
/// when the base URL was *also* overridden. With the production base URL
/// (`base_overridden == false`) the pin is returned unconditionally — no
/// environment variable can weaken the checksum of an official download. This
/// is a security property, so the two conditions are deliberately coupled.
fn sha_seam(pinned: &str, base_overridden: bool, override_sha: Option<String>) -> String {
    match override_sha {
        Some(sha) if base_overridden => sha,
        _ => pinned.to_string(),
    }
}

/// The `wasm-opt` executable path relative to a Binaryen install root. Uses the
/// host executable suffix because a managed install always matches the host.
fn wasm_opt_relative_path() -> PathBuf {
    Path::new("bin").join(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX))
}

/// Whether `dir` (a Binaryen install root) contains a `wasm-opt` executable.
fn dir_has_wasm_opt(dir: &Path) -> bool {
    dir.join(wasm_opt_relative_path()).is_file()
}

/// Converts an archive-relative POSIX path from [`required_files`] into a
/// platform path by joining its `/`-separated segments.
fn rel_to_path(rel: &str) -> PathBuf {
    rel.split('/').collect()
}

/// The path to the managed `wasm-opt`, or `None` if it is not installed.
#[must_use = "returns the path without side effects"]
pub fn installed_wasm_opt(paths: &ToolchainPaths) -> Option<PathBuf> {
    let candidate = paths
        .binaryen_dir(BINARYEN_PIN)
        .join(wasm_opt_relative_path());
    candidate.is_file().then_some(candidate)
}

/// Reports whether the managed `wasm-opt` is installed.
#[must_use = "returns status without side effects"]
pub fn status(paths: &ToolchainPaths) -> ComponentStatus {
    ComponentStatus {
        name: COMPONENT_NAME,
        installed: installed_wasm_opt(paths).is_some(),
        version: BINARYEN_PIN,
    }
}

/// Downloads, verifies, and installs the pinned Binaryen `wasm-opt` into
/// `binaryen_dir(BINARYEN_PIN)`, returning the path to the installed binary.
///
/// The operation is idempotent and atomic. An existing install short-circuits
/// with no network access. Otherwise the download is verified before anything
/// reaches `tools/`, staged under a per-process temp directory, and published
/// with a single [`std::fs::rename`] — so a failure at any step leaves no
/// partial install at the pinned path, and a pre-existing *broken* install
/// (a directory without the binary) is repaired.
///
/// # Errors
///
/// Returns an error if the download fails, the checksum does not match, the
/// archive is missing an expected file, or the install cannot be published.
pub async fn install(paths: &ToolchainPaths, platform: Platform) -> Result<PathBuf> {
    // Idempotent: an existing install means no network access at all.
    if let Some(existing) = installed_wasm_opt(paths) {
        println!(
            "Component '{COMPONENT_NAME}' (Binaryen {BINARYEN_PIN}) is already installed at {}.",
            existing.display()
        );
        return Ok(existing);
    }

    let dest_dir = paths.binaryen_dir(BINARYEN_PIN);
    let binaryen_root = dest_dir
        .parent()
        .expect("binaryen_dir always has a parent")
        .to_path_buf();
    std::fs::create_dir_all(&binaryen_root)
        .with_context(|| format!("Failed to create {}", binaryen_root.display()))?;

    // Sweep temp dirs left behind by interrupted prior installs.
    sweep_stale_temp_dirs(&binaryen_root);

    let asset = asset_name(platform);
    let url = download_url(&base_url(), BINARYEN_PIN, platform);
    let archive_path = paths.download_path(&asset);

    println!("Downloading {url}...");
    download_file(&url, &archive_path).await?;

    // Verify before anything reaches tools/. A mismatch deletes the archive so
    // nothing is left to be mistaken for a good download.
    println!("Verifying checksum...");
    if let Err(err) = verify_checksum(&archive_path, &expected_sha256(platform)) {
        std::fs::remove_file(&archive_path).ok();
        return Err(err);
    }

    // Everything below writes only under the per-process temp dir until the
    // final atomic rename.
    let temp_dir = binaryen_root.join(format!(".tmp-{}", std::process::id()));
    let result = stage_and_publish(&archive_path, &temp_dir, &dest_dir, platform);

    // Always clean the temp dir and the downloaded archive.
    std::fs::remove_dir_all(&temp_dir).ok();
    std::fs::remove_file(&archive_path).ok();
    result?;

    let installed = installed_wasm_opt(paths).with_context(|| {
        format!(
            "wasm-opt still not present at {} after install",
            dest_dir.display()
        )
    })?;
    println!(
        "Installed {COMPONENT_NAME} (Binaryen {BINARYEN_PIN}) at {}.",
        installed.display()
    );
    Ok(installed)
}

/// Extracts the archive, copies the platform's [`required_files`] into a staging
/// directory, and atomically publishes it at `dest_dir`.
fn stage_and_publish(
    archive_path: &Path,
    temp_dir: &Path,
    dest_dir: &Path,
    platform: Platform,
) -> Result<()> {
    // Start from a clean temp dir (a same-pid crash could have left one).
    std::fs::remove_dir_all(temp_dir).ok();
    let extract_dir = temp_dir.join("extract");
    let install_dir = temp_dir.join("install");
    std::fs::create_dir_all(&install_dir)
        .with_context(|| format!("Failed to create {}", install_dir.display()))?;

    println!("Extracting...");
    extract_archive(archive_path, &extract_dir)?;

    for rel in required_files(platform) {
        let src = extract_dir.join(rel_to_path(rel));
        if !src.is_file() {
            bail!(
                "Binaryen archive is missing the expected file `{rel}`. The \
                 release layout for {platform} may have changed; the pin \
                 `{BINARYEN_PIN}` may need updating."
            );
        }
        let dst = install_dir.join(rel_to_path(rel));
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        std::fs::copy(&src, &dst)
            .with_context(|| format!("Failed to copy {} to {}", src.display(), dst.display()))?;
        // Harmless on the dylib (dylibs are conventionally 0o755) and required
        // for the binary.
        set_executable_file(&dst)?;
    }

    publish(&install_dir, dest_dir)
}

/// Atomically moves the staged `install_dir` to `dest_dir`.
///
/// A `dest_dir` that already holds a valid binary is a concurrent installer that
/// won the race (or an install that appeared after our idempotent check) — it is
/// adopted rather than clobbered. A `dest_dir` *without* the binary is a broken
/// leftover, removed so the fresh install repairs it. If the rename still races
/// a concurrent winner, the loser re-checks and succeeds.
fn publish(install_dir: &Path, dest_dir: &Path) -> Result<()> {
    if dir_has_wasm_opt(dest_dir) {
        return Ok(());
    }
    if dest_dir.exists() {
        std::fs::remove_dir_all(dest_dir).with_context(|| {
            format!("Failed to remove broken install at {}", dest_dir.display())
        })?;
    }
    if let Err(err) = std::fs::rename(install_dir, dest_dir) {
        if dir_has_wasm_opt(dest_dir) {
            return Ok(());
        }
        return Err(anyhow::Error::from(err).context(format!(
            "Failed to publish {COMPONENT_NAME} install to {}",
            dest_dir.display()
        )));
    }
    Ok(())
}

/// Removes temp directories left behind by interrupted installs.
fn sweep_stale_temp_dirs(binaryen_root: &Path) {
    let Ok(entries) = std::fs::read_dir(binaryen_root) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(".tmp-") {
            std::fs::remove_dir_all(entry.path()).ok();
        }
    }
}

/// Synchronous bridge to [`install`] for the build-time auto-install path, which
/// runs in a synchronous context. Reuses the ambient multi-threaded Tokio
/// runtime via [`tokio::task::block_in_place`] when one is present, and falls
/// back to a fresh runtime otherwise.
///
/// # Errors
///
/// Propagates every error from [`install`], plus a runtime-creation failure on
/// the fallback path.
pub fn install_blocking(paths: &ToolchainPaths, platform: Platform) -> Result<PathBuf> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(install(paths, platform)))
    } else {
        let runtime = tokio::runtime::Runtime::new()
            .context("Failed to create a Tokio runtime for the wasm-opt download")?;
        runtime.block_on(install(paths, platform))
    }
}

/// Removes the managed Binaryen install.
///
/// # Errors
///
/// Bails when nothing is installed (mirroring `infs uninstall`), or if the
/// install directory cannot be removed.
pub fn remove(paths: &ToolchainPaths) -> Result<()> {
    let dir = paths.binaryen_dir(BINARYEN_PIN);
    if !dir.exists() {
        bail!("Component '{COMPONENT_NAME}' (Binaryen {BINARYEN_PIN}) is not installed.");
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("Failed to remove {}", dir.display()))?;
    // Best-effort: drop the now-empty binaryen parent directory.
    if let Some(parent) = dir.parent() {
        std::fs::remove_dir(parent).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `ToolchainPaths` rooted at a fresh temporary directory.
    ///
    /// The returned `TempDir` owns the directory and deletes it on drop, so
    /// callers must keep it bound for the whole test. The name comes from the
    /// operating system's exclusive-create loop, so it is unique against both
    /// parallel test threads and concurrent test processes.
    fn temp_paths() -> (assert_fs::TempDir, ToolchainPaths) {
        let temp = assert_fs::TempDir::new().unwrap();
        let paths = ToolchainPaths::with_root(temp.path().to_path_buf());
        (temp, paths)
    }

    #[test]
    fn asset_name_matches_release_layout() {
        assert_eq!(
            asset_name(Platform::LinuxX64),
            "binaryen-version_130-x86_64-linux.tar.gz"
        );
        assert_eq!(
            asset_name(Platform::MacosArm64),
            "binaryen-version_130-arm64-macos.tar.gz"
        );
        assert_eq!(
            asset_name(Platform::WindowsX64),
            "binaryen-version_130-x86_64-windows.tar.gz"
        );
    }

    #[test]
    fn download_url_joins_base_version_and_asset() {
        assert_eq!(
            download_url("https://example.com/dl", BINARYEN_PIN, Platform::LinuxX64),
            "https://example.com/dl/version_130/binaryen-version_130-x86_64-linux.tar.gz"
        );
    }

    #[test]
    fn download_url_trims_trailing_slash_on_base() {
        assert_eq!(
            download_url(
                "https://example.com/dl/",
                BINARYEN_PIN,
                Platform::MacosArm64
            ),
            "https://example.com/dl/version_130/binaryen-version_130-arm64-macos.tar.gz"
        );
    }

    #[test]
    fn pinned_sha256_values_are_lowercase_64_hex() {
        for platform in [
            Platform::LinuxX64,
            Platform::MacosArm64,
            Platform::WindowsX64,
        ] {
            let sha = pinned_sha256(platform);
            assert_eq!(sha.len(), 64, "sha for {platform} must be 64 chars");
            assert!(
                sha.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "sha for {platform} must be lowercase hex"
            );
        }
    }

    #[test]
    fn required_files_shapes_per_platform() {
        assert_eq!(required_files(Platform::LinuxX64), &["bin/wasm-opt"]);
        assert_eq!(required_files(Platform::WindowsX64), &["bin/wasm-opt.exe"]);
        let macos = required_files(Platform::MacosArm64);
        assert!(macos.contains(&"bin/wasm-opt"));
        assert!(
            macos.contains(&"lib/libbinaryen.dylib"),
            "macOS layout must include the dylib sibling"
        );
    }

    #[test]
    fn rel_to_path_joins_posix_segments() {
        assert_eq!(
            rel_to_path("bin/wasm-opt"),
            Path::new("bin").join("wasm-opt")
        );
    }

    #[test]
    fn sha_seam_ignores_override_without_base_override() {
        // Security property: the pin cannot be weakened by the sha override alone.
        let pinned = pinned_sha256(Platform::LinuxX64);
        assert_eq!(
            sha_seam(pinned, false, Some("deadbeef".to_string())),
            pinned
        );
    }

    #[test]
    fn sha_seam_honors_override_only_with_base_override() {
        assert_eq!(
            sha_seam("pin", true, Some("deadbeef".to_string())),
            "deadbeef"
        );
    }

    #[test]
    fn sha_seam_uses_pin_when_no_override_present() {
        assert_eq!(sha_seam("pin", true, None), "pin");
    }

    #[test]
    fn installed_wasm_opt_finds_binary_under_root() {
        let (_temp, paths) = temp_paths();
        assert!(
            installed_wasm_opt(&paths).is_none(),
            "absent binary must yield None"
        );

        let bin_dir = paths.binaryen_dir(BINARYEN_PIN).join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let binary = bin_dir.join(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&binary, b"fake").unwrap();

        assert_eq!(installed_wasm_opt(&paths), Some(binary));
    }

    #[test]
    fn status_reflects_install_state() {
        let (_temp, paths) = temp_paths();

        let absent = status(&paths);
        assert_eq!(absent.name, "wasm-opt");
        assert_eq!(absent.version, BINARYEN_PIN);
        assert!(!absent.installed);

        let bin_dir = paths.binaryen_dir(BINARYEN_PIN).join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(
            bin_dir.join(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX)),
            b"fake",
        )
        .unwrap();
        assert!(status(&paths).installed);
    }

    #[test]
    fn remove_bails_when_absent() {
        let (_temp, paths) = temp_paths();
        let err = remove(&paths).unwrap_err();
        assert!(
            err.to_string().contains("not installed"),
            "removing an absent component must bail, got: {err}"
        );
    }

    #[test]
    fn remove_deletes_installed_component() {
        let (_temp, paths) = temp_paths();
        let bin_dir = paths.binaryen_dir(BINARYEN_PIN).join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(
            bin_dir.join(format!("wasm-opt{}", std::env::consts::EXE_SUFFIX)),
            b"fake",
        )
        .unwrap();

        remove(&paths).unwrap();
        assert!(!paths.binaryen_dir(BINARYEN_PIN).exists());
    }
}

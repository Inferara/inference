//! Self command for the infs CLI.
//!
//! Provides subcommands for managing the `infs` binary itself.
//!
//! ## Usage
//!
//! ```bash
//! infs self update    # Update infs to the latest version
//! ```
//!
//! An update also installs the third-party notices the release archive
//! carries in `licenses/`, before it replaces the binary: into the toolchain
//! home's `licenses/` ([`ToolchainPaths::licenses_dir`]), replacing any an
//! earlier update left there, and over a `licenses/` beside the running binary
//! that already holds `infs`'s notices, where the VS Code extension and a
//! hand-unpacked archive leave one.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::toolchain::{
    Platform, ToolchainPaths, download_file, extract_archive, fetch_manifest, latest_stable,
    latest_version, verify_checksum,
};

/// The directory of an `infs` release archive that holds its third-party
/// notices, beside the binary.
const ARCHIVE_NOTICES_DIR: &str = "licenses";

/// Arguments for the self command.
#[derive(Args)]
pub struct SelfArgs {
    #[command(subcommand)]
    pub command: SelfCommand,
}

/// Subcommands for self management.
#[derive(Subcommand)]
pub enum SelfCommand {
    /// Update infs to the latest version.
    Update,
}

/// Executes the self command.
///
/// # Errors
///
/// Returns an error if the subcommand fails.
pub async fn execute(args: &SelfArgs) -> Result<()> {
    match &args.command {
        SelfCommand::Update => execute_update().await,
    }
}

/// Executes the self update subcommand.
///
/// # Process
///
/// 1. Fetch the release manifest
/// 2. Compare current version with latest
/// 3. If newer version available, download it
/// 4. Verify checksum
/// 5. Extract the archive
/// 6. Install the archive's third-party notices into the toolchain home's
///    `licenses/`, and over a `licenses/` beside the running binary that
///    already holds `infs`'s notices, when it carries any (see
///    [`notices_destinations`] and [`install_update`])
/// 7. Replace current binary
/// 8. Report the update and where the notices are (see [`update_report`])
///
/// ## Windows Strategy
///
/// On Windows, the running binary cannot be replaced directly.
/// Instead, we rename the current binary to `infs.old` and place
/// the new binary in its place. The old binary can be deleted
/// on the next run or manually.
///
/// # Errors
///
/// Returns an error if:
/// - Manifest fetch fails
/// - No infs artifact for current platform
/// - Download fails
/// - Checksum verification fails
/// - The archive has no `infs` binary
/// - The third-party notices cannot be installed (the binary is then left as it was)
/// - Binary replacement fails
async fn execute_update() -> Result<()> {
    let platform = Platform::detect()?;
    let paths = ToolchainPaths::new()?;
    paths.ensure_directories()?;

    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current infs version: {current_version}");

    println!("Checking for updates...");
    let manifest = fetch_manifest().await?;

    let latest_entry = latest_stable(&manifest)
        .or_else(|| latest_version(&manifest))
        .context("No version found in manifest")?;
    let latest_version = &latest_entry.version;

    if latest_version == current_version {
        println!("infs is already up to date.");
        return Ok(());
    }

    let current_semver = semver::Version::parse(current_version)
        .with_context(|| format!("Invalid current version: {current_version}"))?;
    let latest_semver = semver::Version::parse(latest_version)
        .with_context(|| format!("Invalid latest version: {latest_version}"))?;

    if current_semver >= latest_semver {
        println!(
            "infs is already up to date (current: {current_version}, available: {latest_version})."
        );
        return Ok(());
    }

    let artifact = latest_entry
        .find_infs_artifact(platform)
        .with_context(|| format!("No infs binary available for platform {platform}"))?;

    println!("Updating infs from {current_version} to {latest_version}...");

    let download_filename = artifact.filename();
    let download_path = paths.download_path(download_filename);

    println!("Downloading from {}...", artifact.url);
    download_file(&artifact.url, &download_path).await?;

    println!("Verifying checksum...");
    verify_checksum(&download_path, &artifact.sha256)?;

    println!("Extracting...");
    let temp_dir = paths.downloads.join(format!("infs-{latest_version}-temp"));
    extract_archive(&download_path, &temp_dir)?;

    let new_binary_name = format!("infs{}", platform.executable_extension());
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
    let home_notices = paths.licenses_dir();
    let archive_notices = temp_dir.join(ARCHIVE_NOTICES_DIR);
    let notices_dirs = notices_destinations(&home_notices, &current_exe, &archive_notices);
    let notices_installed = install_update(
        &temp_dir,
        &new_binary_name,
        &notices_dirs,
        current_version,
        |new_binary| replace_binary(new_binary, platform),
    )?;

    std::fs::remove_file(&download_path).ok();
    std::fs::remove_dir_all(&temp_dir).ok();

    for line in update_report(latest_version, &notices_installed, platform) {
        println!("{line}");
    }

    Ok(())
}

/// The lines `infs self update` prints once it has installed `version`: the
/// update, then each directory it installed the archive's third-party notices
/// at, in `notices_dirs`' order, then, on Windows, a reminder to restart the
/// terminal. An archive that carried no notices leaves `notices_dirs` empty,
/// and the report says nothing about them.
fn update_report(version: &str, notices_dirs: &[PathBuf], platform: Platform) -> Vec<String> {
    let mut lines = vec![format!("Successfully updated infs to {version}.")];
    for dir in notices_dirs {
        lines.push(format!("Third-party notices: {}", dir.display()));
    }
    if platform.is_windows() {
        lines.push("Note: Please restart your terminal to use the new version.".to_string());
    }
    lines
}

/// The directories an update installs the archive's third-party notices at:
/// the toolchain home's `home_notices`, then the `licenses/` beside the
/// running binary `running_binary`, when one is already there, is not
/// `home_notices` itself, however either is spelled, and already holds some of
/// the notices the archive carries in `archive_notices` (see
/// [`holds_infs_notices`]).
///
/// The VS Code extension unpacks the `infs` archive into the home's `bin/`,
/// and a hand-unpacked archive leaves `licenses/` beside `infs` as well. An
/// update refreshes that copy, so that it goes on describing the binary beside
/// it, but never creates one beside a binary that had none: only a directory
/// counts, not a file or a symbolic link of that name. `infs` may share its
/// directory with other programs, as in `/usr/local/bin`, so a `licenses/`
/// there holding none of the archive's notices is another program's, and the
/// update leaves it alone.
fn notices_destinations(
    home_notices: &Path,
    running_binary: &Path,
    archive_notices: &Path,
) -> Vec<PathBuf> {
    let mut destinations = vec![home_notices.to_path_buf()];
    let beside_binary = running_binary.with_file_name(ARCHIVE_NOTICES_DIR);
    let is_directory = std::fs::symlink_metadata(&beside_binary).is_ok_and(|m| m.is_dir());
    if is_directory
        && !is_same_directory(&beside_binary, home_notices)
        && holds_infs_notices(&beside_binary, archive_notices)
    {
        destinations.push(beside_binary);
    }
    destinations
}

/// Whether the directory `dir` holds `infs`'s notices: a regular file at the
/// path, relative to it, of one of the notice files in `archive_notices` (see
/// [`notice_files`]). Only the file's place counts, not its bytes, which an
/// earlier release may have carried differently.
fn holds_infs_notices(dir: &Path, archive_notices: &Path) -> bool {
    let archive_files = notice_files(archive_notices);
    let files = notice_files(dir);
    files.iter().any(|file| archive_files.contains(file))
}

/// The regular files under the notices directory `dir`, as paths relative to
/// it, except the `README.md` at its top, a name any directory of notices
/// might use. Symbolic links are not followed, and a member that cannot be
/// read is left out.
fn notice_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    push_regular_files(dir, Path::new(""), &mut files);
    files.retain(|file| file != Path::new("README.md"));
    files
}

/// Appends the regular files under `dir`, as paths under `prefix`, to `files`,
/// descending into its directories but following no symbolic link.
fn push_regular_files(dir: &Path, prefix: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let member = prefix.join(entry.file_name());
        if file_type.is_dir() {
            push_regular_files(&entry.path(), &member, files);
        } else if file_type.is_file() {
            files.push(member);
        }
    }
}

/// Whether `a` and `b` name the same directory, however each is spelled.
fn is_same_directory(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(canonical_a), Ok(canonical_b)) => canonical_a == canonical_b,
        _ => a == b,
    }
}

/// Installs what the `infs` release archive extracted at `extracted` carries:
/// the third-party notices in its `licenses/` at each of `notices_dirs`, in
/// order, then its binary `binary_name` through `install_binary`, which
/// receives its path.
///
/// The notices go first. An update that cannot install them at every one of
/// `notices_dirs` fails with the running binary untouched, still at
/// `running_version`, and a new binary never reaches the disk without the
/// notices its archive carried. A failure at one directory leaves those before
/// it installed. Returns the directories the notices were installed at: none
/// for an archive without `licenses/`, as released before they shipped, which
/// leaves every one of `notices_dirs` as it was.
///
/// # Errors
///
/// Returns an error, installing nothing, if the archive has no `binary_name`;
/// otherwise the error of [`install_notices`], under one saying that infs was
/// not updated, before `install_binary` runs, or the error of
/// `install_binary`, as it is, after the notices are installed.
fn install_update(
    extracted: &Path,
    binary_name: &str,
    notices_dirs: &[PathBuf],
    running_version: &str,
    install_binary: impl FnOnce(&Path) -> Result<()>,
) -> Result<Vec<PathBuf>> {
    let new_binary = extracted.join(binary_name);
    if !new_binary.exists() {
        bail!(
            "infs binary not found in downloaded archive. Expected at {}",
            new_binary.display()
        );
    }
    let source = extracted.join(ARCHIVE_NOTICES_DIR);
    let not_updated = || format!("infs was not updated; it is still {running_version}");
    let mut installed = Vec::new();
    for dest in notices_dirs {
        if install_notices(&source, dest).with_context(not_updated)? {
            installed.push(dest.clone());
        }
    }
    install_binary(&new_binary)?;
    Ok(installed)
}

/// Installs the third-party notices directory `source` at `dest`, replacing
/// whatever `dest` held.
///
/// Returns `false`, touching nothing, when `source` does not exist. Otherwise
/// every file under `source` is copied byte for byte into a staging directory
/// beside `dest`, `.licenses.new` for `licenses`, on the same filesystem, and
/// swapped in by renames. No platform renames a directory onto a non-empty
/// one, and some Windows versions and filesystems refuse even an empty one, so
/// an existing `dest` is first moved aside to `.licenses.old`; the staged copy
/// then takes its place, and the old copy is removed.
///
/// `dest` never holds a partial or merged copy: it holds the old notices, the
/// new ones, or nothing. A failure before the swap leaves the old ones in
/// place, and a failure moving the new ones in moves the old ones back. When
/// the old ones cannot be moved back either, or the process stops between the
/// two renames, `dest` is left absent and the old notices remain in the
/// set-aside `.licenses.old`, which the error of a failed move back names; the
/// next install discards it, with any staged copy, before it stages anew.
///
/// # Errors
///
/// Returns an error if `source` is not a directory, if a member of it cannot be
/// read or is neither a directory nor a regular file, or if staging or
/// swapping fails.
fn install_notices(source: &Path, dest: &Path) -> Result<bool> {
    install_notices_with(source, dest, |from, to| std::fs::rename(from, to))
}

/// [`install_notices`], with every rename of the swap made through `rename`
/// so that a test can fail one of them.
fn install_notices_with(
    source: &Path,
    dest: &Path,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<bool> {
    match std::fs::symlink_metadata(source) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "Failed to read the third-party notices at {}",
                    source.display()
                )
            });
        }
        Ok(metadata) if !metadata.is_dir() => bail!(
            "The third-party notices at {} are not a directory",
            source.display()
        ),
        Ok(_) => {}
    }

    let staged = beside(dest, "new");
    let old = beside(dest, "old");
    remove_leftover(&staged)?;
    remove_leftover(&old)?;

    if let Err(err) = copy_tree(source, &staged) {
        std::fs::remove_dir_all(&staged).ok();
        return Err(err).with_context(|| {
            format!(
                "Failed to stage the third-party notices at {}",
                staged.display()
            )
        });
    }

    let replacing = std::fs::symlink_metadata(dest).is_ok();
    if replacing && let Err(err) = rename(dest, &old) {
        std::fs::remove_dir_all(&staged).ok();
        return Err(err).with_context(|| {
            format!(
                "Failed to move the previous third-party notices at {} aside",
                dest.display()
            )
        });
    }
    if let Err(err) = rename(&staged, dest) {
        std::fs::remove_dir_all(&staged).ok();
        let failed = format!(
            "Failed to install the third-party notices at {}",
            dest.display()
        );
        if replacing && let Err(restore) = rename(&old, dest) {
            return Err(err).with_context(|| {
                format!(
                    "{failed}; the previous notices could not be moved back ({restore}) and \
                     remain at {}",
                    old.display()
                )
            });
        }
        return Err(err).context(failed);
    }
    if replacing {
        remove_leftover(&old).ok();
    }
    Ok(true)
}

/// The dot-named path beside `dest` named after it and `suffix`:
/// `.licenses.new` for `licenses` and `new`.
fn beside(dest: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(dest.file_name().unwrap_or_default());
    name.push(".");
    name.push(suffix);
    dest.with_file_name(name)
}

/// Removes what `path` holds, a directory tree or a file, if anything: the
/// staging and set-aside copies an interrupted update leaves behind.
fn remove_leftover(path: &Path) -> Result<()> {
    let removed = match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => Err(err),
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
    };
    removed.with_context(|| format!("Failed to remove {}", path.display()))
}

/// Copies the directory tree `source` to `dest`: every directory created and
/// every file copied byte for byte.
///
/// # Errors
///
/// Returns an error if a member cannot be read or written, or is neither a
/// directory nor a regular file.
fn copy_tree(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create {}", dest.display()))?;
    let entries = std::fs::read_dir(source)
        .with_context(|| format!("Failed to read {}", source.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("Failed to read {}", source.display()))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to read {}", from.display()))?;
        if file_type.is_dir() {
            copy_tree(&from, &to)?;
        } else if file_type.is_file() {
            std::fs::copy(&from, &to).with_context(|| {
                format!("Failed to copy {} to {}", from.display(), to.display())
            })?;
        } else {
            bail!("{} is neither a file nor a directory", from.display());
        }
    }
    Ok(())
}

/// Replaces the current binary with a new one.
fn replace_binary(new_binary: &Path, _platform: Platform) -> Result<()> {
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(new_binary)
            .with_context(|| format!("Failed to get metadata: {}", new_binary.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(new_binary, perms)
            .with_context(|| format!("Failed to set permissions: {}", new_binary.display()))?;

        std::fs::rename(new_binary, &current_exe).with_context(|| {
            format!(
                "Failed to replace binary. You may need to run with elevated privileges.\n\
                 Source: {}\n\
                 Destination: {}",
                new_binary.display(),
                current_exe.display()
            )
        })?;
    }

    #[cfg(windows)]
    {
        let old_binary = current_exe.with_extension("old.exe");

        if old_binary.exists() {
            std::fs::remove_file(&old_binary).ok();
        }

        std::fs::rename(&current_exe, &old_binary).with_context(|| {
            format!(
                "Failed to rename current binary to {}",
                old_binary.display()
            )
        })?;

        if let Err(e) = std::fs::rename(new_binary, &current_exe) {
            std::fs::rename(&old_binary, &current_exe).ok();
            return Err(e).with_context(|| {
                format!("Failed to install new binary at {}", current_exe.display())
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TreeEntry, member_path, repository_licenses, tree_entries};

    /// The version of the running `infs` in these tests.
    const RUNNING_VERSION: &str = "0.0.1";

    /// The members of the notices directory `dir`, named from `licenses`
    /// whatever `dir` is called, so that every copy lists alike.
    fn notices_in(dir: &Path) -> Vec<TreeEntry> {
        tree_entries(dir, ARCHIVE_NOTICES_DIR)
    }

    /// The notices CI packs into the release archive: the repository's
    /// `licenses/`.
    fn archive_notices() -> Vec<TreeEntry> {
        notices_in(&repository_licenses())
    }

    /// A directory member.
    fn dir_entry(name: &str) -> TreeEntry {
        TreeEntry::Dir(name.to_string())
    }

    /// A file member holding `bytes`.
    fn file_entry(name: &str, bytes: &[u8]) -> TreeEntry {
        TreeEntry::File(name.to_string(), bytes.to_vec())
    }

    /// Writes `entries` under `dir`, parents before their children as
    /// [`tree_entries`] lists them: each directory created, each file written.
    fn write_entries(dir: &Path, entries: &[TreeEntry]) {
        for entry in entries {
            match entry {
                TreeEntry::Dir(name) => {
                    std::fs::create_dir_all(member_path(dir, name)).expect("Should create");
                }
                TreeEntry::File(name, bytes) => {
                    std::fs::write(member_path(dir, name), bytes).expect("Should write");
                }
            }
        }
    }

    /// `entries` as a listing to compare: each directory as `name/` and each
    /// file as `name`, so that a directory left behind empty shows.
    fn listing(entries: &[TreeEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                TreeEntry::Dir(name) => format!("{name}/"),
                TreeEntry::File(name, _) => name.clone(),
            })
            .collect()
    }

    /// A member's bytes: a file's contents, and none for a directory.
    fn contents(entry: &TreeEntry) -> &[u8] {
        match entry {
            TreeEntry::Dir(_) => &[],
            TreeEntry::File(_, bytes) => bytes,
        }
    }

    /// The names directly under `dir`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("Should read the directory")
            .map(|entry| {
                entry
                    .expect("Should read directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// Asserts that the notices directory `dir` holds exactly `expected`: the
    /// same directories and files, and each file byte for byte.
    fn assert_holds(dir: &Path, expected: &[TreeEntry], what: &str) {
        let actual = notices_in(dir);
        let names = listing(&actual);
        assert_eq!(names, listing(expected), "{what}: the members");
        for ((name, entry), expected_entry) in names.iter().zip(&actual).zip(expected) {
            let (actual, expected) = (contents(entry), contents(expected_entry));
            let first_difference = actual
                .iter()
                .zip(expected)
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(actual.len().min(expected.len()));
            assert!(
                actual == expected,
                "{what}: {name} must be the {} bytes expected, got {} bytes differing from byte \
                 {first_difference}",
                expected.len(),
                actual.len()
            );
        }
    }

    /// The notices an earlier update installed: an older copy of two files the
    /// current archive carries, and a file it no longer carries, in a
    /// directory it no longer has.
    fn previous_notices() -> Vec<TreeEntry> {
        vec![
            dir_entry("licenses"),
            file_entry("licenses/README.md", b"previous notices readme\n"),
            dir_entry("licenses/retired"),
            file_entry(
                "licenses/retired/LICENSE",
                b"the license of a crate infs no longer links\n",
            ),
            dir_entry("licenses/spacewasm"),
            file_entry("licenses/spacewasm/NOTICE", b"an earlier notice\n"),
        ]
    }

    /// A toolchain home holding an extracted `infs` release archive where
    /// `infs self update` extracts one, under `downloads/`, and the path of
    /// the `infs` that runs the update.
    struct Update {
        _temp: assert_fs::TempDir,
        home: ToolchainPaths,
        extracted: PathBuf,
        running_binary: PathBuf,
    }

    impl Update {
        /// An archive carrying the repository's notices in `licenses/` beside
        /// the `infs` binary, as CI packs it.
        fn with_notices() -> Self {
            let update = Self::without_notices();
            write_entries(&update.extracted, &archive_notices());
            update
        }

        /// An archive carrying the `infs` binary alone, as released before
        /// the notices shipped, for an `infs` in the home's `bin/`, which
        /// nothing has created yet.
        fn without_notices() -> Self {
            let temp = assert_fs::TempDir::new().expect("Should create temp dir");
            let home = ToolchainPaths::with_root(temp.path().join("home"));
            let extracted = home.downloads.join("infs-9.9.9-temp");
            std::fs::create_dir_all(&extracted).expect("Should create the extraction dir");
            std::fs::write(extracted.join("infs"), b"new infs binary").expect("Should write");
            let running_binary = home.bin.join("infs");
            Self {
                _temp: temp,
                home,
                extracted,
                running_binary,
            }
        }

        /// The home holding [`previous_notices`] where an earlier update left
        /// them.
        fn after_an_earlier_update(self) -> Self {
            write_entries(&self.home.root, &previous_notices());
            self
        }

        /// The home as the VS Code extension leaves it: an earlier `infs`
        /// archive unpacked into `bin/`, the running binary with
        /// [`previous_notices`] in `licenses/` beside it.
        fn unpacked_into_bin(self) -> Self {
            self.with_licenses_in_bin(&previous_notices())
        }

        /// The running binary in the home's `bin/` with a `licenses/` beside
        /// it holding `entries`, listed from `licenses`.
        fn with_licenses_in_bin(self, entries: &[TreeEntry]) -> Self {
            write_entries(&self.home.bin, entries);
            std::fs::write(&self.running_binary, b"old infs binary").expect("Should write");
            self
        }

        /// The update run by an `infs` in the home itself, whose `licenses/`
        /// is then the home's.
        fn running_from_the_home(mut self) -> Self {
            self.running_binary = self.home.root.join("infs");
            self
        }

        /// The update run by an `infs` in the home reached through a symbolic
        /// link, a spelling of the home other than the one it is configured
        /// with.
        #[cfg(unix)]
        fn running_from_a_link_to_the_home(mut self) -> Self {
            let link = self.home.root.with_file_name("link");
            std::os::unix::fs::symlink(&self.home.root, &link).expect("Should link the home");
            self.running_binary = link.join("infs");
            self
        }

        /// The archive's `licenses/`.
        fn source(&self) -> PathBuf {
            self.extracted.join(ARCHIVE_NOTICES_DIR)
        }

        /// Where the notices are installed in the home.
        fn dest(&self) -> PathBuf {
            self.home.licenses_dir()
        }

        /// The `licenses/` beside the running binary.
        fn beside_binary(&self) -> PathBuf {
            self.running_binary.with_file_name(ARCHIVE_NOTICES_DIR)
        }

        /// The names at the top of the home, sorted.
        fn home_entries(&self) -> Vec<String> {
            names_in(&self.home.root)
        }

        /// Where `infs self update` installs the archive's notices for the
        /// running binary.
        fn notices_dirs(&self) -> Vec<PathBuf> {
            notices_destinations(&self.dest(), &self.running_binary, &self.source())
        }

        /// Runs the installs of `infs self update` for the running binary as
        /// `execute_update` runs them, with `install_binary` standing in for
        /// the binary's replacement.
        fn install(
            &self,
            install_binary: impl FnOnce(&Path) -> Result<()>,
        ) -> Result<Vec<PathBuf>> {
            let notices_dirs = self.notices_dirs();
            install_update(
                &self.extracted,
                "infs",
                &notices_dirs,
                RUNNING_VERSION,
                install_binary,
            )
        }
    }

    /// A rename for [`install_notices_with`] that fails, as a full disk or a
    /// locked file would, when it moves one of `failing`, and renames
    /// otherwise.
    fn rename_failing_from(
        failing: Vec<PathBuf>,
    ) -> impl FnMut(&Path, &Path) -> std::io::Result<()> {
        move |from, to| {
            if failing.iter().any(|path| path == from) {
                Err(std::io::Error::other(format!(
                    "injected failure moving {}",
                    from.display()
                )))
            } else {
                std::fs::rename(from, to)
            }
        }
    }

    #[test]
    fn repository_licenses_carries_nested_notices() {
        let members = listing(&archive_notices());
        let nested = [
            "licenses/spacewasm/",
            "licenses/spacewasm/NOTICE",
            "licenses/spacewasm/LICENSE",
            "licenses/libm/",
            "licenses/libm/LICENSE.txt",
        ];
        for member in nested {
            assert!(
                members.iter().any(|listed| listed == member),
                "the fixture must carry the nested member {member}, got {members:?}"
            );
        }
    }

    #[test]
    fn staging_paths_are_dot_named_siblings_of_the_notices() {
        let dest = Path::new("home").join("licenses");

        assert_eq!(
            beside(&dest, "new"),
            Path::new("home").join(".licenses.new")
        );
        assert_eq!(
            beside(&dest, "old"),
            Path::new("home").join(".licenses.old")
        );
    }

    /// A home with no notices yet receives the archive's `licenses/` whole,
    /// nested directories included, byte for byte, and nothing else.
    #[test]
    fn a_fresh_home_receives_the_archives_notices_byte_for_byte() {
        let update = Update::with_notices();

        let installed = install_notices(&update.source(), &update.dest()).expect("Should install");

        assert!(installed, "an archive carrying notices installs them");
        assert_holds(&update.dest(), &archive_notices(), "the installed notices");
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// An update replaces the notices an earlier one installed: every file is
    /// the archive's, and a file and a directory the archive no longer
    /// carries are gone.
    #[test]
    fn an_update_replaces_the_previous_notices_whole() {
        let update = Update::with_notices().after_an_earlier_update();

        let installed = install_notices(&update.source(), &update.dest()).expect("Should install");

        assert!(installed);
        assert_holds(&update.dest(), &archive_notices(), "the replaced notices");
        assert_eq!(
            update.home_entries(),
            ["downloads", "licenses"],
            "neither the staged nor the previous copy is left beside the notices"
        );
    }

    /// Installing notices over identical ones leaves them as they are.
    #[test]
    fn installing_the_same_notices_again_changes_nothing() {
        let update = Update::with_notices();
        install_notices(&update.source(), &update.dest()).expect("Should install");

        let installed = install_notices(&update.source(), &update.dest()).expect("Should install");

        assert!(installed);
        assert_holds(
            &update.dest(),
            &archive_notices(),
            "the reinstalled notices",
        );
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// An archive released before the notices shipped installs nothing and
    /// leaves the notices an earlier update installed as they are.
    #[test]
    fn an_archive_without_notices_leaves_the_previous_ones_untouched() {
        let update = Update::without_notices().after_an_earlier_update();

        let installed = install_notices(&update.source(), &update.dest()).expect("Should succeed");

        assert!(!installed, "an archive without notices installs none");
        assert_holds(&update.dest(), &previous_notices(), "the previous notices");
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// An archive without notices creates no notices directory in a home
    /// that has none.
    #[test]
    fn an_archive_without_notices_creates_nothing_in_a_fresh_home() {
        let update = Update::without_notices();

        let installed = install_notices(&update.source(), &update.dest()).expect("Should succeed");

        assert!(!installed);
        assert!(!update.dest().exists(), "no notices directory is created");
        assert_eq!(update.home_entries(), ["downloads"]);
    }

    /// The staging and set-aside copies an interrupted update left behind are
    /// discarded, not merged into the installed notices.
    #[test]
    fn leftovers_of_an_interrupted_update_are_discarded() {
        let update = Update::with_notices().after_an_earlier_update();
        let staged = beside(&update.dest(), "new");
        let old = beside(&update.dest(), "old");
        std::fs::create_dir_all(&staged).expect("Should create");
        std::fs::write(staged.join("stale.txt"), b"half-staged").expect("Should write");
        std::fs::write(&old, b"a set-aside file").expect("Should write");

        install_notices(&update.source(), &update.dest()).expect("Should install");

        assert_holds(
            &update.dest(),
            &archive_notices(),
            "the notices installed over leftovers",
        );
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// When the staged notices cannot be moved into place after the previous
    /// ones were moved aside, the previous ones are moved back.
    #[test]
    fn a_failed_move_into_place_restores_the_previous_notices() {
        let update = Update::with_notices().after_an_earlier_update();
        let dest = update.dest();

        let err = install_notices_with(
            &update.source(),
            &dest,
            rename_failing_from(vec![beside(&dest, "new")]),
        )
        .expect_err("the injected failure fails the install");

        assert!(
            format!("{err:#}").contains(&format!(
                "Failed to install the third-party notices at {}",
                dest.display()
            )),
            "the error names the notices directory, got {err:#}"
        );
        assert_holds(&dest, &previous_notices(), "the restored notices");
        assert_eq!(
            update.home_entries(),
            ["downloads", "licenses"],
            "the staged copy is removed and the previous one is back in place"
        );
    }

    /// When the staged notices cannot be moved into a home that had none, the
    /// home is left without a notices directory rather than a partial one.
    #[test]
    fn a_failed_move_into_a_fresh_home_leaves_no_notices_directory() {
        let update = Update::with_notices();
        let dest = update.dest();

        install_notices_with(
            &update.source(),
            &dest,
            rename_failing_from(vec![beside(&dest, "new")]),
        )
        .expect_err("the injected failure fails the install");

        assert!(!dest.exists(), "no notices directory is left behind");
        assert_eq!(update.home_entries(), ["downloads"]);
    }

    /// When neither the new notices nor the previous ones can be moved into
    /// place, the error says where the previous ones remain, and they do.
    #[test]
    fn a_failed_restore_names_where_the_previous_notices_remain() {
        let update = Update::with_notices().after_an_earlier_update();
        let dest = update.dest();
        let old = beside(&dest, "old");

        let err = install_notices_with(
            &update.source(),
            &dest,
            rename_failing_from(vec![beside(&dest, "new"), old.clone()]),
        )
        .expect_err("the injected failures fail the install");

        let message = format!("{err:#}");
        assert!(
            message.contains(&format!(
                "the previous notices could not be moved back (injected failure moving {}) and \
                 remain at {}",
                old.display(),
                old.display()
            )),
            "the error names where the previous notices remain, got {message}"
        );
        assert_holds(&old, &previous_notices(), "the set-aside notices");
        assert!(!dest.exists());
    }

    /// The next install after a failed restore discards the notices it left
    /// set aside and installs the new ones whole where there were none.
    #[test]
    fn the_next_install_recovers_from_a_failed_restore() {
        let update = Update::with_notices().after_an_earlier_update();
        let dest = update.dest();
        let old = beside(&dest, "old");
        install_notices_with(
            &update.source(),
            &dest,
            rename_failing_from(vec![beside(&dest, "new"), old.clone()]),
        )
        .expect_err("the injected failures fail the install");
        assert!(!dest.exists(), "no notices directory is left");
        assert!(old.exists(), "the previous notices are set aside");

        install_notices(&update.source(), &dest).expect("Should install");

        assert_holds(
            &dest,
            &archive_notices(),
            "the notices installed after a failed restore",
        );
        assert_eq!(
            update.home_entries(),
            ["downloads", "licenses"],
            "the set-aside notices are discarded"
        );
    }

    /// When the previous notices cannot be moved aside, they stay in place and
    /// the staged copy is removed.
    #[test]
    fn a_failed_move_aside_leaves_the_previous_notices_in_place() {
        let update = Update::with_notices().after_an_earlier_update();
        let dest = update.dest();

        let err = install_notices_with(
            &update.source(),
            &dest,
            rename_failing_from(vec![dest.clone()]),
        )
        .expect_err("the injected failure fails the install");

        assert!(
            format!("{err:#}").contains("Failed to move the previous third-party notices"),
            "got {err:#}"
        );
        assert_holds(&dest, &previous_notices(), "the untouched notices");
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// An archive whose `licenses` is a file, not a directory, is refused
    /// before anything is staged.
    #[test]
    fn notices_that_are_not_a_directory_are_refused() {
        let update = Update::without_notices().after_an_earlier_update();
        std::fs::write(update.source(), b"not a directory").expect("Should write");

        let err = install_notices(&update.source(), &update.dest())
            .expect_err("a notices file is refused");

        assert!(
            format!("{err:#}").contains("are not a directory"),
            "got {err:#}"
        );
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// A notice that cannot be read fails the install while staging, with the
    /// previous notices in place and no staged copy left behind.
    #[cfg(unix)]
    #[test]
    fn a_notice_that_cannot_be_read_fails_before_the_swap() {
        use std::os::unix::fs::PermissionsExt;

        let update = Update::with_notices().after_an_earlier_update();
        let locked = update.source().join("spacewasm").join("NOTICE");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
            .expect("Should lock the notice");
        // A privileged process reads a 0o000 file regardless; the failure is
        // then impossible to provoke this way.
        if std::fs::read(&locked).is_ok() {
            return;
        }

        let result = install_notices(&update.source(), &update.dest());

        let err = result.expect_err("an unreadable notice fails the install");
        assert!(
            format!("{err:#}").contains("Failed to stage the third-party notices"),
            "got {err:#}"
        );
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
        assert_eq!(
            update.home_entries(),
            ["downloads", "licenses"],
            "the partly staged copy is removed"
        );
    }

    /// A home that cannot be written fails the install before the previous
    /// notices are touched.
    #[cfg(unix)]
    #[test]
    fn a_read_only_home_keeps_the_previous_notices() {
        use std::os::unix::fs::PermissionsExt;

        let update = Update::with_notices().after_an_earlier_update();
        let home = update.home.root.clone();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o555))
            .expect("Should make the home read-only");
        let probe = home.join("probe");
        let writable = std::fs::write(&probe, b"").is_ok();
        std::fs::remove_file(&probe).ok();

        let result = install_notices(&update.source(), &update.dest());
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755))
            .expect("Should restore the home's permissions");
        // A privileged process writes a 0o555 directory regardless.
        if writable {
            return;
        }

        result.expect_err("a read-only home fails the install");
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// A symbolic link among the archive's notices is refused rather than
    /// followed, and nothing is installed.
    #[cfg(unix)]
    #[test]
    fn a_symlink_among_the_notices_is_refused() {
        let update = Update::with_notices().after_an_earlier_update();
        std::os::unix::fs::symlink(
            repository_licenses().join("README.md"),
            update.source().join("link"),
        )
        .expect("Should create the symlink");

        let err =
            install_notices(&update.source(), &update.dest()).expect_err("a symlink is refused");

        assert!(
            format!("{err:#}").contains("is neither a file nor a directory"),
            "got {err:#}"
        );
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
        assert_eq!(update.home_entries(), ["downloads", "licenses"]);
    }

    /// The notices are in place, whole, by the time the binary is replaced.
    #[test]
    fn the_notices_are_installed_before_the_binary_is_replaced() {
        let update = Update::with_notices().after_an_earlier_update();
        let mut replaced = None;

        let installed = update
            .install(|binary| {
                assert_holds(&update.dest(), &archive_notices(), "the notices by then");
                replaced = Some(binary.to_path_buf());
                Ok(())
            })
            .expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_eq!(replaced, Some(update.extracted.join("infs")));
    }

    /// An update whose notices cannot be installed never replaces the binary,
    /// and says that infs was not updated.
    #[test]
    fn an_update_whose_notices_fail_keeps_the_running_binary() {
        let update = Update::without_notices().after_an_earlier_update();
        std::fs::write(update.source(), b"not a directory").expect("Should write");

        let err = update
            .install(|_| panic!("the binary must not be replaced when the notices fail"))
            .expect_err("the notices failure fails the update");

        assert_eq!(
            format!("{err:#}"),
            format!(
                "infs was not updated; it is still {RUNNING_VERSION}: The third-party notices at \
                 {} are not a directory",
                update.source().display()
            )
        );
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
    }

    /// An archive without the binary installs nothing, its notices included.
    #[test]
    fn an_archive_without_the_binary_installs_nothing() {
        let update = Update::with_notices().after_an_earlier_update();
        std::fs::remove_file(update.extracted.join("infs")).expect("Should remove");

        let err = update
            .install(|_| panic!("there is no binary to replace"))
            .expect_err("a missing binary fails the update");

        assert!(
            format!("{err:#}").contains(&format!(
                "infs binary not found in downloaded archive. Expected at {}",
                update.extracted.join("infs").display()
            )),
            "got {err:#}"
        );
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
    }

    /// An archive released before the notices shipped still updates the
    /// binary, and reports that it installed no notices.
    #[test]
    fn an_archive_without_notices_still_replaces_the_binary() {
        let update = Update::without_notices().after_an_earlier_update();
        let mut replaced = None;

        let installed = update
            .install(|binary| {
                replaced = Some(binary.to_path_buf());
                Ok(())
            })
            .expect("Should update");

        assert!(installed.is_empty(), "no notices are reported installed");
        assert_eq!(replaced, Some(update.extracted.join("infs")));
        assert_holds(&update.dest(), &previous_notices(), "the untouched notices");
    }

    /// A binary that cannot be replaced fails the update with that error as
    /// it is, the archive's notices already installed.
    #[test]
    fn a_failed_binary_replacement_fails_the_update() {
        let update = Update::with_notices();

        let err = update
            .install(|_| bail!("the binary is busy"))
            .expect_err("the replacement failure fails the update");

        assert_eq!(format!("{err:#}"), "the binary is busy");
        assert_holds(
            &update.dest(),
            &archive_notices(),
            "the notices installed before the replacement",
        );
    }

    /// Notices an earlier archive left beside the running binary, where the
    /// VS Code extension unpacks it, are refreshed byte for byte after the
    /// home's and before the binary is replaced, and both directories are
    /// reported.
    #[test]
    fn notices_beside_the_running_binary_are_refreshed_before_it_is_replaced() {
        let update = Update::with_notices()
            .after_an_earlier_update()
            .unpacked_into_bin();
        let mut replaced = None;

        let installed = update
            .install(|binary| {
                assert_holds(&update.dest(), &archive_notices(), "the home's notices");
                assert_holds(
                    &update.beside_binary(),
                    &archive_notices(),
                    "the notices beside the binary",
                );
                replaced = Some(binary.to_path_buf());
                Ok(())
            })
            .expect("Should update");

        assert_eq!(installed, [update.dest(), update.beside_binary()]);
        assert_eq!(replaced, Some(update.extracted.join("infs")));
        assert_eq!(
            names_in(&update.home.bin),
            ["infs", "licenses"],
            "neither a staged nor a set-aside copy is left beside the binary"
        );
    }

    /// A binary with no `licenses/` beside it gets none: the notices are
    /// installed in the home alone.
    #[test]
    fn no_notices_are_created_beside_a_binary_that_had_none() {
        let update = Update::with_notices();
        std::fs::create_dir_all(&update.home.bin).expect("Should create bin/");
        std::fs::write(&update.running_binary, b"old infs binary").expect("Should write");

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_eq!(
            names_in(&update.home.bin),
            ["infs"],
            "no notices appear beside the binary"
        );
    }

    /// A file named `licenses` beside the binary is not a notices directory:
    /// the update leaves it as it is.
    #[test]
    fn a_file_named_licenses_beside_the_binary_is_left_alone() {
        let update = Update::with_notices();
        std::fs::create_dir_all(&update.home.bin).expect("Should create bin/");
        std::fs::write(update.beside_binary(), b"not notices").expect("Should write");

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_eq!(
            std::fs::read(update.beside_binary()).expect("Should read"),
            b"not notices"
        );
    }

    /// A symbolic link named `licenses` beside the binary is not followed: the
    /// directory it points at is left as it is.
    #[cfg(unix)]
    #[test]
    fn a_link_named_licenses_beside_the_binary_is_not_followed() {
        let update = Update::with_notices();
        let elsewhere = update.home.root.with_file_name("elsewhere");
        write_entries(&elsewhere, &previous_notices());
        std::fs::create_dir_all(&update.home.bin).expect("Should create bin/");
        std::os::unix::fs::symlink(elsewhere.join("licenses"), update.beside_binary())
            .expect("Should create the symlink");

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_holds(
            &elsewhere.join("licenses"),
            &previous_notices(),
            "the notices the link points at",
        );
    }

    /// A binary running from the home itself has the home's `licenses/`
    /// beside it, and the notices are installed there once.
    #[test]
    fn a_binary_in_the_home_gets_the_notices_once() {
        let update = Update::with_notices()
            .after_an_earlier_update()
            .running_from_the_home();

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_holds(&update.dest(), &archive_notices(), "the home's notices");
    }

    /// The home's `licenses/`, reached through a symbolic link, is still the
    /// home's, and the notices are installed there once.
    #[cfg(unix)]
    #[test]
    fn a_binary_in_a_link_to_the_home_gets_the_notices_once() {
        let update = Update::with_notices()
            .after_an_earlier_update()
            .running_from_a_link_to_the_home();

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_holds(&update.dest(), &archive_notices(), "the home's notices");
    }

    /// A `licenses/` beside the binary holding none of the archive's notices
    /// is another program's, one sharing the binary's directory as in
    /// `/usr/local/bin`, even where its files share a name with a notice: the
    /// update leaves it as it is, does not report it, and still replaces the
    /// binary.
    #[test]
    fn another_programs_licenses_beside_the_binary_are_left_alone() {
        let theirs = [
            dir_entry("licenses"),
            dir_entry("licenses/other-tool"),
            file_entry("licenses/other-tool/LICENSE", b"their license\n"),
            file_entry("licenses/other-tool/NOTICE", b"their notice\n"),
        ];
        let update = Update::with_notices().with_licenses_in_bin(&theirs);
        let mut replaced = None;

        let installed = update
            .install(|binary| {
                replaced = Some(binary.to_path_buf());
                Ok(())
            })
            .expect("Should update");

        assert_eq!(
            installed,
            [update.dest()],
            "only the home's notices are reported"
        );
        assert_eq!(replaced, Some(update.extracted.join("infs")));
        assert_holds(&update.beside_binary(), &theirs, "their licenses");
        assert_holds(&update.dest(), &archive_notices(), "the home's notices");
        assert_eq!(names_in(&update.home.bin), ["infs", "licenses"]);
    }

    /// The `README.md` at the top of the notices is a name any directory of
    /// notices might use: a `licenses/` beside the binary holding one alone
    /// is not `infs`'s, and the update leaves it as it is.
    #[test]
    fn a_readme_alone_does_not_mark_licenses_beside_the_binary_as_infs() {
        let theirs = [
            dir_entry("licenses"),
            file_entry("licenses/README.md", b"their readme\n"),
        ];
        let update = Update::with_notices().with_licenses_in_bin(&theirs);

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_holds(&update.beside_binary(), &theirs, "their readme");
    }

    /// Only the `README.md` at the top of the notices is set aside: one the
    /// archive carries deeper down marks a `licenses/` beside the binary as
    /// `infs`'s like any other notice.
    #[test]
    fn a_readme_below_the_top_marks_licenses_beside_the_binary_as_infs() {
        let nested_readme = [
            dir_entry("licenses"),
            dir_entry("licenses/spacewasm"),
            file_entry("licenses/spacewasm/README.md", b"a readme\n"),
        ];
        let update = Update::with_notices();
        write_entries(&update.extracted, &nested_readme);
        let update = update.with_licenses_in_bin(&nested_readme);

        let notices_dirs = update.notices_dirs();

        assert_eq!(notices_dirs, [update.dest(), update.beside_binary()]);
    }

    /// An empty `licenses/` beside the binary holds none of the archive's
    /// notices, and the update leaves it as it is.
    #[test]
    fn an_empty_licenses_beside_the_binary_is_left_alone() {
        let empty = [dir_entry("licenses")];
        let update = Update::with_notices().with_licenses_in_bin(&empty);

        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(installed, [update.dest()]);
        assert_holds(&update.beside_binary(), &empty, "the empty licenses/");
    }

    /// A directory at the place of one of the archive's notices is not that
    /// notice, and does not mark a `licenses/` beside the binary as `infs`'s.
    #[test]
    fn a_directory_at_a_notice_path_beside_the_binary_does_not_count() {
        let theirs = [
            dir_entry("licenses"),
            dir_entry("licenses/spacewasm"),
            dir_entry("licenses/spacewasm/NOTICE"),
        ];
        let update = Update::with_notices().with_licenses_in_bin(&theirs);

        assert_eq!(update.notices_dirs(), [update.dest()]);
    }

    /// A symbolic link inside a `licenses/` beside the binary is not
    /// followed: a notice reached through a linked directory does not mark it
    /// as `infs`'s.
    #[cfg(unix)]
    #[test]
    fn a_notice_through_a_linked_directory_beside_the_binary_does_not_count() {
        let empty = [dir_entry("licenses")];
        let update = Update::with_notices().with_licenses_in_bin(&empty);
        let elsewhere = update.home.root.with_file_name("elsewhere");
        write_entries(&elsewhere, &previous_notices());
        std::os::unix::fs::symlink(
            elsewhere.join("licenses").join("spacewasm"),
            update.beside_binary().join("spacewasm"),
        )
        .expect("Should create the symlink");

        assert_eq!(update.notices_dirs(), [update.dest()]);
    }

    /// A symbolic link at the place of one of the archive's notices is not
    /// that notice, and does not mark a `licenses/` beside the binary as
    /// `infs`'s.
    #[cfg(unix)]
    #[test]
    fn a_link_at_a_notice_path_beside_the_binary_does_not_count() {
        let theirs = [
            dir_entry("licenses"),
            dir_entry("licenses/spacewasm"),
            file_entry("licenses/their-notice", b"their notice\n"),
        ];
        let update = Update::with_notices().with_licenses_in_bin(&theirs);
        std::os::unix::fs::symlink(
            update.beside_binary().join("their-notice"),
            update.beside_binary().join("spacewasm").join("NOTICE"),
        )
        .expect("Should create the symlink");

        assert_eq!(update.notices_dirs(), [update.dest()]);
    }

    /// An archive released before the notices shipped has none to recognise
    /// a `licenses/` beside the binary by: the update leaves one an earlier
    /// archive left there as it is, and reports no notices.
    #[test]
    fn an_archive_without_notices_leaves_the_notices_beside_the_binary_alone() {
        let update = Update::without_notices().unpacked_into_bin();

        let notices_dirs = update.notices_dirs();
        let installed = update.install(|_| Ok(())).expect("Should update");

        assert_eq!(notices_dirs, [update.dest()]);
        assert!(installed.is_empty(), "no notices are reported installed");
        assert_holds(
            &update.beside_binary(),
            &previous_notices(),
            "the notices beside the binary",
        );
    }

    /// A refresh beside the binary that fails aborts the update before the
    /// binary is replaced, saying that infs was not updated, and leaves the
    /// notices there as they were; the home's, installed first, stay.
    #[cfg(unix)]
    #[test]
    fn a_failed_refresh_beside_the_binary_keeps_the_running_binary() {
        use std::os::unix::fs::PermissionsExt;

        let update = Update::with_notices().unpacked_into_bin();
        let bin = &update.home.bin;
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o555))
            .expect("Should make bin/ read-only");
        let probe = bin.join("probe");
        let writable = std::fs::write(&probe, b"").is_ok();
        std::fs::remove_file(&probe).ok();

        let mut replaced = false;
        let result = update.install(|_| {
            replaced = true;
            Ok(())
        });
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755))
            .expect("Should restore the permissions of bin/");
        // A privileged process writes a 0o555 directory regardless.
        if writable {
            return;
        }

        let err = result.expect_err("the failed refresh fails the update");
        let message = format!("{err:#}");
        let expected = format!(
            "infs was not updated; it is still {RUNNING_VERSION}: Failed to stage the third-party \
             notices at {}: ",
            beside(&update.beside_binary(), "new").display()
        );
        assert!(message.starts_with(&expected), "got {message}");
        assert!(!replaced, "the binary must not be replaced");
        assert_holds(
            &update.beside_binary(),
            &previous_notices(),
            "the notices beside the binary",
        );
        assert_holds(&update.dest(), &archive_notices(), "the home's notices");
    }

    /// With no notices installed, the report says nothing about them.
    #[test]
    fn the_report_is_silent_about_notices_when_none_were_installed() {
        assert_eq!(
            update_report("9.9.9", &[], Platform::LinuxX64),
            ["Successfully updated infs to 9.9.9."]
        );
    }

    /// The report names the home's notices after the update.
    #[test]
    fn the_report_names_the_home_notices() {
        let dirs = [Path::new("home").join("licenses")];

        assert_eq!(
            update_report("9.9.9", &dirs, Platform::LinuxX64),
            [
                "Successfully updated infs to 9.9.9.".to_string(),
                format!("Third-party notices: {}", dirs[0].display()),
            ]
        );
    }

    /// The report names every directory the notices were installed at, one
    /// line each, in the order they were installed.
    #[test]
    fn the_report_names_every_directory_the_notices_were_installed_at() {
        let dirs = [
            Path::new("home").join("licenses"),
            Path::new("home").join("bin").join("licenses"),
        ];

        assert_eq!(
            update_report("9.9.9", &dirs, Platform::LinuxX64),
            [
                "Successfully updated infs to 9.9.9.".to_string(),
                format!("Third-party notices: {}", dirs[0].display()),
                format!("Third-party notices: {}", dirs[1].display()),
            ]
        );
    }

    /// On Windows the report ends with a reminder to restart the terminal.
    #[test]
    fn the_report_ends_with_a_restart_note_on_windows() {
        let dirs = [Path::new("home").join("licenses")];

        assert_eq!(
            update_report("9.9.9", &dirs, Platform::WindowsX64),
            [
                "Successfully updated infs to 9.9.9.".to_string(),
                format!("Third-party notices: {}", dirs[0].display()),
                "Note: Please restart your terminal to use the new version.".to_string(),
            ]
        );
    }
}

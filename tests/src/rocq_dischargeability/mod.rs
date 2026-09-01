use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};

mod cases;
#[cfg(test)]
mod direct;
mod pin;
mod protocol;

const SUCCESS_LINE: &str = "rocq-discharge: result=pass cases=5 proved=11 refuted=1";

pub fn export_cli(exchange: &Path) -> Result<()> {
    export(exchange)?;
    Ok(())
}

pub fn verify_cli(exchange: &Path) -> Result<()> {
    println!("{}", verify(exchange)?);
    Ok(())
}

fn export(exchange: &Path) -> Result<protocol::Request> {
    export_with_generator(exchange, |case| {
        Ok(
            crate::rocq_test_support::generate_v(case.source_name(), case.module_name())
                .into_bytes(),
        )
    })
}

fn export_with_generator<F>(exchange: &Path, mut generate: F) -> Result<protocol::Request>
where
    F: FnMut(&cases::CaseSpec) -> Result<Vec<u8>>,
{
    export_with_generator_and_hook(exchange, &mut generate, |_| Ok(()))
}

enum ExportEvent<'a> {
    StagingDirectoryReady(&'a Path),
    RawDirectoryCreated(&'a Path),
    RawArtifactWritten(&'a Path),
    RequestTemporaryWritten(&'a Path),
    PublishedRawDirectoryReady(&'a Path),
    RequestPublicationReady(&'a Path),
    PublishedRequestReady(&'a Path),
    FailurePreservationStarted(&'a Path),
    FailureEntryReported(&'a Path),
    FinalLayoutValidated(&'a Path),
}

fn export_with_generator_and_hook<F, H>(
    exchange: &Path,
    mut generate: F,
    mut hook: H,
) -> Result<protocol::Request>
where
    F: FnMut(&cases::CaseSpec) -> Result<Vec<u8>>,
    H: for<'a> FnMut(ExportEvent<'a>) -> Result<()>,
{
    let exchange = require_empty_exchange(exchange)?;
    let prepared = PreparedExport::generate(&mut generate)?;
    exchange.require_empty()?;
    let mut transaction = ExportTransaction::new(exchange, prepared)?;
    match transaction.publish(&mut hook) {
        Ok(()) => Ok(transaction.into_request()),
        Err(error) => {
            let incomplete = transaction.preservation_diagnostic(&mut hook);
            Err(error.context(incomplete))
        }
    }
}

fn verify(_exchange: &Path) -> Result<&'static str> {
    protocol::verify_exchange(_exchange)?;
    Ok(SUCCESS_LINE)
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tests crate has a repository parent")
        .to_path_buf()
}

fn require_empty_exchange(exchange: &Path) -> Result<ExchangeRoot> {
    if !exchange.is_absolute() {
        bail!("exchange path must be absolute");
    }
    let metadata = std::fs::symlink_metadata(exchange)
        .with_context(|| format!("inspect exchange directory {}", exchange.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "exchange path must be an existing nonsymlink directory: {}",
            exchange.display()
        );
    }
    let exchange = std::fs::canonicalize(exchange)
        .with_context(|| format!("canonicalize exchange directory {}", exchange.display()))?;
    let root = ExchangeRoot {
        identity: same_file::Handle::from_path(&exchange)
            .with_context(|| format!("open exchange identity {}", exchange.display()))?,
        path: exchange,
    };
    root.require_empty()?;
    Ok(root)
}

const INCOMPLETE_DIAGNOSTIC_LIMIT: usize = 1024;

struct ExchangeRoot {
    path: PathBuf,
    identity: same_file::Handle,
}

impl ExchangeRoot {
    fn path(&self) -> &Path {
        &self.path
    }

    fn ensure_identity(&self) -> Result<()> {
        ensure_identity(&self.path, &self.identity, EntryKind::Directory)
            .context("exchange root identity changed")
    }

    fn require_empty(&self) -> Result<()> {
        self.ensure_identity()?;
        let entries = directory_entry_names(&self.path)?;
        if !entries.is_empty() {
            bail!(
                "export exchange directory must be empty: {}",
                self.path.display()
            );
        }
        Ok(())
    }

    fn require_exact_entries(&self, paths: &[&Path]) -> Result<()> {
        self.ensure_identity()?;
        let expected = paths
            .iter()
            .map(|path| {
                path.file_name()
                    .context("exchange entry has no basename")
                    .map(ToOwned::to_owned)
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let observed = directory_entry_names(&self.path)?;
        if observed != expected {
            bail!(
                "export exchange entry set mismatch: expected {expected:?}, observed {observed:?}"
            );
        }
        Ok(())
    }
}

struct PreparedRaw {
    case: &'static cases::CaseSpec,
    bytes: Vec<u8>,
}

struct PreparedExport {
    raw: Vec<PreparedRaw>,
    request: protocol::Request,
    request_bytes: Vec<u8>,
}

impl PreparedExport {
    fn generate<F>(generate: &mut F) -> Result<Self>
    where
        F: FnMut(&cases::CaseSpec) -> Result<Vec<u8>>,
    {
        let repository = repository_root();
        let mut raw = Vec::with_capacity(cases::CASES.len());
        for case in cases::CASES {
            let generated = generate(case)
                .with_context(|| format!("fresh-generate Rocq artifact for `{}`", case.id()))?;
            let golden_path = repository.join(case.golden_path());
            let golden = std::fs::read(&golden_path)
                .with_context(|| format!("read committed golden {}", golden_path.display()))?;
            if generated != golden {
                bail!(
                    "fresh Rocq artifact for `{}` differs from committed golden {}",
                    case.id(),
                    golden_path.display()
                );
            }
            raw.push(PreparedRaw {
                case,
                bytes: generated,
            });
        }
        let request = protocol::Request::from_raw(
            &pin::Pin::read()?,
            raw.iter()
                .map(|raw| (raw.case, protocol::RawHash::of(&raw.bytes)))
                .collect(),
        );
        let request_bytes = protocol::serialize_request(&request)?;
        Ok(Self {
            raw,
            request,
            request_bytes,
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum EntryKind {
    File,
    Directory,
}

struct RawEvidence {
    basename: &'static str,
    identity: same_file::Handle,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PublicationState {
    Staging,
    RawPublished,
    RequestPublished,
    Committed,
}

/// Publishes into a fresh exchange exclusively owned by this invocation.
///
/// A publication error preserves every staged or published entry and poisons the exchange for
/// caller disposal. Concurrent writers and concurrent exchange-root replacement are outside this
/// contract; validation hooks ensure observed interference fails closed without cleanup.
struct ExportTransaction {
    exchange: ExchangeRoot,
    prepared: PreparedExport,
    raw_staging: PathBuf,
    raw_identity: same_file::Handle,
    raw_evidence: Vec<RawEvidence>,
    request_staging: Option<PathBuf>,
    request_identity: Option<same_file::Handle>,
    state: PublicationState,
}

impl ExportTransaction {
    fn new(exchange: ExchangeRoot, prepared: PreparedExport) -> Result<Self> {
        let (raw_staging, raw_identity) = create_unique_directory(exchange.path(), "raw")?;
        Ok(Self {
            exchange,
            prepared,
            raw_staging,
            raw_identity,
            raw_evidence: Vec::new(),
            request_staging: None,
            request_identity: None,
            state: PublicationState::Staging,
        })
    }

    fn publish<H>(&mut self, hook: &mut H) -> Result<()>
    where
        H: for<'a> FnMut(ExportEvent<'a>) -> Result<()>,
    {
        hook(ExportEvent::StagingDirectoryReady(&self.raw_staging))?;
        self.ensure_raw_staging_identity()?;
        hook(ExportEvent::RawDirectoryCreated(&self.raw_staging))?;
        self.ensure_raw_staging_identity()?;

        for raw in &self.prepared.raw {
            let path = self.raw_staging.join(raw.case.raw_basename());
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .with_context(|| format!("create staged raw artifact {}", path.display()))?;
            let identity =
                same_file::Handle::from_file(file.try_clone().with_context(|| {
                    format!("clone staged raw identity handle {}", path.display())
                })?)
                .with_context(|| format!("capture staged raw identity {}", path.display()))?;
            file.write_all(&raw.bytes)
                .with_context(|| format!("write staged raw artifact {}", path.display()))?;
            file.sync_all()
                .with_context(|| format!("sync staged raw artifact {}", path.display()))?;
            drop(file);
            self.raw_evidence.push(RawEvidence {
                basename: raw.case.raw_basename(),
                identity,
            });
            hook(ExportEvent::RawArtifactWritten(&path))?;
            let written = std::fs::read(&path)
                .with_context(|| format!("read staged raw artifact {}", path.display()))?;
            if written != raw.bytes {
                bail!("raw integrity failure while staging `{}`", raw.case.id());
            }
        }

        let (request_staging, mut request_file) =
            create_unique_file(self.exchange.path(), "request")?;
        self.request_staging = Some(request_staging.clone());
        self.request_identity = Some(
            same_file::Handle::from_file(request_file.try_clone().with_context(|| {
                format!("clone request staging handle {}", request_staging.display())
            })?)
            .with_context(|| {
                format!(
                    "capture request staging identity {}",
                    request_staging.display()
                )
            })?,
        );
        request_file
            .write_all(&self.prepared.request_bytes)
            .with_context(|| format!("write staged request {}", request_staging.display()))?;
        request_file
            .sync_all()
            .with_context(|| format!("sync staged request {}", request_staging.display()))?;
        drop(request_file);
        hook(ExportEvent::RequestTemporaryWritten(&request_staging))?;

        self.validate_staging()?;
        let final_raw = self.exchange.path().join("raw");
        atomic_rename_noreplace(&self.raw_staging, &final_raw).with_context(|| {
            format!(
                "atomically publish raw without clobbering {} as {}",
                self.raw_staging.display(),
                final_raw.display()
            )
        })?;
        self.state = PublicationState::RawPublished;
        hook(ExportEvent::PublishedRawDirectoryReady(&final_raw))?;
        self.validate_raw_published()?;

        let final_request = self.exchange.path().join("request.json");
        hook(ExportEvent::RequestPublicationReady(&final_request))?;
        atomic_rename_noreplace(&request_staging, &final_request).with_context(|| {
            format!(
                "atomically publish request without clobbering {} as {}",
                request_staging.display(),
                final_request.display()
            )
        })?;
        self.state = PublicationState::RequestPublished;
        hook(ExportEvent::PublishedRequestReady(&final_request))?;
        self.validate_committed()?;
        hook(ExportEvent::FinalLayoutValidated(self.exchange.path()))?;
        self.validate_committed()?;
        self.state = PublicationState::Committed;
        Ok(())
    }

    fn validate_staging(&self) -> Result<()> {
        let request = self
            .request_staging
            .as_deref()
            .context("request staging path was not recorded")?;
        self.exchange
            .require_exact_entries(&[&self.raw_staging, request])?;
        self.validate_raw_directory(&self.raw_staging)?;
        self.validate_request(request)
    }

    fn validate_raw_published(&self) -> Result<()> {
        let raw = self.exchange.path().join("raw");
        let request = self
            .request_staging
            .as_deref()
            .context("request staging path was not recorded")?;
        self.exchange.require_exact_entries(&[&raw, request])?;
        self.validate_raw_directory(&raw)?;
        self.validate_request(request)
    }

    fn validate_committed(&self) -> Result<()> {
        let raw = self.exchange.path().join("raw");
        let request = self.exchange.path().join("request.json");
        self.exchange.require_exact_entries(&[&raw, &request])?;
        self.validate_raw_directory(&raw)?;
        self.validate_request(&request)?;
        self.exchange.ensure_identity()
    }

    fn validate_raw_directory(&self, directory: &Path) -> Result<()> {
        ensure_identity(directory, &self.raw_identity, EntryKind::Directory)?;
        let expected = self
            .prepared
            .raw
            .iter()
            .map(|raw| std::ffi::OsString::from(raw.case.raw_basename()))
            .collect::<std::collections::BTreeSet<_>>();
        let observed = directory_entry_names(directory)?;
        if observed != expected {
            bail!(
                "raw publication entry set mismatch: expected {expected:?}, observed {observed:?}"
            );
        }
        for ((prepared, evidence), request_case) in self
            .prepared
            .raw
            .iter()
            .zip(&self.raw_evidence)
            .zip(self.prepared.request.cases())
        {
            if evidence.basename != prepared.case.raw_basename() {
                bail!("raw evidence order mismatch");
            }
            let path = directory.join(evidence.basename);
            ensure_identity(&path, &evidence.identity, EntryKind::File)?;
            let bytes = std::fs::read(&path)
                .with_context(|| format!("read exact raw publication {}", path.display()))?;
            if bytes != prepared.bytes
                || protocol::RawHash::of(&bytes).as_str() != request_case.raw_sha256()
            {
                bail!(
                    "raw publication bytes/hash mismatch for `{}`",
                    prepared.case.id()
                );
            }
        }
        Ok(())
    }

    fn validate_request(&self, path: &Path) -> Result<()> {
        ensure_identity(
            path,
            self.request_identity
                .as_ref()
                .context("request staging identity was not captured")?,
            EntryKind::File,
        )?;
        let bytes = std::fs::read(path)
            .with_context(|| format!("read exact request publication {}", path.display()))?;
        if bytes != self.prepared.request_bytes {
            bail!("request publication bytes mismatch");
        }
        Ok(())
    }

    fn ensure_raw_staging_identity(&self) -> Result<()> {
        ensure_identity(&self.raw_staging, &self.raw_identity, EntryKind::Directory)
    }

    fn preservation_diagnostic<H>(self, hook: &mut H) -> String
    where
        H: for<'a> FnMut(ExportEvent<'a>) -> Result<()>,
    {
        let raw = self.current_raw_path();
        let mut hook_error = hook(ExportEvent::FailurePreservationStarted(&raw)).err();
        for path in self.current_raw_artifact_paths() {
            if hook_error.is_none() {
                hook_error = hook(ExportEvent::FailureEntryReported(&path)).err();
            }
        }
        if let Some(request) = self.current_request_path()
            && hook_error.is_none()
        {
            hook_error = hook(ExportEvent::FailureEntryReported(&request)).err();
        }
        let mut diagnostic = self.incomplete_diagnostic();
        if let Some(error) = hook_error {
            diagnostic.push_str(&format!("; preservation reporting hook failed: {error:#}"));
        }
        truncate_diagnostic(diagnostic)
    }

    fn current_raw_path(&self) -> PathBuf {
        match self.state {
            PublicationState::Staging => self.raw_staging.clone(),
            PublicationState::RawPublished
            | PublicationState::RequestPublished
            | PublicationState::Committed => self.exchange.path().join("raw"),
        }
    }

    fn current_raw_artifact_paths(&self) -> Vec<PathBuf> {
        let directory = self.current_raw_path();
        self.prepared
            .raw
            .iter()
            .map(|raw| directory.join(raw.case.raw_basename()))
            .collect()
    }

    fn current_request_path(&self) -> Option<PathBuf> {
        match self.state {
            PublicationState::Staging | PublicationState::RawPublished => {
                self.request_staging.clone()
            }
            PublicationState::RequestPublished | PublicationState::Committed => {
                Some(self.exchange.path().join("request.json"))
            }
        }
    }

    fn incomplete_diagnostic(&self) -> String {
        let mut diagnostic = format!(
            "incomplete export preserved entries; discard exchange before retry: raw={}",
            self.current_raw_path().display()
        );
        if let Some(request) = self.current_request_path() {
            diagnostic.push_str(&format!(", request={}", request.display()));
        }
        truncate_diagnostic(diagnostic)
    }

    fn into_request(self) -> protocol::Request {
        self.prepared.request
    }
}

fn directory_entry_names(
    directory: &Path,
) -> Result<std::collections::BTreeSet<std::ffi::OsString>> {
    std::fs::read_dir(directory)
        .with_context(|| format!("read export directory {}", directory.display()))?
        .map(|entry| {
            entry
                .with_context(|| format!("read export entry in {}", directory.display()))
                .map(|entry| entry.file_name())
        })
        .collect()
}

fn ensure_identity(path: &Path, identity: &same_file::Handle, kind: EntryKind) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect export identity {}", path.display()))?;
    let file_type = metadata.file_type();
    let expected_type = !file_type.is_symlink()
        && match kind {
            EntryKind::File => file_type.is_file(),
            EntryKind::Directory => file_type.is_dir(),
        };
    if !expected_type {
        bail!("export entry type changed: {}", path.display());
    }
    let observed = same_file::Handle::from_path(path)
        .with_context(|| format!("open export identity {}", path.display()))?;
    if identity != &observed {
        bail!("export entry identity changed: {}", path.display());
    }
    Ok(())
}

fn create_unique_directory(exchange: &Path, purpose: &str) -> Result<(PathBuf, same_file::Handle)> {
    for _ in 0..16 {
        let path = unique_staging_path(exchange, purpose)?;
        match std::fs::create_dir(&path) {
            Ok(()) => {
                let identity = same_file::Handle::from_path(&path).with_context(|| {
                    truncate_diagnostic(format!(
                        "incomplete export preserved entries; discard exchange before retry: raw={}",
                        path.display()
                    ))
                })?;
                return Ok((path, identity));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create raw staging {}", path.display()));
            }
        }
    }
    bail!("could not allocate a unique raw staging name")
}

fn create_unique_file(exchange: &Path, purpose: &str) -> Result<(PathBuf, std::fs::File)> {
    for _ in 0..16 {
        let path = unique_staging_path(exchange, purpose)?;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create request staging {}", path.display()));
            }
        }
    }
    bail!("could not allocate a unique request staging name")
}

fn unique_staging_path(exchange: &Path, purpose: &str) -> Result<PathBuf> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| anyhow::anyhow!("obtain staging randomness: {error}"))?;
    let mut suffix = String::with_capacity(random.len() * 2);
    for byte in random {
        suffix.push(char::from(HEX[usize::from(byte >> 4)]));
        suffix.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(exchange.join(format!(".rocq-discharge-{purpose}-{suffix}")))
}

fn truncate_diagnostic(mut diagnostic: String) -> String {
    if diagnostic.len() <= INCOMPLETE_DIAGNOSTIC_LIMIT {
        return diagnostic;
    }
    let mut boundary = INCOMPLETE_DIAGNOSTIC_LIMIT - 3;
    while !diagnostic.is_char_boundary(boundary) {
        boundary -= 1;
    }
    diagnostic.truncate(boundary);
    diagnostic.push_str("...");
    diagnostic
}

#[cfg(target_os = "linux")]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let source = std::ffi::CString::new(source.as_os_str().as_bytes())
        .context("source path contains NUL")?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .context("destination path contains NUL")?;
    // renameat2 performs the source removal and no-clobber destination creation atomically.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("renameat2(RENAME_NOREPLACE)")
    }
}

#[cfg(target_os = "macos")]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let source = std::ffi::CString::new(source.as_os_str().as_bytes())
        .context("source path contains NUL")?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .context("destination path contains NUL")?;
    // RENAME_EXCL atomically moves the source only when the destination does not exist.
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("renamex_np(RENAME_EXCL)")
    }
}

#[cfg(windows)]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // Omitting MOVEFILE_REPLACE_EXISTING gives MoveFileExW no-clobber semantics.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            0,
        )
    };
    if result != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("MoveFileExW(no replace)")
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("Rocq discharge export requires an atomic no-replace move implementation");

#[cfg(test)]
mod tests {
    use super::cases::CASES;
    use super::{
        ExportEvent, atomic_rename_noreplace, export, export_cli, export_with_generator,
        export_with_generator_and_hook,
    };
    use std::collections::BTreeSet;
    use std::path::Path;

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("tests crate has a repository parent")
    }

    fn exchange_names(exchange: &Path) -> BTreeSet<String> {
        std::fs::read_dir(exchange)
            .expect("read exchange")
            .map(|entry| {
                entry
                    .expect("read exchange entry")
                    .file_name()
                    .into_string()
                    .expect("UTF-8 exchange entry")
            })
            .collect()
    }

    #[test]
    fn export_fresh_generates_golden_equal_raw_and_hashes_exact_written_bytes() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let request = export(exchange.path()).expect("export fresh artifacts");
        let expected_hashes = [
            "f6ea44c6d1eb9120c2982dbc2d5c1f863e5ab4d9eb426be56bc2cea50faa011f",
            "9e04be474dec8f07b734bd4b89c35919951355ea8cb003e3b5cd4bdbf0718446",
            "d5f3645b4c20e48375138001923b8a9e1cd5c6b6e461b39fea0268bfc2f95400",
            "ae92490a10f79700630fcd06f5d4e483e2792c6ec18271e3d4714312a330c611",
            "2889e852f5e99df59f58cd72d6242f2252f3de586e14b80bf6ce46b78906b978",
        ];

        let entries = exchange_names(exchange.path());
        assert_eq!(
            entries,
            BTreeSet::from(["raw".to_string(), "request.json".to_string()])
        );

        for ((case, request_case), expected_hash) in
            CASES.iter().zip(request.cases()).zip(expected_hashes)
        {
            let raw = std::fs::read(exchange.path().join("raw").join(case.raw_basename()))
                .expect("read exported raw");
            let golden = std::fs::read(repository_root().join(case.golden_path()))
                .expect("read committed golden");
            assert_eq!(raw, golden, "exported bytes drifted for {}", case.id());
            assert_eq!(request_case.case_id(), case.id());
            assert_eq!(request_case.raw_basename(), case.raw_basename());
            assert_eq!(request_case.raw_sha256(), expected_hash);
        }

        let request_json =
            std::fs::read_to_string(exchange.path().join("request.json")).expect("read request");
        assert!(!request_json.contains("theorem"));
        assert!(!request_json.contains("proof"));
        assert!(!exchange.path().join(".request.json.tmp").exists());
    }

    #[test]
    fn public_export_cli_uses_the_common_export_path() {
        let exchange = tempfile::tempdir().expect("create exchange");
        export_cli(exchange.path()).expect("export through public facade");
        assert!(exchange.path().join("request.json").is_file());
        assert_eq!(
            std::fs::read_dir(exchange.path().join("raw"))
                .expect("read raw directory")
                .count(),
            5
        );
    }

    #[test]
    fn export_compares_fresh_bytes_before_publishing_request() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let error = export_with_generator(exchange.path(), |case| {
            let mut bytes = std::fs::read(repository_root().join(case.golden_path()))?;
            if case.id() == "prime-bounded" {
                bytes.push(b' ');
            }
            Ok(bytes)
        })
        .expect_err("golden-different fresh bytes must fail export");

        assert!(
            error.to_string().contains("golden"),
            "unexpected error: {error:#}"
        );
        assert!(
            !exchange.path().join("request.json").exists(),
            "request must be published last"
        );
        assert_eq!(
            exchange_names(exchange.path()),
            BTreeSet::new(),
            "failed export left owned artifacts behind"
        );
    }

    #[test]
    fn failed_mid_matrix_generation_never_starts_publication() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let error = export_with_generator(exchange.path(), |case| {
            if case.id() == "unique" {
                anyhow::bail!("injected mid-matrix generation failure");
            }
            Ok(std::fs::read(repository_root().join(case.golden_path()))?)
        })
        .expect_err("mid-matrix generation failure must fail export");

        assert!(
            format!("{error:#}").contains("injected mid-matrix"),
            "unexpected error: {error:#}"
        );
        assert_eq!(
            exchange_names(exchange.path()),
            BTreeSet::new(),
            "generation failed before staging but changed the exchange"
        );
    }

    #[test]
    fn failed_request_publication_preserves_staging_and_a_foreign_directory() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let injected_request = exchange.path().join("request.json");
        let error = export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RequestPublicationReady(_) = event {
                    std::fs::create_dir(&injected_request)
                        .expect("inject foreign request directory");
                    std::fs::write(injected_request.join("sentinel"), b"foreign")
                        .expect("write foreign sentinel");
                }
                Ok(())
            },
        )
        .expect_err("injected request publication failure must fail export");

        assert!(
            format!("{error:#}").contains("atomically publish request"),
            "unexpected error: {error:#}"
        );
        assert!(
            format!("{error:#}").contains("incomplete export preserved entries"),
            "unexpected error: {error:#}"
        );
        assert!(exchange.path().join("raw").is_dir());
        assert!(
            exchange_names(exchange.path())
                .iter()
                .any(|name| name.starts_with(".rocq-discharge-request-")),
            "failed request publication did not preserve its staging file"
        );
        assert_eq!(
            std::fs::read(injected_request.join("sentinel")).expect("read foreign sentinel"),
            b"foreign",
            "failed publication removed or changed a foreign entry"
        );
    }

    #[test]
    fn failure_preserves_a_foreign_raw_directory_replacement() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let moved_owned = outside.path().join("moved-owned-raw");
        let outside_sentinel = outside.path().join("sentinel");
        std::fs::write(&outside_sentinel, b"outside").expect("write outside sentinel");
        let mut replacement = None;

        let error = export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RawDirectoryCreated(path) = event {
                    std::fs::rename(path, &moved_owned).expect("move owned raw outside exchange");
                    std::fs::create_dir(path).expect("install foreign raw directory");
                    replacement = Some(path.to_path_buf());
                    anyhow::bail!("injected raw-directory substitution");
                }
                Ok(())
            },
        )
        .expect_err("raw-directory substitution must fail export");

        assert!(
            format!("{error:#}").contains("injected raw-directory substitution"),
            "unexpected error: {error:#}"
        );
        assert!(
            replacement.expect("record replacement path").is_dir(),
            "failure handling deleted a foreign raw-directory replacement"
        );
        assert!(
            moved_owned.is_dir(),
            "failure handling deleted an owned object moved outside the exchange"
        );
        assert_eq!(
            std::fs::read(&outside_sentinel).expect("read outside sentinel"),
            b"outside",
            "failure handling changed data outside the exchange"
        );
    }

    #[cfg(unix)]
    #[test]
    fn failure_preserves_foreign_raw_file_and_symlink_replacements() {
        for replacement_kind in ["file", "symlink"] {
            let exchange = tempfile::tempdir().expect("create exchange");
            let outside = tempfile::tempdir().expect("create outside directory");
            let moved_owned = outside.path().join("moved-owned-raw");
            let outside_sentinel = outside.path().join("sentinel");
            std::fs::write(&outside_sentinel, b"outside").expect("write outside sentinel");
            let mut replacement = None;

            export_with_generator_and_hook(
                exchange.path(),
                |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
                |event| {
                    if let ExportEvent::RawDirectoryCreated(path) = event {
                        std::fs::rename(path, &moved_owned)
                            .expect("move owned raw outside exchange");
                        match replacement_kind {
                            "file" => {
                                std::fs::write(path, b"foreign raw replacement")
                                    .expect("install foreign raw file");
                            }
                            "symlink" => {
                                std::os::unix::fs::symlink(&outside_sentinel, path)
                                    .expect("install foreign raw symlink");
                            }
                            _ => unreachable!(),
                        }
                        replacement = Some(path.to_path_buf());
                        anyhow::bail!("injected raw substitution");
                    }
                    Ok(())
                },
            )
            .expect_err("raw substitution must fail export");

            let replacement = replacement.expect("record replacement path");
            let metadata =
                std::fs::symlink_metadata(&replacement).expect("inspect foreign raw replacement");
            assert_eq!(
                (metadata.is_file(), metadata.file_type().is_symlink()),
                (replacement_kind == "file", replacement_kind == "symlink"),
                "failure handling deleted a foreign {replacement_kind} raw replacement"
            );
            assert_eq!(
                std::fs::read(&outside_sentinel).expect("read outside sentinel"),
                b"outside",
                "failure handling followed a foreign raw symlink outside the exchange"
            );
        }
    }

    #[test]
    fn failure_preserves_owned_raw_renamed_inside_exchange_and_its_replacement() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let moved_owned = exchange.path().join("moved-owned-raw");
        let mut replacement = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RawDirectoryCreated(path) = event {
                    std::fs::rename(path, &moved_owned).expect("rename owned raw inside exchange");
                    std::fs::create_dir(path).expect("install foreign raw directory");
                    replacement = Some(path.to_path_buf());
                    anyhow::bail!("injected in-exchange raw-directory substitution");
                }
                Ok(())
            },
        )
        .expect_err("raw-directory substitution must fail export");

        assert!(
            replacement.expect("record replacement path").is_dir(),
            "failure handling deleted a foreign raw-directory replacement"
        );
        assert!(
            moved_owned.is_dir(),
            "failure handling deleted a still-reachable owned raw directory"
        );
    }

    #[test]
    fn failure_preserves_a_foreign_request_temporary_file_replacement() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let moved_owned = outside.path().join("moved-owned-request-temp");
        let mut replacement = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RequestTemporaryWritten(path) = event {
                    std::fs::rename(path, &moved_owned)
                        .expect("move owned request temporary outside exchange");
                    std::fs::write(path, b"foreign temporary")
                        .expect("install foreign request temporary");
                    replacement = Some(path.to_path_buf());
                    anyhow::bail!("injected request-temporary substitution");
                }
                Ok(())
            },
        )
        .expect_err("request-temporary substitution must fail export");

        let replacement = replacement.expect("record replacement path");
        assert_eq!(
            std::fs::read(replacement).expect("read foreign request temporary"),
            b"foreign temporary",
            "failure handling deleted or changed a foreign request-temporary replacement"
        );
        assert!(
            moved_owned.is_file(),
            "failure handling deleted an owned object moved outside the exchange"
        );
    }

    #[test]
    fn failure_preserves_a_foreign_request_temporary_directory_replacement() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let moved_owned = outside.path().join("moved-owned-request-temp");
        let mut replacement = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RequestTemporaryWritten(path) = event {
                    std::fs::rename(path, &moved_owned)
                        .expect("move owned request temporary outside exchange");
                    std::fs::create_dir(path).expect("install foreign request-temporary directory");
                    replacement = Some(path.to_path_buf());
                    anyhow::bail!("injected request-temporary directory substitution");
                }
                Ok(())
            },
        )
        .expect_err("request-temporary directory substitution must fail export");

        assert!(
            replacement.expect("record replacement path").is_dir(),
            "failure handling deleted a foreign request-temporary directory"
        );
        assert!(
            moved_owned.is_file(),
            "failure handling deleted an owned object moved outside the exchange"
        );
    }

    #[cfg(unix)]
    #[test]
    fn failure_never_follows_or_deletes_a_foreign_request_temporary_symlink() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let moved_owned = outside.path().join("moved-owned-request-temp");
        let outside_sentinel = outside.path().join("sentinel");
        std::fs::write(&outside_sentinel, b"outside").expect("write outside sentinel");
        let mut replacement = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RequestTemporaryWritten(path) = event {
                    std::fs::rename(path, &moved_owned)
                        .expect("move owned request temporary outside exchange");
                    std::os::unix::fs::symlink(&outside_sentinel, path)
                        .expect("install foreign request-temporary symlink");
                    replacement = Some(path.to_path_buf());
                    anyhow::bail!("injected request-temporary symlink substitution");
                }
                Ok(())
            },
        )
        .expect_err("request-temporary symlink substitution must fail export");

        assert!(
            std::fs::symlink_metadata(replacement.expect("record replacement path"))
                .expect("inspect foreign request-temporary symlink")
                .file_type()
                .is_symlink(),
            "failure handling deleted a foreign request-temporary symlink"
        );
        assert_eq!(
            std::fs::read(&outside_sentinel).expect("read outside sentinel"),
            b"outside",
            "failure handling followed a foreign symlink outside the exchange"
        );
    }

    #[test]
    fn request_publication_never_overwrites_a_foreign_regular_file() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let foreign_request = exchange.path().join("request.json");

        let error = export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::RequestPublicationReady(_) = event {
                    std::fs::write(&foreign_request, b"foreign request")
                        .expect("inject foreign regular request");
                }
                Ok(())
            },
        )
        .expect_err("foreign regular request must prevent publication");

        assert_eq!(
            std::fs::read(&foreign_request).expect("read foreign request"),
            b"foreign request",
            "request publication overwrote a foreign regular file"
        );
        assert!(
            exchange.path().join("raw").is_dir(),
            "failed no-clobber publication deleted already-published raw artifacts"
        );
        assert_eq!(
            std::fs::read_dir(exchange.path().join("raw"))
                .expect("read preserved raw")
                .count(),
            CASES.len(),
            "failed no-clobber publication changed preserved raw artifacts"
        );
        assert!(
            exchange_names(exchange.path())
                .iter()
                .any(|name| name.starts_with(".rocq-discharge-request-")),
            "failed no-clobber publication did not preserve the request staging file"
        );
        let diagnostic = format!("{error:#}");
        assert!(
            diagnostic.contains("incomplete export preserved entries"),
            "unexpected error: {diagnostic}"
        );
        assert!(
            diagnostic.len() <= super::INCOMPLETE_DIAGNOSTIC_LIMIT + 512,
            "public failure diagnostic was not bounded: {} bytes",
            diagnostic.len()
        );
    }

    #[cfg(unix)]
    #[test]
    fn race_failure_reporting_never_traverses_an_outside_symlink() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let outside_owned = outside.path().join("owned-raw.v");
        let outside_sentinel = outside.path().join("sentinel");
        let moved_staging = exchange.path().join("moved-owned-staging");
        std::fs::write(&outside_sentinel, b"outside").expect("write outside sentinel");
        let mut generation_failed = false;
        let mut traversal_race_injected = false;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                match event {
                    ExportEvent::RawArtifactWritten(path) if !generation_failed => {
                        std::fs::rename(path, &outside_owned)
                            .expect("move an owned raw artifact outside");
                        generation_failed = true;
                        anyhow::bail!("injected failure before preservation reporting");
                    }
                    ExportEvent::FailurePreservationStarted(path)
                        if !traversal_race_injected
                            && path.file_name().is_some_and(|name| {
                                name.to_string_lossy().starts_with(".rocq-")
                            }) =>
                    {
                        std::fs::rename(path, &moved_staging)
                            .expect("move inspected staging directory");
                        std::os::unix::fs::symlink(outside.path(), path)
                            .expect("replace inspected directory with outside symlink");
                        traversal_race_injected = true;
                    }
                    _ => {}
                }
                Ok(())
            },
        )
        .expect_err("injected generation failure must fail export");

        assert!(
            traversal_race_injected,
            "test did not reach the failure-reporting mutation seam"
        );
        assert!(
            outside_owned.is_file(),
            "failure reporting traversed the substituted symlink and deleted outside data"
        );
        assert_eq!(
            std::fs::read(&outside_sentinel).expect("read outside sentinel"),
            b"outside",
            "failure reporting changed unrelated outside data"
        );
    }

    #[test]
    fn race_failure_reporting_never_deletes_a_replacement() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let outside_owned = outside.path().join("owned-raw.v");
        let mut generation_failed = false;
        let mut foreign_replacement = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                match event {
                    ExportEvent::RawArtifactWritten(_) if !generation_failed => {
                        generation_failed = true;
                        anyhow::bail!("injected failure before preservation reporting");
                    }
                    ExportEvent::FailureEntryReported(path)
                        if foreign_replacement.is_none()
                            && path
                                .file_name()
                                .is_some_and(|name| name == CASES[0].raw_basename()) =>
                    {
                        std::fs::rename(path, &outside_owned)
                            .expect("move validated owned artifact outside");
                        std::fs::write(path, b"foreign replacement")
                            .expect("install foreign replacement after validation");
                        foreign_replacement = Some(path.to_path_buf());
                    }
                    _ => {}
                }
                Ok(())
            },
        )
        .expect_err("injected generation failure must fail export");

        assert_eq!(
            std::fs::read(foreign_replacement.expect("reach failure-reporting mutation seam"))
                .expect("read foreign replacement"),
            b"foreign replacement",
            "failure reporting deleted a foreign replacement"
        );
        assert!(
            outside_owned.is_file(),
            "failure reporting deleted an owned artifact moved outside"
        );
    }

    #[test]
    fn race_staging_substitution_after_identity_capture_is_rejected() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let outside_owned = outside.path().join("owned-staging");
        let mut foreign_staging = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::StagingDirectoryReady(path) = event {
                    std::fs::rename(path, &outside_owned)
                        .expect("move newly created staging directory");
                    std::fs::create_dir(path).expect("install foreign staging directory");
                    foreign_staging = Some(path.to_path_buf());
                }
                Ok(())
            },
        )
        .expect_err("substituted staging must never produce a successful export");

        assert!(outside_owned.is_dir(), "owned staging outside was deleted");
        assert!(
            foreign_staging
                .expect("record foreign staging path")
                .is_dir(),
            "foreign staging replacement was deleted"
        );
    }

    #[test]
    fn race_published_raw_substitution_before_validation_is_rejected() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let outside_owned = outside.path().join("owned-published-raw");
        let mut foreign_raw = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::PublishedRawDirectoryReady(path) = event {
                    std::fs::rename(path, &outside_owned).expect("move newly published raw");
                    std::fs::create_dir(path).expect("install foreign published raw directory");
                    foreign_raw = Some(path.to_path_buf());
                }
                Ok(())
            },
        )
        .expect_err("substituted published raw must never produce a successful export");

        assert!(outside_owned.is_dir(), "owned published raw was deleted");
        assert!(
            foreign_raw.expect("record foreign raw path").is_dir(),
            "foreign published raw replacement was deleted"
        );
    }

    #[test]
    fn race_published_request_substitution_before_validation_is_rejected() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let outside_owned = outside.path().join("owned-request.json");
        let mut foreign_request = None;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::PublishedRequestReady(path) = event {
                    std::fs::rename(path, &outside_owned).expect("move newly published request");
                    std::fs::write(path, b"foreign request")
                        .expect("install foreign published request");
                    foreign_request = Some(path.to_path_buf());
                }
                Ok(())
            },
        )
        .expect_err("substituted request must never produce a successful export");

        assert!(
            outside_owned.is_file(),
            "owned published request was deleted"
        );
        assert_eq!(
            std::fs::read(foreign_request.expect("record foreign request path"))
                .expect("read foreign request"),
            b"foreign request",
            "foreign published request was deleted or changed"
        );
    }

    #[test]
    fn race_mutation_after_final_validation_can_never_be_certified() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let outside = tempfile::tempdir().expect("create outside directory");
        let outside_owned = outside.path().join("owned-request.json");
        let mut validation_seam_reached = false;

        export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                if let ExportEvent::FinalLayoutValidated(path) = event {
                    let request = path.join("request.json");
                    std::fs::rename(&request, &outside_owned)
                        .expect("move validated request outside");
                    std::fs::write(&request, b"foreign request")
                        .expect("substitute request after validation");
                    validation_seam_reached = true;
                }
                Ok(())
            },
        )
        .expect_err("mutation after validation must not be certified");

        assert!(
            validation_seam_reached,
            "test did not reach final-validation race seam"
        );
        assert!(outside_owned.is_file(), "validated request was deleted");
        assert_eq!(
            std::fs::read(exchange.path().join("request.json")).expect("read foreign request"),
            b"foreign request",
            "foreign request substituted after validation was deleted"
        );
    }

    #[test]
    fn failure_preservation_is_reported_only_once() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let mut injected_failure = false;
        let mut preservation_reports = 0;
        let mut preserved_stage = None;

        let error = export_with_generator_and_hook(
            exchange.path(),
            |case| Ok(std::fs::read(repository_root().join(case.golden_path()))?),
            |event| {
                match event {
                    ExportEvent::RawArtifactWritten(path) if !injected_failure => {
                        injected_failure = true;
                        preserved_stage = path.parent().map(Path::to_path_buf);
                        anyhow::bail!("injected export failure");
                    }
                    ExportEvent::FailurePreservationStarted(_) => preservation_reports += 1,
                    _ => {}
                }
                Ok(())
            },
        )
        .expect_err("injected failure must fail export");

        assert_eq!(
            preservation_reports, 1,
            "failure preservation was reported more than once"
        );
        assert!(
            preserved_stage.expect("record staged directory").is_dir(),
            "failure reporting must preserve staged output"
        );
        assert!(
            format!("{error:#}").contains("incomplete export preserved entries"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn atomic_no_replace_move_consumes_only_an_uncontested_source() {
        let directory = tempfile::tempdir().expect("create atomic-move directory");
        let first_source = directory.path().join("first-source");
        let target = directory.path().join("target");
        std::fs::write(&first_source, b"first").expect("write first source");

        atomic_rename_noreplace(&first_source, &target).expect("publish uncontested source");
        assert!(!first_source.exists());
        assert_eq!(std::fs::read(&target).expect("read target"), b"first");

        let second_source = directory.path().join("second-source");
        std::fs::write(&second_source, b"second").expect("write second source");
        atomic_rename_noreplace(&second_source, &target)
            .expect_err("existing target must reject no-replace move");
        assert_eq!(
            std::fs::read(&second_source).expect("read preserved source"),
            b"second"
        );
        assert_eq!(
            std::fs::read(&target).expect("read preserved target"),
            b"first"
        );
    }

    #[test]
    fn export_requires_an_existing_empty_absolute_nonsymlink_directory() {
        let relative_error = export(Path::new("relative-exchange"))
            .expect_err("relative exchange must fail")
            .to_string();
        assert!(
            relative_error.contains("absolute"),
            "unexpected error: {relative_error}"
        );

        let missing_root = tempfile::tempdir().expect("create missing parent");
        let missing = missing_root.path().join("missing");
        let missing_error = export(&missing)
            .expect_err("missing exchange must fail")
            .to_string();
        assert!(
            missing_error.contains("inspect"),
            "unexpected error: {missing_error}"
        );

        let nonempty = tempfile::tempdir().expect("create nonempty exchange");
        std::fs::write(nonempty.path().join("stale"), b"stale").expect("write stale entry");
        let nonempty_error = export(nonempty.path())
            .expect_err("nonempty exchange must fail")
            .to_string();
        assert!(
            nonempty_error.contains("empty"),
            "unexpected error: {nonempty_error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn export_rejects_a_symlink_exchange_root() {
        let parent = tempfile::tempdir().expect("create symlink parent");
        let target = parent.path().join("target");
        let link = parent.path().join("exchange");
        std::fs::create_dir(&target).expect("create symlink target");
        std::os::unix::fs::symlink(&target, &link).expect("create exchange symlink");

        let error = export(&link)
            .expect_err("symlink exchange must fail")
            .to_string();
        assert!(error.contains("nonsymlink"), "unexpected error: {error}");
    }

    #[cfg(unix)]
    #[test]
    fn stabilized_exchange_root_keeps_export_inside_original_target() {
        let parent = tempfile::tempdir().expect("create exchange parent");
        let original_parent = parent.path().join("original-parent");
        let injected_parent = parent.path().join("injected-parent");
        let ancestor = parent.path().join("ancestor");
        let original_exchange = original_parent.join("exchange");
        let injected_exchange = injected_parent.join("exchange");
        std::fs::create_dir_all(&original_exchange).expect("create original exchange");
        std::fs::create_dir_all(&injected_exchange).expect("create injected exchange");
        std::os::unix::fs::symlink(&original_parent, &ancestor).expect("create ancestor symlink");
        let caller_exchange = ancestor.join("exchange");
        let mut retargeted = false;

        export_with_generator(&caller_exchange, |case| {
            if !retargeted {
                std::fs::remove_file(&ancestor).expect("remove original ancestor symlink");
                std::os::unix::fs::symlink(&injected_parent, &ancestor)
                    .expect("retarget ancestor symlink");
                retargeted = true;
            }
            Ok(std::fs::read(repository_root().join(case.golden_path()))?)
        })
        .expect("export through a stabilized exchange root");

        assert!(original_exchange.join("request.json").is_file());
        assert_eq!(
            std::fs::read_dir(&original_exchange)
                .expect("read original exchange")
                .count(),
            2
        );
        assert_eq!(
            std::fs::read_dir(&injected_exchange)
                .expect("read injected exchange")
                .count(),
            0,
            "retargeted ancestor received export entries"
        );
    }
}

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    RawDirectoryCreated(&'a Path),
    RawArtifactWritten(&'a Path),
    RequestTemporaryWritten(&'a Path),
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
    let mut transaction = ExportTransaction::new(exchange)?;
    let result = (|| {
        let raw_dir = transaction.create_raw_dir()?;
        hook(ExportEvent::RawDirectoryCreated(&raw_dir))?;
        let repository = repository_root();
        let mut raw_cases = Vec::with_capacity(cases::CASES.len());

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

            let raw_path = raw_dir.join(case.raw_basename());
            let mut file = transaction.create_raw_file(&raw_path)?;
            file.write_all(&generated)
                .with_context(|| format!("write raw artifact {}", raw_path.display()))?;
            file.sync_all()
                .with_context(|| format!("sync raw artifact {}", raw_path.display()))?;
            drop(file);
            let written = std::fs::read(&raw_path)
                .with_context(|| format!("read written raw artifact {}", raw_path.display()))?;
            if written != generated {
                bail!("raw integrity failure while exporting `{}`", case.id());
            }
            hook(ExportEvent::RawArtifactWritten(&raw_path))?;
            raw_cases.push((case, protocol::RawHash::of(&written)));
        }

        let request = protocol::Request::from_raw(&pin::Pin::read()?, raw_cases);
        transaction.publish_request(&protocol::serialize_request(&request)?, &mut hook)?;
        transaction.commit();
        Ok(request)
    })();

    match result {
        Ok(request) => Ok(request),
        Err(error) => match transaction.rollback() {
            Ok(()) => Err(error),
            Err(rollback) => {
                Err(error.context(format!("export rollback incomplete: {rollback:#}")))
            }
        },
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

fn require_empty_exchange(exchange: &Path) -> Result<PathBuf> {
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
    let mut entries = std::fs::read_dir(&exchange)
        .with_context(|| format!("read exchange directory {}", exchange.display()))?;
    if entries
        .next()
        .transpose()
        .with_context(|| format!("read entry in exchange directory {}", exchange.display()))?
        .is_some()
    {
        bail!(
            "export exchange directory must be empty: {}",
            exchange.display()
        );
    }
    Ok(exchange)
}

static NEXT_EXPORT_TRANSACTION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnedKind {
    File,
    Directory,
}

#[derive(Debug)]
struct OwnedEntry {
    path: PathBuf,
    identity: same_file::Handle,
    kind: OwnedKind,
    staging: bool,
}

impl OwnedEntry {
    fn capture(path: PathBuf, kind: OwnedKind, staging: bool) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("inspect newly created export entry {}", path.display()))?;
        if !kind.matches(&metadata) {
            bail!(
                "newly created export entry has unexpected type: {}",
                path.display()
            );
        }
        Ok(Self {
            identity: same_file::Handle::from_path(&path).with_context(|| {
                format!("open identity handle for export entry {}", path.display())
            })?,
            path,
            kind,
            staging,
        })
    }

    fn from_file(path: PathBuf, file: &std::fs::File, staging: bool) -> Result<Self> {
        let metadata = file
            .metadata()
            .with_context(|| format!("inspect newly created export file {}", path.display()))?;
        if !metadata.is_file() {
            bail!(
                "newly created export file has unexpected type: {}",
                path.display()
            );
        }
        Ok(Self {
            identity: same_file::Handle::from_file(file.try_clone().with_context(|| {
                format!("clone identity handle for export file {}", path.display())
            })?)
            .with_context(|| format!("read identity for export file {}", path.display()))?,
            path,
            kind: OwnedKind::File,
            staging,
        })
    }

    fn matches(&self, path: &Path, metadata: &std::fs::Metadata) -> Result<bool> {
        if !self.kind.matches(metadata) {
            return Ok(false);
        }
        let candidate = match same_file::Handle::from_path(path) {
            Ok(candidate) => candidate,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("open identity handle for {}", path.display()));
            }
        };
        Ok(self.identity == candidate)
    }
}

impl OwnedKind {
    fn matches(self, metadata: &std::fs::Metadata) -> bool {
        let file_type = metadata.file_type();
        !file_type.is_symlink()
            && match self {
                Self::File => file_type.is_file(),
                Self::Directory => file_type.is_dir(),
            }
    }
}

struct ExportTransaction {
    exchange: PathBuf,
    staging: PathBuf,
    staging_raw_files: Vec<PathBuf>,
    owned: Vec<OwnedEntry>,
    finished: bool,
}

impl ExportTransaction {
    fn new(exchange: PathBuf) -> Result<Self> {
        let sequence = NEXT_EXPORT_TRANSACTION.fetch_add(1, Ordering::Relaxed);
        let staging = exchange.join(format!(
            ".rocq-discharge-export-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&staging)
            .with_context(|| format!("create unique export staging {}", staging.display()))?;
        let staging_owner = OwnedEntry::capture(staging.clone(), OwnedKind::Directory, true)?;
        Ok(Self {
            exchange,
            staging,
            staging_raw_files: Vec::new(),
            owned: vec![staging_owner],
            finished: false,
        })
    }

    fn create_raw_dir(&mut self) -> Result<PathBuf> {
        let raw_dir = self.staging.join("raw");
        std::fs::create_dir(&raw_dir)
            .with_context(|| format!("create raw directory {}", raw_dir.display()))?;
        self.owned.push(OwnedEntry::capture(
            raw_dir.clone(),
            OwnedKind::Directory,
            true,
        )?);
        Ok(raw_dir)
    }

    fn create_raw_file(&mut self, path: &Path) -> Result<std::fs::File> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create raw artifact {}", path.display()))?;
        self.owned
            .push(OwnedEntry::from_file(path.to_path_buf(), &file, true)?);
        self.staging_raw_files.push(path.to_path_buf());
        Ok(file)
    }

    fn publish_request<H>(&mut self, bytes: &[u8], hook: &mut H) -> Result<()>
    where
        H: for<'a> FnMut(ExportEvent<'a>) -> Result<()>,
    {
        let temporary = self.staging.join("request.json.tmp");
        let final_path = self.exchange.join("request.json");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create temporary request {}", temporary.display()))?;
        self.owned
            .push(OwnedEntry::from_file(temporary.clone(), &file, true)?);
        file.write_all(bytes)
            .with_context(|| format!("write temporary request {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("sync temporary request {}", temporary.display()))?;
        drop(file);
        hook(ExportEvent::RequestTemporaryWritten(&temporary))?;

        let final_raw = self.exchange.join("raw");
        std::fs::create_dir(&final_raw)
            .with_context(|| format!("create published raw directory {}", final_raw.display()))?;
        self.owned.push(OwnedEntry::capture(
            final_raw.clone(),
            OwnedKind::Directory,
            false,
        )?);
        for staging_path in &self.staging_raw_files {
            let basename = staging_path
                .file_name()
                .context("staged raw artifact has no basename")?;
            let published_path = final_raw.join(basename);
            std::fs::hard_link(staging_path, &published_path).with_context(|| {
                format!(
                    "publish raw artifact {} as {}",
                    staging_path.display(),
                    published_path.display()
                )
            })?;
            self.owned
                .push(OwnedEntry::capture(published_path, OwnedKind::File, false)?);
        }

        std::fs::hard_link(&temporary, &final_path).with_context(|| {
            format!(
                "atomically publish request without clobbering {} as {}",
                temporary.display(),
                final_path.display()
            )
        })?;
        self.owned
            .push(OwnedEntry::capture(final_path, OwnedKind::File, false)?);
        self.remove_staging_entries()?;
        Ok(())
    }

    fn remove_staging_entries(&mut self) -> Result<()> {
        for entry in self.owned.iter().filter(|entry| entry.staging).rev() {
            remove_exact_owned(entry)?;
        }
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        let mut observed = Vec::new();
        collect_exchange_entries(&self.exchange, &mut observed)?;
        let mut failures = Vec::new();
        for (path, metadata) in observed {
            let owner = match self.matching_owner(&path, &metadata) {
                Ok(owner) => owner,
                Err(error) => {
                    failures.push(format!("{}: {error:#}", path.display()));
                    continue;
                }
            };
            if let Some(owner) = owner
                && let Err(error) = remove_if_still_owned(&path, owner)
            {
                failures.push(format!("{}: {error:#}", path.display()));
            }
        }
        if failures.is_empty() {
            self.finished = true;
            Ok(())
        } else {
            bail!("{}", failures.join("; "))
        }
    }

    fn matching_owner(
        &self,
        path: &Path,
        metadata: &std::fs::Metadata,
    ) -> Result<Option<&OwnedEntry>> {
        if metadata.file_type().is_symlink() {
            return Ok(None);
        }
        let candidate = same_file::Handle::from_path(path)
            .with_context(|| format!("open rollback identity handle for {}", path.display()))?;
        Ok(self
            .owned
            .iter()
            .find(|owner| owner.kind.matches(metadata) && owner.identity == candidate))
    }

    fn commit(&mut self) {
        self.finished = true;
    }
}

impl Drop for ExportTransaction {
    fn drop(&mut self) {
        let _ = self.rollback();
    }
}

fn collect_exchange_entries(
    directory: &Path,
    entries: &mut Vec<(PathBuf, std::fs::Metadata)>,
) -> Result<()> {
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("scan export entry directory {}", directory.display()))?
    {
        let path = entry
            .with_context(|| format!("read export entry in {}", directory.display()))?
            .path();
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("inspect export rollback candidate {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_exchange_entries(&path, entries)?;
        }
        entries.push((path, metadata));
    }
    Ok(())
}

fn remove_exact_owned(entry: &OwnedEntry) -> Result<()> {
    let metadata = std::fs::symlink_metadata(&entry.path)
        .with_context(|| format!("inspect owned staging entry {}", entry.path.display()))?;
    if !entry.matches(&entry.path, &metadata)? {
        bail!(
            "owned staging entry was replaced; refusing to remove {}",
            entry.path.display()
        );
    }
    remove_owned_path(&entry.path, entry.kind)
}

fn remove_if_still_owned(path: &Path, owner: &OwnedEntry) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reinspect rollback entry {}", path.display()));
        }
    };
    if !owner.matches(path, &metadata)? {
        return Ok(());
    }
    remove_owned_path(path, owner.kind)
}

fn remove_owned_path(path: &Path, kind: OwnedKind) -> Result<()> {
    match kind {
        OwnedKind::File => std::fs::remove_file(path),
        OwnedKind::Directory => std::fs::remove_dir(path),
    }
    .with_context(|| format!("remove owned export entry {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::cases::CASES;
    use super::{
        ExportEvent, export, export_cli, export_with_generator, export_with_generator_and_hook,
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
    fn failed_mid_matrix_generation_restores_the_empty_exchange() {
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
            "failed export left partial raw artifacts behind"
        );
        export(exchange.path()).expect("empty rollback result must be immediately retryable");
    }

    #[test]
    fn failed_request_publication_removes_owned_staging_but_preserves_foreign_entries() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let injected_request = exchange.path().join("request.json");
        let error = export_with_generator(exchange.path(), |case| {
            if case.id() == "false-spec" {
                std::fs::create_dir(&injected_request).expect("inject foreign request directory");
                std::fs::write(injected_request.join("sentinel"), b"foreign")
                    .expect("write foreign sentinel");
            }
            Ok(std::fs::read(repository_root().join(case.golden_path()))?)
        })
        .expect_err("injected request publication failure must fail export");

        assert!(
            format!("{error:#}").contains("atomically publish request"),
            "unexpected error: {error:#}"
        );
        assert!(!exchange.path().join("raw").exists());
        assert!(!exchange.path().join(".request.json.tmp").exists());
        assert_eq!(
            std::fs::read(injected_request.join("sentinel")).expect("read foreign sentinel"),
            b"foreign",
            "rollback removed or changed a foreign entry"
        );
    }

    #[test]
    fn rollback_preserves_a_foreign_raw_directory_replacement() {
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
            "rollback deleted a foreign raw-directory replacement"
        );
        assert!(
            moved_owned.is_dir(),
            "rollback must not discover and delete an owned object moved outside the exchange"
        );
        assert_eq!(
            std::fs::read(&outside_sentinel).expect("read outside sentinel"),
            b"outside",
            "rollback changed data outside the exchange"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rollback_preserves_foreign_raw_file_and_symlink_replacements() {
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
                "rollback deleted a foreign {replacement_kind} raw replacement"
            );
            assert_eq!(
                std::fs::read(&outside_sentinel).expect("read outside sentinel"),
                b"outside",
                "rollback followed a foreign raw symlink outside the exchange"
            );
        }
    }

    #[test]
    fn rollback_finds_owned_raw_renamed_inside_exchange_without_deleting_replacement() {
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
            "rollback deleted a foreign raw-directory replacement"
        );
        assert!(
            !moved_owned.exists(),
            "rollback leaked a still-reachable owned raw directory"
        );
    }

    #[test]
    fn rollback_preserves_a_foreign_request_temporary_file_replacement() {
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
            "rollback deleted or changed a foreign request-temporary replacement"
        );
        assert!(
            moved_owned.is_file(),
            "rollback must not delete an owned object moved outside the exchange"
        );
    }

    #[test]
    fn rollback_preserves_a_foreign_request_temporary_directory_replacement() {
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
            "rollback deleted a foreign request-temporary directory"
        );
        assert!(
            moved_owned.is_file(),
            "rollback must not delete an owned object moved outside the exchange"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rollback_never_follows_or_deletes_a_foreign_request_temporary_symlink() {
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
            "rollback deleted a foreign request-temporary symlink"
        );
        assert_eq!(
            std::fs::read(&outside_sentinel).expect("read outside sentinel"),
            b"outside",
            "rollback followed a foreign symlink outside the exchange"
        );
    }

    #[test]
    fn request_publication_never_overwrites_a_foreign_regular_file() {
        let exchange = tempfile::tempdir().expect("create exchange");
        let foreign_request = exchange.path().join("request.json");

        export_with_generator(exchange.path(), |case| {
            if case.id() == "false-spec" {
                std::fs::write(&foreign_request, b"foreign request")
                    .expect("inject foreign regular request");
            }
            Ok(std::fs::read(repository_root().join(case.golden_path()))?)
        })
        .expect_err("foreign regular request must prevent publication");

        assert_eq!(
            std::fs::read(&foreign_request).expect("read foreign request"),
            b"foreign request",
            "request publication overwrote a foreign regular file"
        );
        assert!(
            !exchange.path().join("raw").exists(),
            "failed no-clobber publication left owned raw artifacts"
        );
        assert_eq!(
            exchange_names(exchange.path()),
            BTreeSet::from(["request.json".to_string()]),
            "failed no-clobber publication removed or added foreign entries"
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

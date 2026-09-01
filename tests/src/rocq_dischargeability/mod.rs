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
    let exchange = require_empty_exchange(exchange)?;
    let mut transaction = ExportTransaction::new(exchange);
    let raw_dir = transaction.create_raw_dir()?;
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
        raw_cases.push((case, protocol::RawHash::of(&written)));
    }

    let request = protocol::Request::from_raw(&pin::Pin::read()?, raw_cases);
    transaction.publish_request(&protocol::serialize_request(&request)?)?;
    transaction.commit();
    Ok(request)
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

struct ExportTransaction {
    exchange: PathBuf,
    raw_dir: Option<PathBuf>,
    raw_files: Vec<PathBuf>,
    request_temporary: Option<PathBuf>,
    committed: bool,
}

impl ExportTransaction {
    fn new(exchange: PathBuf) -> Self {
        Self {
            exchange,
            raw_dir: None,
            raw_files: Vec::new(),
            request_temporary: None,
            committed: false,
        }
    }

    fn create_raw_dir(&mut self) -> Result<PathBuf> {
        let raw_dir = self.exchange.join("raw");
        std::fs::create_dir(&raw_dir)
            .with_context(|| format!("create raw directory {}", raw_dir.display()))?;
        self.raw_dir = Some(raw_dir.clone());
        Ok(raw_dir)
    }

    fn create_raw_file(&mut self, path: &Path) -> Result<std::fs::File> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create raw artifact {}", path.display()))?;
        self.raw_files.push(path.to_path_buf());
        Ok(file)
    }

    fn publish_request(&mut self, bytes: &[u8]) -> Result<()> {
        let temporary = self.exchange.join(".request.json.tmp");
        let final_path = self.exchange.join("request.json");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create temporary request {}", temporary.display()))?;
        self.request_temporary = Some(temporary.clone());
        file.write_all(bytes)
            .with_context(|| format!("write temporary request {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("sync temporary request {}", temporary.display()))?;
        drop(file);
        std::fs::rename(&temporary, &final_path).with_context(|| {
            format!(
                "atomically publish request {} as {}",
                temporary.display(),
                final_path.display()
            )
        })?;
        self.request_temporary = None;
        Ok(())
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for ExportTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(temporary) = self.request_temporary.take() {
            let _ = std::fs::remove_file(temporary);
        }
        for raw_file in self.raw_files.drain(..).rev() {
            let _ = std::fs::remove_file(raw_file);
        }
        if let Some(raw_dir) = self.raw_dir.take() {
            let _ = std::fs::remove_dir(raw_dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cases::CASES;
    use super::{export, export_cli, export_with_generator};
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

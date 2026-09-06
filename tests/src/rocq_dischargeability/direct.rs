use super::SUCCESS_LINE;
use super::protocol::{RawHash, Request, RequestCase};
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DISCHARGER_ENV: &str = "INFERENCE_WASM_VERIFIER_DISCHARGER";
const REQUIRED_ENV: &str = "INFERENCE_ROCQ_DISCHARGE_REQUIRED";
const DIAGNOSTIC_TRUNCATION_MARKER: &str = "...";
const CONFIGURED_GATE_FAILURE_PREFIX: &str = "configured Rocq dischargeability gate failed: ";
/// Maximum UTF-8 bytes in the complete application payload, excluding panic-hook framing.
const CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT: usize = 1_024;
const EVIDENCE_ENV: &str = "INFERENCE_WASM_VERIFIER_EVIDENCE_DIR";
const RECEIPT_ENV: &str = "INFERENCE_WASM_VERIFIER_RECEIPT_DIR";
const EVIDENCE_LOCATOR_PATH_LIMIT: usize = 200;

#[derive(Clone, Copy, Eq, PartialEq)]
enum InvocationKind {
    ProvenanceProbe,
    Case,
}

struct Invocation<'a> {
    executable: &'a Path,
    verifier_revision: &'a str,
    case_id: &'a str,
    raw_file: &'a Path,
    receipt_dir: &'a Path,
    kind: InvocationKind,
}

struct RunOutput {
    success: bool,
    code: Option<i32>,
    stderr: String,
    evidence: Option<PathBuf>,
}

trait Runner {
    fn run(&mut self, invocation: &Invocation<'_>) -> Result<RunOutput>;
}

struct ProcessRunner {
    environment: Vec<(OsString, OsString)>,
}

impl ProcessRunner {
    fn with_environment(environment: Vec<(OsString, OsString)>) -> Self {
        Self { environment }
    }
}

struct EvidenceDirectory {
    temporary: Option<tempfile::TempDir>,
    path: PathBuf,
    directory_identity: same_file::Handle,
    capture: File,
    capture_identity: same_file::Handle,
}

impl EvidenceDirectory {
    fn create() -> Result<Self> {
        let temporary = canonical_tempdir(
            "inference-rocq-evidence.",
            "create configured discharger evidence directory",
        )?;
        let initialized = (|| {
            let path = std::fs::canonicalize(temporary.path())
                .context("canonicalize configured discharger evidence directory")?;
            validate_evidence_locator_path(&path)?;
            let directory_identity = same_file::Handle::from_path(&path)
                .context("identify configured discharger evidence directory")?;
            validate_directory_metadata(&path)?;

            let capture_path = path.join("bridge-output.log");
            let mut options = OpenOptions::new();
            options.read(true).append(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let capture = options
                .open(&capture_path)
                .context("create configured discharger bridge output capture")?;
            let capture_identity = same_file::Handle::from_file(
                capture
                    .try_clone()
                    .context("clone bridge output capture for identity")?,
            )
            .context("identify configured discharger bridge output capture")?;
            validate_evidence_file(&capture_path)?;
            validate_exact_evidence_entries(&path, true)?;
            Ok::<_, anyhow::Error>((path, directory_identity, capture, capture_identity))
        })();
        let (path, directory_identity, capture, capture_identity) = match initialized {
            Ok(initialized) => initialized,
            Err(error) => {
                temporary.close().with_context(|| {
                    format!("clean evidence after initialization error: {error:#}")
                })?;
                return Err(error);
            }
        };

        Ok(Self {
            temporary: Some(temporary),
            path,
            directory_identity,
            capture,
            capture_identity,
        })
    }

    fn capture_writer(&self) -> Result<File> {
        self.capture
            .try_clone()
            .context("clone configured discharger bridge output descriptor")
    }

    fn validate(&self, success: bool) -> Result<()> {
        self.validate_root_and_capture()?;
        validate_exact_evidence_entries(&self.path, success)?;
        if !success {
            validate_evidence_file(&self.path.join("verifier.log"))?;
        }
        Ok(())
    }

    fn finish(mut self, success: bool, retain: bool) -> Result<Option<PathBuf>> {
        if let Err(error) = self.validate(success) {
            if retain && self.validate_retainable_invalid().is_ok() {
                let path = self.path.clone();
                let temporary = self
                    .temporary
                    .take()
                    .context("configured discharger evidence directory already finalized")?;
                let _ = temporary.keep();
                return Err(error).context(format!(
                    "configured discharger returned invalid private evidence; {}",
                    evidence_locator(&path)
                ));
            }
            if self.directory_identity_is_current() {
                self.close()?;
                return Err(error)
                    .context("removed invalid configured discharger evidence without a locator");
            }
            self.abandon();
            return Err(error)
                .context("configured discharger evidence identity is unsafe; locator unavailable");
        }
        if retain {
            let temporary = self
                .temporary
                .take()
                .context("configured discharger evidence directory already finalized")?;
            let _ = temporary.keep();
            Ok(Some(self.path.clone()))
        } else {
            self.close()?;
            Ok(None)
        }
    }

    fn discard(mut self) -> Result<()> {
        if !self.directory_identity_is_current() {
            self.abandon();
            bail!("configured discharger evidence identity is unsafe; locator unavailable");
        }
        self.close()
    }

    fn close(mut self) -> Result<()> {
        let path = self.path.clone();
        let temporary = self
            .temporary
            .take()
            .context("configured discharger evidence directory already finalized")?;
        drop(self);
        temporary.close().with_context(|| {
            format!(
                "remove configured discharger evidence; {}",
                evidence_locator(&path)
            )
        })?;
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("confirm configured discharger evidence removal"),
            Ok(_) => bail!(
                "configured discharger evidence remains after cleanup; {}",
                evidence_locator(&path)
            ),
        }
    }

    fn abandon(&mut self) {
        if let Some(temporary) = self.temporary.take() {
            let _ = temporary.keep();
        }
    }

    fn directory_identity_is_current(&self) -> bool {
        validate_directory_metadata(&self.path).is_ok()
            && same_file::Handle::from_path(&self.path)
                .is_ok_and(|current| current == self.directory_identity)
    }

    fn validate_root_and_capture(&self) -> Result<()> {
        validate_directory_metadata(&self.path)?;
        let current_directory = same_file::Handle::from_path(&self.path)
            .context("re-identify configured discharger evidence directory")?;
        if current_directory != self.directory_identity {
            bail!("configured discharger evidence directory identity changed");
        }
        let capture_path = self.path.join("bridge-output.log");
        validate_evidence_file(&capture_path)?;
        let current_capture = same_file::Handle::from_path(&capture_path)
            .context("re-identify configured discharger bridge output capture")?;
        if current_capture != self.capture_identity {
            bail!("configured discharger bridge output capture identity changed");
        }
        Ok(())
    }

    fn validate_retainable_invalid(&self) -> Result<()> {
        self.validate_root_and_capture()?;
        let mut names = evidence_entry_names(&self.path)?;
        names.sort();
        if names == ["bridge-output.log"] {
            return Ok(());
        }
        if names == ["bridge-output.log", "verifier.log"] {
            return validate_evidence_file(&self.path.join("verifier.log"));
        }
        bail!("invalid configured discharger evidence is not a safe exact subset")
    }
}

fn canonical_tempdir(prefix: &str, purpose: &str) -> Result<tempfile::TempDir> {
    let temporary_root = std::fs::canonicalize(std::env::temp_dir())
        .with_context(|| format!("canonicalize temporary root to {purpose}"))?;
    let temporary = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(&temporary_root)
        .with_context(|| purpose.to_string())?;
    let resolved = std::fs::canonicalize(temporary.path())
        .with_context(|| format!("canonicalize temporary directory to {purpose}"))?;
    if resolved != temporary.path() {
        temporary
            .close()
            .with_context(|| format!("remove noncanonical temporary directory for {purpose}"))?;
        bail!("temporary directory for {purpose} is not canonical");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let private = std::fs::Permissions::from_mode(0o700);
        if let Err(error) = std::fs::set_permissions(temporary.path(), private)
            .with_context(|| format!("set temporary directory mode to {purpose}"))
            .and_then(|()| {
                let metadata = std::fs::symlink_metadata(temporary.path())
                    .with_context(|| format!("inspect temporary directory mode to {purpose}"))?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_dir()
                    || metadata.permissions().mode() & 0o7777 != 0o700
                {
                    bail!("temporary directory for {purpose} is not an exact 0700 directory");
                }
                Ok(())
            })
        {
            temporary.close().with_context(|| {
                format!("remove temporary directory with unsafe mode for {purpose}")
            })?;
            return Err(error);
        }
    }
    Ok(temporary)
}

fn validate_evidence_locator_path(path: &Path) -> Result<()> {
    let rendered = path
        .to_str()
        .context("configured discharger evidence path is not UTF-8")?;
    if rendered.len() > EVIDENCE_LOCATOR_PATH_LIMIT {
        bail!("configured discharger evidence path exceeds the locator limit");
    }
    if rendered.chars().any(char::is_control) {
        bail!("configured discharger evidence path contains a control character");
    }
    Ok(())
}

fn validate_directory_metadata(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect evidence directory {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("configured discharger evidence path is not a nonsymlink directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o7777 != 0o700 {
            bail!("configured discharger evidence directory mode is not 0700");
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if metadata.uid() != unsafe { libc::geteuid() } {
            bail!("configured discharger evidence directory owner changed");
        }
    }
    Ok(())
}

fn validate_evidence_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect evidence file {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("configured discharger evidence file is not nonsymlink regular data");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o7777 != 0o600 || metadata.nlink() != 1 {
            bail!("configured discharger evidence file mode/link contract mismatch");
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if metadata.uid() != unsafe { libc::geteuid() } {
            bail!("configured discharger evidence file owner changed");
        }
    }
    Ok(())
}

fn validate_exact_evidence_entries(path: &Path, success: bool) -> Result<()> {
    let mut names = evidence_entry_names(path)?;
    names.sort();
    let expected: &[&str] = if success {
        &["bridge-output.log"]
    } else {
        &["bridge-output.log", "verifier.log"]
    };
    if names.len() != expected.len()
        || !names
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual == expected)
    {
        bail!("configured discharger evidence directory has unexpected entries");
    }
    Ok(())
}

fn evidence_entry_names(path: &Path) -> Result<Vec<OsString>> {
    std::fs::read_dir(path)
        .with_context(|| format!("read evidence directory {}", path.display()))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("read evidence entry in {}", path.display()))
}

impl Runner for ProcessRunner {
    fn run(&mut self, invocation: &Invocation<'_>) -> Result<RunOutput> {
        let evidence = EvidenceDirectory::create()?;
        let stdout = match evidence.capture_writer() {
            Ok(stdout) => stdout,
            Err(error) => {
                evidence.discard()?;
                return Err(error);
            }
        };
        let stderr = match evidence.capture_writer() {
            Ok(stderr) => stderr,
            Err(error) => {
                evidence.discard()?;
                return Err(error);
            }
        };
        let child = Command::new(invocation.executable)
            .arg("--protocol")
            .arg("1")
            .arg("--wasm-verifier-revision")
            .arg(invocation.verifier_revision)
            .arg("--case")
            .arg(invocation.case_id)
            .arg(invocation.raw_file)
            .envs(self.environment.iter().cloned())
            .env(RECEIPT_ENV, invocation.receipt_dir)
            .env(EVIDENCE_ENV, &evidence.path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                evidence.discard()?;
                return Err(error).with_context(|| {
                    format!(
                        "execute configured discharger for case `{}`",
                        invocation.case_id
                    )
                });
            }
        };
        let status = child.wait();
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                evidence.discard()?;
                return Err(error).with_context(|| {
                    format!(
                        "wait for configured discharger case `{}`",
                        invocation.case_id
                    )
                });
            }
        };
        let retain = !status.success() && invocation.kind == InvocationKind::Case;
        let retained_path = evidence.finish(status.success(), retain)?;
        Ok(RunOutput {
            success: status.success(),
            code: status.code(),
            stderr: if status.success() {
                "no diagnostic".to_string()
            } else {
                "private verifier diagnostic retained".to_string()
            },
            evidence: retained_path,
        })
    }
}

fn run_from_configuration(
    configured: Option<OsString>,
    required: bool,
    output: &mut dyn Write,
) -> Result<()> {
    let Some(configured) = configured else {
        if required {
            bail!("required dischargeability gate has no `{DISCHARGER_ENV}` executable");
        }
        writeln!(
            output,
            "rocq-discharge: SKIPPED `{DISCHARGER_ENV}` is not configured"
        )
        .context("write dischargeability skip marker")?;
        return Ok(());
    };

    let mut runner = ProcessRunner::with_environment(Vec::new());
    let summary = run_gate_with(Path::new(&configured), &mut runner)?;
    writeln!(output, "{summary}").context("write dischargeability success marker")
}

fn run_gate_with<R: Runner>(executable: &Path, runner: &mut R) -> Result<&'static str> {
    let exchange = canonical_tempdir(
        "inference-rocq-exchange.",
        "create direct discharge exchange",
    )?;
    let request = super::export(exchange.path()).context("export fresh direct artifacts")?;
    run_provenance_probe(executable, runner, exchange.path(), &request)?;

    let trusted_receipts = exchange.path().join("receipts");
    std::fs::create_dir(&trusted_receipts).with_context(|| {
        format!(
            "create trusted receipt directory {}",
            trusted_receipts.display()
        )
    })?;
    for case in request.cases() {
        run_case(
            executable,
            runner,
            exchange.path(),
            &request,
            case,
            &trusted_receipts,
        )?;
    }
    super::verify(exchange.path())
}

fn run_provenance_probe<R: Runner>(
    executable: &Path,
    runner: &mut R,
    exchange: &Path,
    request: &Request,
) -> Result<()> {
    let case = request
        .cases()
        .first()
        .context("discharge request has no provenance-probe case")?;
    let probe_files =
        canonical_tempdir("inference-rocq-probe.", "create provenance probe directory")?;
    let raw_path = exchange.join("raw").join(case.raw_basename());
    let mut malformed = std::fs::read(&raw_path)
        .with_context(|| format!("read probe source {}", raw_path.display()))?;
    let first = malformed
        .first_mut()
        .context("cannot byte-mutate an empty raw artifact")?;
    *first ^= 1;
    let probe_path = probe_files.path().join(case.raw_basename());
    std::fs::write(&probe_path, &malformed)
        .with_context(|| format!("write malformed provenance probe {}", probe_path.display()))?;
    let receipt_dir = canonical_tempdir(
        "inference-rocq-probe-receipts.",
        "create probe receipt directory",
    )?;
    require_empty_receipt_dir(receipt_dir.path())?;
    let invocation = Invocation {
        executable,
        verifier_revision: request.wasm_verifier_revision(),
        case_id: case.case_id(),
        raw_file: &probe_path,
        receipt_dir: receipt_dir.path(),
        kind: InvocationKind::ProvenanceProbe,
    };
    let output = run_with_raw_integrity(runner, &invocation, None)?;
    if output.success {
        bail!("malformed same-basename provenance probe was accepted");
    }
    Ok(())
}

fn run_case<R: Runner>(
    executable: &Path,
    runner: &mut R,
    exchange: &Path,
    request: &Request,
    case: &RequestCase,
    trusted_receipts: &Path,
) -> Result<()> {
    let invocation_receipts = canonical_tempdir(
        "inference-rocq-receipts.",
        &format!("create receipt directory for `{}`", case.case_id()),
    )?;
    require_empty_receipt_dir(invocation_receipts.path())?;
    let raw_file = exchange.join("raw").join(case.raw_basename());
    let invocation = Invocation {
        executable,
        verifier_revision: request.wasm_verifier_revision(),
        case_id: case.case_id(),
        raw_file: &raw_file,
        receipt_dir: invocation_receipts.path(),
        kind: InvocationKind::Case,
    };
    let output = run_with_raw_integrity(runner, &invocation, Some(case.raw_sha256()))?;
    if !output.success {
        let locator = output
            .evidence
            .as_deref()
            .map(evidence_locator)
            .unwrap_or_else(|| "evidence=unavailable".to_string());
        bail!(
            "discharger case `{}` exited {:?}; {locator}; diagnostic={}",
            case.case_id(),
            output.code,
            output.stderr
        );
    }

    let receipt_path = single_receipt(invocation_receipts.path(), case.case_id())?;
    let destination = trusted_receipts.join(format!("{}.json", case.case_id()));
    let mut source = std::fs::File::open(&receipt_path)
        .with_context(|| format!("open receipt {}", receipt_path.display()))?;
    let mut target = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .with_context(|| format!("create trusted receipt {}", destination.display()))?;
    std::io::copy(&mut source, &mut target)
        .with_context(|| format!("copy receipt for `{}`", case.case_id()))?;
    target
        .sync_all()
        .with_context(|| format!("sync trusted receipt {}", destination.display()))?;
    Ok(())
}

fn run_with_raw_integrity<R: Runner>(
    runner: &mut R,
    invocation: &Invocation<'_>,
    expected_hash: Option<&str>,
) -> Result<RunOutput> {
    let before = file_hash(invocation.raw_file)?;
    if let Some(expected_hash) = expected_hash
        && before.as_str() != expected_hash
    {
        bail!(
            "raw integrity failure before discharging `{}`",
            invocation.case_id
        );
    }
    let output = runner.run(invocation)?;
    let after = file_hash(invocation.raw_file)?;
    if before != after {
        let locator = output
            .evidence
            .as_deref()
            .map(evidence_locator)
            .unwrap_or_else(|| "evidence=unavailable".to_string());
        bail!(
            "raw integrity failure while discharging `{}`; {locator}",
            invocation.case_id,
        );
    }
    Ok(output)
}

fn evidence_locator(path: &Path) -> String {
    let rendered = path
        .to_str()
        .expect("validated evidence locator path became non-UTF-8");
    debug_assert!(rendered.len() <= EVIDENCE_LOCATOR_PATH_LIMIT);
    debug_assert!(!rendered.chars().any(char::is_control));
    format!("evidence={rendered}")
}

fn file_hash(path: &Path) -> Result<RawHash> {
    let bytes = std::fs::read(path).with_context(|| format!("read raw file {}", path.display()))?;
    Ok(RawHash::of(&bytes))
}

fn require_empty_receipt_dir(directory: &Path) -> Result<()> {
    if !directory.is_absolute() {
        bail!("receipt directory must be absolute");
    }
    let metadata = std::fs::symlink_metadata(directory)
        .with_context(|| format!("inspect receipt directory {}", directory.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("receipt directory must be a nonsymlink directory");
    }
    if std::fs::read_dir(directory)
        .with_context(|| format!("read receipt directory {}", directory.display()))?
        .next()
        .transpose()
        .with_context(|| format!("read receipt entry in {}", directory.display()))?
        .is_some()
    {
        bail!("receipt directory must be empty before invocation");
    }
    Ok(())
}

fn single_receipt(directory: &Path, case_id: &str) -> Result<PathBuf> {
    let expected_name = format!("{case_id}.json");
    let mut entries = std::fs::read_dir(directory)
        .with_context(|| format!("read receipt directory {}", directory.display()))?;
    let Some(entry) = entries
        .next()
        .transpose()
        .with_context(|| format!("read receipt entry in {}", directory.display()))?
    else {
        bail!("receipt directory has no receipt for `{case_id}`");
    };
    if entries
        .next()
        .transpose()
        .with_context(|| format!("read extra receipt entry in {}", directory.display()))?
        .is_some()
    {
        bail!("receipt directory has additional entries for `{case_id}`");
    }
    if entry.file_name() != expected_name.as_str() {
        bail!("receipt directory has the wrong receipt name for `{case_id}`");
    }
    let path = entry.path();
    let metadata = std::fs::symlink_metadata(&path)
        .with_context(|| format!("inspect receipt {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("receipt must be a nonsymlink regular file for `{case_id}`");
    }
    Ok(path)
}

fn bounded_sanitized_diagnostic(prefix: &str, rendered: &str, limit: usize) -> String {
    assert!(prefix.len() + DIAGNOSTIC_TRUNCATION_MARKER.len() <= limit);
    assert!(prefix.chars().all(|character| !character.is_control()));

    let mut diagnostic = String::with_capacity(limit);
    diagnostic.push_str(prefix);
    for character in rendered.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if diagnostic.len() + character.len_utf8() <= limit {
            diagnostic.push(character);
            continue;
        }

        let content_limit = limit - DIAGNOSTIC_TRUNCATION_MARKER.len();
        while diagnostic.len() > content_limit {
            diagnostic.pop();
        }
        diagnostic.push_str(DIAGNOSTIC_TRUNCATION_MARKER);
        return diagnostic;
    }
    diagnostic
}

fn configured_gate_panic_payload(error: &anyhow::Error) -> String {
    bounded_sanitized_diagnostic(
        CONFIGURED_GATE_FAILURE_PREFIX,
        &format!("{error:#}"),
        CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT,
    )
}

fn panic_configured_gate(error: &anyhow::Error) -> ! {
    panic!("{}", configured_gate_panic_payload(error));
}

#[test]
fn configured_dischargeability_gate() {
    let configured = std::env::var_os(DISCHARGER_ENV);
    let required = std::env::var_os(REQUIRED_ENV).is_some();
    if let Err(error) = run_from_configuration(configured, required, &mut std::io::stdout().lock())
    {
        panic_configured_gate(&error);
    }
}

#[cfg(test)]
mod tests {
    use super::super::cases::CASES;
    use super::super::pin::Pin;
    use super::super::protocol::RawHash;
    use super::{
        CONFIGURED_GATE_FAILURE_PREFIX, CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT, DISCHARGER_ENV,
        Invocation, ProcessRunner, RunOutput, Runner, SUCCESS_LINE, canonical_tempdir,
        configured_gate_panic_payload, panic_configured_gate, validate_evidence_locator_path,
    };
    use super::{run_from_configuration, run_gate_with};
    use anyhow::{Context, Result};
    use serde_json::json;
    use std::ffi::OsString;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const EXPECTED_TEST_VERIFIER_REVISION: &str = "fb0b2dd56bd451960197cf7e7ccdc513eea47d8b";

    struct HelperBinary {
        _directory: tempfile::TempDir,
        path: PathBuf,
    }

    struct ReceiptTemplates {
        _directory: tempfile::TempDir,
        path: PathBuf,
    }

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("tests crate has a repository parent")
    }

    fn compile_helper() -> HelperBinary {
        let executable_root = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .expect("locate current test executable")
                    .parent()
                    .expect("test executable has a directory")
                    .to_path_buf()
            });
        let directory = tempfile::Builder::new()
            .prefix("rocq-discharge-helper-")
            .tempdir_in(executable_root)
            .expect("create executable helper build directory");
        let path = directory.path().join(format!(
            "rocq-discharge-test-helper{}",
            std::env::consts::EXE_SUFFIX
        ));
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("bin")
            .join("rocq-discharge-test-helper.rs");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
        let output = Command::new(rustc)
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(&path)
            .output()
            .expect("invoke rustc for cross-platform fake discharger");
        assert!(
            output.status.success(),
            "compile fake discharger failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        HelperBinary {
            _directory: directory,
            path,
        }
    }

    fn receipt_templates() -> ReceiptTemplates {
        let directory = tempfile::tempdir().expect("create receipt template directory");
        let pin = Pin::read().expect("read pin");
        for case in CASES {
            let golden = std::fs::read(repository_root().join(case.golden_path()))
                .expect("read case golden");
            let receipt = json!({
                "protocol": 1,
                "case_id": case.id(),
                "raw_basename": case.raw_basename(),
                "raw_sha256": RawHash::of(&golden).as_str(),
                "wasm_verifier_pinned": pin.wasm_verifier_revision(),
                "wasm_verifier_observed": pin.wasm_verifier_revision(),
                "coq_wasm_pinned": pin.coq_wasm_revision(),
                "coq_wasm_observed": pin.coq_wasm_revision(),
                "coq_version": pin.coq_series(),
                "proved": case.expected_proved(),
                "refuted": case.expected_refuted(),
                "audited_endpoints": case.expected_proved() + case.expected_refuted(),
                "allowlisted_dependencies": 0,
                "raw_namespace_dependencies": 0,
                "unapproved_dependencies": 0,
                "result": "pass",
            });
            std::fs::write(
                directory.path().join(format!("{}.json", case.id())),
                serde_json::to_vec(&receipt).expect("serialize receipt template"),
            )
            .expect("write receipt template");
        }
        let path = directory.path().to_path_buf();
        ReceiptTemplates {
            _directory: directory,
            path,
        }
    }

    fn fake_runner(behavior: &str, templates: &ReceiptTemplates) -> ProcessRunner {
        ProcessRunner::with_environment(vec![
            (
                OsString::from("INFERENCE_TEST_DISCHARGER_BEHAVIOR"),
                OsString::from(behavior),
            ),
            (
                OsString::from("INFERENCE_TEST_EXPECTED_RAW_DIR"),
                repository_root()
                    .join("tests")
                    .join("test_data")
                    .join("rocq")
                    .into_os_string(),
            ),
            (
                OsString::from("INFERENCE_TEST_RECEIPT_TEMPLATE_DIR"),
                templates.path.clone().into_os_string(),
            ),
            (
                OsString::from("INFERENCE_TEST_EXPECTED_WASM_VERIFIER_REVISION"),
                OsString::from(EXPECTED_TEST_VERIFIER_REVISION),
            ),
            (
                OsString::from("INFERENCE_WASM_VERIFIER_EVIDENCE_DIR"),
                OsString::from("injected-value-must-not-win"),
            ),
        ])
    }

    fn evidence_paths(path: &Path) -> Vec<PathBuf> {
        std::fs::read_to_string(path)
            .expect("read evidence path log")
            .lines()
            .map(PathBuf::from)
            .collect()
    }

    fn retained_evidence_path(rendered: &str) -> Option<PathBuf> {
        let Some((_, suffix)) = rendered.split_once("evidence=") else {
            return None;
        };
        let locator = suffix
            .split_once("; diagnostic=")
            .map_or(suffix, |(locator, _)| locator);
        Some(PathBuf::from(locator))
    }

    fn remove_retained_evidence(rendered: &str) {
        let Some(path) = retained_evidence_path(rendered) else {
            return;
        };
        if std::fs::symlink_metadata(&path).is_ok() {
            std::fs::remove_dir_all(&path).expect("remove retained fake evidence");
        }
    }

    #[test]
    fn evidence_locator_rejects_unbounded_or_control_character_paths() {
        assert!(validate_evidence_locator_path(Path::new(&"x".repeat(201))).is_err());
        assert!(validate_evidence_locator_path(Path::new("bad\npath")).is_err());
    }

    #[test]
    fn process_runner_uses_fresh_private_evidence_and_cleans_probe_and_success() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let evidence_log = tempfile::NamedTempFile::new().expect("create evidence path log");
        let mut runner = fake_runner("valid", &templates);
        runner.environment.push((
            OsString::from("INFERENCE_TEST_EVIDENCE_PATH_LOG"),
            evidence_log.path().as_os_str().to_owned(),
        ));

        assert_eq!(
            run_gate_with(&helper.path, &mut runner).expect("run valid fake discharger"),
            SUCCESS_LINE
        );
        let paths = evidence_paths(evidence_log.path());
        assert_eq!(paths.len(), CASES.len() + 1);
        assert!(paths.iter().all(|path| path.is_absolute()));
        assert!(
            paths.iter().all(|path| path
                .parent()
                .and_then(|parent| std::fs::canonicalize(parent).ok())
                .as_deref()
                == path.parent()),
            "evidence parent paths were not canonical: {paths:?}"
        );
        assert!(
            paths
                .iter()
                .enumerate()
                .all(|(index, path)| !paths[..index].contains(path)),
            "evidence directory was reused: {paths:?}"
        );
        assert!(
            paths
                .iter()
                .all(|path| std::fs::symlink_metadata(path).is_err()),
            "probe/success evidence was retained: {paths:?}"
        );
    }

    #[test]
    fn process_runner_retains_safe_case_failure_with_a_bounded_first_locator() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let evidence_log = tempfile::NamedTempFile::new().expect("create evidence path log");
        let mut runner = fake_runner("nonzero", &templates);
        runner.environment.push((
            OsString::from("INFERENCE_TEST_EVIDENCE_PATH_LOG"),
            evidence_log.path().as_os_str().to_owned(),
        ));

        let error = run_gate_with(&helper.path, &mut runner)
            .expect_err("real case failure must retain private evidence");
        let rendered = format!("{error:#}");
        let paths = evidence_paths(evidence_log.path());
        assert_eq!(paths.len(), 2, "expected probe plus first case: {paths:?}");
        assert!(
            std::fs::symlink_metadata(&paths[0]).is_err(),
            "expected probe evidence was retained"
        );
        let retained = &paths[1];
        assert!(retained.exists(), "case failure evidence was removed");
        let locator = format!("evidence={}", retained.display());
        assert!(
            rendered.find(&locator).is_some_and(|offset| offset < 256),
            "bounded error omitted an early usable locator: {rendered:?}"
        );
        assert!(rendered.len() <= 512, "failure diagnostic is unbounded");
        let panic_payload = configured_gate_panic_payload(&error);
        assert!(
            panic_payload.contains(&locator),
            "configured panic bounding removed the evidence locator: {panic_payload:?}"
        );

        let directory = std::fs::symlink_metadata(retained).expect("inspect retained evidence");
        assert!(directory.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(directory.permissions().mode() & 0o7777, 0o700);
        }
        let mut names: Vec<_> = std::fs::read_dir(retained)
            .expect("read retained evidence")
            .map(|entry| entry.expect("read evidence entry").file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["bridge-output.log", "verifier.log"]);
        for name in names {
            let metadata = std::fs::symlink_metadata(retained.join(name))
                .expect("inspect retained evidence file");
            assert!(metadata.is_file());
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
                assert_eq!(metadata.nlink(), 1);
            }
        }
        assert!(
            std::fs::read_to_string(retained.join("bridge-output.log"))
                .expect("read retained bridge output")
                .contains("fake nonzero failure"),
            "retained capture omitted the child diagnostic"
        );

        std::fs::remove_dir_all(retained).expect("remove retained test evidence");
    }

    #[test]
    fn process_runner_retains_safe_invalid_case_evidence_with_a_locator() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let evidence_log = tempfile::NamedTempFile::new().expect("create evidence path log");
        let mut runner = fake_runner("nonzero-no-log", &templates);
        runner.environment.push((
            OsString::from("INFERENCE_TEST_EVIDENCE_PATH_LOG"),
            evidence_log.path().as_os_str().to_owned(),
        ));

        let error = run_gate_with(&helper.path, &mut runner)
            .expect_err("missing verifier log must fail with retained safe evidence");
        let rendered = format!("{error:#}");
        let paths = evidence_paths(evidence_log.path());
        assert_eq!(paths.len(), 2);
        assert!(std::fs::symlink_metadata(&paths[0]).is_err());
        let retained = &paths[1];
        assert!(retained.is_dir());
        assert!(rendered.contains(&format!("evidence={}", retained.display())));
        let names: Vec<_> = std::fs::read_dir(retained)
            .expect("read invalid retained evidence")
            .map(|entry| entry.expect("read evidence entry").file_name())
            .collect();
        assert_eq!(names, ["bridge-output.log"]);
        std::fs::remove_dir_all(retained).expect("remove invalid retained test evidence");
    }

    #[test]
    fn process_runner_cleans_invalid_probe_evidence_without_a_locator() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let evidence_log = tempfile::NamedTempFile::new().expect("create evidence path log");
        let mut runner = fake_runner("probe-no-log", &templates);
        runner.environment.push((
            OsString::from("INFERENCE_TEST_EVIDENCE_PATH_LOG"),
            evidence_log.path().as_os_str().to_owned(),
        ));

        let error = run_gate_with(&helper.path, &mut runner)
            .expect_err("invalid probe evidence must fail and clean");
        let rendered = format!("{error:#}");
        let paths = evidence_paths(evidence_log.path());
        assert_eq!(paths.len(), 1);
        assert!(std::fs::symlink_metadata(&paths[0]).is_err());
        assert!(!rendered.contains("evidence="));
    }

    #[cfg(unix)]
    #[test]
    fn process_runner_cleans_and_does_not_advertise_hardlinked_capture() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let evidence_log = tempfile::NamedTempFile::new().expect("create evidence path log");
        let mut runner = fake_runner("capture-hardlink", &templates);
        runner.environment.push((
            OsString::from("INFERENCE_TEST_EVIDENCE_PATH_LOG"),
            evidence_log.path().as_os_str().to_owned(),
        ));

        let error = run_gate_with(&helper.path, &mut runner)
            .expect_err("hardlinked capture must fail closed");
        let rendered = format!("{error:#}");
        let paths = evidence_paths(evidence_log.path());
        assert_eq!(paths.len(), 2);
        assert!(
            paths
                .iter()
                .all(|path| std::fs::symlink_metadata(path).is_err())
        );
        assert!(!rendered.contains("evidence="));
    }

    #[test]
    fn absent_configuration_skips_visibly_unless_required() {
        let mut output = Vec::new();
        run_from_configuration(None, false, &mut output).expect("optional absence should skip");
        let output = String::from_utf8(output).expect("skip output is UTF-8");
        assert!(
            output.contains("SKIPPED"),
            "skip was not visible: {output:?}"
        );

        let error = run_from_configuration(None, true, &mut Vec::new())
            .expect_err("required absence must fail closed");
        assert!(
            error.to_string().contains(DISCHARGER_ENV),
            "required error omitted configuration name: {error:#}"
        );
    }

    #[test]
    fn fake_discharger_failure_modes_are_rejected() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let cases = [
            ("noop", "provenance probe"),
            ("nonzero", "exited"),
            ("no-receipt", "receipt directory"),
            ("malformed", "strict receipt JSON"),
            ("duplicate", "receipt directory"),
        ];
        let mut unexpected = Vec::new();
        for (behavior, expected_error) in cases {
            let mut runner = fake_runner(behavior, &templates);
            let error = run_gate_with(&helper.path, &mut runner)
                .expect_err("invalid fake behavior must fail");
            let rendered = format!("{error:#}");
            if !rendered.contains(expected_error) {
                unexpected.push((behavior, rendered.clone()));
            }
            remove_retained_evidence(&rendered);
        }
        assert!(
            unexpected.is_empty(),
            "fake failure modes produced wrong errors: {unexpected:?}"
        );
    }

    #[test]
    fn large_child_output_completes_with_a_bounded_public_diagnostic() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let mut runner = fake_runner("flood", &templates);

        let error = run_gate_with(&helper.path, &mut runner)
            .expect_err("large-output discharger must not produce a trusted result");
        let error = format!("{error:#}");
        assert!(
            error.contains("private verifier diagnostic retained"),
            "public error omitted the private-diagnostic status: {error}"
        );
        assert!(!error.contains("flood diagnostic begins"));
        assert!(
            error.len() <= 512,
            "public error retained an unbounded child diagnostic: {} bytes",
            error.len()
        );
        assert!(
            !error.contains(SUCCESS_LINE),
            "large-output failure emitted the success marker: {error}"
        );
        let retained = retained_evidence_path(&error).expect("large failure has evidence locator");
        let private = std::fs::read_to_string(retained.join("bridge-output.log"))
            .expect("read retained large bridge output");
        assert!(private.contains("flood diagnostic begins"));
        remove_retained_evidence(&error);
    }

    struct ObservedProcessRunner {
        inner: ProcessRunner,
        diagnostics: Vec<String>,
        receipt_counts: Vec<usize>,
    }

    impl Runner for ObservedProcessRunner {
        fn run(&mut self, invocation: &Invocation<'_>) -> Result<RunOutput> {
            let output = self.inner.run(invocation)?;
            self.diagnostics.push(output.stderr.clone());
            self.receipt_counts.push(
                std::fs::read_dir(invocation.receipt_dir)
                    .context("inspect fake discharger receipt directory")?
                    .count(),
            );
            Ok(output)
        }
    }

    #[test]
    fn control_flood_child_failure_is_sanitized_bounded_and_fully_drained() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let mut runner = ObservedProcessRunner {
            inner: fake_runner("control-flood", &templates),
            diagnostics: Vec::new(),
            receipt_counts: Vec::new(),
        };

        let error = run_gate_with(&helper.path, &mut runner)
            .expect_err("control-flood discharger must not produce a trusted result");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("discharger case `prime-bounded` exited Some(92)"),
            "case and phase context were not retained: {rendered:?}"
        );
        assert!(
            rendered.contains("private verifier diagnostic retained"),
            "public failure omitted private-diagnostic status: {rendered:?}"
        );
        assert!(!rendered.contains("control diagnostic begins"));
        assert!(
            rendered.chars().all(|character| !character.is_control()),
            "child control characters reached the application error: {rendered:?}"
        );
        assert!(
            !rendered.contains(SUCCESS_LINE),
            "control-flood failure emitted the success marker: {rendered:?}"
        );

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic_configured_gate(&error)
        }))
        .expect_err("configured gate must panic on a real child failure");
        let payload = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("configured gate panic payload is text");
        assert!(payload.starts_with(CONFIGURED_GATE_FAILURE_PREFIX));
        assert!(payload.len() <= CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT);
        assert_eq!(payload.lines().count(), 1);
        assert!(payload.chars().all(|character| !character.is_control()));
        assert!(payload.contains("discharger case `prime-bounded` exited Some(92)"));
        assert!(!payload.contains("control diagnostic begins"));
        assert!(
            !payload.contains("oooo"),
            "discarded child stdout reached the panic payload: {payload:?}"
        );
        assert!(!payload.contains(SUCCESS_LINE));

        let child_diagnostic = runner
            .diagnostics
            .last()
            .expect("real failing invocation retained a diagnostic");
        assert_eq!(
            child_diagnostic, "private verifier diagnostic retained",
            "public runner output should reveal no private child bytes"
        );
        let retained = retained_evidence_path(&rendered).expect("control failure has locator");
        let private = std::fs::read_to_string(retained.join("bridge-output.log"))
            .expect("read private control-flood output");
        assert!(private.contains("control diagnostic begins"));
        assert!(private.contains("oooo"));
        assert_eq!(
            runner.receipt_counts,
            [0, 0],
            "failed helper published a receipt: {:?}",
            runner.receipt_counts
        );
        remove_retained_evidence(&rendered);
    }

    #[test]
    fn configured_gate_payload_sanitizes_and_exactly_bounds_nested_errors() {
        let error = anyhow::anyhow!(
            "leaf\n\r\t\0\u{7}\u{1b}\u{7f}\u{85}\u{9f}{}{}",
            "東京".repeat(4),
            "x".repeat(4_000)
        )
        .context("middle\nphase")
        .context("outer\tescape=\u{1b}[31m");

        let payload = configured_gate_panic_payload(&error);

        assert_eq!(
            payload.len(),
            CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT,
            "complete application-controlled panic payload must meet its exact ceiling"
        );
        assert!(
            payload.starts_with(&format!(
                "{CONFIGURED_GATE_FAILURE_PREFIX}outer escape= [31m"
            )),
            "stable prefix and outer error context were not retained: {payload:?}"
        );
        assert!(
            payload.chars().all(|character| !character.is_control()),
            "configured-gate payload retained a control character: {payload:?}"
        );
        assert!(
            payload.ends_with("..."),
            "bounded configured-gate payload omitted the truncation marker"
        );
        assert!(
            payload.contains("東京"),
            "valid Unicode was not retained: {payload:?}"
        );
    }

    #[test]
    fn configured_gate_payload_never_splits_a_utf8_code_point() {
        const CONTEXT: &str = "phase=verify: ";
        const MARKER: &str = "...";
        let ascii_count = CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT
            - CONFIGURED_GATE_FAILURE_PREFIX.len()
            - CONTEXT.len()
            - MARKER.len()
            - 1;
        let error =
            anyhow::anyhow!(format!("{}界tail", "a".repeat(ascii_count))).context("phase=verify");

        let payload = configured_gate_panic_payload(&error);

        assert_eq!(
            payload,
            format!(
                "{CONFIGURED_GATE_FAILURE_PREFIX}{CONTEXT}{}{MARKER}",
                "a".repeat(ascii_count)
            )
        );
        assert_eq!(payload.len(), CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT - 1);
        assert!(std::str::from_utf8(payload.as_bytes()).is_ok());
    }

    #[test]
    fn configured_gate_panics_with_only_the_bounded_application_payload() {
        let error = anyhow::anyhow!("leaf\n{}{}", "界".repeat(4), "x".repeat(2_000))
            .context("verification\tphase");
        let expected = configured_gate_panic_payload(&error);

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic_configured_gate(&error)
        }))
        .expect_err("configured gate failure must panic");
        let payload = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("configured gate panic payload is text");

        assert_eq!(payload, expected);
        assert_eq!(payload.len(), CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT);
        assert!(payload.starts_with(CONFIGURED_GATE_FAILURE_PREFIX));
        assert!(payload.chars().all(|character| !character.is_control()));
    }

    #[test]
    fn fake_discharger_rejects_a_wrong_but_canonical_verifier_revision() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let mut runner = fake_runner("valid", &templates);
        let receipt_dir = canonical_tempdir(
            "inference-rocq-test-receipts.",
            "create wrong-revision receipt directory",
        )
        .expect("create canonical receipt directory");
        let case = &CASES[0];
        let raw_file = repository_root().join(case.golden_path());
        let invocation = Invocation {
            executable: &helper.path,
            verifier_revision: "0000000000000000000000000000000000000000",
            case_id: case.id(),
            raw_file: &raw_file,
            receipt_dir: receipt_dir.path(),
            kind: super::InvocationKind::Case,
        };

        let output = runner
            .run(&invocation)
            .expect("run fake with wrong canonical revision");
        assert!(!output.success, "fake accepted the wrong revision");
        assert_eq!(output.stderr, "private verifier diagnostic retained");
        assert_eq!(
            std::fs::read_dir(receipt_dir.path())
                .expect("read receipt directory")
                .count(),
            0,
            "wrong revision produced a receipt"
        );
        if let Some(evidence) = output.evidence {
            assert!(
                std::fs::read_to_string(evidence.join("bridge-output.log"))
                    .expect("read wrong-revision private evidence")
                    .contains("verifier revision mismatch")
            );
            std::fs::remove_dir_all(evidence).expect("remove wrong-revision test evidence");
        }
    }

    #[test]
    fn valid_fake_discharger_runs_probe_all_cases_and_common_verify() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let mut runner = CanonicalInvocationRunner {
            inner: fake_runner("valid", &templates),
            invocations: 0,
        };

        assert_eq!(
            run_gate_with(&helper.path, &mut runner).expect("run valid fake discharger"),
            SUCCESS_LINE
        );
        assert_eq!(runner.invocations, CASES.len() + 1);
    }

    #[test]
    fn canonical_tempdir_is_canonical_and_exactly_private() {
        let directory = canonical_tempdir(
            "inference-rocq-private-test.",
            "test canonical private temporary directory",
        )
        .expect("create canonical private temporary directory");
        assert_eq!(
            std::fs::canonicalize(directory.path()).expect("canonicalize temporary directory"),
            directory.path()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::symlink_metadata(directory.path())
                .expect("inspect private temporary directory");
            assert_eq!(metadata.permissions().mode() & 0o7777, 0o700);
        }
        directory
            .close()
            .expect("remove canonical private temporary directory");
    }

    struct CanonicalInvocationRunner {
        inner: ProcessRunner,
        invocations: usize,
    }

    impl Runner for CanonicalInvocationRunner {
        fn run(&mut self, invocation: &Invocation<'_>) -> Result<RunOutput> {
            assert_eq!(
                std::fs::canonicalize(invocation.raw_file)
                    .expect("canonicalize invocation raw file"),
                invocation.raw_file,
                "invocation raw path is not canonical"
            );
            assert_eq!(
                std::fs::canonicalize(invocation.receipt_dir)
                    .expect("canonicalize invocation receipt directory"),
                invocation.receipt_dir,
                "invocation receipt path is not canonical"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = std::fs::symlink_metadata(invocation.receipt_dir)
                    .expect("inspect invocation receipt directory");
                assert_eq!(
                    metadata.permissions().mode() & 0o7777,
                    0o700,
                    "invocation receipt directory mode is not 0700"
                );
            }
            self.invocations += 1;
            self.inner.run(invocation)
        }
    }

    struct MutateAfterProcess {
        inner: ProcessRunner,
        invocations: usize,
    }

    impl Runner for MutateAfterProcess {
        fn run(&mut self, invocation: &Invocation<'_>) -> Result<RunOutput> {
            let output = self.inner.run(invocation)?;
            self.invocations += 1;
            if self.invocations == 2 {
                let mut raw = std::fs::OpenOptions::new()
                    .append(true)
                    .open(invocation.raw_file)
                    .with_context(|| {
                        format!("open {} for TOCTOU mutation", invocation.raw_file.display())
                    })?;
                raw.write_all(b"mutated after process")
                    .context("mutate raw after process")?;
            }
            Ok(output)
        }
    }

    #[test]
    fn raw_mutation_between_process_prehash_and_posthash_is_never_trusted() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let mut runner = MutateAfterProcess {
            inner: fake_runner("valid", &templates),
            invocations: 0,
        };

        let error =
            run_gate_with(&helper.path, &mut runner).expect_err("TOCTOU raw mutation must fail");
        let error = format!("{error:#}");
        assert!(error.contains("raw integrity"), "unexpected error: {error}");
        assert!(
            !error.contains(SUCCESS_LINE),
            "mutated result was trusted: {error}"
        );
    }
}

use super::SUCCESS_LINE;
use super::protocol::{RawHash, Request, RequestCase};
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DISCHARGER_ENV: &str = "INFERENCE_WASM_VERIFIER_DISCHARGER";
const REQUIRED_ENV: &str = "INFERENCE_ROCQ_DISCHARGE_REQUIRED";
const DIAGNOSTIC_TRUNCATION_MARKER: &str = "...";
/// Maximum UTF-8 bytes in the complete sanitized child-stderr fragment, including its marker.
const CHILD_DIAGNOSTIC_LIMIT: usize = 240;
/// Leading raw bytes retained while the child pipe continues draining to EOF.
const CHILD_DIAGNOSTIC_CAPTURE_LIMIT: usize = CHILD_DIAGNOSTIC_LIMIT * 4;
const CONFIGURED_GATE_FAILURE_PREFIX: &str = "configured Rocq dischargeability gate failed: ";
/// Maximum UTF-8 bytes in the complete application payload, excluding panic-hook framing.
const CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT: usize = 1_024;

struct Invocation<'a> {
    executable: &'a Path,
    verifier_revision: &'a str,
    case_id: &'a str,
    raw_file: &'a Path,
    receipt_dir: &'a Path,
}

struct RunOutput {
    success: bool,
    code: Option<i32>,
    stderr: String,
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

impl Runner for ProcessRunner {
    fn run(&mut self, invocation: &Invocation<'_>) -> Result<RunOutput> {
        let mut child = Command::new(invocation.executable)
            .arg("--protocol")
            .arg("1")
            .arg("--wasm-verifier-revision")
            .arg(invocation.verifier_revision)
            .arg("--case")
            .arg(invocation.case_id)
            .arg(invocation.raw_file)
            .env(
                "INFERENCE_WASM_VERIFIER_RECEIPT_DIR",
                invocation.receipt_dir,
            )
            .envs(self.environment.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "execute configured discharger for case `{}`",
                    invocation.case_id
                )
            })?;
        let stderr = child
            .stderr
            .take()
            .context("configured discharger has no stderr pipe")?;
        let stderr_reader = std::thread::spawn(move || drain_bounded_diagnostic(stderr));
        let status = child.wait();
        let stderr = stderr_reader
            .join()
            .map_err(|_| anyhow::anyhow!("configured discharger stderr reader panicked"))?
            .context("read configured discharger stderr")?;
        let status = status.with_context(|| {
            format!(
                "wait for configured discharger case `{}`",
                invocation.case_id
            )
        })?;
        Ok(RunOutput {
            success: status.success(),
            code: status.code(),
            stderr,
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
    let exchange = tempfile::tempdir().context("create direct discharge exchange")?;
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
    let probe_files = tempfile::tempdir().context("create provenance probe directory")?;
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
    let receipt_dir = tempfile::tempdir().context("create probe receipt directory")?;
    require_empty_receipt_dir(receipt_dir.path())?;
    let invocation = Invocation {
        executable,
        verifier_revision: request.wasm_verifier_revision(),
        case_id: case.case_id(),
        raw_file: &probe_path,
        receipt_dir: receipt_dir.path(),
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
    let invocation_receipts = tempfile::tempdir()
        .with_context(|| format!("create receipt directory for `{}`", case.case_id()))?;
    require_empty_receipt_dir(invocation_receipts.path())?;
    let raw_file = exchange.join("raw").join(case.raw_basename());
    let invocation = Invocation {
        executable,
        verifier_revision: request.wasm_verifier_revision(),
        case_id: case.case_id(),
        raw_file: &raw_file,
        receipt_dir: invocation_receipts.path(),
    };
    let output = run_with_raw_integrity(runner, &invocation, Some(case.raw_sha256()))?;
    if !output.success {
        bail!(
            "discharger case `{}` exited {:?}: {}",
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
        bail!(
            "raw integrity failure while discharging `{}`",
            invocation.case_id
        );
    }
    Ok(output)
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

fn drain_bounded_diagnostic(mut stderr: impl Read) -> std::io::Result<String> {
    let mut captured = Vec::with_capacity(CHILD_DIAGNOSTIC_CAPTURE_LIMIT);
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stderr.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = CHILD_DIAGNOSTIC_CAPTURE_LIMIT.saturating_sub(captured.len());
        if remaining > 0 {
            captured.extend_from_slice(&buffer[..count.min(remaining)]);
        }
    }
    if captured.is_empty() {
        Ok("no diagnostic".to_string())
    } else {
        Ok(bounded_sanitized_diagnostic(
            "",
            &String::from_utf8_lossy(&captured),
            CHILD_DIAGNOSTIC_LIMIT,
        ))
    }
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
        CHILD_DIAGNOSTIC_LIMIT, CONFIGURED_GATE_FAILURE_PREFIX,
        CONFIGURED_GATE_PANIC_PAYLOAD_LIMIT, DISCHARGER_ENV, Invocation, ProcessRunner, RunOutput,
        Runner, SUCCESS_LINE, configured_gate_panic_payload, panic_configured_gate,
    };
    use super::{run_from_configuration, run_gate_with};
    use anyhow::{Context, Result};
    use serde_json::json;
    use std::ffi::OsString;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const EXPECTED_TEST_VERIFIER_REVISION: &str = "77f1126d5de023d9f8464c60c0137b6321126757";

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
        ])
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
            if !format!("{error:#}").contains(expected_error) {
                unexpected.push((behavior, format!("{error:#}")));
            }
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
            error.contains("flood diagnostic begins"),
            "first child diagnostic was not retained: {error}"
        );
        assert!(
            error.len() <= 512,
            "public error retained an unbounded child diagnostic: {} bytes",
            error.len()
        );
        assert!(
            !error.contains(SUCCESS_LINE),
            "large-output failure emitted the success marker: {error}"
        );
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
            rendered.contains("control diagnostic begins"),
            "leading child context was not retained: {rendered:?}"
        );
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
        assert_eq!(payload.matches("control diagnostic begins").count(), 1);
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
            child_diagnostic.len(),
            CHILD_DIAGNOSTIC_LIMIT,
            "large child diagnostic did not use the exact fragment ceiling"
        );
        assert!(
            child_diagnostic.starts_with(
                "control diagnostic begins: NUL=  BEL=  ESC=  DEL=  C1=  unicode=東京 second line "
            ),
            "controls were not deterministically replaced with spaces: {child_diagnostic:?}"
        );
        assert!(
            child_diagnostic
                .chars()
                .all(|character| !character.is_control()),
            "unsafe child diagnostic was retained: {child_diagnostic:?}"
        );
        assert!(
            child_diagnostic.ends_with("..."),
            "bounded child diagnostic omitted the truncation marker: {child_diagnostic:?}"
        );
        assert_eq!(
            runner.receipt_counts,
            [0, 0],
            "failed helper published a receipt: {:?}",
            runner.receipt_counts
        );
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
        let receipt_dir = tempfile::tempdir().expect("create receipt directory");
        let case = &CASES[0];
        let raw_file = repository_root().join(case.golden_path());
        let invocation = Invocation {
            executable: &helper.path,
            verifier_revision: "0000000000000000000000000000000000000000",
            case_id: case.id(),
            raw_file: &raw_file,
            receipt_dir: receipt_dir.path(),
        };

        let output = runner
            .run(&invocation)
            .expect("run fake with wrong canonical revision");
        assert!(!output.success, "fake accepted the wrong revision");
        assert!(
            output.stderr.contains("verifier revision mismatch"),
            "unexpected diagnostic: {}",
            output.stderr
        );
        assert_eq!(
            std::fs::read_dir(receipt_dir.path())
                .expect("read receipt directory")
                .count(),
            0,
            "wrong revision produced a receipt"
        );
    }

    #[test]
    fn valid_fake_discharger_runs_probe_five_cases_and_common_verify() {
        let helper = compile_helper();
        let templates = receipt_templates();
        let mut runner = fake_runner("valid", &templates);

        assert_eq!(
            run_gate_with(&helper.path, &mut runner).expect("run valid fake discharger"),
            SUCCESS_LINE
        );
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

use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const EVIDENCE_ENV: &str = "INFERENCE_WASM_VERIFIER_EVIDENCE_DIR";

#[cfg(unix)]
fn mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o7777
}

#[cfg(unix)]
fn links(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink()
}

fn validate_evidence() -> PathBuf {
    let evidence = PathBuf::from(required_env(EVIDENCE_ENV));
    if !evidence.is_absolute() {
        eprintln!("evidence directory is not absolute");
        std::process::exit(81);
    }
    if std::fs::canonicalize(&evidence).ok().as_deref() != Some(evidence.as_path()) {
        eprintln!("evidence directory is not canonical");
        std::process::exit(93);
    }
    let metadata = std::fs::symlink_metadata(&evidence).unwrap_or_else(|error| {
        eprintln!("inspect evidence directory: {error}");
        std::process::exit(82);
    });
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        eprintln!("evidence directory is not a nonsymlink directory");
        std::process::exit(83);
    }
    #[cfg(unix)]
    if mode(&metadata) != 0o700 {
        eprintln!("evidence directory mode is not 0700");
        std::process::exit(84);
    }

    let capture = evidence.join("bridge-output.log");
    let metadata = std::fs::symlink_metadata(&capture).unwrap_or_else(|error| {
        eprintln!("inspect bridge output capture: {error}");
        std::process::exit(85);
    });
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != 0 {
        eprintln!("bridge output is not a nonsymlink regular file");
        std::process::exit(86);
    }
    #[cfg(unix)]
    if mode(&metadata) != 0o600 || links(&metadata) != 1 {
        eprintln!("bridge output mode/link contract mismatch");
        std::process::exit(87);
    }
    let entries = std::fs::read_dir(&evidence)
        .unwrap_or_else(|error| {
            eprintln!("read evidence directory: {error}");
            std::process::exit(88);
        })
        .count();
    if entries != 1 {
        eprintln!("evidence directory was not initially capture-only");
        std::process::exit(89);
    }

    if let Some(path_log) = std::env::var_os("INFERENCE_TEST_EVIDENCE_PATH_LOG") {
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path_log)
            .expect("open evidence path log");
        writeln!(log, "{}", evidence.display()).expect("record evidence path");
    }
    evidence
}

fn write_verifier_log(evidence: &Path, message: &str) {
    let path = evidence.join("verifier.log");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut log = options.open(path).expect("create private verifier log");
    writeln!(log, "{message}").expect("write private verifier log");
}

fn required_env(name: &str) -> OsString {
    std::env::var_os(name).unwrap_or_else(|| {
        eprintln!("missing {name}");
        std::process::exit(70);
    })
}

fn parse_args() -> (String, String, PathBuf) {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 7
        || arguments[0] != OsStr::new("--protocol")
        || arguments[1] != OsStr::new("1")
        || arguments[2] != OsStr::new("--wasm-verifier-revision")
        || arguments[4] != OsStr::new("--case")
    {
        eprintln!("invalid discharger arguments: {arguments:?}");
        std::process::exit(71);
    }
    let revision = arguments[3].to_string_lossy();
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        eprintln!("noncanonical verifier revision");
        std::process::exit(72);
    }
    let case_id = arguments[5].to_str().unwrap_or_else(|| {
        eprintln!("non-UTF-8 case ID");
        std::process::exit(73);
    });
    if case_id.is_empty()
        || !case_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        eprintln!("unsafe case ID");
        std::process::exit(74);
    }
    (
        revision.into_owned(),
        case_id.to_string(),
        PathBuf::from(&arguments[6]),
    )
}

fn write_receipt(case_id: &str, receipt_dir: &Path, templates: &Path) {
    std::fs::copy(
        templates.join(format!("{case_id}.json")),
        receipt_dir.join(format!("{case_id}.json")),
    )
    .unwrap_or_else(|error| {
        eprintln!("copy receipt template: {error}");
        std::process::exit(80);
    });
}

fn flood_output(evidence: &Path) -> ! {
    write_verifier_log(evidence, "fake flood failure");
    const CHUNK: &[u8; 8192] = &[b'x'; 8192];
    let mut stdout = std::io::stdout().lock();
    for _ in 0..256 {
        stdout.write_all(CHUNK).expect("write large stdout");
    }
    stdout.flush().expect("flush large stdout");
    drop(stdout);

    let mut stderr = std::io::stderr().lock();
    stderr
        .write_all(b"flood diagnostic begins: ")
        .expect("write diagnostic prefix");
    for _ in 0..256 {
        stderr.write_all(CHUNK).expect("write large stderr");
    }
    stderr.write_all(b"\n").expect("finish large stderr");
    stderr.flush().expect("flush large stderr");
    std::process::exit(7)
}

fn control_flood_output(evidence: &Path) -> ! {
    write_verifier_log(evidence, "fake control flood failure");
    const STDOUT_CHUNK: &[u8; 8192] = &[b'o'; 8192];
    const STDERR_CHUNK: &[u8; 8192] = &[b'x'; 8192];
    let mut stdout = std::io::stdout().lock();
    for _ in 0..256 {
        stdout.write_all(STDOUT_CHUNK).expect("write large stdout");
    }
    stdout.flush().expect("flush large stdout");
    drop(stdout);

    let mut stderr = std::io::stderr().lock();
    stderr
        .write_all(b"control diagnostic begins:\tNUL=\0 BEL=\x07 ESC=\x1b DEL=\x7f C1=")
        .expect("write controlled diagnostic prefix");
    stderr
        .write_all("\u{85} unicode=東京\nsecond\rline ".as_bytes())
        .expect("write Unicode controlled diagnostic prefix");
    for _ in 0..256 {
        stderr.write_all(STDERR_CHUNK).expect("write large stderr");
    }
    stderr.write_all(b"\n").expect("finish large stderr");
    stderr.flush().expect("flush large stderr");
    std::process::exit(92)
}

fn main() {
    let evidence = validate_evidence();
    let behavior = required_env("INFERENCE_TEST_DISCHARGER_BEHAVIOR");
    if behavior == OsStr::new("noop") {
        return;
    }

    let (revision, case_id, raw_file) = parse_args();
    let expected_revision = required_env("INFERENCE_TEST_EXPECTED_WASM_VERIFIER_REVISION");
    if expected_revision != OsStr::new(&revision) {
        write_verifier_log(&evidence, "verifier revision mismatch");
        eprintln!("verifier revision mismatch");
        std::process::exit(91);
    }
    let expected_raw_dir = PathBuf::from(required_env("INFERENCE_TEST_EXPECTED_RAW_DIR"));
    let receipt_dir = PathBuf::from(required_env("INFERENCE_WASM_VERIFIER_RECEIPT_DIR"));
    let templates = PathBuf::from(required_env("INFERENCE_TEST_RECEIPT_TEMPLATE_DIR"));
    if !receipt_dir.is_absolute() {
        eprintln!("receipt directory is not absolute");
        std::process::exit(75);
    }
    if std::fs::canonicalize(&receipt_dir).ok().as_deref() != Some(receipt_dir.as_path()) {
        write_verifier_log(&evidence, "receipt directory is not canonical");
        eprintln!("receipt directory is not canonical");
        std::process::exit(94);
    }
    #[cfg(unix)]
    if mode(
        &std::fs::symlink_metadata(&receipt_dir).unwrap_or_else(|error| {
            write_verifier_log(&evidence, "inspect receipt directory mode");
            eprintln!("inspect receipt directory mode: {error}");
            std::process::exit(96);
        }),
    ) != 0o700
    {
        write_verifier_log(&evidence, "receipt directory mode is not 0700");
        eprintln!("receipt directory mode is not 0700");
        std::process::exit(96);
    }
    if std::fs::canonicalize(&raw_file).ok().as_deref() != Some(raw_file.as_path()) {
        write_verifier_log(&evidence, "raw file path is not canonical");
        eprintln!("raw file path is not canonical");
        std::process::exit(95);
    }
    let basename = raw_file.file_name().unwrap_or_else(|| {
        eprintln!("raw file has no basename");
        std::process::exit(76);
    });
    let actual = std::fs::read(&raw_file).unwrap_or_else(|error| {
        eprintln!("read raw file: {error}");
        std::process::exit(77);
    });
    let expected = std::fs::read(expected_raw_dir.join(basename)).unwrap_or_else(|error| {
        eprintln!("read expected raw file: {error}");
        std::process::exit(78);
    });
    if actual != expected {
        if behavior == OsStr::new("probe-no-log") {
            eprintln!("raw provenance mismatch without verifier log");
            std::process::exit(90);
        }
        write_verifier_log(&evidence, "raw provenance mismatch");
        eprintln!("raw provenance mismatch");
        std::process::exit(90);
    }

    match behavior.to_str() {
        Some("nonzero") => {
            write_verifier_log(&evidence, "fake nonzero failure");
            eprintln!("fake nonzero failure");
            std::process::exit(7)
        }
        Some("nonzero-no-log") => {
            eprintln!("fake nonzero failure without verifier log");
            std::process::exit(7)
        }
        #[cfg(unix)]
        Some("capture-hardlink") => {
            std::fs::hard_link(
                evidence.join("bridge-output.log"),
                evidence.join("capture-link"),
            )
            .expect("hardlink bridge output capture");
            write_verifier_log(&evidence, "fake capture hardlink failure");
            std::process::exit(7)
        }
        Some("flood") => flood_output(&evidence),
        Some("control-flood") => control_flood_output(&evidence),
        Some("no-receipt") => {}
        Some("malformed") => {
            std::fs::write(receipt_dir.join(format!("{case_id}.json")), b"{")
                .expect("write malformed receipt");
        }
        Some("duplicate") => {
            write_receipt(&case_id, &receipt_dir, &templates);
            std::fs::write(receipt_dir.join("extra.json"), b"duplicate")
                .expect("write duplicate receipt marker");
        }
        Some("valid") => write_receipt(&case_id, &receipt_dir, &templates),
        _ => {
            eprintln!("unknown fake behavior: {behavior:?}");
            std::process::exit(79);
        }
    }
}

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};

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

fn flood_output() -> ! {
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

fn main() {
    let behavior = required_env("INFERENCE_TEST_DISCHARGER_BEHAVIOR");
    if behavior == OsStr::new("noop") {
        return;
    }

    let (revision, case_id, raw_file) = parse_args();
    let expected_revision = required_env("INFERENCE_TEST_EXPECTED_WASM_VERIFIER_REVISION");
    if expected_revision != OsStr::new(&revision) {
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
        eprintln!("raw provenance mismatch");
        std::process::exit(90);
    }

    match behavior.to_str() {
        Some("nonzero") => std::process::exit(7),
        Some("flood") => flood_output(),
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

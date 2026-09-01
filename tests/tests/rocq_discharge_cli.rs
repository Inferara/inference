#[cfg(target_os = "linux")]
mod rocq_dischargeability {
    mod cli {
        #[test]
        fn error_is_one_sanitized_bounded_line_for_a_real_path() {
            const PUBLIC_ERROR_LINE_LIMIT: usize = 1_024;
            const PREFIX: &str = "rocq-discharge: ";

            let root = tempfile::tempdir().expect("create CLI path root");
            let mut exchange = root.path().to_path_buf();
            exchange.push("exchange\ncarriage\rbel\u{7}escape\u{1b}");
            for index in 0..14 {
                exchange.push(format!("{index:02}-{}", "a".repeat(170)));
            }
            std::fs::create_dir_all(&exchange).expect("create long control-bearing exchange path");
            std::fs::write(exchange.join("stale"), b"stale").expect("make exchange nonempty");

            let output = std::process::Command::new(env!("CARGO_BIN_EXE_rocq-discharge"))
                .arg("export")
                .arg("--exchange")
                .arg(&exchange)
                .output()
                .expect("run rocq-discharge");

            assert!(
                !output.status.success(),
                "nonempty exchange unexpectedly passed"
            );
            assert!(output.stdout.is_empty(), "failure wrote to stdout");
            let stderr = String::from_utf8(output.stderr).expect("CLI stderr must be UTF-8");
            assert_eq!(
                stderr.bytes().filter(|byte| *byte == b'\n').count(),
                1,
                "CLI stderr must contain only its terminating newline: {stderr:?}"
            );
            let line = stderr
                .strip_suffix('\n')
                .expect("CLI error must have one terminating newline");
            assert_eq!(
                line.len(),
                PUBLIC_ERROR_LINE_LIMIT,
                "long CLI errors must fill the exact public byte ceiling"
            );
            assert!(line.starts_with(PREFIX), "CLI prefix drifted: {line:?}");
            assert!(
                line.starts_with("rocq-discharge: export exchange directory must be empty:"),
                "stable failure phase/context was not retained: {line:?}"
            );
            assert!(
                line.contains("exchange carriage bel escape /00-"),
                "CLI controls were not deterministically replaced with spaces: {line:?}"
            );
            assert!(line.ends_with("..."), "CLI truncation marker missing");
            assert!(
                line.chars().all(|character| !character.is_control()),
                "CLI line retained an unsafe control: {line:?}"
            );
        }
    }
}

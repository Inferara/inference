use anyhow::{Result, bail};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

const PUBLIC_ERROR_PREFIX: &str = "rocq-discharge: ";
/// Maximum UTF-8 bytes in a public failure line, excluding the terminating newline.
const PUBLIC_ERROR_LINE_LIMIT: usize = 1_024;
const PUBLIC_ERROR_TRUNCATION_MARKER: &str = "...";

#[derive(Debug, Eq, PartialEq)]
enum CliCommand {
    Export(PathBuf),
    Verify(PathBuf),
}

fn parse_args<I>(arguments: I) -> Result<CliCommand>
where
    I: IntoIterator<Item = OsString>,
{
    let mut arguments = arguments.into_iter();
    let command = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected `export` or `verify` subcommand"))?;
    let flag = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected `--exchange <absolute-dir>`"))?;
    if flag != OsStr::new("--exchange") {
        bail!("expected `--exchange <absolute-dir>`");
    }
    let exchange = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected a path after `--exchange`"))?;
    if arguments.next().is_some() {
        bail!("unexpected extra command arguments");
    }
    let exchange = PathBuf::from(exchange);
    match command.to_str() {
        Some("export") => Ok(CliCommand::Export(exchange)),
        Some("verify") => Ok(CliCommand::Verify(exchange)),
        _ => bail!("expected `export` or `verify` subcommand"),
    }
}

fn run() -> Result<()> {
    match parse_args(std::env::args_os().skip(1))? {
        CliCommand::Export(exchange) => {
            inference_tests::rocq_dischargeability::export_cli(&exchange)
        }
        CliCommand::Verify(exchange) => {
            inference_tests::rocq_dischargeability::verify_cli(&exchange)
        }
    }
}

fn format_public_error(error: &anyhow::Error) -> String {
    let rendered = format!("{error:#}");
    let mut line = String::with_capacity(PUBLIC_ERROR_LINE_LIMIT);
    line.push_str(PUBLIC_ERROR_PREFIX);

    for character in rendered.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if line.len() + character.len_utf8() <= PUBLIC_ERROR_LINE_LIMIT {
            line.push(character);
            continue;
        }

        let mut boundary = line
            .len()
            .min(PUBLIC_ERROR_LINE_LIMIT - PUBLIC_ERROR_TRUNCATION_MARKER.len());
        while !line.is_char_boundary(boundary) {
            boundary -= 1;
        }
        line.truncate(boundary);
        line.push_str(PUBLIC_ERROR_TRUNCATION_MARKER);
        return line;
    }

    line
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", format_public_error(&error));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod rocq_dischargeability {
    mod cli {
        use super::super::{CliCommand, PUBLIC_ERROR_LINE_LIMIT, format_public_error, parse_args};
        use std::ffi::OsString;
        use std::path::PathBuf;

        #[test]
        fn parses_only_the_two_fixed_exchange_commands() {
            let exchange = tempfile::tempdir().expect("create absolute exchange");
            let path = exchange.path().as_os_str().to_os_string();
            assert_eq!(
                parse_args([
                    OsString::from("export"),
                    OsString::from("--exchange"),
                    path.clone(),
                ])
                .expect("parse export"),
                CliCommand::Export(PathBuf::from(&path))
            );
            assert_eq!(
                parse_args([
                    OsString::from("verify"),
                    OsString::from("--exchange"),
                    path.clone(),
                ])
                .expect("parse verify"),
                CliCommand::Verify(PathBuf::from(path))
            );
        }

        #[test]
        fn rejects_missing_unknown_and_extra_arguments() {
            let invalid = [
                Vec::<OsString>::new(),
                vec![OsString::from("other")],
                vec![OsString::from("export")],
                vec![OsString::from("export"), OsString::from("--exchange")],
                vec![
                    OsString::from("export"),
                    OsString::from("--other"),
                    OsString::from("/tmp/exchange"),
                ],
                vec![
                    OsString::from("verify"),
                    OsString::from("--exchange"),
                    OsString::from("/tmp/exchange"),
                    OsString::from("extra"),
                ],
            ];
            let accepted: Vec<_> = invalid
                .into_iter()
                .filter(|arguments| parse_args(arguments.clone()).is_ok())
                .collect();
            assert!(
                accepted.is_empty(),
                "invalid CLI arguments accepted: {accepted:?}"
            );
        }

        #[test]
        fn public_error_formatter_bounds_and_sanitizes_the_complete_anyhow_chain() {
            let root = anyhow::anyhow!(format!(
                "root\n\r\t\0\u{1b}\u{7f}\u{85}{}",
                "r".repeat(4_000)
            ));
            let error = root
                .context(format!(
                    "middle\n\r\t\0\u{1b}\u{7f}\u{85}東京{}",
                    "m".repeat(4_000)
                ))
                .context("phase=export");

            let line = format_public_error(&error);

            assert_eq!(
                line.len(),
                PUBLIC_ERROR_LINE_LIMIT,
                "oversized public errors must fill but never exceed the byte ceiling"
            );
            assert!(
                line.starts_with("rocq-discharge: phase=export: middle       東京"),
                "stable phase/context was not retained: {line}"
            );
            assert!(
                line.contains("東京"),
                "valid Unicode was not retained: {line}"
            );
            assert!(line.ends_with("..."), "truncation marker missing: {line}");
            assert_eq!(line.lines().count(), 1, "public error was not one line");
            assert!(
                line.chars().all(|character| !character.is_control()),
                "public error retained a control character: {line:?}"
            );
        }

        #[test]
        fn public_error_formatter_never_splits_a_utf8_code_point() {
            const PREFIX: &str = "rocq-discharge: ";
            const MARKER: &str = "...";
            let ascii_count = PUBLIC_ERROR_LINE_LIMIT - PREFIX.len() - MARKER.len() - 1;
            let error = anyhow::anyhow!(format!("{}界tail", "a".repeat(ascii_count)));

            let line = format_public_error(&error);

            assert_eq!(line, format!("{PREFIX}{}{MARKER}", "a".repeat(ascii_count)));
            assert_eq!(line.len(), PUBLIC_ERROR_LINE_LIMIT - 1);
            assert!(std::str::from_utf8(line.as_bytes()).is_ok());
        }
    }
}

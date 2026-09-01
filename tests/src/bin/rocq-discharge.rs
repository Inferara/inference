use anyhow::{Result, bail};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

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

fn main() {
    if let Err(error) = run() {
        eprintln!("rocq-discharge: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod rocq_dischargeability {
    mod cli {
        use super::super::{CliCommand, parse_args};
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
    }
}

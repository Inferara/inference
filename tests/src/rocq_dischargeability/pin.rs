use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub(super) struct Pin {
    wasm_verifier_revision: String,
    coq_wasm_tag: String,
    coq_wasm_revision: String,
    coq_series: String,
    assumption_allowlist_count: usize,
}

impl Pin {
    pub(super) fn read() -> Result<Self> {
        let path = pin_path();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read verifier pin {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parse verifier pin {}", path.display()))
    }

    fn parse(text: &str) -> Result<Self> {
        let mut wasm_verifier_revision = None;
        let mut coq_wasm_tag = None;
        let mut coq_wasm_revision = None;
        let mut coq_series = None;
        let mut assumption_allowlist_count = None;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let key = fields.next().expect("a non-empty line has a key");
            let destination = match key {
                "revision" => Some(&mut wasm_verifier_revision),
                "coq-wasm-tag" => Some(&mut coq_wasm_tag),
                "coq-wasm-commit" => Some(&mut coq_wasm_revision),
                "coq" => Some(&mut coq_series),
                "assumption-allowlist-count" => {
                    let value = pin_value(line, &mut fields)?;
                    if assumption_allowlist_count.is_some() {
                        bail!("duplicate pin key `{key}`");
                    }
                    assumption_allowlist_count = Some(
                        value
                            .parse::<usize>()
                            .with_context(|| format!("`{key}` is not an unsigned integer"))?,
                    );
                    None
                }
                "stdlib-name" | "absent-digest" => None,
                _ => bail!("unknown pin key `{key}`"),
            };

            if let Some(destination) = destination {
                let value = pin_value(line, &mut fields)?;
                if destination.replace(value.to_string()).is_some() {
                    bail!("duplicate pin key `{key}`");
                }
            }
        }

        let pin = Self {
            wasm_verifier_revision: required(wasm_verifier_revision, "revision")?,
            coq_wasm_tag: required(coq_wasm_tag, "coq-wasm-tag")?,
            coq_wasm_revision: required(coq_wasm_revision, "coq-wasm-commit")?,
            coq_series: required(coq_series, "coq")?,
            assumption_allowlist_count: assumption_allowlist_count
                .context("pin has no `assumption-allowlist-count` line")?,
        };
        require_revision("revision", &pin.wasm_verifier_revision)?;
        require_revision("coq-wasm-commit", &pin.coq_wasm_revision)?;
        if pin.coq_wasm_tag.is_empty() || pin.coq_wasm_tag.chars().any(char::is_whitespace) {
            bail!("`coq-wasm-tag` is empty or contains whitespace");
        }
        require_coq_series(&pin.coq_series)?;
        Ok(pin)
    }

    pub(super) fn wasm_verifier_revision(&self) -> &str {
        &self.wasm_verifier_revision
    }

    pub(super) fn coq_wasm_tag(&self) -> &str {
        &self.coq_wasm_tag
    }

    pub(super) fn coq_wasm_revision(&self) -> &str {
        &self.coq_wasm_revision
    }

    pub(super) fn coq_series(&self) -> &str {
        &self.coq_series
    }

    pub(super) fn assumption_allowlist_count(&self) -> usize {
        self.assumption_allowlist_count
    }
}

fn pin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tests crate has a repository parent")
        .join("core")
        .join("wasm-to-v")
        .join("wasm-verifier-pin.txt")
}

fn pin_value<'a>(line: &str, fields: &mut impl Iterator<Item = &'a str>) -> Result<&'a str> {
    let value = fields
        .next()
        .with_context(|| format!("pin line has no value: {line:?}"))?;
    if fields.next().is_some() {
        bail!("pin scalar line has extra fields: {line:?}");
    }
    Ok(value)
}

fn required(value: Option<String>, key: &str) -> Result<String> {
    value.with_context(|| format!("pin has no `{key}` line"))
}

pub(super) fn require_revision(label: &str, value: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be canonical lowercase 40-hex");
    }
    Ok(())
}

fn require_coq_series(value: &str) -> Result<()> {
    let Some((major, minor)) = value.split_once('.') else {
        bail!("`coq` must be a numeric major.minor series");
    };
    if major.is_empty()
        || minor.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || !minor.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("`coq` must be a numeric major.minor series");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Pin;
    use std::path::Path;

    const VERIFIER_B: &str = "181cd676662453182b9753d1b19ca933c68770c3";
    const PROTECTED_CHECKOUT_STEP: &str =
        "uses: actions/checkout@1d96c772d19495a3b5c517cd2bc0cb401ea0529f";
    const SUCCESS_LINE: &str = "rocq-discharge: result=pass cases=5 proved=11 refuted=1";

    #[test]
    fn pin_supplies_every_discharge_protocol_value() {
        let pin = Pin::read().expect("read the committed verifier pin");

        assert_eq!(pin.wasm_verifier_revision(), VERIFIER_B);
        assert_eq!(pin.coq_wasm_tag(), "v2.2.0");
        assert_eq!(
            pin.coq_wasm_revision(),
            "0fd83fa708922721132b6d6737179568d1f1d553"
        );
        assert_eq!(pin.coq_series(), "8.20");
        assert_eq!(pin.assumption_allowlist_count(), 10);
    }

    fn read_workflow() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("tests crate has a repository parent")
                .join(".github/workflows/rocq-real-library.yml"),
        )
        .expect("read real-library workflow")
    }

    fn assert_workflow_contract(workflow: &str) {
        let workflow = workflow.replace("\r\n", "\n");
        let gate = workflow
            .split_once("  dischargeability-gate:")
            .expect("workflow has dischargeability capability job")
            .1
            .split_once("  selected-artifact-discharge:")
            .expect("workflow has selected-artifact job after capability job")
            .0;
        let selected = workflow
            .split_once("  selected-artifact-discharge:")
            .expect("workflow has selected-artifact job")
            .1
            .split_once("  # Its own job rather than a step")
            .expect("selected-artifact job has a bounded workflow slice")
            .0;

        for required in [
            "runs-on: ubuntu-latest",
            "Selected-artifact dischargeability: SKIPPED",
            "DISCHARGER: ${{ vars.WASM_VERIFIER_DISCHARGER }}",
            "RUNNER: ${{ vars.WASM_VERIFIER_RUNNER }}",
            "[ -z \"${DISCHARGER:-}\" ] || [ -z \"${RUNNER:-}\" ]",
            "github.event.pull_request.head.repo.full_name == github.repository",
            "contains(github.event.pull_request.labels.*.name, 'ci:real-rocq')",
        ] {
            assert!(
                gate.contains(required),
                "capability job omitted configured discharge fragment {required:?}"
            );
        }
        for required in [
            "needs: dischargeability-gate",
            "if: needs.dischargeability-gate.outputs.present == 'true'",
            "environment: real-rocq",
            "runs-on: ${{ vars.WASM_VERIFIER_RUNNER }}",
            PROTECTED_CHECKOUT_STEP,
            "INFERENCE_WASM_VERIFIER_DISCHARGER: ${{ vars.WASM_VERIFIER_DISCHARGER }}",
            "INFERENCE_ROCQ_DISCHARGE_REQUIRED: '1'",
            "test ! -e Cargo.lock",
            "cp ci/rocq-discharge.cargo-lock Cargo.lock",
            "cmp -s ci/rocq-discharge.cargo-lock Cargo.lock",
            "rustc +1.98.0 --version",
            "'rustc 1.98.0 '*",
            "cargo +1.98.0 fetch --locked",
            "cargo +1.98.0 test -p inference-tests --locked --offline --verbose",
            "rocq_dischargeability::direct::configured_dischargeability_gate",
            "-- --exact --nocapture",
            "'$0 == marker { count++ } END { print count + 0 }'",
            SUCCESS_LINE,
            VERIFIER_B,
            "ci/discharge/run-docker-case.sh",
        ] {
            assert!(
                selected.contains(required),
                "selected-artifact job omitted discharge contract fragment {required:?}"
            );
        }
        assert!(!selected.contains("ubuntu-latest"));
        assert!(!selected.contains("uses: actions/checkout@v"));
        assert!(!selected.contains("|| true"));
        assert!(workflow.contains("permissions:\n  contents: read"));
        assert!(
            workflow.contains("pull_request:\n    types: [opened, synchronize, reopened, labeled]"),
            "workflow no longer preserves pull_request scheduling"
        );
        assert!(
            !workflow.contains("pull_request_target:"),
            "workflow must never execute this lane through pull_request_target"
        );
    }

    #[test]
    fn workflow_requires_the_configured_single_case_discharger() {
        assert_workflow_contract(&read_workflow());
    }

    #[test]
    fn workflow_contract_is_independent_of_checkout_line_endings() {
        let workflow = read_workflow().replace('\n', "\r\n");

        assert_workflow_contract(&workflow);
    }
}

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

    #[test]
    fn pin_supplies_every_discharge_protocol_value() {
        let pin = Pin::read().expect("read the committed verifier pin");

        assert_eq!(
            pin.wasm_verifier_revision(),
            "77f1126d5de023d9f8464c60c0137b6321126757"
        );
        assert_eq!(pin.coq_wasm_tag(), "v2.2.0");
        assert_eq!(
            pin.coq_wasm_revision(),
            "0fd83fa708922721132b6d6737179568d1f1d553"
        );
        assert_eq!(pin.coq_series(), "8.20");
        assert_eq!(pin.assumption_allowlist_count(), 10);
    }
}

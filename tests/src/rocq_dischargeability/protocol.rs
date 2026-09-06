use super::cases::{CASES, CaseSpec};
use super::pin::{Pin, require_revision};
use anyhow::{Context, Result, bail};
use serde::de::{DeserializeOwned, Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

const PROTOCOL_VERSION: u64 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct Revision(String);

impl Revision {
    fn parse(value: String) -> Result<Self> {
        require_revision("revision", &value)?;
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Revision {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct RawHash(String);

impl RawHash {
    fn parse(value: String) -> Result<Self> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("raw SHA-256 must be canonical lowercase 64-hex");
        }
        Ok(Self(value))
    }

    pub(super) fn of(bytes: &[u8]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(encoded)
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RawHash {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    protocol: u64,
    wasm_verifier_revision: Revision,
    coq_wasm_tag: String,
    coq_wasm_revision: Revision,
    coq_series: String,
    assumption_allowlist_count: usize,
    expected_case_count: usize,
    expected_proved_endpoints: usize,
    expected_refuted_endpoints: usize,
    cases: Vec<RequestCase>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequestCase {
    case_id: String,
    raw_basename: String,
    raw_sha256: RawHash,
    expected_proved_endpoints: usize,
    expected_refuted_endpoints: usize,
}

impl Request {
    pub(super) fn from_raw(pin: &Pin, raw: Vec<(&CaseSpec, RawHash)>) -> Self {
        let cases = raw
            .into_iter()
            .map(|(case, raw_sha256)| RequestCase {
                case_id: case.id().to_string(),
                raw_basename: case.raw_basename().to_string(),
                raw_sha256,
                expected_proved_endpoints: case.expected_proved(),
                expected_refuted_endpoints: case.expected_refuted(),
            })
            .collect();
        Self {
            protocol: PROTOCOL_VERSION,
            wasm_verifier_revision: Revision(pin.wasm_verifier_revision().to_string()),
            coq_wasm_tag: pin.coq_wasm_tag().to_string(),
            coq_wasm_revision: Revision(pin.coq_wasm_revision().to_string()),
            coq_series: pin.coq_series().to_string(),
            assumption_allowlist_count: pin.assumption_allowlist_count(),
            expected_case_count: CASES.len(),
            expected_proved_endpoints: CASES.iter().map(CaseSpec::expected_proved).sum(),
            expected_refuted_endpoints: CASES.iter().map(CaseSpec::expected_refuted).sum(),
            cases,
        }
    }

    pub(super) fn wasm_verifier_revision(&self) -> &str {
        self.wasm_verifier_revision.as_str()
    }

    pub(super) fn cases(&self) -> &[RequestCase] {
        &self.cases
    }
}

impl RequestCase {
    pub(super) fn case_id(&self) -> &str {
        &self.case_id
    }

    pub(super) fn raw_basename(&self) -> &str {
        &self.raw_basename
    }

    pub(super) fn raw_sha256(&self) -> &str {
        self.raw_sha256.as_str()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    protocol: u64,
    case_id: String,
    raw_basename: String,
    raw_sha256: RawHash,
    wasm_verifier_pinned: Revision,
    wasm_verifier_observed: Revision,
    coq_wasm_pinned: Revision,
    coq_wasm_observed: Revision,
    coq_version: String,
    proved: usize,
    refuted: usize,
    audited_endpoints: usize,
    allowlisted_dependencies: usize,
    raw_namespace_dependencies: usize,
    unapproved_dependencies: usize,
    result: ReceiptResult,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ReceiptResult {
    Pass,
}

pub(super) fn serialize_request(request: &Request) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(request).context("serialize discharge request")?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn read_request(exchange: &Path) -> Result<Request> {
    let path = exchange.join("request.json");
    require_regular_file(&path, "request")?;
    let bytes = std::fs::read(&path).with_context(|| format!("read request {}", path.display()))?;
    let request: Request = parse_strict(&bytes, "request")?;
    validate_request(&request, &Pin::read()?)?;
    Ok(request)
}

pub(super) fn verify_exchange(exchange: &Path) -> Result<()> {
    let exchange = require_exchange_root(exchange)?;
    verify_exchange_after_root_check(&exchange)
}

fn verify_exchange_after_root_check(exchange: &Path) -> Result<()> {
    require_exact_top_level(exchange)?;
    let pin = Pin::read()?;
    let request = read_request(exchange)?;
    validate_raw(exchange, &request)?;
    validate_receipts(exchange, &request, &pin)
}

#[cfg(test)]
fn verify_exchange_with_hook<F>(exchange: &Path, after_root_check: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let exchange = require_exchange_root(exchange)?;
    after_root_check()?;
    verify_exchange_after_root_check(&exchange)
}

fn validate_request(request: &Request, pin: &Pin) -> Result<()> {
    if request.protocol != PROTOCOL_VERSION {
        bail!("request protocol must be {PROTOCOL_VERSION}");
    }
    if request.wasm_verifier_revision.as_str() != pin.wasm_verifier_revision() {
        bail!("request wasm-verifier revision does not match the pin");
    }
    if request.coq_wasm_tag != pin.coq_wasm_tag() {
        bail!("request coq-wasm tag does not match the pin");
    }
    if request.coq_wasm_revision.as_str() != pin.coq_wasm_revision() {
        bail!("request coq-wasm revision does not match the pin");
    }
    if request.coq_series != pin.coq_series() {
        bail!("request Coq series does not match the pin");
    }
    if request.assumption_allowlist_count != pin.assumption_allowlist_count() {
        bail!("request assumption allowlist ceiling does not match the pin");
    }
    if request.expected_case_count != CASES.len() {
        bail!("request aggregate case floor mismatch");
    }
    let proved: usize = CASES.iter().map(CaseSpec::expected_proved).sum();
    let refuted: usize = CASES.iter().map(CaseSpec::expected_refuted).sum();
    if request.expected_proved_endpoints != proved || request.expected_refuted_endpoints != refuted
    {
        bail!("request aggregate endpoint floor mismatch");
    }
    if request.cases.len() != CASES.len() {
        bail!("request case count does not match the selected case set");
    }
    for (actual, expected) in request.cases.iter().zip(CASES) {
        if actual.case_id != expected.id() {
            bail!("request case order or ID mismatch at `{}`", expected.id());
        }
        if actual.raw_basename != expected.raw_basename() {
            bail!("request raw basename mismatch for `{}`", expected.id());
        }
        if actual.expected_proved_endpoints != expected.expected_proved()
            || actual.expected_refuted_endpoints != expected.expected_refuted()
        {
            bail!("request endpoint count mismatch for `{}`", expected.id());
        }
    }
    let request_proved: usize = request
        .cases
        .iter()
        .map(|case| case.expected_proved_endpoints)
        .sum();
    let request_refuted: usize = request
        .cases
        .iter()
        .map(|case| case.expected_refuted_endpoints)
        .sum();
    if request.expected_case_count != request.cases.len()
        || request.expected_proved_endpoints != request_proved
        || request.expected_refuted_endpoints != request_refuted
    {
        bail!("request aggregate floor does not equal its case entries");
    }
    Ok(())
}

fn validate_raw(exchange: &Path, request: &Request) -> Result<()> {
    let raw_dir = exchange.join("raw");
    require_directory(&raw_dir, "raw")?;
    let expected: BTreeSet<_> = request
        .cases
        .iter()
        .map(|case| case.raw_basename.clone())
        .collect();
    let actual = directory_names(&raw_dir, "raw")?;
    if actual != expected {
        bail!("raw file set mismatch: expected {expected:?}, observed {actual:?}");
    }
    for case in &request.cases {
        let path = raw_dir.join(&case.raw_basename);
        require_regular_file(&path, "raw")?;
        let bytes = std::fs::read(&path).with_context(|| format!("read raw {}", path.display()))?;
        if RawHash::of(&bytes) != case.raw_sha256 {
            bail!("raw integrity failure for `{}`", case.case_id);
        }
    }
    Ok(())
}

fn validate_receipts(exchange: &Path, request: &Request, pin: &Pin) -> Result<()> {
    let receipt_dir = exchange.join("receipts");
    require_directory(&receipt_dir, "receipt")?;
    let expected_names: BTreeSet<_> = request
        .cases
        .iter()
        .map(|case| format!("{}.json", case.case_id))
        .collect();
    let actual_names = directory_names(&receipt_dir, "receipt")?;
    if actual_names != expected_names {
        bail!("receipt file set mismatch: expected {expected_names:?}, observed {actual_names:?}");
    }

    let mut total_proved = 0;
    let mut total_refuted = 0;
    for (request_case, expected_case) in request.cases.iter().zip(CASES) {
        let path = receipt_dir.join(format!("{}.json", request_case.case_id));
        require_regular_file(&path, "receipt")?;
        let bytes =
            std::fs::read(&path).with_context(|| format!("read receipt {}", path.display()))?;
        let receipt: Receipt = parse_strict(&bytes, "receipt")?;
        validate_receipt(&receipt, request_case, expected_case, request, pin)?;
        total_proved += receipt.proved;
        total_refuted += receipt.refuted;
    }
    if total_proved != request.expected_proved_endpoints
        || total_refuted != request.expected_refuted_endpoints
    {
        bail!("receipt aggregate floor mismatch");
    }
    Ok(())
}

fn validate_receipt(
    receipt: &Receipt,
    request_case: &RequestCase,
    expected_case: &CaseSpec,
    request: &Request,
    pin: &Pin,
) -> Result<()> {
    if receipt.protocol != PROTOCOL_VERSION {
        bail!("receipt protocol mismatch for `{}`", expected_case.id());
    }
    if receipt.case_id != expected_case.id() || receipt.case_id != request_case.case_id {
        bail!("receipt case mismatch for `{}`", expected_case.id());
    }
    if receipt.raw_basename != expected_case.raw_basename()
        || receipt.raw_basename != request_case.raw_basename
    {
        bail!("receipt basename mismatch for `{}`", expected_case.id());
    }
    if receipt.raw_sha256 != request_case.raw_sha256 {
        bail!("receipt raw hash mismatch for `{}`", expected_case.id());
    }
    if receipt.wasm_verifier_pinned != request.wasm_verifier_revision
        || receipt.wasm_verifier_observed != request.wasm_verifier_revision
    {
        bail!(
            "receipt wasm-verifier revision mismatch for `{}`",
            expected_case.id()
        );
    }
    if receipt.coq_wasm_pinned.as_str() != pin.coq_wasm_revision()
        || receipt.coq_wasm_observed.as_str() != pin.coq_wasm_revision()
    {
        bail!(
            "receipt coq-wasm revision mismatch for `{}`",
            expected_case.id()
        );
    }
    if !coq_version_matches_series(&receipt.coq_version, pin.coq_series()) {
        bail!("receipt Coq series mismatch for `{}`", expected_case.id());
    }
    if receipt.proved != expected_case.expected_proved()
        || receipt.refuted != expected_case.expected_refuted()
    {
        bail!(
            "receipt endpoint count mismatch for `{}`",
            expected_case.id()
        );
    }
    if receipt.audited_endpoints != receipt.proved + receipt.refuted {
        bail!(
            "receipt audited endpoint count mismatch for `{}`",
            expected_case.id()
        );
    }
    if receipt.allowlisted_dependencies > pin.assumption_allowlist_count() {
        bail!("receipt allowlisted dependency count exceeds the pin ceiling");
    }
    if receipt.raw_namespace_dependencies != 0 {
        bail!("receipt reports raw-namespace dependencies");
    }
    if receipt.unapproved_dependencies != 0 {
        bail!("receipt reports unapproved dependencies");
    }
    Ok(())
}

fn coq_version_matches_series(version: &str, series: &str) -> bool {
    parse_coq_version(version)
        .is_some_and(|actual| parse_coq_version(series).is_some_and(|expected| actual == expected))
}

fn parse_coq_version(version: &str) -> Option<(u64, u64)> {
    let bytes = version.as_bytes();
    let mut index = 0;
    let major = parse_decimal(bytes, &mut index)?;
    if bytes.get(index) != Some(&b'.') {
        return None;
    }
    index += 1;
    let minor = parse_decimal(bytes, &mut index)?;
    match bytes.get(index) {
        None => Some((major, minor)),
        Some(b'.') => {
            index += 1;
            parse_decimal(bytes, &mut index)?;
            (index == bytes.len()).then_some((major, minor))
        }
        Some(b'+') => {
            index += 1;
            valid_version_suffix(&bytes[index..]).then_some((major, minor))
        }
        _ => None,
    }
}

fn parse_decimal(bytes: &[u8], index: &mut usize) -> Option<u64> {
    let start = *index;
    let mut value = 0_u64;
    while let Some(byte) = bytes.get(*index)
        && byte.is_ascii_digit()
    {
        value = value
            .checked_mul(10)?
            .checked_add(u64::from(*byte - b'0'))?;
        *index += 1;
    }
    (*index > start).then_some(value)
}

fn valid_version_suffix(suffix: &[u8]) -> bool {
    suffix.first().is_some_and(u8::is_ascii_alphanumeric)
        && suffix.last().is_some_and(u8::is_ascii_alphanumeric)
        && suffix
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-'))
}

fn parse_strict<T: DeserializeOwned>(bytes: &[u8], label: &str) -> Result<T> {
    let strict: StrictValue =
        serde_json::from_slice(bytes).with_context(|| format!("parse strict {label} JSON"))?;
    serde_json::from_value(strict.0).with_context(|| format!("validate typed {label} JSON"))
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .map(StrictValue)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value.to_string())))
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(A::Error::custom(format!("duplicate JSON key `{key}`")));
            }
            let value = object.next_value::<StrictValue>()?;
            values.insert(key, value.0);
        }
        Ok(StrictValue(Value::Object(values)))
    }
}

fn require_exchange_root(exchange: &Path) -> Result<std::path::PathBuf> {
    if !exchange.is_absolute() {
        bail!("exchange path must be absolute");
    }
    require_directory(exchange, "exchange")?;
    std::fs::canonicalize(exchange)
        .with_context(|| format!("canonicalize exchange directory {}", exchange.display()))
}

fn require_exact_top_level(exchange: &Path) -> Result<()> {
    let expected = BTreeSet::from([
        "raw".to_string(),
        "receipts".to_string(),
        "request.json".to_string(),
    ]);
    let actual = directory_names(exchange, "exchange")?;
    if actual != expected {
        bail!("exchange entry set mismatch: expected {expected:?}, observed {actual:?}");
    }
    Ok(())
}

fn directory_names(directory: &Path, label: &str) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("read {label} directory {}", directory.display()))?
    {
        let entry = entry.with_context(|| format!("read entry in {}", directory.display()))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .with_context(|| format!("{label} directory contains a non-UTF-8 entry"))?;
        names.insert(name.to_string());
    }
    Ok(names)
}

fn require_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} directory {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "{label} path must be a nonsymlink directory: {}",
            path.display()
        );
    }
    Ok(())
}

fn require_regular_file(path: &Path, label: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} file {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "{label} path must be a nonsymlink regular file: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::cases::CASES;
    use super::super::pin::Pin;
    use super::super::{SUCCESS_LINE, verify};
    use super::verify_exchange_with_hook;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};

    struct ExchangeFixture {
        root: tempfile::TempDir,
        request: Value,
    }

    impl ExchangeFixture {
        fn valid() -> Self {
            let root = tempfile::tempdir().expect("create exchange fixture");
            let raw_dir = root.path().join("raw");
            let receipt_dir = root.path().join("receipts");
            std::fs::create_dir(&raw_dir).expect("create raw directory");
            std::fs::create_dir(&receipt_dir).expect("create receipt directory");
            let pin = Pin::read().expect("read pin");
            let mut request_cases = Vec::new();

            for case in CASES {
                let raw = format!("raw fixture for {}\n", case.id());
                std::fs::write(raw_dir.join(case.raw_basename()), raw.as_bytes())
                    .expect("write raw fixture");
                let raw_sha256 = hash(raw.as_bytes());
                request_cases.push(json!({
                    "case_id": case.id(),
                    "raw_basename": case.raw_basename(),
                    "raw_sha256": raw_sha256,
                    "expected_proved_endpoints": case.expected_proved(),
                    "expected_refuted_endpoints": case.expected_refuted(),
                }));
                let receipt = json!({
                    "protocol": 1,
                    "case_id": case.id(),
                    "raw_basename": case.raw_basename(),
                    "raw_sha256": raw_sha256,
                    "wasm_verifier_pinned": pin.wasm_verifier_revision(),
                    "wasm_verifier_observed": pin.wasm_verifier_revision(),
                    "coq_wasm_pinned": pin.coq_wasm_revision(),
                    "coq_wasm_observed": pin.coq_wasm_revision(),
                    "coq_version": pin.coq_series(),
                    "proved": case.expected_proved(),
                    "refuted": case.expected_refuted(),
                    "audited_endpoints": case.expected_proved() + case.expected_refuted(),
                    "allowlisted_dependencies": pin.assumption_allowlist_count(),
                    "raw_namespace_dependencies": 0,
                    "unapproved_dependencies": 0,
                    "result": "pass",
                });
                write_json(&receipt_dir.join(format!("{}.json", case.id())), &receipt);
            }

            let request = json!({
                "protocol": 1,
                "wasm_verifier_revision": pin.wasm_verifier_revision(),
                "coq_wasm_tag": pin.coq_wasm_tag(),
                "coq_wasm_revision": pin.coq_wasm_revision(),
                "coq_series": pin.coq_series(),
                "assumption_allowlist_count": pin.assumption_allowlist_count(),
                "expected_case_count": 6,
                "expected_proved_endpoints": 13,
                "expected_refuted_endpoints": 1,
                "cases": request_cases,
            });
            let fixture = Self { root, request };
            fixture.write_request();
            fixture
        }

        fn path(&self) -> &Path {
            self.root.path()
        }

        fn write_request(&self) {
            write_json(&self.path().join("request.json"), &self.request);
        }

        fn write_request_bytes(&self, bytes: &[u8]) {
            std::fs::write(self.path().join("request.json"), bytes).expect("write request bytes");
        }

        fn raw_path(&self, case_index: usize) -> PathBuf {
            self.path()
                .join("raw")
                .join(CASES[case_index].raw_basename())
        }

        fn receipt_path(&self, case_index: usize) -> PathBuf {
            self.path()
                .join("receipts")
                .join(format!("{}.json", CASES[case_index].id()))
        }

        fn mutate_receipt(&self, case_index: usize, mutate: impl FnOnce(&mut Value)) {
            let path = self.receipt_path(case_index);
            let mut receipt: Value =
                serde_json::from_slice(&std::fs::read(&path).expect("read receipt"))
                    .expect("parse fixture receipt");
            mutate(&mut receipt);
            write_json(&path, &receipt);
        }
    }

    fn hash(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        encoded
    }

    fn write_json(path: &Path, value: &Value) {
        std::fs::write(
            path,
            serde_json::to_vec(value).expect("serialize fixture JSON"),
        )
        .expect("write fixture JSON");
    }

    fn assert_all_rejected(labels: &[&str], mutate: impl Fn(&str, &mut ExchangeFixture)) {
        let mut accepted = Vec::new();
        for label in labels {
            let mut fixture = ExchangeFixture::valid();
            mutate(label, &mut fixture);
            if verify(fixture.path()).is_ok() {
                accepted.push(*label);
            }
        }
        assert!(
            accepted.is_empty(),
            "invalid protocol inputs were accepted: {accepted:?}"
        );
    }

    #[test]
    fn valid_exchange_verifies_with_the_exact_summary() {
        let fixture = ExchangeFixture::valid();
        assert_eq!(
            verify(fixture.path()).expect("verify fixture"),
            SUCCESS_LINE
        );
    }

    #[test]
    fn strict_json_rejects_unknown_duplicate_wrong_type_and_noncanonical_values() {
        let labels = [
            "request-unknown-top",
            "request-unknown-nested",
            "request-duplicate-top",
            "request-duplicate-nested",
            "request-wrong-type",
            "request-uppercase-revision",
            "request-uppercase-hash",
            "receipt-unknown",
            "receipt-duplicate",
            "receipt-wrong-type",
            "receipt-uppercase-revision",
            "receipt-uppercase-hash",
        ];
        assert_all_rejected(&labels, |label, fixture| match label {
            "request-unknown-top" => {
                fixture.request["unexpected"] = json!(true);
                fixture.write_request();
            }
            "request-unknown-nested" => {
                fixture.request["cases"][0]["unexpected"] = json!(true);
                fixture.write_request();
            }
            "request-duplicate-top" => {
                let text = serde_json::to_string(&fixture.request).expect("serialize request");
                let duplicate = text.replacen("\"protocol\":1", "\"protocol\":1,\"protocol\":1", 1);
                fixture.write_request_bytes(duplicate.as_bytes());
            }
            "request-duplicate-nested" => {
                let text = serde_json::to_string(&fixture.request).expect("serialize request");
                let duplicate = text.replacen(
                    "\"case_id\":\"prime-bounded\"",
                    "\"case_id\":\"prime-bounded\",\"case_id\":\"prime-bounded\"",
                    1,
                );
                fixture.write_request_bytes(duplicate.as_bytes());
            }
            "request-wrong-type" => {
                fixture.request["protocol"] = json!("1");
                fixture.write_request();
            }
            "request-uppercase-revision" => {
                fixture.request["wasm_verifier_revision"] =
                    json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
                fixture.write_request();
            }
            "request-uppercase-hash" => {
                fixture.request["cases"][0]["raw_sha256"] =
                    json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
                fixture.write_request();
            }
            "receipt-unknown" => {
                fixture.mutate_receipt(0, |receipt| receipt["unexpected"] = json!(true));
            }
            "receipt-duplicate" => {
                let path = fixture.receipt_path(0);
                let text = std::fs::read_to_string(&path).expect("read receipt");
                let duplicate = text.replacen("\"protocol\":1", "\"protocol\":1,\"protocol\":1", 1);
                std::fs::write(path, duplicate).expect("write duplicate receipt");
            }
            "receipt-wrong-type" => {
                fixture.mutate_receipt(0, |receipt| receipt["protocol"] = json!("1"));
            }
            "receipt-uppercase-revision" => fixture.mutate_receipt(0, |receipt| {
                receipt["wasm_verifier_observed"] =
                    json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
            }),
            "receipt-uppercase-hash" => fixture.mutate_receipt(0, |receipt| {
                receipt["raw_sha256"] =
                    json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
            }),
            _ => unreachable!("complete strict JSON mutation table"),
        });
    }

    #[test]
    fn request_must_match_pin_exact_case_order_counts_and_aggregate_floor() {
        let labels = [
            "wrong-order",
            "wrong-case",
            "wrong-basename",
            "wrong-hash",
            "wrong-case-counts",
            "wrong-verifier-revision",
            "wrong-coq-wasm-tag",
            "wrong-coq-wasm-revision",
            "wrong-coq-series",
            "wrong-allowlist-ceiling",
            "wrong-aggregate-cases",
            "wrong-aggregate-proved",
            "wrong-aggregate-refuted",
        ];
        assert_all_rejected(&labels, |label, fixture| {
            match label {
                "wrong-order" => fixture.request["cases"]
                    .as_array_mut()
                    .expect("cases array")
                    .swap(0, 1),
                "wrong-case" => fixture.request["cases"][0]["case_id"] = json!("other"),
                "wrong-basename" => {
                    fixture.request["cases"][0]["raw_basename"] = json!("other.v");
                }
                "wrong-hash" => {
                    fixture.request["cases"][0]["raw_sha256"] = json!("0".repeat(64));
                }
                "wrong-case-counts" => {
                    fixture.request["cases"][0]["expected_proved_endpoints"] = json!(3);
                }
                "wrong-verifier-revision" => {
                    fixture.request["wasm_verifier_revision"] = json!("0".repeat(40));
                }
                "wrong-coq-wasm-tag" => fixture.request["coq_wasm_tag"] = json!("other"),
                "wrong-coq-wasm-revision" => {
                    fixture.request["coq_wasm_revision"] = json!("0".repeat(40));
                }
                "wrong-coq-series" => fixture.request["coq_series"] = json!("8.19"),
                "wrong-allowlist-ceiling" => {
                    fixture.request["assumption_allowlist_count"] = json!(11);
                }
                "wrong-aggregate-cases" => fixture.request["expected_case_count"] = json!(5),
                "wrong-aggregate-proved" => {
                    fixture.request["expected_proved_endpoints"] = json!(12);
                }
                "wrong-aggregate-refuted" => {
                    fixture.request["expected_refuted_endpoints"] = json!(0);
                }
                _ => unreachable!("complete request mutation table"),
            }
            fixture.write_request();
        });
    }

    #[test]
    fn receipt_directory_requires_the_exact_fresh_receipt_set() {
        let labels = ["missing", "extra", "duplicate", "stale-non-empty"];
        assert_all_rejected(&labels, |label, fixture| match label {
            "missing" => std::fs::remove_file(fixture.receipt_path(0)).expect("remove receipt"),
            "extra" => std::fs::write(fixture.path().join("receipts").join("other.json"), b"{}")
                .expect("write extra receipt"),
            "duplicate" => std::fs::copy(
                fixture.receipt_path(0),
                fixture.path().join("receipts").join("duplicate.json"),
            )
            .map(|_| ())
            .expect("copy duplicate receipt"),
            "stale-non-empty" => std::fs::write(
                fixture.path().join("receipts").join("stale"),
                b"previous invocation",
            )
            .expect("write stale receipt"),
            _ => unreachable!("complete receipt-set mutation table"),
        });
    }

    #[test]
    fn every_receipt_field_and_assumption_count_is_validated() {
        let labels = [
            "wrong-protocol",
            "wrong-case",
            "wrong-basename",
            "wrong-hash",
            "wrong-pinned-verifier-revision",
            "wrong-observed-verifier-revision",
            "wrong-pinned-coq-wasm-revision",
            "wrong-observed-coq-wasm-revision",
            "wrong-coq-series",
            "wrong-proved-count",
            "wrong-refuted-count",
            "audited-not-sum",
            "allowlist-over-ceiling",
            "raw-namespace-nonzero",
            "unapproved-nonzero",
            "wrong-result",
        ];
        assert_all_rejected(&labels, |label, fixture| {
            fixture.mutate_receipt(0, |receipt| match label {
                "wrong-protocol" => receipt["protocol"] = json!(2),
                "wrong-case" => receipt["case_id"] = json!("other"),
                "wrong-basename" => receipt["raw_basename"] = json!("other.v"),
                "wrong-hash" => receipt["raw_sha256"] = json!("0".repeat(64)),
                "wrong-pinned-verifier-revision" => {
                    receipt["wasm_verifier_pinned"] = json!("0".repeat(40));
                }
                "wrong-observed-verifier-revision" => {
                    receipt["wasm_verifier_observed"] = json!("0".repeat(40));
                }
                "wrong-pinned-coq-wasm-revision" => {
                    receipt["coq_wasm_pinned"] = json!("0".repeat(40));
                }
                "wrong-observed-coq-wasm-revision" => {
                    receipt["coq_wasm_observed"] = json!("0".repeat(40));
                }
                "wrong-coq-series" => receipt["coq_version"] = json!("8.19.4"),
                "wrong-proved-count" => receipt["proved"] = json!(1),
                "wrong-refuted-count" => receipt["refuted"] = json!(1),
                "audited-not-sum" => receipt["audited_endpoints"] = json!(1),
                "allowlist-over-ceiling" => receipt["allowlisted_dependencies"] = json!(11),
                "raw-namespace-nonzero" => receipt["raw_namespace_dependencies"] = json!(1),
                "unapproved-nonzero" => receipt["unapproved_dependencies"] = json!(1),
                "wrong-result" => receipt["result"] = json!("fail"),
                _ => unreachable!("complete receipt-field mutation table"),
            });
        });
    }

    #[test]
    fn receipt_coq_version_uses_a_whole_string_grammar_for_the_pinned_series() {
        for version in ["8.20", "8.20.0", "8.20.123", "8.20+release", "8.20+rc-1"] {
            let fixture = ExchangeFixture::valid();
            fixture.mutate_receipt(0, |receipt| receipt["coq_version"] = json!(version));
            assert_eq!(
                verify(fixture.path()).expect("accept valid Coq version in pinned series"),
                SUCCESS_LINE
            );
        }

        let invalid = [
            "8.20.",
            "8.20+",
            "8.20\n",
            "8.20 trailing",
            "8.200",
            "8.20release",
            "8.20.one",
            "8.20.-1",
            "8.20.1+release",
            "8.x",
            "version-8.20",
        ];
        let mut accepted = Vec::new();
        for version in invalid {
            let fixture = ExchangeFixture::valid();
            fixture.mutate_receipt(0, |receipt| receipt["coq_version"] = json!(version));
            if verify(fixture.path()).is_ok() {
                accepted.push(version);
            }
        }
        assert!(
            accepted.is_empty(),
            "malformed Coq versions were accepted: {accepted:?}"
        );
    }

    #[test]
    fn raw_directory_requires_the_exact_unchanged_file_set() {
        let labels = ["mutated", "missing", "extra"];
        assert_all_rejected(&labels, |label, fixture| match label {
            "mutated" => {
                std::fs::write(fixture.raw_path(0), b"mutated raw\n").expect("mutate raw fixture")
            }
            "missing" => std::fs::remove_file(fixture.raw_path(0)).expect("remove raw fixture"),
            "extra" => std::fs::write(fixture.path().join("raw").join("extra.v"), b"extra")
                .expect("write extra raw fixture"),
            _ => unreachable!("complete raw-set mutation table"),
        });
    }

    #[cfg(unix)]
    #[test]
    fn verifier_rejects_raw_and_receipt_symlinks_that_escape_the_exchange() {
        for target in ["raw", "receipt"] {
            let fixture = ExchangeFixture::valid();
            let outside = tempfile::tempdir().expect("create outside directory");
            let inside_path = if target == "raw" {
                fixture.raw_path(0)
            } else {
                fixture.receipt_path(0)
            };
            let outside_path = outside.path().join("outside");
            std::fs::copy(&inside_path, &outside_path).expect("copy exact bytes outside exchange");
            std::fs::remove_file(&inside_path).expect("remove in-exchange file");
            std::os::unix::fs::symlink(&outside_path, &inside_path)
                .expect("create escaping symlink");

            let error = verify(fixture.path())
                .expect_err("escaping symlink must fail verification")
                .to_string();
            assert!(
                error.contains("nonsymlink"),
                "unexpected {target} error: {error}"
            );
        }
    }

    #[test]
    fn verifier_rejects_relative_exchange_paths_before_filesystem_access() {
        let error = verify(Path::new("relative-exchange"))
            .expect_err("relative verify exchange must fail")
            .to_string();
        assert!(error.contains("absolute"), "unexpected error: {error}");
    }

    #[cfg(unix)]
    #[test]
    fn stabilized_exchange_root_keeps_verify_inside_original_target() {
        let fixture = ExchangeFixture::valid();
        let parent = tempfile::tempdir().expect("create symlink parent");
        let ancestor = parent.path().join("ancestor");
        let original_parent = fixture.path().parent().expect("fixture has parent");
        let exchange_name = fixture.path().file_name().expect("fixture has basename");
        let injected_parent = parent.path().join("injected-parent");
        let injected_exchange = injected_parent.join(exchange_name);
        std::fs::create_dir_all(&injected_exchange).expect("create injected exchange");
        std::fs::write(injected_exchange.join("injected"), b"untrusted")
            .expect("write injected entry");
        std::os::unix::fs::symlink(original_parent, &ancestor).expect("create ancestor symlink");
        let caller_exchange = ancestor.join(exchange_name);

        verify_exchange_with_hook(&caller_exchange, || {
            std::fs::remove_file(&ancestor).expect("remove original ancestor symlink");
            std::os::unix::fs::symlink(&injected_parent, &ancestor)
                .expect("retarget ancestor symlink");
            Ok(())
        })
        .expect("verify through a stabilized exchange root");
    }
}

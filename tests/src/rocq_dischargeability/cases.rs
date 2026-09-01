pub(super) struct CaseSpec {
    id: &'static str,
    source_name: &'static str,
    module_name: &'static str,
    golden_path: &'static str,
    expected_proved: usize,
    expected_refuted: usize,
}

impl CaseSpec {
    const fn new(
        id: &'static str,
        source_name: &'static str,
        module_name: &'static str,
        golden_path: &'static str,
        expected_proved: usize,
        expected_refuted: usize,
    ) -> Self {
        assert!(is_safe_id(id));
        assert!(is_safe_basename(source_name, b".inf"));
        assert!(is_safe_module_name(module_name));
        assert!(is_safe_golden_path(golden_path));
        assert!(expected_proved + expected_refuted > 0);
        Self {
            id,
            source_name,
            module_name,
            golden_path,
            expected_proved,
            expected_refuted,
        }
    }

    pub(super) fn id(&self) -> &str {
        self.id
    }

    pub(super) fn source_name(&self) -> &str {
        self.source_name
    }

    pub(super) fn module_name(&self) -> &str {
        self.module_name
    }

    pub(super) fn golden_path(&self) -> &str {
        self.golden_path
    }

    pub(super) fn raw_basename(&self) -> &str {
        self.golden_path
            .rsplit('/')
            .next()
            .expect("a validated golden path has a basename")
    }

    pub(super) fn expected_proved(&self) -> usize {
        self.expected_proved
    }

    pub(super) fn expected_refuted(&self) -> usize {
        self.expected_refuted
    }
}

const fn is_safe_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'-' {
            return false;
        }
        index += 1;
    }
    true
}

const fn is_safe_basename(value: &str, suffix: &[u8]) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() <= suffix.len() {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'/' || bytes[index] == b'\\' {
            return false;
        }
        index += 1;
    }
    let suffix_start = bytes.len() - suffix.len();
    index = 0;
    while index < suffix.len() {
        if bytes[suffix_start + index] != suffix[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn is_safe_module_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'_' {
            return false;
        }
        index += 1;
    }
    true
}

const fn is_safe_golden_path(value: &str) -> bool {
    const PREFIX: &[u8] = b"tests/test_data/rocq/";
    let bytes = value.as_bytes();
    if bytes.len() <= PREFIX.len() + 2 {
        return false;
    }
    let mut index = 0;
    while index < PREFIX.len() {
        if bytes[index] != PREFIX[index] {
            return false;
        }
        index += 1;
    }
    is_safe_basename_tail(bytes, PREFIX.len(), b".v")
}

const fn is_safe_basename_tail(bytes: &[u8], start: usize, suffix: &[u8]) -> bool {
    if bytes.len() <= start + suffix.len() {
        return false;
    }
    let mut index = start;
    while index < bytes.len() {
        if bytes[index] == b'/' || bytes[index] == b'\\' {
            return false;
        }
        index += 1;
    }
    let suffix_start = bytes.len() - suffix.len();
    index = 0;
    while index < suffix.len() {
        if bytes[suffix_start + index] != suffix[index] {
            return false;
        }
        index += 1;
    }
    true
}

pub(super) const CASES: &[CaseSpec] = &[
    CaseSpec::new(
        "prime-bounded",
        "rocq_prime_bounded_example.inf",
        "rocq_prime_bounded_example",
        "tests/test_data/rocq/rocq_prime_bounded_example.v",
        2,
        0,
    ),
    CaseSpec::new(
        "exists",
        "rocq_exists_spec.inf",
        "rocq_exists_spec",
        "tests/test_data/rocq/rocq_exists_spec.v",
        3,
        0,
    ),
    CaseSpec::new(
        "unique",
        "rocq_unique_spec.inf",
        "rocq_unique_spec",
        "tests/test_data/rocq/rocq_unique_spec.v",
        3,
        0,
    ),
    CaseSpec::new(
        "narrow-domain",
        "spec_narrow_discharge.inf",
        "spec_narrow_discharge",
        "tests/test_data/rocq/spec_narrow_discharge.v",
        2,
        0,
    ),
    CaseSpec::new(
        "false-spec",
        "rocq_false_certificate.inf",
        "rocq_false_certificate",
        "tests/test_data/rocq/rocq_false_certificate.v",
        1,
        1,
    ),
];

#[cfg(test)]
mod tests {
    use super::CASES;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn selected_cases_have_the_exact_order_and_counts() {
        let actual: Vec<_> = CASES
            .iter()
            .map(|case| {
                (
                    case.id(),
                    case.source_name(),
                    case.module_name(),
                    case.raw_basename(),
                    case.expected_proved(),
                    case.expected_refuted(),
                )
            })
            .collect();

        assert_eq!(
            actual,
            [
                (
                    "prime-bounded",
                    "rocq_prime_bounded_example.inf",
                    "rocq_prime_bounded_example",
                    "rocq_prime_bounded_example.v",
                    2,
                    0,
                ),
                (
                    "exists",
                    "rocq_exists_spec.inf",
                    "rocq_exists_spec",
                    "rocq_exists_spec.v",
                    3,
                    0,
                ),
                (
                    "unique",
                    "rocq_unique_spec.inf",
                    "rocq_unique_spec",
                    "rocq_unique_spec.v",
                    3,
                    0,
                ),
                (
                    "narrow-domain",
                    "spec_narrow_discharge.inf",
                    "spec_narrow_discharge",
                    "spec_narrow_discharge.v",
                    2,
                    0,
                ),
                (
                    "false-spec",
                    "rocq_false_certificate.inf",
                    "rocq_false_certificate",
                    "rocq_false_certificate.v",
                    1,
                    1,
                ),
            ]
        );
    }

    #[test]
    fn selected_case_ids_and_basenames_are_unique_and_safe() {
        assert_eq!(
            CASES.len(),
            5,
            "the uniqueness checks must cover five cases"
        );
        let mut ids = BTreeSet::new();
        let mut basenames = BTreeSet::new();

        for case in CASES {
            assert!(ids.insert(case.id()), "duplicate case ID: {}", case.id());
            assert!(
                case.id()
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
                "unsafe case ID: {}",
                case.id()
            );
            assert!(!case.id().is_empty(), "case IDs must not be empty");
            assert!(
                basenames.insert(case.raw_basename()),
                "duplicate raw basename: {}",
                case.raw_basename()
            );
            assert_eq!(
                Path::new(case.raw_basename()).file_name(),
                Some(case.raw_basename().as_ref()),
                "raw basename must not contain a path: {}",
                case.raw_basename()
            );
        }
    }

    #[test]
    fn selected_case_floor_is_five_cases_eleven_proved_one_refuted() {
        assert_eq!(CASES.len(), 5);
        assert_eq!(
            CASES
                .iter()
                .map(|case| case.expected_proved())
                .sum::<usize>(),
            11
        );
        assert_eq!(
            CASES
                .iter()
                .map(|case| case.expected_refuted())
                .sum::<usize>(),
            1
        );
    }

    #[test]
    fn selected_case_counts_agree_with_committed_golden_theorems() {
        assert_eq!(CASES.len(), 5, "the golden checks must cover five cases");
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("tests crate has a repository parent");

        for case in CASES {
            let golden_path = repository.join(case.golden_path());
            let golden = std::fs::read_to_string(&golden_path)
                .unwrap_or_else(|error| panic!("read {}: {error}", golden_path.display()));
            let theorem_count = golden
                .lines()
                .filter(|line| line.starts_with("Theorem "))
                .count();
            assert_eq!(
                theorem_count,
                case.expected_proved() + case.expected_refuted(),
                "golden theorem count drifted for {}",
                case.id()
            );
        }
    }
}

use crate::rocq_test_support::{ExternalBytes, LinkedExternal};
use inference::{ExternalSpecPolicy, LinkOptions};
use inference_wasm_codegen::CompilationMode;

/// How a case's producer reaches the bytes it publishes.
///
/// The externals a case merges and the policy that merge runs under are one
/// decision rather than two: the policy governs a merge, and it decides which
/// obligations the merged module states — so it decides how many theorems the
/// raw artifact carries, which is the number the case's expected proved and
/// refuted counts describe. Spelling them as one enum leaves "no externals,
/// but a policy" unrepresentable, and leaves the choice of producer a match on
/// this rather than a test on an empty list.
pub(super) enum Linkage {
    /// One compilation, translated directly.
    SingleFile,
    /// Compiled with its libraries separately, statically merged under
    /// `options`, and translated post-merge.
    ///
    /// This changes which producer runs, not merely what it is given: the raw
    /// artifact becomes the translation of a *merged* module, so the theorem
    /// types the companion discharges are about bodies no single compilation
    /// emitted.
    Merged {
        externals: &'static [LinkedExternal],
        options: LinkOptions,
    },
}

pub(super) struct CaseSpec {
    id: &'static str,
    source_name: &'static str,
    module_name: &'static str,
    linkage: Linkage,
    golden_path: &'static str,
    expected_proved: usize,
    expected_refuted: usize,
}

impl CaseSpec {
    const fn new(
        id: &'static str,
        source_name: &'static str,
        module_name: &'static str,
        linkage: Linkage,
        golden_path: &'static str,
        expected_proved: usize,
        expected_refuted: usize,
    ) -> Self {
        assert!(is_safe_id(id));
        assert!(is_safe_basename(source_name, b".inf"));
        assert!(is_safe_module_name(module_name));
        assert!(are_safe_externals(externals_of(&linkage)));
        assert!(is_safe_golden_path(golden_path));
        assert!(expected_proved + expected_refuted > 0);
        Self {
            id,
            source_name,
            module_name,
            linkage,
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

    /// How this case's raw bytes are produced.
    pub(super) fn linkage(&self) -> &Linkage {
        &self.linkage
    }

    /// The external `.wasm` modules this case's producer merges before
    /// translating, empty for a case compiled from one file.
    pub(super) fn externals(&self) -> &'static [LinkedExternal] {
        externals_of(&self.linkage)
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

const fn externals_of(linkage: &Linkage) -> &'static [LinkedExternal] {
    match linkage {
        Linkage::SingleFile => &[],
        Linkage::Merged { externals, .. } => externals,
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

/// Every external a case links, held to the same shape rules as the case's own
/// names: a logical module is a module name, a fixture-built external names an
/// `.inf` under `tests/test_data/inf/` and the module it compiles under, and a
/// committed one names a `.wasm` under `tests/test_data/wasmlib/`. All of it is
/// checked at compile time, so an unusable path cannot reach the exchange.
const fn are_safe_externals(externals: &[LinkedExternal]) -> bool {
    let mut index = 0;
    while index < externals.len() {
        let external = &externals[index];
        if !is_safe_module_name(external.logical_module) {
            return false;
        }
        match &external.bytes {
            ExternalBytes::Fixture {
                source,
                module_name,
                ..
            } => {
                if !is_safe_basename(source, b".inf") || !is_safe_module_name(module_name) {
                    return false;
                }
            }
            ExternalBytes::Artifact { file } => {
                if !is_safe_basename(file, b".wasm") {
                    return false;
                }
            }
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

/// The selected artifacts, in the wire order the request and the receipts are
/// keyed by. Appending is the only safe edit: a case's position is what its
/// receipt is matched against, so inserting one renames every later position.
///
/// `linked-extern` is the only entry whose producer runs more than one
/// compilation. Its raw bytes are the translation of a *merged* module, so it
/// is the only selected artifact whose theorems are about a body the compiler
/// never emitted — every other case's obligations apply functions from their
/// own translation unit, which no amount of them can turn into linked
/// coverage.
pub(super) const CASES: &[CaseSpec] = &[
    CaseSpec::new(
        "prime-bounded",
        "rocq_prime_bounded_example.inf",
        "rocq_prime_bounded_example",
        Linkage::SingleFile,
        "tests/test_data/rocq/rocq_prime_bounded_example.v",
        2,
        0,
    ),
    CaseSpec::new(
        "exists",
        "rocq_exists_spec.inf",
        "rocq_exists_spec",
        Linkage::SingleFile,
        "tests/test_data/rocq/rocq_exists_spec.v",
        3,
        0,
    ),
    CaseSpec::new(
        "unique",
        "rocq_unique_spec.inf",
        "rocq_unique_spec",
        Linkage::SingleFile,
        "tests/test_data/rocq/rocq_unique_spec.v",
        3,
        0,
    ),
    CaseSpec::new(
        "narrow-domain",
        "spec_narrow_discharge.inf",
        "spec_narrow_discharge",
        Linkage::SingleFile,
        "tests/test_data/rocq/spec_narrow_discharge.v",
        2,
        0,
    ),
    CaseSpec::new(
        "false-spec",
        "rocq_false_certificate.inf",
        "rocq_false_certificate",
        Linkage::SingleFile,
        "tests/test_data/rocq/rocq_false_certificate.v",
        1,
        1,
    ),
    CaseSpec::new(
        "linked-extern",
        "spec_linked_extern.inf",
        "spec_linked_extern",
        Linkage::Merged {
            externals: &[LinkedExternal {
                logical_module: "mathlib",
                bytes: ExternalBytes::Fixture {
                    source: "spec_linked_extern_mathlib.inf",
                    module_name: "mathlib_impl",
                    mode: CompilationMode::Compile,
                },
            }],
            options: LinkOptions {
                external_specs: ExternalSpecPolicy::Warn,
            },
        },
        "tests/test_data/rocq/spec_linked_extern.v",
        2,
        0,
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
                    case.externals()
                        .iter()
                        .map(|external| external.logical_module)
                        .collect::<Vec<_>>(),
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
                    vec![],
                    "rocq_prime_bounded_example.v",
                    2,
                    0,
                ),
                (
                    "exists",
                    "rocq_exists_spec.inf",
                    "rocq_exists_spec",
                    vec![],
                    "rocq_exists_spec.v",
                    3,
                    0,
                ),
                (
                    "unique",
                    "rocq_unique_spec.inf",
                    "rocq_unique_spec",
                    vec![],
                    "rocq_unique_spec.v",
                    3,
                    0,
                ),
                (
                    "narrow-domain",
                    "spec_narrow_discharge.inf",
                    "spec_narrow_discharge",
                    vec![],
                    "spec_narrow_discharge.v",
                    2,
                    0,
                ),
                (
                    "false-spec",
                    "rocq_false_certificate.inf",
                    "rocq_false_certificate",
                    vec![],
                    "rocq_false_certificate.v",
                    1,
                    1,
                ),
                (
                    "linked-extern",
                    "spec_linked_extern.inf",
                    "spec_linked_extern",
                    vec!["mathlib"],
                    "spec_linked_extern.v",
                    2,
                    0,
                ),
            ]
        );
    }

    #[test]
    fn selected_case_ids_and_basenames_are_unique_and_safe() {
        assert_eq!(CASES.len(), 6, "the uniqueness checks must cover six cases");
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
    fn selected_case_floor_is_six_cases_thirteen_proved_one_refuted() {
        assert_eq!(CASES.len(), 6);
        assert_eq!(
            CASES
                .iter()
                .map(|case| case.expected_proved())
                .sum::<usize>(),
            13
        );
        assert_eq!(
            CASES
                .iter()
                .map(|case| case.expected_refuted())
                .sum::<usize>(),
            1
        );
    }

    /// The printed success marker states the aggregate this list carries.
    ///
    /// Every consumer of the marker compares `verify()`'s output against the
    /// same constant it returns, which is true whatever the constant says. The
    /// numbers in it are a claim about *these* cases, so they are derived here
    /// and compared, leaving the CI lane's copy of the marker the only literal
    /// and no way to leave it behind when a case is appended.
    #[test]
    fn the_success_marker_states_the_aggregate_the_cases_carry() {
        let derived = format!(
            "rocq-discharge: result=pass cases={} proved={} refuted={}",
            CASES.len(),
            CASES
                .iter()
                .map(|case| case.expected_proved())
                .sum::<usize>(),
            CASES
                .iter()
                .map(|case| case.expected_refuted())
                .sum::<usize>(),
        );

        assert_eq!(
            super::super::SUCCESS_LINE,
            derived,
            "the success marker no longer states this list's aggregate; the \
             protected lane greps for the marker, so a stale one reports a \
             discharge that never happened"
        );
    }

    /// The linked case is produced from the same inputs as the `coqc` corpus
    /// entry it shares a golden with.
    ///
    /// `tests/test_data/rocq/spec_linked_extern.v` is regenerated through the
    /// corpus entry and validated through this case. The two lists state the
    /// producer's inputs separately — the module name, the externals with
    /// their sources and compilation modes, and the merge policy — and every
    /// one of them decides what the merged module contains, so sharing the
    /// generator does not make them agree. A divergence would surface as a
    /// golden mismatch in
    /// [`super::super::tests::export_fresh_generates_golden_equal_raw_and_hashes_exact_written_bytes`],
    /// which is loud but names neither of the two literals that disagree.
    #[test]
    fn the_linked_case_merges_under_the_corpus_entrys_policy() {
        const FIXTURE: &str = "spec_linked_extern.inf";

        let case = CASES
            .iter()
            .find(|case| case.source_name() == FIXTURE)
            .expect("the linked case must stay in this list");
        let super::Linkage::Merged { externals, options } = case.linkage() else {
            panic!("{FIXTURE} must stay a merged case for this comparison to mean anything");
        };
        let (_, corpus_module, corpus_externals, corpus_options) =
            crate::rocq_typecheck::gate::LINKED_CORPUS
                .iter()
                .find(|(source, _, _, _)| *source == FIXTURE)
                .expect(
                    "the corpus entry the golden is regenerated through must stay in that list",
                );

        assert_eq!(
            case.module_name(),
            *corpus_module,
            "{FIXTURE} is translated under one module name for the `coqc` gate and \
             another for the discharge export, so every `Definition` and `Theorem` \
             name a companion proof binds would differ from what `coqc` elaborates"
        );
        assert_eq!(
            externals, corpus_externals,
            "{FIXTURE} is merged with different libraries, or the same library \
             compiled differently, for the `coqc` gate and for the discharge \
             export, so the merged bodies a companion proof realizes are not the \
             bodies `coqc` elaborates"
        );
        assert_eq!(
            options, corpus_options,
            "{FIXTURE} is linked one way for the `coqc` gate and another for the \
             discharge export, so the bytes a companion proof is written against \
             are not the bytes `coqc` elaborates"
        );
    }

    #[test]
    fn selected_case_counts_agree_with_committed_golden_theorems() {
        assert_eq!(CASES.len(), 6, "the golden checks must cover six cases");
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

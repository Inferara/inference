//! The stable half of the `link_memory` fuzz target: a hand-pinned seed table,
//! the seed corpus committed from it, a totality probe of the input format,
//! and a deterministic sweep, all run through the target's own generator and
//! checks.
//!
//! The libFuzzer harness in `core/wasm-linker/fuzz/fuzz_targets/link_memory.rs`
//! needs `cargo-fuzz` and nightly, and CI fuzzes it only on Linux. This file
//! includes the same `core/wasm-linker/fuzz/src/link_memory.rs` the target
//! does, so every `cargo test` runs literally the same generator, oracle and
//! property checks, on every platform:
//!
//! - [`every_seed_reaches_its_pinned_outcome`] links each seed and compares the
//!   outcome with a pin derived by hand from the documented memory rule. The
//!   pins are a second witness beside the oracle: a mistake the oracle and the
//!   linker share still goes red here.
//! - [`the_seed_table_covers_every_rule_branch`] keeps the table from
//!   regressing to vacuity.
//! - [`committed_link_memory_seeds_are_current`] holds the corpus at
//!   `core/wasm-linker/fuzz/seeds/link_memory/`, which the fuzzer and
//!   ClusterFuzzLite start from, to the table: each seed's file is the `encode`
//!   of its spec and decodes back to it, and no other file rides along.
//! - [`regenerate::regenerate_link_memory_seeds`] (`#[ignore]`d) rewrites the
//!   corpus from the table, so the files are reproducible rather than opaque.
//!   Run it with:
//!   `cargo test -p inference-wasm-linker --test link_memory_seeds regenerate -- --ignored`.
//! - [`decoding_is_total`] sets each byte of an input whose every slot is in
//!   use to each of its 256 values, and checks that every result decodes to a
//!   spec the generator builds, and that each byte changes the spec for some
//!   value.
//! - [`a_deterministic_sweep_upholds_every_property`] checks pseudo-random
//!   inputs, so the generative coverage does not depend on a fuzzer being
//!   available.

#[path = "../fuzz/src/link_memory.rs"]
mod link_memory;

use std::collections::BTreeSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use inference_wasm_linker::{LinkError, LinkWarning};
use link_memory::Body::{Grow, GrowViaHelper, Load, Pure, Size};
use link_memory::{
    Body, Expected, INPUT_LEN, ImportSpec, Limits, LinkSpec, MAX_IMPORTS, MAX_PROVIDERS, Outcome,
    ProviderSpec, Refusal, check, decode, encode, expected, generate, match_refusal,
};

/// What a seed must produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pinned {
    /// Links with this memory. `tier_b` holds the fields the Tier-B warning
    /// names, in order, and is empty when no warning is raised.
    Linked {
        memory: Option<(u64, Option<u64>)>,
        tier_b: &'static [&'static str],
    },
    /// Is refused, reporting this kind.
    Refused(Refusal),
}

impl Pinned {
    /// The fields the pin's Tier-B warning names: none for a refusal, or for a
    /// link that raises no warning.
    fn tier_b(self) -> &'static [&'static str] {
        match self {
            Pinned::Linked { tier_b, .. } => tier_b,
            Pinned::Refused(_) => &[],
        }
    }
}

struct Seed {
    name: &'static str,
    spec: LinkSpec,
    pinned: Pinned,
}

/// A memory of `min` pages that may grow to `max`, or without bound.
fn mem(min: u64, max: Option<u64>) -> Option<Limits> {
    Some(Limits { min, max })
}

fn provider(memory: Option<Limits>, exports: &[Body]) -> ProviderSpec {
    ProviderSpec {
        memory,
        exports: exports.to_vec(),
    }
}

/// Export `f{export}` of provider `m{provider}`.
fn imp(provider: usize, export: usize) -> ImportSpec {
    ImportSpec { provider, export }
}

/// A spec importing `m0.f0 m1.f0 …`, one import per provider in provider
/// order, linked in that order.
fn each_f0(main_memory: Option<Limits>, providers: Vec<ProviderSpec>) -> LinkSpec {
    let imports = (0..providers.len()).map(|p| imp(p, 0)).collect();
    with_imports(main_memory, providers, imports)
}

/// A spec importing `imports`, whose shuffle is the identity.
fn with_imports(
    main_memory: Option<Limits>,
    providers: Vec<ProviderSpec>,
    imports: Vec<ImportSpec>,
) -> LinkSpec {
    let shuffle = (0..imports.len() as u8).collect();
    LinkSpec {
        main_memory,
        providers,
        imports,
        shuffle,
    }
}

fn linked(memory: Option<(u64, Option<u64>)>) -> Pinned {
    Pinned::Linked {
        memory,
        tier_b: &[],
    }
}

/// Links with `memory` and a Tier-B warning naming `fields`.
fn linked_tier_b(memory: (u64, Option<u64>), fields: &'static [&'static str]) -> Pinned {
    Pinned::Linked {
        memory: Some(memory),
        tier_b: fields,
    }
}

/// The seed table. Each pin is derived by hand from the documented rule: with
/// main's memory, main's maximum over the largest contributed minimum; without
/// it, the largest contributed minimum and the smallest declared maximum. Only
/// an import whose body touches memory contributes, once per import entry.
fn seeds() -> Vec<Seed> {
    let seed = |name, spec, pinned| Seed { name, spec, pinned };
    vec![
        seed(
            "pure_no_memory",
            each_f0(None, vec![provider(None, &[Pure])]),
            linked(None),
        ),
        // The provider's memory is not imported through anything that uses
        // it: only `f1` loads, and main imports `f0`.
        seed(
            "idle_memory_contributes_nothing",
            each_f0(None, vec![provider(mem(10, Some(10)), &[Pure, Load])]),
            linked(None),
        ),
        seed(
            "single_user_adopted",
            each_f0(None, vec![provider(mem(2, Some(10)), &[Load])]),
            linked_tier_b((2, Some(10)), &["f0"]),
        ),
        seed(
            "meet_of_two_users",
            each_f0(
                None,
                vec![
                    provider(mem(1, Some(10)), &[Load]),
                    provider(mem(1, Some(2)), &[Load]),
                ],
            ),
            linked(Some((1, Some(2)))),
        ),
        // The grower's cap is the shared one, as in `link.rs`'s
        // `a_grower_keeps_its_own_cap_beside_a_wider_reader`.
        seed(
            "grower_beside_user",
            each_f0(
                None,
                vec![
                    provider(mem(1, Some(2)), &[Grow]),
                    provider(mem(1, Some(10)), &[Load]),
                ],
            ),
            linked(Some((1, Some(2)))),
        ),
        // Two growers whose caps differ, as in `link.rs`'s
        // `two_growers_with_different_caps_are_rejected_in_either_order`.
        seed(
            "two_growers_caps_differ",
            each_f0(
                None,
                vec![
                    provider(mem(1, Some(10)), &[Grow]),
                    provider(mem(1, Some(2)), &[Grow]),
                ],
            ),
            Pinned::Refused(Refusal::GrowerCapMoved),
        ),
        // A reader pins the memory below the grower's cap, as in `link.rs`'s
        // `a_grower_capped_below_its_own_maximum_by_a_reader_is_rejected`. The
        // meet is (5, 5), so the grower's cap moves from 10 and the result is
        // pinned: two refusals apply. The linker reports the moved cap, the
        // message #388 settled on for this case.
        seed(
            "grower_capped_by_a_pinned_user",
            each_f0(
                None,
                vec![
                    provider(mem(1, Some(10)), &[Grow]),
                    provider(mem(5, Some(5)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::GrowerCapMoved),
        ),
        // One reader's minimum is above another's maximum, as in `link.rs`'s
        // `declarations_no_one_memory_satisfies_are_rejected_in_either_order`.
        seed(
            "no_memory_fits",
            each_f0(
                None,
                vec![
                    provider(mem(5, Some(5)), &[Load]),
                    provider(mem(1, Some(2)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::NoFit),
        ),
        // Two contributors tie on the largest minimum (`m0.f0` and `m1.f1`),
        // and the reversed run meets them in the other order.
        seed(
            "refusal_names_do_not_follow_import_order",
            LinkSpec {
                main_memory: None,
                providers: vec![
                    provider(mem(5, Some(5)), &[Load]),
                    provider(mem(5, None), &[Pure, Load]),
                    provider(mem(1, Some(2)), &[Load]),
                ],
                imports: vec![imp(1, 1), imp(2, 0), imp(0, 0)],
                shuffle: vec![2, 0, 1],
            },
            Pinned::Refused(Refusal::NoFit),
        ),
        // The user listed after the grower pins the memory, as in `link.rs`'s
        // `growth_against_a_memory_a_later_import_pins_is_rejected_onto_a_memoryless_main`.
        seed(
            "growth_pinned_by_a_later_import",
            each_f0(
                None,
                vec![
                    provider(mem(1, Some(10)), &[Grow]),
                    provider(mem(10, Some(10)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::GrowthPinned),
        ),
        seed(
            "lone_pinned_grower",
            each_f0(None, vec![provider(mem(5, Some(5)), &[Grow])]),
            Pinned::Refused(Refusal::GrowthPinned),
        ),
        seed(
            "unbounded_meets_bounded",
            each_f0(
                None,
                vec![
                    provider(mem(1, None), &[Load]),
                    provider(mem(1, Some(5)), &[Load]),
                ],
            ),
            linked(Some((1, Some(5)))),
        ),
        seed(
            "unbounded_grower_is_capped",
            each_f0(
                None,
                vec![
                    provider(mem(1, None), &[Grow]),
                    provider(mem(1, Some(5)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::GrowerCapMoved),
        ),
        seed(
            "grow_through_a_helper",
            each_f0(
                None,
                vec![
                    provider(mem(1, Some(10)), &[GrowViaHelper]),
                    provider(mem(1, Some(2)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::GrowerCapMoved),
        ),
        seed(
            "memory_size_is_a_use",
            each_f0(
                None,
                vec![
                    provider(mem(3, None), &[Size]),
                    provider(mem(1, Some(2)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::NoFit),
        ),
        seed(
            "duplicate_import",
            with_imports(
                None,
                vec![provider(mem(1, Some(2)), &[Grow])],
                vec![imp(0, 0), imp(0, 0)],
            ),
            linked(Some((1, Some(2)))),
        ),
        // Main's maximum governs two growers whose caps differ, as in
        // `link.rs`'s `mains_own_maximum_governs_two_growers_in_either_order`.
        seed(
            "main_governs_two_growers",
            each_f0(
                mem(1, Some(10)),
                vec![
                    provider(mem(1, Some(10)), &[Grow]),
                    provider(mem(1, Some(2)), &[Grow]),
                ],
            ),
            linked(Some((1, Some(10)))),
        ),
        // A later import pins main's memory under a grower, as in `link.rs`'s
        // `growth_against_a_memory_a_later_import_pins_is_rejected_under_mains_own_memory`.
        seed(
            "main_growth_pinned_by_external",
            each_f0(
                mem(1, Some(10)),
                vec![
                    provider(mem(1, Some(10)), &[Grow]),
                    provider(mem(10, Some(10)), &[Load]),
                ],
            ),
            Pinned::Refused(Refusal::GrowthPinned),
        ),
        seed(
            "main_too_small",
            each_f0(mem(1, Some(1)), vec![provider(mem(2, None), &[Load])]),
            Pinned::Refused(Refusal::MainTooSmall),
        ),
        // Main's minimum is above the one contributed, so it is the output's,
        // and its three pages raise the warning the contributed one page would
        // not.
        seed(
            "main_minimum_dominates",
            each_f0(mem(3, Some(10)), vec![provider(mem(1, Some(10)), &[Load])]),
            linked_tier_b((3, Some(10)), &["f0"]),
        ),
        seed(
            "main_without_imports",
            with_imports(
                mem(3, Some(5)),
                vec![provider(mem(1, Some(2)), &[Grow])],
                vec![],
            ),
            linked(Some((3, Some(5)))),
        ),
        seed(
            "page_limit_boundary",
            each_f0(
                mem(0, Some(65536)),
                vec![provider(mem(65536, None), &[Load])],
            ),
            linked_tier_b((65536, Some(65536)), &["f0"]),
        ),
        // Main imports `f1` first; the warning names `f0` first.
        seed(
            "tier_b_fields_are_sorted",
            with_imports(
                None,
                vec![provider(mem(2, None), &[Load, Size])],
                vec![imp(0, 1), imp(0, 0)],
            ),
            linked_tier_b((2, None), &["f0", "f1"]),
        ),
        // `f1` is pure, so the warning names only `f0`, although both come
        // from the module whose memory the output adopts.
        seed(
            "tier_b_fields_omit_a_pure_import",
            with_imports(
                None,
                vec![provider(mem(2, Some(10)), &[Load, Pure])],
                vec![imp(0, 0), imp(0, 1)],
            ),
            linked_tier_b((2, Some(10)), &["f0"]),
        ),
        // The contributors are m0.f0 (2, 10) twice, m1.f0 (1, 10),
        // m3.f0 (3, unbounded), m1.f1 (1, 10) and m3.f2 (3, unbounded); m2.f1
        // and m0.f2 are pure. So the minimum is 3 and the maximum 10, and both
        // growers (m0.f0 and m1.f0) are capped at 10, above 3. `f2` is named
        // for m3.f2, although m0.f2 is pure.
        seed(
            "four_providers_eight_imports",
            LinkSpec {
                main_memory: None,
                providers: vec![
                    provider(mem(2, Some(10)), &[Grow, Load, Pure]),
                    provider(mem(1, Some(10)), &[GrowViaHelper, Size]),
                    provider(None, &[Pure, Pure]),
                    provider(mem(3, None), &[Load, Pure, Size]),
                ],
                imports: vec![
                    imp(0, 0),
                    imp(1, 0),
                    imp(3, 0),
                    imp(2, 1),
                    imp(0, 0),
                    imp(1, 1),
                    imp(3, 2),
                    imp(0, 2),
                ],
                shuffle: vec![7, 3, 5, 1, 8, 2, 6, 4],
            },
            linked_tier_b((3, Some(10)), &["f0", "f1", "f2"]),
        ),
    ]
}

/// Runs `check`, turning a panic or a violation into a message naming the
/// case.
fn try_check(name: &str, spec: &LinkSpec) -> Result<Outcome, String> {
    match catch_unwind(AssertUnwindSafe(|| check(spec))) {
        Ok(Ok(outcome)) => Ok(outcome),
        Ok(Err(violation)) => Err(format!("`{name}`:\n{violation}")),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("a non-string panic payload");
            Err(format!("`{name}`: checking it panicked: {message}"))
        }
    }
}

/// Whether `outcome` is what `pinned` says.
fn reaches(outcome: &Outcome, pinned: Pinned) -> bool {
    match (outcome, pinned) {
        (
            Outcome::Linked { memory, warnings },
            Pinned::Linked {
                memory: pin,
                tier_b,
            },
        ) => *memory == pin && *warnings == pinned_warnings(pin, tier_b),
        (Outcome::Refused { kind, .. }, Pinned::Refused(pin)) => *kind == pin,
        _ => false,
    }
}

/// The warnings a `Linked` pin stands for: none when `tier_b` is empty, and
/// otherwise one Tier-B warning naming `tier_b` at the memory's minimum.
fn pinned_warnings(memory: Option<(u64, Option<u64>)>, tier_b: &[&str]) -> Vec<LinkWarning> {
    if tier_b.is_empty() {
        return Vec::new();
    }
    let (pages, _) = memory.expect("a pin naming Tier-B fields links with a memory");
    vec![LinkWarning::TierBInMultiPageMemory {
        fields: tier_b.iter().map(ToString::to_string).collect(),
        pages,
    }]
}

/// The limits each import whose body touches memory contributes, once per
/// import entry.
fn contributed(spec: &LinkSpec) -> Vec<Limits> {
    spec.imports
        .iter()
        .filter_map(|import| {
            let provider = &spec.providers[import.provider];
            let limits = provider.memory?;
            provider.exports[import.export]
                .uses_memory()
                .then_some(limits)
        })
        .collect()
}

/// Every seed is checked before the test fails, so a regression reports the
/// whole set of seeds it breaks rather than the first.
#[test]
fn every_seed_reaches_its_pinned_outcome() {
    let seeds = seeds();
    let mut failures = Vec::new();
    for seed in &seeds {
        let encoded = encode(&seed.spec);
        assert_eq!(
            encoded.len(),
            57,
            "`{}`: the encoding is the documented 57 bytes wide",
            seed.name
        );
        assert_eq!(
            decode(&encoded),
            seed.spec,
            "`{}`: decode(encode(spec)) must give the spec back",
            seed.name
        );
        match try_check(seed.name, &seed.spec) {
            Ok(outcome) if reaches(&outcome, seed.pinned) => {}
            Ok(outcome) => failures.push(format!(
                "`{}`: pinned {:?}, but checking it gave {outcome:#?}",
                seed.name, seed.pinned
            )),
            Err(failure) => failures.push(failure),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} seeds failed:\n\n{}",
        failures.len(),
        seeds.len(),
        failures.join("\n")
    );
}

#[test]
fn the_seed_table_covers_every_rule_branch() {
    let seeds = seeds();
    let pins: Vec<Pinned> = seeds.iter().map(|s| s.pinned).collect();
    for kind in Refusal::ALL {
        assert!(
            pins.contains(&Pinned::Refused(kind)),
            "no seed is refused as {kind:?}"
        );
    }
    assert!(
        pins.iter()
            .any(|p| matches!(p, Pinned::Linked { memory: None, .. })),
        "no seed links without a memory"
    );
    assert!(
        pins.iter().any(|p| matches!(
            p,
            Pinned::Linked {
                memory: Some(_),
                ..
            }
        )),
        "no seed links with a memory"
    );
    assert!(
        pins.iter().any(|p| !p.tier_b().is_empty()),
        "no seed raises a Tier-B warning"
    );
    assert!(
        seeds.iter().any(|s| {
            let contributed = contributed(&s.spec);
            !s.pinned.tier_b().is_empty()
                && !contributed.is_empty()
                && s.spec
                    .main_memory
                    .is_some_and(|main| contributed.iter().all(|c| c.min < main.min))
        }),
        "no seed raises a Tier-B warning from main's minimum, above every contributed one"
    );
    assert!(
        seeds.iter().any(|s| {
            let tier_b = s.pinned.tier_b();
            !tier_b.is_empty()
                && s.spec.imports.iter().any(|import| {
                    let provider = &s.spec.providers[import.provider];
                    let field = format!("f{}", import.export);
                    provider.memory.is_some()
                        && !provider.exports[import.export].uses_memory()
                        && !tier_b.contains(&field.as_str())
                })
        }),
        "no seed raises a Tier-B warning beside a pure import from a module with a memory, \
         whose field the warning must not name"
    );
    assert!(
        seeds.iter().any(|s| s.spec.main_memory.is_some()),
        "no seed gives main a memory"
    );
    assert!(
        seeds.iter().any(|s| s.spec.main_memory.is_none()),
        "no seed leaves main memoryless"
    );
    let imported_bodies: BTreeSet<Body> = seeds
        .iter()
        .flat_map(|s| {
            s.spec
                .imports
                .iter()
                .map(|i| s.spec.providers[i.provider].exports[i.export])
        })
        .collect();
    for body in Body::ALL {
        assert!(
            imported_bodies.contains(&body),
            "no seed imports a {body:?} body"
        );
    }
    assert!(
        seeds.iter().any(|s| {
            let imports = &s.spec.imports;
            imports
                .iter()
                .enumerate()
                .any(|(i, a)| imports[i + 1..].contains(a))
        }),
        "no seed imports one export twice"
    );
    assert!(
        seeds.iter().any(|s| {
            let imports = &s.spec.imports;
            imports.iter().any(|a| {
                imports
                    .iter()
                    .any(|b| a.export == b.export && a.provider != b.provider)
            })
        }),
        "no seed imports one field name from two providers"
    );
}

/// The command that rewrites the committed seed corpus from the seed table.
const REGENERATE: &str =
    "cargo test -p inference-wasm-linker --test link_memory_seeds regenerate -- --ignored";

/// The committed seed corpus: one file per seed, named after it.
fn seed_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz")
        .join("seeds")
        .join("link_memory")
}

/// The entries in `dir` other than the seeds' files: a file a renamed or
/// removed seed leaves behind, or anything else put there, a subdirectory
/// included. ClusterFuzzLite packs every file under the directory into the
/// target's seed corpus, so an orphan would ship in it while it stays
/// committed.
fn orphans(dir: &Path, seeds: &[Seed]) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => panic!("cannot list {}: {e}", dir.display()),
    };
    let mut orphans = Vec::new();
    for entry in entries {
        let entry = entry.expect("a directory entry can be read");
        let kind = entry.file_type().expect("an entry has a type");
        let name = entry.file_name();
        if !(kind.is_file() && seeds.iter().any(|seed| name == *seed.name)) {
            orphans.push(entry.path());
        }
    }
    orphans.sort();
    orphans
}

/// Every file is checked before the test fails, so a stale corpus reports
/// every file the regenerate test would rewrite or remove, not only the first.
///
/// A seed's name is its file's, so no two names may match, ignoring case as
/// the macOS and Windows file systems do. Two seeds that share a file cannot
/// both be current, and regenerating leaves one of them stale.
#[test]
fn committed_link_memory_seeds_are_current() {
    let dir = seed_dir();
    assert!(
        dir.is_dir(),
        "the committed seed corpus is missing at {}; regenerate it with `{REGENERATE}`",
        dir.display()
    );
    let seeds = seeds();
    let mut names = BTreeSet::new();
    for seed in &seeds {
        assert!(
            names.insert(seed.name.to_ascii_lowercase()),
            "two seeds are named `{}`, ignoring case, so they share one file; rename one",
            seed.name
        );
    }
    let mut failures = Vec::new();
    for seed in &seeds {
        let committed = match fs::read(dir.join(seed.name)) {
            Ok(committed) => committed,
            Err(e) => {
                failures.push(format!(
                    "`{}`: the seed's file cannot be read ({e})",
                    seed.name
                ));
                continue;
            }
        };
        let decoded = decode(&committed);
        if decoded != seed.spec {
            failures.push(format!(
                "`{}`: the committed file decodes to a spec other than the table's: {decoded:?}",
                seed.name
            ));
        } else if committed != encode(&seed.spec) {
            failures.push(format!(
                "`{}`: the committed file decodes to the table's spec, but is not its encoding",
                seed.name
            ));
        }
    }
    for orphan in orphans(&dir, &seeds) {
        failures.push(format!("{}: not the file of any seed", orphan.display()));
    }
    assert!(
        failures.is_empty(),
        "the seed corpus at {} does not match the seed table:\n\n{}\n\n\
         regenerate it with `{REGENERATE}`",
        dir.display(),
        failures.join("\n")
    );
}

/// A refusal that moves a grower's cap must state the cap of the import it
/// names. Naming one moved grower with another's cap is a wrong diagnostic,
/// although both caps belong to moved growers.
#[test]
fn a_moved_cap_refusal_must_state_the_named_growers_own_cap() {
    // `m0.f0` grows up to 3 pages and `m1.f1` up to 5, and `m2.f0` only
    // reads, under a cap of 2 that moves both growers.
    let spec = with_imports(
        None,
        vec![
            provider(mem(1, Some(3)), &[Grow]),
            provider(mem(1, Some(5)), &[Pure, Grow]),
            provider(mem(1, Some(2)), &[Load]),
        ],
        vec![imp(0, 0), imp(1, 1), imp(2, 0)],
    );
    let Expected::Refused { applicable } = expected(&spec) else {
        panic!("the rule refuses two growers whose caps a reader's moves");
    };
    let Ok(Outcome::Refused {
        kind: Refusal::GrowerCapMoved,
        message,
    }) = check(&spec)
    else {
        panic!("the linker refuses the link as a moved cap");
    };
    let field = message
        .strip_prefix("cannot reconcile linear memory for `")
        .and_then(|rest| rest.split_once('`'))
        .map(|(field, _)| field.to_string())
        .expect("a memory refusal names its field");
    let error = LinkError::IncompatibleMemory {
        field: field.clone(),
        reason: String::new(),
    };
    assert_eq!(
        match_refusal(&error, &message, &applicable),
        Ok(Refusal::GrowerCapMoved),
        "the linker's own message matches"
    );

    let (own, other) = match field.as_str() {
        "f0" => ("up to 3 pages", "up to 5 pages"),
        "f1" => ("up to 5 pages", "up to 3 pages"),
        _ => panic!("the refusal names `{field}`, which no growing import is"),
    };
    let swapped = message.replace(own, other);
    assert_ne!(swapped, message, "the message states `{own}`");
    assert!(
        match_refusal(&error, &swapped, &applicable).is_err(),
        "a refusal naming `{field}` with the other grower's cap must not match: {swapped}"
    );
}

#[test]
fn decoding_is_total() {
    assert_eq!(
        decode(&[]),
        LinkSpec {
            main_memory: None,
            providers: vec![provider(None, &[Pure])],
            imports: vec![],
            shuffle: vec![],
        },
        "the empty input decodes to the minimal spec"
    );
    // Every value at every offset, over a base that puts every byte to use:
    // main and all four providers declare a memory, so the limits and body
    // bytes count, and all three exports and eight imports are live. Over a
    // zero buffer, the zero counts and memory tags would leave only five
    // offsets able to change the spec.
    let zero_pages = mem(0, Some(0));
    let base = LinkSpec {
        main_memory: zero_pages,
        providers: vec![provider(zero_pages, &[Pure; 3]); MAX_PROVIDERS],
        imports: vec![imp(0, 0); MAX_IMPORTS],
        shuffle: vec![0; MAX_IMPORTS],
    };
    let base_bytes = encode(&base);
    for offset in 0..INPUT_LEN {
        let mut live = false;
        for value in 0..=u8::MAX {
            let mut input = base_bytes.clone();
            input[offset] = value;
            let spec = decode(&input);
            if let Err(violation) = generate(&spec) {
                panic!("input byte {offset} = {value}:\n{violation}");
            }
            live |= spec != base;
        }
        assert!(
            live,
            "no value of input byte {offset} changes the decoded spec, so the probe never \
             exercises it"
        );
    }
}

/// A xorshift64 stream, so the sweep is the same on every run and platform.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn a_deterministic_sweep_upholds_every_property() {
    const INPUTS: usize = 4096;
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut refused: BTreeSet<Refusal> = BTreeSet::new();
    let (mut linked_without, mut linked_with, mut warned) = (false, false, false);
    let (mut with_main_memory, mut without_main_memory) = (false, false);

    for case in 0..INPUTS {
        let input: Vec<u8> = (0..INPUT_LEN).map(|_| rng.next() as u8).collect();
        let spec = decode(&input);
        let name = format!("sweep input {case}, bytes {input:?}");
        match try_check(&name, &spec).unwrap_or_else(|failure| panic!("{failure}")) {
            Outcome::Refused { kind, .. } => {
                refused.insert(kind);
            }
            Outcome::Linked { memory, warnings } => {
                linked_without |= memory.is_none();
                linked_with |= memory.is_some();
                warned |= !warnings.is_empty();
            }
        }
        with_main_memory |= spec.main_memory.is_some();
        without_main_memory |= spec.main_memory.is_none();
    }

    // The sweep has to reach every branch the properties distinguish, or a
    // change to the byte format could quietly leave it checking little.
    for kind in Refusal::ALL {
        assert!(refused.contains(&kind), "the sweep never reached {kind:?}");
    }
    assert!(linked_without, "the sweep never linked without a memory");
    assert!(linked_with, "the sweep never linked with a memory");
    assert!(warned, "the sweep never raised a warning");
    assert!(with_main_memory, "the sweep never gave main a memory");
    assert!(without_main_memory, "the sweep never left main memoryless");
}

/// Rewrites the committed seed corpus from the seed table.
mod regenerate {
    use super::{encode, orphans, seed_dir, seeds};
    use std::fs;

    /// Removes every file that is not a seed's, then writes every seed's file,
    /// so that a run leaves `committed_link_memory_seeds_are_current` green.
    /// Removing first matters on a case-insensitive file system: a seed renamed
    /// only in case would otherwise be written into its old file, which the
    /// removal would then delete. Anything it cannot remove as a file, such as
    /// a subdirectory, it reports once the seeds are written.
    #[test]
    #[ignore = "writes the committed seed corpus; run with --ignored to regenerate"]
    fn regenerate_link_memory_seeds() {
        let dir = seed_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            panic!("cannot create {}: {e}", dir.display());
        }
        let seeds = seeds();
        let mut kept = Vec::new();
        for orphan in orphans(&dir, &seeds) {
            match fs::remove_file(&orphan) {
                Ok(()) => eprintln!("removed {}", orphan.display()),
                Err(e) => kept.push(format!("{} ({e})", orphan.display())),
            }
        }
        for seed in &seeds {
            let path = dir.join(seed.name);
            if let Err(e) = fs::write(&path, encode(&seed.spec)) {
                panic!("cannot write {}: {e}", path.display());
            }
        }
        assert!(
            kept.is_empty(),
            "the seeds are written, but these entries could not be removed; remove them by \
             hand:\n{}",
            kept.join("\n")
        );
    }
}

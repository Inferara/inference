//! One theorem, over every target and the whole single-file codegen corpus:
//! nothing on the emission path reads a target, so for any program a
//! non-default target accepts, its module is byte-for-byte the default target's.
//!
//! This is what lets a project prove one build and ship another. A proof is
//! written about the `wasm32` module; what reaches a device is a build for some
//! other target. If those two are not the same bytes, the proof describes a
//! program nobody runs — and no downstream test would say so, because each
//! target's own tests only ever look at that target's output.
//!
//! The sweep is deliberately generic rather than per target. A target-specific
//! copy states the theorem for the target somebody wrote it for and leaves the
//! next one uncovered; iterating `Target::ALL` covers a new target on the day it
//! is added, which is the day the claim starts being made about it.
//!
//! Each side is built at its **own** `default_opt_level()` — `O3` against `Oz`
//! or `Os` — because that is the pairing the two command lines really produce.
//! The level is recorded metadata and no pass acts on it, so the difference is
//! not what gives the comparison teeth; the committed negative control below is.
//! What the recorded level and the recorded target do give is a check that each
//! side was configured as asked, which is how a byte comparison stops quietly
//! being a build against itself: a helper that dropped the target argument
//! would compare the default build with the default build and never say so.

#[cfg(test)]
mod target_identity_tests {
    use crate::corpus::{codegen_for_target_no_analysis, single_file_corpus_sources};
    use inference_wasm_codegen::Target;

    /// The least number of corpus fixtures each target must actually compare,
    /// as against skip.
    ///
    /// Per target rather than one shared constant, because the two skip sets are
    /// nested and very differently sized. The non-determinism gate is target
    /// generic — every target but the default one refuses a non-deterministic
    /// function, and it runs before the Stellar export gate is reached — so
    /// everything `SpaceWasm` skips, Stellar skips too, for the same reason.
    /// Stellar then skips a great deal more on its export gate: a `u64`
    /// parameter, an aggregate return, 33 or more parameters, a `__`-prefixed
    /// export, which are properties of a fixture's *signatures* and have no
    /// analogue here. One constant tuned to Stellar's rate would leave
    /// `SpaceWasm` almost unguarded.
    ///
    /// Measured over the 148 fixtures the corpus holds today: `SpaceWasm`
    /// compares 132 and refuses 16, all 16 on non-determinism; Stellar compares
    /// 68 and refuses 80, those same 16 among them and the rest on its export
    /// gate. Every fixture compiles at the default target, so nothing is skipped
    /// for failing to compile at all. The floors are those counts with roughly a
    /// tenth of headroom, which is what keeps a fixture added or retired from
    /// turning this red for a reason that has nothing to do with the theorem.
    /// They were deliberately left where they were when the measurement moved —
    /// 118 against a compared 132 and 60 against a compared 68 are both inside
    /// that rule — because what pins the skip set exactly is
    /// `ENVELOPE_REFUSED`'s list equality in `tests/tests/spacewasm/differential.rs`,
    /// while a floor here guards only against a sweep that compares almost
    /// nothing.
    ///
    /// Both skip sets grew by the same one when the non-determinism gate became
    /// total within a body: a fixture whose `forall` sits in a *loop body* used
    /// to pass the gate at both targets and be compared. Stellar's count moving
    /// with `SpaceWasm`'s is a measurement and not an inference — the sentence
    /// above says Stellar refuses everything `SpaceWasm` does, but that fixture
    /// declares no signature Stellar's export gate objects to, so it was one of
    /// the 69 Stellar compared and is now one of the 80 it refuses.
    ///
    /// The corpus is compiled with analysis skipped precisely so the constructs
    /// A012/A027/A042 reject still reach code generation, which is why it holds
    /// non-deterministic fixtures at all.
    ///
    /// They are lower bounds on *coverage*, not predictions — a fixture added to
    /// the corpus only ever raises the real number. What they catch is the
    /// failure a sweep like this dies of: a skip predicate that widens until the
    /// test compares almost nothing and passes because it found no mismatch
    /// among the four modules it looked at.
    fn comparison_floor(target: Target) -> usize {
        match target {
            // Never reached: the default target is its own baseline.
            Target::Wasm32 => usize::MAX,
            Target::Stellar => 60,
            Target::SpaceWasm => 118,
        }
    }

    /// For every non-default target, every single-file codegen fixture that both
    /// targets accept compiles to the same bytes.
    ///
    /// A fixture the non-default target refuses is skipped rather than failed —
    /// its refusal is that target's own gate doing its job, and the shapes that
    /// gate refuses are pinned in the target's gate module, while the size of
    /// the skip set is held by the comparison floor below. A fixture the default
    /// target cannot compile is another test's business, but it may not drop out
    /// quietly: the baseline loop counts those and fails if there are any, so a
    /// corpus-wide compilation break cannot masquerade as a wide skip set.
    ///
    /// The baselines are built once, before the target loop, because a baseline
    /// depends on the source and not on the target being compared against it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_target_emits_what_the_default_target_emits() {
        let sources = single_file_corpus_sources();
        assert!(
            sources.len() >= 100,
            "expected at least 100 single-file fixtures, found {}; a collector \
             that silently found nothing would pass this test vacuously",
            sources.len()
        );

        let mut baseline_failed = 0usize;
        let mut baselines: Vec<(&str, &str, Vec<u8>)> = Vec::with_capacity(sources.len());
        for (label, source) in &sources {
            let Ok(output) = codegen_for_target_no_analysis(source, Target::Wasm32) else {
                baseline_failed += 1;
                continue;
            };
            assert_eq!(
                output.target(),
                Target::Wasm32,
                "{label}: the baseline build did not reach the target it was asked for"
            );
            baselines.push((label.as_str(), source.as_str(), output.wasm().to_vec()));
        }
        assert_eq!(
            baseline_failed,
            0,
            "{baseline_failed} of {} fixtures did not compile at the default target; \
             each one leaves the identity claim without being refused by any target \
             gate, and the comparison floors are measured against a set that compiles",
            sources.len()
        );

        for target in Target::ALL {
            if target == Target::Wasm32 {
                continue;
            }
            assert_ne!(
                target.default_opt_level(),
                Target::Wasm32.default_opt_level(),
                "`{}` builds at the default target's own level, so the two sides are \
                 configured alike",
                target.as_str()
            );

            let mut compared = 0usize;
            let mut refused = 0usize;

            for (label, source, baseline) in &baselines {
                let Ok(output) = codegen_for_target_no_analysis(source, target) else {
                    refused += 1;
                    continue;
                };

                assert_eq!(
                    output.target(),
                    target,
                    "{label}: the build did not reach the target it was asked for"
                );
                assert_eq!(
                    output.opt_level(),
                    target.default_opt_level(),
                    "{label}: each side is built at its own default optimization level"
                );
                assert!(
                    !output.wasm().is_empty(),
                    "{label}: an empty module would make this comparison vacuous"
                );
                assert_eq!(
                    output.wasm(),
                    baseline,
                    "{label}: the `{}` build differs from the default build",
                    target.as_str()
                );
                compared += 1;
            }

            assert!(
                compared >= comparison_floor(target),
                "`{}` compared only {compared} of {} fixtures ({refused} refused by that \
                 target, {baseline_failed} not compiled at the default target at all); \
                 the floor is {}, and a sweep that compares almost nothing passes for \
                 the wrong reason",
                target.as_str(),
                sources.len(),
                comparison_floor(target)
            );
        }
    }

    /// The identity above would pass on any two builds that happen to coincide,
    /// so this is what says the comparison has teeth: two programs that differ
    /// produce different modules, at every non-default target as much as at the
    /// default one.
    ///
    /// Committed rather than run by hand. A byte comparison that cannot fail is
    /// exactly what a broken helper produces — one returning an empty `Vec`, or
    /// the same cached module twice — and it looks identical to a passing sweep.
    #[test]
    fn the_identity_comparison_can_tell_two_modules_apart() {
        for target in Target::ALL {
            let one =
                codegen_for_target_no_analysis("pub fn answer() -> i32 { return 42; }", target)
                    .unwrap_or_else(|e| panic!("`{}` must accept this: {e}", target.as_str()));
            let other =
                codegen_for_target_no_analysis("pub fn answer() -> i32 { return 43; }", target)
                    .unwrap_or_else(|e| panic!("`{}` must accept this: {e}", target.as_str()));
            assert_ne!(
                one.wasm(),
                other.wasm(),
                "`{}`: the byte comparison must be able to fail",
                target.as_str()
            );
        }
    }
}

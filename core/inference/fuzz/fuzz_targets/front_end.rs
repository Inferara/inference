//! libFuzzer harness over the compiler front end: parsing, type checking and
//! the analysis rules, on arbitrary UTF-8 source.
//!
//! A source file is untrusted input, and CONTRIBUTING.md requires the compiler
//! to handle any input without panicking. The language server is the widest
//! consumer of that input: it analyses whatever file an editor opens, and it
//! does not stop at the first error. It type-checks the arena the resilient
//! parser built even when that arena has syntax errors, then runs every
//! analysis rule on whatever the type checker recovered (`FileAnalysis::compute`
//! in `ide/ide-db/src/analysis.rs`). This target drives the same path, so it
//! reaches every phase `infc` runs before code generation as well as the states
//! only error recovery produces. Rendering each type-check and analysis
//! diagnostic is part of the path, since both `infc` and the language server
//! print them.
//!
//! The phases run on [`inference::with_compiler_stack`], the stack `infc` and
//! the language server give the compiler. On the fuzzer's own thread a deeply
//! nested input would overflow a stack the compiler never runs on.
//!
//! A panic, an abort, a stack overflow, a timeout or running out of memory is a
//! crash.
//!
//! ## Running
//!
//! From the repository root:
//!
//! ```text
//! cargo +nightly fuzz run --fuzz-dir core/inference/fuzz front_end
//! ```
//!
//! `core/inference/fuzz/README.md` shows how to start from the seed corpus.

#![no_main]

use inference_analysis::AnalysisOptions;
use inference_analysis::rules::all_rules;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    inference::with_compiler_stack(|| {
        let parsed = inference_parser::parse(source);
        let outcome = inference_type_checker::check_with_diagnostics(parsed.arena);
        for diagnostic in &outcome.errors {
            let _ = diagnostic.error.to_string();
        }
        for rule in all_rules() {
            for finding in rule.check(&outcome.typed_context, AnalysisOptions::default()) {
                let _ = finding.diagnostic.to_string();
            }
        }
    });
});

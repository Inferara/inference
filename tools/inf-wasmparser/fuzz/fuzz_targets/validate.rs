//! libFuzzer harness over `inf-wasmparser`, the fork of `wasmparser` that
//! decodes Inference's custom `0xfc` opcodes, on arbitrary bytes.
//!
//! An external module linked with `use … from` or `--wasm-dep` is a third-party
//! binary, and the fork is what reads it: the linker validates and parses each
//! one with it before merging, and the Rocq translation reads modules with it
//! too. Parsing and validating must therefore never panic, hang or run out of
//! memory, whatever the bytes.
//!
//! Each input is validated at three feature levels: the fork's default, the
//! WebAssembly 1.0 level the linker holds externals to, and every feature the
//! fork knows. Validation stops at the first error, so the input is also walked
//! by the plain parser, which decodes every function body's locals and
//! operators and the function, local and global names of a `name` section,
//! the custom section the translation reads names from.
//!
//! ## Running
//!
//! From the repository root:
//!
//! ```text
//! cargo +nightly fuzz run --fuzz-dir tools/inf-wasmparser/fuzz validate
//! ```
//!
//! `tools/inf-wasmparser/fuzz/README.md` shows how to start from the seed
//! corpus.

#![no_main]

use inf_wasmparser::{KnownCustom, Name, NameMap, Parser, Payload, Validator, WasmFeatures};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for features in [
        WasmFeatures::default(),
        WasmFeatures::WASM1,
        WasmFeatures::all(),
    ] {
        let _ = Validator::new_with_features(features).validate_all(data);
    }

    for payload in Parser::new(0).parse_all(data) {
        let Ok(payload) = payload else {
            break;
        };
        match payload {
            Payload::CodeSectionEntry(body) => {
                if let Ok(mut locals) = body.get_locals_reader() {
                    for _ in 0..locals.get_count() {
                        if locals.read().is_err() {
                            break;
                        }
                    }
                }
                if let Ok(mut operators) = body.get_operators_reader() {
                    while !operators.eof() {
                        if operators.read().is_err() {
                            break;
                        }
                    }
                }
            }
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(names) = section.as_known() {
                    for name in names {
                        let Ok(name) = name else {
                            break;
                        };
                        match name {
                            Name::Function(map) | Name::Global(map) => walk_names(map),
                            Name::Local(map) => {
                                for indirect in map {
                                    let Ok(indirect) = indirect else {
                                        break;
                                    };
                                    walk_names(indirect.names);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
});

/// Reads every entry of `map`, stopping at the first that does not decode.
fn walk_names(map: NameMap<'_>) {
    for naming in map {
        if naming.is_err() {
            break;
        }
    }
}

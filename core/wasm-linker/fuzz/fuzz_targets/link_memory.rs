//! Structured libFuzzer harness over the linker's memory reconciliation.
//!
//! The `link` target feeds raw bytes to [`inference_wasm_linker::link`], which
//! catches panics and invalid `Ok` outputs but rarely forms modules whose
//! memory sections and import lists interact, and checks nothing of a
//! successful link beyond its validity. This target decodes each input into a
//! link description: a main module, with or without a memory, importing a
//! chosen list of functions in a chosen order from one to four providers, each
//! with an optional memory and exports that grow memory, use it, or touch
//! nothing. It builds the modules, links them in three import orders, and
//! asserts that the linked memory, or the refusal, and the warnings are the
//! same in every order and match the documented rule. #388, where swapping two
//! `external fn` declarations changed what `memory.grow` returned, is the bug
//! it targets.
//!
//! The generator, the oracle and the checks live in
//! `core/wasm-linker/fuzz/src/link_memory.rs`, which documents the input
//! format and the properties. The stable `cargo test` replay in
//! `core/wasm-linker/tests/link_memory_seeds.rs` includes the same file, so a
//! spec found here reproduces there once written into its seed table.
//!
//! ## Running
//!
//! `cargo-fuzz` and a nightly toolchain are required and are intentionally *not*
//! part of the default workspace (this crate declares its own `[workspace]`).
//! From the repository root:
//!
//! ```text
//! cargo install cargo-fuzz
//! cargo +nightly fuzz run --fuzz-dir core/wasm-linker/fuzz link_memory
//! ```
//!
//! A crash prints the violated property, the spec, and each run's import order
//! and outcome. `cargo fuzz fmt` prints the spec of a saved crash input, in the
//! `Debug` form the seed table's literals follow:
//!
//! ```text
//! cargo +nightly fuzz fmt --fuzz-dir core/wasm-linker/fuzz link_memory <crash-file>
//! ```

#![no_main]

#[path = "../src/link_memory.rs"]
mod link_memory;

use libfuzzer_sys::arbitrary::{self, Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;

// The orphan rule allows this impl here because `LinkSpec` is local to this
// binary crate, which keeps `arbitrary` out of the shared file. The input
// format is `link_memory::decode`'s own, so it does not change with the
// `arbitrary` version. `arbitrary` hands `decode` every remaining byte, and
// since every byte string decodes, it never fails. The default
// `arbitrary_take_rest`, which `fuzz_target!` calls, defers to it, and
// `size_hint` keeps its default.
impl<'a> Arbitrary<'a> for link_memory::LinkSpec {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(link_memory::decode(u.bytes(u.len())?))
    }
}

fuzz_target!(|spec: link_memory::LinkSpec| {
    if let Err(violation) = link_memory::check(&spec) {
        panic!("{violation}");
    }
});

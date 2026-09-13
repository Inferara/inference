//! Turns a compiled Inference module into an uploadable Soroban contract.
//!
//! A Stellar contract does not have an ordinary WebAssembly calling convention.
//! Every method takes and returns 64-bit `Val` words — tagged unions whose low
//! byte names what the rest of the word holds — and every contract carries a
//! `contractenvmetav0` custom section declaring the environment protocol it was
//! built for. An Inference module has neither: it exports `add(i32, i32) -> i32`
//! and carries no metadata at all.
//!
//! This crate is the layer between the two. It takes the linked module and the
//! per-export source-type descriptor code generation recorded, and gives back
//! the same module with one `Val` wrapper appended per exported function, the
//! export section retargeted onto the wrappers, and the metadata section on the
//! end.
//!
//! # Why a rewriter and not an emitter
//!
//! Nothing in code generation knows about Stellar. The bytes `codegen()`
//! produces for the Stellar target are the same bytes it produces for Wasm32,
//! which is what makes "prove the Wasm32 build, deploy the Stellar build" a
//! theorem rather than a hope: the marshalling layer is a post-link pass over an
//! artifact that has already been verified, and the wrappers it appends contain
//! no arithmetic, no memory access and no control flow beyond one guard per
//! argument.
//!
//! # The rewrite
//!
//! 1. The module is validated at the WebAssembly 1.0 feature set — the narrowest
//!    dialect any Soroban host runs — and parsed, recording every section's byte
//!    range.
//! 2. Each exported function is matched against the descriptor **by name**. The
//!    static-merge linker renumbers functions, so an index recorded at code
//!    generation is stale here; the name is what survives. The match is total in
//!    both directions and fails closed on either mismatch.
//! 3. Each export is checked for admissibility, and the first refusal ends the
//!    pass. Nothing is written.
//! 4. One wrapper is synthesized per exported function: a deduplicated
//!    `(i64 × n) -> i64` type entry, a function entry, and a body that unwraps
//!    each argument, calls the function that holds the real code, and wraps the
//!    result.
//! 5. The module is rebuilt. Every untouched section is copied through by byte
//!    range; the type, function and code sections keep their existing entries
//!    verbatim and gain the new ones; the export section keeps its names, kinds
//!    and order and moves only the targets of the wrapped methods.
//! 6. The wrappers go **after** every existing function, so no existing index
//!    moves. That is what keeps the `inference.checked` overflow-guard record —
//!    a list of raw function indices this crate does not decode — correct with
//!    no remap. It is also true: a wrapper holds no guardable arithmetic, so its
//!    absence from that list is the accurate statement.
//! 7. The name section, when there is one, gains one function-name entry per
//!    wrapper, and the metadata section is appended last.
//! 8. The result is validated at WebAssembly 1.0 again and returned, or refused.
//!
//! # What it refuses
//!
//! The admissible set is M1: `u32`, `i32` and `bool` parameters, those three or
//! nothing as a return. Everything else is a [`StellarAbiError`] naming the
//! export and the offending element rather than a guess — a 64-bit integer, a
//! narrow integer, a struct, an array, an enum, a compound return, an
//! unreachable method name, a surviving import, a start function.
//!
//! Two refusals are of things nothing downstream would reject. A module
//! carrying two `name` custom sections is legal WebAssembly, but this pass
//! rebuilds exactly one function-name map, so carrying both would duplicate one
//! body and drop the other. And a declared protocol below
//! [`STELLAR_ENV_PROTOCOL`] produces a well-formed metadata section that no
//! host has ever accepted, which would surface at upload rather than here.
//!
//! # WebAssembly 1.0 is this pass's bar, not the host's
//!
//! The host of record runs more than 1.0: `soroban-env-host` 28.0.2 builds its
//! wasmi configuration with bulk memory, mutable globals and sign extension
//! enabled, and multi-value, reference types, tail calls and floats disabled.
//! Holding a contract to 1.0 is therefore deliberate conservatism of the same
//! kind as `Target::permits_bulk_memory`'s refusal — the widest dialect every
//! host has accepted, rather than the widest one this host accepts — and not a
//! restriction the host imposes. Relaxing it is a decision about which hosts an
//! artifact is allowed to require, not a bug fix.
//!
//! # Asking the question before the merge
//!
//! Step 1 asks it of the module it is handed, which by then is one module: a
//! program and every foreign artifact linked into it, merged. A foreign
//! artifact built by an ordinary toolchain is very often not WebAssembly 1.0 —
//! `rustc` has emitted sign-extension instructions for `wasm32-unknown-unknown`
//! by default for years — and refused at that point the answer is a byte offset
//! into bytes that came from no single file.
//!
//! `inference_target_conformance::check_wasm1` is the same question, asked of
//! one artifact at a time so that a caller holding them separately can name the
//! file in the refusal. It is the caller's job because only the caller knows
//! where the bytes came from, and it lives in that crate rather than this one
//! because two targets are now held to the WebAssembly 1.0 envelope and two
//! crates spelling it independently is how the two answers drift apart. This
//! pass calls it on both the module it is handed and the module it produces.
//!
//! # Example
//!
//! ```no_run
//! use inference_stellar_abi::{STELLAR_ENV_PROTOCOL, rewrite};
//! use inference_wasm_codegen::{AbiReturn, AbiType, ExportSignature};
//!
//! # fn main() -> Result<(), inference_stellar_abi::StellarAbiError> {
//! # let linked: Vec<u8> = Vec::new();
//! let exports = vec![ExportSignature {
//!     name: "add".to_string(),
//!     params: vec![AbiType::U32, AbiType::U32],
//!     ret: AbiReturn::Scalar(AbiType::U32),
//! }];
//! let contract = rewrite(&linked, &exports, STELLAR_ENV_PROTOCOL)?;
//! # let _ = contract;
//! # Ok(())
//! # }
//! ```

#![warn(clippy::pedantic)]

mod error;
mod meta;
mod rewrite;
mod val;

pub use error::StellarAbiError;
pub use meta::{STELLAR_ENV_PRE_RELEASE, STELLAR_ENV_PROTOCOL};
pub use rewrite::{MAX_EXPORT_NAME_BYTES, MAX_VAL_PARAMETERS, rewrite};

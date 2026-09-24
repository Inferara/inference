//! Soroban host tier: modules — hand-assembled here, compiled by this
//! repository's own toolchain in `contracts` — uploaded and invoked in process
//! against the same host, wasmi configuration and validation a Stellar
//! validator runs.
//!
//! Five things live here. `envelope` pins what the host accepts and refuses of
//! a module's *shape* — the mandatory metadata section, the declared protocol,
//! floats, multi-value, an exported mutable global, and the two tooling
//! sections it never reads. `wrappers` measures the `Val` marshalling
//! instruction sequences themselves: each shape is assembled, executed and
//! round-tripped, and the sequence that actually works is recorded in
//! `MEASURED_ABI.md` beside this file. `contracts` leaves hand assembly
//! behind: it compiles `.inf` fixtures through the same three steps
//! `infc --target stellar` runs, invokes them on the host, and requires each to
//! answer exactly what the default build answers under `wasmtime`. `spec`
//! reads the same compiled contracts the way the tooling does: every fixture's
//! `contractspecv0` section decodes the way both of the `stellar` CLI's readers
//! decode it, and is byte for byte what `stellar-xdr`'s own encoder writes for
//! the method descriptions code generation recorded. `cli_flags` holds the
//! second flag the `stellar` CLI derives for each parameter, which both gates
//! refuse a method's names to collide with, equal to the `heck` conversion the
//! CLI runs, and the gates' verdicts equal to what the CLI measurably did.
//!
//! The host is a dev-dependency and this is an integration binary, so neither
//! `cargo build` nor `cargo clippy` without `--all-targets` compiles any of it.

mod cli_flags;
mod contracts;
mod envelope;
mod spec;
mod support;
mod wrappers;

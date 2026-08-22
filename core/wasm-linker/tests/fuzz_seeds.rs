//! Seed corpus for the `cargo-fuzz` `link` target, plus the deterministic guard
//! that keeps it honest under stable `cargo test`.
//!
//! The libFuzzer harness in `core/wasm-linker/fuzz/fuzz_targets/link.rs` cannot
//! run on the default toolchain (`cargo-fuzz` + nightly are detached from the
//! workspace). Its seed corpus, however, *is* committed — at
//! `core/wasm-linker/fuzz/seeds/link/` — so a developer running
//! `cargo +nightly fuzz run link core/wasm-linker/fuzz/seeds/link` starts from
//! the round-2 audit reproductions rather than from zero coverage.
//!
//! Two tests keep the corpus trustworthy on every `cargo test`:
//!
//! - [`committed_fuzz_seeds_reach_link_cleanly`] replays each committed seed
//!   through the fuzz target's exact wire-format `split` and module-name
//!   rotation, asserting it neither panics nor produces a silently-invalid `Ok`
//!   — the same invariant the fuzzer enforces. It also checks each round-2 seed
//!   reaches its intended rejection (so a seed that silently stops exercising the
//!   guard it was built for is caught).
//! - [`regenerate_fuzz_seeds`] (`#[ignore]`d) rebuilds the corpus from source, so
//!   the binary blobs are reproducible rather than opaque. Run it with:
//!   `cargo test -p inference-wasm-linker --test fuzz_seeds regenerate -- --ignored`.

use inference_wasm_linker::link as raw_link;
use std::fs;
use std::path::{Path, PathBuf};

/// The committed seed-corpus directory for the `link` fuzz target.
fn seed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz")
        .join("seeds")
        .join("link")
}

/// The logical module names the `link` fuzz target rotates externals through,
/// kept byte-identical to `fuzz_targets/link.rs` so the replay binds imports the
/// same way the fuzzer does. Seed mains therefore import from the empty module
/// `""` — the name the *first* external is always assigned — so the import binds
/// and the seed reaches the real closure / provenance / merge logic rather than
/// stalling on an unsatisfied import.
const MODULE_NAMES: [&str; 4] = ["", "mathlib", "crypto::sha256", "a"];

/// The fuzz target's wire-format split, mirrored here so the replay is faithful:
/// `[count:u8][ (len:u16le, bytes) * (count % 5) ][ main bytes ]`.
fn split(data: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let Some((&count_byte, rest)) = data.split_first() else {
        return (Vec::new(), Vec::new());
    };
    let count = (count_byte % 5) as usize;
    let mut externals = Vec::with_capacity(count);
    let mut cursor = rest;
    for _ in 0..count {
        if cursor.len() < 2 {
            break;
        }
        let len = u16::from_le_bytes([cursor[0], cursor[1]]) as usize;
        cursor = &cursor[2..];
        let take = len.min(cursor.len());
        externals.push(cursor[..take].to_vec());
        cursor = &cursor[take..];
    }
    (cursor.to_vec(), externals)
}

/// Encodes one `(main, externals)` case in the fuzz target's wire format.
fn encode(main: &[u8], externals: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(externals.len() as u8);
    for ext in externals {
        out.extend_from_slice(&(ext.len() as u16).to_le_bytes());
        out.extend_from_slice(ext);
    }
    out.extend_from_slice(main);
    out
}

/// Assembles a `.wasm` from WAT, panicking with the source on error.
fn wasm(src: &str) -> Vec<u8> {
    wat::parse_str(src).unwrap_or_else(|e| panic!("invalid seed WAT: {e}\n{src}"))
}

/// Links a decoded seed exactly as the fuzz target would: external `i` is tagged
/// with `MODULE_NAMES[i % 4]`.
fn link_like_fuzzer(main: &[u8], externals: &[Vec<u8>]) -> Result<Vec<u8>, inference_wasm_linker::LinkError> {
    let pairs: Vec<(&str, &[u8])> = externals
        .iter()
        .enumerate()
        .map(|(i, b)| (MODULE_NAMES[i % MODULE_NAMES.len()], b.as_slice()))
        .collect();
    raw_link(main, &pairs, None)
}

/// A named seed: its file name, the bytes, and the substring its rejection must
/// contain (or `None` for the positive control that must merge into a valid
/// module). The substring pins each round-2 seed to the *specific* guard it was
/// built to exercise, so a refactor that lets a laundering seed slip to a
/// different (or absent) rejection is caught here, not only in the fuzzer.
struct Seed {
    name: &'static str,
    bytes: Vec<u8>,
    /// `Some(needle)` ⇒ must be rejected with a message containing `needle`;
    /// `None` ⇒ must merge into a valid module.
    rejection_needle: Option<&'static str>,
}

/// The full seed corpus, built from source. Each round-2 reproduction mirrors
/// the dedicated regression test's fixture, and imports from the empty module so
/// the fuzzer's first-external binding satisfies it.
fn seeds() -> Vec<Seed> {
    let mem_main = |ity: &str, field: &str, body: &str| {
        wasm(&format!(
            "(module {ity} (import \"\" \"{field}\" (func (;0;) (type 0))) \
             (memory (;0;) 1 1) {body} \
             (export \"memory\" (memory 0)) (export \"run\" (func 1)))"
        ))
    };
    let mem_lib = |ty: &str, field: &str, body: &str| {
        wasm(&format!(
            "(module {ty} (memory (;0;) 1) \
             (func (;0;) (type 0) {body}) (export \"{field}\" (func 0)))"
        ))
    };

    let main_sum = wasm(
        "(module (type (;0;) (func (param i32 i32) (result i32))) \
         (import \"\" \"sum\" (func (;0;) (type 0))) \
         (func (;1;) (type 0) (param i32 i32) (result i32) local.get 0 local.get 1 call 0) \
         (export \"compute\" (func 1)))",
    );
    let pure_lib = wasm(
        "(module (type (;0;) (func (param i32 i32) (result i32))) \
         (func (;0;) (type 0) (param i32 i32) (result i32) local.get 0 local.get 1 i32.add) \
         (export \"sum\" (func 0)))",
    );

    let mut deep_body = String::new();
    for _ in 0..5_000 {
        deep_body.push_str("block ");
    }
    for _ in 0..5_000 {
        deep_body.push_str("end ");
    }
    let deep_lib = wasm(&format!(
        "(module (type (;0;) (func (param i32 i32) (result i32))) \
         (func (;0;) (type 0) (param i32 i32) (result i32) {deep_body} \
           local.get 0 local.get 1 i32.add) \
         (export \"sum\" (func 0)))"
    ));

    let m2_main = wasm(
        "(module (type (;0;) (func (param i32 i32) (result i32))) \
         (import \"\" \"sum\" (func (;0;) (type 0))) \
         (memory (;0;) 1 1) (data (;0;) (i32.const 0) \"\\2a\\00\\00\\00\") \
         (func (;1;) (type 0) (param i32 i32) (result i32) local.get 0 local.get 1 call 0) \
         (export \"compute\" (func 1)))",
    );

    let mk = |name, main: &[u8], ext: Vec<u8>, needle| Seed {
        name,
        bytes: encode(main, &[ext]),
        rejection_needle: needle,
    };

    vec![
        // C-1: a constant address laundered through a control-flow join into an
        // address-feeding local.
        mk(
            "c1_control_flow_join",
            &mem_main(
                "(type (;0;) (func (param i32 i32) (result i32)))",
                "peek",
                "(func (;1;) (type 0) (param i32 i32) (result i32) local.get 0 local.get 1 call 0)",
            ),
            mem_lib(
                "(type (;0;) (func (param i32 i32) (result i32)))",
                "peek",
                "(param i32 i32) (result i32) (local i32) \
                   i32.const 1024 local.set 2 \
                   (block local.get 1 (if (then local.get 0 local.set 2))) \
                   local.get 2 i32.load",
            ),
            Some("relocatable build"),
        ),
        // C-2: param-nulling arithmetic — `(addr - addr) + base` is a fixed host
        // address.
        mk(
            "c2_param_nulling_arith",
            &mem_main(
                "(type (;0;) (func (param i32 i32)))",
                "poke",
                "(func (;1;) (type 0) (param i32 i32) local.get 0 local.get 1 call 0)",
            ),
            mem_lib(
                "(type (;0;) (func (param i32 i32)))",
                "poke",
                "(param i32 i32) \
                   local.get 0 local.get 0 i32.sub i32.const 65536 i32.add \
                   local.get 1 i32.store",
            ),
            Some("relocatable build"),
        ),
        // C-2b: add-side algebraic cancellation — `(C - p) + p == C` re-derives
        // the fixed host address the `sub` rule demoted. The `add` rule must not
        // re-promote `Param + NotParam` to Param.
        mk(
            "c2b_add_side_cancellation",
            &mem_main(
                "(type (;0;) (func (param i32 i32)))",
                "poke",
                "(func (;1;) (type 0) (param i32 i32) local.get 0 local.get 1 call 0)",
            ),
            mem_lib(
                "(type (;0;) (func (param i32 i32)))",
                "poke",
                "(param i32 i32) \
                   i32.const 65536 local.get 0 i32.sub local.get 0 i32.add \
                   local.get 1 i32.store",
            ),
            Some("relocatable build"),
        ),
        // C-3: a constant address laundered across a `call` boundary.
        mk(
            "c3_call_laundered",
            &mem_main(
                "(type (;0;) (func (param i32 i32) (result i32)))",
                "peek",
                "(func (;1;) (type 0) (param i32 i32) (result i32) local.get 0 local.get 1 call 0)",
            ),
            wasm(
                "(module (type (;0;) (func (param i32 i32) (result i32))) \
                 (type (;1;) (func (param i32) (result i32))) \
                 (memory (;0;) 1) \
                 (func (;0;) (type 0) (param i32 i32) (result i32) i32.const 1024 call 1) \
                 (func (;1;) (type 1) (param i32) (result i32) local.get 0 i32.load) \
                 (export \"peek\" (func 0)))",
            ),
            Some("relocatable build"),
        ),
        // C-4: a memory64 external folded onto a memoryless main.
        mk(
            "c4_memory64",
            &wasm(
                "(module (type (;0;) (func (param i64) (result i64))) \
                 (import \"\" \"load_at\" (func (;0;) (type 0))) \
                 (func (;1;) (type 0) (param i64) (result i64) local.get 0 call 0) \
                 (export \"run\" (func 1)))",
            ),
            wasm(
                "(module (type (;0;) (func (param i64) (result i64))) \
                 (memory (;0;) i64 1) \
                 (func (;0;) (type 0) (param i64) (result i64) local.get 0 i64.load) \
                 (export \"load_at\" (func 0)))",
            ),
            Some("memory64"),
        ),
        // H-3: a deeply-nested external body the merge must reject before it can
        // abort the wasm-to-v translator.
        mk("h3_deep_nesting", &main_sum, deep_lib, Some("nests structured control flow")),
        // M-1: an over-declared locals count, rejected by the pre-validation gate
        // before any per-local allocation.
        mk("m1_over_declared_locals", &main_sum, over_declared_locals_external(u32::MAX), Some("parse")),
        // M-2: a main module carrying an active data segment.
        mk("m2_main_data_segment", &m2_main, pure_lib.clone(), Some("data segment")),
        // Positive control: a genuinely-pure external that must merge into a
        // valid module, so the corpus is never vacuously all-rejection.
        mk("pure_control_merges", &main_sum, pure_lib, None),
    ]
}

#[test]
fn committed_fuzz_seeds_reach_link_cleanly() {
    let dir = seed_dir();
    assert!(
        dir.is_dir(),
        "the committed fuzz seed corpus is missing at {}; regenerate it with \
         `cargo test -p inference-wasm-linker --test fuzz_seeds regenerate -- --ignored`",
        dir.display()
    );

    for seed in seeds() {
        let path = dir.join(seed.name);
        let committed = fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "missing seed `{}` ({e}); regenerate with \
                 `cargo test -p inference-wasm-linker --test fuzz_seeds regenerate -- --ignored`",
                seed.name
            )
        });

        // The committed bytes must match what the generator produces, so the
        // corpus stays reproducible (a reviewer can rebuild it from source).
        assert_eq!(
            committed, seed.bytes,
            "committed seed `{}` is stale; regenerate it with \
             `cargo test -p inference-wasm-linker --test fuzz_seeds regenerate -- --ignored`",
            seed.name
        );

        // Replay through the fuzzer's exact decode + module rotation, wrapped in
        // catch_unwind so a reintroduced panic names the offending seed.
        let (main, externals) = split(&committed);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            link_like_fuzzer(&main, &externals)
        }));
        let result = outcome.unwrap_or_else(|_| {
            panic!("seed `{}`: link panicked — it must return an Err", seed.name)
        });

        match (result, seed.rejection_needle) {
            (Ok(merged), None) => {
                inf_wasmparser::validate(&merged).unwrap_or_else(|e| {
                    panic!("seed `{}`: merged module fails validation: {e}", seed.name)
                });
            }
            (Ok(_), Some(needle)) => panic!(
                "seed `{}`: a soundness reproduction merged instead of being rejected \
                 (expected a `{needle}` rejection) — a silent miscompile",
                seed.name
            ),
            (Err(e), Some(needle)) => {
                let msg = e.to_string();
                assert!(
                    msg.contains(needle),
                    "seed `{}`: rejected, but not for its intended reason; \
                     expected a message containing `{needle}`, got `{msg}`",
                    seed.name
                );
            }
            (Err(e), None) => panic!(
                "seed `{}`: the positive control must merge, got a rejection: {e}",
                seed.name
            ),
        }
    }
}

#[test]
#[ignore = "writes the committed fuzz seed corpus; run with --ignored to regenerate"]
fn regenerate_fuzz_seeds() {
    let dir = seed_dir();
    fs::create_dir_all(&dir).expect("create seed dir");
    for seed in seeds() {
        write_seed(&dir, seed.name, &seed.bytes);
    }
}

/// Writes one seed file, creating it byte-for-byte from the generator output.
fn write_seed(dir: &Path, name: &str, bytes: &[u8]) {
    fs::write(dir.join(name), bytes)
        .unwrap_or_else(|e| panic!("failed to write seed `{name}`: {e}"));
}

// -- M-1 fixture (hand-assembled invalid module) -----------------------------
//
// A real assembler cannot emit an over-declared locals count — `wat` computes
// the locals header from the declared types — so the M-1 reproduction is written
// byte-by-byte, mirroring `over_declared_locals_external` in `link.rs`.

fn push_uleb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn framed_section(id: u8, section_bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    push_uleb(&mut out, section_bytes.len() as u32);
    out.extend_from_slice(section_bytes);
    out
}

/// A memory-using external exporting `sum:(i32,i32)->i32` whose single function
/// over-declares its locals count as `locals_count`. With `u32::MAX` this is the
/// M-1 reproduction: the value a 6-byte locals group can set, which the universal
/// pre-validation gate must reject before provenance sizes a per-local `vec!`.
fn over_declared_locals_external(locals_count: u32) -> Vec<u8> {
    let type_section = framed_section(0x01, &[0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f]);
    let function_section = framed_section(0x03, &[0x01, 0x00]);
    let memory_section = framed_section(0x05, &[0x01, 0x00, 0x01]);

    let mut export_payload = vec![0x01];
    push_uleb(&mut export_payload, 3);
    export_payload.extend_from_slice(b"sum");
    export_payload.push(0x00);
    export_payload.push(0x00);
    let export_section = framed_section(0x07, &export_payload);

    let mut body = Vec::new();
    body.push(0x01);
    push_uleb(&mut body, locals_count);
    body.push(0x7f);
    body.extend_from_slice(&[0x41, 0x00]);
    body.extend_from_slice(&[0x28, 0x02, 0x00]);
    body.push(0x1a);
    body.extend_from_slice(&[0x41, 0x00]);
    body.push(0x0b);

    let mut code_payload = vec![0x01];
    push_uleb(&mut code_payload, body.len() as u32);
    code_payload.extend_from_slice(&body);
    let code_section = framed_section(0x0a, &code_payload);

    let mut module = Vec::new();
    module.extend_from_slice(b"\0asm");
    module.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
    module.extend_from_slice(&type_section);
    module.extend_from_slice(&function_section);
    module.extend_from_slice(&memory_section);
    module.extend_from_slice(&export_section);
    module.extend_from_slice(&code_section);
    module
}

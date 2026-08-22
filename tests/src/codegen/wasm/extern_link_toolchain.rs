//! Acceptance gate for the linker envelope: a stock Rust toolchain artifact
//! links, runs, and is reasoned about (issue #363).
//!
//! Every other linker test builds its external out of WAT written for the test,
//! or out of Inference source compiled by this very compiler. Both are inputs
//! the envelope was designed around, so neither can answer the question the
//! issue is actually about: does something a *foreign* toolchain emits, with no
//! knowledge of Inference and no cooperation from its author, fall inside the
//! envelope? These tests answer it against the committed
//! `tests/test_data/wasmlib/rustlib.wasm` — the unmodified output of
//! `cargo build --release --target wasm32-unknown-unknown` over the crate
//! beside it. Its `.wasm` is committed rather than built here so CI needs no
//! `wasm32` target; `tests/test_data/wasmlib/README.md` records the toolchain
//! and how to regenerate and diff it.
//!
//! The artifact's two exports were chosen to land one in each admitted tier:
//! `clamp_add` touches no memory (Tier A), `sum_n` walks a caller-supplied
//! pointer (Tier B). Both are exercised by outcome rather than by the absence of
//! an error — the merged bodies are executed under `wasmtime` and asserted on
//! their computed results, because a merge that mis-wired an index or dropped a
//! body would still link and still validate.
//!
//! The Rocq half of the acceptance criterion lives in `rocq_typecheck.rs`, where
//! the same artifact is merged into a proof-mode module and the result is
//! compiled by `coqc`.

#[cfg(test)]
mod extern_link_toolchain_tests {
    use crate::utils::get_test_data_path;
    use inf_wasmparser::{Parser, Payload};
    use inference::wasm_link::{SearchPath, resolve_external_modules};
    use inference::{LinkWarning, analyze, link_with_warnings, parse, type_check};
    use inference_wasm_codegen::{CodegenOptions, MemoryLayout, MemoryLayoutSource, codegen};
    use std::path::PathBuf;
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// The directory holding the committed artifact, usable directly as an
    /// `infc -L` search directory: a `-L` search resolves the logical module
    /// `rustlib` — the name every `use … from` clause below binds — to
    /// `rustlib.wasm` under it.
    fn wasmlib_dir() -> PathBuf {
        get_test_data_path().join("wasmlib")
    }

    /// The committed artifact's bytes.
    fn artifact() -> Vec<u8> {
        let path = wasmlib_dir().join("rustlib.wasm");
        std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "read the committed toolchain artifact at {}: {e}",
                path.display()
            )
        })
    }

    /// Compiles `source` and merges the committed artifact into it, exactly as
    /// `infc -L tests/test_data/wasmlib` does: the external is resolved off a
    /// search path (which validates it against the supported WASM subset before
    /// the merge sees it), then statically linked.
    ///
    /// `pages` is the main module's declared linear memory, the `--memory-pages`
    /// flag's value. The shadow stack keeps its default size, so plain
    /// [`analyze`] still measures A036 against the budget the artifact has.
    fn link_against_artifact(
        source: &str,
        module_name: &str,
        pages: u32,
    ) -> (Vec<u8>, Vec<LinkWarning>) {
        let arena = parse(source).expect("main source parses");
        let typed = type_check(arena).expect("main source type-checks");
        analyze(&typed).expect("main source passes analysis");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(wasmlib_dir());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("the committed artifact resolves and validates");
        let external_bytes = externals.module_bytes();

        let layout = MemoryLayout::resolve(Some(pages), None, MemoryLayoutSource::Flag)
            .expect("the requested memory layout is legal");
        let output = codegen(
            &typed,
            module_name,
            CodegenOptions {
                layout,
                ..Default::default()
            },
        )
        .expect("main codegen succeeds");

        // The checked write-set mode, the same one `infc` uses. The artifact's
        // exports are read-only, so every declaration below leaves its parameters
        // unannotated and the merge must confirm the foreign bodies record no
        // store at all — which is the acceptance criterion a *foreign* toolchain
        // artifact has to meet before a caller may skip a defensive copy.
        let linked = link_with_warnings(output.wasm(), &external_bytes, Some(&externals.contracts))
            .expect("the merge succeeds");
        inf_wasmparser::validate(&linked.wasm).expect("the merged module is valid wasm");
        (linked.wasm, linked.warnings)
    }

    /// Instantiates a merged module, which imports nothing by construction.
    fn instantiate(wasm: &[u8]) -> (Store<()>, Instance) {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));
        (store, instance)
    }

    /// The `(module, field)` of every import in `wasm`, of any kind.
    ///
    /// Deliberately not restricted to function imports the way the sibling
    /// linker tests are: an `--import-memory` build is ordinary enough, and an
    /// external that imports its memory rather than declaring one changes the
    /// whole reconciliation story while leaving a function-only count at zero.
    fn imports(wasm: &[u8]) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ImportSection(reader) = payload.expect("valid payload") {
                for import in reader {
                    let import = import.expect("valid import");
                    found.push((import.module.to_string(), import.name.to_string()));
                }
            }
        }
        found
    }

    /// The `__stack_pointer` the merged module exports, which the merge carries
    /// through from the main module at its original index.
    ///
    /// Reading it across a call is the half a returned value cannot state: a
    /// merged body that walks the shadow stack down — a prologue whose restore
    /// went missing — is invisible to a program short enough to never exhaust
    /// it, because each call re-derives its frame from wherever the pointer now
    /// happens to be.
    fn stack_pointer(store: &mut Store<()>, instance: &Instance) -> i32 {
        instance
            .get_global(&mut *store, "__stack_pointer")
            .expect("the merge must preserve the main module's `__stack_pointer` export")
            .get(&mut *store)
            .i32()
            .expect("__stack_pointer is an i32 global")
    }

    /// The section kinds `wasm` carries, as the names the linker's rejection
    /// policy uses, plus the payload of each custom section by name.
    struct Sections {
        data_segments: u32,
        element_segments: u32,
        tables: u32,
        globals: u32,
        memory_pages: Vec<u64>,
        exports: Vec<String>,
        customs: Vec<(String, Vec<u8>)>,
    }

    /// Counts each of those sections in `wasm`.
    ///
    /// A count rather than a presence flag because an empty section and an
    /// absent one are different artifacts, and the merge treats them alike only
    /// for the two that are declaration-gated.
    fn sections(wasm: &[u8]) -> Sections {
        let mut found = Sections {
            data_segments: 0,
            element_segments: 0,
            tables: 0,
            globals: 0,
            memory_pages: Vec::new(),
            exports: Vec::new(),
            customs: Vec::new(),
        };
        for payload in Parser::new(0).parse_all(wasm) {
            match payload.expect("valid payload") {
                Payload::DataSection(reader) => found.data_segments = reader.count(),
                Payload::DataCountSection { count, .. } => found.data_segments = count,
                Payload::ElementSection(reader) => found.element_segments = reader.count(),
                Payload::TableSection(reader) => found.tables = reader.count(),
                Payload::GlobalSection(reader) => found.globals = reader.count(),
                Payload::MemorySection(reader) => {
                    for memory in reader {
                        found
                            .memory_pages
                            .push(memory.expect("valid memory").initial);
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        found
                            .exports
                            .push(export.expect("valid export").name.to_string());
                    }
                }
                Payload::CustomSection(reader) => found
                    .customs
                    .push((reader.name().to_string(), reader.data().to_vec())),
                _ => {}
            }
        }
        found
    }

    /// The committed artifact really is foreign toolchain output, and its shape
    /// is what puts it inside the envelope.
    ///
    /// This is the fixture's own premise, and nothing else checks it. Replacing
    /// the artifact with hand-written WAT that happened to link would leave every
    /// test below green while the thing the issue is about — that *stock* Rust
    /// output fits — quietly stopped being tested. The section inventory is
    /// asserted for the same reason in the other direction: a regenerated
    /// artifact that gained a data or element segment, or a table, would fail
    /// here naming the construct, instead of surfacing as an opaque
    /// `RequiresRelocatableBuild` from a linker test.
    #[test]
    fn the_committed_artifact_is_stock_rust_output_inside_the_envelope() {
        let wasm = artifact();
        let found = sections(&wasm);

        let producers = found
            .customs
            .iter()
            .find(|(name, _)| name == "producers")
            .map(|(_, data)| String::from_utf8_lossy(data).into_owned())
            .expect(
                "the artifact must carry a `producers` section: it is the evidence that a real \
                 toolchain emitted it, which is the whole premise of this fixture",
            );
        assert!(
            producers.contains("rustc"),
            "the artifact must name `rustc` as its producer; got: {producers:?}"
        );

        assert_eq!(
            found.data_segments, 0,
            "a data segment is a Tier-C signal: the artifact would stop linking at all"
        );
        assert_eq!(
            found.element_segments, 0,
            "an element segment is a Tier-C signal: the artifact would stop linking at all"
        );
        assert_eq!(
            found.tables, 0,
            "a table means the crate grew indirect dispatch, which the merge rejects on use"
        );
        assert_eq!(
            found.globals, 1,
            "lld gives every `wasm32-unknown-unknown` artifact a `__stack_pointer` global, and \
             the merge admitting one is the reason stock output links at all — globals are \
             classified on use, not on declaration. An artifact that declared none would keep \
             every test below green while silently retiring the coverage they are credited with"
        );
        assert_eq!(
            found.memory_pages,
            vec![16],
            "the 16-page memory is what a memoryless main adopts, and the page count the \
             Tier-B warning names"
        );
        assert!(
            imports(&wasm).is_empty(),
            "an artifact that imports its environment has nothing to merge those imports onto"
        );
        for export in ["clamp_add", "mulhi", "sum_n"] {
            assert!(
                found.exports.iter().any(|name| name == export),
                "the artifact must export `{export}`; it exports {:?}",
                found.exports
            );
        }
    }

    /// Tier A: a leaf the Rust toolchain emitted computes the right number after
    /// the merge.
    ///
    /// `clamp_add` is the shape a merge can get wrong invisibly — it shares its
    /// `(i32,i32)->i32` type with `sum_n` in the artifact, so a body swapped for
    /// its neighbour still validates. The points pin both saturation directions,
    /// the unsaturated path, and the two identities.
    ///
    /// The identities are the discriminating ones, and they are not there for
    /// symmetry. The emitted overflow test is `(b < 0) XOR (sum < a)`, and every
    /// point far from the boundary survives relaxing that `<` to `<=` — only
    /// `(0, 0)` separates them, returning `i32::MIN` under the relaxed form.
    /// They also give `spec_linked_toolchain.inf`'s `clamp_add(a, 0) == a` its
    /// only executed witnesses: the `coqc` gate admits that obligation without
    /// proving it, so nothing else in the suite tests it at even one point.
    #[test]
    fn a_tier_a_leaf_from_the_rust_toolchain_executes_after_the_merge() {
        let (linked, warnings) = link_against_artifact(
            "external fn clamp_add(a: i32, b: i32) -> i32;\n\
             use { clamp_add } from rustlib;\n\
             pub fn saturating_add(a: i32, b: i32) -> i32 { return clamp_add(a, b); }",
            "toolchain_tier_a",
            1,
        );

        assert!(
            imports(&linked).is_empty(),
            "the merge must fold the external in, not reference it: {:?}",
            imports(&linked)
        );
        assert!(
            warnings.is_empty(),
            "a leaf that never touches memory has no reach to warn about; got {warnings:?}"
        );
        assert!(
            sections(&linked).memory_pages.is_empty(),
            "a Tier-A closure contributes no memory, so a memoryless main stays memoryless"
        );

        let (mut store, instance) = instantiate(&linked);
        let saturating_add: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "saturating_add")
            .expect("the merged module exports `saturating_add`");

        assert_eq!(
            saturating_add
                .call(&mut store, (2_000_000_000, 2_000_000_000))
                .expect("executes"),
            2_147_483_647,
            "a sum past i32::MAX must saturate rather than wrap; a body computing in 32 bits \
             throughout would return -294967296"
        );
        assert_eq!(
            saturating_add
                .call(&mut store, (-2_000_000_000, -2_000_000_000))
                .expect("executes"),
            -2_147_483_648,
            "the other saturation direction, which a one-sided clamp would miss"
        );
        assert_eq!(
            saturating_add.call(&mut store, (2, 3)).expect("executes"),
            5,
            "an in-range sum must pass through, ruling out a body that always saturates"
        );
        assert_eq!(
            saturating_add.call(&mut store, (0, 0)).expect("executes"),
            0,
            "the boundary the far-from-overflow points cannot reach: an off-by-one in the \
             overflow test reads 0 + 0 as overflowing and returns i32::MIN"
        );
        assert_eq!(
            saturating_add
                .call(&mut store, (2_147_483_647, 0))
                .expect("executes"),
            2_147_483_647,
            "adding zero at the largest representable value must still be the identity"
        );
    }

    /// Tier A again, for the operators the numeric envelope was widened to admit:
    /// `i64.extend_i32_s` and `i32.wrap_i64`, carried by a real artifact.
    ///
    /// Those two had no toolchain producer until this function existed. Writing a
    /// computation "in 64 bits" is not enough to get them — `clamp_add` is
    /// written that way and reaches the artifact as branchless `i32`, because an
    /// optimizer narrows any intermediate whose result it can obtain more
    /// cheaply. They survive only where 64 bits are load-bearing, and the high
    /// half of a 32x32 product is the smallest natural case: no `i32`-only
    /// lowering computes it.
    ///
    /// The negative points are the discriminating ones. They separate the
    /// *signedness* of the widening, which is what the emitted
    /// `BI_cvtop … (Some SX_S)` records and the one part of the conversion a
    /// wrong lowering would get wrong silently: widening unsigned instead returns
    /// -2 for `(-1, -1)` and 0 for `(-1, 1)`, while agreeing on every
    /// non-negative point below.
    #[test]
    fn a_tier_a_leaf_carrying_width_conversions_executes_after_the_merge() {
        let (linked, warnings) = link_against_artifact(
            "external fn mulhi(a: i32, b: i32) -> i32;\n\
             use { mulhi } from rustlib;\n\
             pub fn high_product(a: i32, b: i32) -> i32 { return mulhi(a, b); }",
            "toolchain_width_conversions",
            1,
        );
        assert!(
            warnings.is_empty(),
            "a leaf that never touches memory has no reach to warn about; got {warnings:?}"
        );

        let (mut store, instance) = instantiate(&linked);
        let high_product: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "high_product")
            .expect("the merged module exports `high_product`");

        for (a, b, expected, what) in [
            (
                65_536,
                65_536,
                1,
                "a product of exactly 2^32 has all of its content above the low word, so a \
                 body that stayed in 32 bits returns 0",
            ),
            (
                2_147_483_647,
                2_147_483_647,
                1_073_741_823,
                "the largest positive product, far outside anything 32 bits can hold",
            ),
            (
                -1,
                -1,
                0,
                "widening unsigned instead of signed turns this into (2^32-1)^2 and returns -2",
            ),
            (
                -1,
                1,
                -1,
                "the high word of a negative product is its sign extension; widening \
                 unsigned returns 0",
            ),
            (
                3,
                5,
                0,
                "a product with no high bits must return none, ruling out a body that \
                 returns the low word",
            ),
        ] {
            assert_eq!(
                high_product.call(&mut store, (a, b)).expect("executes"),
                expected,
                "mulhi({a}, {b}): {what}"
            );
        }
    }

    /// Tier B: the same artifact's pointer walk reads memory this program owns.
    ///
    /// The arrays are ordinary Inference locals, so their addresses are frame
    /// pointers the compiler chose; the loop reading them is machine code `rustc`
    /// emitted. That the two agree on where the bytes are is the property Tier B
    /// exists to make usable, and only executing it shows the property holds. The
    /// forwarding ABI itself is not what is new here — the sibling
    /// `extern_link.rs` already hands an Inference frame array to a foreign body
    /// that writes through it — what is new is that a foreign optimizer chose the
    /// instructions doing the reading.
    ///
    /// Three things vary, each closing a way the assertions could hold without
    /// the property doing. `n` varies, so the loop reads the count it was handed
    /// rather than a fixed extent. The *pointer* varies across two arrays at
    /// different frame offsets, so a body that ignored `p` and read one constant
    /// address cannot pass. And `n = -1` exercises the guard the artifact opens
    /// with — `n > 0 ? n : 0` — which every non-negative count leaves inert; drop
    /// the guard and that call walks memory until it traps instead of returning.
    ///
    /// The shadow stack is read across the calls for the reason
    /// [`stack_pointer`] gives: every call re-derives its frame from the pointer's
    /// current value, so a lost epilogue keeps all six results correct while
    /// walking the stack down.
    /// The main is built at sixteen pages because the external declares that
    /// many and the reconciled memory keeps the larger minimum inside the kept
    /// maximum — a main at the default single page cannot take this external at
    /// all.
    #[test]
    fn a_tier_b_pointer_walk_reads_inference_owned_memory() {
        let (linked, warnings) = link_against_artifact(
            "external fn sum_n(p: [i32; 4], n: i32) -> i32;\n\
             use { sum_n } from rustlib;\n\
             pub fn total(n: i32) -> i32 {\n\
                 let values: [i32; 4] = [10, 20, 30, 40];\n\
                 return sum_n(values, n);\n\
             }\n\
             pub fn other_total(n: i32) -> i32 {\n\
                 let spacer: [i32; 4] = [0, 0, 0, 0];\n\
                 let values: [i32; 4] = [1, 2, 3, 4];\n\
                 return sum_n(spacer, 0) + sum_n(values, n);\n\
             }",
            "toolchain_tier_b_exec",
            16,
        );
        assert_eq!(
            warnings.len(),
            1,
            "a Tier-B merge into a multi-page memory owes exactly one warning; got {warnings:?}"
        );

        let (mut store, instance) = instantiate(&linked);
        let total: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "total")
            .expect("the merged module exports `total`");
        let other_total: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "other_total")
            .expect("the merged module exports `other_total`");

        let before = stack_pointer(&mut store, &instance);

        assert_eq!(
            total.call(&mut store, 4).expect("executes"),
            100,
            "the merged loop must sum all four elements the Inference frame holds"
        );
        assert_eq!(
            total.call(&mut store, 2).expect("executes"),
            30,
            "a shorter count must stop early, so the loop reads `n` rather than a fixed extent"
        );
        assert_eq!(
            total.call(&mut store, 0).expect("executes"),
            0,
            "a zero count must read nothing at all"
        );
        assert_eq!(
            total.call(&mut store, -1).expect("executes"),
            0,
            "a negative count must be clamped to zero by the guard the artifact opens with, \
             not decremented toward it"
        );
        assert_eq!(
            other_total.call(&mut store, 4).expect("executes"),
            10,
            "a second buffer at a different frame offset must be summed as its own bytes; a \
             body reading a constant address would return 100 here"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            before,
            "every frame those buffers lived in must be unwound; a merged body that walked the \
             shadow stack down would leave all five results above unchanged"
        );
    }

    /// Tier B into a memoryless main: the merge adopts the external's memory and
    /// says what that costs.
    ///
    /// The warning is the one thing a user gets telling them the merge proved
    /// derivation and not containment, and its page count is the number that
    /// makes the difference material — a single page usually traps an
    /// out-of-region address, sixteen do not.
    ///
    /// The linker's own tests already hold the warning's fields and its rendered
    /// sentence to their shapes, against WAT fixtures. What they cannot state is
    /// the number: a fixture chooses its page count, and here sixteen is what a
    /// foreign toolchain declared and a memoryless main inherited without
    /// choosing anything. That is the assertion this test exists for; the field
    /// list and the rendered text come along because a page count nobody reads is
    /// not the point.
    #[test]
    fn a_memoryless_main_adopts_the_externals_memory_and_is_warned_about_its_reach() {
        let (linked, warnings) = link_against_artifact(
            "external fn sum_n(p: i32, n: i32) -> i32;\n\
             use { sum_n } from rustlib;\n\
             pub fn total(p: i32, n: i32) -> i32 { return sum_n(p, n); }",
            "toolchain_tier_b_warning",
            1,
        );

        assert_eq!(
            sections(&linked).memory_pages,
            vec![16],
            "the main declares no memory of its own, so the merged module must be the \
             external's sixteen pages"
        );

        let [warning] = warnings.as_slice() else {
            panic!("the merge owes exactly one warning; got {warnings:?}");
        };
        assert_eq!(
            warning,
            &LinkWarning::TierBInMultiPageMemory {
                fields: vec!["sum_n".to_string()],
                pages: 16,
            },
            "the warning must name the external whose reach is unbounded, and the page count \
             must be the reconciled memory's — which here is the external's own declaration, \
             adopted by a main that asked for one page"
        );
        assert!(
            warning.to_string().contains("16 pages"),
            "the rendered warning must state the page count a reader is meant to weigh; \
             got: {warning}"
        );
    }
}

//! A growable linear memory, end to end.
//!
//! Nothing Inference emits grows memory: a maximum above the size is room for a
//! linked module or the host to grow into. So the claim these tests make is
//! about a *pair* — an Inference program compiled with `[memory] max-pages`, and
//! a linked module that calls `memory.grow` — and it is made by executing the
//! merged module rather than by reading its bytes: the growth has to succeed at
//! run time, which a memory section that merely looked right would not show.
//!
//! The control is the same pair under the default fixed layout, which the
//! linker refuses up front, naming the key that would admit the growth.

#[cfg(test)]
mod memory_growth_tests {
    use std::path::Path;

    use inference::wasm_link::{SearchPath, resolve_external_modules};
    use inference::{
        LinkOptions, LinkOutput, LinkWarning, OverriddenMaximum, analyze, link_with_options, parse,
        type_check,
    };
    use inference_wasm_codegen::{
        CodegenOptions, CompilationMode, MemoryLayout, MemoryLayoutSource, MemoryRequest, codegen,
    };
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// A program with a stack frame, so it declares a memory of its own, whose
    /// entry point grows that memory by one page through the linked module and
    /// adds its array's first element to the page count `memory.grow` returns.
    const MAIN_SOURCE: &str = "\
external fn grow_by(pages: i32) -> i32;
use { grow_by } from memlib;

pub fn run() -> i32 {
    let arr: [i32; 4] = [1, 2, 3, 4];
    return grow_by(1) + arr[0];
}
";

    /// The same call with no stack frame of its own: a program whose code
    /// touches no memory, so the only memory in play is the one its build
    /// configured and the one the linked module declares.
    const SCALAR_MAIN_SOURCE: &str = "\
external fn grow_by(pages: i32) -> i32;
use { grow_by } from memlib;

pub fn run() -> i32 {
    return grow_by(1);
}
";

    /// A linked module that grows the shared memory by its argument, returning
    /// the page count before the growth, or -1 when growth failed.
    const MEMLIB_WAT: &str = r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            memory.grow)
          (export "grow_by" (func 0)))
    "#;

    /// A layout of one page that may grow to `max_pages`.
    fn layout_growable_to(max_pages: u32) -> MemoryLayout {
        MemoryLayout::resolve(
            MemoryRequest {
                pages: Some(1),
                max_pages: Some(max_pages),
                stack_size: None,
                import: false,
            },
            MemoryLayoutSource::Flag,
        )
        .expect("one page growable to a few more is admissible")
    }

    /// Compiles `MAIN_SOURCE` under `layout` and links it against `MEMLIB_WAT`
    /// the way `infc -L <dir>` does, returning the merged module or the
    /// linker's refusal.
    fn build_and_link(layout: MemoryLayout, mode: CompilationMode) -> Result<Vec<u8>, String> {
        build_and_link_source(MAIN_SOURCE, layout, mode)
    }

    fn build_and_link_source(
        source: &str,
        layout: MemoryLayout,
        mode: CompilationMode,
    ) -> Result<Vec<u8>, String> {
        let lib_dir = memlib_dir();
        link_in(source, lib_dir.path(), layout, mode).map(|output| output.wasm)
    }

    /// A library search directory holding `MEMLIB_WAT` as `memlib.wasm`.
    fn memlib_dir() -> tempfile::TempDir {
        let lib_dir = tempfile::tempdir().expect("create a library search directory");
        let lib = wat::parse_str(MEMLIB_WAT).expect("the library WAT assembles");
        std::fs::write(lib_dir.path().join("memlib.wasm"), lib).expect("write the library");
        lib_dir
    }

    /// Compiles `source` under `layout` and links it against the libraries in
    /// `lib_dir`, returning the linker's whole output, the merged module and
    /// its warnings, or its refusal.
    fn link_in(
        source: &str,
        lib_dir: &Path,
        layout: MemoryLayout,
        mode: CompilationMode,
    ) -> Result<LinkOutput, String> {
        let arena = parse(source).expect("the program parses");
        let typed = type_check(arena).expect("the program type-checks");
        analyze(&typed).expect("the program passes analysis");
        let main = codegen(
            &typed,
            "main",
            CodegenOptions {
                mode,
                layout,
                ..CodegenOptions::default()
            },
        )
        .expect("codegen succeeds");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("the library resolves and validates");
        link_with_options(
            main.wasm(),
            &externals.module_bytes(),
            Some(&externals.contracts),
            &LinkOptions::default(),
        )
        .map_err(|e| e.to_string())
    }

    fn instantiate(wasm: &[u8]) -> (Store<()>, Instance) {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));
        (store, instance)
    }

    /// The main module's maximum is the merged module's, and growth up to it
    /// succeeds: the first call finds one page and leaves two, and growth stops
    /// exactly at the declared maximum rather than before or past it.
    #[test]
    fn a_linked_module_grows_the_memory_up_to_the_declared_maximum() {
        let linked = build_and_link(layout_growable_to(3), CompilationMode::Compile)
            .expect("a growable main admits the library's growth");
        inf_wasmparser::validate(&linked).expect("the merged module is valid");

        let (mut store, instance) = instantiate(&linked);
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("the merged module exports its memory");
        assert_eq!(memory.ty(&store).maximum(), Some(3));
        assert_eq!(memory.size(&store), 1, "the memory starts at its declared size");

        let run: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "run")
            .expect("the merged module exports `run`");
        assert_eq!(
            run.call(&mut store, ()).expect("run executes"),
            2,
            "growth from one page returns 1, plus arr[0]"
        );
        assert_eq!(memory.size(&store), 2, "the memory grew by one page");
        assert_eq!(run.call(&mut store, ()).expect("run executes"), 3);
        assert_eq!(memory.size(&store), 3, "the memory reached its maximum");
        assert_eq!(
            run.call(&mut store, ()).expect("run executes"),
            0,
            "growth past the maximum fails with -1, plus arr[0]"
        );
        assert_eq!(memory.size(&store), 3, "a failed growth leaves the memory as it was");
    }

    /// The control: under the default fixed layout the growth could never
    /// succeed, so the link is refused, and the refusal names the key that
    /// would admit it.
    #[test]
    fn a_fixed_main_refuses_the_growing_library_naming_the_maximum() {
        let err = build_and_link(MemoryLayout::default(), CompilationMode::Compile)
            .expect_err("a fixed memory cannot admit growth");
        assert!(err.contains("grows linear memory"), "{err}");
        assert!(
            err.contains("`max-pages`") && err.contains("--max-memory-pages"),
            "the refusal must name the setting that admits growth, got: {err}"
        );
    }

    /// A program that touches no memory still declares the memory its build
    /// configured, so the linked module's declaration cannot replace it. Were
    /// the program memoryless, the linker would adopt the library's own
    /// `(memory 1)` — no maximum at all — and a build that asked for a cap of
    /// three pages would ship an unbounded memory.
    #[test]
    fn a_memoryless_program_keeps_the_maximum_its_build_configured() {
        let linked = build_and_link_source(
            SCALAR_MAIN_SOURCE,
            layout_growable_to(3),
            CompilationMode::Compile,
        )
        .expect("a growable main admits the library's growth");

        let (mut store, instance) = instantiate(&linked);
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("the merged module exports its memory");
        assert_eq!(
            memory.ty(&store).maximum(),
            Some(3),
            "the configured maximum must survive the link"
        );
        let run: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "run")
            .expect("the merged module exports `run`");
        assert_eq!(run.call(&mut store, ()).expect("run executes"), 1);
        assert_eq!(run.call(&mut store, ()).expect("run executes"), 2);
        assert_eq!(
            run.call(&mut store, ()).expect("run executes"),
            -1,
            "growth stops at the configured maximum"
        );
    }

    /// That configured maximum replaces the library's own: `memlib` declares
    /// no maximum, so on its own its `memory.grow` never stops, and linked
    /// under three pages it stops at three. The link goes ahead and names
    /// `memlib::grow_by` as a growing import whose maximum moved, recording that
    /// its module declares none.
    #[test]
    fn a_library_with_no_maximum_is_named_under_a_growable_program() {
        let lib_dir = memlib_dir();
        let output = link_in(
            SCALAR_MAIN_SOURCE,
            lib_dir.path(),
            layout_growable_to(3),
            CompilationMode::Compile,
        )
        .expect("a growable main admits the library's growth");
        assert_eq!(
            memory_limits(&output.wasm),
            Some((1, Some(3))),
            "the warning must not change the linked memory"
        );
        assert_eq!(
            output.warnings,
            vec![LinkWarning::MemoryMaximumOverridden {
                program_maximum: Some(3),
                overridden: vec![OverriddenMaximum {
                    module: "memlib".to_string(),
                    field: "grow_by".to_string(),
                    declared_maximum: None,
                }],
            }],
            "the one growing import is named, with its module declaring no maximum"
        );
    }

    /// The same holds for a configured fixed memory: the build's pages are the
    /// merged module's, and a library that grows memory is refused against
    /// them rather than handed an unbounded memory of its own.
    #[test]
    fn a_memoryless_program_keeps_the_fixed_memory_its_build_configured() {
        let fixed = MemoryLayout::resolve(
            MemoryRequest {
                pages: Some(2),
                ..MemoryRequest::default()
            },
            MemoryLayoutSource::Flag,
        )
        .expect("two fixed pages are admissible");
        let err = build_and_link_source(SCALAR_MAIN_SOURCE, fixed, CompilationMode::Compile)
            .expect_err("a configured fixed memory cannot admit growth");
        assert!(err.contains("grows linear memory"), "{err}");
    }

    /// The proof artifact describes the same machine: the `.v` a proof-mode
    /// build of the program translates to carries the declared maximum.
    #[test]
    fn the_proof_translation_carries_the_declared_maximum() {
        let arena = parse("pub fn f() -> i32 { let arr: [i32; 4] = [1, 2, 3, 4]; return arr[0]; }")
            .expect("the program parses");
        let typed = type_check(arena).expect("the program type-checks");
        let output = codegen(
            &typed,
            "growable",
            CodegenOptions {
                mode: CompilationMode::Proof,
                layout: layout_growable_to(8),
                ..CodegenOptions::default()
            },
        )
        .expect("codegen succeeds");
        let v = inference::wasm_to_v(
            "growable",
            output.wasm(),
            output.spec_func_indices_by_spec(),
            output.hspecs(),
        )
        .expect("the module translates");
        assert!(
            v.contains("Mm {|lim_min := 1%N; lim_max := Some(8%N)|}"),
            "the translation must describe one page growable to eight:\n{v}"
        );
    }

    // -- The order of `external fn` declarations ------------------------------
    //
    // A program's imports are its `external fn` declarations, in the order they
    // were written, and the linker used to settle a memoryless program's shared
    // memory by that order: whichever memory-using library came first set the
    // maximum. Each case below compiles one program twice, with only its two
    // `external fn` lines swapped, links both against the same libraries, runs
    // both, and asserts the two programs are indistinguishable — the same memory,
    // the same result and the same linker warnings, or the same refusal.

    /// Libraries for the declaration-order cases, each exporting one function of
    /// `(i32) -> i32` over a memory of its own: `grow*` grows memory by its
    /// argument (returning the page count before, or -1), `peek*` loads at its
    /// argument.
    const PROVIDERS: [(&str, &str, &str, &str); 4] = [
        ("a", "growA", "1 10", "memory.grow"),
        ("b", "growB", "1 2", "memory.grow"),
        ("d", "peekD", "10 10", "i32.load"),
        ("e", "peekE", "1 10", "i32.load"),
    ];

    /// A library directory holding every [`PROVIDERS`] module as `<module>.wasm`.
    fn provider_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create a library search directory");
        for (module, field, limits, op) in PROVIDERS {
            let wat = format!(
                r#"
                (module
                  (type (;0;) (func (param i32) (result i32)))
                  (memory (;0;) {limits})
                  (func (;0;) (type 0) (param i32) (result i32)
                    local.get 0
                    {op})
                  (export "{field}" (func 0)))
                "#
            );
            let wasm = wat::parse_str(&wat).expect("the library WAT assembles");
            std::fs::write(dir.path().join(format!("{module}.wasm")), wasm)
                .expect("write the library");
        }
        dir
    }

    /// The `(initial, maximum)` limits of the module's memory section, or `None`
    /// when it defines no memory.
    fn memory_limits(wasm: &[u8]) -> Option<(u64, Option<u64>)> {
        for payload in inf_wasmparser::Parser::new(0).parse_all(wasm) {
            if let inf_wasmparser::Payload::MemorySection(reader) = payload.expect("valid payload")
            {
                let memory = reader.into_iter().next()?.expect("a memory type");
                return Some((memory.initial, memory.maximum));
            }
        }
        None
    }

    /// What one compiled-and-linked program is observably: its memory's limits,
    /// what `run` returns and the warnings the link raised, or the linker's
    /// refusal.
    type Linked = Result<(Option<(u64, Option<u64>)>, i32, Vec<LinkWarning>), String>;

    fn build_link_and_run(source: &str, layout: MemoryLayout) -> Linked {
        let lib_dir = provider_dir();
        let LinkOutput { wasm, warnings } =
            link_in(source, lib_dir.path(), layout, CompilationMode::Compile)?;
        inf_wasmparser::validate(&wasm).expect("the merged module is valid");
        let (mut store, instance) = instantiate(&wasm);
        let run: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "run")
            .expect("the merged module exports `run`");
        let result = run.call(&mut store, ()).expect("run executes");
        Ok((memory_limits(&wasm), result, warnings))
    }

    /// Builds the program declaring `externs` in both orders — its `use` lines
    /// and body unchanged — asserts the two programs agree, and returns what
    /// they agree on.
    fn in_both_declaration_orders(
        externs: [&str; 2],
        uses: &str,
        body: &str,
        layout: MemoryLayout,
    ) -> Linked {
        let program = |first: &str, second: &str| {
            format!(
                "external fn {first}(n: i32) -> i32;\nexternal fn {second}(n: i32) -> i32;\n{uses}\n\
                 pub fn run() -> i32 {{\n    return {body};\n}}\n"
            )
        };
        let forward = build_link_and_run(&program(externs[0], externs[1]), layout);
        let reverse = build_link_and_run(&program(externs[1], externs[0]), layout);
        assert_eq!(
            forward, reverse,
            "swapping the `external fn {}` and `external fn {}` declarations changed the program",
            externs[0], externs[1]
        );
        forward
    }

    /// What `growB(2)` returns in `b` on its own: the behaviour the linked
    /// program is held to.
    fn grow_b_standalone(pages: i32) -> i32 {
        let lib_dir = provider_dir();
        let wasm = std::fs::read(lib_dir.path().join("b.wasm")).expect("read the library");
        let (mut store, instance) = instantiate(&wasm);
        let grow: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "growB")
            .expect("the library exports `growB`");
        grow.call(&mut store, pages).expect("growB executes")
    }

    /// A memoryless program calling `growB` and `peekE`: the shared memory takes
    /// `b`'s two-page cap, so `growB(2)` fails exactly as it does in `b` alone,
    /// in either order. Declared first, `peekE` used to hand `growB` its own
    /// ten pages, and the growth succeeded. No maximum was replaced, so the link
    /// warns of nothing.
    #[test]
    fn a_growing_library_keeps_its_own_cap_in_either_declaration_order() {
        let linked = in_both_declaration_orders(
            ["growB", "peekE"],
            "use { growB } from b;\nuse { peekE } from e;",
            "growB(2) + peekE(0)",
            MemoryLayout::default(),
        );
        assert_eq!(
            grow_b_standalone(2),
            -1,
            "two pages of growth exceed b's cap"
        );
        assert_eq!(
            linked,
            Ok((Some((1, Some(2))), -1, vec![])),
            "growB(2) must fail as it does in b alone, peekE(0) reads 0, and a memoryless \
             program never warns of a replaced maximum"
        );
    }

    /// Two libraries that grow memory under different caps cannot share one
    /// memory without one of them stopping somewhere its module never declared,
    /// so a memoryless program linking both is refused, in either order, and
    /// told to give itself a memory. `run` never calls `growA`: a declared
    /// external contributes its memory whether or not anything calls it.
    #[test]
    fn two_growing_libraries_with_different_caps_are_refused_in_either_order() {
        let err = in_both_declaration_orders(
            ["growA", "growB"],
            "use { growA } from a;\nuse { growB } from b;",
            "growB(2)",
            MemoryLayout::default(),
        )
        .expect_err("the two caps cannot both hold");
        assert!(
            err.contains("for `growA`") && err.contains("`b::growB`"),
            "the refusal names both libraries: {err}"
        );
        assert!(
            err.contains("`max-pages` in the `[memory]` table")
                && err.contains("--max-memory-pages"),
            "the refusal names the setting that resolves it: {err}"
        );
    }

    /// The remedy the refusal names: a program with its own growable memory.
    /// Its maximum governs both libraries — `b`'s two-page cap included, by the
    /// author's choice — so either order links to that memory and `growB(2)`
    /// succeeds within it. The link says so in one warning that names
    /// `b::growB`, whose two pages were raised to ten, and not `a`, which
    /// declares ten itself.
    #[test]
    fn a_configured_maximum_governs_both_libraries_in_either_order() {
        let linked = in_both_declaration_orders(
            ["growA", "growB"],
            "use { growA } from a;\nuse { growB } from b;",
            "growB(2)",
            layout_growable_to(10),
        );
        assert_eq!(
            linked,
            Ok((
                Some((1, Some(10))),
                1,
                vec![LinkWarning::MemoryMaximumOverridden {
                    program_maximum: Some(10),
                    overridden: vec![OverriddenMaximum {
                        module: "b".to_string(),
                        field: "growB".to_string(),
                        declared_maximum: Some(2),
                    }],
                }],
            )),
            "growth from one page returns 1, and only b's replaced maximum is named"
        );
    }

    /// One page that may grow to one page is the default layout, so a program
    /// built with `max-pages = 1` stays memoryless and meets the same refusal
    /// as one built with no setting at all.
    #[test]
    fn a_maximum_of_one_page_is_the_default_and_does_not_resolve_the_refusal() {
        let one_page = MemoryLayout::resolve(
            MemoryRequest {
                pages: Some(1),
                max_pages: Some(1),
                stack_size: None,
                import: false,
            },
            MemoryLayoutSource::Flag,
        )
        .expect("one fixed page is admissible");
        assert!(
            one_page.is_default_memory(),
            "one fixed page is the default memory"
        );
        let refused = |layout| {
            in_both_declaration_orders(
                ["growA", "growB"],
                "use { growA } from a;\nuse { growB } from b;",
                "growB(2)",
                layout,
            )
        };
        let with_setting = refused(one_page).expect_err("the program stays memoryless");
        let without = refused(MemoryLayout::default()).expect_err("a memoryless program");
        assert_eq!(with_setting, without);
    }

    /// Under a program's own growable memory, a library that raises the shared
    /// minimum to the shared maximum pins it, so `growA` could never grow. The
    /// check runs against the final memory: declared before `peekD`, `growA`
    /// used to pass it and ship a `memory.grow` that always returns -1.
    #[test]
    fn growth_a_later_library_makes_impossible_is_refused_in_either_order() {
        let err = in_both_declaration_orders(
            ["growA", "peekD"],
            "use { growA } from a;\nuse { peekD } from d;",
            "growA(1) + peekD(0)",
            layout_growable_to(10),
        )
        .expect_err("a pinned memory cannot admit growth");
        assert!(
            err.contains("for `growA`")
                && err.contains("maximum (10 pages) does not exceed its minimum (10 pages)"),
            "{err}"
        );
    }
}

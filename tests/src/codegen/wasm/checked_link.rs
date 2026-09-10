//! The `inference.checked` section across the static-merge linker.
//!
//! The section records which functions of a module trap when their arithmetic
//! overflows, and its absence records that none does — which is exactly true of
//! a module produced by any other toolchain, none of which emits an Inference
//! overflow guard. The linker rewrites it into the merged index space, re-emits
//! one merged section, and refuses a link in which a reachability
//! specification reaches a guarded body a library supplied.
//!
//! That refusal is the linker's alone. Code generation refuses the same reach
//! for every callee whose body it can see, and it can never see an
//! `external fn`'s: it is handed a typed context, while the dependency bytes
//! arrive one phase later, at `link`.
//!
//! The fixtures are built in memory rather than committed, because what is under
//! test is a *pair* of modules and the relation between them: a committed golden
//! would pin the bytes of each half separately and say nothing about the link.
//! The one exception is the committed `rustlib.wasm`, which is here to prove
//! that a `.wasm` no Inference compiler produced links at all — a fact about a
//! foreign artifact that cannot be built from Inference source by definition.
//!
//! Some tests hand the linker bytes the compiler would never emit: a section
//! spliced in after code generation, or replaced with the opaque form a
//! post-build optimizer's marker takes. Those cover the arms the compiler driver
//! cannot reach, which the public linker API can.

#[cfg(test)]
mod checked_link_tests {
    use std::path::Path;

    use inf_wasmparser::{BinaryReader, Parser, Payload};
    use inference::wasm_link::{resolve_external_modules, SearchPath};
    use inference::{LinkOptions, link_with_options, parse, type_check};
    use inference_type_checker::typed_context::TypedContext;
    use inference_wasm_codegen::{
        CHECKED_SECTION_NAME, CHECKED_SECTION_VERSION, CodegenOptions, CompilationMode,
        EmitFeatures, MemoryLayout, OptLevel, SPEC_FUNCS_SECTION_NAME, Target,
    };

    /// The exact wire form's version as the single payload byte it encodes to.
    const CHECKED_SECTION_VERSION_BYTE: u8 = CHECKED_SECTION_VERSION as u8;

    /// A library whose exported function traps on overflow.
    const GUARDED_LIB: &str = "pub fn double(x: i32) -> i32 { return checked(x + x); }";

    /// The same library with the annotation removed: the emitted arithmetic
    /// wraps, so nothing is guarded and the module carries no section at all.
    const WRAPPING_LIB: &str = "pub fn double(x: i32) -> i32 { return x + x; }";

    /// A library whose *exported* function is unguarded and whose private
    /// callee is not, so a walk that stopped one hop past the specification
    /// would find nothing.
    const TWO_HOP_LIB: &str = "fn helper(x: i32) -> i32 { return checked(x + x); } \
                               pub fn entry(x: i32) -> i32 { return helper(x); }";

    /// A program whose `exists`-bodied specification calls the linked function
    /// directly.
    ///
    /// Shaped like the committed reachability fixtures: the body declares no
    /// return type and contains no `return` (the verifier reduces it without an
    /// enclosing activation frame), takes no entry parameter (the judgment
    /// quantifies entry values universally, so an `assume` over one falsifies
    /// rather than filters), and states a real property so the obligation does
    /// not collapse to the vacuous shape.
    fn exists_program(callee: &str) -> String {
        format!(
            "external fn {callee}(x: i32) -> i32;
             use {{ {callee} }} from mathlib;
             pub fn twice(x: i32) -> i32 {{ return {callee}(x); }}
             spec ReachableExtern {{
               fn reaches_six() exists {{
                 let n: i32 = @;
                 assume {{ assert(n == 3); }}
                 assert({callee}(n) == 6);
               }}
             }}"
        )
    }

    /// The `unique` counterpart: the two kinds are gathered into different
    /// partitions and consumed by different predicates, so one being refused
    /// says nothing about the other.
    fn unique_program(callee: &str) -> String {
        format!(
            "external fn {callee}(x: i32) -> i32;
             use {{ {callee} }} from mathlib;
             pub fn twice(x: i32) -> i32 {{ return {callee}(x); }}
             spec UniqueExtern {{
               fn one_choice() unique {{
                 let n: i32 = @;
                 assume {{ assert(n == 3); }}
                 assert({callee}(n) == 6);
               }}
             }}"
        )
    }

    /// A `forall`-bodied specification over the same linked function.
    ///
    /// The accepting side of the rule, and it is not an oversight: a universal
    /// obligation is discharged denotationally rather than by reducing a
    /// retained body, so a trap in the callee does not empty an observation set
    /// — it makes the universal claim about a trapping application false, which
    /// is the corpus invariant's business and not the linker's.
    fn forall_program(callee: &str) -> String {
        format!(
            "external fn {callee}(x: i32) -> i32;
             use {{ {callee} }} from mathlib;
             pub fn twice(x: i32) -> i32 {{ return {callee}(x); }}
             spec UniversalExtern {{
               fn doubling_is_doubling() forall {{
                 let n: i32 = @;
                 assert({callee}(n) == twice(n));
               }}
             }}"
        )
    }

    /// An `exists` body that reaches the library through one of the program's
    /// own functions rather than calling it itself.
    fn two_hop_through_the_program(callee: &str) -> String {
        format!(
            "external fn {callee}(x: i32) -> i32;
             use {{ {callee} }} from mathlib;
             pub fn twice(x: i32) -> i32 {{ return {callee}(x); }}
             spec ReachableExtern {{
               fn reaches_six() exists {{
                 let n: i32 = @;
                 assume {{ assert(n == 3); }}
                 assert(twice(n) == 6);
               }}
             }}"
        )
    }

    /// A throwaway library search directory holding one `.wasm`, laid out the
    /// way `-L` expects to find it.
    struct LibDir {
        dir: tempfile::TempDir,
    }

    impl LibDir {
        fn with(logical_module: &str, wasm: &[u8]) -> Self {
            let dir = tempfile::tempdir().expect("create a library search directory");
            std::fs::write(dir.path().join(format!("{logical_module}.wasm")), wasm)
                .expect("write the library");
            LibDir { dir }
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    /// Compiles a typed program at `mode`, the way `infc` does.
    fn compile(typed: &TypedContext, module_name: &str, mode: CompilationMode) -> Vec<u8> {
        inference_wasm_codegen::codegen(
            typed,
            module_name,
            CodegenOptions {
                target: Target::Wasm32,
                mode,
                opt_level: OptLevel::O3,
                features: EmitFeatures::default(),
                layout: MemoryLayout::default(),
            },
        )
        .expect("codegen succeeds")
        .wasm()
        .to_vec()
    }

    /// Compiles `source` in compile mode, the way a library is built.
    fn compile_library(source: &str) -> Vec<u8> {
        let arena = parse(source).expect("the library parses");
        let typed = type_check(arena).expect("the library type-checks");
        inference_analysis::analyze(&typed).expect("the library passes analysis");
        compile(&typed, "mathlib_impl", CompilationMode::Compile)
    }

    /// Runs the pipeline an `infc -v -L <dir>` invocation runs, stopping at the
    /// link, and returning whatever the linker answered.
    ///
    /// One step short of parity: `inference_analysis::analyze` is not run, so a
    /// program these tests write is held only to what the type checker and the
    /// linker say about it. That is deliberate — the arms under test are the
    /// linker's, and a rule rejecting the fixture first would leave them
    /// unreached — and it is why a source here may hold a shape `infc` would
    /// refuse at the command line.
    fn link_against(main_source: &str, lib_dir: &Path) -> Result<Vec<u8>, String> {
        link_bytes_against(main_source, &compile_main(main_source), lib_dir)
    }

    /// Compiles a program's own module the way [`link_against`] does, for a test
    /// that edits the bytes before handing them to the linker.
    fn compile_main(main_source: &str) -> Vec<u8> {
        let arena = parse(main_source).expect("the program parses");
        let typed = type_check(arena).expect("the program type-checks");
        compile(&typed, "main", CompilationMode::Proof)
    }

    /// Links `main_bytes` against the libraries in `lib_dir`, resolving the
    /// externals `main_source` declares.
    ///
    /// Takes the bytes separately from the source so a test can splice a section
    /// into them first: the arms that judge a program's *own* guarded function
    /// are unreachable from the compiler driver, which refuses that shape at
    /// code generation, and this is the library entry point that accepts
    /// whatever bytes a caller has.
    fn link_bytes_against(
        main_source: &str,
        main_bytes: &[u8],
        lib_dir: &Path,
    ) -> Result<Vec<u8>, String> {
        let arena = parse(main_source).expect("the program parses");
        let typed = type_check(arena).expect("the program type-checks");
        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("the external resolves and validates");
        link_with_options(
            main_bytes,
            &externals.module_bytes(),
            Some(&externals.contracts),
            &LinkOptions::default(),
        )
        .map(|output| output.wasm)
        .map_err(|e| e.to_string())
    }

    /// The raw `inference.checked` payload bytes, or `None` when the module
    /// carries no such section.
    ///
    /// Reads the bytes rather than the decoded list, because the two wire forms
    /// are told apart by their first byte and a test about *which form* was
    /// emitted has to see it.
    fn checked_payload(wasm: &[u8]) -> Option<Vec<u8>> {
        for payload in Parser::new(0).parse_all(wasm) {
            let Payload::CustomSection(reader) = payload.expect("the module parses") else {
                continue;
            };
            if reader.name() == CHECKED_SECTION_NAME {
                return Some(reader.data().to_vec());
            }
        }
        None
    }

    /// The opaque `inference.checked` payload: the version byte alone.
    ///
    /// Spelled here as the literal bytes rather than taken from a constant,
    /// because these tests stand in for the producer `infs` is and a helper
    /// shared with the decoder would agree with it by construction.
    const OPAQUE_PAYLOAD: [u8; 1] = [2];

    /// Appends `value` to `out` as an unsigned LEB128.
    fn write_uleb(mut value: u32, out: &mut Vec<u8>) {
        loop {
            let mut byte = u8::try_from(value & 0x7f).expect("seven bits fit in a byte");
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return;
            }
        }
    }

    /// Appends one custom section to `out`.
    fn push_custom_section(name: &str, payload: &[u8], out: &mut Vec<u8>) {
        let mut body = Vec::new();
        write_uleb(
            u32::try_from(name.len()).expect("a section name fits in u32"),
            &mut body,
        );
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(payload);
        out.push(0);
        write_uleb(
            u32::try_from(body.len()).expect("a section fits in u32"),
            out,
        );
        out.extend_from_slice(&body);
    }

    /// Rebuilds `wasm` with the custom section named `name` carrying `payload`,
    /// appending the section when the module has none.
    ///
    /// Every other section is copied verbatim out of the input, so the result
    /// differs from it in exactly one section — which is what makes a test that
    /// links the result a test about that section and not about a re-encode.
    fn with_custom_section(wasm: &[u8], name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = wasm[..8].to_vec();
        let mut replaced = false;
        for item in Parser::new(0).parse_all(wasm) {
            let item = item.expect("the module parses");
            if let Payload::CustomSection(reader) = &item
                && reader.name() == name
            {
                push_custom_section(name, payload, &mut out);
                replaced = true;
                continue;
            }
            let Some((id, range)) = item.as_section() else {
                continue;
            };
            out.push(id);
            write_uleb(
                u32::try_from(range.len()).expect("a section fits in u32"),
                &mut out,
            );
            out.extend_from_slice(&wasm[range]);
        }
        if !replaced {
            push_custom_section(name, payload, &mut out);
        }
        out
    }

    /// Rebuilds `wasm` with one more custom section named `name`, leaving any
    /// existing one in place.
    fn with_appended_custom_section(wasm: &[u8], name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = wasm.to_vec();
        push_custom_section(name, payload, &mut out);
        out
    }

    /// Rebuilds `wasm` without the custom section named `name`.
    fn without_custom_section(wasm: &[u8], name: &str) -> Vec<u8> {
        let mut out = wasm[..8].to_vec();
        for item in Parser::new(0).parse_all(wasm) {
            let item = item.expect("the module parses");
            if let Payload::CustomSection(reader) = &item
                && reader.name() == name
            {
                continue;
            }
            let Some((id, range)) = item.as_section() else {
                continue;
            };
            out.push(id);
            write_uleb(
                u32::try_from(range.len()).expect("a section fits in u32"),
                &mut out,
            );
            out.extend_from_slice(&wasm[range]);
        }
        out
    }

    /// The output function index the module's `name` section gives `wanted`.
    fn function_index_named(wasm: &[u8], wanted: &str) -> u32 {
        for payload in Parser::new(0).parse_all(wasm) {
            let Payload::CustomSection(reader) = payload.expect("the module parses") else {
                continue;
            };
            let inf_wasmparser::KnownCustom::Name(names) = reader.as_known() else {
                continue;
            };
            for subsection in names {
                let inf_wasmparser::Name::Function(map) =
                    subsection.expect("the name section parses")
                else {
                    continue;
                };
                for naming in map {
                    let naming = naming.expect("a name entry parses");
                    if naming.name == wanted {
                        return naming.index;
                    }
                }
            }
        }
        panic!("no function of the module is named `{wanted}`");
    }

    /// An `inference.spec_funcs` payload listing `spec` with no index at all.
    ///
    /// Hand-encoded rather than produced: the point of the fixture it builds is
    /// a module whose two verification sections disagree, which no producer
    /// emits.
    fn spec_funcs_payload_with_no_indices(spec: &str) -> Vec<u8> {
        let mut payload = vec![1, 1];
        write_uleb(
            u32::try_from(spec.len()).expect("a spec name fits in u32"),
            &mut payload,
        );
        payload.extend_from_slice(spec.as_bytes());
        payload.push(0);
        payload
    }

    /// The function indices an `inference.checked` section lists, or `None` when
    /// the module carries no such section.
    fn checked_indices(wasm: &[u8]) -> Option<Vec<u32>> {
        for payload in Parser::new(0).parse_all(wasm) {
            let Payload::CustomSection(reader) = payload.expect("the module parses") else {
                continue;
            };
            if reader.name() != CHECKED_SECTION_NAME {
                continue;
            }
            let mut bytes = BinaryReader::new(reader.data(), 0);
            let version = bytes.read_var_u32().expect("a version");
            assert_eq!(version, CHECKED_SECTION_VERSION, "unexpected section version");
            let count = bytes.read_var_u32().expect("a count");
            let mut indices = Vec::with_capacity(count as usize);
            for _ in 0..count {
                indices.push(bytes.read_var_u32().expect("an index"));
            }
            assert_eq!(bytes.bytes_remaining(), 0, "trailing bytes in the payload");
            return Some(indices);
        }
        None
    }

    /// `function index -> debug name` from the module's `name` section.
    fn function_names(wasm: &[u8]) -> Vec<(u32, String)> {
        let mut names = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            let Payload::CustomSection(reader) = payload.expect("the module parses") else {
                continue;
            };
            let inf_wasmparser::KnownCustom::Name(subsections) = reader.as_known() else {
                continue;
            };
            for subsection in subsections {
                let Ok(inf_wasmparser::Name::Function(map)) = subsection else {
                    continue;
                };
                for naming in map {
                    let naming = naming.expect("a naming");
                    names.push((naming.index, naming.name.to_string()));
                }
            }
        }
        names
    }

    /// The names of the functions a module's `inference.checked` section lists.
    fn guarded_names(wasm: &[u8]) -> Vec<String> {
        let names = function_names(wasm);
        checked_indices(wasm)
            .unwrap_or_default()
            .into_iter()
            .map(|idx| {
                names
                    .iter()
                    .find(|(entry, _)| *entry == idx)
                    .map_or_else(|| format!("#{idx}"), |(_, name)| name.clone())
            })
            .collect()
    }

    #[test]
    fn a_library_that_asks_for_no_guard_carries_no_section() {
        assert_eq!(
            checked_indices(&compile_library(WRAPPING_LIB)),
            None,
            "absence is the statement that nothing in the module traps on overflow"
        );
    }

    #[test]
    fn a_guarded_library_lists_exactly_the_functions_that_trap() {
        assert_eq!(guarded_names(&compile_library(GUARDED_LIB)), vec!["double"]);
        assert_eq!(
            guarded_names(&compile_library(TWO_HOP_LIB)),
            vec!["helper"],
            "the exported entry point wraps; only its private callee traps"
        );
    }

    #[test]
    fn the_merged_module_carries_the_section_at_the_post_link_indices() {
        // The program declares one function import, which the merge satisfies
        // and removes, so every index shifts. A section carried through
        // unrewritten would name a different function in the output than it
        // named in the library.
        let lib = compile_library(GUARDED_LIB);
        let pre_link = checked_indices(&lib).expect("the library carries a section");
        let dir = LibDir::with("mathlib", &lib);
        let merged = link_against(&forall_program("double"), dir.path())
            .expect("a `forall` specification over a guarded library links");

        let merged_indices = checked_indices(&merged).expect("the merged module carries a section");
        assert_eq!(merged_indices.len(), 1);
        assert_ne!(
            merged_indices, pre_link,
            "the merged index space is not the library's own; a section that came \
             through unchanged was not rewritten"
        );
        assert_eq!(
            guarded_names(&merged),
            vec!["mathlib::double"],
            "the rewritten index must name the merged body under the name the output \
             records for it"
        );
    }

    #[test]
    fn a_forall_specification_over_a_guarded_library_links() {
        let dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
        link_against(&forall_program("double"), dir.path())
            .expect("a universal obligation is discharged denotationally, not by reducing a body");
    }

    #[test]
    fn an_exists_specification_reaching_a_guarded_library_is_refused() {
        let dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
        let error = link_against(&exists_program("double"), dir.path())
            .expect_err("a reachability body that reaches a trapping merged body must be refused");
        assert!(
            error.contains("ReachableExtern.reaches_six"),
            "the rejection must name the specification function: {error}"
        );
        assert!(
            error.contains("`mathlib::double`"),
            "the rejection must name the function it reaches: {error}"
        );
        assert!(
            error.contains("linked module `mathlib`"),
            "the rejection must name where that body came from: {error}"
        );
    }

    #[test]
    fn a_unique_specification_reaching_a_guarded_library_is_refused() {
        let dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
        let error = link_against(&unique_program("double"), dir.path())
            .expect_err("the `unique` kind is refused for the same reason, and a worse one");
        assert!(
            error.contains("UniqueExtern.one_choice"),
            "the rejection must name the specification function: {error}"
        );
        assert!(error.contains("`mathlib::double`"), "{error}");
    }

    #[test]
    fn a_specification_reaching_a_guarded_library_through_its_own_function_is_refused() {
        // Two hops: the specification calls a function of the program, and that
        // function calls the library. A walk that stopped at the specification's
        // own call list would find nothing to object to.
        let dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
        let error = link_against(&two_hop_through_the_program("double"), dir.path())
            .expect_err("the walk must follow calls transitively");
        assert!(error.contains("`mathlib::double`"), "{error}");
    }

    #[test]
    fn a_specification_reaching_a_guarded_library_function_two_hops_in_is_refused() {
        // Two hops entirely inside the library: its exported entry point wraps
        // and its private callee traps, so the edge the walk has to follow is
        // one between two *merged* bodies — which is mapped through the merge's
        // own closure index rather than the main module's.
        let dir = LibDir::with("mathlib", &compile_library(TWO_HOP_LIB));
        let error = link_against(&exists_program("entry"), dir.path())
            .expect_err("a guarded body two hops into the library must be refused");
        assert!(
            error.contains("ReachableExtern.reaches_six"),
            "the rejection must name the specification function: {error}"
        );
        assert!(
            error.contains("`mathlib::#helper`"),
            "the rejection must name the private callee that traps, by the full merged name the \
             artifact carries — `#` marks a body no import asked for by name — rather than \
             the entry point that does not trap: {error}"
        );
    }

    #[test]
    fn a_merged_artifact_linked_as_a_library_still_reports_the_guard_it_absorbed() {
        // What the merged section is *for*. The rejection above reads the
        // sections of this link's own inputs, so it would reach the same verdict
        // with nothing re-emitted at all; the re-emitted section is what a
        // *later* link reads, and this is that later link.
        //
        // A program binds the guarded library and is linked into a
        // self-contained artifact. That artifact is then bound as a library by a
        // second program whose `exists` body reaches the guarded body two hops
        // in — through the first program's own function, which the merge has by
        // then made an ordinary function of the library.
        let first = {
            let dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
            let source = "external fn double(x: i32) -> i32;
                          use { double } from mathlib;
                          pub fn twice(x: i32) -> i32 { return double(x); }";
            let arena = parse(source).expect("the program parses");
            let typed = type_check(arena).expect("the program type-checks");
            let mut search_path = SearchPath::new();
            search_path.push_lib_dir(dir.path().to_path_buf());
            let externals = resolve_external_modules(&typed, &search_path, None)
                .expect("the external resolves and validates");
            let main = compile(&typed, "middle", CompilationMode::Compile);
            link_with_options(
                &main,
                &externals.module_bytes(),
                Some(&externals.contracts),
                &LinkOptions::default(),
            )
            .expect("a program with no specification links against a guarded library")
            .wasm
        };
        assert_eq!(
            guarded_names(&first),
            vec!["mathlib::double"],
            "the merged artifact carries the guard it absorbed"
        );

        let dir = LibDir::with("middle", &first);
        let error = link_against(
            "external fn twice(x: i32) -> i32;
             use { twice } from middle;
             pub fn quadruple(x: i32) -> i32 { return twice(twice(x)); }
             spec ReachableExtern {
               fn reaches_six() exists {
                 let n: i32 = @;
                 assume { assert(n == 3); }
                 assert(twice(n) == 6);
               }
             }",
            dir.path(),
        )
        .expect_err("the guard the first link absorbed must still be visible to the second");
        assert!(
            error.contains("ReachableExtern.reaches_six"),
            "the rejection must name the specification function: {error}"
        );
        assert!(
            error.contains("`middle::#mathlib::double`"),
            "the rejection must name the guarded body the first link absorbed, by the name two \
             merges have prefixed rather than by a bare substring of it: {error}"
        );
        assert!(
            error.contains("linked module `middle`"),
            "the guarded body now arrives from the artifact the first link produced: {error}"
        );
    }

    #[test]
    fn an_exists_specification_over_an_unguarded_library_links() {
        // The same program and the same library, with the annotation removed.
        // Without this pair the rejection above could be a rejection of linking
        // a reachability specification against anything at all.
        //
        // It is also the accepting side of the resolution itself: the
        // obligation's symbol resolves against the sections the compiler wrote,
        // and a guard-free merge is judged on that rather than waved through.
        // `an_unresolvable_obligation_is_refused_with_nothing_guarded_anywhere`
        // is the same pair with the resolution broken.
        let dir = LibDir::with("mathlib", &compile_library(WRAPPING_LIB));
        let merged = link_against(&exists_program("double"), dir.path())
            .expect("a library whose arithmetic wraps is reachable from any specification");
        assert_eq!(
            checked_indices(&merged),
            None,
            "with nothing guarded anywhere, the merged module carries no section either"
        );
    }

    #[test]
    fn an_exists_specification_over_a_foreign_library_links() {
        // A `.wasm` no Inference compiler produced: it carries no
        // `inference.checked` section, which is not a gap in what is known about
        // it — no other toolchain emits an overflow guard, so wrapping is what
        // its arithmetic does and the absence is exactly right.
        let rustlib = std::fs::read(
            crate::utils::get_test_data_path()
                .join("wasmlib")
                .join("rustlib.wasm"),
        )
        .expect("the committed foreign artifact is readable");
        assert_eq!(
            checked_indices(&rustlib),
            None,
            "a foreign artifact carries no section"
        );

        // The foreign library declares sixteen pages of memory, so the program
        // linking it must declare none of its own — every local here is a
        // scalar.
        let dir = LibDir::with("rustlib", &rustlib);
        let merged = link_against(
            "external fn clamp_add(a: i32, b: i32) -> i32;
             use { clamp_add } from rustlib;
             pub fn saturate(a: i32, b: i32) -> i32 { return clamp_add(a, b); }
             spec ReachableForeign {
               fn reaches_three() exists {
                 let n: i32 = @;
                 assume { assert(n == 3); }
                 assert(clamp_add(n, 0) == 3);
               }
             }",
            dir.path(),
        )
        .expect("a foreign library is reachable from a reachability specification");
        assert_eq!(checked_indices(&merged), None);
    }

    #[test]
    fn a_specification_reaching_the_program_s_own_guarded_function_is_refused() {
        // The arm the compiler driver never reaches: `P018` refuses a guarded
        // body of the program's own against the source line that wrote the
        // arithmetic, one phase earlier. It is reachable through the public
        // linker API, whose main bytes are whatever a caller hands over, so the
        // section is spliced into a program that has none.
        //
        // The program's own `twice` is marked guarded and the library it calls
        // is not, so the only guard anywhere is main's — which the check used
        // to skip by construction, returning `Ok` on a merge whose
        // specification does reach a trap.
        let source = two_hop_through_the_program("double");
        let main = compile_main(&source);
        assert_eq!(
            checked_payload(&main),
            None,
            "the program writes no `checked(...)`, so codegen emits no section"
        );
        let guarded = function_index_named(&main, "twice");
        let spliced = with_custom_section(
            &main,
            CHECKED_SECTION_NAME,
            &[CHECKED_SECTION_VERSION_BYTE, 1, u8::try_from(guarded).expect("a small index")],
        );

        let dir = LibDir::with("mathlib", &compile_library(WRAPPING_LIB));
        let error = link_bytes_against(&source, &spliced, dir.path())
            .expect_err("a guarded body of the program's own must be refused too");
        assert!(
            error.contains("`twice`"),
            "the rejection must name the program's own function: {error}"
        );
        assert!(
            error.contains("ReachableExtern.reaches_six"),
            "the rejection must name the specification function: {error}"
        );
        assert!(
            error.contains("code generation refuses"),
            "the main-module arm says the compiler reports this against the source line, \
             rather than repeating a remedy there is no source line here to apply: {error}"
        );
        assert!(
            !error.contains("linked module"),
            "no library supplied this body, so none may be named: {error}"
        );
    }

    #[test]
    fn a_reachability_obligation_that_resolves_to_no_function_is_refused() {
        // Fail-closed on the resolution itself. The walk starts at the function
        // an obligation names, so a symbol that resolves to nothing is not a
        // check that passed but one that never ran — and every way of failing
        // to resolve (a producer that spells symbols differently, a section an
        // optimizer dropped) would otherwise be a silent accept.
        //
        // The fixture is a module whose two verification sections disagree:
        // `inference.hspecs` carries the `exists` obligation while
        // `inference.spec_funcs` lists no index under its specification, so the
        // intersection that picks the root is empty.
        let source = exists_program("double");
        let main = compile_main(&source);
        let spliced = with_custom_section(
            &main,
            SPEC_FUNCS_SECTION_NAME,
            &spec_funcs_payload_with_no_indices("ReachableExtern"),
        );

        let dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
        let error = link_bytes_against(&source, &spliced, dir.path())
            .expect_err("an obligation whose root cannot be resolved must be refused");
        assert!(
            error.contains("ReachableExtern.reaches_six"),
            "the rejection must name the unresolved symbol: {error}"
        );
        assert!(
            error.contains("inference.spec_funcs"),
            "the rejection must say which half of the resolution failed: {error}"
        );
    }

    #[test]
    fn an_unresolvable_obligation_is_refused_with_nothing_guarded_anywhere() {
        // The same defect with every guard taken out of the picture. Whether an
        // obligation resolves is a question about the obligation, so the answer
        // must not turn on whether some unrelated function traps: the program
        // writes no `checked(...)` and links a library that writes none either,
        // nothing in the merged module is guarded, and the obligation is still
        // one whose walk could never have started.
        //
        // The accepting twin is
        // `an_exists_specification_over_an_unguarded_library_links`: the same
        // guard-free pair with its `inference.spec_funcs` section left as the
        // compiler wrote it, where the symbol resolves and the link stands.
        let source = exists_program("double");
        let main = compile_main(&source);
        assert_eq!(
            checked_payload(&main),
            None,
            "the program writes no `checked(...)`, so codegen emits no section"
        );
        let spliced = with_custom_section(
            &main,
            SPEC_FUNCS_SECTION_NAME,
            &spec_funcs_payload_with_no_indices("ReachableExtern"),
        );

        let library = compile_library(WRAPPING_LIB);
        assert_eq!(
            checked_payload(&library),
            None,
            "the library's arithmetic wraps, so it carries no section either"
        );
        let dir = LibDir::with("mathlib", &library);
        let error = link_bytes_against(&source, &spliced, dir.path())
            .expect_err("an obligation whose root cannot be resolved must be refused");
        assert!(
            error.contains("ReachableExtern.reaches_six"),
            "the rejection must name the unresolved symbol: {error}"
        );
        assert!(
            error.contains("inference.spec_funcs"),
            "the rejection must say which half of the resolution failed: {error}"
        );
    }

    #[test]
    fn an_opaque_library_under_an_exists_specification_is_refused_by_module() {
        // A library `infs` optimized after building it: the section survives,
        // but the indices it held stopped naming the bodies they were written
        // for, so the marker says only that something in there traps. Every
        // function it supplies is then a candidate, and the specification that
        // reaches one is refused by the name of the module rather than of a
        // function nobody can identify any more.
        let opaque = with_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &OPAQUE_PAYLOAD,
        );
        let dir = LibDir::with("mathlib", &opaque);
        let error = link_against(&exists_program("double"), dir.path())
            .expect_err("an optimized library's guarded set is unknown, so it is refused");
        assert!(
            error.contains("linked module `mathlib`"),
            "the rejection must name the module, which is all that can be named: {error}"
        );
        assert!(
            error.contains("can no longer be named"),
            "the rejection must say why no function is named: {error}"
        );
    }

    #[test]
    fn an_opaque_library_under_a_forall_specification_links() {
        // The accepting side, and the reason the marker is not simply a refusal
        // to link. A universal obligation is discharged denotationally rather
        // than by reducing a retained body, so no trap in the library empties an
        // observation set and the merge has nothing to object to.
        let opaque = with_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &OPAQUE_PAYLOAD,
        );
        let dir = LibDir::with("mathlib", &opaque);
        let merged = link_against(&forall_program("double"), dir.path())
            .expect("a universal specification is unaffected by the marker");
        assert_eq!(
            checked_payload(&merged).as_deref(),
            Some(&OPAQUE_PAYLOAD[..]),
            "the merged artifact repeats the admission rather than inventing a list"
        );
    }

    #[test]
    fn an_opaque_library_a_specification_does_not_reach_still_links() {
        // The marker widens *which* functions count as guarded, not whether the
        // walk still decides. This program's `exists` body calls only its own
        // `plain`, so nothing it reaches came out of the optimized library, and
        // the link stands — the marker is not a refusal to link against an
        // optimized artifact at all.
        let opaque = with_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &OPAQUE_PAYLOAD,
        );
        let dir = LibDir::with("mathlib", &opaque);
        let merged = link_against(
            "external fn double(x: i32) -> i32;
             use { double } from mathlib;
             pub fn plain(x: i32) -> i32 { return x; }
             spec Untouched {
               fn reaches_three() exists {
                 let n: i32 = @;
                 assume { assert(n == 3); }
                 assert(plain(n) == 3);
               }
             }",
            dir.path(),
        )
        .expect("a specification that reaches none of the library's bodies links");
        assert_eq!(
            checked_payload(&merged).as_deref(),
            Some(&OPAQUE_PAYLOAD[..]),
            "the merged artifact still carries the admission it absorbed"
        );
    }

    #[test]
    fn a_merge_absorbing_an_opaque_input_re_emits_the_opaque_form() {
        // The conservative direction. This merge knows the exact index of every
        // guarded body it took from the *exact* inputs, so it could write a
        // list — and a list naming only them would state that the optimized
        // library's bodies are unguarded, which is the one thing its marker
        // denies.
        let opaque = with_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &OPAQUE_PAYLOAD,
        );
        let dir = LibDir::with("mathlib", &opaque);
        let merged = link_against(&forall_program("double"), dir.path())
            .expect("a universal specification links against an optimized library");
        assert_eq!(
            checked_payload(&merged).as_deref(),
            Some(&OPAQUE_PAYLOAD[..])
        );

        // And the exact case is untouched by the widening: with every input
        // naming its own guarded functions, the output names them too.
        let exact_dir = LibDir::with("mathlib", &compile_library(GUARDED_LIB));
        let exact = link_against(&forall_program("double"), exact_dir.path())
            .expect("a universal specification links against a guarded library");
        let payload = checked_payload(&exact).expect("the merged artifact carries the section");
        assert_eq!(
            payload.first().copied(),
            Some(CHECKED_SECTION_VERSION_BYTE),
            "an exact-only merge stays exact: {payload:?}"
        );
        assert!(
            !checked_indices(&exact)
                .expect("the exact form decodes to a list")
                .is_empty(),
            "the exact list names the guarded body it absorbed"
        );
    }

    #[test]
    fn a_library_whose_marker_also_carries_a_list_fails_the_link() {
        // The two wire forms are told apart by their leading version and differ
        // by what follows it. A payload writing both is a producer that
        // disagrees with this decoder about which form it wrote, and reading it
        // as the opaque form would discard the list it went to the trouble of
        // writing — so it is refused rather than guessed at.
        let malformed = with_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &[2, 1, 0],
        );
        let dir = LibDir::with("mathlib", &malformed);
        let error = link_against(&forall_program("double"), dir.path())
            .expect_err("a payload mixing the two forms must be refused");
        assert!(
            error.contains("opaque version 2"),
            "the rejection must name the form it could not read: {error}"
        );
    }

    #[test]
    fn a_guarded_program_with_no_externals_keeps_its_section_through_the_link() {
        // The no-externals fast path returns the main bytes unchanged, which is
        // only correct while "unchanged" includes this section. A path that
        // rebuilt the module and dropped the custom sections it did not name
        // would leave a guarded artifact claiming nothing — and the next link
        // to bind it as a library would read that as "no guard here".
        let source = "pub fn double(x: i32) -> i32 { return checked(x + x); }";
        let arena = parse(source).expect("the program parses");
        let typed = type_check(arena).expect("the program type-checks");
        let main = compile(&typed, "main", CompilationMode::Compile);
        let before = checked_payload(&main).expect("the guarded program carries the section");

        let linked = link_with_options(&main, &[], None, &LinkOptions::default())
            .expect("a program with no externals links")
            .wasm;
        assert_eq!(
            checked_payload(&linked).as_deref(),
            Some(&before[..]),
            "the section crosses the no-externals path byte for byte"
        );
        assert_eq!(linked, main, "the fast path returns the artifact unchanged");
    }

    #[test]
    fn the_proof_translation_reads_nothing_from_the_checked_section() {
        // The section is the linker's alone. A `.v` that varied with it would
        // move the discharge protocol's hash of every guarded module for no
        // proof value, so the claim is stated as an equality rather than left
        // to the translator happening not to look.
        let source = "pub fn twice(x: i32) -> i32 { return checked(x + x); }
                      spec Doubling {
                        fn doubling_is_doubling() forall {
                          let n: i32 = @;
                          assert(twice(n) == twice(n));
                        }
                      }";
        let arena = parse(source).expect("the program parses");
        let typed = type_check(arena).expect("the program type-checks");
        let output = inference_wasm_codegen::codegen(
            &typed,
            "guarded",
            CodegenOptions {
                target: Target::Wasm32,
                mode: CompilationMode::Proof,
                opt_level: OptLevel::O3,
                features: EmitFeatures::default(),
                layout: MemoryLayout::default(),
            },
        )
        .expect("codegen succeeds");

        let with_section = output.wasm().to_vec();
        assert!(
            checked_payload(&with_section).is_some(),
            "the fixture has to carry the section for the comparison to say anything"
        );
        let without_section = without_custom_section(&with_section, CHECKED_SECTION_NAME);
        assert_eq!(checked_payload(&without_section), None);

        let spec_funcs = output.spec_func_indices_by_spec().clone();
        let translate = |wasm: &[u8]| {
            inference::wasm_to_v("guarded", wasm, &spec_funcs, output.hspecs())
                .expect("the module translates")
        };
        assert_eq!(
            translate(&with_section),
            translate(&without_section),
            "the emitted `.v` must not vary with a section the translator never reads"
        );
    }

    #[test]
    fn a_checked_section_naming_a_function_the_module_lacks_is_refused_at_parse_time() {
        // The shape a post-build optimizer leaves behind when it removes
        // functions and carries the unknown custom section through: the list
        // survives and names bodies that are gone. Refused rather than mapped
        // onto whichever function now sits at that index, which would report a
        // guard against a body that has none.
        let spliced = with_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &[CHECKED_SECTION_VERSION_BYTE, 1, 0xC8, 0x01],
        );
        let dir = LibDir::with("mathlib", &spliced);
        let error = link_against(&forall_program("double"), dir.path())
            .expect_err("an index past the function count must be refused");
        assert!(
            error.contains("`inference.checked` section of linked module `mathlib` names \
                            function 200"),
            "the rejection must name the module and the offending index: {error}"
        );
        assert!(
            error.contains("no longer describes the bytes it travels in"),
            "the rejection must say what a stale index means: {error}"
        );
    }

    #[test]
    fn a_module_carrying_the_checked_section_twice_is_refused_at_parse_time() {
        // A last-wins assignment would silently discard the first section,
        // leaving its guarded bodies unlisted and the reachability check blind
        // to exactly the functions it exists to find.
        let doubled = with_appended_custom_section(
            &compile_library(GUARDED_LIB),
            CHECKED_SECTION_NAME,
            &[CHECKED_SECTION_VERSION_BYTE, 1, 0],
        );
        let dir = LibDir::with("mathlib", &doubled);
        let error = link_against(&forall_program("double"), dir.path())
            .expect_err("a second section must be refused");
        assert!(
            error.contains("`mathlib`") && error.contains("inference.checked"),
            "the rejection must name the module and the section: {error}"
        );
        assert!(
            error.contains("more than one inference.checked section"),
            "the rejection must say the section appears more than once: {error}"
        );
        assert!(
            error.contains("guarded functions would be silently dropped"),
            "the rejection must name what a last-wins assignment would lose, which for this \
             section is not an obligation and not a matter of adoption: {error}"
        );
    }

    #[test]
    fn the_codegen_and_linker_copies_of_the_section_identifiers_agree() {
        // Two crates spell this section, and neither imports it from the other:
        // the linker keeps its own name and version rather than depend on the
        // crate whose output it checks. Nothing in the type system holds the two
        // together, so this does.
        //
        // A drift in the *name* is the quiet one. The linker would simply not
        // find the section, read its absence as "no function of this module
        // traps" — the reading that makes a foreign artifact linkable — and
        // accept every merge the section exists to refuse. A drift in the
        // version fails loudly instead, as an unsupported-version rejection.
        assert_eq!(
            CHECKED_SECTION_NAME,
            inference::LINKER_CHECKED_SECTION_NAME,
            "the emitter and the decoder must name one section"
        );
        assert_eq!(
            CHECKED_SECTION_VERSION,
            inference::LINKER_CHECKED_SECTION_VERSION,
            "the emitter and the decoder must agree on the exact form's version"
        );
    }
}

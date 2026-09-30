//! The `inference.bounds_elided` section, from code generation across the
//! static-merge linker.
//!
//! The section records which functions of a module hold an array access
//! emitted without its runtime bounds guard, and its absence records that every
//! dynamic access Inference emitted keeps its guard. Only a build under
//! `BoundsChecks::OmitProven` writes it. The linker rewrites it into the merged
//! index space and re-emits one merged section, so a program that keeps every
//! guard and links a library that did not still says so in the module it ships.
//!
//! As in the `inference.checked` suite, the fixtures are built in memory: what
//! is under test is a pair of modules and the relation between them.

#[cfg(test)]
mod bounds_elided_link_tests {
    use std::path::Path;

    use inf_wasmparser::{BinaryReader, ExternalKind, Parser, Payload};
    use inference::wasm_link::{SearchPath, resolve_external_modules};
    use inference::{LinkOptions, link_with_options, parse, type_check};
    use inference_wasm_codegen::{
        BOUNDS_ELIDED_SECTION_NAME, BOUNDS_ELIDED_SECTION_VERSION, BoundsChecks, CodegenOptions,
        CompilationMode,
    };

    /// A library with two exported functions, each indexing the array its
    /// caller passes with a value it bounded itself, so an `omit-proven` build
    /// drops both guards. The array is the caller's: a library that addressed a
    /// frame of its own could not be merged at all.
    const LIB: &str = "pub fn pick(a: [i32; 4], i: u32) -> i32 { \
                       if i < 4 { return a[i]; } return 0; } \
                       pub fn unused(a: [i32; 4], i: u32) -> i32 { \
                       if i < 4 { return a[i]; } return 0; }";

    /// A program that calls the library's `pick` and indexes an array of its own
    /// with a value it bounded itself.
    const MAIN: &str = "external fn pick(a: [i32; 4], i: u32) -> i32; \
                        use { pick } from mathlib; \
                        pub fn f(i: u32) -> i32 { let a: [i32; 4] = [9, 8, 7, 6]; \
                        let mut r: i32 = 0; if i < 4 { r = a[i]; } return r + pick(a, i); } \
                        pub fn g(x: i32) -> i32 { return x; }";

    /// Compiles `source` the way `infc` does under `policy`: analysis under
    /// the policy, then code generation handed what that analysis proved.
    fn compile(source: &str, module_name: &str, policy: BoundsChecks) -> Vec<u8> {
        let arena = parse(source).expect("the source parses");
        let typed = type_check(arena).expect("the source type-checks");
        let options = CodegenOptions {
            mode: CompilationMode::Compile,
            bounds_checks: policy,
            ..Default::default()
        };
        let analysis = inference_analysis::analyze_with_options(
            &typed,
            inference_analysis::AnalysisOptions {
                bounds_checks: policy,
                ..Default::default()
            },
        )
        .expect("the source passes analysis");
        inference_wasm_codegen::codegen_with_proven_in_bounds(
            &typed,
            module_name,
            options,
            analysis.proven_in_bounds().accesses(),
        )
        .expect("codegen succeeds")
        .wasm()
        .to_vec()
    }

    /// Links `main` (compiled from [`MAIN`]) against a library directory
    /// holding `lib` as `mathlib.wasm`.
    fn link(main: &[u8], lib: &[u8]) -> Result<Vec<u8>, String> {
        let dir = tempfile::tempdir().expect("create a library search directory");
        std::fs::write(dir.path().join("mathlib.wasm"), lib).expect("write the library");
        let typed =
            type_check(parse(MAIN).expect("the program parses")).expect("the program type-checks");
        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(Path::new(dir.path()).to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("the external resolves and validates");
        link_with_options(
            main,
            &externals.module_bytes(),
            Some(&externals.contracts),
            &LinkOptions::default(),
        )
        .map(|output| output.wasm)
        .map_err(|e| e.to_string())
    }

    /// The raw `inference.bounds_elided` payload, or `None` when the module
    /// carries no such section.
    fn record(wasm: &[u8]) -> Option<Vec<u8>> {
        Parser::new(0)
            .parse_all(wasm)
            .filter_map(|payload| match payload.expect("the module parses") {
                Payload::CustomSection(reader) if reader.name() == BOUNDS_ELIDED_SECTION_NAME => {
                    Some(reader.data().to_vec())
                }
                _ => None,
            })
            .next()
    }

    /// The function indices the exact record lists.
    fn listed(wasm: &[u8]) -> Vec<u32> {
        let payload = record(wasm).expect("the module carries the record");
        let mut bytes = BinaryReader::new(&payload, 0);
        assert_eq!(
            bytes.read_var_u32().expect("a version"),
            BOUNDS_ELIDED_SECTION_VERSION,
            "the record is in its exact form"
        );
        let count = bytes.read_var_u32().expect("a count");
        let indices: Vec<u32> = (0..count)
            .map(|_| bytes.read_var_u32().expect("an index"))
            .collect();
        assert_eq!(bytes.bytes_remaining(), 0, "trailing bytes in the payload");
        indices
    }

    /// The function index `wasm` exports under `name`.
    fn exported(wasm: &[u8], name: &str) -> u32 {
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ExportSection(reader) = payload.expect("the module parses") {
                for export in reader {
                    let export = export.expect("an export");
                    if export.name == name && export.kind == ExternalKind::Func {
                        return export.index;
                    }
                }
            }
        }
        panic!("no function is exported as `{name}`");
    }

    /// The emitter and the linker's decoder are hand-synchronised copies, so a
    /// drift in either would have the linker drop the record as an unknown
    /// section.
    #[test]
    fn the_emitter_and_the_linker_name_one_section() {
        assert_eq!(
            BOUNDS_ELIDED_SECTION_NAME,
            inference::LINKER_BOUNDS_ELIDED_SECTION_NAME
        );
        assert_eq!(
            BOUNDS_ELIDED_SECTION_VERSION,
            inference::LINKER_BOUNDS_ELIDED_SECTION_VERSION
        );
        assert_eq!(BOUNDS_ELIDED_SECTION_NAME, "inference.bounds_elided");
    }

    /// A build that keeps every guard carries no record, so its bytes are those
    /// every build produced before the policy existed.
    #[test]
    fn a_build_that_keeps_every_guard_carries_no_record() {
        assert_eq!(
            record(&compile(LIB, "mathlib_impl", BoundsChecks::All)),
            None
        );
    }

    /// An `omit-proven` build lists exactly the functions that lost a guard:
    /// both of the library's, and neither of the program's function that
    /// indexes nothing.
    #[test]
    fn an_omit_proven_build_lists_exactly_the_functions_that_omit_a_guard() {
        let lib = compile(LIB, "mathlib_impl", BoundsChecks::OmitProven);
        assert_eq!(
            listed(&lib),
            vec![exported(&lib, "pick"), exported(&lib, "unused")]
        );

        let main = compile(MAIN, "main", BoundsChecks::OmitProven);
        assert_eq!(listed(&main), vec![exported(&main, "f")]);
    }

    /// The case the record exists for: a program that keeps every guard links a
    /// library that did not. The merged module lists the library body it took
    /// and no other — `unused`, which no closure pulled in, drops out.
    #[test]
    fn a_guarded_program_linking_an_unguarded_library_records_the_library_body() {
        let main = compile(MAIN, "main", BoundsChecks::All);
        let lib = compile(LIB, "mathlib_impl", BoundsChecks::OmitProven);
        let merged = link(&main, &lib).expect("the link succeeds");

        let entries = listed(&merged);
        assert_eq!(entries.len(), 1, "only `pick` was merged: {entries:?}");
        assert!(
            ![exported(&merged, "f"), exported(&merged, "g")].contains(&entries[0]),
            "the program's own functions keep their guards: {entries:?}"
        );
    }

    /// The program's own entries are rewritten into the merged index space: the
    /// import the link resolved is gone, so `f` moves, and the record follows
    /// it rather than naming whichever function now sits at its old index.
    #[test]
    fn the_programs_own_entries_follow_their_functions_through_the_merge() {
        let main = compile(MAIN, "main", BoundsChecks::OmitProven);
        let lib = compile(LIB, "mathlib_impl", BoundsChecks::All);
        assert_eq!(record(&lib), None);
        let merged = link(&main, &lib).expect("the link succeeds");

        assert_ne!(
            exported(&main, "f"),
            exported(&merged, "f"),
            "the resolved import is removed, so `f` moves"
        );
        assert_eq!(listed(&merged), vec![exported(&merged, "f")]);
    }

    /// Appends a custom section named `name` carrying `payload` to `wasm`, as a
    /// producer other than code generation might.
    fn with_appended_record(wasm: &[u8], payload: &[u8]) -> Vec<u8> {
        fn uleb(mut value: usize, out: &mut Vec<u8>) {
            loop {
                let byte = u8::try_from(value & 0x7f).expect("seven bits fit in a byte");
                value >>= 7;
                if value == 0 {
                    out.push(byte);
                    return;
                }
                out.push(byte | 0x80);
            }
        }
        let name = BOUNDS_ELIDED_SECTION_NAME.as_bytes();
        let mut body = Vec::new();
        uleb(name.len(), &mut body);
        body.extend_from_slice(name);
        body.extend_from_slice(payload);
        let mut out = wasm.to_vec();
        out.push(0);
        uleb(body.len(), &mut out);
        out.extend_from_slice(&body);
        out
    }

    /// A library whose record is opaque — the form `infs` writes after a
    /// post-build optimizer renumbered it — can no longer say which of its
    /// bodies omit a guard, so the merged record says the same of the output.
    #[test]
    fn an_opaque_library_record_merges_opaque() {
        let main = compile(MAIN, "main", BoundsChecks::All);
        let lib = with_appended_record(&compile(LIB, "mathlib_impl", BoundsChecks::All), &[2]);
        let merged = link(&main, &lib).expect("the link succeeds");
        assert_eq!(record(&merged), Some(vec![2]));
    }

    /// A record naming a function the module does not have no longer describes
    /// the bytes it travels in, and is refused rather than carried onto
    /// whichever function sits at that index after the merge.
    #[test]
    fn a_record_naming_a_function_the_module_lacks_is_refused() {
        let main = compile(MAIN, "main", BoundsChecks::All);
        let lib = with_appended_record(
            &compile(LIB, "mathlib_impl", BoundsChecks::All),
            &[1, 1, 99],
        );
        let err = link(&main, &lib).expect_err("an out-of-range entry is refused");
        assert!(
            err.contains("`inference.bounds_elided` section") && err.contains("function 99"),
            "{err}"
        );
    }

    /// A second record would drop the first under a last-wins read.
    #[test]
    fn a_duplicated_record_is_refused() {
        let main = compile(MAIN, "main", BoundsChecks::All);
        let lib = compile(LIB, "mathlib_impl", BoundsChecks::OmitProven);
        let doubled = with_appended_record(&lib, &record(&lib).expect("the library records"));
        let err = link(&main, &doubled).expect_err("a duplicated record is refused");
        assert!(
            err.contains("more than one inference.bounds_elided section"),
            "{err}"
        );
    }

    /// Two guarded halves link into a module with no record at all.
    #[test]
    fn a_link_of_guarded_modules_carries_no_record() {
        let main = compile(MAIN, "main", BoundsChecks::All);
        let lib = compile(LIB, "mathlib_impl", BoundsChecks::All);
        assert_eq!(record(&link(&main, &lib).expect("the link succeeds")), None);
    }
}

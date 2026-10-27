//! A linear memory the host supplies rather than the module defines (#512).
//!
//! A build that imports its memory emits `(import "env" "memory" (memory N M))`
//! in place of a Memory section, and, whenever the defined build declares a
//! memory, nothing else about the module changes: memories have an index space
//! of their own, so no function, global or data index moves, and the custom
//! sections that record function indices keep the defined build's bytes. (A
//! program that touches no memory at the default layout is the exception: its
//! defined build declares no memory at all, while the import build still
//! imports one, with the stack pointer and both exports.) That claim is checked
//! four ways, from the bytes up to a running instance.
//!
//! - Section identity. The defined-memory build and the import-mode build of
//!   one program at the same numbers, whenever the defined build declares a
//!   memory, agree on every section but Import and Memory, byte for byte, and
//!   the import-mode Import section is the memory entry followed by the defined
//!   build's entries. This stands in for a golden fixture, which the
//!   whole-corpus SpaceWasm conformance tables would enroll as a conformant
//!   artifact of a mode that target refuses, and it pins more than a golden
//!   would: a golden fixes each build's bytes, this fixes the relation between
//!   the two.
//! - WAT, for the shape a reader sees.
//! - Validation, with a count of memories beside it, because the validator
//!   enables multi-memory and would admit a stray Memory section next to the
//!   import.
//! - Execution under Wasmtime against a memory the test creates, the only
//!   place the host's half of the contract is exercised: which memories a host
//!   may supply, and that the stack, the data segment and the `memory` export
//!   are all the host's memory.

#[cfg(test)]
mod memory_import_tests {
    use crate::utils::codegen_with_options;
    use inf_wasmparser::{
        BinaryReader, DataKind, ExternalKind, Operator, Parser, Payload, TypeRef,
    };
    use inference_wasm_codegen::{
        BOUNDS_ELIDED_SECTION_NAME, BoundsChecks, CHECKED_SECTION_NAME, CodegenOptions,
        MemoryImport, MemoryLayout, MemoryLayoutSource, MemoryRequest,
    };
    use wasmtime::{
        Config, Engine, Extern, Instance, Linker, Memory, MemoryType, Module, SharedMemory, Store,
        WasmParams, WasmResults,
    };

    /// Arrays in a stack frame, one of them written only where it is nonzero,
    /// so `zeros` reads three elements nothing but the frame's zero fill wrote.
    const FRAME_SOURCE: &str = "pub fn read_first() -> i32 { let arr: [i32; 4] = [1, 2, 3, 4]; \
                                return arr[0]; } \
                                pub fn sum() -> i32 { let arr: [i32; 4] = [1, 2, 3, 4]; \
                                return arr[0] + arr[1] + arr[2] + arr[3]; } \
                                pub fn zeros() -> i32 { let z: [i32; 4] = [0, 0, 7, 0]; \
                                return z[0] + z[1] + z[3]; }";

    /// Two structs passed by value to a private callee.
    const STRUCT_PARAMS_SOURCE: &str = "struct Point { x: i32; y: i32; } \
                                        fn span(p: Point, q: Point) -> i32 { \
                                        return (q.x - p.x) * 10 + (q.y - p.y); } \
                                        pub fn run() -> i32 { let p: Point = Point { x: 2, y: 3 }; \
                                        let q: Point = Point { x: 7, y: 9 }; return span(p, q); }";

    /// An array passed to a private callee, and an exported function whose
    /// array parameter is a pointer into memory the caller fills.
    const ARRAY_PARAMS_SOURCE: &str = "fn total(xs: [i32; 4]) -> i32 { \
                                       return xs[0] + xs[1] + xs[2] + xs[3]; } \
                                       pub fn run() -> i32 { let xs: [i32; 4] = [1, 2, 3, 4]; \
                                       return total(xs) * 100 + xs[3]; } \
                                       pub fn pick(a: [i32; 4], i: u32) -> i32 { \
                                       if i < 4 { return a[i]; } return -1; }";

    /// Structs returned through a result pointer, one of them built from the
    /// other.
    const SRET_SOURCE: &str = "struct Pair { a: i32; b: i32; } \
                               fn make(x: i32) -> Pair { return Pair { a: x, b: x * 2 }; } \
                               fn swap(p: Pair) -> Pair { return Pair { a: p.b, b: p.a }; } \
                               pub fn run(x: i32) -> i32 { let p: Pair = make(x); \
                               let q: Pair = swap(p); return p.a * 100 + q.a; }";

    /// An array and a struct constant, so the module carries a data segment
    /// above the stack. `TABLE` is the first constant a body reads, so it opens
    /// the region.
    const CONSTS_SOURCE: &str = "struct Point { x: i32; y: i32; } \
                                 const TABLE: [i32; 4] = [10, -20, 30, 40]; \
                                 const ORIGIN: Point = Point { x: 5, y: -6 }; \
                                 pub fn table_sum() -> i32 { \
                                 return TABLE[0] + TABLE[1] + TABLE[2] + TABLE[3]; } \
                                 pub fn at(i: i32) -> i32 { \
                                 if i >= 0 && i < 4 { return TABLE[i]; } return -1; } \
                                 pub fn origin_y() -> i32 { return ORIGIN.y; }";

    /// `TABLE`'s bytes, little-endian.
    const TABLE_BYTES: [u8; 16] = [
        10, 0, 0, 0, 0xec, 0xff, 0xff, 0xff, 30, 0, 0, 0, 40, 0, 0, 0,
    ];

    /// `ORIGIN`'s bytes, little-endian.
    const ORIGIN_BYTES: [u8; 8] = [5, 0, 0, 0, 0xfa, 0xff, 0xff, 0xff];

    /// Host bindings from two host modules, one of them `env`, the module the
    /// memory is imported from, with a frame so the program touches memory.
    const HOST_SOURCE: &str = "external fn clock_ms() -> i64; \
                               external fn emit(x: i32) -> i32; \
                               use { clock_ms } from host::env; \
                               use { emit } from host::telemetry; \
                               pub fn now() -> i64 { let a: [i64; 2] = [clock_ms(), 7]; \
                               return a[0] + a[1]; } \
                               pub fn report(x: i32) -> i32 { return emit(x) + 1; }";

    /// A dynamic index the range analysis bounds, so a build under
    /// `BoundsChecks::OmitProven` drops its guard and records the function in
    /// `inference.bounds_elided`.
    const BOUNDS_ELIDED_SOURCE: &str = "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [9, 8, 7, 6]; \
                                        let mut r: i32 = 0; if i < 4 { r = a[i]; } return r; }";

    /// Overflow-checked arithmetic, recorded in `inference.checked`, in a
    /// function with a frame.
    const CHECKED_SOURCE: &str = "pub fn double(x: i32) -> i32 { let a: [i32; 2] = [x, x]; \
                                  return a[0] + a[1]; }";

    /// A program that touches no memory at all.
    const MEMORYLESS_SOURCE: &str = "pub fn answer() -> i32 { return 42; }";

    const IMPORT_SECTION: u8 = 2;
    const GLOBAL_SECTION: u8 = 6;
    const EXPORT_SECTION: u8 = 7;
    const MEMORY_SECTION: u8 = 5;
    const DATA_SECTION: u8 = 11;

    /// The numbers a layout is resolved from, built once defined and once
    /// imported.
    #[derive(Debug, Clone, Copy)]
    struct Numbers {
        pages: Option<u32>,
        max_pages: Option<u32>,
        stack_size: Option<u32>,
    }

    /// The layout a build that configures nothing gets: one fixed page.
    const DEFAULT: Numbers = Numbers {
        pages: None,
        max_pages: None,
        stack_size: None,
    };

    /// Two pages growable to eight.
    const GROWABLE: Numbers = Numbers {
        pages: Some(2),
        max_pages: Some(8),
        stack_size: None,
    };

    /// Two fixed pages with a half-page stack, so the stack pointer and the
    /// data base are not the defaults either.
    const SMALL_STACK: Numbers = Numbers {
        pages: Some(2),
        max_pages: None,
        stack_size: Some(32_768),
    };

    const NUMBERS: [Numbers; 3] = [DEFAULT, GROWABLE, SMALL_STACK];

    impl Numbers {
        fn layout(self, import: bool) -> MemoryLayout {
            MemoryLayout::resolve(
                MemoryRequest {
                    pages: self.pages,
                    max_pages: self.max_pages,
                    stack_size: self.stack_size,
                    import,
                },
                MemoryLayoutSource::Flag,
            )
            .expect("the layout is admissible")
        }

        fn is_default(self) -> bool {
            self.layout(false).is_default_memory()
        }
    }

    /// What a case must carry for its row of the matrix to mean anything.
    #[derive(Debug, Clone, Copy)]
    enum Carries {
        DataSegment,
        Custom(&'static str),
        FunctionImports(usize),
    }

    /// One program of the section-identity matrix.
    struct Case {
        name: &'static str,
        source: &'static str,
        bounds_checks: BoundsChecks,
        carries: &'static [Carries],
        touches_memory: bool,
    }

    const CASES: &[Case] = &[
        Case {
            name: "frame-allocating",
            source: FRAME_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[Carries::Custom(CHECKED_SECTION_NAME)],
            touches_memory: true,
        },
        Case {
            name: "struct parameters",
            source: STRUCT_PARAMS_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[],
            touches_memory: true,
        },
        Case {
            name: "array parameters",
            source: ARRAY_PARAMS_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[],
            touches_memory: true,
        },
        Case {
            name: "struct return",
            source: SRET_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[],
            touches_memory: true,
        },
        Case {
            name: "module constants",
            source: CONSTS_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[Carries::DataSegment],
            touches_memory: true,
        },
        Case {
            name: "host imports",
            source: HOST_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[Carries::FunctionImports(2)],
            touches_memory: true,
        },
        Case {
            name: "omitted bounds guard",
            source: BOUNDS_ELIDED_SOURCE,
            bounds_checks: BoundsChecks::OmitProven,
            carries: &[Carries::Custom(BOUNDS_ELIDED_SECTION_NAME)],
            touches_memory: true,
        },
        Case {
            name: "overflow-checked",
            source: CHECKED_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[Carries::Custom(CHECKED_SECTION_NAME)],
            touches_memory: true,
        },
        Case {
            name: "memoryless",
            source: MEMORYLESS_SOURCE,
            bounds_checks: BoundsChecks::All,
            carries: &[],
            touches_memory: false,
        },
    ];

    /// A section's identity: its id, or a custom section's name.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum SectionKey {
        Standard(u8),
        Custom(String),
    }

    type Sections = Vec<(SectionKey, Vec<u8>)>;

    fn build(source: &str, layout: MemoryLayout, bounds_checks: BoundsChecks) -> Vec<u8> {
        codegen_with_options(
            source,
            CodegenOptions {
                layout,
                bounds_checks,
                ..CodegenOptions::default()
            },
        )
        .unwrap_or_else(|e| panic!("the program builds: {e:#}\n{source}"))
        .wasm()
        .to_vec()
    }

    /// Every section of `wasm` in order, with its raw payload: what follows
    /// the section's size, a custom section's name included.
    fn sections(wasm: &[u8]) -> Sections {
        let mut reader = BinaryReader::new(wasm, 0);
        assert_eq!(
            reader.read_bytes(8).expect("a module header"),
            b"\0asm\x01\0\0\0",
            "a core module header"
        );
        let mut found = Vec::new();
        while !reader.eof() {
            let id = reader.read_u8().expect("a section id");
            let size = reader.read_var_u32().expect("a section size");
            let size = usize::try_from(size).expect("a section size fits in usize");
            let payload = reader.read_bytes(size).expect("the section's payload");
            let key = if id == 0 {
                let name = BinaryReader::new(payload, 0)
                    .read_string()
                    .expect("a custom section name");
                SectionKey::Custom(name.to_string())
            } else {
                SectionKey::Standard(id)
            };
            found.push((key, payload.to_vec()));
        }
        found
    }

    fn has_section(sections: &Sections, id: u8) -> bool {
        sections
            .iter()
            .any(|(key, _)| *key == SectionKey::Standard(id))
    }

    fn has_custom_section(sections: &Sections, name: &str) -> bool {
        sections
            .iter()
            .any(|(key, _)| matches!(key, SectionKey::Custom(found) if found == name))
    }

    /// The Import section's entry count and the bytes of its entries, `(0,
    /// [])` for a module that has none.
    fn import_entries(sections: &Sections) -> (u32, Vec<u8>) {
        let Some((_, payload)) = sections
            .iter()
            .find(|(key, _)| *key == SectionKey::Standard(IMPORT_SECTION))
        else {
            return (0, Vec::new());
        };
        let mut reader = BinaryReader::new(payload, 0);
        let count = reader.read_var_u32().expect("an import count");
        let entries = reader
            .read_bytes(reader.bytes_remaining())
            .expect("the import entries");
        (count, entries.to_vec())
    }

    fn write_leb128(out: &mut Vec<u8>, mut value: u32) {
        loop {
            let low = u8::try_from(value & 0x7f).expect("seven bits fit in a byte");
            value >>= 7;
            if value == 0 {
                out.push(low);
                return;
            }
            out.push(low | 0x80);
        }
    }

    /// The binary encoding of `(import "env" "memory" (memory min max))`,
    /// spelled out from the specification rather than taken from an encoder:
    /// each name length-prefixed, the memory import kind `0x02`, then limits
    /// flagged `0x01` for a present maximum.
    fn memory_import_entry(import: MemoryImport) -> Vec<u8> {
        let mut entry = Vec::new();
        for name in ["env", "memory"] {
            entry.push(u8::try_from(name.len()).expect("a short name"));
            entry.extend_from_slice(name.as_bytes());
        }
        entry.extend_from_slice(&[0x02, 0x01]);
        write_leb128(&mut entry, import.minimum);
        write_leb128(&mut entry, import.maximum);
        entry
    }

    /// Every import of `wasm` as `(module, field, type)`, in section order.
    fn imports(wasm: &[u8]) -> Vec<(String, String, TypeRef)> {
        let mut found = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ImportSection(reader) = payload.expect("the module parses") {
                for import in reader {
                    let import = import.expect("an import");
                    found.push((
                        import.module.to_string(),
                        import.name.to_string(),
                        import.ty,
                    ));
                }
            }
        }
        found
    }

    /// Every export of `wasm` as `(name, kind, index)`, in section order.
    fn exports(wasm: &[u8]) -> Vec<(String, ExternalKind, u32)> {
        let mut found = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ExportSection(reader) = payload.expect("the module parses") {
                for export in reader {
                    let export = export.expect("an export");
                    found.push((export.name.to_string(), export.kind, export.index));
                }
            }
        }
        found
    }

    /// The structured form of the import [`memory_import_entry`] encodes.
    fn memory_import_ref(import: MemoryImport) -> (String, String, TypeRef) {
        (
            "env".to_string(),
            "memory".to_string(),
            TypeRef::Memory(inf_wasmparser::MemoryType {
                memory64: false,
                shared: false,
                initial: u64::from(import.minimum),
                maximum: Some(u64::from(import.maximum)),
                page_size_log2: None,
            }),
        )
    }

    /// How many memories `wasm` imports, and how many it defines.
    fn memory_counts(wasm: &[u8]) -> (usize, usize) {
        let mut imported = 0;
        let mut defined = 0;
        for payload in Parser::new(0).parse_all(wasm) {
            match payload.expect("the module parses") {
                Payload::ImportSection(reader) => {
                    for import in reader {
                        if matches!(import.expect("an import").ty, TypeRef::Memory(_)) {
                            imported += 1;
                        }
                    }
                }
                Payload::MemorySection(reader) => {
                    defined += usize::try_from(reader.count()).expect("a memory count");
                }
                _ => {}
            }
        }
        (imported, defined)
    }

    /// Each active data segment as `(offset, bytes)`.
    fn data_segments(wasm: &[u8]) -> Vec<(usize, Vec<u8>)> {
        let mut segments = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::DataSection(reader) = payload.expect("the module parses") {
                for data in reader {
                    let data = data.expect("a data segment");
                    let DataKind::Active {
                        memory_index,
                        offset_expr,
                    } = data.kind
                    else {
                        panic!("only active segments are emitted");
                    };
                    assert_eq!(memory_index, 0, "the segment writes memory 0");
                    let Operator::I32Const { value } = offset_expr
                        .get_operators_reader()
                        .read()
                        .expect("an offset")
                    else {
                        panic!("the offset is an i32.const");
                    };
                    let offset = usize::try_from(value).expect("a non-negative offset");
                    segments.push((offset, data.data.to_vec()));
                }
            }
        }
        segments
    }

    /// Asserts that `imported` is `defined` with the memory declaration moved
    /// into the Import section and nothing else changed.
    ///
    /// The other sections are compared first, so an import-mode build that
    /// shifted an index fails on the section the shift reached, and only then
    /// the Import section, byte for byte against the memory entry followed by
    /// the defined build's entries, and in the structured form a reader sees.
    fn assert_only_the_memory_declaration_moved(
        label: &str,
        layout: MemoryLayout,
        defined: &[u8],
        imported: &[u8],
    ) {
        let defined_sections = sections(defined);
        let imported_sections = sections(imported);
        let rest = |all: &Sections| -> Sections {
            all.iter()
                .filter(|(key, _)| {
                    !matches!(key, SectionKey::Standard(IMPORT_SECTION | MEMORY_SECTION))
                })
                .cloned()
                .collect()
        };
        let keys = |all: &Sections| -> Vec<SectionKey> {
            all.iter().map(|(key, _)| key.clone()).collect()
        };
        let defined_rest = rest(&defined_sections);
        let imported_rest = rest(&imported_sections);
        assert_eq!(
            keys(&defined_rest),
            keys(&imported_rest),
            "{label}: both builds carry the same other sections in the same order"
        );
        for ((key, defined_bytes), (_, imported_bytes)) in defined_rest.iter().zip(&imported_rest) {
            assert!(
                defined_bytes == imported_bytes,
                "{label}: section {key:?} differs between the defined and the import-mode build"
            );
        }

        assert!(
            has_section(&defined_sections, MEMORY_SECTION),
            "{label}: the defined build declares its memory in a Memory section"
        );
        assert!(
            !has_section(&imported_sections, MEMORY_SECTION),
            "{label}: the import-mode build defines no memory"
        );

        let import = layout
            .memory_import()
            .expect("the import-mode layout imports its memory");
        let (defined_count, defined_entries) = import_entries(&defined_sections);
        let (imported_count, imported_entries) = import_entries(&imported_sections);
        assert_eq!(
            imported_count,
            defined_count + 1,
            "{label}: the import-mode build adds exactly one import"
        );
        let mut expected = memory_import_entry(import);
        expected.extend_from_slice(&defined_entries);
        assert!(
            imported_entries == expected,
            "{label}: the Import section must be `env.memory` followed by the defined build's \
             entries, byte for byte"
        );

        let mut expected = vec![memory_import_ref(import)];
        expected.extend(imports(defined));
        assert_eq!(imports(imported), expected, "{label}");
    }

    /// Asserts that `imported` carries what its case was written to cover.
    fn assert_carries(label: &str, carries: &[Carries], imported: &[u8]) {
        let all = sections(imported);
        for carried in carries {
            match *carried {
                Carries::DataSegment => {
                    assert!(
                        has_section(&all, DATA_SECTION) && !data_segments(imported).is_empty(),
                        "{label}: the build must carry a data segment"
                    );
                }
                Carries::Custom(name) => {
                    assert!(
                        has_custom_section(&all, name),
                        "{label}: the build must carry the `{name}` section"
                    );
                }
                Carries::FunctionImports(count) => {
                    let functions = imports(imported)
                        .iter()
                        .filter(|(_, _, ty)| matches!(ty, TypeRef::Func(_)))
                        .count();
                    assert_eq!(functions, count, "{label}: function imports");
                }
            }
        }
    }

    /// The matrix: every case at every set of numbers, except the memoryless
    /// program at the default ones, which is pinned on its own below because
    /// there the two builds differ by design. The number of rows compared is
    /// asserted, so a filter that skipped more than that one could not leave
    /// the test passing on nothing.
    #[test]
    fn an_imported_memory_moves_nothing_but_the_memory_declaration() {
        cov_mark::check!(wasm_codegen_emit_memory_import);
        let mut compared = 0;
        for case in CASES {
            for numbers in NUMBERS {
                if !case.touches_memory && numbers.is_default() {
                    continue;
                }
                let label = format!("{} at {numbers:?}", case.name);
                let imported_layout = numbers.layout(true);
                let defined = build(case.source, numbers.layout(false), case.bounds_checks);
                let imported = build(case.source, imported_layout, case.bounds_checks);
                assert_only_the_memory_declaration_moved(
                    &label,
                    imported_layout,
                    &defined,
                    &imported,
                );
                assert_carries(&label, case.carries, &imported);
                compared += 1;
            }
        }
        assert_eq!(compared, CASES.len() * NUMBERS.len() - 1);
    }

    /// A program that touches no memory, at the default numbers, is where the
    /// two builds part: the defined build declares no memory, no stack pointer
    /// and neither export, and the import-mode build declares all three, so
    /// what a host must supply does not hang on whether the program uses
    /// memory. Everything the program itself contributes is unchanged.
    #[test]
    fn a_memoryless_program_imports_what_its_default_defined_build_omits() {
        cov_mark::check!(wasm_codegen_emit_memory_import);
        let defined = build(MEMORYLESS_SOURCE, DEFAULT.layout(false), BoundsChecks::All);
        let imported_layout = DEFAULT.layout(true);
        let imported = build(MEMORYLESS_SOURCE, imported_layout, BoundsChecks::All);
        let defined_sections = sections(&defined);
        let imported_sections = sections(&imported);

        for id in [IMPORT_SECTION, MEMORY_SECTION, GLOBAL_SECTION] {
            assert!(
                !has_section(&defined_sections, id),
                "the default defined build declares no section {id}"
            );
        }
        assert!(!has_section(&imported_sections, MEMORY_SECTION));
        assert!(has_section(&imported_sections, GLOBAL_SECTION));
        assert_eq!(
            imports(&imported),
            vec![memory_import_ref(
                imported_layout.memory_import().expect("an imported layout")
            )]
        );
        assert_eq!(
            exports(&defined),
            vec![("answer".to_string(), ExternalKind::Func, 0)]
        );
        assert_eq!(
            exports(&imported),
            vec![
                ("answer".to_string(), ExternalKind::Func, 0),
                ("memory".to_string(), ExternalKind::Memory, 0),
                ("__stack_pointer".to_string(), ExternalKind::Global, 0),
            ]
        );

        let unchanged = |all: &Sections| -> Sections {
            all.iter()
                .filter(|(key, _)| {
                    !matches!(
                        key,
                        SectionKey::Standard(
                            IMPORT_SECTION | MEMORY_SECTION | GLOBAL_SECTION | EXPORT_SECTION
                        )
                    )
                })
                .cloned()
                .collect()
        };
        assert!(
            unchanged(&defined_sections) == unchanged(&imported_sections),
            "the type, function, code and custom sections are the defined build's"
        );
    }

    fn wat_of(wasm: &[u8]) -> String {
        wasmprinter::print_bytes(wasm).expect("the module prints as WAT")
    }

    fn lines_starting<'a>(wat: &'a str, prefix: &str) -> Vec<&'a str> {
        wat.lines()
            .map(str::trim)
            .filter(|line| line.starts_with(prefix))
            .collect()
    }

    /// The memory import is the first import and carries the layout's limits,
    /// the function imports following in the defined build's order.
    #[test]
    fn the_memory_import_is_the_first_import_at_the_layout_s_limits() {
        for (numbers, line) in [
            (DEFAULT, "(import \"env\" \"memory\" (memory (;0;) 1 1))"),
            (GROWABLE, "(import \"env\" \"memory\" (memory (;0;) 2 8))"),
            (
                SMALL_STACK,
                "(import \"env\" \"memory\" (memory (;0;) 2 2))",
            ),
        ] {
            let wat = wat_of(&build(
                FRAME_SOURCE,
                numbers.layout(true),
                BoundsChecks::All,
            ));
            assert_eq!(lines_starting(&wat, "(import "), vec![line], "{wat}");

            let defined = wat_of(&build(
                HOST_SOURCE,
                numbers.layout(false),
                BoundsChecks::All,
            ));
            let imported = wat_of(&build(HOST_SOURCE, numbers.layout(true), BoundsChecks::All));
            let defined_imports = lines_starting(&defined, "(import ");
            let imported_imports = lines_starting(&imported, "(import ");
            assert_eq!(defined_imports.len(), 2, "{defined}");
            assert_eq!(imported_imports.first(), Some(&line), "{imported}");
            assert_eq!(imported_imports[1..], defined_imports[..], "{imported}");
        }
    }

    /// No memory is defined beside the import, and the imported one is
    /// re-exported as `memory` with the stack pointer, as a defined one is.
    #[test]
    fn an_imported_memory_is_exported_and_not_also_defined() {
        for source in [FRAME_SOURCE, CONSTS_SOURCE, HOST_SOURCE, MEMORYLESS_SOURCE] {
            for numbers in [DEFAULT, GROWABLE] {
                let wat = wat_of(&build(source, numbers.layout(true), BoundsChecks::All));
                assert!(
                    lines_starting(&wat, "(memory ").is_empty(),
                    "an imported memory is not also defined:\n{wat}"
                );
                assert!(
                    wat.contains("(export \"memory\" (memory 0))")
                        && wat.contains("(export \"__stack_pointer\" (global 0))"),
                    "the imported memory is exported with the stack pointer:\n{wat}"
                );
            }
            let defined = wat_of(&build(source, GROWABLE.layout(false), BoundsChecks::All));
            assert_eq!(
                lines_starting(&defined, "(memory "),
                vec!["(memory (;0;) 2 8)"],
                "the control defines the memory the import replaces:\n{defined}"
            );
        }
    }

    /// The stack pointer starts where the defined build's does, wherever that
    /// is: at the default stack, below constant data a default stack gave way
    /// to, and at a requested stack size.
    #[test]
    fn the_stack_pointer_starts_where_the_defined_build_s_does() {
        let global = |wat: &str| -> String {
            let globals = lines_starting(wat, "(global ");
            let [line] = globals.as_slice() else {
                panic!("one global, the stack pointer:\n{wat}");
            };
            (*line).to_string()
        };
        for (source, numbers, expected) in [
            (FRAME_SOURCE, DEFAULT, Some(65_536)),
            (FRAME_SOURCE, GROWABLE, Some(65_536)),
            (FRAME_SOURCE, SMALL_STACK, Some(32_768)),
            (CONSTS_SOURCE, DEFAULT, None),
            (CONSTS_SOURCE, GROWABLE, Some(65_536)),
            (CONSTS_SOURCE, SMALL_STACK, Some(32_768)),
            (MEMORYLESS_SOURCE, GROWABLE, Some(65_536)),
        ] {
            let defined = global(&wat_of(&build(
                source,
                numbers.layout(false),
                BoundsChecks::All,
            )));
            let imported = global(&wat_of(&build(
                source,
                numbers.layout(true),
                BoundsChecks::All,
            )));
            assert_eq!(imported, defined, "{numbers:?}");
            match expected {
                Some(stack) => assert_eq!(
                    imported,
                    format!("(global (;0;) (mut i32) i32.const {stack})"),
                    "{numbers:?}"
                ),
                None => assert_ne!(
                    imported, "(global (;0;) (mut i32) i32.const 65536)",
                    "the default stack gives way to the constant data"
                ),
            }
        }
    }

    /// A memoryless program still imports its memory at the default numbers,
    /// with the stack pointer and both exports, where its defined build
    /// declares none of them.
    #[test]
    fn a_memoryless_program_still_imports_its_memory() {
        cov_mark::check!(wasm_codegen_emit_memory_import);
        let wat = wat_of(&build(
            MEMORYLESS_SOURCE,
            DEFAULT.layout(true),
            BoundsChecks::All,
        ));
        assert_eq!(
            lines_starting(&wat, "(import "),
            vec!["(import \"env\" \"memory\" (memory (;0;) 1 1))"],
            "{wat}"
        );
        assert!(
            wat.contains("(global (;0;) (mut i32) i32.const 65536)")
                && wat.contains("(export \"memory\" (memory 0))")
                && wat.contains("(export \"__stack_pointer\" (global 0))"),
            "{wat}"
        );
        let defined = wat_of(&build(
            MEMORYLESS_SOURCE,
            DEFAULT.layout(false),
            BoundsChecks::All,
        ));
        assert!(
            !defined.contains("(import ")
                && !defined.contains("(memory ")
                && !defined.contains("(global ")
                && !defined.contains("(export \"memory\""),
            "{defined}"
        );
    }

    /// Every import-mode build of the matrix validates, and holds exactly one
    /// memory, the imported one. The count is asserted beside the validation
    /// because the validator alone would admit a defined memory next to it.
    #[test]
    fn every_import_mode_build_validates_with_exactly_one_memory() {
        for case in CASES {
            for numbers in NUMBERS {
                let label = format!("{} at {numbers:?}", case.name);
                let imported = build(case.source, numbers.layout(true), case.bounds_checks);
                inf_wasmparser::validate(&imported)
                    .unwrap_or_else(|e| panic!("{label}: the import-mode build is invalid: {e}"));
                assert_eq!(memory_counts(&imported), (1, 0), "{label}");

                let defined = build(case.source, numbers.layout(false), case.bounds_checks);
                let declared = usize::from(case.touches_memory || !numbers.is_default());
                assert_eq!(memory_counts(&defined), (0, declared), "{label}");
            }
        }
    }

    /// Why the count above is needed: the validator enables multi-memory, so
    /// a module defining a memory beside the import is valid to it.
    #[test]
    fn the_validator_admits_a_defined_memory_beside_an_imported_one() {
        let wasm = wat::parse_str(r#"(module (import "env" "memory" (memory 1 1)) (memory 1 1))"#)
            .expect("the module assembles");
        assert!(
            inf_wasmparser::validate(&wasm).is_ok(),
            "multi-memory is enabled, so validation cannot catch a stray Memory section"
        );
        assert_eq!(memory_counts(&wasm), (1, 1));
    }

    /// An import-mode module instantiated against a memory the host made.
    struct Hosted {
        store: Store<()>,
        memory: Memory,
        instance: Instance,
    }

    impl Hosted {
        fn call<P: WasmParams, R: WasmResults>(&mut self, name: &str, params: P) -> R {
            self.instance
                .get_typed_func::<P, R>(&mut self.store, name)
                .unwrap_or_else(|e| panic!("`{name}` is exported with that signature: {e}"))
                .call(&mut self.store, params)
                .unwrap_or_else(|e| panic!("`{name}` runs: {e:#}"))
        }

        /// Calls an `i32`-returning export with no argument or with one.
        fn call_i32(&mut self, name: &str, arg: Option<i32>) -> i32 {
            match arg {
                Some(arg) => self.call::<i32, i32>(name, arg),
                None => self.call::<(), i32>(name, ()),
            }
        }
    }

    /// A memory the host creates, every byte set to `fill` before the module
    /// sees it.
    fn host_memory(store: &mut Store<()>, minimum: u32, maximum: Option<u32>, fill: u8) -> Memory {
        let memory = Memory::new(&mut *store, MemoryType::new(minimum, maximum))
            .unwrap_or_else(|e| panic!("the host creates the memory: {e:#}"));
        memory.data_mut(&mut *store).fill(fill);
        memory
    }

    /// Instantiates `wasm` with `memory` as `env.memory`, beside the host
    /// functions [`HOST_SOURCE`] binds, which a module that imports none of
    /// them leaves unused.
    fn try_instantiate(
        engine: &Engine,
        store: &mut Store<()>,
        wasm: &[u8],
        memory: impl Into<Extern>,
    ) -> wasmtime::Result<Instance> {
        let module = Module::new(engine, wasm).unwrap_or_else(|e| panic!("module rejected: {e}"));
        let mut linker = Linker::new(engine);
        linker
            .define(&*store, "env", "memory", memory)
            .expect("the host defines `env.memory`");
        linker
            .func_wrap("env", "clock_ms", || 1_234_i64)
            .expect("the host defines `env.clock_ms`");
        linker
            .func_wrap("telemetry", "emit", |x: i32| x * 3)
            .expect("the host defines `telemetry.emit`");
        linker.instantiate(&mut *store, &module)
    }

    /// Instantiates `wasm` against a fresh host memory of the given limits.
    fn host(wasm: &[u8], minimum: u32, maximum: Option<u32>, fill: u8) -> Hosted {
        let engine = Engine::default();
        let mut store = Store::new(&engine, ());
        let memory = host_memory(&mut store, minimum, maximum, fill);
        let instance = try_instantiate(&engine, &mut store, wasm, memory)
            .unwrap_or_else(|e| panic!("the module instantiates in the host's memory: {e:#}"));
        Hosted {
            store,
            memory,
            instance,
        }
    }

    /// Instantiates `wasm` against the memory its import declares, exactly.
    fn host_as_declared(wasm: &[u8], layout: MemoryLayout, fill: u8) -> Hosted {
        host(wasm, layout.pages(), Some(layout.max_pages()), fill)
    }

    /// Runs `name` in a defined-memory build, which needs no host, as the
    /// reference an import-mode run must match.
    fn run_defined(wasm: &[u8], name: &str, arg: Option<i32>) -> i32 {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).unwrap_or_else(|e| panic!("module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("the defined build instantiates alone: {e:#}"));
        match arg {
            Some(arg) => instance
                .get_typed_func::<i32, i32>(&mut store, name)
                .and_then(|f| f.call(&mut store, arg)),
            None => instance
                .get_typed_func::<(), i32>(&mut store, name)
                .and_then(|f| f.call(&mut store, ())),
        }
        .unwrap_or_else(|e| panic!("`{name}` runs in the defined build: {e:#}"))
    }

    /// Every runnable call: source, export, argument, expected result.
    const RUNS: &[(&str, &str, Option<i32>, i32)] = &[
        (FRAME_SOURCE, "read_first", None, 1),
        (FRAME_SOURCE, "sum", None, 10),
        (FRAME_SOURCE, "zeros", None, 0),
        (STRUCT_PARAMS_SOURCE, "run", None, 56),
        (ARRAY_PARAMS_SOURCE, "run", None, 1004),
        (SRET_SOURCE, "run", Some(5), 510),
        (CONSTS_SOURCE, "table_sum", None, 60),
        (CONSTS_SOURCE, "at", Some(1), -20),
        (CONSTS_SOURCE, "at", Some(4), -1),
        (CONSTS_SOURCE, "origin_y", None, -6),
        (CHECKED_SOURCE, "double", Some(21), 42),
        (MEMORYLESS_SOURCE, "answer", None, 42),
    ];

    /// Every program runs in a memory the host supplies at exactly the
    /// import's limits, and returns what its defined build returns.
    #[test]
    fn programs_run_in_a_memory_the_host_supplies() {
        for numbers in NUMBERS {
            for &(source, name, arg, expected) in RUNS {
                let layout = numbers.layout(true);
                let wasm = build(source, layout, BoundsChecks::All);
                let mut hosted = host_as_declared(&wasm, layout, 0);
                let defined = build(source, numbers.layout(false), BoundsChecks::All);
                assert_eq!(
                    hosted.call_i32(name, arg),
                    expected,
                    "`{name}` at {numbers:?}"
                );
                assert_eq!(
                    run_defined(&defined, name, arg),
                    expected,
                    "`{name}` defined"
                );
            }
        }
    }

    /// The omitted-guard build runs too: the access the analysis proved in
    /// bounds reads the right element without its guard.
    #[test]
    fn an_omitted_guard_build_runs_in_a_memory_the_host_supplies() {
        let layout = GROWABLE.layout(true);
        let wasm = build(BOUNDS_ELIDED_SOURCE, layout, BoundsChecks::OmitProven);
        let mut hosted = host_as_declared(&wasm, layout, 0);
        assert_eq!(hosted.call::<i32, i32>("f", 2), 7);
        assert_eq!(hosted.call::<i32, i32>("f", 9), 0);
    }

    /// A host memory full of `0xAA` gives the results a zeroed one does: the
    /// module writes every byte it reads first, its frames through their zero
    /// fill and its constants through the data segment.
    #[test]
    fn a_pre_filled_host_memory_gives_the_results_a_zeroed_one_does() {
        for numbers in [DEFAULT, GROWABLE] {
            for &(source, name, arg, expected) in RUNS {
                let layout = numbers.layout(true);
                let wasm = build(source, layout, BoundsChecks::All);
                let zeroed = host_as_declared(&wasm, layout, 0x00).call_i32(name, arg);
                let filled = host_as_declared(&wasm, layout, 0xAA).call_i32(name, arg);
                assert_eq!(
                    zeroed, expected,
                    "`{name}` in a zeroed memory at {numbers:?}"
                );
                assert_eq!(filled, expected, "`{name}` in a 0xAA memory at {numbers:?}");
            }
        }
    }

    /// The stack lives in the host's memory: the frame `read_first` built is
    /// there, below the stack top, after the call returns.
    #[test]
    fn the_stack_frames_live_in_the_host_s_memory() {
        let layout = DEFAULT.layout(true);
        let wasm = build(FRAME_SOURCE, layout, BoundsChecks::All);
        let mut hosted = host_as_declared(&wasm, layout, 0xAA);
        assert_eq!(hosted.call::<(), i32>("read_first", ()), 1);
        let frame: [u8; 16] = [1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0];
        let stack_top = usize::try_from(layout.stack_size()).expect("the stack size fits");
        let stack = &hosted.memory.data(&hosted.store)[..stack_top];
        assert!(
            stack.windows(frame.len()).any(|window| window == frame),
            "the array `read_first` wrote must be in the host's memory below the stack top"
        );
    }

    /// The data segment writes the constants into the host's memory at the
    /// layout's data base, where the host reads them through its own handle,
    /// whatever the memory held before.
    #[test]
    fn module_constants_are_written_into_the_host_s_memory() {
        cov_mark::check!(wasm_codegen_emit_data_section);
        for numbers in NUMBERS {
            let layout = numbers.layout(true);
            let wasm = build(CONSTS_SOURCE, layout, BoundsChecks::All);
            let segments = data_segments(&wasm);
            let [(offset, bytes)] = segments.as_slice() else {
                panic!("exactly one data segment, got {}", segments.len());
            };
            let placed = layout
                .with_static_data(u64::try_from(bytes.len()).expect("a data size"))
                .expect("the constants fit the layout");
            assert_eq!(
                *offset,
                usize::try_from(placed.data_base()).expect("the data base fits"),
                "the segment starts at the layout's data base at {numbers:?}"
            );
            assert_eq!(&bytes[..16], TABLE_BYTES, "TABLE opens the region");
            assert!(
                bytes
                    .windows(ORIGIN_BYTES.len())
                    .any(|window| window == ORIGIN_BYTES),
                "the region holds ORIGIN"
            );
            for fill in [0x00, 0xAA] {
                let mut hosted = host_as_declared(&wasm, layout, fill);
                let seen =
                    hosted.memory.data(&hosted.store)[*offset..*offset + bytes.len()].to_vec();
                assert_eq!(
                    seen, *bytes,
                    "the host reads the constants at the data base at {numbers:?}, fill {fill:#x}"
                );
                assert_eq!(hosted.call::<(), i32>("table_sum", ()), 60);
                assert_eq!(hosted.call::<i32, i32>("at", 2), 30);
                assert_eq!(hosted.call::<(), i32>("origin_y", ()), -6);
            }
        }
    }

    /// The module's `memory` export is the host's memory: what one handle
    /// writes the other reads, what the host writes the module reads, and a
    /// growth through the host's handle is seen through the export.
    #[test]
    fn the_module_s_memory_export_is_the_host_s_memory() {
        let layout = GROWABLE.layout(true);
        let wasm = build(ARRAY_PARAMS_SOURCE, layout, BoundsChecks::All);
        let mut hosted = host_as_declared(&wasm, layout, 0);
        let export = hosted
            .instance
            .get_memory(&mut hosted.store, "memory")
            .expect("the module exports its memory");

        hosted
            .memory
            .write(&mut hosted.store, 70_000, b"from the host")
            .expect("the host writes its memory");
        let mut seen = [0_u8; 13];
        export
            .read(&hosted.store, 70_000, &mut seen)
            .expect("the export reads");
        assert_eq!(&seen, b"from the host");

        export
            .write(&mut hosted.store, 70_100, b"through the export")
            .expect("the export writes");
        let mut seen = [0_u8; 18];
        hosted
            .memory
            .read(&hosted.store, 70_100, &mut seen)
            .expect("the host reads its memory");
        assert_eq!(&seen, b"through the export");

        let array: Vec<u8> = [11_i32, 22, 33, 44]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        hosted
            .memory
            .write(&mut hosted.store, 80_000, &array)
            .expect("the host writes the array");
        assert_eq!(hosted.call::<(i32, i32), i32>("pick", (80_000, 2)), 33);

        assert_eq!(export.size(&hosted.store), 2);
        assert_eq!(
            hosted
                .memory
                .grow(&mut hosted.store, 1)
                .expect("growth within the maximum"),
            2
        );
        assert_eq!(
            export.size(&hosted.store),
            3,
            "the export sees the host's growth"
        );
        assert!(
            hosted.memory.grow(&mut hosted.store, 6).is_err(),
            "the declared maximum caps the host's memory"
        );
        assert_eq!(export.size(&hosted.store), 3);
    }

    /// A host memory whose limits the import does not admit is refused at
    /// instantiation: one that could grow past the declared maximum, or has
    /// none, which the overflow trap's headroom cannot allow, and one smaller
    /// than the minimum the stack and the data are placed in.
    #[test]
    fn a_host_memory_the_import_does_not_admit_fails_to_instantiate() {
        for (numbers, minimum, maximum, what) in [
            (GROWABLE, 2, Some(9), "a larger maximum"),
            (GROWABLE, 2, None, "no maximum"),
            (GROWABLE, 1, Some(8), "fewer pages than the minimum"),
            (DEFAULT, 1, Some(2), "a maximum above a fixed memory's"),
            (DEFAULT, 1, None, "no maximum for a fixed memory"),
            (
                DEFAULT,
                2,
                Some(2),
                "a maximum above a fixed memory's, at more pages",
            ),
        ] {
            let wasm = build(FRAME_SOURCE, numbers.layout(true), BoundsChecks::All);
            let engine = Engine::default();
            let mut store = Store::new(&engine, ());
            let memory = host_memory(&mut store, minimum, maximum, 0);
            let err = try_instantiate(&engine, &mut store, &wasm, memory)
                .expect_err(&format!("a host memory with {what} must be refused"));
            assert!(
                format!("{err:#}").contains("incompatible import type for `env::memory`"),
                "{what}: {err:#}"
            );
        }
    }

    /// A host that supplies no memory cannot instantiate the module at all:
    /// the import is a requirement, not an option.
    #[test]
    fn a_host_without_a_memory_fails_to_instantiate() {
        let wasm = build(MEMORYLESS_SOURCE, DEFAULT.layout(true), BoundsChecks::All);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).expect("the module compiles");
        let mut store = Store::new(&engine, ());
        let err = Linker::new(&engine)
            .instantiate(&mut store, &module)
            .expect_err("`env.memory` is required");
        assert!(format!("{err:#}").contains("env::memory"), "{err:#}");
    }

    /// A shared host memory is refused, the import being unshared. The engine
    /// is configured to create one, and the same engine instantiates the
    /// module against an unshared memory of the same limits, so the refusal is
    /// about the memory and not the configuration.
    #[test]
    fn a_shared_host_memory_fails_to_instantiate() {
        let mut config = Config::new();
        config.wasm_threads(true);
        config.shared_memory(true);
        let engine = Engine::new(&config).expect("the engine admits shared memories");
        let layout = GROWABLE.layout(true);
        let wasm = build(FRAME_SOURCE, layout, BoundsChecks::All);

        let shared = SharedMemory::new(&engine, MemoryType::shared(2, 8))
            .expect("the host creates a shared memory");
        let mut store = Store::new(&engine, ());
        let err = try_instantiate(&engine, &mut store, &wasm, shared)
            .expect_err("a shared memory must be refused");
        assert!(
            format!("{err:#}").contains("incompatible import type for `env::memory`"),
            "{err:#}"
        );

        let mut store = Store::new(&engine, ());
        let memory = host_memory(&mut store, 2, Some(8), 0);
        let instance = try_instantiate(&engine, &mut store, &wasm, memory)
            .unwrap_or_else(|e| panic!("the same engine admits an unshared memory: {e:#}"));
        let read_first = instance
            .get_typed_func::<(), i32>(&mut store, "read_first")
            .expect("`read_first` is exported");
        assert_eq!(read_first.call(&mut store, ()).expect("it runs"), 1);
    }

    /// A host memory the import admits other than the declared one: more
    /// pages up front within the maximum, or a smaller maximum, instantiates
    /// and runs, the export reporting the host's size.
    #[test]
    fn a_host_memory_within_the_import_s_limits_instantiates() {
        let layout = GROWABLE.layout(true);
        for (minimum, maximum) in [(3, Some(8)), (8, Some(8)), (2, Some(4)), (2, Some(2))] {
            for source in [FRAME_SOURCE, CONSTS_SOURCE] {
                let wasm = build(source, layout, BoundsChecks::All);
                let mut hosted = host(&wasm, minimum, maximum, 0xAA);
                let export = hosted
                    .instance
                    .get_memory(&mut hosted.store, "memory")
                    .expect("the module exports its memory");
                assert_eq!(export.size(&hosted.store), u64::from(minimum));
                if source == FRAME_SOURCE {
                    assert_eq!(hosted.call::<(), i32>("sum", ()), 10);
                } else {
                    assert_eq!(hosted.call::<(), i32>("table_sum", ()), 60);
                }
            }
        }
    }

    /// A program binding host functions from `env` and another module runs
    /// with the memory and the functions all supplied, the memory import ahead
    /// of the function imports.
    #[test]
    fn a_host_import_program_runs_beside_the_memory_import() {
        cov_mark::check!(wasm_codegen_emit_import_section);
        for numbers in [DEFAULT, GROWABLE] {
            let layout = numbers.layout(true);
            let wasm = build(HOST_SOURCE, layout, BoundsChecks::All);
            let found = imports(&wasm);
            let names: Vec<(&str, &str)> = found
                .iter()
                .map(|(module, field, _)| (module.as_str(), field.as_str()))
                .collect();
            assert_eq!(names.first(), Some(&("env", "memory")));
            assert!(names.contains(&("env", "clock_ms")) && names.contains(&("telemetry", "emit")));
            let mut hosted = host_as_declared(&wasm, layout, 0xAA);
            assert_eq!(hosted.call::<(), i64>("now", ()), 1_241);
            assert_eq!(hosted.call::<i32, i32>("report", 5), 16);
        }
    }
}

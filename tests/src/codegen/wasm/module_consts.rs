//! Module-scope constants, end to end: computed by the type checker, emitted by
//! code generation, and read by the running module (#211).
//!
//! A scalar constant is an immediate at every use, so what can go wrong is the
//! value — the evaluator's arithmetic, or the immediate's representation for a
//! narrow or unsigned type. An array or struct constant is bytes of the static
//! data region above the stack, so what can go wrong is the placement — a byte
//! written at the wrong offset or width, a region that overlaps the stack, a
//! read through the wrong address — or the protection: a binding, a parameter
//! or a returned value that aliases the region instead of copying it, through
//! which the program could change a constant.
//!
//! Every function below therefore reads values back through the module, at both
//! instruction levels (the WebAssembly 1.0 copies and the `memory.copy` a bulk
//! memory build emits), and the module's own sections are checked for the one
//! data segment and the stack it leaves.

#[cfg(test)]
mod module_consts_tests {
    use crate::utils::{
        codegen_output_no_analysis, wasm_codegen, wasm_codegen_multi_file,
        wasm_codegen_with_features, wasm_codegen_with_layout,
    };
    use inf_wasmparser::{DataKind, Operator, Parser, Payload};
    use inference_wasm_codegen::{EmitFeatures, MemoryLayout, MemoryLayoutSource, MemoryRequest};
    use wasmtime::{Engine, Instance, Module, Store, WasmParams, WasmResults};

    const SOURCE: &str = r#"
enum Color { Red, Green, Blue }

struct Mixed { a: u8; b: i32; c: u16; d: i64; ok: bool; }
struct Pair { x: i16; y: i16; }
struct Holder { xs: [i32; 3]; tag: Color; }

const N: i32 = 4;
const SHIFTED: u32 = 1 << 31;
const NEG8: i8 = -128;
const MAX_U8: u8 = 255;
const MAX_U32: u32 = 4294967295;
const MIN_I64: i64 = -9223372036854775808;
const MAX_U64: u64 = 18446744073709551615;
const WRAPPED: i32 = wrapping(2147483647 + 1);
const QUOTIENT: i32 = -7 / 2;
const REMAINDER: i32 = -7 % 2;
const MASKED: u16 = (65535 ^ 255) & 4080;
const ARITH_SHIFT: i8 = -64 >> 3;
const LOGICAL_SHIFT: u8 = 255 >> 4;
const INVERTED: u8 = ~MAX_U8;
const BIG: bool = N * 1000 > 3999 && !(N == 5);
const FAVORITE: Color = Color::Blue;

const TABLE: [i32; 4] = [10, -20, 30, N * 10];
const BYTES: [i8; 3] = [-1, 127, -128];
const UBYTES: [u8; 3] = [255, 0, 128];
const HALVES: [i16; 2] = [-32768, 32767];
const UHALVES: [u16; 2] = [65535, 1];
const WIDE: [u64; 2] = [MAX_U64, 1];
const FLAGS: [bool; 3] = [true, false, BIG];
const COLORS: [Color; 3] = [Color::Green, FAVORITE, Color::Red];
const MIXED: Mixed = Mixed { d: MIN_I64, ok: true, c: 65534, b: -7, a: 200 };
const PAIRS: [Pair; 3] = [Pair { x: -1, y: 2 }, Pair { x: 3, y: -4 }, Pair { x: 5, y: 6 }];
const HOLDER: Holder = Holder { xs: [7, 8, 9], tag: Color::Green };
const GRID: [[u8; 3]; 2] = [[1, 2, 3], [4, 5, 6]];
const SEVENS: [i16; 1000] = [7; 1000];
const COPIED: [i32; 4] = TABLE;
const PICKED: i32 = TABLE[3] + MIXED.b;
const PICKED16: i16 = PAIRS[1].y * 2;
const ONLY_IN_AN_INITIALIZER: [i64; 512] = [1; 512];
const FROM_THE_UNPLACED: i64 = ONLY_IN_AN_INITIALIZER[511];

fn sum_by_ref(a: [i32; 4]) -> i32 {
    return a[0] + a[1] + a[2] + a[3];
}

fn scribble(mut a: [i32; 4]) -> i32 {
    a[0] = 999;
    return a[0];
}

fn table() -> [i32; 4] {
    return TABLE;
}

fn pair() -> Pair {
    return PAIRS[2];
}

fn whole() -> Mixed {
    return MIXED;
}

pub fn n() -> i32 { return N; }
pub fn shifted() -> u32 { return SHIFTED; }
pub fn neg8() -> i8 { return NEG8; }
pub fn max_u8() -> u8 { return MAX_U8; }
pub fn max_u32() -> u32 { return MAX_U32; }
pub fn min_i64() -> i64 { return MIN_I64; }
pub fn max_u64() -> u64 { return MAX_U64; }
pub fn wrapped() -> i32 { return WRAPPED; }
pub fn quotient() -> i32 { return QUOTIENT; }
pub fn remainder() -> i32 { return REMAINDER; }
pub fn masked() -> u16 { return MASKED; }
pub fn arith_shift() -> i8 { return ARITH_SHIFT; }
pub fn logical_shift() -> u8 { return LOGICAL_SHIFT; }
pub fn inverted() -> u8 { return INVERTED; }
pub fn big() -> bool { return BIG; }
pub fn favorite() -> Color { return FAVORITE; }
pub fn picked() -> i32 { return PICKED; }
pub fn picked16() -> i16 { return PICKED16; }
pub fn from_the_unplaced() -> i64 { return FROM_THE_UNPLACED; }

pub fn table_sum() -> i32 {
    let mut i: i32 = 0;
    let mut acc: i32 = 0;
    loop i < N {
        acc = acc + TABLE[i];
        i = i + 1;
    }
    return acc;
}

pub fn byte_at(i: i32) -> i8 { if i >= 0 && i < 3 { return BYTES[i]; } return 0; }
pub fn ubyte_at(i: i32) -> u8 { if i >= 0 && i < 3 { return UBYTES[i]; } return 0; }
pub fn half_at(i: i32) -> i16 { if i >= 0 && i < 2 { return HALVES[i]; } return 0; }
pub fn uhalf_at(i: i32) -> u16 { if i >= 0 && i < 2 { return UHALVES[i]; } return 0; }
pub fn wide_at(i: i32) -> u64 { if i >= 0 && i < 2 { return WIDE[i]; } return 0; }
pub fn flag_at(i: i32) -> bool { if i >= 0 && i < 3 { return FLAGS[i]; } return false; }
pub fn color_at(i: i32) -> Color { if i >= 0 && i < 3 { return COLORS[i]; } return Color::Red; }
pub fn mixed_a() -> u8 { return MIXED.a; }
pub fn mixed_b() -> i32 { return MIXED.b; }
pub fn mixed_c() -> u16 { return MIXED.c; }
pub fn mixed_d() -> i64 { return MIXED.d; }
pub fn mixed_ok() -> bool { return MIXED.ok; }
pub fn pairs() -> i16 { return PAIRS[0].x * 100 + PAIRS[1].y * 10 + PAIRS[2].x; }
pub fn holder() -> i32 { return HOLDER.xs[2] * 10 + HOLDER.xs[0]; }
pub fn holder_tag() -> Color { return HOLDER.tag; }
pub fn grid() -> u8 { return GRID[1][2] * 10 + GRID[0][0]; }
pub fn sevens() -> i16 { return SEVENS[0] + SEVENS[999]; }
pub fn copied() -> i32 { return COPIED[3]; }

pub fn copy_is_not_an_alias() -> i32 {
    let mut t: [i32; 4] = TABLE;
    t[0] = 1000;
    return t[0] + TABLE[0];
}

pub fn parenthesized_copy_is_not_an_alias() -> i32 {
    let mut t: [i32; 4] = (TABLE);
    t[1] = 5;
    return t[1] + TABLE[1];
}

pub fn struct_copy_is_not_an_alias() -> i32 {
    let mut m: Mixed = MIXED;
    m.b = 1;
    return m.b + MIXED.b;
}

pub fn passed_by_reference() -> i32 { return sum_by_ref(TABLE); }

pub fn a_writing_callee_copies() -> i32 {
    let written: i32 = scribble(TABLE);
    return written + TABLE[0];
}

pub fn returned_array() -> i32 {
    let t: [i32; 4] = table();
    return t[3];
}

pub fn returned_struct() -> i16 {
    let p: Pair = pair();
    return p.x * 10 + p.y;
}

pub fn returned_whole() -> i64 {
    let m: Mixed = whole();
    return m.d;
}

pub fn indexed(i: i32) -> i32 { if i >= 0 && i < N { return TABLE[i]; } return -1; }
"#;

    /// Runs `name` in the module built from [`SOURCE`] at both instruction
    /// levels, asserting they agree, and returns the result.
    fn call<P: WasmParams + Copy, R: WasmResults + PartialEq + std::fmt::Debug>(
        name: &str,
        params: P,
    ) -> R {
        let lowered = run::<P, R>(&wasm_codegen(SOURCE), name, params);
        let bulk = run::<P, R>(
            &wasm_codegen_with_features(SOURCE, EmitFeatures { bulk_memory: true }),
            name,
            params,
        );
        assert_eq!(lowered, bulk, "`{name}` differs between instruction levels");
        lowered
    }

    fn run<P: WasmParams, R: WasmResults>(wasm: &[u8], name: &str, params: P) -> R {
        try_run(wasm, name, params).unwrap_or_else(|e| panic!("`{name}` trapped: {e}"))
    }

    fn try_run<P: WasmParams, R: WasmResults>(
        wasm: &[u8],
        name: &str,
        params: P,
    ) -> wasmtime::Result<R> {
        inf_wasmparser::validate(wasm).unwrap_or_else(|e| panic!("invalid module: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).unwrap_or_else(|e| panic!("module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("module failed to instantiate: {e}"));
        instance
            .get_typed_func::<P, R>(&mut store, name)
            .unwrap_or_else(|e| panic!("`{name}` is not exported with that signature: {e}"))
            .call(&mut store, params)
    }

    /// What a module says about its data: each segment as `(offset, bytes)`,
    /// the `__stack_pointer` initializer, and whether a DataCount section is
    /// present.
    type DataAndStack = (Vec<(i32, Vec<u8>)>, Option<i32>, bool);

    fn data_and_stack(wasm: &[u8]) -> DataAndStack {
        let mut segments = Vec::new();
        let mut stack = None;
        let mut data_count = false;
        for payload in Parser::new(0).parse_all(wasm) {
            match payload.expect("a valid module") {
                Payload::DataSection(reader) => {
                    for data in reader {
                        let data = data.expect("a data segment");
                        let DataKind::Active {
                            memory_index,
                            offset_expr,
                        } = data.kind
                        else {
                            panic!("only active segments are emitted");
                        };
                        assert_eq!(memory_index, 0);
                        let mut ops = offset_expr.get_operators_reader();
                        let Operator::I32Const { value } = ops.read().expect("an offset") else {
                            panic!("the offset is an i32.const");
                        };
                        segments.push((value, data.data.to_vec()));
                    }
                }
                Payload::DataCountSection { .. } => data_count = true,
                Payload::GlobalSection(reader) => {
                    for global in reader {
                        let global = global.expect("a global");
                        if let Ok(Operator::I32Const { value }) =
                            global.init_expr.get_operators_reader().read()
                        {
                            stack = Some(value);
                        }
                    }
                }
                _ => {}
            }
        }
        (segments, stack, data_count)
    }

    #[test]
    fn scalar_constants_are_their_values_at_every_width() {
        cov_mark::check!(wasm_codegen_module_const_scalar_read);
        assert_eq!(call::<(), i32>("n", ()), 4);
        assert_eq!(
            call::<(), i32>("shifted", ()),
            i32::MIN,
            "u32 1 << 31 as its bits"
        );
        assert_eq!(call::<(), i32>("neg8", ()), -128);
        assert_eq!(call::<(), i32>("max_u8", ()), 255, "u8 is zero-extended");
        assert_eq!(call::<(), i32>("max_u32", ()), -1, "u32::MAX as its bits");
        assert_eq!(call::<(), i64>("min_i64", ()), i64::MIN);
        assert_eq!(call::<(), i64>("max_u64", ()), -1, "u64::MAX as its bits");
    }

    /// The evaluator computes what the instruction would: a wrap only where
    /// `wrapping` asks for one, truncating division, arithmetic and logical
    /// shifts by signedness, bitwise operators on the type's bits.
    #[test]
    fn constant_expressions_follow_the_program_s_arithmetic() {
        assert_eq!(call::<(), i32>("wrapped", ()), i32::MIN);
        assert_eq!(call::<(), i32>("quotient", ()), -3);
        assert_eq!(call::<(), i32>("remainder", ()), -1);
        assert_eq!(call::<(), i32>("masked", ()), 0x0f00);
        assert_eq!(call::<(), i32>("arith_shift", ()), -8);
        assert_eq!(call::<(), i32>("logical_shift", ()), 15);
        assert_eq!(call::<(), i32>("inverted", ()), 0);
        assert_eq!(call::<(), i32>("big", ()), 1);
        assert_eq!(call::<(), i32>("favorite", ()), 2, "`Color::Blue` is tag 2");
        assert_eq!(call::<(), i32>("picked", ()), 40 - 7);
        assert_eq!(call::<(), i32>("picked16", ()), -8);
        assert_eq!(call::<(), i64>("from_the_unplaced", ()), 1);
    }

    /// Every element of an array constant of every element width reads back
    /// at a dynamic index, sign- or zero-extended by its type.
    #[test]
    fn array_constants_are_read_from_the_data_region() {
        cov_mark::check!(wasm_codegen_module_const_compound_read);
        assert_eq!(call::<(), i32>("table_sum", ()), 10 - 20 + 30 + 40);
        let at = |name: &str, i: i32| call::<i32, i32>(name, i);
        assert_eq!(
            [at("byte_at", 0), at("byte_at", 1), at("byte_at", 2)],
            [-1, 127, -128]
        );
        assert_eq!(
            [at("ubyte_at", 0), at("ubyte_at", 1), at("ubyte_at", 2)],
            [255, 0, 128]
        );
        assert_eq!([at("half_at", 0), at("half_at", 1)], [-32768, 32767]);
        assert_eq!([at("uhalf_at", 0), at("uhalf_at", 1)], [65535, 1]);
        assert_eq!(
            [
                call::<i32, i64>("wide_at", 0),
                call::<i32, i64>("wide_at", 1)
            ],
            [-1, 1]
        );
        assert_eq!(
            [at("flag_at", 0), at("flag_at", 1), at("flag_at", 2)],
            [1, 0, 1]
        );
        assert_eq!(
            [at("color_at", 0), at("color_at", 1), at("color_at", 2)],
            [1, 2, 0]
        );
        assert_eq!(call::<(), i32>("grid", ()), 61);
        assert_eq!(call::<(), i32>("sevens", ()), 14);
        assert_eq!(call::<(), i32>("copied", ()), 40);
    }

    /// Every field of a struct with every alignment lands at the offset the
    /// frame layout gives it, whatever order the literal wrote the fields in.
    #[test]
    fn struct_constants_lay_out_each_field_at_its_alignment() {
        assert_eq!(call::<(), i32>("mixed_a", ()), 200);
        assert_eq!(call::<(), i32>("mixed_b", ()), -7);
        assert_eq!(call::<(), i32>("mixed_c", ()), 65534);
        assert_eq!(call::<(), i64>("mixed_d", ()), i64::MIN);
        assert_eq!(call::<(), i32>("mixed_ok", ()), 1);
        assert_eq!(call::<(), i32>("pairs", ()), -100 - 40 + 5);
        assert_eq!(call::<(), i32>("holder", ()), 97);
        assert_eq!(call::<(), i32>("holder_tag", ()), 1);
    }

    /// A binding, a parenthesized initializer, a writing callee and a returned
    /// value each get a copy: a write through any of them leaves the constant
    /// as declared.
    #[test]
    fn nothing_writes_through_a_constant() {
        assert_eq!(call::<(), i32>("copy_is_not_an_alias", ()), 1010);
        assert_eq!(
            call::<(), i32>("parenthesized_copy_is_not_an_alias", ()),
            5 - 20
        );
        assert_eq!(call::<(), i32>("struct_copy_is_not_an_alias", ()), 1 - 7);
        assert_eq!(call::<(), i32>("passed_by_reference", ()), 60);
        assert_eq!(call::<(), i32>("a_writing_callee_copies", ()), 999 + 10);
    }

    #[test]
    fn a_constant_is_returned_through_the_result_pointer() {
        assert_eq!(call::<(), i32>("returned_array", ()), 40);
        assert_eq!(call::<(), i32>("returned_struct", ()), 56);
        assert_eq!(call::<(), i64>("returned_whole", ()), i64::MIN);
    }

    /// An index guarded by a module constant is proven in bounds by A056,
    /// which reads the constant's value; and an index nothing guards keeps the
    /// runtime check every array access has — reading the constant in range,
    /// trapping past either end.
    #[test]
    fn a_dynamic_index_into_a_constant_is_bounds_checked() {
        let wasm = wasm_codegen(SOURCE);
        assert_eq!(run::<i32, i32>(&wasm, "indexed", 3), 40);
        assert_eq!(run::<i32, i32>(&wasm, "indexed", 4), -1);

        let unguarded = codegen_output_no_analysis(
            "const T: [i32; 4] = [1, 2, 3, 4];\npub fn at(i: i32) -> i32 { return T[i]; }",
        );
        let wasm = unguarded.wasm();
        assert_eq!(run::<i32, i32>(wasm, "at", 2), 3);
        try_run::<i32, i32>(wasm, "at", 4).expect_err("index 4 of a 4-element constant");
        try_run::<i32, i32>(wasm, "at", -1).expect_err("a negative index");
    }

    /// One active segment holds the whole region, directly above a stack that
    /// gave up exactly what the region needs, rounded to the frame grid; no
    /// DataCount section accompanies it, and a constant only another
    /// constant's initializer reads has no bytes of its own.
    #[test]
    fn the_data_segment_sits_directly_above_the_stack() {
        cov_mark::check!(wasm_codegen_emit_data_section);
        let wasm = wasm_codegen(SOURCE);
        let (segments, stack, data_count) = data_and_stack(&wasm);
        assert!(!data_count, "no DataCount section is emitted");
        let [(offset, bytes)] = segments.as_slice() else {
            panic!("exactly one data segment, got {}", segments.len());
        };
        let stack = stack.expect("a __stack_pointer global");
        assert_eq!(*offset, stack, "the region starts at the top of the stack");
        assert_eq!(stack % 16, 0, "the stack top stays on the frame grid");
        let end = i64::from(*offset) + bytes.len() as i64;
        assert!(end <= 65_536, "the region fits the one default page");
        assert!(
            65_536 - end < 16,
            "the default stack gives up only what the data needs: region ends at {end}"
        );
        // The 512 eight-byte elements only an initializer reads are not here:
        // the region is far smaller than they alone would be.
        assert!(
            bytes.len() < 4_096,
            "the region holds {} bytes",
            bytes.len()
        );
        // TABLE is the first constant a body reads, so it opens the region.
        assert_eq!(&bytes[..8], &[10, 0, 0, 0, 0xec, 0xff, 0xff, 0xff]);
    }

    /// A program whose constants are all scalars places nothing: no data
    /// section, and the module a program without constants would be.
    #[test]
    fn scalar_constants_place_no_data() {
        let with = wasm_codegen("const K: i32 = 6;\npub fn f() -> i32 { return K * 7; }");
        let without = wasm_codegen("pub fn f() -> i32 { return 6 * 7; }");
        let (segments, _, _) = data_and_stack(&with);
        assert!(segments.is_empty());
        assert_eq!(run::<(), i32>(&with, "f", ()), 42);
        assert_eq!(
            with.len(),
            without.len(),
            "an inlined constant costs what the literal does"
        );
    }

    /// A program whose only memory is its constant data still declares a
    /// memory for the segment to live in.
    #[test]
    fn constant_data_alone_declares_a_memory() {
        let wasm = wasm_codegen("const T: [u8; 2] = [9, 8];\npub fn f() -> u8 { return T[1]; }");
        assert_eq!(run::<(), i32>(&wasm, "f", ()), 8);
        let (segments, stack, _) = data_and_stack(&wasm);
        assert_eq!(segments.len(), 1);
        assert_eq!(stack, Some(65_520));
    }

    /// A requested stack is kept, and the data goes above it, inside a memory
    /// the build made large enough.
    #[test]
    fn a_requested_stack_keeps_its_size_with_the_data_above_it() {
        let layout = MemoryLayout::resolve(
            MemoryRequest {
                pages: Some(2),
                max_pages: None,
                stack_size: Some(65_536),
            },
            MemoryLayoutSource::Flag,
        )
        .expect("two pages hold a full-page stack");
        let wasm = wasm_codegen_with_layout(SOURCE, layout);
        let (segments, stack, _) = data_and_stack(&wasm);
        assert_eq!(stack, Some(65_536));
        assert_eq!(segments[0].0, 65_536);
        assert_eq!(run::<(), i32>(&wasm, "table_sum", ()), 60);
    }

    /// Code generation driven without analysis refuses data that does not fit
    /// beside a requested stack, rather than emitting a segment past the end of
    /// memory.
    #[test]
    fn data_beside_a_full_requested_stack_is_refused_without_analysis() {
        let layout = MemoryLayout::resolve(
            MemoryRequest {
                pages: None,
                max_pages: None,
                stack_size: Some(65_536),
            },
            MemoryLayoutSource::Flag,
        )
        .expect("a full-page stack");
        let err = crate::utils::codegen_attempt_with_layout_no_analysis(
            "const T: [i32; 2] = [1, 2];\npub fn f() -> i32 { return T[0]; }",
            layout,
        )
        .expect_err("no room is left for the data");
        let text = format!("{err:#}");
        assert!(text.contains("8 bytes of static data"), "{text}");
        assert!(text.contains("A058"), "{text}");
    }

    /// A frame larger than the whole stack is a refusal, not a panic, when code
    /// generation is driven without the analysis that reports it (A036).
    #[test]
    fn a_frame_larger_than_the_stack_is_refused_without_analysis() {
        cov_mark::check!(wasm_codegen_frame_exceeds_stack);
        let err = std::panic::catch_unwind(|| {
            codegen_output_no_analysis(
                "pub fn f() -> i32 { let a: [i64; 10000] = [1; 10000]; return 0; }",
            )
        });
        let message = match err {
            Ok(_) => panic!("an 80 KB frame cannot fit a 64 KB stack"),
            Err(payload) => payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_default(),
        };
        assert!(
            message.contains("more than the whole 65536-byte shadow stack"),
            "{message}"
        );
        assert!(message.contains("A036"), "{message}");
    }

    /// Constants cross files the way functions do: an item import, a
    /// namespace-qualified path, and a library constant built from another
    /// library constant, each read from the entry file.
    #[test]
    fn constants_are_read_across_files() {
        let entry = "\
use lib::tables::{SQUARES};
use lib::tables;

pub fn imported() -> i32 { return SQUARES[3]; }
pub fn qualified() -> i32 { return tables::LIMIT + tables::SQUARES[2]; }
pub fn absolute() -> i32 { return lib::tables::DOUBLED[1]; }
";
        let tables = "\
pub const LIMIT: i32 = 100;
pub const SQUARES: [i32; 4] = [0, 1, 4, 9];
pub const DOUBLED: [i32; 2] = [SQUARES[1] * 2, SQUARES[3] * 2];
";
        let wasm = wasm_codegen_multi_file(&[(vec![], entry), (vec!["lib", "tables"], tables)]);
        assert_eq!(run::<(), i32>(&wasm, "imported", ()), 9);
        assert_eq!(run::<(), i32>(&wasm, "qualified", ()), 104);
        assert_eq!(run::<(), i32>(&wasm, "absolute", ()), 18);
    }
}

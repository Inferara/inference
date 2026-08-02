//! Golden and execution coverage for the region fill and copy forms that the
//! rest of the corpus never reaches.
//!
//! Whole-region fills and copies are emitted as ordinary loads and stores: a
//! straight-line form for small regions and an index loop for larger ones, with
//! a statically unrolled 4/2/1 tail when the byte length is not a multiple of
//! the eight-byte chunk. Every other codegen fixture allocates frames and passes
//! compounds well under the byte threshold and no longer than sixteen elements,
//! so the looped and tailed forms are invisible to them — a copy loop that ran
//! one iteration short, or a tail that dropped its last byte, would leave the
//! entire suite green.
//!
//! The fixtures here sit on both sides of the threshold and on every tail
//! combination. Two properties make the execution assertions sharp:
//!
//! - An array literal emits no store for a zero-valued element, so a zero read
//!   back from a frame slot was produced by the zero fill and by nothing else.
//! - A callee's frame is zero-filled before a parameter is copied into it, so a
//!   byte the copy fails to move reads back as zero. Probing with a pattern that
//!   is nonzero at every index turns a dropped byte into a failed assertion
//!   rather than a coincidence.
//!
//! Arrays and structs are passed as pointers, so the parameter probes lay their
//! pattern down in the module's linear memory and pass its address. That is also
//! what lets them verify the caller's bytes are untouched after the callee
//! mutates its own copy.

#[cfg(test)]
mod bulk_free_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };
    use wasmtime::{Engine, Instance, Memory, Module, Store, TypedFunc};

    /// Address of the byte pattern the pointer-ABI probes pass to array and
    /// struct parameters.
    ///
    /// Memory is one page and the shadow stack starts at its top and grows down,
    /// so the low end is untouched by any frame these fixtures allocate.
    const PROBE_ADDR: i32 = 1024;

    /// Compiles a fixture, validates it, and asserts it matches its goldens.
    fn assert_golden(test_name: &str) -> Vec<u8> {
        let source_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {source_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid for {test_name}: {e}"));
        let expected_path = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
        actual
    }

    /// Compiles a fixture and instantiates it, without the golden comparison.
    fn instantiate(test_name: &str) -> (Store<()>, Instance) {
        let source_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {source_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module for {test_name}: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate {test_name}: {e}"));
        (store, instance)
    }

    fn memory_of(store: &mut Store<()>, instance: &Instance) -> Memory {
        instance
            .get_memory(&mut *store, "memory")
            .expect("Module should export 'memory'")
    }

    fn stack_pointer(store: &mut Store<()>, instance: &Instance) -> i32 {
        instance
            .get_global(&mut *store, "__stack_pointer")
            .expect("Module should export '__stack_pointer'")
            .get(&mut *store)
            .i32()
            .expect("__stack_pointer should be an i32 global")
    }

    /// Byte offset of [`PROBE_ADDR`] for the host-side memory accessors.
    fn probe_offset() -> usize {
        usize::try_from(PROBE_ADDR).expect("the probe address is positive")
    }

    /// A byte value that is unique across every index these probes use and never
    /// zero.
    ///
    /// Never zero, so a byte the copy fails to move reads back as the callee
    /// frame's zero fill and fails the comparison. Unique, so a byte the copy
    /// moves to the wrong displacement fails it too — 7 and 251 are coprime, and
    /// the longest probed region is shorter than 251 bytes, so no two indices
    /// share a value.
    fn pattern_byte(index: usize) -> u8 {
        u8::try_from((index * 7 + 3) % 251).expect("modulus keeps the value in u8 range") + 1
    }

    /// Writes `len` pattern bytes at [`PROBE_ADDR`] and returns them.
    fn write_pattern(store: &mut Store<()>, memory: &Memory, len: usize) -> Vec<u8> {
        let bytes: Vec<u8> = (0..len).map(pattern_byte).collect();
        memory
            .write(&mut *store, probe_offset(), &bytes)
            .expect("probe pattern should fit in the module's memory");
        bytes
    }

    fn read_probe_region(store: &mut Store<()>, memory: &Memory, len: usize) -> Vec<u8> {
        let mut buffer = vec![0u8; len];
        memory
            .read(&mut *store, probe_offset(), &mut buffer)
            .expect("probe region should be readable");
        buffer
    }

    fn call0<R>(store: &mut Store<()>, instance: &Instance, name: &str) -> R
    where
        R: wasmtime::WasmResults,
    {
        let f: TypedFunc<(), R> = instance
            .get_typed_func(&mut *store, name)
            .unwrap_or_else(|e| panic!("Failed to get '{name}': {e}"));
        f.call(&mut *store, ())
            .unwrap_or_else(|e| panic!("'{name}' failed: {e}"))
    }

    /// Calls a pointer-ABI probe with the pattern address and an element index.
    fn call_probe<R>(
        store: &mut Store<()>,
        probe: &TypedFunc<(i32, i32), R>,
        name: &str,
        index: usize,
    ) -> R
    where
        R: wasmtime::WasmResults,
    {
        let index = i32::try_from(index).expect("probe indices are small");
        probe
            .call(&mut *store, (PROBE_ADDR, index))
            .unwrap_or_else(|e| panic!("{name} at {index} failed: {e}"))
    }

    // Frame zero-fill ---

    #[test]
    fn frame_fill_golden_test() {
        cov_mark::check!(wasm_codegen_frame_fill_unrolled);
        cov_mark::check!(wasm_codegen_frame_fill_loop);
        assert_golden("frame_fill");
    }

    /// Every zero below comes from the prologue fill: the literals store nothing
    /// for their zero elements. `fill_128_unrolled` sits exactly on the byte
    /// threshold, so it is the last frame that stays straight-line while its
    /// neighbours are looped.
    #[test]
    fn frame_fill_execution_test() {
        let (mut store, instance) = instantiate("frame_fill");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i64>(&mut store, &instance, "fill_128_unrolled"),
            3004,
            "a[7] was never stored, so the 128-byte unrolled fill must have zeroed it"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "fill_144_loop"),
            5006,
            "b[9] was never stored, so the 144-byte fill loop must have zeroed it"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "fill_160_loop"),
            7008,
            "c[10] was never stored, so the 160-byte fill loop must have zeroed it"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "fill_320_loop"),
            1002,
            "a 320-byte frame is twenty loop iterations; the interior must be zero"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "fill_multi_slot"),
            9002,
            "one fill covers every slot in the frame, not just the first"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// Extracts one function's WAT text, from its `(func $name ` header to the
    /// closing paren at function indentation.
    fn function_wat(wat: &str, name: &str) -> String {
        let marker = format!("(func ${name} ");
        let start = wat
            .find(&marker)
            .unwrap_or_else(|| panic!("no function ${name} in the printed module"));
        let rest = &wat[start..];
        let end = rest.find("\n  )").map_or(rest.len(), |offset| offset + 4);
        rest[..end].to_string()
    }

    /// The first zero-fill store in a function body, trimmed of indentation.
    fn first_fill_store(function_wat: &str) -> &str {
        function_wat
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("i64.store"))
            .unwrap_or_else(|| panic!("no i64.store in:\n{function_wat}"))
    }

    /// The byte threshold decides the fill form, and both forms touch the
    /// frame's lowest address first.
    ///
    /// A frame exactly at the threshold stays straight-line; one sixteen bytes
    /// larger becomes a loop. Pinning which function got which form is what the
    /// goldens alone cannot say — a threshold that drifted would still produce a
    /// module that runs correctly and matches a regenerated golden.
    ///
    /// The first access of either form carries no offset immediate, and in the
    /// looped form the induction variable starts at zero. That ordering is what
    /// makes a wrapped stack pointer trap before any byte is written, which is
    /// the all-or-nothing behaviour the bulk fill provided through its up-front
    /// bounds check.
    #[test]
    fn frame_fill_form_and_first_access_test() {
        let source_path = get_test_file_path(module_path!(), "frame_fill");
        let source_code =
            std::fs::read_to_string(&source_path).expect("Failed to read frame_fill.inf");
        let wasm = wasm_codegen(&source_code);
        let wat = wasmprinter::print_bytes(&wasm).expect("Failed to print WAT");

        let unrolled = function_wat(&wat, "fill_128_unrolled");
        assert!(
            !unrolled.contains("\n    loop"),
            "a frame exactly at the 128-byte threshold must stay straight-line:\n{unrolled}"
        );
        assert_eq!(
            first_fill_store(&unrolled),
            "i64.store",
            "the unrolled fill must write the frame's lowest address first:\n{unrolled}"
        );

        for name in ["fill_144_loop", "fill_160_loop", "fill_320_loop"] {
            let looped = function_wat(&wat, name);
            assert!(
                looped.contains("\n    loop"),
                "a frame past the 128-byte threshold must use the fill loop:\n{looped}"
            );
            assert_eq!(
                first_fill_store(&looped),
                "i64.store",
                "the fill loop's first store must carry no offset:\n{looped}"
            );
        }
    }

    /// Reading each slot through an index the compiler cannot fold proves the
    /// fill reached every element, not just the ones a constant index names.
    #[test]
    fn frame_fill_every_element_zeroed_test() {
        let (mut store, instance) = instantiate("frame_fill");
        let read: TypedFunc<i32, i64> = instance
            .get_typed_func(&mut store, "fill_read_dynamic")
            .expect("Failed to get 'fill_read_dynamic'");
        for index in 0..20i32 {
            let expected = match index {
                0 => 4,
                19 => 5,
                _ => 0,
            };
            assert_eq!(
                read.call(&mut store, index)
                    .unwrap_or_else(|e| panic!("fill_read_dynamic({index}) failed: {e}")),
                expected,
                "element {index} of a loop-filled frame"
            );
        }
    }

    // Array parameter copies without a tail ---

    /// The copy count is asserted, not just the copy forms.
    ///
    /// Only a parameter the callee writes is copied into a frame of its own, so
    /// every function here that this file's assertions read through has to be
    /// one that writes. Counting the copies is what states that: a fixture whose
    /// parameters drifted back to read-only would still produce a module whose
    /// remaining copy exercises the loop, and every probe below would still pass
    /// while measuring nothing.
    #[test]
    fn array_copy_loop_golden_test() {
        cov_mark::check!(wasm_codegen_memcpy_loop);
        cov_mark::check!(wasm_codegen_memcpy_unrolled);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 5);
        assert_golden("array_copy_loop");
    }

    #[test]
    fn array_copy_loop_execution_test() {
        let (mut store, instance) = instantiate("array_copy_loop");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_sum_ends_17"),
            33,
            "136 bytes is seventeen chunks with no tail"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_sum_ends_20"),
            40,
            "160 bytes is twenty chunks with no tail"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_sum_ends_i32_20"),
            44,
            "80 bytes is past the element threshold but under the byte threshold"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "copy_preserves_zero"),
            0,
            "an element the caller left at zero must arrive as zero"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "value_semantics_20"),
            11029,
            "the callee wrote its own copy, so the caller's array is unchanged"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// Drives the copy through the parameter ABI so every one of the twenty
    /// elements can be read back individually. An eight-byte chunk that ran
    /// short would leave the tail elements reading zero.
    #[test]
    fn array_copy_loop_every_element_survives_test() {
        let (mut store, instance) = instantiate("array_copy_loop");
        let memory = memory_of(&mut store, &instance);
        let bytes = write_pattern(&mut store, &memory, 20 * 8);

        let pick: TypedFunc<(i32, i32), i64> = instance
            .get_typed_func(&mut store, "pick_20")
            .expect("Failed to get 'pick_20'");
        for index in 0..20usize {
            let mut element = [0u8; 8];
            element.copy_from_slice(&bytes[index * 8..index * 8 + 8]);
            let expected = i64::from_le_bytes(element);
            let actual = call_probe(&mut store, &pick, "pick_20", index);
            assert_eq!(actual, expected, "element {index} did not survive the copy");
        }
    }

    #[test]
    fn array_copy_loop_callee_mutation_does_not_leak_test() {
        let (mut store, instance) = instantiate("array_copy_loop");
        let memory = memory_of(&mut store, &instance);
        let bytes = write_pattern(&mut store, &memory, 20 * 8);

        let clobber: TypedFunc<(i32, i32), i64> = instance
            .get_typed_func(&mut store, "clobber_20")
            .expect("Failed to get 'clobber_20'");
        for index in [0usize, 9, 19] {
            assert_eq!(
                call_probe(&mut store, &clobber, "clobber_20", index),
                999,
                "the callee should observe its own write"
            );
        }
        assert_eq!(
            read_probe_region(&mut store, &memory, 20 * 8),
            bytes,
            "the caller's array must be untouched by the callee's writes"
        );
    }

    // Array parameter copies with a tail ---

    /// Seven copies for seven parameters: six tail-width combinations and the
    /// clobber probe. Five of the six exist nowhere else in the corpus, so a
    /// count that fell to one would leave the tail lowering pinned by a single
    /// width while the boundary probe below still passed.
    #[test]
    fn array_copy_tail_golden_test() {
        cov_mark::check!(wasm_codegen_memcpy_loop);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 7);
        assert_golden("array_copy_tail");
    }

    /// Reads back every index in the last two chunks of each region, which is
    /// where the loop hands over to the tail. A tail width dropped from the
    /// descending 4/2/1 sequence, or one applied at the wrong displacement,
    /// changes exactly these bytes.
    #[test]
    fn array_copy_tail_boundary_bytes_survive_test() {
        let (mut store, instance) = instantiate("array_copy_tail");
        let memory = memory_of(&mut store, &instance);

        for (name, len) in [
            ("at_129", 129usize),
            ("at_130", 130),
            ("at_131", 131),
            ("at_135", 135),
        ] {
            let bytes = write_pattern(&mut store, &memory, len);
            let at: TypedFunc<(i32, i32), i32> = instance
                .get_typed_func(&mut store, name)
                .unwrap_or_else(|e| panic!("Failed to get '{name}': {e}"));
            let probes: Vec<usize> = (0..8).chain(len - 16..len).collect();
            for index in probes {
                let actual = call_probe(&mut store, &at, name, index);
                assert_eq!(
                    u8::try_from(actual).expect("a u8 element reads back in u8 range"),
                    bytes[index],
                    "{name}: byte {index} of {len} did not survive the copy"
                );
            }
        }
    }

    /// The same boundary probe for tails whose leading width is four and two
    /// bytes rather than one, reached through wider element types.
    #[test]
    fn array_copy_tail_wide_elements_survive_test() {
        let (mut store, instance) = instantiate("array_copy_tail");
        let memory = memory_of(&mut store, &instance);

        let bytes = write_pattern(&mut store, &memory, 33 * 4);
        let at_u32: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "at_u32_33")
            .expect("Failed to get 'at_u32_33'");
        for index in [0usize, 15, 31, 32] {
            let mut element = [0u8; 4];
            element.copy_from_slice(&bytes[index * 4..index * 4 + 4]);
            let expected = u32::from_le_bytes(element);
            let actual: i32 = call_probe(&mut store, &at_u32, "at_u32_33", index);
            assert_eq!(
                actual.cast_unsigned(),
                expected,
                "u32 element {index} did not survive the four-byte tail"
            );
        }

        let bytes = write_pattern(&mut store, &memory, 67 * 2);
        let at_u16: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "at_u16_67")
            .expect("Failed to get 'at_u16_67'");
        for index in [0usize, 31, 63, 64, 65, 66] {
            let mut element = [0u8; 2];
            element.copy_from_slice(&bytes[index * 2..index * 2 + 2]);
            let expected = u32::from(u16::from_le_bytes(element));
            let actual: i32 = call_probe(&mut store, &at_u16, "at_u16_67", index);
            assert_eq!(
                actual.cast_unsigned(),
                expected,
                "u16 element {index} did not survive the four-plus-two-byte tail"
            );
        }
    }

    #[test]
    fn array_copy_tail_callee_mutation_does_not_leak_test() {
        let (mut store, instance) = instantiate("array_copy_tail");
        let memory = memory_of(&mut store, &instance);
        let bytes = write_pattern(&mut store, &memory, 135);

        let clobber: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "clobber_135")
            .expect("Failed to get 'clobber_135'");
        for index in [0usize, 127, 128, 130, 134] {
            assert_eq!(
                call_probe(&mut store, &clobber, "clobber_135", index),
                255,
                "the callee should observe its own write"
            );
        }
        assert_eq!(
            read_probe_region(&mut store, &memory, 135),
            bytes,
            "writes to a tail-copied parameter must not reach the caller's bytes"
        );
    }

    // Struct parameter copies ---

    /// Five copies for the five struct parameters: both layouts read from both
    /// ends, plus the clobber probe. The two layouts are the point — an array
    /// between two `i64` fields and one between two narrow fields — and a count
    /// that fell to one would leave whichever of them survived standing in for
    /// the other.
    #[test]
    fn struct_copy_loop_golden_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 5);
        cov_mark::check!(wasm_codegen_memcpy_loop);
        assert_golden("struct_copy_loop");
    }

    #[test]
    fn struct_copy_loop_execution_test() {
        let (mut store, instance) = instantiate("struct_copy_loop");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_block_ends"),
            3004,
            "the fields on both sides of the embedded array must arrive intact"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_block_body"),
            41043,
            "the embedded array's first, middle, and last elements must arrive intact"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "block_value_semantics"),
            3004,
            "the callee wrote its own copy, so the caller's struct is unchanged"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_frame_edges"),
            67,
            "the narrow fields at the low and high ends of the layout must arrive intact"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_frame_body_ends"),
            51053,
            "the array between those narrow fields must arrive intact"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    // sret returns ---

    #[test]
    fn sret_big_golden_test() {
        cov_mark::check!(wasm_codegen_memcpy_loop);
        assert_golden("sret_big");
    }

    #[test]
    fn sret_big_execution_test() {
        let (mut store, instance) = instantiate("sret_big");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(call0::<i64>(&mut store, &instance, "read_20_first"), 11);
        assert_eq!(
            call0::<i64>(&mut store, &instance, "read_20_middle"),
            0,
            "an element the callee never stored must survive the return copy as zero"
        );
        assert_eq!(call0::<i64>(&mut store, &instance, "read_20_last"), 29);
        assert_eq!(
            call0::<i64>(&mut store, &instance, "read_block_ends"),
            3004,
            "a returned struct's fields on both sides of its array must arrive intact"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "read_block_body"),
            41043
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "sret_neighbour_intact"),
            11672,
            "the return copy must not write past its length into a neighbouring slot"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    #[test]
    fn sret_big_every_element_survives_test() {
        let (mut store, instance) = instantiate("sret_big");
        let read: TypedFunc<i32, i64> = instance
            .get_typed_func(&mut store, "read_20_dynamic")
            .expect("Failed to get 'read_20_dynamic'");
        for index in 0..20i32 {
            let expected = match index {
                0 => 11,
                19 => 29,
                _ => 0,
            };
            assert_eq!(
                read.call(&mut store, index)
                    .unwrap_or_else(|e| panic!("read_20_dynamic({index}) failed: {e}")),
                expected,
                "element {index} of a returned 160-byte array"
            );
        }
    }

    // Overlap corners ---

    #[test]
    fn overlap_corners_golden_test() {
        cov_mark::check!(wasm_codegen_memcpy_unrolled);
        assert_golden("overlap_corners");
    }

    /// `arr[i] = arr[j]` with `i == j` at runtime is the identical-region case
    /// the forward copy has to handle: every byte's read and write coincide.
    /// The compiler cannot fold either index, so the same code path serves both
    /// the equal and the distinct call.
    #[test]
    fn overlap_corners_element_self_copy_test() {
        let (mut store, instance) = instantiate("overlap_corners");
        let elem: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "elem_copy")
            .expect("Failed to get 'elem_copy'");

        for (index, expected) in [(0i32, 12), (1, 34), (2, 56), (3, 78)] {
            assert_eq!(
                elem.call(&mut store, (index, index))
                    .unwrap_or_else(|e| panic!("elem_copy({index}, {index}) failed: {e}")),
                expected,
                "self-copying element {index} must leave it unchanged"
            );
        }

        assert_eq!(
            elem.call(&mut store, (0, 2))
                .unwrap_or_else(|e| panic!("elem_copy(0, 2) failed: {e}")),
            56,
            "a distinct-index copy must still move the source element"
        );
    }

    #[test]
    fn overlap_corners_element_copy_leaves_neighbours_test() {
        let (mut store, instance) = instantiate("overlap_corners");
        let neighbours: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "elem_copy_neighbours")
            .expect("Failed to get 'elem_copy_neighbours'");

        assert_eq!(
            neighbours
                .call(&mut store, (1, 1))
                .unwrap_or_else(|e| panic!("elem_copy_neighbours(1, 1) failed: {e}")),
            1357,
            "a self-copy must not disturb any other element"
        );
        assert_eq!(
            neighbours
                .call(&mut store, (0, 3))
                .unwrap_or_else(|e| panic!("elem_copy_neighbours(0, 3) failed: {e}")),
            7357,
            "an element copy must write exactly one element"
        );
    }

    #[test]
    fn overlap_corners_self_assignment_execution_test() {
        let (mut store, instance) = instantiate("overlap_corners");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i32>(&mut store, &instance, "field_self_assign"),
            1249,
            "assigning a compound field from itself must leave the struct unchanged"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "whole_self_assign"),
            1234,
            "a whole-array round trip must preserve every element"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "method_result_into_other_field"),
            1212,
            "a compound returned by an immutable-self method must land intact in a \
             sibling field of the receiver it was read from"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "method_result_into_source_field"),
            5678,
            "...and routing it back into the field it came from must be a no-op"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }
}

/// Regeneration helpers for the goldens in this file, gated behind `#[ignore]`.
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::bulk_free::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn fixture_dir(test_name: &str) -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("bulk_free")
            .join(test_name)
    }

    fn regenerate(test_name: &str) {
        let dir = fixture_dir(test_name);
        let source_path = dir.join(format!("{test_name}.inf"));
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", source_path.display()));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid for {test_name}: {e}"));
        let wasm_path = dir.join(format!("{test_name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, test_name);
    }

    #[test]
    #[ignore]
    fn regenerate_frame_fill_wasm() {
        regenerate("frame_fill");
    }

    #[test]
    #[ignore]
    fn regenerate_array_copy_loop_wasm() {
        regenerate("array_copy_loop");
    }

    #[test]
    #[ignore]
    fn regenerate_array_copy_tail_wasm() {
        regenerate("array_copy_tail");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_copy_loop_wasm() {
        regenerate("struct_copy_loop");
    }

    #[test]
    #[ignore]
    fn regenerate_sret_big_wasm() {
        regenerate("sret_big");
    }

    #[test]
    #[ignore]
    fn regenerate_overlap_corners_wasm() {
        regenerate("overlap_corners");
    }
}

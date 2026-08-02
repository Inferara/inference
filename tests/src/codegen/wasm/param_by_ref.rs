//! Golden and execution coverage for compound parameters that are passed by
//! reference.
//!
//! A compound parameter arrives as an address into the caller's memory and used
//! to be copied into the callee's own frame on entry, unconditionally. Only a
//! parameter something can *write* needs that copy, and a callee's region can be
//! written in exactly two ways: an assignment rooted at the parameter, and the
//! parameter reaching an `external fn` argument, whose foreign body shares the
//! same linear memory. A parameter that does neither now gets no frame slot, no
//! entry copy and no rebinding — and when nothing else in the function needs a
//! frame, the prologue, the epilogue and the `__stack_pointer` write go with it.
//!
//! The family is arranged around two questions the goldens alone cannot answer.
//!
//! **Which decision was taken.** Bytes that match a regenerated golden are
//! produced equally by a gate that fired for the right reason and by one that
//! stopped firing altogether, so every fixture states its decision with
//! `check_count!` in both directions: the by-reference count, the write count,
//! the escape count, and the number of entry copies actually emitted.
//!
//! **Whether the aliasing it creates is safe.** Skipping the copy lets two
//! parameters — or a parameter and a receiver, or a parameter and a return
//! destination — name overlapping regions, which no previous program state
//! could reach. That cannot be established by reading the emitter, so each
//! aliasing shape is compiled *and executed* under Wasmtime with assertions on
//! the return value, on the caller's own bytes afterwards, and on
//! `__stack_pointer` being restored.
//!
//! Compounds are passed as pointers, so the probes that need to inspect the
//! caller's memory lay a byte pattern down in linear memory and pass its
//! address. The pattern is never zero and unique per index, so a byte read from
//! the wrong place, or one a copy failed to move, fails the comparison rather
//! than coinciding with the frame's zero fill.

#[cfg(test)]
mod param_by_ref_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, codegen_output_with_mode,
        get_test_file_path, get_test_wasm_path, wasm_codegen,
    };
    use inference_wasm_codegen::CompilationMode;
    use wasmtime::{Engine, Instance, Memory, Module, Store, TypedFunc};

    /// Address of the byte pattern the pointer-ABI probes pass to array and
    /// struct parameters.
    ///
    /// Memory is one page and the shadow stack starts at its top and grows down,
    /// so the low end is untouched by any frame these fixtures allocate.
    const PROBE_ADDR: i32 = 1024;

    fn fixture_source(test_name: &str) -> String {
        let source_path = get_test_file_path(module_path!(), test_name);
        std::fs::read_to_string(&source_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {source_path:?}"))
    }

    /// Compiles a fixture, validates it, and asserts it matches both goldens.
    ///
    /// The three static tiers in one place: an independent parser accepts the
    /// module, its bytes match the checked-in `.wasm`, and its printed form
    /// matches the checked-in `.wat` — which is where the absence of a
    /// `$__frame_ptr` local is legible to a reviewer rather than only to a
    /// differ.
    fn assert_golden(test_name: &str) -> Vec<u8> {
        let actual = wasm_codegen(&fixture_source(test_name));
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
        instantiate_wasm(&wasm_codegen(&fixture_source(test_name)), test_name)
    }

    /// Instantiates an already-compiled module, named `label` in failures.
    fn instantiate_wasm(wasm_bytes: &[u8], label: &str) -> (Store<()>, Instance) {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module for {label}: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate {label}: {e}"));
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
    /// zero — the same generator the bulk-copy probes use, for the same two
    /// reasons: a byte read out of a zero-filled frame instead of the caller's
    /// memory fails the comparison, and so does one read at the wrong
    /// displacement.
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

    fn i32_at(bytes: &[u8], offset: usize) -> i32 {
        let mut word = [0u8; 4];
        word.copy_from_slice(&bytes[offset..offset + 4]);
        i32::from_le_bytes(word)
    }

    fn i64_at(bytes: &[u8], offset: usize) -> i64 {
        let mut word = [0u8; 8];
        word.copy_from_slice(&bytes[offset..offset + 8]);
        i64::from_le_bytes(word)
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

    fn call1<A, R>(store: &mut Store<()>, instance: &Instance, name: &str, arg: A) -> R
    where
        A: wasmtime::WasmParams,
        R: wasmtime::WasmResults,
    {
        let f: TypedFunc<A, R> = instance
            .get_typed_func(&mut *store, name)
            .unwrap_or_else(|e| panic!("Failed to get '{name}': {e}"));
        f.call(&mut *store, arg)
            .unwrap_or_else(|e| panic!("'{name}' failed: {e}"))
    }

    /// Extracts one function's WAT text, from its `(func $name ` header to the
    /// closing paren at function indentation.
    fn function_wat(wasm: &[u8], name: &str) -> String {
        let wat = wasmprinter::print_bytes(wasm).expect("Failed to print WAT");
        let marker = format!("(func ${name} ");
        let start = wat
            .find(&marker)
            .unwrap_or_else(|| panic!("no function ${name} in the printed module:\n{wat}"));
        let rest = &wat[start..];
        let end = rest.find("\n  )").map_or(rest.len(), |offset| offset + 4);
        rest[..end].to_string()
    }

    /// Whether the module declares a linear memory of its own.
    fn has_memory_section(wasm: &[u8]) -> bool {
        inf_wasmparser::Parser::new(0)
            .parse_all(wasm)
            .any(|payload| {
                matches!(
                    payload.expect("a validated module parses"),
                    inf_wasmparser::Payload::MemorySection(_)
                )
            })
    }

    /// Asserts that `name` has no frame at all: nothing to hold a parameter
    /// copy, and therefore no shadow-stack traffic either.
    ///
    /// All three clauses are separate failures on purpose. Dropping only the
    /// copy would leave the frame pointer and the prologue behind; keeping the
    /// slot but not the copy would leave the parameter reading an uninitialized
    /// region. The global accesses are named directly because they are the
    /// verification half of the change: a function that does not touch
    /// `__stack_pointer` carries no global-effect clause for its callers to
    /// inherit.
    fn assert_frameless(wasm: &[u8], name: &str) {
        let body = function_wat(wasm, name);
        assert!(
            !body.contains("__frame_ptr"),
            "{name} reads its compound parameters through the caller's pointers, so \
             it must declare no frame pointer:\n{body}"
        );
        assert!(
            !body.contains("global.get 0"),
            "...and must not read the shadow stack pointer:\n{body}"
        );
        assert!(
            !body.contains("global.set 0"),
            "...and must not write it, so a leaf reader is a pure function of \
             memory:\n{body}"
        );
    }

    /// Asserts that `name` allocates a frame and rebinds `param` to a slot
    /// inside it — the two halves of an entry copy that are common to both
    /// lowering arms.
    ///
    /// The rebind is what makes the copy matter: without it the body would keep
    /// reading and writing the caller's region beside a fully formed, entirely
    /// unused copy.
    fn assert_param_copied(wasm: &[u8], name: &str, param: &str) {
        let body = function_wat(wasm, name);
        assert!(
            body.contains("__frame_ptr"),
            "{name} must allocate a frame to hold its own copy of `{param}`:\n{body}"
        );
        assert!(
            body.lines()
                .any(|line| line.trim_start() == format!("local.set ${param}")),
            "{name} must rebind `{param}` to the copy, or the copy is dead and the \
             body still works on the caller's memory:\n{body}"
        );
    }

    /// Asserts which of the two copy lowerings `name` used.
    ///
    /// A struct parameter is copied as one untyped region, whose accesses carry
    /// the conservative one-byte alignment hint a typed field store never does.
    /// An array parameter is copied element by element with ordinary typed
    /// stores and no such hint. The distinction is asserted in both directions
    /// because the two arms are separate emitters reached by separate gates: a
    /// change that only reached one of them would leave the other passing the
    /// caller's pointer straight through, and "a copy was emitted" alone cannot
    /// tell which arm ran.
    fn assert_copy_form(wasm: &[u8], name: &str, region: bool) {
        let body = function_wat(wasm, name);
        assert_eq!(
            body.contains("store align=1"),
            region,
            "{name} must copy its parameter as {} — a region copy stores untyped \
             bytes with the one-byte alignment hint, an element-wise copy stores \
             typed elements without it:\n{body}",
            if region { "one region" } else { "elements" }
        );
    }

    // The elision itself ---

    /// The headline shape: two struct parameters and an array parameter, read
    /// and never written, in functions that need no frame for anything else.
    ///
    /// Both counts are asserted. Zero copies alone would also hold if the
    /// parameters had stopped being recognized as compound at all; pairing it
    /// with four by-reference decisions says the predicate ran and answered.
    #[test]
    fn read_only_params_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 4);
        cov_mark::check_count!(wasm_codegen_param_written_in_body, 0);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 0);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 0);
        let wasm = assert_golden("read_only_params");

        assert_frameless(&wasm, "dot");
        assert_frameless(&wasm, "sum4");
        assert_frameless(&wasm, "pick4");
        // The contrast: the callers hold their compounds in frames of their own,
        // so the readers' framelessness is a property of those functions and not
        // of a module that lost its shadow stack.
        assert!(
            function_wat(&wasm, "call_dot").contains("__frame_ptr"),
            "the caller still frames its own compound locals"
        );
    }

    /// The by-reference readers return the same values as before the elision.
    #[test]
    fn read_only_params_execution_test() {
        let (mut store, instance) = instantiate("read_only_params");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_dot"),
            75,
            "2*5 + 3*7 + 4*11 read straight out of the caller's two structs"
        );
        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_sum4"),
            100,
            "10 + 20 + 30 + 40 read straight out of the caller's array"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// Driving the readers through the pointer ABI is what proves *where* they
    /// read: the pattern lives at a fixed low address no frame occupies, so a
    /// value that matches it came out of the caller's own memory.
    ///
    /// The region is read back afterwards because a by-reference parameter is
    /// the caller's memory — a reader that stored anything at all would be
    /// visible here and nowhere else.
    #[test]
    fn read_only_params_read_the_callers_bytes_test() {
        let (mut store, instance) = instantiate("read_only_params");
        let memory = memory_of(&mut store, &instance);
        // Six i64s: the first four back the array probes, and `dot` reads two
        // three-field structs out of the same region at two different addresses.
        let bytes = write_pattern(&mut store, &memory, 6 * 8);

        // The pattern makes each i64 enormous, and both the module and these
        // expectations wrap on overflow — the arithmetic is incidental, the
        // provenance of the operands is the point.
        assert_eq!(
            call1::<i32, i64>(&mut store, &instance, "sum4", PROBE_ADDR),
            (0..4).fold(0i64, |acc, i| acc.wrapping_add(i64_at(&bytes, i * 8))),
            "the array parameter's elements must come from the probe region"
        );
        for index in 0..4usize {
            let selector = i32::try_from(index).expect("probe indices are small");
            assert_eq!(
                call1::<(i32, i32), i64>(&mut store, &instance, "pick4", (PROBE_ADDR, selector)),
                i64_at(&bytes, index * 8),
                "element {index} must be read through the caller's pointer"
            );
        }
        assert_eq!(
            call1::<(i32, i32), i64>(&mut store, &instance, "dot", (PROBE_ADDR, PROBE_ADDR + 24)),
            i64_at(&bytes, 0)
                .wrapping_mul(i64_at(&bytes, 24))
                .wrapping_add(i64_at(&bytes, 8).wrapping_mul(i64_at(&bytes, 32)))
                .wrapping_add(i64_at(&bytes, 16).wrapping_mul(i64_at(&bytes, 40))),
            "two struct parameters at two different addresses in the caller's memory"
        );

        assert_eq!(
            read_probe_region(&mut store, &memory, 6 * 8),
            bytes,
            "a parameter nothing writes must leave the caller's bytes untouched"
        );
    }

    /// The written twin. One assignment rooted at the parameter is the whole
    /// difference from `read_only_params`, and it restores the slot, the copy
    /// and the rebind.
    #[test]
    fn written_param_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_written_in_body, 1);
        cov_mark::check_count!(wasm_codegen_param_by_reference, 0);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 1);
        let wasm = assert_golden("written_param");
        assert_param_copied(&wasm, "bump", "v");
        assert_copy_form(&wasm, "bump", true);
    }

    /// And the copy is observable, not merely counted: the caller's field still
    /// holds its own value after the callee wrote through its parameter.
    #[test]
    fn written_param_execution_test() {
        let (mut store, instance) = instantiate("written_param");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i64>(&mut store, &instance, "call_bump"),
            106_001,
            "the callee sees its own incremented copy (101+2+3) and the caller's \
             x is still 1"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// `mut` parameters that are never assigned — the one half of the domain
    /// where the write scan says something the `mut` marker cannot.
    ///
    /// Everywhere else the two must agree, because an assignment rooted at a
    /// non-`mut` parameter is already a type error. Here the marker says the
    /// callee may write and the body says it never does, and the body decides.
    /// Without this fixture the scan's entire reason for existing is untested.
    #[test]
    fn mut_never_written_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 2);
        cov_mark::check_count!(wasm_codegen_param_written_in_body, 0);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 0);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 0);
        let wasm = assert_golden("mut_never_written");
        assert_frameless(&wasm, "peek_struct");
        assert_frameless(&wasm, "peek_array");
    }

    /// The execution half of the same claim: a `mut` parameter that is passed by
    /// reference reads the caller's bytes and leaves every one of them alone.
    #[test]
    fn mut_never_written_execution_test() {
        let (mut store, instance) = instantiate("mut_never_written");
        let initial_sp = stack_pointer(&mut store, &instance);
        let memory = memory_of(&mut store, &instance);
        let bytes = write_pattern(&mut store, &memory, 8 * 4);

        for index in 0..8usize {
            let selector = i32::try_from(index).expect("probe indices are small");
            assert_eq!(
                call1::<(i32, i32), i32>(
                    &mut store,
                    &instance,
                    "peek_array",
                    (PROBE_ADDR, selector)
                ),
                i32_at(&bytes, index * 4),
                "element {index} of a `mut` array parameter must be read in place"
            );
        }
        assert_eq!(
            call1::<i32, i32>(&mut store, &instance, "peek_struct", PROBE_ADDR),
            i32_at(&bytes, 0)
                .wrapping_mul(10)
                .wrapping_add(i32_at(&bytes, 4)),
            "a `mut` struct parameter must be read in place too"
        );
        assert_eq!(
            read_probe_region(&mut store, &memory, 8 * 4),
            bytes,
            "and neither may disturb the caller's bytes"
        );

        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_peek_struct"),
            4747,
            "the in-language caller sees its own struct unchanged as well"
        );
        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// The gate: a parameter reaching an `external fn` argument keeps its copy.
    ///
    /// This fixture is the only thing in the corpus that exercises the escape
    /// mark at all — no other fixture declares an external with a compound
    /// parameter — so it is the sole coverage of the one condition that makes
    /// the elision sound in the presence of foreign code.
    ///
    /// Both lowering arms are counted separately. A struct copy is emitted as
    /// one region move and an array copy as element-wise stores, so a gate that
    /// reached only one of them would leave the other handing the caller's
    /// pointer to a body that may store through it.
    #[test]
    fn extern_forward_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_escapes_to_extern, 2);
        cov_mark::check_count!(wasm_codegen_param_by_reference, 1);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 1);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 1);
        let wasm = assert_golden("extern_forward");

        assert_param_copied(&wasm, "forward_struct", "p");
        assert_copy_form(&wasm, "forward_struct", true);
        assert_param_copied(&wasm, "forward_array", "a");
        assert_copy_form(&wasm, "forward_array", false);
        assert_frameless(&wasm, "no_forward");
    }

    // What the module keeps when the last frame goes ---

    /// A module whose only memory user is a by-reference parameter must still
    /// declare the memory that parameter is read out of.
    ///
    /// `get_a` reads its parameter through the caller's pointer, so it gets no
    /// slot and no frame — and this module holds nothing else: no caller, no
    /// compound local, no `main`. Its one `i32.load` is the only memory access
    /// in the whole program, so whether a memory section, a `__stack_pointer`
    /// global and their exports are emitted rests on the parameter alone. A
    /// compiler that concluded "memory is needed" from the frame layout instead
    /// would emit, for this module, loads against a memory it never declares —
    /// no validator accepts that and no host can instantiate it — and would
    /// drop the `__stack_pointer` export every caller and every harness reads.
    ///
    /// The source is inline because every fixture in this file hides the trap:
    /// each of their callers holds a compound local, so some function allocates
    /// a frame and the memory comes along behind it no matter where the
    /// decision is taken. Only a leaf with no caller separates the two.
    ///
    /// The counts say the parameter was in fact elided. Were it copied instead,
    /// the module would carry a frame and prove nothing.
    #[test]
    fn a_lone_by_reference_parameter_declares_the_modules_memory() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 1);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 0);

        let source = "\
struct Pair {
    a: i32;
    b: i32;
}

pub fn get_a(p: Pair) -> i32 {
    return p.a;
}
";
        let wasm = wasm_codegen(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("a lone by-reference parameter must still validate: {e}"));

        assert_frameless(&wasm, "get_a");
        assert!(
            has_memory_section(&wasm),
            "nothing in this module allocates a frame, so the parameter is the \
             only thing that can ask for a memory to load from:\n{}",
            wasmprinter::print_bytes(&wasm).expect("Failed to print WAT")
        );

        let (mut store, instance) = instantiate_wasm(&wasm, "get_a");
        // Looking the global up is the export assertion: a module that lost it
        // fails here rather than quietly passing the value check below.
        let initial_sp = stack_pointer(&mut store, &instance);
        let memory = memory_of(&mut store, &instance);
        let bytes = write_pattern(&mut store, &memory, 8);

        assert_eq!(
            call1::<i32, i32>(&mut store, &instance, "get_a", PROBE_ADDR),
            i32_at(&bytes, 0),
            "the load must resolve through the module's own memory and return the \
             caller's first field"
        );
        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "and a frameless function must leave the shadow stack it never \
             touched exactly where it was"
        );
    }

    // Aliasing the elision makes reachable ---

    /// The same variable passed twice. Without a copy the two parameters are one
    /// address, which is a state no previous program could produce.
    #[test]
    fn alias_same_argument_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 3);
        cov_mark::check_count!(wasm_codegen_param_written_in_body, 1);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 1);
        let wasm = assert_golden("alias_same_argument");
        assert_frameless(&wasm, "add_xy");
        assert_param_copied(&wasm, "write_b", "b");
        assert_copy_form(&wasm, "write_b", true);
    }

    /// The load-bearing row: one parameter is written and copied while the other
    /// is passed by reference, and both are the same caller variable. The write
    /// must reach neither.
    #[test]
    fn alias_same_argument_execution_test() {
        let (mut store, instance) = instantiate("alias_same_argument");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i32>(&mut store, &instance, "same_twice"),
            1212,
            "both parameters point at the same struct and must read the same values"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "distinct_arguments"),
            1234,
            "two distinct arguments must still be told apart"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "write_b_same_variable"),
            109_901,
            "the write to the copied parameter must reach neither the by-reference \
             parameter (a.x stays 1, so the callee returns 1099) nor the caller's \
             variable (t.x stays 1); 99099 would mean it reached the former and \
             109999 the latter"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "write_b_distinct_variables"),
            109_913,
            "and with two distinct arguments the same write still touches only the \
             copy: the by-reference `a` reads 1 and both caller variables are \
             unchanged"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// Whole and part: one parameter's region strictly contains the other's.
    #[test]
    fn alias_whole_and_part_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 4);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 0);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 0);
        let wasm = assert_golden("alias_whole_and_part");
        assert_frameless(&wasm, "whole_and_field");
        assert_frameless(&wasm, "whole_and_element");
    }

    /// Each of the four reads is weighted differently in the result, so a read
    /// that resolved against the containing region instead of the contained one
    /// changes the number rather than coinciding with it.
    ///
    /// Both halves of each number are load-bearing. The leading four digits are
    /// what the callee returned, and the trailing digits are the caller's own
    /// aggregate read back after control returned — the half that says the two
    /// pointers, one of which addresses a range inside the other's, left the
    /// caller's bytes where they were.
    #[test]
    fn alias_whole_and_part_execution_test() {
        let (mut store, instance) = instantiate("alias_whole_and_part");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_whole_and_field"),
            7_334_734,
            "a struct and its own field, passed together: the callee reads \
             7*1000 + 3*100 + 3*10 + 4 = 7334 through the two overlapping \
             pointers, and afterwards the caller's own `Outer` still reads \
             {{ head: 7, body: {{ p: 3, q: 4 }} }} (734)"
        );
        for (index, expected) in [(0i32, 1_612_161), (1, 1_634_163), (2, 1_656_165)] {
            assert_eq!(
                call1::<i32, i32>(&mut store, &instance, "call_whole_and_element", index),
                expected,
                "an array and its own element {index}, passed together: the callee's \
                 answer leads, and the trailing three digits are the caller's array \
                 read back — items[0].a = 1, items[2].b = 6 and the very element it \
                 handed over as the contained region"
            );
        }

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// The receiver channel. A receiver moves without ever appearing as a call
    /// argument, so it is the one hop an argument-position analysis does not
    /// walk — which is why `self` is treated on exactly the same terms as a
    /// named parameter.
    ///
    /// The counts split the two halves: eight by-reference decisions across the
    /// receivers and arguments that are only read, and two writes — the
    /// `mut self` methods that assign `self.tag`, which keep their slots and
    /// their copies.
    #[test]
    fn alias_receiver_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 8);
        cov_mark::check_count!(wasm_codegen_param_written_in_body, 2);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 2);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 2);
        let wasm = assert_golden("alias_receiver");

        assert_frameless(&wasm, "Holder.read_with_part");
        assert_frameless(&wasm, "Holder.read_with_holder");
        assert_frameless(&wasm, "Holder.native_sub_object");
        assert_frameless(&wasm, "part_sum");
        assert_param_copied(&wasm, "Holder.bump_with_part", "self");
        assert_copy_form(&wasm, "Holder.bump_with_part", true);
        assert_param_copied(&wasm, "Holder.bump_with_holder", "self");
    }

    /// Every row here reports both halves: the callee's answer leads, and the
    /// trailing `53` is the caller's own `Holder` read back after control
    /// returned — its `tag` beside the `body.u` it handed over. A pointer that
    /// addressed the wrong region moves the leading digits; one that was written
    /// through moves the trailing pair.
    #[test]
    fn alias_receiver_execution_test() {
        let (mut store, instance) = instantiate("alias_receiver");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_receiver_and_part"),
            533_453,
            "a receiver and one of its own fields, passed together: the callee \
             reads 5*1000 + 3*100 + 3*10 + 4 = 5334 through the two overlapping \
             pointers, and the caller's Holder still reads tag 5, body.u 3"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_receiver_and_receiver"),
            545_453,
            "a receiver passed to itself as an argument: 5*1000 + 4*100 + 5*10 + \
             4 = 5454 out of the one region both pointers name, and that region \
             is unchanged afterwards"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_mut_receiver_and_part"),
            63_453,
            "the `mut self` copy is what the method increments (634), so the \
             caller's tag is still 5 and the field it passed is still 3"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_mut_receiver_and_receiver"),
            65_453,
            "the receiver is copied because it is written and the argument — the \
             same variable — is not, so the method reads its own incremented tag \
             (6) beside the caller's original one (5); 66 in the leading pair \
             would mean the argument saw the receiver's copy"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_native_sub_object"),
            3453,
            "a sub-object of a by-reference receiver is an address two frames up: \
             the native callee reads Part {{ u: 3, v: 4 }} there (34) and leaves \
             the caller's Holder holding tag 5 and body.u 3"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    /// A compound return writes the caller's destination field by field while
    /// the function reads a parameter that is no longer copied.
    ///
    /// The white-box argument says the two can never overlap — a destination is
    /// always a fresh binding and parameter slots are a frame prefix — but that
    /// is an argument about unreachability, which is exactly the kind that stops
    /// being true without anything failing. These execute.
    ///
    /// `idp` and `ida` are the sret arm's other source shape. Returning the
    /// parameter itself moves one whole region rather than storing the
    /// destination field by field, and its source is a by-reference parameter —
    /// an address in the caller's memory, not a callee frame slot that sits
    /// safely below it. Both the struct and the array are present because the
    /// two sret arms are reached separately.
    ///
    /// Which rows take that whole-region move is counted, because it is the
    /// only thing that makes them a test of it. The region copy is reached from
    /// exactly one return form — `return <identifier>` — so the two hits are
    /// `idp` and `ida`. `swap_xy`, `copy_of` and `rotate` return struct
    /// literals and store the destination field by field; `wrap` returns a call
    /// and forwards its own destination pointer down instead of copying
    /// anything. A two that became a five would mean the field-by-field rows
    /// had quietly started moving regions, and a two that became a zero would
    /// mean the row written to exercise a caller-memory source stopped
    /// exercising it — neither changes a returned value.
    #[test]
    fn alias_sret_golden_test() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 6);
        cov_mark::check_count!(wasm_codegen_param_written_in_body, 0);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 0);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 0);
        cov_mark::check_count!(wasm_codegen_emit_sret_copy, 2);
        let wasm = assert_golden("alias_sret");
        assert_frameless(&wasm, "swap_xy");
        assert_frameless(&wasm, "copy_of");
        assert_frameless(&wasm, "rotate");
        assert_frameless(&wasm, "wrap");
        // The identity functions need no frame either: both ends of the region
        // copy are addresses the caller supplied, so there is nothing left for a
        // frame to hold.
        assert_frameless(&wasm, "idp");
        assert_frameless(&wasm, "ida");
    }

    #[test]
    fn alias_sret_execution_test() {
        let (mut store, instance) = instantiate("alias_sret");
        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_swap_xy"),
            1213,
            "the destination is written x-then-y while the source is read \
             y-then-x, so a destination overlapping the argument would read `x` \
             back after its own store and return 2223 instead of 213 with the \
             caller's x still 1"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_copy_of"),
            456_456,
            "a plain `let d: P = copy_of(c);` destination beside a by-reference \
             argument: the copy came out right (the trailing 456) and the source \
             it was copied from still reads {{ 4, 5, 6 }} afterwards (the leading \
             456). A field-for-field copy returns the correct value even when its \
             destination *is* its source, so only the caller's half separates the \
             two"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_wrap"),
            3312,
            "the same destination pointer forwarded one level down: the writer and \
             the reader of any overlap would be two different frames"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_idp"),
            789_789,
            "returning a by-reference struct parameter copies one region out of \
             the caller's memory into another region of it: the destination came \
             out {{ 7, 8, 9 }} (the trailing 789), so the copy ran, and the source \
             still reads {{ 7, 8, 9 }} (the leading 789), so being the source of a \
             whole-region move left it alone"
        );
        assert_eq!(
            call0::<i32>(&mut store, &instance, "call_ida"),
            12_341_234,
            "and the array arm of the same shape: [1, 2, 3, 4] copied whole out of \
             the caller's memory (the trailing 1234) with the source array intact \
             behind it (the leading 1234)"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "Stack pointer should be restored after every call"
        );
    }

    // Mode independence ---

    /// The elision fires in proof mode too.
    ///
    /// Nothing may gate emission on the compilation mode, or the Rocq
    /// translation describes a different program than the shipped binary. The
    /// breadth is the point: the decision is taken once per parameter in the
    /// layout pass, but the frame it suppresses is emitted from the prologue,
    /// the epilogue and two separate copy emitters, and a mode check left in any
    /// of them would show up in only some of these fixtures.
    ///
    /// The sret-copy count carries over unchanged as well. `alias_sret`'s `idp`
    /// and `ida` are the only compound returns in the whole family, so the two
    /// whole-region moves they emit in compile mode are the two this loop must
    /// still see.
    #[test]
    fn proof_mode_elides_the_same_parameters() {
        cov_mark::check_count!(wasm_codegen_param_by_reference, 27);
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 3);
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 0);
        cov_mark::check_count!(wasm_codegen_emit_sret_copy, 2);

        for (fixture, frameless) in [
            ("read_only_params", &["dot", "sum4", "pick4"][..]),
            ("mut_never_written", &["peek_struct", "peek_array"]),
            ("alias_same_argument", &["add_xy"]),
            (
                "alias_whole_and_part",
                &["whole_and_field", "whole_and_element"],
            ),
            (
                "alias_receiver",
                &["Holder.read_with_part", "Holder.native_sub_object"],
            ),
            ("alias_sret", &["swap_xy", "wrap", "idp", "ida"]),
        ] {
            let output = codegen_output_with_mode(&fixture_source(fixture), CompilationMode::Proof);
            for name in frameless {
                assert_frameless(output.wasm(), name);
            }
        }
    }
}

/// Regeneration helpers for the goldens in this file, gated behind `#[ignore]`.
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::param_by_ref::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn fixture_dir(test_name: &str) -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("param_by_ref")
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
    fn regenerate_read_only_params_wasm() {
        regenerate("read_only_params");
    }

    #[test]
    #[ignore]
    fn regenerate_written_param_wasm() {
        regenerate("written_param");
    }

    #[test]
    #[ignore]
    fn regenerate_mut_never_written_wasm() {
        regenerate("mut_never_written");
    }

    #[test]
    #[ignore]
    fn regenerate_extern_forward_wasm() {
        regenerate("extern_forward");
    }

    #[test]
    #[ignore]
    fn regenerate_alias_same_argument_wasm() {
        regenerate("alias_same_argument");
    }

    #[test]
    #[ignore]
    fn regenerate_alias_whole_and_part_wasm() {
        regenerate("alias_whole_and_part");
    }

    #[test]
    #[ignore]
    fn regenerate_alias_receiver_wasm() {
        regenerate("alias_receiver");
    }

    #[test]
    #[ignore]
    fn regenerate_alias_sret_wasm() {
        regenerate("alias_sret");
    }
}

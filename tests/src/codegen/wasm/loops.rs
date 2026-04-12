// Loop codegen tests: conditional loops, infinite loops with break, nested loops,
// loops with if-else bodies, and accumulator patterns.
//
// WASM lowering pattern for conditional loop (`loop COND { body }`):
//
//   block $exit
//     loop $continue
//       <lower condition>
//       i32.eqz
//       br_if 1             ;; exit when condition is false
//       <lower body>
//       br 0                ;; unconditional back-edge
//     end
//   end
//
// WASM lowering pattern for infinite loop (`loop { body }`):
//
//   block $exit
//     loop $continue
//       <lower body>        ;; break => br to $exit
//       br 0                ;; unconditional back-edge
//     end
//   end
//
// Break statement lowers to `br <depth>` where depth is computed from the
// LoopContext tracking wasm_block_depth and loop_exit_depths.

#[cfg(test)]
mod loops_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn simple_loop_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 2);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 2);
        let test_name = "simple_loop";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn simple_loop_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "simple_loop";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // count_to_ten: loop from 0 while i < 10 => returns 10
        call!("count_to_ten", i32, (), 10_i32);

        // count_down: loop from n while n > 0 => returns 0
        call!("count_down", i32, 5_i32, 0_i32);
        call!("count_down", i32, 0_i32, 0_i32);
        call!("count_down", i32, 1_i32, 0_i32);
    }

    #[test]
    fn infinite_loop_break_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 2);
        cov_mark::check_count!(wasm_codegen_emit_loop_infinite, 2);
        cov_mark::check_count!(wasm_codegen_emit_break, 2);
        let test_name = "infinite_loop_break";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn infinite_loop_break_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "infinite_loop_break";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // find_threshold: starts at start, adds 7 each iteration until >= 100
        call!("find_threshold", i32, 0_i32, 105_i32);
        call!("find_threshold", i32, 50_i32, 106_i32);
        call!("find_threshold", i32, 99_i32, 106_i32);
        call!("find_threshold", i32, 100_i32, 100_i32);

        // first_multiple_of: finds smallest i such that i * n >= limit
        call!("first_multiple_of", i32, (3_i32, 10_i32), 4_i32);
        call!("first_multiple_of", i32, (5_i32, 25_i32), 5_i32);
        call!("first_multiple_of", i32, (1_i32, 1_i32), 1_i32);
    }

    #[test]
    fn nested_loop_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 4);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 3);
        cov_mark::check_count!(wasm_codegen_emit_loop_infinite, 1);
        cov_mark::check_count!(wasm_codegen_emit_break, 1);
        let test_name = "nested_loop";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn nested_loop_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "nested_loop";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // nested_count: 3 * 4 = 12
        call!("nested_count", i32, (), 12_i32);

        // nested_break: 10 outer iterations, inner breaks at j >= 3 => 10 * 3 = 30
        call!("nested_break", i32, (), 30_i32);
    }

    #[test]
    fn loop_with_if_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 2);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 2);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 2);
        let test_name = "loop_with_if";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_with_if_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "loop_with_if";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // count_evens: counts even numbers in [0, n)
        call!("count_evens", i32, 10_i32, 5_i32);
        call!("count_evens", i32, 1_i32, 1_i32);
        call!("count_evens", i32, 0_i32, 0_i32);
        call!("count_evens", i32, 7_i32, 4_i32);

        // abs_sum: sum of |i| for i in [-n, n]
        call!("abs_sum", i32, 3_i32, 12_i32);
        call!("abs_sum", i32, 1_i32, 2_i32);
        call!("abs_sum", i32, 0_i32, 0_i32);
    }

    #[test]
    fn loop_accumulator_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 3);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 3);
        let test_name = "loop_accumulator";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_accumulator_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "loop_accumulator";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // sum_1_to_n: sum of 1..=n
        call!("sum_1_to_n", i32, 10_i32, 55_i32);
        call!("sum_1_to_n", i32, 1_i32, 1_i32);
        call!("sum_1_to_n", i32, 0_i32, 0_i32);
        call!("sum_1_to_n", i32, 100_i32, 5050_i32);

        // factorial: n!
        call!("factorial", i32, 5_i32, 120_i32);
        call!("factorial", i32, 1_i32, 1_i32);
        call!("factorial", i32, 0_i32, 1_i32);
        call!("factorial", i32, 10_i32, 3628800_i32);

        // power: base^exp
        call!("power", i32, (2_i32, 10_i32), 1024_i32);
        call!("power", i32, (3_i32, 5_i32), 243_i32);
        call!("power", i32, (5_i32, 0_i32), 1_i32);
        call!("power", i32, (7_i32, 1_i32), 7_i32);
    }

    #[test]
    fn loop_break_early_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 1);
        cov_mark::check_count!(wasm_codegen_emit_break, 1);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 1);
        let test_name = "loop_break_early";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_break_early_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "loop_break_early";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // loop_break_early: sums i=0,1,2,3,4,5 => sum=15 (breaks when sum > 10)
        call!("loop_break_early", i32, 100_i32, 15_i32);
        // n=3: sums i=0,1,2 => sum=3 (loop ends normally, 3 < 3 is false)
        call!("loop_break_early", i32, 3_i32, 3_i32);
        // n=0: loop doesn't execute
        call!("loop_break_early", i32, 0_i32, 0_i32);
        // n=-1: condition false from start
        call!("loop_break_early", i32, -1_i32, 0_i32);
    }

    #[test]
    fn break_nested_if_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 2);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_infinite, 1);
        cov_mark::check_count!(wasm_codegen_emit_break, 2);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 4);
        let test_name = "break_nested_if";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn break_nested_if_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "break_nested_if";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // break_nested_if(5): iterates i=0..5 (6 iters), then at i=6 (>5, 6%2==0) breaks
        call!("break_nested_if", i32, 5_i32, 6_i32);
        // break_nested_if(0): at i=1 (>0, 1%2!=0), continues; at i=2 (>0, 2%2==0) breaks
        call!("break_nested_if", i32, 0_i32, 2_i32);
        // break_nested_if(99): iterates i=0..99 (100 iters), loop ends (100 < 100 is false)
        call!("break_nested_if", i32, 99_i32, 100_i32);

        // break_double_nested_if: iterates i=0..9 (10 iters), at i=10 both ifs true, breaks
        call!("break_double_nested_if", i32, (), 10_i32);
    }

    #[test]
    fn void_loop_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 1);
        let test_name = "void_loop";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn void_loop_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "void_loop";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        // void_loop: executes without error, returns nothing
        let func: TypedFunc<i32, ()> = instance
            .get_typed_func(&mut store, "void_loop")
            .unwrap_or_else(|e| panic!("Failed to get 'void_loop': {e}"));
        func.call(&mut store, 5_i32)
            .unwrap_or_else(|e| panic!("Call to 'void_loop(5)' failed: {e}"));
        func.call(&mut store, 0_i32)
            .unwrap_or_else(|e| panic!("Call to 'void_loop(0)' failed: {e}"));
    }

    #[test]
    fn loop_zero_iters_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 1);
        let test_name = "loop_zero_iters";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_zero_iters_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "loop_zero_iters";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        // loop_zero_iters: condition `false` means body never executes, x stays 0
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "loop_zero_iters")
            .unwrap_or_else(|e| panic!("Failed to get 'loop_zero_iters': {e}"));
        let result = func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Call to 'loop_zero_iters' failed: {e}"));
        assert_eq!(result, 0_i32, "loop_zero_iters() expected 0");
    }

    #[test]
    fn loop_with_array_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 2);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 2);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 2);
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 6);
        cov_mark::check_count!(wasm_codegen_emit_array_index_write, 1);
        let test_name = "loop_with_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_with_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "loop_with_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // sum_array_elements: 10 + 20 + 30 + 40 = 100
        call!("sum_array_elements", i32, (), 100_i32);

        // fill_and_sum(3): arr = [0, 3, 6, 9, 12] => 0+3+6+9+12 = 30
        call!("fill_and_sum", i32, 3_i32, 30_i32);
        // fill_and_sum(1): arr = [0, 1, 2, 3, 4] => 0+1+2+3+4 = 10
        call!("fill_and_sum", i32, 1_i32, 10_i32);
        // fill_and_sum(0): arr = [0, 0, 0, 0, 0] => 0
        call!("fill_and_sum", i32, 0_i32, 0_i32);
    }

    #[test]
    fn loop_in_nondet_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 2);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 2);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_exists_block, 1);
        let test_name = "loop_in_nondet";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn nondet_then_break_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 1);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_assume_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_break, 1);
        let test_name = "nondet_then_break";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_return_array_test() {
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 1);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 1);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 1);
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 2);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 1);
        let test_name = "loop_return_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn loop_return_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "loop_return_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // arr[0]=10 > 5, returns 10
        call!("loop_return_array", i32, 5_i32, 10_i32);
        // arr[1]=20 > 15, returns 20
        call!("loop_return_array", i32, 15_i32, 20_i32);
        // arr[2]=30 > 25, returns 30
        call!("loop_return_array", i32, 25_i32, 30_i32);
        // no element > 100, returns 0
        call!("loop_return_array", i32, 100_i32, 0_i32);
    }
}

/// Test data regeneration helper.
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir(name: &str) -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("loops")
            .join(name)
    }

    #[test]
    #[ignore]
    fn regenerate_simple_loop() {
        let name = "simple_loop";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_infinite_loop_break() {
        let name = "infinite_loop_break";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_nested_loop() {
        let name = "nested_loop";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_loop_with_if() {
        let name = "loop_with_if";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_loop_accumulator() {
        let name = "loop_accumulator";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_loop_break_early() {
        let name = "loop_break_early";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_break_nested_if() {
        let name = "break_nested_if";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_void_loop() {
        let name = "void_loop";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_loop_zero_iters() {
        let name = "loop_zero_iters";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_loop_with_array() {
        let name = "loop_with_array";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }

    #[test]
    #[ignore]
    fn regenerate_loop_in_nondet() {
        let name = "loop_in_nondet";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }

    #[test]
    #[ignore]
    fn regenerate_nondet_then_break() {
        let name = "nondet_then_break";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }

    #[test]
    #[ignore]
    fn regenerate_loop_return_array() {
        let name = "loop_return_array";
        let dir = test_dir(name);
        let source_code =
            std::fs::read_to_string(dir.join(format!("{name}.inf"))).expect("Failed to read .inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join(format!("{name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, name);
    }
}

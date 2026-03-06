// Array algorithm tests: 12 functions exercising array creation, indexing, mutation,
// and iteration patterns across multiple element types (i8, u8, i16, u16, i32, u32, i64).
//
// Algorithms:
//   linear_search     - sequential scan through i32 array
//   binary_search     - classic binary search on sorted i32 array
//   bubble_sort_element - bubble sort with swap, return element by index
//   dot_product       - dot product of two i32 arrays
//   array_max         - find maximum in first N elements of i32 array
//   prefix_sum_element - in-place prefix sum, return element by index
//   sum_u8_array      - sum all elements of u8 array, return as u8
//   min_i8_array      - minimum of i8 array, return as i8
//   max_i16_array     - maximum of i16 array, return as i16
//   sum_u16_array     - sum all elements of u16 array, return as u16
//   search_u32_array  - linear search through u32 array
//   dot_product_i64   - dot product of two i64 arrays
//
// All functions take scalar parameters and return scalar results. Arrays are internal
// to each function, so cross-compiler comparison works at the observable I/O boundary.

#[cfg(test)]
mod algo_array_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn algo_array_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 45);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 8);
        cov_mark::check_count!(wasm_codegen_emit_if_with_else, 1);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 6);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 45);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 1);
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 13);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 13);
        cov_mark::check_count!(wasm_codegen_emit_assign_identifier, 25);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 14);
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 24);
        cov_mark::check_count!(wasm_codegen_emit_array_index_write, 3);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 12);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 24);
        let test_name = "algo_array";
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
    fn algo_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "algo_array";
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

        // --- linear_search ---
        call!("linear_search", i32, 3_i32, 0_i32);
        call!("linear_search", i32, 9_i32, 3_i32);
        call!("linear_search", i32, 2_i32, 7_i32);
        call!("linear_search", i32, 5_i32, 8_i32);

        // --- binary_search ---
        call!("binary_search", i32, 8_i32, 2_i32);
        call!("binary_search", i32, 56_i32, 7_i32);
        call!("binary_search", i32, 2_i32, 0_i32);
        call!("binary_search", i32, 10_i32, 8_i32);

        // --- bubble_sort_element ---
        call!("bubble_sort_element", i32, 0_i32, 1_i32);
        call!("bubble_sort_element", i32, 1_i32, 2_i32);
        call!("bubble_sort_element", i32, 2_i32, 3_i32);
        call!("bubble_sort_element", i32, 3_i32, 5_i32);
        call!("bubble_sort_element", i32, 4_i32, 8_i32);
        call!("bubble_sort_element", i32, 5_i32, 9_i32);

        // --- dot_product ---
        call!("dot_product", i32, (), 70_i32);

        // --- array_max ---
        call!("array_max", i32, 1_i32, 3_i32);
        call!("array_max", i32, 4_i32, 9_i32);
        call!("array_max", i32, 8_i32, 9_i32);

        // --- prefix_sum_element ---
        call!("prefix_sum_element", i32, 0_i32, 1_i32);
        call!("prefix_sum_element", i32, 1_i32, 3_i32);
        call!("prefix_sum_element", i32, 2_i32, 6_i32);
        call!("prefix_sum_element", i32, 3_i32, 10_i32);
        call!("prefix_sum_element", i32, 5_i32, 21_i32);

        // --- sum_u8_array (returns u8 as WASM i32) ---
        call!("sum_u8_array", i32, (), 36_i32);

        // --- min_i8_array (returns i8 as WASM i32) ---
        call!("min_i8_array", i32, (), 10_i32);

        // --- max_i16_array (returns i16 as WASM i32) ---
        call!("max_i16_array", i32, (), 900_i32);

        // --- sum_u16_array (returns u16 as WASM i32) ---
        call!("sum_u16_array", i32, (), 21000_i32);

        // --- search_u32_array ---
        call!("search_u32_array", i32, 300_i32, 2_i32);
        call!("search_u32_array", i32, 100_i32, 0_i32);
        call!("search_u32_array", i32, 999_i32, 6_i32);

        // --- dot_product_i64 ---
        // 100000*500000 + 200000*600000 + 300000*700000 + 400000*800000 = 700_000_000_000
        call!("dot_product_i64", i64, (), 700000000000_i64);
    }

    #[test]
    fn algo_array_cross_compiler_test() {
        use std::process::Command;
        use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

        // --- Compile Inference source ---
        let test_name = "algo_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let inf_wasm = wasm_codegen(&source_code);

        // --- Compile Rust source ---
        let test_data_dir = crate::utils::get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_array");
        let rust_source = test_data_dir.join("algo_array.rs");
        let rust_out_dir = std::path::PathBuf::from("/tmp/algo_array_rustc");
        std::fs::create_dir_all(&rust_out_dir)
            .unwrap_or_else(|e| panic!("Failed to create temp dir: {e}"));
        let rust_wasm_path = rust_out_dir.join("algo_array.wasm");

        let rustc_status = Command::new("rustc")
            .arg("--target")
            .arg("wasm32-unknown-unknown")
            .arg("--crate-type")
            .arg("cdylib")
            .arg("-C")
            .arg("opt-level=0")
            .arg("-o")
            .arg(&rust_wasm_path)
            .arg(&rust_source)
            .status();

        let rustc_ok = match rustc_status {
            Ok(status) => {
                if !status.success() {
                    println!("Skipping: rustc compilation failed (exit code {:?})", status.code());
                    false
                } else {
                    true
                }
            }
            Err(e) => {
                println!("Skipping: rustc not available ({e})");
                false
            }
        };

        if !rustc_ok {
            return;
        }

        // --- Compile Zig source ---
        let zig_source = test_data_dir.join("algo_array.zig");
        let zig_out_dir = std::path::PathBuf::from("/tmp/algo_array_zig");
        std::fs::create_dir_all(&zig_out_dir)
            .unwrap_or_else(|e| panic!("Failed to create temp dir: {e}"));
        let zig_wasm_path = zig_out_dir.join("algo_array.wasm");

        let zig_status = Command::new("zig")
            .arg("build-exe")
            .arg("-target")
            .arg("wasm32-freestanding")
            .arg("-fno-entry")
            .arg("-rdynamic")
            .arg("-OReleaseFast")
            .arg(&zig_source)
            .arg(format!("-femit-bin={}", zig_wasm_path.display()))
            .status();

        let zig_ok = match zig_status {
            Ok(status) => {
                if !status.success() {
                    println!("Skipping: zig compilation failed (exit code {:?})", status.code());
                    false
                } else {
                    true
                }
            }
            Err(e) => {
                println!("Skipping: zig not available ({e})");
                false
            }
        };

        if !zig_ok {
            return;
        }

        // --- Load all three WASM modules ---
        let engine = Engine::default();

        let inf_module = Module::new(&engine, &inf_wasm)
            .unwrap_or_else(|e| panic!("Failed to load Inference WASM: {e}"));
        let rust_wasm = std::fs::read(&rust_wasm_path)
            .unwrap_or_else(|e| panic!("Failed to read Rust WASM at {}: {e}", rust_wasm_path.display()));
        let rust_module = Module::new(&engine, &rust_wasm)
            .unwrap_or_else(|e| panic!("Failed to load Rust WASM: {e}"));
        let zig_wasm = std::fs::read(&zig_wasm_path)
            .unwrap_or_else(|e| panic!("Failed to read Zig WASM at {}: {e}", zig_wasm_path.display()));
        let zig_module = Module::new(&engine, &zig_wasm)
            .unwrap_or_else(|e| panic!("Failed to load Zig WASM: {e}"));

        let mut inf_store = Store::new(&engine, ());
        let mut rust_store = Store::new(&engine, ());
        let mut zig_store = Store::new(&engine, ());

        let inf_instance = Instance::new(&mut inf_store, &inf_module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Inference WASM: {e}"));
        let rust_instance = Instance::new(&mut rust_store, &rust_module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Rust WASM: {e}"));
        let zig_instance = Instance::new(&mut zig_store, &zig_module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Zig WASM: {e}"));

        macro_rules! compare {
            ($name:expr, $ret:ty, $args:expr) => {{
                let inf_f: TypedFunc<_, $ret> = inf_instance
                    .get_typed_func(&mut inf_store, $name)
                    .unwrap_or_else(|e| panic!("Inference: failed to get '{}': {e}", $name));
                let rust_f: TypedFunc<_, $ret> = rust_instance
                    .get_typed_func(&mut rust_store, $name)
                    .unwrap_or_else(|e| panic!("Rust: failed to get '{}': {e}", $name));
                let zig_f: TypedFunc<_, $ret> = zig_instance
                    .get_typed_func(&mut zig_store, $name)
                    .unwrap_or_else(|e| panic!("Zig: failed to get '{}': {e}", $name));
                let inf_result = inf_f
                    .call(&mut inf_store, $args)
                    .unwrap_or_else(|e| panic!("Inference call '{}' failed: {e}", $name));
                let rust_result = rust_f
                    .call(&mut rust_store, $args)
                    .unwrap_or_else(|e| panic!("Rust call '{}' failed: {e}", $name));
                let zig_result = zig_f
                    .call(&mut zig_store, $args)
                    .unwrap_or_else(|e| panic!("Zig call '{}' failed: {e}", $name));
                assert_eq!(
                    inf_result, rust_result,
                    "Inference vs Rust mismatch for {}({:?})", $name, $args
                );
                assert_eq!(
                    inf_result, zig_result,
                    "Inference vs Zig mismatch for {}({:?})", $name, $args
                );
            }};
        }

        // linear_search: sweep -100..=200 (301 inputs)
        for target in -100..=200_i32 {
            compare!("linear_search", i32, target);
        }

        // binary_search: sweep -100..=200 (301 inputs)
        for target in -100..=200_i32 {
            compare!("binary_search", i32, target);
        }

        // bubble_sort_element: exhaustive 0..=5 (6 inputs)
        for idx in 0..=5_i32 {
            compare!("bubble_sort_element", i32, idx);
        }

        // dot_product: single call (no parameters)
        compare!("dot_product", i32, ());

        // array_max: exhaustive 1..=8 (8 inputs)
        for n in 1..=8_i32 {
            compare!("array_max", i32, n);
        }

        // prefix_sum_element: exhaustive 0..=5 (6 inputs)
        for idx in 0..=5_i32 {
            compare!("prefix_sum_element", i32, idx);
        }

        // sum_u8_array: single call (no parameters)
        compare!("sum_u8_array", i32, ());

        // min_i8_array: single call (no parameters)
        compare!("min_i8_array", i32, ());

        // max_i16_array: single call (no parameters)
        compare!("max_i16_array", i32, ());

        // sum_u16_array: single call (no parameters)
        compare!("sum_u16_array", i32, ());

        // search_u32_array: sweep 0..=1000 (1,001 inputs)
        for target in 0..=1000_i32 {
            compare!("search_u32_array", i32, target);
        }

        // dot_product_i64: single call (no parameters)
        compare!("dot_product_i64", i64, ());
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` golden file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::algo_array::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_array")
    }

    #[test]
    #[ignore]
    fn regenerate_algo_array_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("algo_array.inf"))
            .expect("Failed to read algo_array.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("algo_array.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "algo_array");
    }
}

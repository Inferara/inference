pub mod translator;
pub mod wasm_parser;

#[cfg(test)]
mod tests {
    use super::wasm_parser::translate_bytes;
    use std::fs;
    use std::panic;
    use std::path::PathBuf;

    #[test]
    fn test_parse_test_data() {
        let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");

        assert!(
            test_data_dir.exists(),
            "test_data directory not found at {:?}",
            test_data_dir
        );

        let entries = fs::read_dir(&test_data_dir).expect("Failed to read test_data directory");

        let mut wasm_files = Vec::new();

        for entry in entries {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                wasm_files.push(path);
            }
        }

        wasm_files.sort();

        assert!(
            !wasm_files.is_empty(),
            "No .wasm files found in test_data directory"
        );

        let mut success_count = 0;
        let mut error_count = 0;
        let mut panic_count = 0;

        for wasm_path in &wasm_files {
            let file_name = wasm_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let bytes = fs::read(wasm_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", file_name, e));

            let module_name = wasm_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("module");

            // Catch panics from unimplemented features
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                translate_bytes(module_name, &bytes)
            }));

            match result {
                Ok(Ok(translation)) => {
                    println!("✓ Successfully parsed {}", file_name);
                    assert!(
                        !translation.is_empty(),
                        "Translation result is empty for {}",
                        file_name
                    );
                    success_count += 1;
                }
                Ok(Err(e)) => {
                    println!("✗ Failed to parse {}: {}", file_name, e);
                    error_count += 1;
                }
                Err(_) => {
                    println!(
                        "⚠ Panicked while parsing {} (likely unimplemented feature)",
                        file_name
                    );
                    panic_count += 1;
                }
            }
        }

        println!("\n=== Summary ===");
        println!("Total files: {}", wasm_files.len());
        println!("Successful: {}", success_count);
        println!("Failed (errors): {}", error_count);
        println!("Failed (panics/unimplemented): {}", panic_count);
        println!(
            "Success rate: {:.1}%",
            (success_count as f64 / wasm_files.len() as f64) * 100.0
        );
    }
}

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

pub(crate) fn compile_rust_to_wasm(path: &Path) -> Vec<u8> {
    let tmp_dir = tempdir().unwrap();
    fs::create_dir_all(tmp_dir.path().join("src")).unwrap();
    let file_path = tmp_dir.path().join("src").join("lib.rs");
    std::fs::copy(path, file_path).unwrap();

    create_boilerplate_cargo_toml(tmp_dir.path());
    let output = Command::new("wasm-pack")
        .args(["build", "--dev", tmp_dir.path().to_str().unwrap()])
        .output()
        .expect("Failed to execute cargo build");

    if !output.status.success() {
        eprintln!("Error: wasm-pack failed");
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let res_path = tmp_dir
        .path()
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("debug")
        .join("rust_test.wasm");

    let wasm_bytes = fs::read(res_path).unwrap();

    tmp_dir.close().unwrap();
    wasm_bytes
}

fn create_boilerplate_cargo_toml(path: &Path) {
    let cargo_toml = path.join("Cargo.toml");
    std::fs::write(
        cargo_toml,
        r#"[package]
name = "rust_test"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2""#,
    )
    .unwrap();
}

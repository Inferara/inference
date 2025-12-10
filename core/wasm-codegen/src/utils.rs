use std::{path::PathBuf, process::Command};

use inkwell::{module::Module, targets::TargetTriple};
use tempfile::tempdir;

pub(crate) fn compile_to_wasm(
    module: &Module,
    output_fname: &str,
    optimization_level: u32,
) -> anyhow::Result<Vec<u8>> {
    let llc_path = get_inf_llc_path()?;
    let temp_dir = tempdir()?;
    let obj_path = temp_dir.path().join(output_fname).with_extension("o");
    let ir_path = temp_dir.path().join(output_fname).with_extension("ll");
    let triple = TargetTriple::create("wasm32-unknown-unknown");
    module.set_triple(&triple);
    let ir_str = module.print_to_string().to_string();
    std::fs::write(&ir_path, ir_str)?;
    let opt_flag = format!("-O{}", optimization_level.min(3));
    let output = Command::new(&llc_path)
        // .arg("-march=wasm32") // same as triple
        .arg("-mcpu=mvp")
        // .arg("-mattr=+mutable-globals") // https://doc.rust-lang.org/beta/rustc/platform-support/wasm32v1-none.html
        .arg("-filetype=obj")
        .arg(&ir_path)
        .arg(&opt_flag)
        .arg("-o")
        .arg(&obj_path)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "inf-llc failed with status: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let wasm_ld_path = get_wasm_ld_path()?;
    let wasm_path = temp_dir.path().join(output_fname).with_extension("wasm");
    let wasm_ld_output = Command::new(&wasm_ld_path)
        .arg("-flavor")
        .arg("wasm")
        .arg(&obj_path)
        .arg("--no-entry")
        // .arg("--export=hello_world")
        .arg("-o")
        .arg(&wasm_path)
        .output()?;

    if !wasm_ld_output.status.success() {
        return Err(anyhow::anyhow!(
            "wasm-ld failed with status: {}\nstderr: {}",
            wasm_ld_output.status,
            String::from_utf8_lossy(&wasm_ld_output.stderr)
        ));
    }

    let wasm_bytes = std::fs::read(&wasm_path)?;
    std::fs::remove_file(obj_path)?;
    Ok(wasm_bytes)
}

pub(crate) fn get_inf_llc_path() -> anyhow::Result<std::path::PathBuf> {
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let llc_name = format!("inf-llc{exe_suffix}");

    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {e}"))?;

    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

    // Try multiple possible locations:
    // 1. For regular binaries: <exe_dir>/bin/llc
    // 2. For test binaries in deps/: <exe_dir>/../bin/llc
    let candidates = vec![
        exe_dir.join("bin").join(&llc_name), // target/debug/bin/llc or target/release/bin/llc
        exe_dir.parent().map_or_else(
            || exe_dir.join("bin").join(&llc_name),
            |p| p.join("bin").join(&llc_name), // target/debug/bin/llc when exe is in target/debug/deps/
        ),
    ];

    for llc_path in &candidates {
        if llc_path.exists() {
            return Ok(llc_path.clone());
        }
    }

    Err(anyhow::anyhow!(
        "🚫 Inference llc binary not found\n\
            \n\
            This package requires LLVM with custom intrinsics support.\n\n\
            Executable: {}\n\
            Searched locations:\n  - {}\n  - {}",
        exe_path.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}

pub(crate) fn get_wasm_ld_path() -> anyhow::Result<PathBuf> {
    let sysroot = String::from_utf8(
        Command::new("rustc")
            .arg("--print")
            .arg("sysroot")
            .output()?
            .stdout,
    )?
    .trim()
    .to_string();

    let rustc_vv = String::from_utf8(Command::new("rustc").arg("-vV").output()?.stdout)?;
    let host_triple = rustc_vv
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| anyhow::anyhow!("failed to parse rustc -vV host triple"))?
        .to_string();

    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let host_ld = PathBuf::from(&sysroot)
        .join("lib")
        .join("rustlib")
        .join(&host_triple)
        .join("bin")
        .join(format!("rust-lld{exe_suffix}"));

    if host_ld.is_file() {
        return Ok(host_ld);
    }

    if let Ok(path) = which::which(format!("rust-lld{exe_suffix}")) {
        return Ok(path);
    }

    if let Ok(path) = which::which(format!("wasm-ld{exe_suffix}")) {
        return Ok(path);
    }

    Err(anyhow::anyhow!(
        "Could not find rust-lld or wasm-ld.\n\
         - rust-lld is usually at <sysroot>/lib/rustlib/<host>/bin/rust-lld\n\
         - or install LLVM lld and ensure `wasm-ld` is on PATH."
    ))
}

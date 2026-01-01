use std::{path::PathBuf, process::Command};

use inkwell::{module::Module, targets::TargetTriple};
use tempfile::tempdir;

#[allow(clippy::similar_names)]
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
    let mut llc_cmd = Command::new(&llc_path);
    configure_llvm_env(&mut llc_cmd)?;
    let output = llc_cmd
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
    let rust_lld_path = get_rust_lld_path()?;
    let wasm_path = temp_dir.path().join(output_fname).with_extension("wasm");
    let mut lld_cmd = Command::new(&rust_lld_path);
    configure_llvm_env(&mut lld_cmd)?;
    let wasm_lld_output = lld_cmd
        .arg("-flavor")
        .arg("wasm")
        .arg(&obj_path)
        .arg("--no-entry")
        // .arg("--export=hello_world")
        .arg("-o")
        .arg(&wasm_path)
        .output()?;

    if !wasm_lld_output.status.success() {
        return Err(anyhow::anyhow!(
            "rust-lld failed with status: {}\nstderr: {}",
            wasm_lld_output.status,
            String::from_utf8_lossy(&wasm_lld_output.stderr)
        ));
    }

    let wasm_bytes = std::fs::read(&wasm_path)?;
    std::fs::remove_file(obj_path)?;
    Ok(wasm_bytes)
}

pub(crate) fn get_inf_llc_path() -> anyhow::Result<std::path::PathBuf> {
    get_bin_path(
        "inf-llc",
        "This package requires LLVM with Inference intrinsics support.",
    )
}

pub(crate) fn get_rust_lld_path() -> anyhow::Result<PathBuf> {
    get_bin_path(
        "rust-lld",
        "This package requires rust-lld to link WebAssembly modules.",
    )
}

fn get_bin_path(bin_name: &str, not_found_message: &str) -> anyhow::Result<PathBuf> {
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let llc_name = format!("{bin_name}{exe_suffix}");

    // First, try the build-time hint if available
    if let Some(bin_dir) = option_env!("INF_WASM_CODEGEN_BIN_DIR") {
        let candidate = PathBuf::from(bin_dir).join(&llc_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

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
        "🚫 {bin_name} binary not found\n\
            \n\
            {not_found_message}\n\n\
            Executable: {}\n\
            Searched locations:\n  - {}\n  - {}",
        exe_path.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}

#[cfg(target_os = "linux")]
fn get_llvm_lib_dir() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {e}"))?;

    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

    let candidates = vec![
        exe_dir.join("bin"), // target/debug/lib or target/release/lib
        exe_dir.parent().map_or_else(
            || exe_dir.join("lib"),
            |p| p.join("lib"), // target/debug/lib when exe is in target/debug/deps/
        ),
    ];

    for lib_path in &candidates {
        if lib_path.exists() {
            return Ok(lib_path.clone());
        }
    }

    Err(anyhow::anyhow!(
        "🚫 LLVM library directory not found\n\
            \n\
            This package requires LLVM shared libraries.\n\n\
            Executable: {}\n\
            Searched locations:\n  - {}\n  - {}",
        exe_path.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}

/// Configure environment variables for spawned LLVM tools to find bundled libraries
#[cfg(target_os = "linux")]
fn configure_llvm_env(cmd: &mut Command) -> anyhow::Result<()> {
    let lib_dir = get_llvm_lib_dir()?;
    let lib_dir_str = lib_dir.to_string_lossy();

    // Prepend to LD_LIBRARY_PATH
    let ld_library_path = if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
        format!("{lib_dir_str}:{existing}")
    } else {
        lib_dir_str.to_string()
    };

    cmd.env("LD_LIBRARY_PATH", ld_library_path);
    Ok(())
}

/// Configure environment for Windows (DLLs are in bin/ next to executables, so no-op)
#[cfg(target_os = "windows")]
#[allow(clippy::unnecessary_wraps)]
fn configure_llvm_env(_cmd: &mut Command) -> anyhow::Result<()> {
    // On Windows, DLLs are placed in the same directory as the executables,
    // so Windows will find them automatically. No environment modification needed.
    Ok(())
}

/// Fallback for unsupported platforms
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn configure_llvm_env(_cmd: &mut Command) -> anyhow::Result<()> {
    Ok(())
}

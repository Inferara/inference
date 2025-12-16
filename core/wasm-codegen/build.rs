use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        panic!("Unsupported platform");
    };

    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let llc_binary = format!("inf-llc{exe_suffix}");
    let rust_lld_binary = format!("rust-lld{exe_suffix}");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent() // core/
        .and_then(|p| p.parent()) // workspace root
        .expect("Failed to determine workspace root");

    let source_llc = workspace_root.join("bin").join(platform).join(&llc_binary);
    let source_rust_lld = workspace_root
        .join("bin")
        .join(platform)
        .join(&rust_lld_binary);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_profile_dir = out_dir
        .parent() // build/<crate-name>-<hash>
        .and_then(|p| p.parent()) // build/
        .and_then(|p| p.parent()) // target/<profile>/
        .expect("Failed to determine target profile directory");

    let bin_dir = target_profile_dir.join("bin");
    let dest_llc = bin_dir.join(&llc_binary);
    let dest_rust_lld = bin_dir.join(&rust_lld_binary);

    if source_llc.exists() {
        if !bin_dir.exists() {
            fs::create_dir_all(&bin_dir).expect("Failed to create bin directory");
        }

        fs::copy(&source_llc, &dest_llc).unwrap_or_else(|e| {
            panic!(
                "Failed to copy inf-llc from {} to {}: {}",
                source_llc.display(),
                dest_llc.display(),
                e
            )
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest_llc)
                .expect("Failed to read inf-llc metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest_llc, perms).expect("Failed to set executable permissions");

            // Set RPATH to look in the same directory as the binary
            set_rpath(&dest_llc);
        }

        println!("cargo:info=Copied inf-llc to {}", dest_llc.display());
    } else {
        println!(
            "cargo:info=inf-llc not found at {}, skipping copy",
            source_llc.display()
        );
    }

    if source_rust_lld.exists() {
        if !bin_dir.exists() {
            fs::create_dir_all(&bin_dir).expect("Failed to create bin directory");
        }

        fs::copy(&source_rust_lld, &dest_rust_lld).unwrap_or_else(|e| {
            panic!(
                "Failed to copy rust-lld from {} to {}: {}",
                source_rust_lld.display(),
                dest_rust_lld.display(),
                e
            )
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest_rust_lld)
                .expect("Failed to read rust-lld metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest_rust_lld, perms)
                .expect("Failed to set executable permissions");

            // Copy LLVM libraries to make rust-lld self-contained
            copy_llvm_libraries(bin_dir);

            // Set RPATH to look in the same directory as the binary
            set_rpath(&dest_rust_lld);
        }

        println!("cargo:info=Copied rust-lld to {}", dest_rust_lld.display());
    } else {
        println!(
            "cargo:info=rust-lld not found at {}, skipping copy",
            source_rust_lld.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source_llc.display());
}

fn copy_llvm_libraries(bin_dir: PathBuf) {
    // Get the Rust toolchain's lib directory
    if let Ok(output) = Command::new("rustc").arg("--print").arg("sysroot").output()
        && output.status.success()
    {
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let lib_dir = PathBuf::from(sysroot).join("lib");

        if lib_dir.exists() {
            // Copy LLVM shared libraries
            if let Ok(entries) = fs::read_dir(&lib_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(file_name) = path.file_name() {
                        let name_str = file_name.to_string_lossy();
                        // Copy LLVM libraries (both .so and .so.version files)
                        if name_str.contains("libLLVM") && name_str.contains(".so") {
                            let dest_path = bin_dir.join(file_name);
                            if let Err(e) = fs::copy(&path, &dest_path) {
                                eprintln!("Warning: Failed to copy {}: {}", path.display(), e);
                            } else {
                                println!(
                                    "cargo:info=Copied LLVM library {} to {}",
                                    path.display(),
                                    dest_path.display()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn set_rpath(binary_path: &PathBuf) {
    // Set RPATH to $ORIGIN so the binary looks for libraries in its own directory
    let _ = Command::new("patchelf")
        .arg("--set-rpath")
        .arg("$ORIGIN")
        .arg(binary_path)
        .output();
}

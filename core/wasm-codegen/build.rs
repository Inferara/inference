use std::env;
use std::fs;
use std::path::PathBuf;

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
    let libllvm_lib = if cfg!(target_os = "windows") {
        "libLLVM.dll"
    } else {
        "libLLVM.so.21.1-rust-1.94.0-nightly"
    };

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent() // core/
        .and_then(|p| p.parent()) // workspace root
        .expect("Failed to determine workspace root");
    let source_bin_dir = workspace_root.join("external").join("bin").join(platform);
    let source_lib_dir = workspace_root.join("external").join("lib").join(platform);

    let source_llc = source_bin_dir.join(&llc_binary);
    let source_rust_lld = source_bin_dir.join(&rust_lld_binary);
    let source_lib_llvm = source_lib_dir.join(libllvm_lib);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_profile_dir = out_dir
        .parent() // build/<crate-name>-<hash>
        .and_then(|p| p.parent()) // build/
        .and_then(|p| p.parent()) // target/<profile>/
        .expect("Failed to determine target profile directory");

    let bin_dir = target_profile_dir.join("bin");
    let lib_dir = target_profile_dir.join("lib");
    let dest_llc = bin_dir.join(&llc_binary);
    let dest_rust_lld = bin_dir.join(&rust_lld_binary);
    let dest_lib_llvm = lib_dir.join(libllvm_lib);

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
        }

        println!("cargo:info=Copied inf-llc to {}", dest_llc.display());
    } else {
        println!(
            "cargo:info=inf-llc not found at {}, skipping copy",
            source_llc.display()
        );
    }

    if source_rust_lld.exists() {
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
        }

        println!("cargo:info=Copied rust-lld to {}", dest_rust_lld.display());
    } else {
        println!(
            "cargo:info=rust-lld not found at {}, skipping copy",
            source_rust_lld.display()
        );
    }

    if source_lib_llvm.exists() {
        if !lib_dir.exists() {
            fs::create_dir_all(&lib_dir).expect("Failed to create lib directory");
        }
        fs::copy(&source_lib_llvm, &dest_lib_llvm).unwrap_or_else(|e| {
            panic!(
                "Failed to copy libLLVM from {} to {}: {}",
                source_lib_llvm.display(),
                dest_lib_llvm.display(),
                e
            )
        });
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest_lib_llvm)
                .expect("Failed to read libLLVM metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest_lib_llvm, perms)
                .expect("Failed to set executable permissions");
        }
        println!("cargo:info=Copied libLLVM to {}", dest_lib_llvm.display());
    } else {
        println!(
            "cargo:info=libLLVM not found at {}, skipping copy",
            source_lib_llvm.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source_llc.display());
    println!("cargo:rerun-if-changed={}", source_rust_lld.display());
    println!("cargo:rerun-if-changed={}", source_lib_llvm.display());
}

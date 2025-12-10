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

    let llc_binary = if cfg!(windows) {
        "inf-llc.exe"
    } else {
        "inf-llc"
    };

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent() // core/
        .and_then(|p| p.parent()) // workspace root
        .expect("Failed to determine workspace root");

    let source_llc = workspace_root.join("lib").join(platform).join(llc_binary);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_profile_dir = out_dir
        .parent() // build/<crate-name>-<hash>
        .and_then(|p| p.parent()) // build/
        .and_then(|p| p.parent()) // target/<profile>/
        .expect("Failed to determine target profile directory");

    let bin_dir = target_profile_dir.join("bin");
    let dest_llc = bin_dir.join(llc_binary);

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

        println!("cargo:warning=Copied inf-llc to {}", dest_llc.display());
    } else {
        println!(
            "cargo:warning=inf-llc not found at {}, skipping copy",
            source_llc.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source_llc.display());
}

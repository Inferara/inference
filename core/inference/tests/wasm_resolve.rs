//! Integration tests for the driver-side `.wasm` module resolver
//! (`inference::wasm_link::resolve`).
//!
//! Resolution precedence, path portability, and the miss diagnostic are
//! exercised here against a real temporary directory tree so that `is_file`
//! probing behaves exactly as it would in a build.

use std::path::{Path, PathBuf};

use inference::wasm_link::resolve::{
    resolve_wasm_module, ManifestDeps, ModulePath, ModulePathError, ResolveError, SearchPath,
};

/// A self-cleaning temporary directory rooted under the OS temp dir.
struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(tag: &str) -> Self {
        let unique = format!(
            "inference-wasm-resolve-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();
        TempTree { root }
    }

    /// Creates an empty file at `relative` (creating parent dirs) and returns it.
    fn touch(&self, relative: impl AsRef<Path>) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, b"\0asm").unwrap();
        path
    }

    /// Creates a subdirectory and returns it.
    fn dir(&self, relative: impl AsRef<Path>) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn module(name: &str) -> ModulePath {
    ModulePath::from_segments(name.split("::")).unwrap()
}

#[test]
fn resolves_from_single_lib_dir() {
    let tree = TempTree::new("single");
    let lib = tree.dir("lib");
    let expected = tree.touch("lib/sorting.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);

    let got = resolve_wasm_module(&module("sorting"), &search, None).unwrap();
    assert_eq!(got, expected);
}

#[test]
fn maps_colon_path_to_nested_file() {
    let tree = TempTree::new("nested");
    let lib = tree.dir("lib");
    let expected = tree.touch("lib/crypto/sha256.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);

    let got = resolve_wasm_module(&module("crypto::sha256"), &search, None).unwrap();
    assert_eq!(got, expected);
}

#[test]
fn relative_path_uses_host_separator_not_literal_slash() {
    // Portability: the logical name must map onto nested path *components*, never
    // a single segment containing a literal separator. Asserting on components
    // makes the test pass identically on Windows, macOS, and Linux.
    let relative = module("crypto::sha256").to_relative_path();
    let components: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(components, ["crypto", "sha256.wasm"]);
}

#[test]
fn lib_dir_precedes_env_dir() {
    // The same logical module exists in both a `-L` dir and an env dir; the
    // `-L` hit must win because it is pushed first.
    let tree = TempTree::new("precedence-lib-env");
    let lib = tree.dir("lib");
    let env = tree.dir("env");
    let lib_hit = tree.touch("lib/sorting.wasm");
    let _env_hit = tree.touch("env/sorting.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);
    search.push_env_dir(&env);

    let got = resolve_wasm_module(&module("sorting"), &search, None).unwrap();
    assert_eq!(got, lib_hit);
}

#[test]
fn falls_back_to_env_dir_when_lib_dir_misses() {
    let tree = TempTree::new("env-fallback");
    let lib = tree.dir("lib");
    let env = tree.dir("env");
    let env_hit = tree.touch("env/sorting.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);
    search.push_env_dir(&env);

    let got = resolve_wasm_module(&module("sorting"), &search, None).unwrap();
    assert_eq!(got, env_hit);
}

#[test]
fn manifest_precedes_search_path() {
    // A manifest entry must beat any directory hit, even when both exist.
    let tree = TempTree::new("precedence-manifest");
    let lib = tree.dir("lib");
    let _lib_hit = tree.touch("lib/sorting.wasm");
    let manifest_target = tree.touch("vendor/sorting-1.2.3.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);

    let mut manifest = ManifestDeps::new();
    manifest.insert("sorting", &manifest_target);

    let got = resolve_wasm_module(&module("sorting"), &search, Some(&manifest)).unwrap();
    assert_eq!(got, manifest_target);
}

#[test]
fn manifest_beats_lib_dir_beats_env_dir() {
    // The full Phase-5 precedence chain in one shot: the same logical module is
    // available from the manifest, a `-L` directory, and an env directory. The
    // manifest entry must win over both, and `-L` must win over env.
    let tree = TempTree::new("precedence-three-way");
    let lib = tree.dir("lib");
    let env = tree.dir("env");
    let manifest_target = tree.touch("vendor/sorting.wasm");
    let lib_hit = tree.touch("lib/sorting.wasm");
    let env_hit = tree.touch("env/sorting.wasm");

    // 1. Manifest present: it wins over everything.
    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);
    search.push_env_dir(&env);
    let mut manifest = ManifestDeps::new();
    manifest.insert("sorting", &manifest_target);
    let got = resolve_wasm_module(&module("sorting"), &search, Some(&manifest)).unwrap();
    assert_eq!(got, manifest_target, "manifest entry must take priority");

    // 2. No manifest: `-L` wins over env.
    let got = resolve_wasm_module(&module("sorting"), &search, None).unwrap();
    assert_eq!(got, lib_hit, "`-L` directory must beat env directory");

    // 3. No manifest, `-L` misses: env is the fallback.
    let mut env_only = SearchPath::new();
    env_only.push_env_dir(&env);
    let got = resolve_wasm_module(&module("sorting"), &env_only, None).unwrap();
    assert_eq!(got, env_hit, "env directory is the last resort");
}

#[test]
fn manifest_path_missing_is_a_distinct_error() {
    let tree = TempTree::new("manifest-missing");
    let lib = tree.dir("lib");
    // A directory hit exists, but the manifest takes priority and points nowhere.
    let _lib_hit = tree.touch("lib/sorting.wasm");
    let bogus = tree.root.join("vendor").join("does-not-exist.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);

    let mut manifest = ManifestDeps::new();
    manifest.insert("sorting", &bogus);

    let err = resolve_wasm_module(&module("sorting"), &search, Some(&manifest)).unwrap_err();
    match err {
        ResolveError::ManifestPathMissing { logical_name, path } => {
            assert_eq!(logical_name, "sorting");
            assert_eq!(path, bogus);
        }
        other => panic!("expected ManifestPathMissing, got {other:?}"),
    }
}

#[test]
fn unmatched_manifest_entry_falls_through_to_search_path() {
    // The manifest carries a *different* module; resolution should ignore it and
    // fall through to the search directories for the requested name.
    let tree = TempTree::new("manifest-unmatched");
    let lib = tree.dir("lib");
    let expected = tree.touch("lib/sorting.wasm");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);

    let mut manifest = ManifestDeps::new();
    manifest.insert("other", tree.root.join("other.wasm"));

    let got = resolve_wasm_module(&module("sorting"), &search, Some(&manifest)).unwrap();
    assert_eq!(got, expected);
}

#[test]
fn miss_lists_every_searched_location_in_order() {
    let tree = TempTree::new("miss");
    let lib = tree.dir("lib");
    let env = tree.dir("env");

    let mut search = SearchPath::new();
    search.push_lib_dir(&lib);
    search.push_env_dir(&env);

    let err = resolve_wasm_module(&module("crypto::sha256"), &search, None).unwrap_err();
    // The rendered diagnostic should name the logical module and the probed paths.
    let rendered = err.to_string();
    assert!(rendered.contains("crypto::sha256"));
    assert!(rendered.contains("sha256.wasm"));
    match err {
        ResolveError::NotFound {
            logical_name,
            searched,
        } => {
            assert_eq!(logical_name, "crypto::sha256");
            assert_eq!(
                searched,
                vec![
                    lib.join("crypto").join("sha256.wasm"),
                    env.join("crypto").join("sha256.wasm"),
                ]
            );
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn empty_search_path_miss_reports_no_directories() {
    let err = resolve_wasm_module(&module("sorting"), &SearchPath::new(), None).unwrap_err();
    let rendered = err.to_string();
    assert!(rendered.contains("no search directories"));
}

#[test]
fn module_path_rejects_empty_reference() {
    let err = ModulePath::from_segments(Vec::<String>::new()).unwrap_err();
    assert_eq!(err, ModulePathError::Empty);
}

#[test]
fn module_path_rejects_separator_bearing_segment() {
    // A segment must never smuggle a path separator; that would let source
    // escape the search directory — exactly the portability hole we close.
    for bad in ["a/b", "a\\b", "..", "."] {
        let err = ModulePath::from_segments([bad]).unwrap_err();
        assert!(
            matches!(err, ModulePathError::InvalidSegment(_)),
            "segment {bad:?} should be rejected, got {err:?}"
        );
    }
}

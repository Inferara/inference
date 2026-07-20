//! Shared helpers for the feature tests: build an [`AnalysisHost`] over in-memory
//! documents and compute byte offsets from the source text (never hardcoded).

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;

use crate::AnalysisHost;

/// The synthetic source root every test document lives under. A real path is
/// needed so import resolution has a directory to resolve siblings against; the
/// overlay shadows disk, so nothing is ever read from the filesystem.
const ROOT: &str = "/inf-test";

/// The absolute path a module lives at: the entry is `main.inf`, an imported
/// module `lib` is `lib.inf`, both under [`ROOT`].
#[must_use]
pub(crate) fn module_path(name: &str) -> PathBuf {
    PathBuf::from(format!("{ROOT}/{name}.inf"))
}

/// A host with a single open entry document, plus that document's path.
#[must_use]
pub(crate) fn single(source: &str) -> (AnalysisHost, PathBuf) {
    let mut host = AnalysisHost::default();
    let path = module_path("main");
    host.open_document(&path, source);
    (host, path)
}

/// A host with an open entry document plus one open imported sibling `lib.inf`,
/// and the entry's path. The entry should `use lib;` to pull the sibling in.
#[must_use]
pub(crate) fn with_lib(entry: &str, lib: &str) -> (AnalysisHost, PathBuf) {
    let mut host = AnalysisHost::default();
    let entry_path = module_path("main");
    host.open_document(&module_path("lib"), lib);
    host.open_document(&entry_path, entry);
    (host, entry_path)
}

/// The absolute path of a (possibly nested) module: the last segment names the
/// file (`<leaf>.inf`), the earlier segments its parent directories under
/// [`ROOT`], so `["lib", "geom"]` is `<ROOT>/lib/geom.inf`.
#[must_use]
pub(crate) fn nested_module_path(segments: &[&str]) -> PathBuf {
    let (leaf, dirs) = segments.split_last().expect("at least one segment");
    let mut path = PathBuf::from(ROOT);
    for dir in dirs {
        path.push(dir);
    }
    path.push(format!("{leaf}.inf"));
    path
}

/// A host over several open documents, each named by its module-path segments and
/// carrying its source. The entry document must be `["main"]`; its path is
/// returned. Analysis is lazy, so open order does not matter.
#[must_use]
pub(crate) fn project(files: &[(&[&str], &str)]) -> (AnalysisHost, PathBuf) {
    let mut host = AnalysisHost::default();
    for &(segments, source) in files {
        host.open_document(&nested_module_path(segments), source);
    }
    (host, nested_module_path(&["main"]))
}

/// The byte offset of the first occurrence of `needle` in `source`.
#[must_use]
pub(crate) fn at(source: &str, needle: &str) -> u32 {
    source.find(needle).expect("needle present in source") as u32
}

/// The byte offset just past the first occurrence of `needle` in `source`.
#[must_use]
pub(crate) fn after(source: &str, needle: &str) -> u32 {
    at(source, needle) + needle.len() as u32
}

/// The byte offset of the `n`-th (0-based) occurrence of `needle` in `source`.
#[must_use]
pub(crate) fn nth(source: &str, needle: &str, n: usize) -> u32 {
    source
        .match_indices(needle)
        .nth(n)
        .expect("nth needle present in source")
        .0 as u32
}

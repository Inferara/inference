//! Platform-independent resolution of a logical module reference to a `.wasm`
//! file on disk.
//!
//! Source never names a filesystem path (no `./`, no OS separators); it names a
//! *logical* module — a `::`-separated identifier path mirrored by
//! [`inference_ast::nodes::ModuleRef`]. This module turns that logical name into
//! a concrete [`PathBuf`] by searching, in priority order:
//!
//! 1. **manifest** dependency entries (`Inference.toml [wasm-dependencies]`,
//!    delivered fully in a later phase — accepted here as a stub map so the
//!    precedence is wired from the start),
//! 2. **`-L` / `--wasm-lib-dir`** directories,
//! 3. **`INFERENCE_*`** environment directories.
//!
//! The `-L` and environment directories arrive already concatenated in
//! [`SearchPath::dirs`] in exactly that order, so the resolver walks them
//! front-to-back. A logical name `a::b` maps to the relative path `a/b.wasm`
//! using [`std::path::MAIN_SEPARATOR`] (via [`Path::join`]) at resolve time, so
//! the same source resolves identically on every operating system.

use std::path::{Path, PathBuf};

use inference_ast::arena::AstArena;
use inference_ast::nodes::ModuleRef;
use rustc_hash::FxHashMap;

/// File extension of a compiled WebAssembly module.
const WASM_EXTENSION: &str = "wasm";

/// A logical, platform-independent module name as a sequence of identifier
/// segments (e.g. `crypto::sha256` → `["crypto", "sha256"]`).
///
/// The segments are validated to be non-empty and free of path separators at
/// construction, so mapping them onto a [`Path`] can never escape the search
/// directory or smuggle in an OS separator.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModulePath {
    segments: Vec<String>,
}

/// Reason a logical name could not be turned into a [`ModulePath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModulePathError {
    /// The reference had no segments at all.
    Empty,
    /// A segment was empty or contained a path separator / `.` component.
    InvalidSegment(String),
}

impl std::fmt::Display for ModulePathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModulePathError::Empty => write!(f, "module reference has no path segments"),
            ModulePathError::InvalidSegment(seg) => {
                write!(f, "invalid module path segment `{seg}`")
            }
        }
    }
}

impl std::error::Error for ModulePathError {}

impl ModulePath {
    /// Builds a [`ModulePath`] from already-owned segment strings, validating each.
    ///
    /// # Errors
    ///
    /// Returns [`ModulePathError::Empty`] if there are no segments, or
    /// [`ModulePathError::InvalidSegment`] if a segment is empty, a `.`/`..`
    /// component, or contains a path separator.
    pub fn from_segments<I, S>(segments: I) -> Result<Self, ModulePathError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        if segments.is_empty() {
            return Err(ModulePathError::Empty);
        }
        for segment in &segments {
            if segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.contains('/')
                || segment.contains('\\')
            {
                return Err(ModulePathError::InvalidSegment(segment.clone()));
            }
        }
        Ok(ModulePath { segments })
    }

    /// Builds a [`ModulePath`] from a parsed [`ModuleRef`], resolving each
    /// identifier index against `arena`.
    ///
    /// # Errors
    ///
    /// Propagates the validation errors of [`ModulePath::from_segments`].
    pub fn from_module_ref(
        module_ref: &ModuleRef,
        arena: &AstArena,
    ) -> Result<Self, ModulePathError> {
        Self::from_segments(module_ref.segments.iter().map(|&id| arena.ident_name(id)))
    }

    /// The logical name in `a::b` form, for diagnostics.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.segments.join("::")
    }

    /// The relative filesystem path this logical name maps to, e.g.
    /// `crypto::sha256` → `crypto/sha256.wasm` (with the host separator).
    ///
    /// Built exclusively through [`Path::join`] / [`Path::with_extension`], so no
    /// literal separator ever appears in source or here.
    #[must_use]
    pub fn to_relative_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for segment in &self.segments {
            path.push(segment);
        }
        path.with_extension(WASM_EXTENSION)
    }
}

/// Ordered search directories for the resolver. `-L` directories precede
/// `INFERENCE_*` environment directories; callers assemble them in that order.
#[derive(Debug, Default, Clone)]
pub struct SearchPath {
    dirs: Vec<PathBuf>,
}

impl SearchPath {
    /// Creates an empty search path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a `-L` / `--wasm-lib-dir` directory (highest of the directory tiers).
    ///
    /// An empty path is dropped: a bare `dir.join(relative)` against an empty
    /// directory resolves against the process CWD, silently turning the build
    /// directory into a `.wasm` search root.
    pub fn push_lib_dir(&mut self, dir: impl Into<PathBuf>) {
        let dir = dir.into();
        if dir.as_os_str().is_empty() {
            return;
        }
        self.dirs.push(dir);
    }

    /// Appends an `INFERENCE_*` environment directory (lowest tier).
    ///
    /// An empty path is dropped, for the same reason as [`Self::push_lib_dir`].
    pub fn push_env_dir(&mut self, dir: impl Into<PathBuf>) {
        let dir = dir.into();
        if dir.as_os_str().is_empty() {
            return;
        }
        self.dirs.push(dir);
    }

    /// The directories in resolution order.
    #[must_use]
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
}

/// Manifest-declared `.wasm` dependencies (`Inference.toml [wasm-dependencies]`).
///
/// Phase 0 accepts this as a plain logical-name → file map so the resolver's
/// precedence is exercised; the manifest *producer* lands in a later phase.
#[derive(Debug, Default, Clone)]
pub struct ManifestDeps {
    entries: FxHashMap<String, PathBuf>,
}

impl ManifestDeps {
    /// Creates an empty manifest dependency set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares that logical `name` resolves to `path`.
    pub fn insert(&mut self, name: impl Into<String>, path: impl Into<PathBuf>) {
        self.entries.insert(name.into(), path.into());
    }

    /// The manifest entry for `name`, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Path> {
        self.entries.get(name).map(PathBuf::as_path)
    }
}

/// Failure to resolve a logical module reference to a `.wasm` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No candidate path existed under any searched location.
    NotFound {
        /// The logical name in `a::b` form.
        logical_name: String,
        /// Every absolute/relative candidate that was probed, in order.
        searched: Vec<PathBuf>,
    },
    /// The manifest named a path that does not exist on disk.
    ManifestPathMissing {
        /// The logical name in `a::b` form.
        logical_name: String,
        /// The path the manifest pointed at.
        path: PathBuf,
    },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotFound {
                logical_name,
                searched,
            } => {
                writeln!(
                    f,
                    "could not resolve module `{logical_name}` to a `.wasm` file"
                )?;
                if searched.is_empty() {
                    write!(f, "  (no search directories were configured)")
                } else {
                    writeln!(f, "  searched the following locations:")?;
                    for (i, path) in searched.iter().enumerate() {
                        let last = i + 1 == searched.len();
                        if last {
                            write!(f, "    - {}", path.display())?;
                        } else {
                            writeln!(f, "    - {}", path.display())?;
                        }
                    }
                    Ok(())
                }
            }
            ResolveError::ManifestPathMissing { logical_name, path } => {
                write!(
                    f,
                    "manifest declares module `{logical_name}` at `{}`, but no file exists there",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Resolves a logical module reference to a concrete `.wasm` file.
///
/// Order: `manifest_deps` (if given) → `search_path` directories (`-L` then env).
/// The logical name `a::b` maps to the relative path `a/b.wasm` under each
/// directory, using the host separator via [`Path::join`].
///
/// # Errors
///
/// Returns [`ResolveError::ManifestPathMissing`] when the manifest names a file
/// that does not exist, and [`ResolveError::NotFound`] when no candidate exists
/// under any searched location (the error lists every probed path).
pub fn resolve_wasm_module(
    logical_name: &ModulePath,
    search_path: &SearchPath,
    manifest_deps: Option<&ManifestDeps>,
) -> Result<PathBuf, ResolveError> {
    if let Some(path) = manifest_deps.and_then(|m| m.get(&logical_name.display_name())) {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        return Err(ResolveError::ManifestPathMissing {
            logical_name: logical_name.display_name(),
            path: path.to_path_buf(),
        });
    }

    let relative = logical_name.to_relative_path();
    let mut searched = Vec::with_capacity(search_path.dirs().len());
    for dir in search_path.dirs() {
        let candidate = dir.join(&relative);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(candidate);
    }

    Err(ResolveError::NotFound {
        logical_name: logical_name.display_name(),
        searched,
    })
}

#[cfg(test)]
mod tests {
    //! Unit tests for module-path construction from the AST and the error
    //! diagnostics' rendered form. The resolution-precedence behaviour itself is
    //! covered by the integration suite in `tests/wasm_resolve.rs`; these focus
    //! on the AST bridge and the `Display` rendering the integration tests reach
    //! only partially.

    use super::*;
    use inference_ast::nodes::Directive;

    /// Parses `source` and returns the `ModuleRef` of its first `use … from …;`.
    fn first_module_ref(source: &str) -> (inference_ast::arena::AstArena, ModuleRef) {
        let arena = crate::parse(source).expect("source parses");
        let module_ref = arena
            .source_files()
            .flat_map(|file| file.directives.iter())
            .find_map(|directive| {
                let Directive::Use(use_dir) = directive;
                use_dir.from.clone()
            })
            .expect("a `use … from …;` directive");
        (arena, module_ref)
    }

    #[test]
    fn module_path_from_a_parsed_use_directive() {
        let (arena, module_ref) = first_module_ref(
            "external fn hash(a: i32) -> i32;\n\
             use { hash } from crypto::sha256;",
        );
        let path = ModulePath::from_module_ref(&module_ref, &arena).expect("valid module ref");
        assert_eq!(path.display_name(), "crypto::sha256");

        let components: Vec<_> = path
            .to_relative_path()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            components,
            ["crypto", "sha256.wasm"],
            "the relative path uses host separators, never a literal slash"
        );
    }

    #[test]
    fn single_segment_use_directive_maps_to_a_flat_file() {
        let (arena, module_ref) = first_module_ref(
            "external fn sum(a: i32) -> i32;\n\
             use { sum } from arith;",
        );
        let path = ModulePath::from_module_ref(&module_ref, &arena).expect("valid module ref");
        assert_eq!(path.display_name(), "arith");
        assert_eq!(path.to_relative_path(), PathBuf::from("arith.wasm"));
    }

    #[test]
    fn module_path_error_display_renders_both_variants() {
        assert!(ModulePathError::Empty
            .to_string()
            .contains("no path segments"));
        assert!(ModulePathError::InvalidSegment("a/b".into())
            .to_string()
            .contains("a/b"));
    }

    #[test]
    fn not_found_display_lists_every_searched_location() {
        let rendered = ResolveError::NotFound {
            logical_name: "crypto::sha256".into(),
            searched: vec![
                PathBuf::from("lib").join("crypto").join("sha256.wasm"),
                PathBuf::from("env").join("crypto").join("sha256.wasm"),
            ],
        }
        .to_string();
        assert!(rendered.contains("crypto::sha256"), "names the module");
        assert!(rendered.contains("searched the following locations"), "{rendered}");
        // Both probed paths appear, each on its own line.
        let lib_line = format!("{}", Path::new("lib").join("crypto").join("sha256.wasm").display());
        let env_line = format!("{}", Path::new("env").join("crypto").join("sha256.wasm").display());
        assert!(rendered.contains(&lib_line), "lists first path: {rendered}");
        assert!(rendered.contains(&env_line), "lists last path: {rendered}");
    }

    #[test]
    fn empty_search_dirs_are_dropped() {
        // An empty `PathBuf` from a stray separator in `INFERENCE_WASM_LIB_PATH`
        // (or an empty `-L`) must never become a search root: `dir.join(rel)`
        // against an empty dir resolves relative to the process CWD. Both push
        // entry points drop it, so a path built only from empty entries searches
        // nothing — identical to no directories being configured at all.
        let mut search = SearchPath::new();
        search.push_env_dir(PathBuf::new());
        search.push_lib_dir(PathBuf::from(""));
        assert!(
            search.dirs().is_empty(),
            "empty directory entries must be dropped, got {:?}",
            search.dirs()
        );

        let module = ModulePath::from_segments(["arith"]).unwrap();
        let err = resolve_wasm_module(&module, &search, None).unwrap_err();
        let ResolveError::NotFound { searched, .. } = err else {
            panic!("expected NotFound, got {err:?}");
        };
        assert!(
            searched.is_empty(),
            "an all-empty search path probes nothing, like an unset path"
        );
    }

    #[test]
    fn non_empty_dirs_are_kept_alongside_dropped_empties() {
        // A real directory survives even when interleaved with empty entries,
        // mirroring `"/real/dir:"` splitting into `["/real/dir", ""]`.
        let mut search = SearchPath::new();
        search.push_env_dir(PathBuf::new());
        search.push_env_dir(PathBuf::from("real"));
        search.push_env_dir(PathBuf::new());
        assert_eq!(search.dirs(), [PathBuf::from("real")]);
    }

    #[test]
    fn manifest_path_missing_display_names_the_module_and_path() {
        let path = PathBuf::from("vendor").join("missing.wasm");
        let rendered = ResolveError::ManifestPathMissing {
            logical_name: "sorting".into(),
            path: path.clone(),
        }
        .to_string();
        assert!(rendered.contains("sorting"), "names the module: {rendered}");
        assert!(
            rendered.contains(&path.display().to_string()),
            "names the declared path: {rendered}"
        );
    }
}

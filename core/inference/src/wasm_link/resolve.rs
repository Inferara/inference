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

use inference_type_checker::HOST_SEGMENT;
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

/// What to check when a name that *is* meant to be a linked module does not
/// resolve, stated before the host clause below.
///
/// The ordinary cause of this miss is a misspelling or a search path that was
/// never configured, and a bare list of probed paths names neither: the list
/// says where the resolver looked, and the branch that shows no list at all is
/// precisely the one whose author forgot `-L`. Leaving those unsaid would make
/// the host clause the only actionable sentence in the message — and the host
/// clause is the wrong fix for the common case, now doubly so: taking it no
/// longer fails, it *resolves*, so an author who mistypes a linked module and
/// follows the only advice on offer ships an artifact importing the typo and
/// hears about it from the embedder.
///
/// What it does *not* do is re-spell the file the resolver was looking for.
/// Every probed path in the list above is already a whole candidate —
/// `resolve_wasm_module` pushes `dir.join(relative)`, not `dir` — so the
/// mapping is on the page, and printing it again beside a list of paths that
/// end in it would describe the list as a list of directories, which it is not:
/// a reader following `lib/crypto/sha256.wasm` plus "`crypto::sha256` is looked
/// up as `crypto/sha256.wasm` under each location above" concludes that
/// `lib/crypto/sha256.wasm/crypto/sha256.wasm` was probed. What is left is the
/// actionable half, and it is the half that holds for every name: the three
/// places the spelling has to agree, which the reader can compare against the
/// list themselves.
fn linked_module_next_step(logical_name: &str, any_location_searched: bool) -> String {
    if !any_location_searched {
        return format!(
            "Pass `-L <dir>` to name a directory to look in, or bind this module straight to a \
             file with `--wasm-dep {logical_name}=<path>` or a `[wasm-dependencies]` entry."
        );
    }
    format!(
        "Check the spelling of `{logical_name}` against the file on disk, and against any \
         `[wasm-dependencies]` key or `--wasm-dep` name meant to bind it."
    )
}

/// Why a miss can survive a correct search path, and the clause that satisfies
/// it without one.
///
/// The resolver's only currency is a file: a linked external is a `.wasm`
/// module read off disk and merged into the artifact. An author who meant the
/// function to be supplied by whatever runs the program wrote the wrong clause,
/// and a list of probed paths on its own sends them hunting for a file that was
/// never going to exist.
///
/// Specialised on the shape of the name, because only one shape has a fix that
/// can be written out. A host module is a single segment — it is the string an
/// embedder registers, not a path — so a one-segment miss can be quoted back as
/// the clause that would have worked. A `::`-joined name has no single host
/// module to name, and inventing one would be a guess at which segment the
/// author meant, so that branch teaches the form and leaves the module to them.
///
/// Both branches carry the all-or-nothing rule, because this miss can only be
/// reached from a program that already binds at least one linked module — the
/// driver refuses a mixed program ahead of every lookup — so an author with a
/// second extern who follows the advice as written lands in that refusal. A
/// sentence naming the restriction costs one clause; discovering it costs a
/// build.
fn embedder_supplied_next_step(logical_name: &str) -> String {
    const ALL_OR_NOTHING: &str =
        "Today a program's externs must all be host imports or all be linked modules, so this \
         works only if every other `use … from` clause is bound the same way.";
    let mut segments = logical_name.split("::");
    match (segments.next(), segments.next()) {
        (Some(single), None) => format!(
            "If `{single}` is meant to be supplied by the embedder at run time rather than by a \
             `.wasm` file, bind it with `use {{ … }} from {HOST_SEGMENT}::{single};` instead. \
             {ALL_OR_NOTHING}"
        ),
        _ => format!(
            "If these functions are meant to be supplied by the embedder at run time rather than \
             by a `.wasm` file, bind them with `use {{ … }} from {HOST_SEGMENT}::<module>;` \
             instead. {ALL_OR_NOTHING}"
        ),
    }
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
                    writeln!(f, "  (no search directories were configured)")?;
                } else {
                    writeln!(f, "  searched the following locations:")?;
                    for path in searched {
                        writeln!(f, "    - {}", path.display())?;
                    }
                }
                writeln!(
                    f,
                    "  {}",
                    linked_module_next_step(logical_name, !searched.is_empty())
                )?;
                write!(f, "  {}", embedder_supplied_next_step(logical_name))
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

    /// The `::` segments of the first `use … from …;` clause in `source`, the
    /// spelling a caller hands to [`ModulePath::from_segments`].
    fn first_module_segments(source: &str) -> Vec<String> {
        let arena = crate::parse(source).expect("source parses");
        let module_ref = arena
            .source_files()
            .flat_map(|file| file.directives.iter())
            .find_map(|directive| {
                let Directive::Use(use_dir) = directive;
                use_dir.from.clone()
            })
            .expect("a `use … from …;` directive");
        module_ref
            .segments
            .iter()
            .map(|&id| arena.ident_name(id).to_string())
            .collect()
    }

    #[test]
    fn module_path_from_a_parsed_use_directive() {
        let segments = first_module_segments(
            "external fn hash(a: i32) -> i32;\n\
             use { hash } from crypto::sha256;",
        );
        let path = ModulePath::from_segments(segments).expect("a parsed clause is a valid path");
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
        let segments = first_module_segments(
            "external fn sum(a: i32) -> i32;\n\
             use { sum } from arith;",
        );
        let path = ModulePath::from_segments(segments).expect("a parsed clause is a valid path");
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

    /// The advice never re-spells the file the resolver looked for, whatever the
    /// logical name is.
    ///
    /// Each entry of the probed list is already a whole candidate path, ending
    /// in that spelling, so a sentence quoting it again describes the list as a
    /// list of directories: a reader shown `lib/crypto/sha256.wasm` and told the
    /// name "is looked up as `crypto/sha256.wasm` under each location above"
    /// concludes the resolver probed `lib/crypto/sha256.wasm/crypto/sha256.wasm`
    /// and goes looking for a mistake in the resolver.
    ///
    /// Both a well-formed name and one that is not a module path at all are
    /// rendered, because `ResolveError`'s fields are public and nothing stops
    /// either arriving, and the second is the one that would have crashed a
    /// rendering that derived a file name to print.
    #[test]
    fn not_found_display_never_re_spells_the_file_it_looked_for() {
        for (logical_name, expected) in [
            ("crypto::sha256", "Check the spelling of `crypto::sha256`"),
            ("", "Check the spelling of ``"),
        ] {
            let rendered = ResolveError::NotFound {
                logical_name: logical_name.to_string(),
                searched: vec![PathBuf::from("lib").join("crypto").join("sha256.wasm")],
            }
            .to_string();
            assert!(
                rendered.contains(expected),
                "the advice names what to check, in the spelling the author wrote: {rendered}"
            );
            assert!(
                !rendered.contains("is looked up as"),
                "a candidate file printed beside a list of candidate paths reads as a path \
                 joined onto each of them: {rendered}"
            );
        }
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

    /// Both shapes of a not-found miss end by naming the clause that would have
    /// satisfied an extern nothing on disk provides, and a single-segment miss
    /// names the module in it.
    ///
    /// The two branches print entirely separate bodies — a list of probed paths,
    /// or a note that nothing was probed — so a pointer appended inside one of
    /// them reaches only half the users who need it, and the half that
    /// configured no search path at all is the half most likely to have meant
    /// the embedder. Asserting on the sentence rather than on a keyword is what
    /// makes this fail if the advice survives in form while the explanation is
    /// dropped or reworded past recognition; asserting that it comes last is
    /// what makes it fail if the sentence is hoisted above the body it is the
    /// next step after, where a reader who stops at the list of paths never
    /// reaches it.
    ///
    /// The one-segment assertion is the half that can go wrong silently. A
    /// generic sentence is right for every miss and so never looks broken, and
    /// it is exactly what a reader whose module *is* a host module does not
    /// need: the clause they should have written can be spelled out in full,
    /// and printing `<module>` at them instead buries the one fix under a
    /// template.
    ///
    /// Every rendering also states the all-or-nothing rule, and that assertion
    /// is what keeps the advice from being a round trip. This miss implies at
    /// least one *linked* binding — it is raised from nowhere else — so an
    /// author with a second extern who takes the advice as written is met by the
    /// driver's mixed-program refusal, and a sentence that is true only of
    /// single-extern programs looks correct in every fixture.
    ///
    /// The ordinary remedy is asserted to come *first* in both branches, and
    /// that ordering is the substance rather than the presentation. The common
    /// cause of this miss is a typo or a missing `-L`, and the host clause is
    /// the wrong fix for both — a fix that no longer merely fails, since a host
    /// binding now resolves with nothing probed, so an author who reads only as
    /// far as the first instruction and takes it ships the typo as an import.
    ///
    /// That remedy is asserted on the three places a spelling has to agree and
    /// never on a candidate file name. The probed list is already a list of
    /// whole candidate paths, so the spelling a typo is compared against is on
    /// the page; a sentence re-spelling it beside them would read as a path
    /// joined onto each, which
    /// [`not_found_display_never_re_spells_the_file_it_looked_for`] is the
    /// standing guard against.
    #[test]
    fn not_found_display_points_at_the_host_clause_in_both_branches() {
        const PHRASE: &str = "by the embedder at run time";
        const ALL_OR_NOTHING: &str = "must all be host imports or all be linked modules";
        const NO_LOCATION: &str = "Pass `-L <dir>` to name a directory to look in";

        let with_paths = ResolveError::NotFound {
            logical_name: "crypto::sha256".into(),
            searched: vec![PathBuf::from("lib").join("crypto").join("sha256.wasm")],
        }
        .to_string();
        assert!(with_paths.contains(PHRASE), "{with_paths}");
        const CHECK_SPELLING: &str = "Check the spelling of `crypto::sha256`";
        assert!(
            with_paths.contains(CHECK_SPELLING),
            "the ordinary cause is named, or a list of paths is the only thing on the page and \
             the host clause is the only instruction: {with_paths}"
        );
        assert!(
            with_paths.contains("`[wasm-dependencies]` key or `--wasm-dep` name"),
            "a spelling has three places to agree, and a manifest key is the one an `infs` \
             project author never typed as a flag: {with_paths}"
        );
        assert!(
            with_paths.find(CHECK_SPELLING) < with_paths.find(PHRASE),
            "the ordinary cause is offered before the host clause, which is the wrong fix for it \
             and no longer fails when taken: {with_paths}"
        );
        assert!(
            with_paths.contains("from host::<module>;"),
            "a multi-segment name has no single host module to name, so the form is taught \
             instead: {with_paths}"
        );
        assert!(
            with_paths.contains("searched the following locations"),
            "the probed-path list still renders beside the pointer: {with_paths}"
        );
        assert!(
            with_paths.find(PHRASE) > with_paths.find("searched the following locations"),
            "the pointer follows the list it is the next step after: {with_paths}"
        );
        assert!(
            with_paths.contains(ALL_OR_NOTHING),
            "the advice states the restriction that makes it conditional, or it sends a \
             multi-extern program into the mixed-program refusal: {with_paths}"
        );

        let without_paths = ResolveError::NotFound {
            logical_name: "crypto::sha256".into(),
            searched: Vec::new(),
        }
        .to_string();
        assert!(without_paths.contains(PHRASE), "{without_paths}");
        assert!(
            without_paths.contains(NO_LOCATION)
                && without_paths.contains("`-L <dir>`")
                && without_paths.contains("`--wasm-dep crypto::sha256=<path>`")
                && without_paths.contains("`[wasm-dependencies]` entry"),
            "the branch an author with a forgotten `-L` sees must name every way to configure \
             one, the manifest key included — its sibling names all three, and an author whose \
             build is driven by `infs` reaches `infc` without typing a flag at all: \
             {without_paths}"
        );
        assert!(
            without_paths.find(NO_LOCATION) < without_paths.find(PHRASE),
            "the ordinary cause leads here too, and here it is the likelier one: {without_paths}"
        );
        assert!(
            without_paths.contains("(no search directories were configured)"),
            "the no-directories line still renders beside the pointer: {without_paths}"
        );
        assert!(
            without_paths.find(PHRASE)
                > without_paths.find("(no search directories were configured)"),
            "the pointer follows the line it is the next step after: {without_paths}"
        );
        assert!(
            without_paths.contains(ALL_OR_NOTHING),
            "the restriction reaches the branch that renders no paths as well: {without_paths}"
        );

        let single = ResolveError::NotFound {
            logical_name: "env".into(),
            searched: vec![PathBuf::from("lib").join("env.wasm")],
        }
        .to_string();
        assert!(
            single.contains("`use { … } from host::env;`"),
            "a one-segment miss is quoted back as the clause that would have worked, not as a \
             template: {single}"
        );
        assert!(
            !single.contains("<module>"),
            "the placeholder must not survive where the real module is known: {single}"
        );
        assert!(
            single.contains(ALL_OR_NOTHING),
            "the branch that spells the clause out in full is the one most likely to be \
             followed verbatim, so it carries the restriction too: {single}"
        );
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

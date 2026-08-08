//! Code generation output containing WASM bytecode and compilation metadata.
//!
//! This module defines [`CodegenOutput`], the return type of the `codegen()` function.
//! It carries the generated WASM binary along with metadata about the compilation.
//!
//! # Architecture
//!
//! The code generation pipeline produces WASM bytecode directly in-process:
//!
//! 1. **WASM Generation** (this crate) -- produces `CodegenOutput` with WASM binary and metadata
//! 2. **File Output** (CLI layer) -- reads `CodegenOutput` and writes the WASM file

use std::io;
use std::path::Path;

use inference_fn_key::FnKey;
use inference_hassert::HSpecMap;
use rustc_hash::FxHashMap;

use crate::target::{CompilationMode, OptLevel, Target};

/// Output of the WebAssembly code generation phase.
///
/// Contains the generated WASM binary and all metadata about the compilation.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::{CodegenOutput, Target, CompilationMode, OptLevel};
/// use rustc_hash::FxHashMap;
///
/// // The top-level `inference` orchestrator crate also re-exports
/// // `FxHashMap` for consumers that want to avoid a direct `rustc-hash`
/// // dependency; from this crate, use `rustc_hash::FxHashMap` directly.
/// let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d]; // WASM magic number
/// let output = CodegenOutput::new(
///     wasm_bytes,
///     Target::Wasm32,
///     CompilationMode::Compile,
///     OptLevel::O3,
///     "output".to_string(),
///     false,
///     FxHashMap::default(),
/// );
///
/// assert!(!output.wasm().is_empty());
/// assert_eq!(output.target(), Target::Wasm32);
/// assert_eq!(output.mode(), CompilationMode::Compile);
/// assert_eq!(output.opt_level(), OptLevel::O3);
/// assert_eq!(output.module_name(), "output");
/// assert!(!output.has_main());
/// assert!(output.spec_func_indices_by_spec().is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct CodegenOutput {
    /// WASM binary produced by the compiler.
    wasm: Vec<u8>,

    /// Compilation target.
    target: Target,

    /// Compilation mode controlling spec-node handling.
    mode: CompilationMode,

    /// Optimization level used for this compilation.
    ///
    /// Determined by the [`BuildProfile`] in the toolchain layer. Stored here so
    /// that downstream consumers can read it directly from the output.
    opt_level: OptLevel,

    /// Module name (currently hardcoded as `"output"`).
    ///
    /// Stored here for future parameterization (e.g., deriving from source filename).
    module_name: String,

    /// Whether a public `main()` function was found during compilation.
    ///
    /// When true, the runtime can use `main` as an entry point. The compiler
    /// discovers this during code generation when it encounters a `pub fn main()`.
    has_main: bool,

    /// WASM function indices of functions that originated in `spec` blocks,
    /// keyed by spec name.
    ///
    /// Empty in `compile` mode. In `proof` mode, contains per-spec function
    /// indices in registration order. The Rocq translator uses this to emit
    /// per-spec `Definition <mod>__<SpecName>_specs : list N` lists consumed
    /// by the corresponding `ValidModule` theorems.
    spec_func_indices_by_spec: FxHashMap<String, Vec<u32>>,

    /// Per-function shadow-stack frame sizes in bytes, keyed by the structured
    /// [`FnKey`] shared with the analysis passes.
    ///
    /// Keyed by the structured key (not its lossy `Display` string) so the
    /// cross-crate A036 parity test compares each function's estimate against its
    /// own real frame rather than collapsing keys that render identically.
    ///
    /// Exposed for testing and diagnostics; empty unless populated by the
    /// codegen entry point.
    frame_sizes: FxHashMap<FnKey, u32>,

    /// Per-spec `hassert` verification obligations, keyed by folded spec name.
    ///
    /// Empty in `compile` mode (specs are stripped). In `proof` mode, each
    /// `forall`-quantified (or plain) spec *free* function contributes one
    /// obligation, in source order. A later phase serializes these into the
    /// `inference.hspecs` custom section for the Rocq translator.
    hspecs: HSpecMap,
}

impl CodegenOutput {
    /// Creates a new `CodegenOutput` with the given WASM binary and metadata.
    #[must_use]
    pub fn new(
        wasm: Vec<u8>,
        target: Target,
        mode: CompilationMode,
        opt_level: OptLevel,
        module_name: String,
        has_main: bool,
        spec_func_indices_by_spec: FxHashMap<String, Vec<u32>>,
    ) -> Self {
        Self {
            wasm,
            target,
            mode,
            opt_level,
            module_name,
            has_main,
            spec_func_indices_by_spec,
            frame_sizes: FxHashMap::default(),
            hspecs: HSpecMap::default(),
        }
    }

    /// Attaches per-function shadow-stack frame sizes to this output.
    ///
    /// Builder-style setter so the public [`Self::new`] signature stays
    /// non-breaking. The map is keyed by the structured [`FnKey`] (matching the
    /// analysis key scheme) with the value in bytes.
    #[must_use]
    pub fn with_frame_sizes(mut self, frame_sizes: FxHashMap<FnKey, u32>) -> Self {
        self.frame_sizes = frame_sizes;
        self
    }

    /// Returns the per-function shadow-stack frame sizes in bytes, keyed by the
    /// structured [`FnKey`] (matching the analysis key scheme).
    ///
    /// Exposed for testing and diagnostics; empty unless populated by the
    /// codegen entry point.
    #[must_use]
    pub fn frame_sizes(&self) -> &FxHashMap<FnKey, u32> {
        &self.frame_sizes
    }

    /// Attaches the per-spec `hassert` verification obligations to this output.
    ///
    /// Builder-style setter so the public [`Self::new`] signature stays
    /// non-breaking, mirroring [`Self::with_frame_sizes`]. The map is empty in
    /// compile mode and populated in proof mode.
    #[must_use]
    pub fn with_hspecs(mut self, hspecs: HSpecMap) -> Self {
        self.hspecs = hspecs;
        self
    }

    /// Returns the per-spec `hassert` verification obligations, keyed by folded
    /// spec name.
    ///
    /// Empty in compile mode; populated in proof mode with one obligation per
    /// spec free function, in source order.
    #[must_use]
    pub fn hspecs(&self) -> &HSpecMap {
        &self.hspecs
    }

    /// Returns the WASM function indices for functions originating in `spec`
    /// blocks, grouped by spec name.
    #[must_use]
    pub fn spec_func_indices_by_spec(&self) -> &FxHashMap<String, Vec<u32>> {
        &self.spec_func_indices_by_spec
    }

    /// Returns the WASM binary bytes.
    #[must_use]
    pub fn wasm(&self) -> &[u8] {
        &self.wasm
    }

    /// Returns the compilation target.
    #[must_use]
    pub fn target(&self) -> Target {
        self.target
    }

    /// Returns the compilation mode.
    #[must_use]
    pub fn mode(&self) -> CompilationMode {
        self.mode
    }

    /// Returns the optimization level used for this compilation.
    #[must_use]
    pub fn opt_level(&self) -> OptLevel {
        self.opt_level
    }

    /// Returns the module name.
    #[must_use]
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Returns whether a public `main()` function was found.
    #[must_use]
    pub fn has_main(&self) -> bool {
        self.has_main
    }

    /// Writes the WASM binary to a file at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or written.
    pub fn write_wasm_to(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, &self.wasm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_output() -> CodegenOutput {
        CodegenOutput::new(
            vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::O3,
            "output".to_string(),
            false,
            FxHashMap::default(),
        )
    }

    fn sample_output_with_main() -> CodegenOutput {
        CodegenOutput::new(
            vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output".to_string(),
            true,
            FxHashMap::default(),
        )
    }

    #[test]
    fn wasm_returns_wasm_bytes() {
        let output = sample_output();
        assert_eq!(
            output.wasm(),
            &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn target_returns_target() {
        let output = sample_output();
        assert_eq!(output.target(), Target::Wasm32);
    }

    #[test]
    fn mode_returns_mode() {
        let output = sample_output();
        assert_eq!(output.mode(), CompilationMode::Compile);
    }

    #[test]
    fn opt_level_returns_opt_level() {
        let output = sample_output();
        assert_eq!(output.opt_level(), OptLevel::O3);
    }

    #[test]
    fn module_name_returns_module_name() {
        let output = sample_output();
        assert_eq!(output.module_name(), "output");
    }

    #[test]
    fn has_main_returns_false_by_default() {
        let output = sample_output();
        assert!(!output.has_main());
    }

    #[test]
    fn has_main_returns_true_when_set() {
        let output = sample_output_with_main();
        assert!(output.has_main());
    }

    #[test]
    fn soroban_output() {
        let output = CodegenOutput::new(
            Vec::new(),
            Target::Soroban,
            CompilationMode::Compile,
            OptLevel::Oz,
            "soroban_module".to_string(),
            false,
            FxHashMap::default(),
        );
        assert_eq!(output.target(), Target::Soroban);
    }

    #[test]
    fn spec_func_indices_getter_preserves_single_spec() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("S".to_string(), vec![3, 4, 7]);
        let output = CodegenOutput::new(
            Vec::new(),
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output".to_string(),
            false,
            map,
        );
        let by_spec = output.spec_func_indices_by_spec();
        assert_eq!(by_spec.len(), 1);
        assert_eq!(by_spec.get("S"), Some(&vec![3, 4, 7]));
    }

    /// Two specs with different index-list lengths round-trip distinctly.
    /// Guards against accidental field aliasing or shared-state bugs in the
    /// `CodegenOutput` constructor / getter pair.
    #[test]
    fn spec_func_indices_getter_preserves_multi_spec() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("A".to_string(), vec![1]);
        map.insert("B".to_string(), vec![5, 6, 7, 8]);
        let output = CodegenOutput::new(
            Vec::new(),
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output".to_string(),
            false,
            map,
        );
        let by_spec = output.spec_func_indices_by_spec();
        assert_eq!(by_spec.len(), 2);
        assert_eq!(by_spec.get("A"), Some(&vec![1]));
        assert_eq!(by_spec.get("B"), Some(&vec![5, 6, 7, 8]));
    }

    /// A spec with an empty indices list is a legitimate state (e.g. a spec
    /// block whose inner functions were all stripped). The accessor must
    /// preserve the empty `Vec` rather than coalescing it to `None`.
    #[test]
    fn spec_func_indices_getter_preserves_empty_indices() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("Empty".to_string(), Vec::new());
        let output = CodegenOutput::new(
            Vec::new(),
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output".to_string(),
            false,
            map,
        );
        let by_spec = output.spec_func_indices_by_spec();
        assert_eq!(by_spec.len(), 1);
        assert_eq!(by_spec.get("Empty"), Some(&Vec::<u32>::new()));
    }

    /// `write_wasm_to` puts exactly the bytes `wasm()` reports on disk. The
    /// scratch file lives inside a `TempDir` so that concurrent test processes
    /// never write to and delete the same path; the directory's drop guard
    /// removes it even when an assertion fails.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn write_wasm_to_creates_file() {
        let output = sample_output();
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let path = dir.path().join("codegen_output.wasm");
        output.write_wasm_to(&path).expect("Failed to write WASM");
        let contents = std::fs::read(&path).expect("Failed to read WASM");
        assert_eq!(contents, output.wasm());
    }
}

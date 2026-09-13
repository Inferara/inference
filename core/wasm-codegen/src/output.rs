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

/// The source-level type of one exported parameter or return value.
///
/// A WebAssembly value type cannot answer this question. `i32` is what `bool`,
/// every integer narrower than 64 bits, an enum tag, a struct pointer and an
/// array pointer all lower to, so a consumer that has only the emitted bytes
/// cannot tell them apart — and a foreign calling convention, a contract ABI
/// for instance, has to encode each of them differently. This is the one thing
/// the WASM binary does not carry.
///
/// The variants name what the source declared, not how it is passed: a struct
/// and an array both arrive as an `i32` address into linear memory, and an enum
/// arrives as a bare `i32` tag with no memory footprint at all. `Array::len` is
/// the declared element count, and a nested array nests.
///
/// The `name` of a `Struct` or an `Enum` is its spelling at the position that
/// used it — a bare `Point`, or the `::`-joined path of a qualified reference.
/// It is a label, not an identity, and it is injective in neither direction:
/// two distinct types defined in different files can share one spelling, and
/// one type reached two ways carries two, `Point` where it was item-imported
/// and `geom::Point` where it was reached through a namespace. So a consumer
/// that groups exports by this string — a contract-spec emitter listing each
/// type once is the case to expect — would both merge two types and split one.
/// No identity is recorded because nothing on the emission path needs one; a
/// consumer that does need one has to take it from the type checker, whose
/// canonical key for a definition is qualified by the file that defines it and
/// is therefore the same key however an annotation spelled the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiType {
    /// A `bool`, lowered to an `i32`.
    ///
    /// What the descriptor promises about that `i32` differs by position, and
    /// the difference is why a foreign calling convention that encodes booleans
    /// as two distinct values has to normalize a *returned* one itself. A
    /// `bool` parameter of an exported function is canonicalized to 0 or 1 by
    /// the entry prologue before the body runs — a host may pass any bit
    /// pattern, and this is where truthiness is decided — so a caller may rely
    /// on that. A returned `bool` is whatever the body left on the stack, and
    /// no return-position counterpart of that prologue exists: the descriptor
    /// says the source wrote `bool` and promises nothing about the width of
    /// true, so a consumer that needs exactly 1 must produce it.
    Bool,
    /// A signed 8-bit integer, lowered to an `i32`. As an exported parameter it
    /// is sign-extended from its low byte by the entry prologue.
    I8,
    /// An unsigned 8-bit integer, lowered to an `i32`. As an exported parameter
    /// it is masked to its low byte by the entry prologue.
    U8,
    /// A signed 16-bit integer, lowered to an `i32`. As an exported parameter it
    /// is sign-extended from its low half by the entry prologue.
    I16,
    /// An unsigned 16-bit integer, lowered to an `i32`. As an exported parameter
    /// it is masked to its low half by the entry prologue.
    U16,
    /// A signed 32-bit integer, lowered to an `i32` and passed unchanged.
    I32,
    /// An unsigned 32-bit integer, lowered to an `i32` and passed unchanged:
    /// the same value type `I32` uses, with only this descriptor separating
    /// them.
    U32,
    /// A signed 64-bit integer, lowered to an `i64`.
    I64,
    /// An unsigned 64-bit integer, lowered to an `i64`, which is the value type
    /// `I64` uses as well.
    U64,
    /// An enum, passed as a bare `i32` tag.
    ///
    /// It has no memory footprint: unlike a struct or an array, an enum is the
    /// tag itself rather than a pointer to one, so an enum return is a real
    /// WebAssembly result and never the hidden-pointer convention
    /// [`AbiReturn::Sret`] describes. `name` is a label, under the rule above.
    Enum { name: String },
    /// A struct, passed as an `i32` address of its frame-allocated bytes.
    /// `name` is a label, under the rule above.
    Struct { name: String },
    /// An array, passed as an `i32` address of its elements. `len` is the
    /// declared element count and `elem` the declared element type, which is
    /// itself an `AbiType` so that a nested array nests.
    Array { elem: Box<AbiType>, len: u32 },
}

/// What an exported function gives back.
///
/// `Sret` is the compound-return calling convention: a function returning a
/// struct or an array declares no WebAssembly result and instead takes a hidden
/// leading `i32` pointer parameter, which the caller owns and the callee writes
/// through. That pointer is deliberately absent from
/// [`ExportSignature::params`], which lists only the parameters the source
/// declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiReturn {
    /// The function gives nothing back and declares no WebAssembly result. The
    /// three spellings that say so — no arrow, `-> ()` and `-> unit` — are one
    /// declaration and are recorded identically.
    Unit,
    /// The function gives back one value in a WebAssembly result: a `bool`, an
    /// integer of any width, or an enum tag. Never a struct or an array, which
    /// take [`Self::Sret`] instead.
    Scalar(AbiType),
    /// The function gives back a struct or an array through the hidden-pointer
    /// convention described above: no WebAssembly result, and a leading
    /// caller-owned `i32` pointer that [`ExportSignature::params`] does not
    /// list.
    Sret(AbiType),
}

/// The source-level signature of one exported function.
///
/// # Match an export by name, never by index
///
/// No function index is recorded here, and none may be added. The static-merge
/// linker renumbers functions whenever an external module is merged in, so an
/// index recorded at code generation is stale by the time any post-link
/// consumer reads it — while a name survives, because the export section is
/// what the linker preserves. Every consumer must therefore look an entry up by
/// [`Self::name`](ExportSignature::name) against the export section of the
/// module it is actually holding, and treat an export it finds no entry for as
/// unknown rather than guessing.
///
/// A position in the descriptor is no substitute either. Entries follow the
/// order of the *function* exports in the export section, and a module with
/// memory exports `memory` and `__stack_pointer` there as well, so entry `k` is
/// not export `k`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSignature {
    /// The exported name, as it appears in the export section.
    pub name: String,

    /// The parameters the source declared, in declaration order. A hidden `sret`
    /// pointer is not one of them, and neither is a lowering-introduced
    /// parameter.
    pub params: Vec<AbiType>,

    /// What the function gives back.
    pub ret: AbiReturn,
}

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
/// assert!(output.export_signatures().is_empty());
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
    /// indices in registration order. The Rocq translator uses this to decide
    /// which functions to omit from the emitted module record and how to
    /// renumber every surviving reference; the obligations themselves travel
    /// separately, as `hassert` payloads in `inference.hspecs`.
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

    /// Every emitted function whose body carries an overflow guard, by the
    /// structured [`FnKey`] shared with the analysis passes and named by an
    /// obligation's `T_app`/`HA_app_ok`.
    ///
    /// A guarded body traps rather than wrapping when a `+`, `-`, `*` or unary
    /// `-` leaves its type, which is what a caller reasoning about whether a
    /// compiled body can trap needs to know. Empty for a module none of whose
    /// arithmetic is effectively checked, which is every module built from
    /// source that names no mode.
    guarded_functions: Vec<FnKey>,

    /// Per-spec `hassert` verification obligations, keyed by folded spec name.
    ///
    /// Empty in `compile` mode (specs are stripped). In `proof` mode, each
    /// `forall`-quantified (or plain) spec *free* function contributes one
    /// obligation, in source order. A later phase serializes these into the
    /// `inference.hspecs` custom section for the Rocq translator.
    hspecs: HSpecMap,

    /// The source-level signature of every exported function, in the order of
    /// the function exports in the export section.
    ///
    /// Not the order of the export section itself: a module with memory also
    /// exports `memory` and `__stack_pointer` there, after every function, so
    /// entry `k` here is generally not export `k` of the emitted module.
    ///
    /// Pure observation: recorded while a function is registered, read by
    /// nothing on the emission path, and absent from the emitted bytes. A
    /// consumer choosing a foreign calling convention for an export needs the
    /// source type of each parameter, which the WASM value types do not carry —
    /// see [`ExportSignature`], including its rule that an entry is matched by
    /// name and never by index.
    export_signatures: Vec<ExportSignature>,
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
            guarded_functions: Vec::new(),
            hspecs: HSpecMap::default(),
            export_signatures: Vec::new(),
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

    /// Attaches the guarded-function list. Builder-style so adding it was
    /// non-breaking, mirroring [`Self::with_frame_sizes`].
    #[must_use = "returns the updated output"]
    pub fn with_guarded_functions(mut self, guarded_functions: Vec<FnKey>) -> Self {
        self.guarded_functions = guarded_functions;
        self
    }

    /// Every emitted function whose body carries an overflow guard.
    #[must_use = "returns the guarded functions without modifying the output"]
    pub fn guarded_functions(&self) -> &[FnKey] {
        &self.guarded_functions
    }

    /// Attaches the per-export source-type descriptor, in the order of the
    /// function exports in the export section. Builder-style so adding it was
    /// non-breaking, mirroring [`Self::with_frame_sizes`].
    #[must_use = "returns the updated output"]
    pub fn with_export_signatures(mut self, export_signatures: Vec<ExportSignature>) -> Self {
        self.export_signatures = export_signatures;
        self
    }

    /// The source-level signature of every exported function, in the order of
    /// the function exports in the export section — which is not the order of
    /// the export section, since a module with memory also exports `memory` and
    /// `__stack_pointer` there.
    ///
    /// Empty for a module that exports no function. The order is a convenience
    /// for a reader; an entry is identified by its
    /// [`name`](ExportSignature::name), never by its position or by a function
    /// index, for the reason [`ExportSignature`] gives.
    #[must_use = "returns the export signatures without modifying the output"]
    pub fn export_signatures(&self) -> &[ExportSignature] {
        &self.export_signatures
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
    fn stellar_output() {
        let output = CodegenOutput::new(
            Vec::new(),
            Target::Stellar,
            CompilationMode::Compile,
            OptLevel::Oz,
            "stellar_module".to_string(),
            false,
            FxHashMap::default(),
        );
        assert_eq!(output.target(), Target::Stellar);
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

    /// An output carries no export descriptor until one is attached, and the
    /// builder round-trips whatever it is given — including the two compound
    /// shapes whose emitted `i32` says nothing about them.
    #[test]
    fn export_signatures_round_trip_through_the_builder() {
        let output = sample_output();
        assert!(output.export_signatures().is_empty());

        let described = output.with_export_signatures(vec![
            ExportSignature {
                name: "scale".to_string(),
                params: vec![AbiType::U32, AbiType::Bool],
                ret: AbiReturn::Scalar(AbiType::U32),
            },
            ExportSignature {
                name: "corners".to_string(),
                params: vec![AbiType::Struct {
                    name: "Point".to_string(),
                }],
                ret: AbiReturn::Sret(AbiType::Array {
                    elem: Box::new(AbiType::I32),
                    len: 4,
                }),
            },
        ]);

        let signatures = described.export_signatures();
        assert_eq!(signatures.len(), 2);
        assert_eq!(signatures[0].name, "scale");
        assert_eq!(signatures[0].params, vec![AbiType::U32, AbiType::Bool]);
        assert_eq!(
            signatures[1].ret,
            AbiReturn::Sret(AbiType::Array {
                elem: Box::new(AbiType::I32),
                len: 4,
            }),
        );
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

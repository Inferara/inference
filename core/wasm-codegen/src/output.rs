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

use crate::target::{CompilationMode, OptLevel, Target};

/// Output of the WebAssembly code generation phase.
///
/// Contains the generated WASM binary and all metadata about the compilation.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::{CodegenOutput, Target, CompilationMode, OptLevel};
///
/// let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d]; // WASM magic number
/// let output = CodegenOutput::new(
///     wasm_bytes,
///     Target::Wasm32,
///     CompilationMode::Compile,
///     OptLevel::O3,
///     "output".to_string(),
///     false,
/// );
///
/// assert!(!output.wasm().is_empty());
/// assert_eq!(output.target(), Target::Wasm32);
/// assert_eq!(output.mode(), CompilationMode::Compile);
/// assert_eq!(output.opt_level(), OptLevel::O3);
/// assert_eq!(output.module_name(), "output");
/// assert!(!output.has_main());
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
    ) -> Self {
        Self {
            wasm,
            target,
            mode,
            opt_level,
            module_name,
            has_main,
        }
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
        );
        assert_eq!(output.target(), Target::Soroban);
    }

    #[test]
    fn write_wasm_to_creates_file() {
        let output = sample_output();
        let dir = std::env::temp_dir();
        let path = dir.join("test_codegen_output_wasm.wasm");
        output.write_wasm_to(&path).expect("Failed to write WASM");
        let contents = std::fs::read(&path).expect("Failed to read WASM");
        assert_eq!(contents, output.wasm());
        std::fs::remove_file(&path).ok();
    }
}

//! Code generation output containing LLVM IR and compilation metadata.
//!
//! This module defines [`CodegenOutput`], the return type of the `codegen()` function.
//! It carries the generated LLVM IR text along with metadata needed by the toolchain
//! layer to invoke `inf-llc` and `rust-lld` with the correct flags.
//!
//! # Architecture
//!
//! The code generation pipeline is split into two stages:
//!
//! 1. **IR Generation** (this crate) — produces `CodegenOutput` with LLVM IR and metadata
//! 2. **Tool Invocation** (CLI layer) — reads `CodegenOutput` and invokes external tools
//!
//! This separation follows the Clang/rustc pattern where the codegen library returns IR
//! and the driver/CLI layer handles subprocess management.

use std::io;
use std::path::Path;

use crate::target::{CompilationMode, OptLevel, Target};

/// Output of the LLVM IR code generation phase.
///
/// Contains the generated LLVM IR text and all metadata required by the toolchain
/// layer to compile the IR into a WebAssembly binary with the correct target-specific
/// flags.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::{CodegenOutput, Target, CompilationMode, OptLevel};
///
/// let output = CodegenOutput::new(
///     "define i32 @hello_world() { ret i32 42 }".to_string(),
///     Target::Wasm32,
///     CompilationMode::Compile,
///     OptLevel::O3,
///     "output".to_string(),
///     false,
/// );
///
/// assert!(!output.ir().is_empty());
/// assert_eq!(output.target(), Target::Wasm32);
/// assert_eq!(output.mode(), CompilationMode::Compile);
/// assert_eq!(output.opt_level(), OptLevel::O3);
/// assert_eq!(output.target_triple(), "wasm32-unknown-unknown");
/// assert_eq!(output.module_name(), "output");
/// assert!(!output.has_main());
/// ```
#[derive(Debug, Clone)]
pub struct CodegenOutput {
    /// LLVM IR text produced by the compiler.
    ir: String,

    /// Compilation target that determines `inf-llc` and `rust-lld` flags.
    target: Target,

    /// Compilation mode controlling optimization level and spec-node handling.
    mode: CompilationMode,

    /// Optimization level used for this compilation.
    ///
    /// Determined by the [`BuildProfile`] in the toolchain layer. Stored here so
    /// that `inf-llc` can read it directly from the output without recomputing.
    opt_level: OptLevel,

    /// LLVM module name (currently hardcoded as `"output"`).
    ///
    /// Stored here for future parameterization (e.g., deriving from source filename).
    module_name: String,

    /// Whether a public `main()` function was found during compilation.
    ///
    /// When true, the linker should receive the `--export=main` flag. The compiler
    /// discovers this during IR generation when it encounters a `pub fn main()`.
    has_main: bool,
}

impl CodegenOutput {
    /// Creates a new `CodegenOutput` with the given IR and metadata.
    #[must_use]
    pub fn new(
        ir: String,
        target: Target,
        mode: CompilationMode,
        opt_level: OptLevel,
        module_name: String,
        has_main: bool,
    ) -> Self {
        Self {
            ir,
            target,
            mode,
            opt_level,
            module_name,
            has_main,
        }
    }

    /// Returns the LLVM IR text.
    #[must_use]
    pub fn ir(&self) -> &str {
        &self.ir
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

    /// Returns the LLVM target triple string for this output.
    ///
    /// Convenience method equivalent to `self.target().triple()`.
    #[must_use]
    pub fn target_triple(&self) -> &'static str {
        self.target.triple()
    }

    /// Returns the LLVM module name.
    #[must_use]
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Returns whether a public `main()` function was found.
    ///
    /// When true, the linker should use `--export=main` to export the main function.
    #[must_use]
    pub fn has_main(&self) -> bool {
        self.has_main
    }

    /// Writes the LLVM IR to a file at the given path.
    ///
    /// This is a convenience method for the toolchain layer to write IR to a
    /// temporary file before invoking `inf-llc`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or written.
    pub fn write_ir_to(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, &self.ir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_output() -> CodegenOutput {
        CodegenOutput::new(
            "define i32 @test() { ret i32 42 }".to_string(),
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::O3,
            "output".to_string(),
            false,
        )
    }

    fn sample_output_with_main() -> CodegenOutput {
        // Decision #32: Proof mode uses the target's release optimization (O3 for Wasm32).
        CodegenOutput::new(
            "define i32 @main() { ret i32 0 }".to_string(),
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output".to_string(),
            true,
        )
    }

    #[test]
    fn ir_returns_ir_text() {
        let output = sample_output();
        assert_eq!(output.ir(), "define i32 @test() { ret i32 42 }");
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
    fn target_triple_returns_correct_triple() {
        let output = sample_output();
        assert_eq!(output.target_triple(), "wasm32-unknown-unknown");
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
    fn soroban_target_triple() {
        let output = CodegenOutput::new(
            String::new(),
            Target::Soroban,
            CompilationMode::Compile,
            OptLevel::Oz,
            "soroban_module".to_string(),
            false,
        );
        assert_eq!(output.target_triple(), "wasm32-unknown-unknown");
        assert_eq!(output.target(), Target::Soroban);
    }

    #[test]
    fn write_ir_to_creates_file() {
        let output = sample_output();
        let dir = std::env::temp_dir();
        let path = dir.join("test_codegen_output_ir.ll");
        output.write_ir_to(&path).expect("Failed to write IR");
        let contents = std::fs::read_to_string(&path).expect("Failed to read IR");
        assert_eq!(contents, output.ir());
        std::fs::remove_file(&path).ok();
    }
}

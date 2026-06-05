//! Front-end support for linking external `.wasm` modules.
//!
//! This module hosts the **driver-side** half of Issue #9's `.wasm` static-merge
//! feature — everything that runs before any bytes are merged:
//!
//! - [`resolve`] turns a logical module reference (`use { f } from a::b;`) into a
//!   concrete `.wasm` [`std::path::PathBuf`], portably and with a precise miss
//!   diagnostic.
//! - [`validate`] confirms that a resolved `.wasm` actually exports the named
//!   function and that its signature matches the `external fn` declaration.
//!
//! The later codegen and merge phases (a dedicated `core/wasm-linker/` crate)
//! consume the validated bindings these utilities produce.

pub mod driver;
pub mod resolve;
pub mod validate;

pub use driver::{
    resolve_external_modules, ExternalResolutionError, ResolvedExternalModule,
    MAX_EXTERNAL_MODULE_BYTES,
};
pub use resolve::{
    resolve_wasm_module, ManifestDeps, ModulePath, ModulePathError, ResolveError, SearchPath,
};
pub use validate::{
    lower_extern_signature, validate_extern, DeclaredSignature, LowerSignatureError,
    SignatureMismatch, ValidateError, WasmValType,
};

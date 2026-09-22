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
//! - [`driver`] runs the two over every extern a program binds, and collects the
//!   bindings that name no file at all — the imports an embedder supplies.
//!
//! The later codegen and merge phases (a dedicated `core/wasm-linker/` crate)
//! consume the validated bindings these utilities produce.
//!
//! One module here runs after those phases rather than before: [`imports`] reads
//! a *compiled* module's import section and holds it to the host bindings the
//! driver collected. A program whose imports are meant to survive never reaches
//! the merge, so it never gets the linker's guarantee that nothing is left
//! unsatisfied, and that check is what stands in its place.

pub mod driver;
pub mod imports;
pub mod resolve;
pub mod validate;

pub use driver::{
    host_import_label, resolve_external_modules, ExternalResolutionError, HostImport,
    ResolvedExternalModule, ResolvedExternals, MAX_EXTERNAL_MODULE_BYTES,
};
pub use imports::HostImportError;
pub use resolve::{
    resolve_wasm_module, ManifestDeps, ModulePath, ModulePathError, ResolveError, SearchPath,
};
pub use validate::{
    lower_extern_signature, validate_extern, DeclaredSignature, LoweredExtern,
    LowerSignatureError, SignatureMismatch, ValidateError, WasmValType,
};

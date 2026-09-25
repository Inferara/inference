//! Host modules: what an embedder registers for a module's imports to bind to.

use spacewasm::{HostFunction, HostGlobal, HostModule, HostName};

use crate::errors::HostSetError;
use crate::session::Session;

/// The host modules a load binds a module's imports to, as
/// [`crate::load_with`] takes them: a list allocated by the interpreter's
/// allocator, which [`host_set`] builds.
pub type HostSet = spacewasm::Vec<HostModule>;

/// The host module registered under `name`, supplying `functions` and
/// `globals` to the modules that import them.
///
/// A host module built here supplies functions and globals only; it registers
/// no memory and no table. The name is put to the interpreter's own
/// constructor, so a name it would not register is refused rather than cut
/// short.
///
/// The session is never read: the lists are allocated through the
/// interpreter's allocator, and taking the session is what keeps that under
/// its lock. The module must be moved into a load, or dropped, while the
/// session it was built under is still held, since dropping it frees through
/// the same allocator; it borrows nothing because [`crate::load_with`] takes
/// that session mutably beside it.
///
/// # Errors
///
/// [`HostSetError::Name`] when `name` is longer than a host module name holds,
/// and [`HostSetError::Allocation`] when the interpreter's allocator cannot
/// hold either list.
pub fn host_module(
    _session: &Session,
    name: &str,
    functions: Vec<HostFunction>,
    globals: Vec<HostGlobal>,
) -> Result<HostModule, HostSetError> {
    let name = HostName::try_from_str(name)
        .map_err(|error| HostSetError::Name { name: name.to_string(), error })?;
    Ok(HostModule {
        name,
        globals: interpreter_vec(globals)?,
        functions: interpreter_vec(functions)?,
        memory: spacewasm::Vec::zero(),
        table: spacewasm::Vec::zero(),
    })
}

/// `modules` as the host set [`crate::load_with`] takes.
///
/// The set must be moved into a load, or dropped, while the session it was
/// built under is still held; it borrows nothing because [`crate::load_with`]
/// takes that session mutably beside it.
///
/// # Errors
///
/// [`HostSetError::Allocation`] when the interpreter's allocator cannot hold
/// the list.
pub fn host_set(_session: &Session, modules: Vec<HostModule>) -> Result<HostSet, HostSetError> {
    interpreter_vec(modules)
}

/// `items`, moved into a list allocated by the interpreter's allocator, which
/// is the only kind of list a host module or a host set is made of.
fn interpreter_vec<T>(items: Vec<T>) -> Result<spacewasm::Vec<T>, HostSetError> {
    spacewasm::Vec::from_exact_iter(items.into_iter()).map_err(HostSetError::Allocation)
}

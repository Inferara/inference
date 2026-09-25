//! Host modules: what an embedder registers for a module's imports to bind to,
//! and the log the reference hosts write their calls to.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;

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

/// Where the F´ reference hosts write their log lines, one per call that
/// logs, and how many they have written.
///
/// A host writes its line during the call, before it answers, so a line on
/// the stream is a call the program made even when the call it was part of
/// then traps or runs out of fuel; the count is what lets a caller say so.
/// Clones share one log, so a caller can keep a handle on a log it hands to a
/// load.
#[derive(Clone)]
pub struct HostLog(Rc<LogState>);

/// A [`HostLog`]'s sink and count.
struct LogState {
    sink: Sink,
    lines: Cell<usize>,
}

/// Where a [`HostLog`]'s lines go.
enum Sink {
    /// The process's standard error, one line at a time, as each is written.
    Stderr,
    /// A list kept for the caller to read.
    Recording(RefCell<Vec<String>>),
}

impl HostLog {
    /// A log that streams each line to standard error as the host writes it.
    ///
    /// A line that cannot be written is dropped without a word: a closed
    /// stream must not turn a program that ran into a runner that panicked,
    /// and a failure to log has nowhere left to be reported to.
    #[must_use]
    pub fn stderr() -> Self {
        Self::with_sink(Sink::Stderr)
    }

    /// A log that keeps its lines for [`HostLog::recorded`] instead of
    /// printing them.
    #[must_use]
    pub fn recording() -> Self {
        Self::with_sink(Sink::Recording(RefCell::default()))
    }

    fn with_sink(sink: Sink) -> Self {
        Self(Rc::new(LogState { sink, lines: Cell::new(0) }))
    }

    /// How many lines the hosts have written, whether or not they reached the
    /// stream.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.0.lines.get()
    }

    /// The lines a recording log holds, oldest first. A log that streams to
    /// standard error keeps none.
    #[must_use]
    pub fn recorded(&self) -> Vec<String> {
        match &self.0.sink {
            Sink::Stderr => Vec::new(),
            Sink::Recording(lines) => lines.borrow().clone(),
        }
    }

    /// Writes one line.
    pub(crate) fn write_line(&self, line: &str) {
        self.0.lines.set(self.0.lines.get() + 1);
        match &self.0.sink {
            Sink::Stderr => {
                let _ = writeln!(std::io::stderr().lock(), "{line}");
            }
            Sink::Recording(lines) => lines.borrow_mut().push(line.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HostLog;

    /// A recording log keeps every line in order and counts them, and a clone
    /// is the same log.
    ///
    /// Fails if a clone starts a log of its own, which would leave a caller
    /// holding a handle on a log the hosts never write to.
    #[test]
    fn a_recording_log_keeps_and_counts_its_lines_across_clones() {
        let log = HostLog::recording();
        let handed_on = log.clone();
        handed_on.write_line("MESSAGE one");
        log.write_line("COMMAND 1 2");
        assert_eq!(log.recorded(), ["MESSAGE one", "COMMAND 1 2"]);
        assert_eq!(handed_on.line_count(), 2);
    }

    /// A log streaming to standard error counts what it writes and keeps
    /// nothing.
    #[test]
    fn a_stderr_log_counts_its_lines_and_keeps_none() {
        let log = HostLog::stderr();
        log.write_line("RSLEEP 1");
        assert_eq!(log.line_count(), 1);
        assert_eq!(log.recorded(), Vec::<String>::new());
    }
}

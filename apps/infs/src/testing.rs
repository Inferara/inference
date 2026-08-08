//! Test-only helpers shared by the `infs` unit tests.
//!
//! # The write-then-exec (`ETXTBSY`) race
//!
//! Several unit tests write a small shell script, mark it executable, and then
//! have the code under test spawn it. That sequence is racy in a multithreaded
//! process. Rust opens files with `O_CLOEXEC`, but `CLOEXEC` closes a descriptor
//! only *at* `exec`: a child forked by another thread between the writer's
//! `open` and its `close` inherits a copy of the still-open write descriptor and
//! holds it until it reaches its own `exec`. An `execve` of the script inside
//! that window fails with `ETXTBSY` ("text file busy"). This test binary spawns
//! processes throughout, so the window is occasionally hit.
//!
//! The condition is transient — every inherited copy is closed by the child's
//! own `exec`, which follows within milliseconds — so a bounded retry is the
//! remedy, the same one Cargo and rustc carry in their spawn paths.
//!
//! Two shapes are needed, because a retry can only fire on a failure it can see:
//!
//! - When the code under test *surfaces* the spawn error, wrap the call in
//!   [`retry_while_exec_busy`]. The operation under test is retried exactly as
//!   written, and any other error returns immediately, so a test asserting a
//!   real failure is unaffected.
//! - When the code under test *swallows* it — mapping a failed spawn to a
//!   fallback value, as the `--version` probes do — there is no error to key on.
//!   Call [`settle_executable`] right after writing the stub instead, which
//!   blocks until the file can be executed at all.

#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

/// How many times [`retry_while_exec_busy`] runs its operation before giving up.
const EXEC_BUSY_ATTEMPTS: u32 = 50;

/// Pause between attempts. Together with [`EXEC_BUSY_ATTEMPTS`] this bounds the
/// wait at roughly a second — orders of magnitude longer than a fork/exec
/// window, yet short enough that a genuinely broken stub fails a test rather
/// than hanging it.
const EXEC_BUSY_DELAY: Duration = Duration::from_millis(20);

/// Runs `op`, retrying while it fails with `ETXTBSY`.
///
/// Wrap the operation under test — not just the spawn — so the retry replays
/// exactly what raced. `Ok` and every non-`ETXTBSY` error pass straight through,
/// so a test that expects a real failure sees it on the first attempt.
///
/// # Errors
///
/// Returns the first non-`ETXTBSY` error, or the last `ETXTBSY` error once the
/// attempt bound is exhausted.
pub(crate) fn retry_while_exec_busy<T>(op: impl FnMut() -> Result<T>) -> Result<T> {
    retry_while_exec_busy_with(EXEC_BUSY_ATTEMPTS, EXEC_BUSY_DELAY, op)
}

/// The body of [`retry_while_exec_busy`] with the schedule supplied rather than
/// fixed, so the exhaustion path can be covered without paying the full wait.
fn retry_while_exec_busy_with<T>(
    attempts: u32,
    delay: Duration,
    mut op: impl FnMut() -> Result<T>,
) -> Result<T> {
    let mut attempt = 1;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if attempt < attempts && is_exec_busy(&err) => {
                attempt += 1;
                std::thread::sleep(delay);
            }
            Err(err) => return Err(err),
        }
    }
}

/// Whether any error in `err`'s chain is an `ETXTBSY` I/O error.
///
/// The chain is walked rather than the head inspected because callers add
/// context to a failed spawn (`Failed to execute wasm-opt at …`), which buries
/// the [`std::io::Error`] carrying the errno.
fn is_exec_busy(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io_err| io_err.kind() == std::io::ErrorKind::ExecutableFileBusy)
}

/// Blocks until the freshly written executable at `path` can be executed, by
/// running it with `probe_args` until the spawn stops reporting `ETXTBSY`.
///
/// Pass an invocation the code under test makes anyway (a `--version` probe,
/// say) so the extra run has no effect the test can observe. The stub's exit
/// status and output are discarded — only reaching `exec` matters. On a platform
/// that enforces the check, a successful `exec` proves no process still holds a
/// write descriptor to the file, and nothing reopens it for writing afterwards,
/// so the spawns the test goes on to make are safe too.
///
/// # Panics
///
/// Panics if the stub cannot be spawned for any other reason, or if the busy
/// window has not cleared within the attempt bound.
#[cfg(unix)]
pub(crate) fn settle_executable(path: &Path, probe_args: &[&str]) {
    retry_while_exec_busy(|| {
        Command::new(path)
            .args(probe_args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("failed to execute the stub at {}", path.display()))?;
        Ok(())
    })
    .unwrap_or_else(|err| {
        panic!(
            "the stub at {} never became executable: {err:#}",
            path.display()
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A spawn failure shaped like the real one: an `ETXTBSY` [`std::io::Error`]
    /// buried under the context a caller attaches.
    fn busy_error() -> anyhow::Error {
        anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::ExecutableFileBusy))
            .context("Failed to execute wasm-opt at /tmp/stub/wasm-opt")
    }

    #[test]
    fn a_busy_operation_is_retried_until_the_window_clears() {
        let calls = Cell::new(0_u32);
        let value = retry_while_exec_busy(|| {
            calls.set(calls.get() + 1);
            if calls.get() <= 2 {
                Err(busy_error())
            } else {
                Ok(7)
            }
        })
        .unwrap();

        assert_eq!(value, 7, "the first successful attempt must be returned");
        assert_eq!(calls.get(), 3, "both busy failures must have been retried");
    }

    #[test]
    fn a_non_busy_error_is_returned_on_the_first_attempt() {
        let calls = Cell::new(0_u32);
        let err = retry_while_exec_busy(|| {
            calls.set(calls.get() + 1);
            Err::<(), _>(anyhow::anyhow!("wasm-opt failed (exit code 1)"))
        })
        .unwrap_err();

        assert_eq!(
            calls.get(),
            1,
            "an error a test is asserting on must not be retried"
        );
        assert!(err.to_string().contains("wasm-opt failed"));
    }

    #[test]
    fn a_permanently_busy_operation_gives_up_with_the_last_error() {
        let calls = Cell::new(0_u32);
        // The real delay is elided: the bound is what is under test, and
        // sleeping through it would cost a second on every run of the suite.
        let err = retry_while_exec_busy_with(EXEC_BUSY_ATTEMPTS, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            Err::<(), _>(busy_error().context(format!("attempt {}", calls.get())))
        })
        .unwrap_err();

        assert_eq!(
            calls.get(),
            EXEC_BUSY_ATTEMPTS,
            "the retry must stop at the attempt bound rather than spin"
        );
        assert!(
            format!("{err:#}").contains(&format!("attempt {EXEC_BUSY_ATTEMPTS}")),
            "the surfaced error must be the last attempt's, got: {err:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn settle_executable_accepts_a_stub_that_exits_nonzero() {
        use std::os::unix::fs::PermissionsExt;

        let dir = assert_fs::TempDir::new().unwrap();
        let stub = dir.path().join("stub");
        std::fs::write(&stub, b"#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Settling asks only that the `exec` succeed; what the stub then does
        // with its exit status belongs to the test that runs it for real.
        settle_executable(&stub, &["--version"]);
    }

    /// Exercises the retry against a genuine `ETXTBSY` rather than a synthetic
    /// error. The fork/exec window cannot be scheduled on demand, but the
    /// condition it opens can: a still-open write handle blocks `execve` the
    /// same way an inherited descriptor does, and closing it clears the block.
    ///
    /// Only Linux enforces that in practice — macOS execs a script its writer
    /// still holds open — so on a macOS host this self-skips and the real-race
    /// coverage comes from the ubuntu CI leg.
    #[cfg(unix)]
    #[test]
    fn a_real_etxtbsy_spawn_is_retried_until_the_writer_closes() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let dir = assert_fs::TempDir::new().unwrap();
        let stub = dir.path().join("stub");
        let mut writer = std::fs::File::create(&stub).unwrap();
        writer.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        writer.flush().unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let blocked = Command::new(&stub).stdin(Stdio::null()).output();
        match &blocked {
            Err(err) if err.kind() == std::io::ErrorKind::ExecutableFileBusy => {}
            other => {
                // Nothing to retry where the platform permits the exec anyway;
                // report rather than assert, so the suite stays portable.
                eprintln!(
                    "note: skipping, an open write handle does not block exec here: {other:?}"
                );
                return;
            }
        }

        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(writer);
        });

        settle_executable(&stub, &[]);
        release.join().unwrap();
    }
}

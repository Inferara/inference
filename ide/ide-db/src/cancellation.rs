//! Cross-thread cancellation for in-flight analyses.
//!
//! An [`AnalysisCancelSource`] couples two facts a caller needs to classify an
//! interrupted analysis: a monotonic *write epoch* and the bound database
//! handle's cancellation token. A writer bumps the epoch **before** firing the
//! token, so an observer that catches the resulting unwind can tell a
//! *superseded* analysis (a newer write is pending — the epoch moved) from a
//! *residual* self-cancel (the token fired with no newer write behind it). The
//! predicate [`is_cancellation`] recognizes the semantic layer's cancellation
//! payload so the protocol layer can discriminate it from a genuine panic
//! without naming the framework.
//!
//! # Reader tokens (#292)
//!
//! Concurrent snapshot reads run on cloned database handles, each of which mints
//! its own cancellation token. A snapshot registers that token here
//! ([`register_reader`](AnalysisCancelSource::register_reader)) for as long as it
//! lives, and [`request_cancellation`](AnalysisCancelSource::request_cancellation)
//! fires every registered reader token **after** the bound worker token — so a
//! write (or a shutdown) unwinds not only the worker's own in-flight analysis but
//! every live snapshot read, and each drops its cloned handle promptly. The
//! registration is RAII: a snapshot that serves and drops deregisters itself, so
//! the set is exactly the snapshots currently in flight.

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering, fence};
use std::sync::{Arc, Mutex, PoisonError};

/// A shareable handle that requests cancellation of a bound database's in-flight
/// analysis and tracks the write epoch used to classify the resulting unwind.
///
/// Clones share one inner state, so a request made through any clone is observed
/// by all of them — the writer thread and the thread that catches the unwind can
/// hold separate clones of the same source.
#[derive(Clone, Default)]
pub struct AnalysisCancelSource {
    inner: Arc<SourceInner>,
}

#[derive(Default)]
struct SourceInner {
    /// Write epoch. Bumped by `request_cancellation` BEFORE the token fires.
    epoch: AtomicU64,
    /// The bound database handle's cancellation token, if any.
    token: Mutex<Option<salsa::CancellationToken>>,
    /// The cancellation tokens of every in-flight snapshot read, each paired with
    /// the id that deregisters it (#292). Fired after the worker token so a write
    /// unwinds every live reader clone, not only the worker's own analysis.
    reader_tokens: Mutex<Vec<(u64, salsa::CancellationToken)>>,
    /// Source of the deregistration ids above; only ever incremented.
    next_reader_id: AtomicU64,
}

impl AnalysisCancelSource {
    /// A source bound to no database yet. Firing it is a no-op until a handle
    /// binds its token (see [`crate::RootDatabase::bind_cancellation`]).
    #[must_use]
    pub fn detached() -> Self {
        Self::default()
    }

    /// Starts a new epoch, then asks the bound database to cancel its in-flight
    /// query. Returns the new epoch, which the caller stamps on the write it is
    /// about to apply.
    ///
    /// Ordering: the epoch bump is sequenced before a `SeqCst` fence, which is
    /// sequenced before the token store; an observer that saw the cancellation
    /// (the token bit) and then runs [`epoch`](Self::epoch) (fence, then load) is
    /// guaranteed to see at least this bump — the fence/fence pairing is what
    /// makes superseded-vs-residual classification exact, not best-effort.
    ///
    /// The worker token fires first, then every registered reader token (#292),
    /// so a write unwinds the worker's own in-flight analysis and every live
    /// snapshot read. Firing a token is a plain atomic store — no Salsa write —
    /// so this stays callable from the router thread without touching storage.
    ///
    /// The lock scopes only read/iterate a `Vec`/`Option`, so a poisoning panic
    /// cannot leave them observably inconsistent; a poisoned guard is recovered
    /// with [`PoisonError::into_inner`] rather than propagated.
    #[must_use = "the returned epoch identifies the write this cancellation clears the way for"]
    pub fn request_cancellation(&self) -> u64 {
        let epoch = self.inner.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        fence(Ordering::SeqCst);
        if let Some(token) = self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            token.cancel();
        }
        for (_, token) in self
            .inner
            .reader_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            token.cancel();
        }
        epoch
    }

    /// The current write epoch (fence-then-load; see
    /// [`request_cancellation`](Self::request_cancellation)).
    #[must_use]
    pub fn epoch(&self) -> u64 {
        fence(Ordering::SeqCst);
        self.inner.epoch.load(Ordering::SeqCst)
    }

    /// Binds `token` as the handle this source cancels. Called by the database
    /// when a handle asks to be interruptible; a later bind replaces the token,
    /// so a rebuilt handle (fresh token) re-arms the source.
    pub(crate) fn bind(&self, token: salsa::CancellationToken) {
        *self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(token);
    }

    /// Registers a snapshot read's cancellation `token` so a later
    /// [`request_cancellation`](Self::request_cancellation) unwinds that read
    /// (#292). The returned guard deregisters the token when it drops, so the
    /// registered set is exactly the snapshots currently in flight — the worker
    /// mints the guard when it plans a read and the snapshot holds it until it
    /// serves and drops.
    pub(crate) fn register_reader(
        &self,
        token: salsa::CancellationToken,
    ) -> ReaderTokenRegistration {
        let id = self.inner.next_reader_id.fetch_add(1, Ordering::SeqCst);
        self.inner
            .reader_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((id, token));
        ReaderTokenRegistration {
            source: Arc::clone(&self.inner),
            id,
        }
    }

    /// Test-only: fires the bound token WITHOUT starting a new epoch, so an
    /// unwind classifies as a residual self-cancel (the retry arm). Deliberately
    /// worker-token-only — the residual-self-cancel unit tests rely on it not
    /// disturbing any reader registration.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug_fire_token_only(&self) {
        if let Some(token) = self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            token.cancel();
        }
    }

    /// Test-only: the number of reader tokens currently registered, for the
    /// write-turn tripwire (a write must observe zero live readers for the path
    /// it mutates) and the concurrency tests.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    #[must_use = "the reader-token count is the reason to call this"]
    pub fn debug_reader_token_count(&self) -> usize {
        self.inner
            .reader_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// RAII deregistration handle for a snapshot read's cancellation token (#292).
///
/// Held by the snapshot for its whole lifetime; dropping it removes the token
/// from the source, so a served-and-dropped read leaves no stale token behind
/// for a later [`request_cancellation`](AnalysisCancelSource::request_cancellation)
/// to fire.
pub(crate) struct ReaderTokenRegistration {
    source: Arc<SourceInner>,
    id: u64,
}

impl Drop for ReaderTokenRegistration {
    fn drop(&mut self) {
        self.source
            .reader_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(id, _)| *id != self.id);
    }
}

/// Whether a caught unwind payload is the semantic layer's cancellation signal
/// (delivered via `resume_unwind`, bypassing the panic hook) rather than a
/// genuine panic.
///
/// The protocol layer catches an analysis unwind without naming the semantic
/// framework; this predicate is the one place that inspects the payload type, so
/// a caught cancellation can be answered as "retry against the new content"
/// while a real panic still tears down and rebuilds the host.
#[must_use]
pub fn is_cancellation(payload: &(dyn Any + Send)) -> bool {
    payload.is::<salsa::Cancelled>()
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AnalysisCancelSource>();
};

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

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering, fence};
use std::sync::{Arc, Mutex};

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
    /// # Panics
    ///
    /// Panics if the token lock was poisoned by a thread that panicked while
    /// holding it; the guarded scope only reads an `Option`, which cannot panic,
    /// so a poison does not arise in practice.
    #[must_use = "the returned epoch identifies the write this cancellation clears the way for"]
    pub fn request_cancellation(&self) -> u64 {
        let epoch = self.inner.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        fence(Ordering::SeqCst);
        if let Some(token) = self
            .inner
            .token
            .lock()
            .expect("cancellation token lock")
            .as_ref()
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
        *self.inner.token.lock().expect("cancellation token lock") = Some(token);
    }

    /// Test-only: fires the bound token WITHOUT starting a new epoch, so an
    /// unwind classifies as a residual self-cancel (the retry arm).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug_fire_token_only(&self) {
        if let Some(token) = self
            .inner
            .token
            .lock()
            .expect("cancellation token lock")
            .as_ref()
        {
            token.cancel();
        }
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

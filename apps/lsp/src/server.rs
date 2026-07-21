//! The server state and the router/worker message loop.
//!
//! [`ServerState`] holds the analysis host and the set of open documents; it turns
//! one request into one [`Response`] and one notification into the diagnostics to
//! publish, with no I/O of its own — which is what makes it directly testable.
//!
//! [`run`] splits the session across two threads so an in-flight analysis stays
//! interruptible even though analysis itself is strictly serial (issue #157). A
//! **worker** thread ([`worker_loop`]) owns the [`ServerState`] and the sole
//! analysis handle and processes jobs one at a time — today's message loop, one
//! thread over. A **router** thread ([`router_loop`]) reads the transport and
//! forwards each message to the worker instantly over an unbounded [`Job`] channel,
//! handling inline only what must not wait behind an analysis: incoming
//! request-id bookkeeping, `$/cancelRequest`, and requesting cancellation of the
//! worker's in-flight analysis before it forwards an adopted document write. Every
//! response and publish leaves from the worker; nothing here prints to stdout,
//! which is the protocol channel.

use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use inference_ide::{AnalysisCancelSource, AnalysisHost, is_cancellation};
use lsp_server::{
    Connection, ErrorCode, ExtractError, Message, Notification, ReqQueue, Request, RequestId,
    Response,
};
use lsp_types::notification::{
    Cancel, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit,
    Notification as _, PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Initialize, InlayHintRequest,
    Request as _, Shutdown,
};
use lsp_types::{
    CancelParams, InitializeParams, MarkupKind, NumberOrString, PublishDiagnosticsParams, Uri,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{capabilities, handlers};

/// The server name reported in the initialize result's `serverInfo` (the crate
/// name), which clients surface in their logs and crash reports.
const SERVER_NAME: &str = env!("CARGO_PKG_NAME");
/// The server version reported in the initialize result's `serverInfo` (the crate
/// version).
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The stack each of the server's threads runs on. The analysis pipeline
/// (type-checker, analysis passes) recurses with the input's nesting depth, so a
/// pathological or generated document can overflow the default stack and abort the
/// whole process — taking every open document's state with it. A stack overflow
/// aborts rather than unwinds, so a thread cannot *catch* it; the mitigation is
/// headroom. 64 MiB (mirroring rust-analyzer's main-loop stack) clears realistic
/// deep nesting by a wide margin. A thread must set this explicitly: a spawned
/// thread's default stack is far smaller than the main thread's. `main` runs the
/// router on a thread of this size and the analysis worker — where the deep
/// recursion now happens — is spawned with the same headroom.
pub(crate) const SERVER_STACK_SIZE: usize = 64 * 1024 * 1024;

/// A document the editor has opened, with the path the analysis host knows it by
/// and the last version the editor reported (echoed back in published
/// diagnostics so the client can correlate them).
pub(crate) struct Document {
    pub(crate) path: PathBuf,
    pub(crate) version: i32,
    /// The document's last-seen text. Retained so [`ServerState`] can rebuild the
    /// analysis host from the tracked documents after a contained panic, without
    /// re-reading anything from disk (the editor's overlay may never have been
    /// saved).
    pub(crate) text: Arc<str>,
}

/// The client capabilities this server negotiates once during initialize and then
/// consults while answering requests.
#[derive(Clone, Copy)]
pub(crate) struct NegotiatedCapabilities {
    /// The client accepts the hierarchical document-symbol response; when it does
    /// not, symbols are flattened to `SymbolInformation`.
    pub(crate) hierarchical_symbols: bool,
    /// The client accepts Markdown hover content; when it does not, hover contents
    /// are emitted as plain text (no code fences or backticks).
    pub(crate) hover_markdown: bool,
}

impl NegotiatedCapabilities {
    /// Reads the negotiated bits out of the client's `initialize` capabilities,
    /// defaulting each to the LSP fallback when the client is silent.
    fn from_init_params(init_params: &InitializeParams) -> Self {
        Self {
            hierarchical_symbols: hierarchical_symbol_support(init_params),
            hover_markdown: hover_markdown_support(init_params),
        }
    }

    /// The markup kind to render hover contents in for this client.
    pub(crate) fn hover_format(self) -> MarkupKind {
        if self.hover_markdown {
            MarkupKind::Markdown
        } else {
            MarkupKind::PlainText
        }
    }
}

/// The analysis host plus per-document bookkeeping. Feature queries and
/// diagnostics are answered against this; the transport is elsewhere.
pub(crate) struct ServerState {
    pub(crate) host: AnalysisHost,
    pub(crate) documents: FxHashMap<Uri, Document>,
    pub(crate) capabilities: NegotiatedCapabilities,
    /// Open documents a recent change invalidated but has not yet been
    /// republished. Filled by [`on_notification`](Self::on_notification) and
    /// drained when the message loop goes idle (see [`run`]), so an interactive
    /// request arriving right after a keystroke is answered before the other
    /// affected documents are recomputed.
    pending_republish: FxHashSet<Uri>,
    /// Cancellation handle bound to [`host`](Self::host). Firing it interrupts an
    /// in-flight analysis at its next checkpoint; the write epoch it carries is
    /// what tells a caught cancellation apart from a residual self-cancel.
    cancel_source: AnalysisCancelSource,
    /// The write epoch stamped on the job currently being processed; a caught
    /// cancellation with a newer source epoch means a write superseded this work.
    job_epoch: u64,
}

impl ServerState {
    /// Builds a state bound to a fresh detached cancellation source, so a unit test
    /// can fire `cancel_source` and reach the real database token. The worker uses
    /// [`with_cancel_source`](Self::with_cancel_source) instead, to share the
    /// router's source; this convenience has no production caller.
    #[cfg(test)]
    pub(crate) fn new(capabilities: NegotiatedCapabilities) -> Self {
        Self::with_cancel_source(capabilities, AnalysisCancelSource::detached())
    }

    /// Builds a state whose host is bound to `cancel_source`, so a cancellation
    /// requested through that source interrupts this state's analyses. `new`
    /// binds a fresh detached source; the two entry points differ only in whether
    /// the source is shared with another thread.
    pub(crate) fn with_cancel_source(
        capabilities: NegotiatedCapabilities,
        cancel_source: AnalysisCancelSource,
    ) -> Self {
        let host = AnalysisHost::default();
        host.bind_cancellation(&cancel_source);
        Self {
            host,
            documents: FxHashMap::default(),
            capabilities,
            pending_republish: FxHashSet::default(),
            cancel_source,
            job_epoch: 0,
        }
    }

    /// Stamps this turn with the epoch the router forwarded the job under, so a
    /// cancellation caught while processing it is classified against the write
    /// state current when the job was routed. The worker calls this before
    /// dispatching each job.
    fn begin_turn(&mut self, epoch: u64) {
        self.job_epoch = epoch;
    }

    /// Adopts the source's current epoch as this turn's baseline, so a
    /// cancellation caught afterward is classified against work that started now
    /// rather than against whatever job last ran. Called before the idle and
    /// shutdown drains, whose publishes are their own units of work.
    fn refresh_turn(&mut self) {
        self.job_epoch = self.cancel_source.epoch();
    }

    /// Whether a write has been requested since this turn's baseline epoch — i.e.
    /// the work in flight is stale and a caught cancellation should be answered
    /// ContentModified rather than retried.
    fn superseded(&self) -> bool {
        self.cancel_source.epoch() > self.job_epoch
    }

    /// The worker's request-dispatch wrapper: a request whose job the router
    /// forwarded before a newer write is answered ContentModified without
    /// computing at all.
    ///
    /// The dispatch-time fast-fail lives here rather than inside
    /// [`handle_request_resilient`](Self::handle_request_resilient) because the
    /// unit tests call that method directly and rely on it computing; only a job
    /// routed through the worker carries a `job_epoch` older than a landed write.
    pub(crate) fn respond_to_request(&mut self, request: Request) -> Response {
        if self.superseded() {
            return content_modified_response(request.id);
        }
        self.handle_request_resilient(request)
    }

    /// Routes a request through [`handle_request`](Self::handle_request),
    /// containing any panic in the analysis stack.
    ///
    /// A `todo!`/`unwrap` deep in the type-checker or analysis passes (the class
    /// tracked in #240) unwinds; left unguarded it would tear down the whole
    /// session. Caught here, the analysis host is rebuilt from the tracked open
    /// documents — the same recovery the notification path takes — the offending
    /// request is answered with an `InternalError` carrying its original id (so
    /// the client can correlate the failure), and every other document keeps
    /// working. A stack overflow aborts the process on its own and cannot be
    /// caught; that is intentionally left to abort.
    ///
    /// A memoizing query does mutate the host: it bumps the analysis generation
    /// stamp before computing an analysis and stores the finished analysis only
    /// afterward, so a panic partway through leaves at most a bumped stamp and
    /// never a half-built cache entry. The unconditional rebuild does not lean on
    /// that invariant — it keeps the recovery identical to the notification path
    /// and robust to future changes in what a query mutates.
    ///
    /// A caught *cancellation* (not a panic) is classified against the write
    /// epoch: if a newer write superseded this request, it is answered
    /// ContentModified so the client retries against the new content; the host is
    /// left intact, because a cancelled analysis leaves no memo behind. A
    /// cancellation with no newer write behind it is a residual self-cancel — the
    /// unwind already consumed the signal — so the request is simply retried; a
    /// genuinely newer write always arrives with a newer epoch, which the
    /// supersede arm catches, bounding the retry.
    pub(crate) fn handle_request_resilient(&mut self, request: Request) -> Response {
        loop {
            let id = request.id.clone();
            match catch(|| self.handle_request(request.clone())) {
                Caught::Completed(response) => return response,
                Caught::Canceled if self.superseded() => return content_modified_response(id),
                Caught::Canceled => {}
                Caught::Panicked => {
                    self.rebuild_host();
                    return panic_response(id);
                }
            }
        }
    }

    /// Routes a request to its handler, producing the response to send back. An
    /// unknown method is a `MethodNotFound` error; malformed params are
    /// `InvalidParams`; neither disturbs the server.
    pub(crate) fn handle_request(&mut self, request: Request) -> Response {
        if request.method == HoverRequest::METHOD {
            self.dispatch::<HoverRequest>(request, handlers::hover)
        } else if request.method == GotoDefinition::METHOD {
            self.dispatch::<GotoDefinition>(request, handlers::goto_definition)
        } else if request.method == Completion::METHOD {
            self.dispatch::<Completion>(request, handlers::completion)
        } else if request.method == DocumentSymbolRequest::METHOD {
            self.dispatch::<DocumentSymbolRequest>(request, handlers::document_symbol)
        } else if request.method == InlayHintRequest::METHOD {
            self.dispatch::<InlayHintRequest>(request, handlers::inlay_hint)
        } else if request.method == Initialize::METHOD {
            // `initialize` is a once-per-session lifecycle request; the handshake
            // already consumed the only valid one, so a second is InvalidRequest —
            // not the misleading MethodNotFound the generic arm below would give.
            Response::new_err(
                request.id,
                ErrorCode::InvalidRequest as i32,
                "the server is already initialized".to_owned(),
            )
        } else {
            Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported request: {}", request.method),
            )
        }
    }

    /// Deserializes the request's params for `R`, runs `handler`, and wraps the
    /// result — turning a params-deserialization failure into `InvalidParams`.
    fn dispatch<R>(
        &mut self,
        request: Request,
        handler: fn(&mut ServerState, R::Params) -> R::Result,
    ) -> Response
    where
        R: lsp_types::request::Request,
    {
        let id = request.id.clone();
        match request.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => Response::new_ok(id, handler(self, params)),
            Err(ExtractError::JsonError { error, .. }) => {
                Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string())
            }
            Err(ExtractError::MethodMismatch(request)) => Response::new_err(
                id,
                ErrorCode::InvalidRequest as i32,
                format!(
                    "method mismatch: expected {}, got {}",
                    R::METHOD,
                    request.method
                ),
            ),
        }
    }

    /// Applies a document notification through
    /// [`on_notification`](Self::on_notification), containing any panic in the
    /// analysis stack.
    ///
    /// Diagnostics are computed on the message loop thread, so a panic while
    /// analyzing a just-opened or just-changed document would otherwise abort the
    /// session — and, because the client re-sends the same `didOpen` on restart,
    /// crash-loop until it gives up (#241). Caught here, the failed notification
    /// publishes nothing, and the analysis host — which the unwinding computation
    /// may have left with half-updated state — is rebuilt from the tracked open
    /// documents so later queries start from a clean, consistent host.
    pub(crate) fn on_notification_resilient(
        &mut self,
        notification: Notification,
    ) -> Vec<PublishDiagnosticsParams> {
        loop {
            match catch(|| self.on_notification(notification.clone())) {
                Caught::Completed(publishes) => return publishes,
                Caught::Canceled if self.superseded() => {
                    // The write applied but its eager publish was superseded by a
                    // still-newer write behind us: requeue rather than lose it.
                    if let Some(uri) = notification_document_uri(&notification)
                        .and_then(|raw| Uri::from_str(raw).ok())
                        && self.documents.contains_key(&uri)
                    {
                        self.pending_republish.insert(uri.clone());
                        self.queue_invalidated_dependents(&uri);
                    }
                    return Vec::new();
                }
                Caught::Canceled => {}
                Caught::Panicked => {
                    self.rebuild_host();
                    return Vec::new();
                }
            }
        }
    }

    /// Applies a document notification, eagerly returning the diagnostics to
    /// publish for *only* the notified document and queuing every other open
    /// document the change invalidated for a deferred republish (see
    /// [`queue_invalidated_dependents`](Self::queue_invalidated_dependents) and
    /// [`drain_pending_republishes`](Self::drain_pending_republishes)). An unknown
    /// or unparsable notification — or a `didChange` for a document that was never
    /// opened (#275) — publishes nothing and queues nothing.
    pub(crate) fn on_notification(
        &mut self,
        notification: Notification,
    ) -> Vec<PublishDiagnosticsParams> {
        let primary = if notification.method == DidOpenTextDocument::METHOD {
            match notification.extract(DidOpenTextDocument::METHOD) {
                Ok(params) => handlers::did_open(self, params),
                Err(_) => return Vec::new(),
            }
        } else if notification.method == DidChangeTextDocument::METHOD {
            match notification.extract(DidChangeTextDocument::METHOD) {
                Ok(params) => handlers::did_change(self, params),
                Err(_) => return Vec::new(),
            }
        } else if notification.method == DidCloseTextDocument::METHOD {
            match notification.extract(DidCloseTextDocument::METHOD) {
                Ok(params) => handlers::did_close(self, params),
                Err(_) => return Vec::new(),
            }
        } else {
            return Vec::new();
        };
        let Some(primary) = primary else {
            return Vec::new();
        };
        // The notified document was just published fresh, so it no longer owes a
        // deferred republish even if an earlier change had queued it.
        self.pending_republish.remove(&primary.uri);
        self.queue_invalidated_dependents(&primary.uri);
        vec![primary]
    }

    /// Queues every *other* open document the just-applied change invalidated.
    ///
    /// A change to one file can invalidate another open document whose import
    /// closure includes it — `ide-db` drops exactly those analyses. After the
    /// notified document's own publish has recomputed and re-memoized its
    /// analysis, an open document whose analysis is no longer memoized is one this
    /// change invalidated; it is queued for a deferred republish rather than
    /// recomputed inside this notification turn. Documents the change left
    /// untouched keep their memoized analysis and are not queued, so the client
    /// never sees a needless republish and never keeps a stale one.
    fn queue_invalidated_dependents(&mut self, changed: &Uri) {
        let invalidated: Vec<Uri> = self
            .documents
            .iter()
            .filter(|(uri, document)| {
                *uri != changed && !self.host.is_document_analyzed(&document.path)
            })
            .map(|(uri, _)| uri.clone())
            .collect();
        self.pending_republish.extend(invalidated);
    }

    /// Drains the pending-republish set into a fresh publish per queued document,
    /// containing any analysis panic so one poisoned document cannot lose the
    /// others' publishes.
    ///
    /// Called when the message loop goes idle and when the client shuts down, so a
    /// document a keystroke invalidated is refreshed before the loop blocks and
    /// pending publishes are never dropped on the way out.
    pub(crate) fn drain_pending_republishes(&mut self) -> Vec<PublishDiagnosticsParams> {
        self.refresh_turn();
        let mut pending: VecDeque<Uri> = self.pending_republish.drain().collect();
        let mut publishes = Vec::with_capacity(pending.len());
        while let Some(uri) = pending.pop_front() {
            match catch(|| handlers::publish_diagnostics_params(self, &uri)) {
                Caught::Completed(params) => publishes.push(params),
                Caught::Canceled if self.superseded() => {
                    // A newer write is queued behind this drain; put this uri and
                    // the untried remainder back for the post-write drain.
                    self.pending_republish.insert(uri);
                    self.pending_republish.extend(pending);
                    break;
                }
                Caught::Canceled => pending.push_front(uri), // residual: retry this uri
                Caught::Panicked => {} // existing behavior: skip the poisoned uri, no rebuild
            }
        }
        publishes
    }

    /// If `uri` is awaiting a deferred republish, publishes its now-fresh
    /// diagnostics and removes it from the pending set; otherwise `None`.
    ///
    /// A feature request against a queued document recomputes that document's
    /// analysis on demand, so the client is already getting fresh answers; this
    /// also refreshes its diagnostics and drops it from the queue so the idle
    /// drain does not redo the work. Any analysis panic is contained.
    pub(crate) fn publish_if_pending(&mut self, uri: &Uri) -> Option<PublishDiagnosticsParams> {
        if !self.pending_republish.remove(uri) {
            return None;
        }
        self.refresh_turn();
        loop {
            match catch(|| handlers::publish_diagnostics_params(self, uri)) {
                Caught::Completed(params) => return Some(params),
                Caught::Canceled if self.superseded() => {
                    self.pending_republish.insert(uri.clone());
                    return None;
                }
                Caught::Canceled => {}
                Caught::Panicked => return None,
            }
        }
    }

    /// Replaces the analysis host with a fresh one carrying only the tracked open
    /// documents' last-seen text.
    ///
    /// Called after a contained analysis panic: rather than reason about which
    /// cached state the unwinding computation may have corrupted, the host is
    /// discarded and rebuilt from the [`documents`](Self::documents) the server
    /// still considers open. The first query after this recomputes every analysis
    /// from scratch. Documents are the sole source of truth here, so anything the
    /// editor had not opened is simply gone — which is correct.
    ///
    /// It must never run a semantic query — it only re-applies overlays — because
    /// it executes outside [`catch`] while a cancellation may still be pending; a
    /// query here could unwind uncontained. The fresh host mints a fresh
    /// cancellation token, so the source is rebound before returning: forgetting
    /// this would silently disable all cancellation after the first contained
    /// panic.
    fn rebuild_host(&mut self) {
        let mut host = AnalysisHost::default();
        for document in self.documents.values() {
            host.open_document(&document.path, Arc::clone(&document.text));
        }
        self.host = host;
        self.host.bind_cancellation(&self.cancel_source);
    }
}

/// Runs the initialize handshake, answering the `initialize` request directly so a
/// malformed `InitializeParams` fails *that request* instead of aborting the
/// process after the handshake has already completed.
///
/// `lsp-server`'s convenience `Connection::initialize` returns the raw params and
/// hard-codes the result body, so the caller can only deserialize *after* the
/// handshake and cannot attach `serverInfo`. This drives `initialize_start` /
/// `initialize_finish` instead: the params are validated first, and only a valid
/// `initialize` completes the handshake — with `serverInfo` (name and version)
/// attached. A malformed one is answered with an `InvalidParams` error and the
/// session ends without ever entering the message loop.
///
/// Returns the negotiated params on success, or `None` when initialize failed (the
/// error response has already been sent, and the client is expected to `exit`).
///
/// # Errors
///
/// Returns an error if the handshake cannot be driven to completion — the
/// connection dropped, or a message could not be written.
pub fn initialize(connection: &Connection) -> anyhow::Result<Option<InitializeParams>> {
    let (id, params) = connection.initialize_start()?;
    let init_params: InitializeParams = match serde_json::from_value(params) {
        Ok(params) => params,
        Err(error) => {
            send(
                connection,
                Message::Response(Response::new_err(
                    id,
                    ErrorCode::InvalidParams as i32,
                    format!("invalid initialize params: {error}"),
                )),
            )?;
            drain_until_exit(connection);
            return Ok(None);
        }
    };

    let result = serde_json::json!({
        "capabilities": capabilities::server_capabilities(),
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
    });
    connection.initialize_finish(id, result)?;
    Ok(Some(init_params))
}

/// Drains messages until the client sends `exit` or closes the connection.
///
/// Used after a failed `initialize`: it keeps the process alive long enough for
/// the error response to be written, and keeps a receiver on the channel so the
/// transport's reader thread is never left blocked forwarding into it.
fn drain_until_exit(connection: &Connection) {
    for message in &connection.receiver {
        if matches!(&message, Message::Notification(n) if n.method == Exit::METHOD) {
            return;
        }
    }
}

/// Runs one session as a router thread plus an analysis worker thread, returning
/// when the client exits or the connection closes.
///
/// # Router / worker split
///
/// Analysis is strictly serial — the semantic stack holds `!Send` state and one
/// message is answered at a time — but an in-flight analysis must still be
/// *interruptible* so a fresh keystroke does not wait out a stale request (issue
/// #157). The work is split across two threads with no shared analysis state:
///
/// * The **worker** ([`worker_loop`]) owns the [`ServerState`] and the sole
///   analysis handle, and runs today's message loop over an unbounded [`Job`]
///   channel: one job at a time, in arrival order. Every response and every
///   publish leaves from here.
/// * The **router** ([`router_loop`]) reads the transport directly and forwards
///   each message to the worker *instantly*, so nothing that must not wait behind
///   an analysis does. It keeps the incoming request-id bookkeeping, answers
///   `$/cancelRequest`, and — for a document write it adopts (and for
///   shutdown/exit) — requests cancellation of the worker's in-flight analysis
///   *before* forwarding the write, stamping the write's job with the post-bump
///   write epoch so the worker classifies its own eager publish as current.
///
/// Because the router forwards without blocking, the unbounded job channel is the
/// buffer a typing burst accumulates in; the worker collapses consecutive
/// same-document changes at dequeue ([`coalesced_job_batch`]), so a burst runs the
/// pipeline a handful of times instead of once per keystroke. A `didOpen`/
/// `didClose` for that document, or any request, is a barrier the coalescer never
/// reorders across, and no non-`didChange` job is ever dropped.
///
/// A caught cancellation is not a panic: a request superseded by a newer write is
/// answered ContentModified (the client retries against the new content) with the
/// analysis cache left intact, while a residual self-cancel is simply retried.
///
/// # Deferred dependents
///
/// A notification publishes eagerly only for the changed document; every other
/// open document it invalidated is queued ([`ServerState::on_notification`]) and
/// republished when the worker next goes idle — after the interactive request that
/// arrived right behind the keystroke has already been answered. The queue is
/// drained before the worker parks on the next job, a request against a queued
/// document publishes it fresh immediately ([`ServerState::publish_if_pending`]),
/// and a shutdown flushes it, so a client never keeps a stale diagnostic set.
///
/// # Shutdown handshake
///
/// The shutdown handshake is handled inline rather than delegated to
/// `lsp-server`'s `Connection::handle_shutdown`, which consumes the next message
/// itself and turns anything but `exit` into a fatal protocol error. Instead, a
/// `shutdown` request is answered and flips the worker's `shutting_down` flag;
/// while it is set, every further request — including a repeated `shutdown` — is
/// answered with `InvalidRequest` and every notification but `exit` is ignored.
/// The `exit` notification ends the loop.
///
/// # Teardown
///
/// The threads are joined through [`std::thread::scope`], so the worker may borrow
/// the [`Connection`] for its sender without any `'static` bound. When the router
/// returns it drops the job sender, so the worker unblocks on channel disconnect
/// even if it never saw the exit job, and the scope guarantees the join on every
/// path.
///
/// # Errors
///
/// Returns an error if a message cannot be written to the transport, or if the
/// analysis worker thread panicked outside the contained analysis boundary.
pub fn run(connection: &Connection, init_params: &InitializeParams) -> anyhow::Result<()> {
    let capabilities = NegotiatedCapabilities::from_init_params(init_params);
    let cancel = AnalysisCancelSource::detached();
    let req_queue: Mutex<ReqQueue<(), ()>> = Mutex::new(ReqQueue::default());
    let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();

    thread::scope(|scope| {
        let worker_cancel = cancel.clone();
        let worker_req_queue = &req_queue;
        let worker = thread::Builder::new()
            .name("inference-lsp-analysis".to_owned())
            .stack_size(SERVER_STACK_SIZE)
            .spawn_scoped(scope, move || {
                worker_loop(
                    jobs_rx,
                    connection,
                    capabilities,
                    worker_cancel,
                    worker_req_queue,
                )
            })?;

        let router_result = router_loop(connection, jobs_tx, &cancel, &req_queue);
        // `jobs_tx` moved into the router and dropped when it returned, so the
        // worker unblocks on disconnect even if it never saw the exit job; the
        // scope then guarantees the join on every path.
        let worker_result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("the analysis worker panicked"))?;
        router_result?;
        worker_result
    })
}

/// A transport message paired with the write epoch the router forwarded it under,
/// so the worker can tell a cancellation that superseded this job apart from a
/// residual self-cancel.
struct Job {
    epoch: u64,
    message: Message,
}

/// Routes transport messages to the analysis worker in arrival order, handling
/// only what must not wait behind an analysis: incoming request-id bookkeeping,
/// `$/cancelRequest`, and requesting cancellation for adopted document writes.
///
/// Cancellation for a write is requested *strictly before* the write is forwarded
/// and the job is stamped with the post-bump epoch, so the worker always
/// classifies the write's own eager publish as current (a residual self-cancel it
/// retries) rather than as superseded.
fn router_loop(
    connection: &Connection,
    jobs: mpsc::Sender<Job>,
    cancel: &AnalysisCancelSource,
    req_queue: &Mutex<ReqQueue<(), ()>>,
) -> anyhow::Result<()> {
    let mut tracked: FxHashSet<String> = FxHashSet::default();
    for message in &connection.receiver {
        let mut epoch = cancel.epoch();
        match &message {
            Message::Request(request) => {
                req_queue
                    .lock()
                    .expect("request queue lock")
                    .incoming
                    .register(request.id.clone(), ());
                // Shutdown aborts any in-flight seam-slow compute within one poll
                // instead of stalling teardown while the worker finishes it.
                if request.method == Shutdown::METHOD {
                    epoch = cancel.request_cancellation();
                }
            }
            Message::Notification(notification) => {
                if notification.method == Cancel::METHOD {
                    handle_cancel_notification(connection, req_queue, notification)?;
                    continue; // Consumed here: bookkeeping only, never forwarded.
                }
                if notification.method == Exit::METHOD
                    || is_adopted_write(&mut tracked, notification)
                {
                    epoch = cancel.request_cancellation();
                }
            }
            Message::Response(_) => {}
        }
        let exit = matches!(&message, Message::Notification(n) if n.method == Exit::METHOD);
        if jobs.send(Job { epoch, message }).is_err() {
            break; // The worker is gone; stop routing.
        }
        if exit {
            break;
        }
    }
    Ok(())
}

/// Mirrors the worker's adoption decision from the message stream alone, using the
/// same checks the handlers apply: a `didOpen` is adopted iff its URI maps to a
/// path; a `didChange` iff its document is tracked and carries content changes; a
/// `didClose` iff its document was tracked. FIFO delivery keeps this mirror and the
/// worker's `documents` map consistent for every mappable URI.
///
/// A benign desync (a client spelling one document two ways across its lifecycle)
/// degrades gracefully in both directions: a spurious fire is a harmless
/// ContentModified under the epoch protocol, and a missed fire only loses
/// preemption — the worker still applies the write and publishes.
fn is_adopted_write(tracked: &mut FxHashSet<String>, notification: &Notification) -> bool {
    let Some(raw) = notification_document_uri(notification) else {
        return false;
    };
    if notification.method == DidOpenTextDocument::METHOD {
        let mappable = Uri::from_str(raw)
            .ok()
            .as_ref()
            .and_then(crate::uri::to_path)
            .is_some();
        if mappable {
            tracked.insert(raw.to_owned());
        }
        mappable
    } else if notification.method == DidChangeTextDocument::METHOD {
        tracked.contains(raw) && has_content_changes(notification)
    } else if notification.method == DidCloseTextDocument::METHOD {
        tracked.remove(raw)
    } else {
        false
    }
}

/// Whether a `didChange` notification carries a non-empty `contentChanges` array.
/// An empty one applies nothing (the worker ignores it), so it is not a write for
/// cancellation purposes.
fn has_content_changes(notification: &Notification) -> bool {
    notification
        .params
        .get("contentChanges")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|changes| !changes.is_empty())
}

/// Answers `$/cancelRequest` from the router: a still-pending id is completed and
/// answered RequestCanceled (-32800) immediately, and the worker's late response
/// for it is then suppressed by the completion gate. The in-flight compute is not
/// aborted (bookkeeping only, rust-analyzer parity).
///
/// Parsed error-tolerantly — a malformed one is dropped rather than propagated, so
/// the router stays infallible on client input.
fn handle_cancel_notification(
    connection: &Connection,
    req_queue: &Mutex<ReqQueue<(), ()>>,
    notification: &Notification,
) -> anyhow::Result<()> {
    let Ok(params) = serde_json::from_value::<CancelParams>(notification.params.clone()) else {
        return Ok(());
    };
    let id = match params.id {
        NumberOrString::Number(n) => RequestId::from(n),
        NumberOrString::String(s) => RequestId::from(s),
    };
    if let Some(response) = req_queue
        .lock()
        .expect("request queue lock")
        .incoming
        .cancel(id)
    {
        send(connection, Message::Response(response))?;
    }
    Ok(())
}

/// The analysis worker: owns [`ServerState`] and processes jobs strictly in order
/// — today's message loop, one thread over. It borrows the connection only for its
/// sender (every response and publish leaves from here, except the router-built
/// -32800) and must never read `connection.receiver`. It breaks on the exit job
/// and on channel disconnect, so teardown cannot hang.
fn worker_loop(
    jobs: Receiver<Job>,
    connection: &Connection,
    capabilities: NegotiatedCapabilities,
    cancel: AnalysisCancelSource,
    req_queue: &Mutex<ReqQueue<(), ()>>,
) -> anyhow::Result<()> {
    let mut state = ServerState::with_cancel_source(capabilities, cancel);
    let mut shutting_down = false;

    loop {
        let job = match jobs.try_recv() {
            Ok(job) => job,
            Err(TryRecvError::Empty) => {
                // The backlog is empty: refresh every document a recent change
                // invalidated before parking on the next job, so no queued
                // republish is left indefinitely stale.
                publish_all(connection, state.drain_pending_republishes())?;
                match jobs.recv() {
                    Ok(job) => job,
                    Err(_) => return Ok(()),
                }
            }
            Err(TryRecvError::Disconnected) => return Ok(()),
        };

        for job in coalesced_job_batch(job, &jobs) {
            state.begin_turn(job.epoch);
            if handle_message(connection, req_queue, &mut state, &mut shutting_down, job.message)?
                .is_break()
            {
                return Ok(());
            }
        }
    }
}

/// Whether the message loop should keep running or return after a message.
#[derive(Clone, Copy)]
enum Flow {
    Continue,
    Break,
}

impl Flow {
    fn is_break(self) -> bool {
        matches!(self, Flow::Break)
    }
}

/// Handles one message: the shutdown/exit handshake, a routed request, or a
/// document notification. Returns [`Flow::Break`] only for `exit`.
///
/// Every response send goes through [`send_gated_response`], so a request the
/// router already answered RequestCanceled for is never answered a second time.
fn handle_message(
    connection: &Connection,
    req_queue: &Mutex<ReqQueue<(), ()>>,
    state: &mut ServerState,
    shutting_down: &mut bool,
    message: Message,
) -> anyhow::Result<Flow> {
    match message {
        Message::Request(request) if *shutting_down => {
            send_gated_response(
                connection,
                req_queue,
                Response::new_err(
                    request.id,
                    ErrorCode::InvalidRequest as i32,
                    "the server is shutting down".to_owned(),
                ),
            )?;
        }
        Message::Request(request) if request.method == Shutdown::METHOD => {
            *shutting_down = true;
            // Flush queued republishes before parking on `exit`, so a graceful
            // shutdown never drops a document's owed diagnostics.
            publish_all(connection, state.drain_pending_republishes())?;
            send_gated_response(connection, req_queue, Response::new_ok(request.id, ()))?;
        }
        Message::Request(request) => {
            let document = request_document_uri(&request);
            let response = state.respond_to_request(request);
            send_gated_response(connection, req_queue, response)?;
            // A request against a document a recent change invalidated recomputes
            // it on demand; publish its now-fresh diagnostics and clear it from
            // the queue so the idle drain does not redo it.
            if let Some(params) = document.and_then(|uri| state.publish_if_pending(&uri)) {
                publish_all(connection, vec![params])?;
            }
        }
        Message::Notification(notification) if notification.method == Exit::METHOD => {
            return Ok(Flow::Break);
        }
        // A stray notification after `shutdown` (other than `exit`) is dropped.
        Message::Notification(_) if *shutting_down => {}
        Message::Notification(notification) => {
            publish_all(connection, state.on_notification_resilient(notification))?;
        }
        Message::Response(_) => {}
    }
    Ok(Flow::Continue)
}

/// Sends `response` only if its request is still pending: a completion gate so a
/// request already answered RequestCanceled (-32800) by the router's
/// `$/cancelRequest` handling is never answered a second time. Completing the id
/// (removing it from the incoming queue) both records the answer and is the check.
fn send_gated_response(
    connection: &Connection,
    req_queue: &Mutex<ReqQueue<(), ()>>,
    response: Response,
) -> anyhow::Result<()> {
    if req_queue
        .lock()
        .expect("request queue lock")
        .incoming
        .complete(&response.id)
        .is_some()
    {
        send(connection, Message::Response(response))?;
    }
    Ok(())
}

/// Sends each diagnostics set as a `publishDiagnostics` notification.
fn publish_all(
    connection: &Connection,
    publishes: Vec<PublishDiagnosticsParams>,
) -> anyhow::Result<()> {
    for params in publishes {
        let published = Notification::new(PublishDiagnostics::METHOD.to_owned(), params);
        send(connection, Message::Notification(published))?;
    }
    Ok(())
}

/// The batch of jobs to process for `first`, in arrival order.
///
/// Only a `didChange` head is worth batching: the backlog buffered on the job
/// channel is drained non-blockingly and consecutive same-document changes collapse
/// to their final text ([`coalesce_by`]). Any other head is returned alone so
/// requests and lifecycle notifications keep exact arrival order and timing.
fn coalesced_job_batch(first: Job, incoming: &Receiver<Job>) -> Vec<Job> {
    if did_change_uri(&first.message).is_none() {
        return vec![first];
    }
    let mut batch = vec![first];
    while let Ok(job) = incoming.try_recv() {
        batch.push(job);
    }
    coalesce_by(batch, |job| &job.message)
}

/// Drops each item whose `didChange` a later `didChange` for the same document
/// supersedes, keeping only the final text of a burst. `message_of` projects each
/// item to the transport message it carries, so the same rule serves both the
/// worker's [`Job`] batches and the message-level unit tests.
///
/// An item at index `i` is dropped when a later same-document `didChange` appears
/// before any barrier between them: a request, or a `didOpen`/`didClose` for that
/// same document. Barriers are never reordered across — a request must observe the
/// edits that preceded it, and a lifecycle event bounds a document's edit run — and
/// no non-`didChange` item and no item for another document is ever dropped, so
/// every item's relative order is preserved.
fn coalesce_by<T>(items: Vec<T>, message_of: impl Fn(&T) -> &Message) -> Vec<T> {
    let mut keep = vec![true; items.len()];
    for (i, item) in items.iter().enumerate() {
        let Some(document) = did_change_uri(message_of(item)) else {
            continue;
        };
        for later in &items[i + 1..] {
            if is_barrier_for(message_of(later), document) {
                break;
            }
            if did_change_uri(message_of(later)) == Some(document) {
                keep[i] = false;
                break;
            }
        }
    }
    items
        .into_iter()
        .zip(keep)
        .filter_map(|(item, keep)| keep.then_some(item))
        .collect()
}

// The message-level coalescer entry points the in-file unit tests exercise
// directly. They compose the generic [`coalesce_by`] over a message stream, so the
// coalescing rule cannot drift from the worker's job-level batching.
#[cfg(test)]
fn coalesce_changes(messages: Vec<Message>) -> Vec<Message> {
    coalesce_by(messages, |message| message)
}

#[cfg(test)]
fn coalesced_batch(first: Message, incoming: &Receiver<Message>) -> Vec<Message> {
    if did_change_uri(&first).is_none() {
        return vec![first];
    }
    let mut batch = vec![first];
    while let Ok(message) = incoming.try_recv() {
        batch.push(message);
    }
    coalesce_changes(batch)
}

/// Whether `message` bars coalescing a `didChange` for `document` across it: any
/// request, or a `didOpen`/`didClose` for that same document.
fn is_barrier_for(message: &Message, document: &str) -> bool {
    match message {
        Message::Request(_) => true,
        Message::Notification(notification) => {
            matches!(
                notification.method.as_str(),
                DidOpenTextDocument::METHOD | DidCloseTextDocument::METHOD
            ) && notification_document_uri(notification) == Some(document)
        }
        Message::Response(_) => false,
    }
}

/// The document URI of a `didChange` notification, or `None` for anything else.
fn did_change_uri(message: &Message) -> Option<&str> {
    match message {
        Message::Notification(notification)
            if notification.method == DidChangeTextDocument::METHOD =>
        {
            notification_document_uri(notification)
        }
        _ => None,
    }
}

/// The `textDocument.uri` string a document notification carries, if present.
fn notification_document_uri(notification: &Notification) -> Option<&str> {
    notification
        .params
        .get("textDocument")?
        .get("uri")?
        .as_str()
}

/// The `textDocument.uri` a request targets, parsed back into a [`Uri`].
///
/// Every feature request this server handles carries its document under
/// `textDocument.uri`; a request without one (or with an unparsable one) simply
/// has no pending document to refresh.
fn request_document_uri(request: &Request) -> Option<Uri> {
    let uri = request.params.get("textDocument")?.get("uri")?.as_str()?;
    Uri::from_str(uri).ok()
}

/// Whether the client advertised support for the hierarchical document-symbol
/// response; absent capabilities mean the flat form.
fn hierarchical_symbol_support(init_params: &InitializeParams) -> bool {
    init_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.document_symbol.as_ref())
        .and_then(|document_symbol| document_symbol.hierarchical_document_symbol_support)
        .unwrap_or(false)
}

/// Whether the client accepts Markdown hover content.
///
/// The client's `textDocument.hover.contentFormat` lists the formats it supports,
/// most-preferred first. A client that lists formats but omits Markdown wants
/// plain text; a client that advertises no `contentFormat` at all keeps the
/// historical Markdown default.
fn hover_markdown_support(init_params: &InitializeParams) -> bool {
    init_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.hover.as_ref())
        .and_then(|hover| hover.content_format.as_ref())
        .is_none_or(|formats| formats.contains(&MarkupKind::Markdown))
}

fn send(connection: &Connection, message: Message) -> anyhow::Result<()> {
    connection
        .sender
        .send(message)
        .map_err(|error| anyhow::anyhow!("failed to send message: {error}"))
}

/// What became of a contained unit of work.
enum Caught<R> {
    Completed(R),
    Canceled,
    Panicked,
}

/// Runs `f`, classifying an unwinding exit: the semantic layer's cancellation
/// signal (which bypasses the panic hook by design) versus a genuine panic.
///
/// For a genuine panic the process-wide panic hook still runs first, so the
/// message and backtrace reach stderr as usual — only the unwind is swallowed,
/// and only stderr (never stdout, the protocol channel) is touched. `f` borrows
/// the server state mutably, which is not `UnwindSafe`; asserting it is safe is
/// sound per-arm: the [`Caught::Panicked`] arm's callers still discard the host
/// ([`ServerState::rebuild_host`]) and never read the possibly-inconsistent
/// cached state back, exactly as before. The [`Caught::Canceled`] arm
/// deliberately does NOT discard the host: a cancelled analysis leaves no memo
/// behind, and the database's pre-query bookkeeping is idempotent setup that
/// re-converges on retry while result bookkeeping is written only after a compute
/// returns — so the host is consistent and retrying (or answering
/// ContentModified) is sound.
fn catch<R>(f: impl FnOnce() -> R) -> Caught<R> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => Caught::Completed(value),
        Err(payload) if is_cancellation(payload.as_ref()) => Caught::Canceled,
        Err(_) => Caught::Panicked,
    }
}

/// The response for a request a newer document write superseded: the client
/// should retry against the new content.
fn content_modified_response(id: RequestId) -> Response {
    Response::new_err(
        id,
        ErrorCode::ContentModified as i32,
        "content modified while the request was in flight; please retry".to_owned(),
    )
}

/// The response to a request whose handler panicked: the analysis stack unwound
/// and was contained, so this request fails but the session lives on. The
/// original request id is echoed so the client can match the failure to its call.
fn panic_response(id: RequestId) -> Response {
    Response::new_err(
        id,
        ErrorCode::InternalError as i32,
        "the request handler panicked and was contained; the server is still running".to_owned(),
    )
}

/// The environment variable that arms the analysis-panic test seam out of process.
///
/// The panic-boundary tests (#241) need a document whose analysis deterministically
/// unwinds. Rather than couple that guarantee to a specific compiler bug, the seam
/// is deliberate and self-contained: any document whose path contains the substring
/// named here panics when it is analyzed. The out-of-process e2e harness sets this
/// on the server it spawns; the in-process unit tests use the thread-local arm
/// ([`arm_analysis_panic`]) instead, which does not race across parallel tests.
#[cfg(debug_assertions)]
const TEST_PANIC_ENV: &str = "INFERENCE_LSP_TEST_PANIC_PATH_SUBSTR";

#[cfg(debug_assertions)]
thread_local! {
    /// In-process arm for [`analysis_panic_seam`]. `None` unless a unit test set it;
    /// see [`TEST_PANIC_ENV`] for the out-of-process arm.
    static ANALYSIS_PANIC_SUBSTR: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Test-only seam (compiled only in debug builds) that forces a deterministic panic
/// while analyzing a marked document, so the message-loop panic boundary (#241) can
/// be exercised without depending on a specific compiler bug as its trigger.
///
/// A document panics when its path contains the armed substring — set either by the
/// [`TEST_PANIC_ENV`] environment variable (the out-of-process e2e harness) or by
/// the thread-local [`arm_analysis_panic`] (in-process unit tests). Every other
/// document is analyzed normally. In release builds this compiles to nothing.
#[cfg(debug_assertions)]
pub(crate) fn analysis_panic_seam(path: &std::path::Path) {
    let display = path.to_string_lossy();
    let matches = |substr: &str| !substr.is_empty() && display.contains(substr);
    let armed_in_process = ANALYSIS_PANIC_SUBSTR.with(|cell| {
        cell.borrow().as_deref().is_some_and(matches)
    });
    let armed_by_env = std::env::var(TEST_PANIC_ENV).is_ok_and(|substr| matches(&substr));
    assert!(
        !(armed_in_process || armed_by_env),
        "deliberate LSP analysis panic for {display}: exercising the #241 panic boundary"
    );
}

/// Release builds carry no test seam; the call sites optimize away entirely.
#[cfg(not(debug_assertions))]
#[inline]
pub(crate) fn analysis_panic_seam(_path: &std::path::Path) {}

/// Arms [`analysis_panic_seam`] in the current thread for documents whose path
/// contains `substr`, disarming when the returned guard drops. Used by the unit
/// tests, where an environment variable would race across the parallel test
/// threads sharing this process. Gated on `debug_assertions` alongside the seam it
/// arms, so a release test build (where the seam is a no-op) neither references the
/// absent thread-local nor compiles a dead arming helper.
#[cfg(all(test, debug_assertions))]
pub(crate) fn arm_analysis_panic(substr: &str) -> AnalysisPanicArm {
    ANALYSIS_PANIC_SUBSTR.with(|cell| *cell.borrow_mut() = Some(substr.to_owned()));
    AnalysisPanicArm
}

/// Drop guard returned by [`arm_analysis_panic`]; clears the thread-local arm.
#[cfg(all(test, debug_assertions))]
pub(crate) struct AnalysisPanicArm;

#[cfg(all(test, debug_assertions))]
impl Drop for AnalysisPanicArm {
    fn drop(&mut self) {
        ANALYSIS_PANIC_SUBSTR.with(|cell| *cell.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    // `Arc` and the `Document` constructor are only reached through `track`, which is
    // debug-only; gating the imports keeps a release test build warning-free.
    #[cfg(debug_assertions)]
    use std::sync::Arc;

    use lsp_server::{Connection, Message, Request, RequestId, Response};
    use lsp_types::notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit,
        Notification as _, PublishDiagnostics,
    };
    use lsp_types::request::{HoverRequest, Initialize, Request as _, Shutdown};
    use lsp_types::{InitializeParams, PublishDiagnosticsParams, Uri};

    use super::{
        coalesce_changes, coalesced_batch, is_barrier_for, run, NegotiatedCapabilities, ServerState,
    };
    #[cfg(debug_assertions)]
    use super::{arm_analysis_panic, Document};

    /// A client that supports everything this server negotiates. The individual
    /// tests here do not depend on the negotiated bits (those are exercised by the
    /// `convert`/`handlers` and e2e tests), so they share one fully-capable client.
    fn full_client() -> NegotiatedCapabilities {
        NegotiatedCapabilities {
            hierarchical_symbols: true,
            hover_markdown: true,
        }
    }

    /// The path substring the panic-boundary tests arm the analysis-panic seam with;
    /// only the documents deliberately named with it (`.../panic.inf`) unwind, while
    /// the healthy siblings analyze normally.
    #[cfg(debug_assertions)]
    const PANIC_MARKER: &str = "panic";

    /// A well-formed document used by the panic-boundary tests. Its analysis panics
    /// not because of its contents — it type-checks cleanly — but because its path
    /// carries [`PANIC_MARKER`] and the armed [`super::analysis_panic_seam`] forces
    /// a deterministic unwind for it (see #241 for the boundary it exercises).
    #[cfg(debug_assertions)]
    const PANIC_DOC_SOURCE: &str = "fn main() -> i32 { return 0; }";

    fn open(state: &mut ServerState, uri: &str, text: &str) {
        state.on_notification(did_open_notification(uri, text));
    }

    fn did_open_notification(uri: &str, text: &str) -> lsp_server::Notification {
        let params = serde_json::json!({
            "textDocument": { "uri": uri, "languageId": "inference", "version": 1, "text": text }
        });
        lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params)
    }

    fn did_change_notification(uri: &str, version: i32, text: &str) -> lsp_server::Notification {
        let params = serde_json::json!({
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [ { "text": text } ]
        });
        lsp_server::Notification::new(DidChangeTextDocument::METHOD.to_owned(), params)
    }

    fn did_close_notification(uri: &str) -> lsp_server::Notification {
        let params = serde_json::json!({ "textDocument": { "uri": uri } });
        lsp_server::Notification::new(DidCloseTextDocument::METHOD.to_owned(), params)
    }

    /// URIs of a publish list, for order-sensitive assertions.
    fn published_uris(publishes: &[lsp_types::PublishDiagnosticsParams]) -> Vec<&str> {
        publishes.iter().map(|p| p.uri.as_str()).collect()
    }

    const LIB_SOURCE: &str = "pub fn helper() -> i32 { return 7; }";
    const MAIN_IMPORTING_LIB: &str = "use lib;\nfn main() -> i32 { return lib::helper(); }";

    /// Opens `lib.inf` and a `main.inf` that imports it, both under `/inf-test`,
    /// and drains the (empty) pending set so a later change starts from a clean
    /// slate. Returns nothing; the two URIs are fixed and known to the caller.
    fn open_lib_and_dependent(state: &mut ServerState) {
        open(state, "file:///inf-test/lib.inf", LIB_SOURCE);
        open(state, "file:///inf-test/main.inf", MAIN_IMPORTING_LIB);
        let _ = state.drain_pending_republishes();
    }

    /// Installs `text` as the overlay for `uri` and tracks the document, without
    /// computing diagnostics — so a document whose *analysis* panics can be staged
    /// for a later query without the staging itself unwinding. Only the debug-only
    /// panic-boundary test uses it.
    #[cfg(debug_assertions)]
    fn track(state: &mut ServerState, uri: &str, text: &str) {
        let uri = Uri::from_str(uri).expect("a valid uri");
        let path = crate::uri::to_path(&uri).expect("a file uri");
        let text: Arc<str> = text.into();
        state.host.open_document(&path, Arc::clone(&text));
        state.documents.insert(
            uri,
            Document {
                path,
                version: 1,
                text,
            },
        );
    }

    fn diagnostics_for(state: &mut ServerState, uri: &str) -> Vec<lsp_types::Diagnostic> {
        let uri = Uri::from_str(uri).expect("a valid uri");
        crate::handlers::publish_diagnostics_params(state, &uri).diagnostics
    }

    #[cfg(debug_assertions)]
    fn hover_request(id: i32, uri: &str, line: u32, character: u32) -> Request {
        Request::new(
            RequestId::from(id),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
    }

    fn error_code(response: &Response) -> i32 {
        response.error.as_ref().expect("an error response").code
    }

    #[test]
    fn unknown_request_is_method_not_found() {
        let mut state = ServerState::new(full_client());
        let request = Request::new(
            RequestId::from(1),
            "textDocument/rename".to_owned(),
            serde_json::json!({}),
        );
        let response = state.handle_request(request);
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::MethodNotFound as i32
        );
    }

    #[test]
    fn a_repeated_initialize_request_is_invalid_request() {
        // The handshake consumed the one valid `initialize`; a second one arriving
        // mid-session is a protocol error (InvalidRequest), not the misleading
        // MethodNotFound the generic unknown-method arm would report.
        let mut state = ServerState::new(full_client());
        let request = Request::new(
            RequestId::from(1),
            Initialize::METHOD.to_owned(),
            serde_json::json!({ "capabilities": {} }),
        );
        let response = state.handle_request(request);
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::InvalidRequest as i32,
            "a second initialize is InvalidRequest"
        );
    }

    #[test]
    fn malformed_params_are_invalid_params_and_leave_the_server_usable() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/main.inf",
            "fn f() -> i32 { return 1; }",
        );

        // A hover request whose params are not a `HoverParams`.
        let bad = Request::new(
            RequestId::from(2),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({ "unexpected": true }),
        );
        let response = state.handle_request(bad);
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::InvalidParams as i32
        );

        // The server still answers a well-formed request afterwards.
        let good = Request::new(
            RequestId::from(3),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({
                "textDocument": { "uri": "file:///inf-test/main.inf" },
                "position": { "line": 0, "character": 3 }
            }),
        );
        let response = state.handle_request(good);
        assert!(response.error.is_none(), "a valid request still succeeds");
        assert!(
            response.result.is_some_and(|result| !result.is_null()),
            "hover over the function name returns a result"
        );
    }

    #[test]
    fn did_open_broken_source_publishes_a_diagnostic() {
        let mut state = ServerState::new(full_client());
        let params = serde_json::json!({
            "textDocument": {
                "uri": "file:///inf-test/broken.inf",
                "languageId": "inference",
                "version": 7,
                "text": "fn f() -> i32 { return x; }"
            }
        });
        let notification =
            lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params);
        let mut publishes = state.on_notification(notification);
        assert_eq!(publishes.len(), 1, "the only open document publishes once");
        let published = publishes.remove(0);
        assert_eq!(published.uri.as_str(), "file:///inf-test/broken.inf");
        assert_eq!(published.version, Some(7));
        assert!(
            !published.diagnostics.is_empty(),
            "an undeclared variable is reported"
        );
    }

    #[test]
    fn a_change_publishes_only_the_changed_document_eagerly() {
        let mut state = ServerState::new(full_client());
        open_lib_and_dependent(&mut state);

        // Changing the shared lib publishes lib eagerly and nothing else in the
        // same turn: the interactive request that may be right behind the keystroke
        // must not wait on the dependent's recompute.
        let eager = state.on_notification(did_change_notification(
            "file:///inf-test/lib.inf",
            2,
            "pub fn helper() -> i32 { return 8; }",
        ));
        assert_eq!(
            published_uris(&eager),
            vec!["file:///inf-test/lib.inf"],
            "only the changed document publishes eagerly"
        );
    }

    #[test]
    fn a_change_queues_the_invalidated_dependent_for_a_deferred_republish() {
        let mut state = ServerState::new(full_client());
        open_lib_and_dependent(&mut state);

        state.on_notification(did_change_notification(
            "file:///inf-test/lib.inf",
            2,
            "pub fn helper() -> i32 { return 8; }",
        ));

        // The dependent main.inf — whose closure includes the changed lib — is
        // drained at idle, so the client never keeps stale diagnostics on a
        // document a cross-file edit invalidated.
        let deferred = state.drain_pending_republishes();
        assert_eq!(
            published_uris(&deferred),
            vec!["file:///inf-test/main.inf"],
            "the invalidated dependent republishes at idle"
        );
        // Draining is exhaustive: a second drain has nothing left.
        assert!(
            state.drain_pending_republishes().is_empty(),
            "the pending set is emptied by a drain"
        );
    }

    #[test]
    fn a_change_does_not_queue_an_unaffected_open_document() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/a.inf",
            "fn a() -> i32 { return 1; }",
        );
        open(
            &mut state,
            "file:///inf-test/b.inf",
            "fn b() -> i32 { return 2; }",
        );
        let _ = state.drain_pending_republishes();

        // a.inf and b.inf are independent, so changing a.inf leaves b.inf's
        // analysis memoized and queues no republish for it.
        state.on_notification(did_change_notification(
            "file:///inf-test/a.inf",
            2,
            "fn a() -> i32 { return 11; }",
        ));
        assert!(
            state.drain_pending_republishes().is_empty(),
            "an unaffected open document is not republished"
        );
    }

    #[test]
    fn a_request_against_a_pending_document_publishes_it_fresh_and_clears_it() {
        let mut state = ServerState::new(full_client());
        open_lib_and_dependent(&mut state);

        state.on_notification(did_change_notification(
            "file:///inf-test/lib.inf",
            2,
            "pub fn helper() -> i32 { return 8; }",
        ));

        // main.inf is queued. A request against it (the loop calls this after
        // answering) publishes it now and removes it from the queue.
        let main_uri = Uri::from_str("file:///inf-test/main.inf").expect("a valid uri");
        let published = state
            .publish_if_pending(&main_uri)
            .expect("the pending dependent is published on demand");
        assert_eq!(published.uri.as_str(), "file:///inf-test/main.inf");
        assert!(
            state.drain_pending_republishes().is_empty(),
            "the on-demand publish cleared the pending document"
        );

        // A URI that was never queued yields nothing.
        assert!(
            state.publish_if_pending(&main_uri).is_none(),
            "a document not awaiting republish publishes nothing on demand"
        );
    }

    #[test]
    fn changing_a_queued_document_clears_its_pending_republish() {
        let mut state = ServerState::new(full_client());
        open_lib_and_dependent(&mut state);

        // A change to lib queues main.
        state.on_notification(did_change_notification(
            "file:///inf-test/lib.inf",
            2,
            "pub fn helper() -> i32 { return 8; }",
        ));
        // main is then edited itself, publishing it fresh — it no longer owes a
        // deferred republish, so the idle drain has nothing left for it.
        let eager = state.on_notification(did_change_notification(
            "file:///inf-test/main.inf",
            2,
            "use lib;\nfn main() -> i32 { return lib::helper() + 1; }",
        ));
        assert_eq!(published_uris(&eager), vec!["file:///inf-test/main.inf"]);
        assert!(
            state.drain_pending_republishes().is_empty(),
            "a document published fresh by its own change is no longer pending"
        );
    }

    #[test]
    fn closing_a_document_queues_its_open_dependents() {
        let mut state = ServerState::new(full_client());
        open_lib_and_dependent(&mut state);

        // Closing lib removes its overlay; the open dependent main.inf must
        // re-read lib from disk, so it is queued for a deferred republish.
        state.on_notification(did_close_notification("file:///inf-test/lib.inf"));
        assert_eq!(
            published_uris(&state.drain_pending_republishes()),
            vec!["file:///inf-test/main.inf"],
            "closing an imported file republishes the open dependent"
        );
    }

    #[test]
    fn did_close_publishes_an_empty_set() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/main.inf",
            "fn f() -> i32 { return 1; }",
        );
        let params = serde_json::json!({ "textDocument": { "uri": "file:///inf-test/main.inf" } });
        let notification =
            lsp_server::Notification::new(DidCloseTextDocument::METHOD.to_owned(), params);
        let mut publishes = state.on_notification(notification);
        assert_eq!(
            publishes.len(),
            1,
            "closing the only document clears it once"
        );
        let published = publishes.remove(0);
        assert!(published.diagnostics.is_empty());
        assert_eq!(published.version, None);
    }

    #[test]
    fn did_close_of_an_unmappable_uri_publishes_nothing() {
        // A URI this server cannot map to a file was never opened or tracked, so
        // closing it must publish nothing — not an empty diagnostics set under a
        // garbage URI, and no dependents republish — mirroring `did_open`, which
        // already ignores such URIs. An open document is present to prove the sweep
        // over other documents is *not* triggered by the unmappable close.
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/main.inf",
            "fn f() -> i32 { return 1; }",
        );

        let params = serde_json::json!({ "textDocument": { "uri": "untitled:Untitled-1" } });
        let notification =
            lsp_server::Notification::new(DidCloseTextDocument::METHOD.to_owned(), params);
        assert!(
            state.on_notification(notification).is_empty(),
            "closing an unmappable URI publishes nothing at all"
        );
    }

    #[test]
    fn a_change_for_an_unopened_document_is_dropped_then_a_later_open_adopts_it() {
        // LSP 3.17 sends `didChange` only for an open document. A change for a URI
        // the server never tracked is dropped — nothing is published, nothing is
        // queued, and the document is not adopted into the tracked set (#275). A
        // later proper `didOpen` still adopts it cleanly.
        let mut state = ServerState::new(full_client());

        let ghost = "file:///inf-test/ghost.inf";
        let publishes = state.on_notification(did_change_notification(
            ghost,
            2,
            "fn f() -> i32 { return x; }",
        ));
        assert!(
            publishes.is_empty(),
            "a change to an unopened document publishes nothing"
        );
        assert!(
            state.drain_pending_republishes().is_empty(),
            "a dropped change queues no republish"
        );
        let ghost_uri = Uri::from_str(ghost).expect("a valid uri");
        assert!(
            !state.documents.contains_key(&ghost_uri),
            "the never-opened document was not adopted into the tracked set"
        );

        // Opening it afterwards tracks it and publishes its diagnostics normally.
        let mut opened =
            state.on_notification(did_open_notification(ghost, "fn f() -> i32 { return x; }"));
        assert_eq!(opened.len(), 1, "the later open publishes once");
        let published = opened.remove(0);
        assert_eq!(published.uri.as_str(), ghost);
        assert!(
            !published.diagnostics.is_empty(),
            "the opened document's broken text is analyzed and reported"
        );
    }

    #[test]
    fn a_change_after_close_is_dropped_and_does_not_resurrect_tracking() {
        // After `didClose` the document leaves the tracked set, so a late change —
        // the same protocol violation as a change before any open — is dropped and
        // does not silently resurrect tracking (#275).
        let mut state = ServerState::new(full_client());
        let uri = "file:///inf-test/main.inf";
        open(&mut state, uri, "fn f() -> i32 { return x; }");
        state.on_notification(did_close_notification(uri));
        let _ = state.drain_pending_republishes();

        let publishes = state.on_notification(did_change_notification(
            uri,
            3,
            "fn f() -> i32 { return y; }",
        ));
        assert!(
            publishes.is_empty(),
            "a change after close publishes nothing"
        );
        assert!(
            state.drain_pending_republishes().is_empty(),
            "a dropped change queues no republish"
        );
        let main_uri = Uri::from_str(uri).expect("a valid uri");
        assert!(
            !state.documents.contains_key(&main_uri),
            "the closed document is not resurrected into the tracked set by a late change"
        );
    }

    #[test]
    fn non_file_uri_is_ignored_on_open() {
        let mut state = ServerState::new(full_client());
        let params = serde_json::json!({
            "textDocument": {
                "uri": "untitled:Untitled-1",
                "languageId": "inference",
                "version": 1,
                "text": "fn f() {}"
            }
        });
        let notification =
            lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params);
        assert!(
            state.on_notification(notification).is_empty(),
            "an untitled buffer is not analyzed and publishes nothing"
        );
    }

    #[test]
    fn rebuild_host_reconstructs_tracked_documents() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/clean.inf",
            "fn f() -> i32 { return 1; }",
        );
        open(
            &mut state,
            "file:///inf-test/broken.inf",
            "fn g() -> i32 { return x; }",
        );

        state.rebuild_host();

        // The rebuilt host carries each tracked document's last-seen text, so the
        // clean file is still clean and the broken one still reports its undeclared
        // variable — proving the text, not just the path, survives the rebuild.
        assert!(
            diagnostics_for(&mut state, "file:///inf-test/clean.inf").is_empty(),
            "the clean document stays clean after a rebuild"
        );
        assert!(
            !diagnostics_for(&mut state, "file:///inf-test/broken.inf").is_empty(),
            "the broken document still reports after a rebuild"
        );
    }

    // Gated on `debug_assertions`: the analysis-panic seam these two tests rely on is
    // a no-op in release builds, so the tests are compiled and run only where it is
    // active (the standard `cargo test` runs debug).
    #[cfg(debug_assertions)]
    #[test]
    fn handle_request_resilient_contains_a_handler_panic_and_rebuilds_the_host() {
        let _arm = arm_analysis_panic(PANIC_MARKER);
        let mut state = ServerState::new(full_client());
        // Stage both documents (tracking never analyzes, so it never unwinds); the
        // hover request against the panic file is what the seam makes unwind, and
        // what the resilient wrapper must contain. Requests never republish, so
        // staging a healthy sibling this way is safe.
        track(&mut state, "file:///inf-test/panic.inf", PANIC_DOC_SOURCE);
        track(
            &mut state,
            "file:///inf-test/ok.inf",
            "fn f() -> i32 { return 1; }",
        );

        // The recovery rebuilds the host from the tracked documents, so make the
        // sibling's *tracked* text (a rebuild's only input) diverge from its stale
        // host overlay: an undeclared variable the current host, still holding the
        // clean overlay, would not report. A reported diagnostic afterward can then
        // only come from a rebuild.
        let ok_uri = Uri::from_str("file:///inf-test/ok.inf").expect("a valid uri");
        state
            .documents
            .get_mut(&ok_uri)
            .expect("the tracked document")
            .text = "fn g() -> i32 { return x; }".into();

        let response =
            state.handle_request_resilient(hover_request(1, "file:///inf-test/panic.inf", 0, 3));
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::InternalError as i32,
            "a panicking handler answers InternalError"
        );
        assert_eq!(
            response.id,
            RequestId::from(1),
            "the failed request's own id is echoed back"
        );

        // The host was rebuilt from the tracked documents — proven by the sibling's
        // last-seen text (not its stale overlay) now being analyzed and reporting
        // its undeclared variable — so the session keeps serving from a clean host.
        assert!(
            !diagnostics_for(&mut state, "file:///inf-test/ok.inf").is_empty(),
            "the sibling's tracked text is analyzed after the rebuild"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn on_notification_resilient_contains_a_diagnostics_panic_and_recovers() {
        let _arm = arm_analysis_panic(PANIC_MARKER);
        let mut state = ServerState::new(full_client());
        // A healthy document opened before the bad one; the bad open's panic must
        // not disturb it. (An undeclared variable keeps its diagnostics non-empty.)
        open(
            &mut state,
            "file:///inf-test/ok.inf",
            "fn g() -> i32 { return x; }",
        );

        // Opening a document whose diagnostics computation panics publishes nothing
        // and rebuilds the host rather than tearing down the session.
        let publishes = state.on_notification_resilient(did_open_notification(
            "file:///inf-test/panic.inf",
            PANIC_DOC_SOURCE,
        ));
        assert!(
            publishes.is_empty(),
            "a panic during diagnostics publishes nothing"
        );

        // The host was rebuilt from the tracked documents, so the healthy document
        // is still analyzable and still reports its undeclared variable.
        assert!(
            !diagnostics_for(&mut state, "file:///inf-test/ok.inf").is_empty(),
            "the healthy document survives the recovery and is still analyzed"
        );
    }

    // --- Cancellation discrimination (issue #157) --------------------------
    //
    // A caught cancellation is not a panic: it answers ContentModified and leaves
    // the host intact, the exact inverse of the panic-boundary tests above (which
    // answer InternalError and rebuild). The pre-fired cancellation token makes
    // these deterministic with zero threads and zero sleeps — an analysis unwinds
    // at its first query checkpoint even on a memo hit.

    #[cfg(debug_assertions)]
    #[test]
    fn a_superseded_request_is_answered_content_modified_without_rebuilding() {
        let mut state = ServerState::new(full_client());
        // Stage two clean documents and prime their analyses (memoized clean from
        // the host overlay), then make each one's *tracked* text — the only input a
        // rebuild would re-apply — diverge into broken source. A rebuild would
        // adopt that broken text and report; no rebuild keeps the clean overlay.
        let ok_uri = "file:///inf-test/ok.inf";
        let bystander_uri = "file:///inf-test/bystander.inf";
        track(&mut state, ok_uri, "fn f() -> i32 { return 1; }");
        track(&mut state, bystander_uri, "fn h() -> i32 { return 2; }");
        assert!(
            diagnostics_for(&mut state, ok_uri).is_empty(),
            "ok primes clean"
        );
        assert!(
            diagnostics_for(&mut state, bystander_uri).is_empty(),
            "bystander primes clean"
        );
        for uri in [ok_uri, bystander_uri] {
            let uri = Uri::from_str(uri).expect("a valid uri");
            state
                .documents
                .get_mut(&uri)
                .expect("the tracked document")
                .text = "fn broken() -> i32 { return z; }".into();
        }

        // A write landed while this hover was in flight: fire the cancellation,
        // then run the hover. It unwinds at the first fetch checkpoint (even though
        // ok's analysis is memoized) and, because the epoch moved, is superseded.
        let _epoch = state.cancel_source.request_cancellation();
        let response = state.handle_request_resilient(hover_request(1, ok_uri, 0, 3));
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::ContentModified as i32,
            "a superseded request is answered ContentModified"
        );
        assert_eq!(
            response.id,
            RequestId::from(1),
            "the superseded request's own id is echoed back"
        );

        // No rebuild: both documents still serve their clean overlay, not the
        // divergent tracked text a rebuild would have adopted (the contrapositive
        // of the genuine-panic test, which asserts exactly this reports).
        assert!(
            diagnostics_for(&mut state, ok_uri).is_empty(),
            "the stale-but-clean overlay is still served — no host rebuild"
        );
        assert!(
            diagnostics_for(&mut state, bystander_uri).is_empty(),
            "an untouched document's clean overlay survives too — no host rebuild"
        );

        // The session stays healthy: a follow-up request completes normally
        // (handle_request_resilient re-checks the epoch only on the cancel arm, so
        // a completed answer is never downgraded to ContentModified).
        let followup = state.handle_request_resilient(hover_request(2, ok_uri, 0, 3));
        assert!(
            followup.error.is_none(),
            "a follow-up request after the superseded one completes"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn a_stale_self_cancel_retries_and_completes() {
        // A cancellation token fired with no newer write behind it (no epoch bump)
        // is a residual self-cancel: the unwind consumes the signal, and the
        // resilient wrapper retries and completes. This is the arm every adopted
        // write's own eager publish will exercise once the router fires before
        // forwarding, so it must recover with no rebuild.
        let mut state = ServerState::new(full_client());
        let doc_uri = "file:///inf-test/retry.inf";
        track(&mut state, doc_uri, "fn f() -> i32 { return 1; }");
        // Divergent tracked text: a rebuild would adopt broken source and report.
        let uri = Uri::from_str(doc_uri).expect("a valid uri");
        state
            .documents
            .get_mut(&uri)
            .expect("the tracked document")
            .text = "fn broken() -> i32 { return z; }".into();

        state.cancel_source.debug_fire_token_only();
        let response = state.handle_request_resilient(hover_request(1, doc_uri, 0, 3));
        assert!(
            response.error.is_none(),
            "a residual self-cancel retries and the request completes"
        );
        assert!(
            diagnostics_for(&mut state, doc_uri).is_empty(),
            "the retry served the clean overlay — no host rebuild on a self-cancel"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn cancellation_still_fires_after_a_host_rebuild() {
        // A rebuilt host mints a fresh cancellation token; rebuild_host must rebind
        // the source to it. This pins that rebind: a genuine panic rebuilds the
        // host, then a cancellation fired afterward must still interrupt the new
        // host's analysis (answered ContentModified). A forgotten rebind leaves the
        // source firing the discarded host's token, so the request would complete
        // Ok instead — the failure signature of the bug.
        let _arm = arm_analysis_panic(PANIC_MARKER);
        let mut state = ServerState::new(full_client());
        track(&mut state, "file:///inf-test/panic.inf", PANIC_DOC_SOURCE);
        track(
            &mut state,
            "file:///inf-test/ok.inf",
            "fn f() -> i32 { return 1; }",
        );

        let panicked =
            state.handle_request_resilient(hover_request(1, "file:///inf-test/panic.inf", 0, 3));
        assert_eq!(
            error_code(&panicked),
            lsp_server::ErrorCode::InternalError as i32,
            "the panicking request rebuilds the host and answers InternalError"
        );

        drop(_arm);
        let _epoch = state.cancel_source.request_cancellation();
        let superseded =
            state.handle_request_resilient(hover_request(2, "file:///inf-test/ok.inf", 0, 3));
        assert_eq!(
            error_code(&superseded),
            lsp_server::ErrorCode::ContentModified as i32,
            "cancellation still interrupts the rebuilt host — the source was rebound"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn a_request_queued_behind_a_write_fast_fails_without_computing() {
        // A request the router forwarded before a write landed must be answered
        // ContentModified at *dispatch*, without computing at all. The analysis-panic
        // seam is armed on the document, so any actual analysis would answer
        // InternalError — a ContentModified here therefore proves the dispatch-time
        // fast-fail short-circuited before the handler ever ran.
        let _arm = arm_analysis_panic(PANIC_MARKER);
        let mut state = ServerState::new(full_client());
        track(&mut state, "file:///inf-test/panic.inf", PANIC_DOC_SOURCE);

        // A write bumped the source epoch past this turn's baseline: job_epoch is
        // still 0 (the just-constructed value — respond_to_request is the worker's
        // dispatch entry and no begin_turn advanced it), so the job is superseded.
        let _epoch = state.cancel_source.request_cancellation();
        let response =
            state.respond_to_request(hover_request(1, "file:///inf-test/panic.inf", 0, 3));
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::ContentModified as i32,
            "a queued request superseded before dispatch fast-fails without computing"
        );
        assert_eq!(
            response.id,
            RequestId::from(1),
            "the fast-failed request's own id is echoed back"
        );
    }

    #[test]
    fn a_superseded_notification_requeues_its_publish_and_dependents() {
        // A notification whose own eager publish is superseded by a newer write
        // must not lose that publish: it requeues the changed document and its
        // invalidated dependents for the deferred drain, and does not rebuild.
        let mut state = ServerState::new(full_client());
        open_lib_and_dependent(&mut state);

        // Divergent tracked text on the dependent: a rebuild would adopt it and
        // report; the drain reading the clean overlay proves no rebuild happened.
        let main_uri = Uri::from_str("file:///inf-test/main.inf").expect("a valid uri");
        state
            .documents
            .get_mut(&main_uri)
            .expect("the tracked dependent")
            .text = "fn broken() -> i32 { return z; }".into();

        let lib_uri = Uri::from_str("file:///inf-test/lib.inf").expect("a valid uri");
        let _epoch = state.cancel_source.request_cancellation();
        let eager = state.on_notification_resilient(did_change_notification(
            "file:///inf-test/lib.inf",
            2,
            "pub fn helper() -> i32 { return 8; }",
        ));
        assert!(
            eager.is_empty(),
            "a superseded notification publishes nothing eagerly"
        );
        assert!(
            state.pending_republish.contains(&lib_uri),
            "the changed document is requeued rather than lost"
        );
        assert!(
            state.pending_republish.contains(&main_uri),
            "the invalidated dependent is requeued too"
        );

        let publishes = state.drain_pending_republishes();
        let drained = published_uris(&publishes);
        assert!(
            drained.contains(&"file:///inf-test/lib.inf"),
            "the requeued changed document is drained, got {drained:?}"
        );
        assert!(
            drained.contains(&"file:///inf-test/main.inf"),
            "the requeued dependent is drained, got {drained:?}"
        );

        assert!(
            diagnostics_for(&mut state, "file:///inf-test/main.inf").is_empty(),
            "the dependent's clean overlay is served — no host rebuild"
        );
    }

    #[test]
    fn a_completed_request_is_not_answered_after_a_client_cancel() {
        // The completion gate: once `$/cancelRequest` completes a pending request
        // (answered RequestCanceled by the router), the worker's later response for
        // the same id is suppressed, so the client sees exactly one response.
        use std::sync::Mutex;

        use super::{ReqQueue, send, send_gated_response};

        let (server, client) = Connection::memory();
        let req_queue: Mutex<ReqQueue<(), ()>> = Mutex::new(ReqQueue::default());

        // The router registered request 7, then a client `$/cancelRequest` cancelled
        // it: the RequestCanceled response is built and sent from the router side.
        req_queue
            .lock()
            .expect("lock")
            .incoming
            .register(RequestId::from(7), ());
        let canceled = req_queue
            .lock()
            .expect("lock")
            .incoming
            .cancel(RequestId::from(7))
            .expect("a registered request is cancelable");
        assert_eq!(
            canceled.error.as_ref().expect("an error response").code,
            lsp_server::ErrorCode::RequestCanceled as i32,
            "the client cancel is answered RequestCanceled"
        );
        send(&server, Message::Response(canceled)).expect("send the cancel response");

        // The worker then finishes the request and tries to answer it Ok; the gate
        // must drop that late response because the id is no longer pending.
        send_gated_response(&server, &req_queue, Response::new_ok(RequestId::from(7), ()))
            .expect("gated send");

        // The client received exactly the one RequestCanceled and nothing after it.
        match client.receiver.recv().expect("the cancel response arrives") {
            Message::Response(response) => {
                assert_eq!(response.id, RequestId::from(7), "the cancelled id");
                assert_eq!(
                    response.error.expect("an error").code,
                    lsp_server::ErrorCode::RequestCanceled as i32,
                    "the sole response is the RequestCanceled"
                );
            }
            other => panic!("expected a response, got {other:?}"),
        }
        assert!(
            client.receiver.try_recv().is_err(),
            "no second response is sent for the same id"
        );
    }

    #[test]
    fn named_constant_array_size_publishes_a_diagnostic_not_a_panic() {
        // The #240 fix at the LSP boundary: the source that used to `todo!`-panic the
        // analysis now type-checks into an ordinary diagnostic, so opening it
        // publishes a normal diagnostic set instead of unwinding the session. The
        // seam is not armed here — the panic must be gone on its own.
        let mut state = ServerState::new(full_client());
        let source = "const N: i32 = 3;\n\
fn main() -> i32 { let arr: [i32; N] = [1, 2, 3]; return arr[0]; }";
        let mut publishes = state.on_notification(did_open_notification(
            "file:///inf-test/const-size.inf",
            source,
        ));
        assert_eq!(publishes.len(), 1, "the only open document publishes once");
        let published = publishes.remove(0);
        assert!(
            published
                .diagnostics
                .iter()
                .any(|d| d.message.contains("array size must be an integer literal")),
            "the named-constant array size is reported as a diagnostic, got {:?}",
            published.diagnostics
        );
    }

    // --- Coalescing (issue #247, item 1) -----------------------------------

    const DOC_A: &str = "file:///inf-test/a.inf";
    const DOC_B: &str = "file:///inf-test/b.inf";

    fn change_msg(uri: &str, version: i32) -> Message {
        Message::Notification(did_change_notification(uri, version, "fn f() -> i32 { return 0; }"))
    }

    fn open_msg(uri: &str) -> Message {
        Message::Notification(did_open_notification(uri, "fn f() {}"))
    }

    fn close_msg(uri: &str) -> Message {
        Message::Notification(did_close_notification(uri))
    }

    fn request_msg(id: i32) -> Message {
        Message::Request(Request::new(
            RequestId::from(id),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({}),
        ))
    }

    /// A compact label per message, so a coalesced batch is asserted by shape.
    fn tag(message: &Message) -> String {
        match message {
            Message::Notification(n) if n.method == DidChangeTextDocument::METHOD => {
                let uri = n.params["textDocument"]["uri"].as_str().unwrap();
                let version = n.params["textDocument"]["version"].as_i64().unwrap();
                format!("change:{uri}:{version}")
            }
            Message::Notification(n) if n.method == DidOpenTextDocument::METHOD => {
                format!("open:{}", n.params["textDocument"]["uri"].as_str().unwrap())
            }
            Message::Notification(n) if n.method == DidCloseTextDocument::METHOD => {
                format!("close:{}", n.params["textDocument"]["uri"].as_str().unwrap())
            }
            Message::Request(r) => format!("req:{}", r.id),
            other => panic!("unexpected message {other:?}"),
        }
    }

    fn tags(messages: &[Message]) -> Vec<String> {
        messages.iter().map(tag).collect()
    }

    #[test]
    fn coalesce_collapses_a_same_document_burst_to_its_final_text() {
        let batch = vec![change_msg(DOC_A, 1), change_msg(DOC_A, 2), change_msg(DOC_A, 3)];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![format!("change:{DOC_A}:3")],
            "a burst of edits to one document collapses to its final text"
        );
    }

    #[test]
    fn coalesce_keeps_the_final_change_per_document_when_documents_interleave() {
        // Two documents edited in an interleaved burst: each collapses to its own
        // final text, and the survivors keep their arrival order.
        let batch = vec![
            change_msg(DOC_A, 1),
            change_msg(DOC_B, 1),
            change_msg(DOC_A, 2),
            change_msg(DOC_B, 2),
        ];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![format!("change:{DOC_A}:2"), format!("change:{DOC_B}:2")],
        );
    }

    #[test]
    fn a_request_is_a_barrier_the_coalescer_never_crosses() {
        // The request must observe the edit that preceded it, so the earlier change
        // is not dropped even though a later change to the same document follows.
        let batch = vec![change_msg(DOC_A, 1), request_msg(7), change_msg(DOC_A, 2)];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![
                format!("change:{DOC_A}:1"),
                "req:7".to_owned(),
                format!("change:{DOC_A}:2"),
            ],
        );
    }

    #[test]
    fn a_same_document_didclose_bars_coalescing_across_it() {
        let batch = vec![change_msg(DOC_A, 1), close_msg(DOC_A), change_msg(DOC_A, 2)];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![
                format!("change:{DOC_A}:1"),
                format!("close:{DOC_A}"),
                format!("change:{DOC_A}:2"),
            ],
        );
    }

    #[test]
    fn a_same_document_didopen_bars_coalescing_across_it() {
        let batch = vec![change_msg(DOC_A, 1), open_msg(DOC_A), change_msg(DOC_A, 2)];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![
                format!("change:{DOC_A}:1"),
                format!("open:{DOC_A}"),
                format!("change:{DOC_A}:2"),
            ],
        );
    }

    #[test]
    fn another_documents_lifecycle_event_does_not_bar_coalescing() {
        // A close of a *different* document is not a barrier for A: A's earlier
        // change is superseded by its later one across it. The close is kept.
        let batch = vec![change_msg(DOC_A, 1), close_msg(DOC_B), change_msg(DOC_A, 2)];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![format!("close:{DOC_B}"), format!("change:{DOC_A}:2")],
        );
    }

    #[test]
    fn a_didclose_mid_burst_coalesces_each_run_but_not_across_the_close() {
        // Changes before the same-document close collapse among themselves; the
        // close is a barrier; changes after it collapse among themselves.
        let batch = vec![
            change_msg(DOC_A, 1),
            change_msg(DOC_A, 2),
            close_msg(DOC_A),
            change_msg(DOC_A, 3),
            change_msg(DOC_A, 4),
        ];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec![
                format!("change:{DOC_A}:2"),
                format!("close:{DOC_A}"),
                format!("change:{DOC_A}:4"),
            ],
        );
    }

    #[test]
    fn coalesce_leaves_a_batch_without_a_superseding_change_untouched() {
        let batch = vec![request_msg(1), open_msg(DOC_A), change_msg(DOC_A, 5)];
        assert_eq!(
            tags(&coalesce_changes(batch)),
            vec!["req:1".to_owned(), format!("open:{DOC_A}"), format!("change:{DOC_A}:5")],
        );
    }

    #[test]
    fn coalesced_batch_collapses_a_buffered_change_backlog() {
        // The router forwards each keystroke instantly onto the unbounded job
        // channel, so a typing burst reaches the worker as a buffered backlog;
        // this covers the drain-and-coalesce path over that buffer directly. A
        // `didChange` head plus two more for the same document, all already
        // buffered, collapse to the final text — what a keystroke burst must
        // become at worker dequeue.
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(change_msg(DOC_A, 2)).expect("buffer a change");
        sender.send(change_msg(DOC_A, 3)).expect("buffer a change");
        let batch = coalesced_batch(change_msg(DOC_A, 1), &receiver);
        assert_eq!(
            tags(&batch),
            vec![format!("change:{DOC_A}:3")],
            "a buffered same-document burst collapses to its final text"
        );
    }

    #[test]
    fn coalesced_batch_returns_a_non_change_head_alone_without_draining() {
        // A non-`didChange` head is never batched: it is returned immediately and the
        // buffered backlog behind it is left untouched, so nothing is reordered ahead
        // of a request or lifecycle event and no message is dropped.
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(change_msg(DOC_A, 1)).expect("buffer a change");
        let batch = coalesced_batch(request_msg(9), &receiver);
        assert_eq!(
            tags(&batch),
            vec!["req:9".to_owned()],
            "the request head is returned alone"
        );
        assert!(
            receiver.try_recv().is_ok(),
            "the backlog behind a non-change head is left for the next loop turn"
        );
    }

    #[test]
    fn is_barrier_for_distinguishes_requests_and_same_document_lifecycle() {
        assert!(
            is_barrier_for(&request_msg(1), DOC_A),
            "any request bars coalescing across it"
        );
        assert!(
            is_barrier_for(&close_msg(DOC_A), DOC_A),
            "a same-document close is a barrier"
        );
        assert!(
            !is_barrier_for(&close_msg(DOC_B), DOC_A),
            "another document's close is not a barrier"
        );
        assert!(
            !is_barrier_for(&change_msg(DOC_A, 1), DOC_A),
            "a change is coalesced, not a barrier"
        );
    }

    // --- Message loop, over an in-memory transport (issue #247, items 1-2) --

    /// Spawns [`run`] against one half of an in-memory connection and returns the
    /// client half plus the server thread's join handle.
    fn run_server() -> (Connection, std::thread::JoinHandle<()>) {
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || {
            let init: InitializeParams =
                serde_json::from_value(serde_json::json!({ "capabilities": {} }))
                    .expect("default init params");
            let _ = run(&server, &init);
        });
        (client, handle)
    }

    fn send_to(client: &Connection, message: Message) {
        client.sender.send(message).expect("send to the server");
    }

    /// Receives the next `publishDiagnostics` for `uri` within `timeout`, ignoring
    /// other messages; `None` on timeout or disconnect.
    fn recv_publish_for(
        client: &Connection,
        uri: &str,
        timeout: std::time::Duration,
    ) -> Option<PublishDiagnosticsParams> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match client.receiver.recv_timeout(remaining) {
                Ok(Message::Notification(n)) if n.method == PublishDiagnostics::METHOD => {
                    let params: PublishDiagnosticsParams =
                        serde_json::from_value(n.params).expect("publish params");
                    if params.uri.as_str() == uri {
                        return Some(params);
                    }
                }
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    const LOOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    fn shutdown_and_exit(client: &Connection, handle: std::thread::JoinHandle<()>) {
        send_to(
            client,
            Message::Request(Request::new(
                RequestId::from(1),
                Shutdown::METHOD.to_owned(),
                serde_json::Value::Null,
            )),
        );
        send_to(
            client,
            Message::Notification(lsp_server::Notification::new(
                Exit::METHOD.to_owned(),
                serde_json::Value::Null,
            )),
        );
        handle.join().expect("server thread joins after exit");
    }

    #[test]
    fn the_loop_republishes_an_invalidated_dependent_when_idle() {
        let (client, handle) = run_server();
        send_to(
            &client,
            Message::Notification(did_open_notification("file:///inf-test/lib.inf", LIB_SOURCE)),
        );
        recv_publish_for(&client, "file:///inf-test/lib.inf", LOOP_TIMEOUT)
            .expect("lib publishes on open");
        send_to(
            &client,
            Message::Notification(did_open_notification(
                "file:///inf-test/main.inf",
                MAIN_IMPORTING_LIB,
            )),
        );
        recv_publish_for(&client, "file:///inf-test/main.inf", LOOP_TIMEOUT)
            .expect("main publishes on open");

        // A change to the shared lib queues the dependent; the loop must republish
        // it once idle, with no request to prompt it.
        send_to(
            &client,
            Message::Notification(did_change_notification(
                "file:///inf-test/lib.inf",
                2,
                "pub fn helper() -> i32 { return 8; }",
            )),
        );
        assert!(
            recv_publish_for(&client, "file:///inf-test/main.inf", LOOP_TIMEOUT).is_some(),
            "the invalidated dependent must republish once the loop goes idle"
        );

        shutdown_and_exit(&client, handle);
    }

    #[test]
    fn shutdown_does_not_drop_a_queued_republish() {
        let (client, handle) = run_server();
        send_to(
            &client,
            Message::Notification(did_open_notification("file:///inf-test/lib.inf", LIB_SOURCE)),
        );
        recv_publish_for(&client, "file:///inf-test/lib.inf", LOOP_TIMEOUT)
            .expect("lib publishes on open");
        send_to(
            &client,
            Message::Notification(did_open_notification(
                "file:///inf-test/main.inf",
                MAIN_IMPORTING_LIB,
            )),
        );
        recv_publish_for(&client, "file:///inf-test/main.inf", LOOP_TIMEOUT)
            .expect("main publishes on open");

        // Change lib and immediately shut down. Whether the dependent drains at
        // idle or on the shutdown flush, the queued republish for main must not be
        // lost.
        send_to(
            &client,
            Message::Notification(did_change_notification(
                "file:///inf-test/lib.inf",
                2,
                "pub fn helper() -> i32 { return 8; }",
            )),
        );
        send_to(
            &client,
            Message::Request(Request::new(
                RequestId::from(1),
                Shutdown::METHOD.to_owned(),
                serde_json::Value::Null,
            )),
        );
        send_to(
            &client,
            Message::Notification(lsp_server::Notification::new(
                Exit::METHOD.to_owned(),
                serde_json::Value::Null,
            )),
        );
        assert!(
            recv_publish_for(&client, "file:///inf-test/main.inf", LOOP_TIMEOUT).is_some(),
            "a graceful shutdown must not lose the queued dependent republish"
        );
        handle.join().expect("server thread joins after exit");
    }
}

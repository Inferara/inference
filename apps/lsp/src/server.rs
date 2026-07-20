//! The server state and the single-threaded message loop.
//!
//! [`ServerState`] holds the analysis host and the set of open documents; it turns
//! one request into one [`Response`] and one notification into the diagnostics to
//! publish, with no I/O of its own — which is what makes it directly testable.
//! [`run`] owns the transport: it reads messages, handles the shutdown/exit
//! handshake inline, routes everything else through the state, and writes the
//! results back. Nothing here prints to stdout; that stream is the protocol
//! channel.

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use inference_ide::AnalysisHost;
use lsp_server::{
    Connection, ErrorCode, ExtractError, Message, Notification, Request, RequestId, Response,
};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Initialize, InlayHintRequest,
    Request as _, Shutdown,
};
use lsp_types::{InitializeParams, MarkupKind, PublishDiagnosticsParams, Uri};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{capabilities, handlers};

/// The server name reported in the initialize result's `serverInfo` (the crate
/// name), which clients surface in their logs and crash reports.
const SERVER_NAME: &str = env!("CARGO_PKG_NAME");
/// The server version reported in the initialize result's `serverInfo` (the crate
/// version).
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

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
}

impl ServerState {
    pub(crate) fn new(capabilities: NegotiatedCapabilities) -> Self {
        Self {
            host: AnalysisHost::default(),
            documents: FxHashMap::default(),
            capabilities,
            pending_republish: FxHashSet::default(),
        }
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
    pub(crate) fn handle_request_resilient(&mut self, request: Request) -> Response {
        let id = request.id.clone();
        match catch(|| self.handle_request(request)) {
            Some(response) => response,
            None => {
                self.rebuild_host();
                panic_response(id)
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
        match catch(|| self.on_notification(notification)) {
            Some(publishes) => publishes,
            None => {
                self.rebuild_host();
                Vec::new()
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
        let pending: Vec<Uri> = self.pending_republish.drain().collect();
        let mut publishes = Vec::with_capacity(pending.len());
        for uri in pending {
            if let Some(params) = catch(|| handlers::publish_diagnostics_params(self, &uri)) {
                publishes.push(params);
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
        catch(|| handlers::publish_diagnostics_params(self, uri))
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
    fn rebuild_host(&mut self) {
        let mut host = AnalysisHost::default();
        for document in self.documents.values() {
            host.open_document(&document.path, Arc::clone(&document.text));
        }
        self.host = host;
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

/// Runs the message loop until the client exits or the connection closes.
///
/// # Shedding per-keystroke work
///
/// Each keystroke arrives as its own full-text `didChange`, and analysis is
/// single-threaded, so a naive one-message-at-a-time loop would run the whole
/// closure pipeline once per keystroke while an interactive request waits behind
/// the burst. Two mechanisms shed that cost (issue #247):
///
/// * **Coalescing.** A dedicated forwarder ([`spawn_transport_pump`]) keeps the
///   transport's rendezvous receiver continuously drained into a buffer, so while
///   the loop analyzes one change the burst behind it accumulates there. When the
///   head of that buffer is a `didChange`, the available backlog is drained and
///   consecutive changes to the same document collapse to their final text
///   ([`coalesce_changes`]), so a typing burst runs the pipeline a handful of times
///   instead of once per keystroke. A `didOpen`/`didClose` for that document, or
///   any request, is a barrier the coalescer never reorders across, and no
///   non-`didChange` message is ever dropped.
/// * **Deferred dependents.** A notification publishes eagerly only for the
///   changed document; every other open document it invalidated is queued
///   ([`ServerState::on_notification`]) and republished when the loop next goes
///   idle — after the interactive request that arrived right behind the keystroke
///   has already been answered. The queue is always drained before the loop
///   blocks on the next message, a request against a queued document publishes it
///   fresh immediately ([`ServerState::publish_if_pending`]), and a shutdown
///   flushes it, so a client never keeps a stale diagnostic set indefinitely.
///
/// # Shutdown handshake
///
/// The shutdown handshake is handled inline rather than delegated to
/// `lsp-server`'s `Connection::handle_shutdown`, which consumes the next message
/// itself and turns anything but `exit` into a fatal protocol error. Instead, a
/// `shutdown` request is answered and flips a `shutting_down` flag; while it is
/// set, every further request — including a repeated `shutdown` — is answered with
/// `InvalidRequest` (the spec's behaviour for requests received between `shutdown`
/// and `exit`) and every notification but `exit` is ignored. The `exit`
/// notification ends the loop. Every other request is routed through
/// [`ServerState`], and document notifications may publish diagnostics.
///
/// The `shutting_down` guard precedes the `shutdown` arm so a *second* `shutdown`
/// is rejected like any other post-shutdown request rather than answered a second
/// `null` success.
///
/// Requests and notifications are dispatched through the resilient wrappers
/// ([`handle_request_resilient`](ServerState::handle_request_resilient),
/// [`on_notification_resilient`](ServerState::on_notification_resilient)), so an
/// unwinding panic in the analysis stack fails a single request or publish
/// instead of the whole session.
///
/// # Errors
///
/// Returns an error if a message cannot be written to the transport.
pub fn run(connection: &Connection, init_params: &InitializeParams) -> anyhow::Result<()> {
    let mut state = ServerState::new(NegotiatedCapabilities::from_init_params(init_params));
    let mut shutting_down = false;

    // Read through a buffering forwarder rather than the transport receiver
    // directly, so a typing burst can accumulate a backlog the coalescer can
    // collapse (see [`spawn_transport_pump`]).
    let incoming = spawn_transport_pump(connection)?;

    loop {
        let message = match incoming.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => {
                // The backlog is empty: refresh every document a recent change
                // invalidated before parking on the next message, so no queued
                // republish is left indefinitely stale.
                publish_all(connection, state.drain_pending_republishes())?;
                match incoming.recv() {
                    Ok(message) => message,
                    Err(_) => return Ok(()),
                }
            }
            Err(TryRecvError::Disconnected) => return Ok(()),
        };

        for message in coalesced_batch(message, &incoming) {
            if handle_message(connection, &mut state, &mut shutting_down, message)?.is_break() {
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
fn handle_message(
    connection: &Connection,
    state: &mut ServerState,
    shutting_down: &mut bool,
    message: Message,
) -> anyhow::Result<Flow> {
    match message {
        Message::Request(request) if *shutting_down => {
            send(
                connection,
                Message::Response(Response::new_err(
                    request.id,
                    ErrorCode::InvalidRequest as i32,
                    "the server is shutting down".to_owned(),
                )),
            )?;
        }
        Message::Request(request) if request.method == Shutdown::METHOD => {
            *shutting_down = true;
            // Flush queued republishes before parking on `exit`, so a graceful
            // shutdown never drops a document's owed diagnostics.
            publish_all(connection, state.drain_pending_republishes())?;
            send(
                connection,
                Message::Response(Response::new_ok(request.id, ())),
            )?;
        }
        Message::Request(request) => {
            let document = request_document_uri(&request);
            let response = state.handle_request_resilient(request);
            send(connection, Message::Response(response))?;
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

/// Spawns a forwarder that drains the transport's rendezvous receiver into an
/// unbounded buffer, and returns the buffer's receiver for the loop to read.
///
/// `lsp-server`'s stdio and socket transports connect their reader thread to the
/// loop over a zero-capacity channel (`bounded(0)`): the reader blocks handing over
/// each frame until the loop takes it, so consecutive frames of a typing burst
/// never pile up in the channel — they sit unparsed in the OS pipe, invisible to a
/// `try_recv`. Draining that channel continuously on a dedicated thread moves the
/// backlog into an unbounded buffer instead, so while the loop is busy analyzing
/// one change the burst behind it collects where [`coalesced_batch`] can see and
/// collapse it. Without this, coalescing is a no-op over the production transport,
/// since [`Connection::memory`] (the tests' transport) is the only one that buffers.
///
/// The forwarder ends on its own when either end disconnects — the transport closes
/// (its reader gone) or the loop drops the returned receiver — so it needs no
/// explicit join.
fn spawn_transport_pump(connection: &Connection) -> anyhow::Result<Receiver<Message>> {
    let source = connection.receiver.clone();
    let (buffered_sender, buffered_receiver) = mpsc::channel();
    thread::Builder::new()
        .name("inference-lsp-transport-pump".to_owned())
        .spawn(move || {
            for message in source {
                if buffered_sender.send(message).is_err() {
                    break;
                }
            }
        })?;
    Ok(buffered_receiver)
}

/// The batch of messages to process for `first`, in arrival order.
///
/// Only a `didChange` head is worth batching: the backlog the transport pump has
/// buffered is drained non-blockingly and consecutive same-document changes collapse
/// to their final text ([`coalesce_changes`]). Any other head is returned alone so
/// requests and lifecycle notifications keep exact arrival order and timing.
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

/// Drops each `didChange` a later `didChange` for the same document supersedes,
/// keeping only the final text of a burst.
///
/// A `didChange` at index `i` is dropped when a later `didChange` for the same
/// document appears before any barrier between them: a request, or a
/// `didOpen`/`didClose` for that same document. Barriers are never reordered
/// across — a request must observe the edits that preceded it, and a lifecycle
/// event bounds a document's edit run — and no non-`didChange` message and no
/// message for another document is ever dropped, so every message's relative
/// order is preserved.
fn coalesce_changes(messages: Vec<Message>) -> Vec<Message> {
    let mut keep = vec![true; messages.len()];
    for (i, message) in messages.iter().enumerate() {
        let Some(document) = did_change_uri(message) else {
            continue;
        };
        for later in &messages[i + 1..] {
            if is_barrier_for(later, document) {
                break;
            }
            if did_change_uri(later) == Some(document) {
                keep[i] = false;
                break;
            }
        }
    }
    messages
        .into_iter()
        .zip(keep)
        .filter_map(|(message, keep)| keep.then_some(message))
        .collect()
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

/// Runs `f`, containing an unwinding panic and returning `None` in its place.
///
/// The process-wide panic hook still runs first, so the panic's message and
/// backtrace are written to stderr as usual — only the unwind is swallowed, and
/// only stderr (never stdout, the protocol channel) is touched. `f` borrows the
/// server state mutably, which is not `UnwindSafe`; asserting it is safe is sound
/// because both callers treat a caught panic the same way: they discard the host
/// with [`ServerState::rebuild_host`] and never read the possibly-inconsistent
/// cached state back. Any future `catch` site must recover the same way.
fn catch<R>(f: impl FnOnce() -> R) -> Option<R> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).ok()
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
        // The transport pump surfaces a typing burst to the loop as a buffered
        // backlog; this covers the drain-and-coalesce path over that buffer
        // directly. A `didChange` head plus two more for the same document, all
        // already buffered, collapse to the final text — what a keystroke burst must
        // become once the pump lets the backlog exist.
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

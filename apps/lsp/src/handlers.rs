//! Per-request and per-notification handlers.
//!
//! Each handler resolves the document's path from its URI, converts the LSP
//! position(s) to byte offsets with the correct file's line index, asks the `ide`
//! layer, and converts the answer back. A URI this server cannot map to a file
//! (a non-`file` scheme, an untitled buffer) yields a null result and no
//! diagnostics, never a panic.
//!
//! The per-method orchestration lives in small **cores** generic over
//! [`DocQueries`], so the worker (an [`ide::Analysis`] bound to a path) and a
//! concurrent read (an [`ide::DocumentAnalysis`]) answer through exactly the same
//! code and cannot drift (#292). The `pub(crate)` request handlers keep their
//! exact signatures — the unit tests pin them — and delegate to the cores; the
//! read pool reaches the same cores through [`dispatch_pool_request`].

use std::path::Path;
use std::sync::Arc;

use inference_ide as ide;
use lsp_server::{ErrorCode, ExtractError, Request, Response};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, InlayHintRequest,
    Request as _,
};
use lsp_types::{
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, InlayHint, InlayHintParams, Location, MarkupKind,
    Position, PublishDiagnosticsParams, Range, SymbolInformation, Uri,
};

use crate::server::{Document, NegotiatedCapabilities, ServerState};
use crate::{convert, uri};

/// The document-query surface both the worker and a concurrent read answer
/// through, so the per-method cores below cannot drift between them (#292).
///
/// The worker answers with an [`ide::Analysis`] bound to the document's path
/// ([`WorkerDoc`]); a pool read answers with an owned [`ide::DocumentAnalysis`].
trait DocQueries {
    fn line_index(&self) -> Option<Arc<ide::LineIndex>>;
    fn hover(&self, offset: u32) -> Option<ide::Hover>;
    fn goto_definition(&self, offset: u32) -> Option<Vec<ide::NavigationTarget>>;
    fn completions(&self, offset: u32) -> Vec<ide::CompletionItem>;
    fn document_symbols(&self) -> Vec<ide::DocumentSymbol>;
    fn inlay_hints(&self, range: Option<ide::TextRange>) -> Vec<ide::InlayHint>;
    fn closure_line_index(&self, target: &Path) -> Option<Arc<ide::LineIndex>>;
}

/// The worker adapter: an [`ide::Analysis`] paired with the document path its
/// queries take.
struct WorkerDoc<'a> {
    analysis: ide::Analysis<'a>,
    path: &'a Path,
}

impl DocQueries for WorkerDoc<'_> {
    fn line_index(&self) -> Option<Arc<ide::LineIndex>> {
        self.analysis.line_index(self.path)
    }
    fn hover(&self, offset: u32) -> Option<ide::Hover> {
        self.analysis.hover(self.path, offset)
    }
    fn goto_definition(&self, offset: u32) -> Option<Vec<ide::NavigationTarget>> {
        self.analysis.goto_definition(self.path, offset)
    }
    fn completions(&self, offset: u32) -> Vec<ide::CompletionItem> {
        self.analysis.completions(self.path, offset)
    }
    fn document_symbols(&self) -> Vec<ide::DocumentSymbol> {
        self.analysis.document_symbols(self.path)
    }
    fn inlay_hints(&self, range: Option<ide::TextRange>) -> Vec<ide::InlayHint> {
        self.analysis.inlay_hints(self.path, range)
    }
    fn closure_line_index(&self, target: &Path) -> Option<Arc<ide::LineIndex>> {
        self.analysis.closure_line_index(self.path, target)
    }
}

impl DocQueries for ide::DocumentAnalysis {
    // Explicit UFCS to the inherent methods, so these delegations cannot be read as
    // recursive trait calls.
    fn line_index(&self) -> Option<Arc<ide::LineIndex>> {
        ide::DocumentAnalysis::line_index(self)
    }
    fn hover(&self, offset: u32) -> Option<ide::Hover> {
        ide::DocumentAnalysis::hover(self, offset)
    }
    fn goto_definition(&self, offset: u32) -> Option<Vec<ide::NavigationTarget>> {
        ide::DocumentAnalysis::goto_definition(self, offset)
    }
    fn completions(&self, offset: u32) -> Vec<ide::CompletionItem> {
        ide::DocumentAnalysis::completions(self, offset)
    }
    fn document_symbols(&self) -> Vec<ide::DocumentSymbol> {
        ide::DocumentAnalysis::document_symbols(self)
    }
    fn inlay_hints(&self, range: Option<ide::TextRange>) -> Vec<ide::InlayHint> {
        ide::DocumentAnalysis::inlay_hints(self, range)
    }
    fn closure_line_index(&self, target: &Path) -> Option<Arc<ide::LineIndex>> {
        ide::DocumentAnalysis::closure_line_index(self, target)
    }
}

// --- Per-method cores (shared by the worker handlers and the pool dispatcher) --

fn hover_core(doc: &impl DocQueries, position: Position, format: MarkupKind) -> Option<Hover> {
    let index = doc.line_index()?;
    let offset = convert::offset(&index, position)?;
    let hover = doc.hover(offset)?;
    Some(convert::hover(&index, hover, format))
}

fn goto_definition_core(
    doc: &impl DocQueries,
    position: Position,
    path: &Path,
) -> Option<GotoDefinitionResponse> {
    let index = doc.line_index()?;
    let offset = convert::offset(&index, position)?;
    let targets = doc.goto_definition(offset)?;

    let mut locations = Vec::with_capacity(targets.len());
    for target in targets {
        // A cross-file target's range is in that file's own coordinates, so it
        // must be converted with the target file's line index — reused from this
        // document's already-computed closure, never re-analyzed.
        let target_index = if target.path == path {
            index.clone()
        } else {
            match doc.closure_line_index(&target.path) {
                Some(target_index) => target_index,
                None => continue,
            }
        };
        let Some(target_uri) = uri::from_path(&target.path) else {
            continue;
        };
        locations.push(Location {
            uri: target_uri,
            range: convert::range(&target_index, target.focus_range),
        });
    }

    match locations.len() {
        0 => None,
        1 => Some(GotoDefinitionResponse::Scalar(locations.remove(0))),
        _ => Some(GotoDefinitionResponse::Array(locations)),
    }
}

fn completion_core(doc: &impl DocQueries, position: Position) -> Option<CompletionResponse> {
    let index = doc.line_index()?;
    let offset = convert::offset(&index, position)?;
    let items = doc
        .completions(offset)
        .into_iter()
        .map(convert::completion_item)
        .collect();
    Some(CompletionResponse::Array(items))
}

fn document_symbol_core(
    doc: &impl DocQueries,
    uri: &Uri,
    hierarchical: bool,
) -> Option<DocumentSymbolResponse> {
    let index = doc.line_index()?;
    let symbols = doc.document_symbols();

    if hierarchical {
        let nested = symbols
            .into_iter()
            .map(|symbol| convert::document_symbol(&index, symbol))
            .collect();
        return Some(DocumentSymbolResponse::Nested(nested));
    }

    let mut flat = Vec::new();
    for symbol in symbols {
        push_flat_symbol(&index, uri, None, symbol, &mut flat);
    }
    Some(DocumentSymbolResponse::Flat(flat))
}

fn inlay_hint_core(doc: &impl DocQueries, range: Range) -> Option<Vec<InlayHint>> {
    let index = doc.line_index()?;
    let clip = convert::text_range_clamped(&index, range);
    let hints = doc
        .inlay_hints(Some(clip))
        .into_iter()
        .map(|hint| convert::inlay_hint(&index, hint))
        .collect();
    Some(hints)
}

// --- Worker request handlers (exact signatures pinned by the unit tests) ------

pub(crate) fn hover(state: &mut ServerState, params: HoverParams) -> Option<Hover> {
    let position = params.text_document_position_params;
    let path = uri::to_path(&position.text_document.uri)?;
    crate::server::analysis_panic_seam(&path);
    let format = state.capabilities.hover_format();
    let doc = WorkerDoc {
        analysis: state.host.analysis(),
        path: &path,
    };
    hover_core(&doc, position.position, format)
}

pub(crate) fn goto_definition(
    state: &mut ServerState,
    params: GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let position = params.text_document_position_params;
    let path = uri::to_path(&position.text_document.uri)?;
    let doc = WorkerDoc {
        analysis: state.host.analysis(),
        path: &path,
    };
    goto_definition_core(&doc, position.position, &path)
}

pub(crate) fn completion(
    state: &mut ServerState,
    params: CompletionParams,
) -> Option<CompletionResponse> {
    let position = params.text_document_position;
    let path = uri::to_path(&position.text_document.uri)?;
    let doc = WorkerDoc {
        analysis: state.host.analysis(),
        path: &path,
    };
    completion_core(&doc, position.position)
}

pub(crate) fn document_symbol(
    state: &mut ServerState,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;
    let path = uri::to_path(&uri)?;
    let hierarchical = state.capabilities.hierarchical_symbols;
    let doc = WorkerDoc {
        analysis: state.host.analysis(),
        path: &path,
    };
    document_symbol_core(&doc, &uri, hierarchical)
}

pub(crate) fn inlay_hint(
    state: &mut ServerState,
    params: InlayHintParams,
) -> Option<Vec<InlayHint>> {
    let path = uri::to_path(&params.text_document.uri)?;
    let doc = WorkerDoc {
        analysis: state.host.analysis(),
        path: &path,
    };
    inlay_hint_core(&doc, params.range)
}

// --- Pool dispatcher (#292) ---------------------------------------------------

/// Runs a pool-served request against `doc`, producing the response to send —
/// extracting params and classifying errors exactly as [`ServerState::dispatch`]
/// does, so a pool answer is byte-identical to the worker's for the same input.
///
/// Only the pool-eligible methods (see `crate::server::POOL_METHODS`) reach here;
/// an unexpected method is answered defensively rather than panicking the pool
/// thread.
pub(crate) fn dispatch_pool_request(
    request: Request,
    doc: &ide::DocumentAnalysis,
    capabilities: NegotiatedCapabilities,
    path: &Path,
    uri: &Uri,
) -> Response {
    // Test-only: forces a post-serve dispatch panic for a marked document, so the
    // read pool's widened catch is exercised (a no-op in release).
    crate::server::dispatch_panic_seam(path);
    if request.method == HoverRequest::METHOD {
        pool_dispatch::<HoverRequest>(request, |params| {
            hover_core(
                doc,
                params.text_document_position_params.position,
                capabilities.hover_format(),
            )
        })
    } else if request.method == GotoDefinition::METHOD {
        pool_dispatch::<GotoDefinition>(request, |params| {
            goto_definition_core(doc, params.text_document_position_params.position, path)
        })
    } else if request.method == Completion::METHOD {
        pool_dispatch::<Completion>(request, |params| {
            completion_core(doc, params.text_document_position.position)
        })
    } else if request.method == DocumentSymbolRequest::METHOD {
        pool_dispatch::<DocumentSymbolRequest>(request, |_params| {
            document_symbol_core(doc, uri, capabilities.hierarchical_symbols)
        })
    } else if request.method == InlayHintRequest::METHOD {
        pool_dispatch::<InlayHintRequest>(request, |params| inlay_hint_core(doc, params.range))
    } else {
        Response::new_err(
            request.id,
            ErrorCode::MethodNotFound as i32,
            format!("unsupported pool request: {}", request.method),
        )
    }
}

/// Deserializes `request`'s params for `R`, runs `run`, and wraps the result —
/// the pool-side twin of [`ServerState::dispatch`], with the identical
/// InvalidParams / method-mismatch classification.
fn pool_dispatch<R>(request: Request, run: impl FnOnce(R::Params) -> R::Result) -> Response
where
    R: lsp_types::request::Request,
{
    let id = request.id.clone();
    match request.extract::<R::Params>(R::METHOD) {
        Ok((id, params)) => Response::new_ok(id, run(params)),
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

/// Appends `symbol` and its descendants to `flat`, recording each one's enclosing
/// symbol name as its container (the shape a non-hierarchical client expects).
fn push_flat_symbol(
    index: &ide::LineIndex,
    uri: &Uri,
    container: Option<&str>,
    symbol: ide::DocumentSymbol,
    flat: &mut Vec<SymbolInformation>,
) {
    flat.push(convert::symbol_information(index, uri, container, &symbol));
    for child in symbol.children {
        push_flat_symbol(index, uri, Some(&symbol.name), child, flat);
    }
}

pub(crate) fn did_open(
    state: &mut ServerState,
    params: DidOpenTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    let document = params.text_document;
    let path = uri::to_path(&document.uri)?;
    let text: Arc<str> = document.text.into();
    state.host.open_document(&path, Arc::clone(&text));
    state.documents.insert(
        document.uri.clone(),
        Document {
            path,
            version: document.version,
            text,
        },
    );
    Some(publish_diagnostics_params(state, &document.uri))
}

/// Applies a full-text change to an already-open document and returns its fresh
/// diagnostics.
///
/// A `didChange` is only valid for a document the client has already opened (LSP
/// 3.17 sends `didChange` only between a document's `didOpen` and its
/// `didClose`). A change for a URI not in the tracked set — one never opened, or
/// one closed since (VS Code's preview-tab close race can emit a change just
/// after `didClose`) — is a protocol violation and is dropped: the path is not
/// interned, the URI is not adopted into the tracked set or any future
/// dependents-republish sweep, and nothing is published (#275). This mirrors the
/// URI layer's treat-unmappable-input-as-absent philosophy; a later proper
/// `didOpen` starts tracking the document normally, unaffected by the dropped
/// change. The drop is logged to stderr, never stdout (the protocol channel).
pub(crate) fn did_change(
    state: &mut ServerState,
    params: DidChangeTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;
    if !state.documents.contains_key(&uri) {
        eprintln!(
            "inference-lsp: ignoring didChange for a document with no prior didOpen: {}",
            uri.as_str()
        );
        return None;
    }
    let version = params.text_document.version;
    let path = uri::to_path(&uri)?;
    // Full-text sync: the last content change carries the whole new document.
    let text: Arc<str> = params.content_changes.into_iter().next_back()?.text.into();
    state.host.change_document(&path, Arc::clone(&text));
    state.documents.insert(
        uri.clone(),
        Document {
            path,
            version,
            text,
        },
    );
    Some(publish_diagnostics_params(state, &uri))
}

pub(crate) fn did_close(
    state: &mut ServerState,
    params: DidCloseTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;
    // A URI this server never mapped to a file was never opened or tracked, so
    // closing it clears nothing: publish no empty set and trigger no dependents
    // sweep, matching `did_open`'s "no diagnostics for an unmappable URI".
    let path = uri::to_path(&uri)?;
    state.host.close_document(&path);
    state.documents.remove(&uri);
    // Publish an empty set so the editor clears any diagnostics it was showing.
    Some(PublishDiagnosticsParams {
        uri,
        diagnostics: Vec::new(),
        version: None,
    })
}

/// The diagnostics to publish for the tracked document `uri`, converted to LSP
/// coordinates. An untracked or non-analyzable document publishes an empty set.
pub(crate) fn publish_diagnostics_params(
    state: &mut ServerState,
    uri: &Uri,
) -> PublishDiagnosticsParams {
    let Some(document) = state.documents.get(uri) else {
        return PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: Vec::new(),
            version: None,
        };
    };
    let path = document.path.clone();
    let version = document.version;
    crate::server::analysis_panic_seam(&path);

    let diagnostics = match state.host.analysis().line_index(&path) {
        Some(index) => state
            .host
            .analysis()
            .diagnostics(&path)
            .into_iter()
            .map(|diagnostic| convert::diagnostic(&index, diagnostic))
            .collect(),
        None => Vec::new(),
    };

    PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: Some(version),
    }
}

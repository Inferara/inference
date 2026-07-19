//! Per-request and per-notification handlers.
//!
//! Each handler resolves the document's path from its URI, converts the LSP
//! position(s) to byte offsets with the correct file's line index, asks the `ide`
//! layer, and converts the answer back. A URI this server cannot map to a file
//! (a non-`file` scheme, an untitled buffer) yields a null result and no
//! diagnostics, never a panic.

use std::sync::Arc;

use inference_ide as ide;
use lsp_types::{
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, InlayHint, InlayHintParams, Location,
    PublishDiagnosticsParams, SymbolInformation, Uri,
};

use crate::server::{Document, ServerState};
use crate::{convert, uri};

pub(crate) fn hover(state: &mut ServerState, params: HoverParams) -> Option<Hover> {
    let position = params.text_document_position_params;
    let path = uri::to_path(&position.text_document.uri)?;
    let index = state.host.analysis().line_index(&path)?;
    let offset = convert::offset(&index, position.position)?;
    let hover = state.host.analysis().hover(&path, offset)?;
    Some(convert::hover(&index, hover))
}

pub(crate) fn goto_definition(
    state: &mut ServerState,
    params: GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let position = params.text_document_position_params;
    let path = uri::to_path(&position.text_document.uri)?;
    let index = state.host.analysis().line_index(&path)?;
    let offset = convert::offset(&index, position.position)?;
    let targets = state.host.analysis().goto_definition(&path, offset)?;

    let mut locations = Vec::with_capacity(targets.len());
    for target in targets {
        // A cross-file target's range is in that file's own coordinates, so it
        // must be converted with the target file's line index — reused from this
        // document's already-computed closure, never re-analyzed.
        let target_index = if target.path == path {
            index.clone()
        } else {
            match state
                .host
                .analysis()
                .closure_line_index(&path, &target.path)
            {
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

pub(crate) fn completion(
    state: &mut ServerState,
    params: CompletionParams,
) -> Option<CompletionResponse> {
    let position = params.text_document_position;
    let path = uri::to_path(&position.text_document.uri)?;
    let index = state.host.analysis().line_index(&path)?;
    let offset = convert::offset(&index, position.position)?;
    let items = state
        .host
        .analysis()
        .completions(&path, offset)
        .into_iter()
        .map(convert::completion_item)
        .collect();
    Some(CompletionResponse::Array(items))
}

pub(crate) fn document_symbol(
    state: &mut ServerState,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;
    let path = uri::to_path(&uri)?;
    let index = state.host.analysis().line_index(&path)?;
    let symbols = state.host.analysis().document_symbols(&path);

    if state.hierarchical_symbols {
        let nested = symbols
            .into_iter()
            .map(|symbol| convert::document_symbol(&index, symbol))
            .collect();
        return Some(DocumentSymbolResponse::Nested(nested));
    }

    let mut flat = Vec::new();
    for symbol in symbols {
        push_flat_symbol(&index, &uri, None, symbol, &mut flat);
    }
    Some(DocumentSymbolResponse::Flat(flat))
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

pub(crate) fn inlay_hint(
    state: &mut ServerState,
    params: InlayHintParams,
) -> Option<Vec<InlayHint>> {
    let path = uri::to_path(&params.text_document.uri)?;
    let index = state.host.analysis().line_index(&path)?;
    let clip = convert::text_range(&index, params.range);
    let hints = state
        .host
        .analysis()
        .inlay_hints(&path, clip)
        .into_iter()
        .map(|hint| convert::inlay_hint(&index, hint))
        .collect();
    Some(hints)
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

pub(crate) fn did_change(
    state: &mut ServerState,
    params: DidChangeTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;
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
    if let Some(path) = uri::to_path(&uri) {
        state.host.close_document(&path);
    }
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

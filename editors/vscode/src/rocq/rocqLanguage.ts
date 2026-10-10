import * as vscode from 'vscode';

import { ROCQ_LANGUAGE_ID, shouldShowAsRocq } from './rocqDocuments';

/** `files.associations` patterns that map to plain text for this document. */
function plainTextPatterns(uri: vscode.Uri): string[] {
    const associations = vscode.workspace
        .getConfiguration('files', uri)
        .get<Record<string, string>>('associations') ?? {};
    return Object.entries(associations)
        .filter(([, language]) => language === 'plaintext')
        .map(([pattern]) => pattern);
}

/**
 * Highlights `.v` files as Rocq when no other extension does (see
 * `rocqDocuments.ts`). A file the user switches to another language keeps
 * that choice until it is reopened.
 */
export function registerRocqHighlighting(): vscode.Disposable {
    const shown = new Set<string>();
    const keptByUser = new Set<string>();
    // VS Code reports a language change as a close and an open in the same
    // turn; a real close is followed by the reopen later, if at all. Several
    // documents can close in one turn, so each is tracked on its own.
    const closing = new Set<string>();

    const show = (doc: vscode.TextDocument): void => {
        const key = doc.uri.toString();
        if (keptByUser.has(key) || !shouldShowAsRocq({ languageId: doc.languageId, path: doc.uri.path }, plainTextPatterns(doc.uri))) {
            return;
        }
        shown.add(key);
        vscode.languages.setTextDocumentLanguage(doc, ROCQ_LANGUAGE_ID).then(undefined, () => {
            // The document closed before the switch.
        });
    };

    const subscriptions = [
        vscode.workspace.onDidCloseTextDocument((doc) => {
            const key = doc.uri.toString();
            if (!shown.has(key)) {
                return;
            }
            closing.add(key);
            queueMicrotask(() => {
                // Still pending: a real close, so a reopen starts afresh.
                if (closing.delete(key)) {
                    keptByUser.delete(key);
                }
            });
        }),
        vscode.workspace.onDidOpenTextDocument((doc) => {
            const key = doc.uri.toString();
            if (closing.delete(key)) {
                if (doc.languageId !== ROCQ_LANGUAGE_ID) {
                    keptByUser.add(key);
                }
                return;
            }
            show(doc);
        }),
    ];
    for (const doc of vscode.workspace.textDocuments) {
        show(doc);
    }
    return vscode.Disposable.from(...subscriptions);
}

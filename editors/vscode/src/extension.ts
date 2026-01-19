import * as vscode from 'vscode';

export function activate(context: vscode.ExtensionContext) {
    // Syntax-only extension - no activation logic needed
    // TextMate grammar and language configuration handle everything
    console.log('Inference extension activated');
}

export function deactivate() {
    // Nothing to clean up
}

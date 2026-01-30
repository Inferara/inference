import * as vscode from 'vscode';

export interface InferenceSettings {
    /** Custom path to infs binary. Empty string means auto-detect. */
    path: string;
    /** Prompt to install toolchain if not found. */
    autoInstall: boolean;
    /** Check for toolchain updates on activation. */
    checkForUpdates: boolean;
}

/** Read current inference.* configuration values. */
export function getSettings(): InferenceSettings {
    const config = vscode.workspace.getConfiguration('inference');
    return {
        path: config.get<string>('path', ''),
        autoInstall: config.get<boolean>('autoInstall', true),
        checkForUpdates: config.get<boolean>('checkForUpdates', true),
    };
}

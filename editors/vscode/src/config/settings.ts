import * as vscode from 'vscode';

export type ReleaseChannel = 'stable' | 'latest' | 'none';

export interface InferenceSettings {
    /** Custom path to infs binary. Empty string means auto-detect. */
    path: string;
    /** Prompt to install toolchain if not found. */
    autoInstall: boolean;
    /** Release channel for update checks. */
    channel: ReleaseChannel;
    /** Check for toolchain updates on activation. */
    checkForUpdates: boolean;
}

/** Read current inference.* configuration values. */
export function getSettings(): InferenceSettings {
    const config = vscode.workspace.getConfiguration('inference');
    return {
        path: config.get<string>('path', ''),
        autoInstall: config.get<boolean>('autoInstall', true),
        channel: config.get<ReleaseChannel>('channel', 'stable'),
        checkForUpdates: config.get<boolean>('checkForUpdates', true),
    };
}

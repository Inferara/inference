import * as vscode from 'vscode';
import { DoctorResult } from '../toolchain/doctor';
import { determineStatusBarState, StatusBarIcon } from './statusBarState';

const ICON_MAP: Record<StatusBarIcon, string> = {
    loading: '$(loading~spin)',
    dash: '$(dash)',
    check: '$(check)',
    warning: '$(warning)',
    error: '$(error)',
};

const BACKGROUND_MAP: Record<string, vscode.ThemeColor | undefined> = {
    none: undefined,
    warning: new vscode.ThemeColor('statusBarItem.warningBackground'),
    error: new vscode.ThemeColor('statusBarItem.errorBackground'),
};

/**
 * Create the Inference status bar item.
 * Positioned on the left side with low priority.
 * Clicking triggers the inference.runDoctor command.
 */
export function createStatusBar(): vscode.StatusBarItem {
    const item = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Left,
        0,
    );
    item.command = 'inference.runDoctor';
    item.text = '$(loading~spin) Inference';
    item.tooltip = 'Inference: Checking toolchain...';
    item.show();
    return item;
}

/**
 * Update the status bar to reflect doctor results.
 *
 * - null: toolchain not found (grey dash icon)
 * - hasErrors: red error icon
 * - hasWarnings: yellow warning icon
 * - all OK: green check icon
 */
export function updateStatusBar(
    item: vscode.StatusBarItem,
    result: DoctorResult | null,
): void {
    const state = determineStatusBarState(result);
    item.text = `${ICON_MAP[state.icon]} ${state.label}`;
    item.tooltip = state.tooltip;
    item.backgroundColor = BACKGROUND_MAP[state.background];
}

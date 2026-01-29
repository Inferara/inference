import * as vscode from 'vscode';
import { DoctorResult } from '../toolchain/doctor';

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
    if (result === null) {
        item.text = '$(dash) Inference';
        item.tooltip = 'Inference: Toolchain not found. Click to run doctor.';
        item.backgroundColor = undefined;
        return;
    }

    if (result.hasErrors) {
        item.text = '$(error) Inference';
        item.tooltip = `Inference: ${result.summary || 'Toolchain errors detected'}`;
        item.backgroundColor = new vscode.ThemeColor(
            'statusBarItem.errorBackground',
        );
        return;
    }

    if (result.hasWarnings) {
        item.text = '$(warning) Inference';
        item.tooltip = `Inference: ${result.summary || 'Toolchain warnings detected'}`;
        item.backgroundColor = new vscode.ThemeColor(
            'statusBarItem.warningBackground',
        );
        return;
    }

    item.text = '$(check) Inference';
    item.tooltip = 'Inference: Toolchain healthy';
    item.backgroundColor = undefined;
}

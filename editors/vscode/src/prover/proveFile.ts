import * as path from 'path';
import * as vscode from 'vscode';

import { performVersionChange } from '../commands/versionChange';
import { detectInfs } from '../toolchain/detection';
import { runDoctor } from '../toolchain/doctor';
import { run } from '../utils/spawn';
import { ProverApi } from './api';
import { requireConfig } from './config';
import { proveInfFile, type ProveOutcome } from './proveFlow';
import { confirmUpload, pickFile, saveIfDirty, submitErrorMessage, type SubmitHooks } from './submitProof';
import { installableVersion, resolvedInfcPath } from './toolchainIdentity';

/** Last compiler check, shown in the Configuration view. */
export interface CompilerCheck {
    state: 'match' | 'mismatch' | 'unchecked';
    detail: string;
}

/**
 * `inference.proveFile`: compile the `.inf` in proof mode with the installed
 * toolchain, check the compiler against the proof server's accepted one,
 * then submit the generated `.v` and open the job.
 */
export function registerProveFileCommand(
    context: vscode.ExtensionContext,
    log: vscode.LogOutputChannel,
    hooks: SubmitHooks,
    onCompilerCheck: (check: CompilerCheck) => void,
): vscode.Disposable {
    let running = false;
    return vscode.commands.registerCommand('inference.proveFile', async (arg?: unknown) => {
        if (running) {
            vscode.window.showInformationMessage('Inference: a proof build is already running.');
            return;
        }
        const config = await requireConfig(context.secrets);
        if (!config) {
            return;
        }
        const uri = await pickFile(arg, '.inf', {
            title: 'Select the Inference source to prove',
            filters: { 'Inference source': ['inf'] },
        });
        if (!uri || uri.scheme !== 'file' || !(await saveIfDirty(uri, 'Prove'))) {
            return;
        }
        if (!(await confirmUpload(context, config.serverUrl))) {
            return;
        }
        running = true;
        const source = path.basename(uri.fsPath);
        const api = new ProverApi(config.serverUrl, config.apiKey);
        try {
            const outcome = await vscode.window.withProgress(
                {
                    location: vscode.ProgressLocation.Notification,
                    title: `Proving ${source}`,
                    cancellable: true,
                },
                (progress, token) => {
                    const abort = new AbortController();
                    token.onCancellationRequested(() => abort.abort());
                    log.info(`Prover: proving ${uri.fsPath}`);
                    return proveInfFile(uri.fsPath, {
                        locateInfs: () => detectInfs()?.path ?? null,
                        resolveInfc: async (infsPath) => resolvedInfcPath(await runDoctor(infsPath)),
                        run: (command, args, options) =>
                            run(command, args, {
                                ...options,
                                signal: abort.signal,
                                onLine: (line) => log.info(`  ${line}`),
                            }),
                        readFile: (file) => Promise.resolve(vscode.workspace.fs.readFile(vscode.Uri.file(file))),
                        api,
                        confirmUnchecked: async () =>
                            (await vscode.window.showWarningMessage(
                                'This proof server does not say which compiler it accepts.',
                                {
                                    modal: true,
                                    detail: 'Your infc cannot be checked against it. A mismatched compiler produces a file the prover cannot compile.',
                                },
                                'Compile and Submit',
                            )) === 'Compile and Submit',
                        progress: (message) => {
                            progress.report({ message });
                            log.info(`Prover: ${message}`);
                        },
                    });
                },
            );
            await report(outcome, source, log, hooks, onCompilerCheck);
        } catch (err) {
            log.error(`Prover: prove ${source}: ${err instanceof Error ? err.message : err}`);
            vscode.window.showErrorMessage(`Inference: ${submitErrorMessage(err)}`);
        } finally {
            running = false;
        }
    });
}

async function report(
    outcome: ProveOutcome,
    source: string,
    log: vscode.LogOutputChannel,
    hooks: SubmitHooks,
    onCompilerCheck: (check: CompilerCheck) => void,
): Promise<void> {
    const showOutput = async (message: string, error = true) => {
        const show = error
            ? vscode.window.showErrorMessage(message, 'Show Output')
            : vscode.window.showWarningMessage(message, 'Show Output');
        if ((await show) === 'Show Output') {
            log.show();
        }
    };
    switch (outcome.kind) {
        case 'submitted': {
            onCompilerCheck(
                outcome.checked
                    ? { state: 'match', detail: 'matches the accepted compiler' }
                    : { state: 'unchecked', detail: 'server does not publish an accepted compiler' },
            );
            log.info(`Prover: ${source} → ${outcome.vPath} (${outcome.holes} holes) → job ${outcome.job.id}`);
            await hooks.showJob(outcome.job, source);
            return;
        }
        case 'no-infs': {
            const action = await vscode.window.showErrorMessage(
                'Inference: the toolchain is not installed, so the file cannot be compiled.',
                'Install Toolchain',
            );
            if (action === 'Install Toolchain') {
                await vscode.commands.executeCommand('inference.installToolchain');
            }
            return;
        }
        case 'no-infc': {
            const action = await vscode.window.showErrorMessage(
                'Inference: infs could not find a working infc compiler.',
                'Run Doctor',
            );
            if (action === 'Run Doctor') {
                await vscode.commands.executeCommand('inference.runDoctor');
            }
            return;
        }
        case 'toolchain-mismatch': {
            const { local, accepted, reasons } = outcome;
            onCompilerCheck({ state: 'mismatch', detail: reasons.join('; ') });
            const version = installableVersion(accepted);
            const detail = [
                ...reasons.map((r) => `• ${r}`),
                '',
                `Yours: ${local.version ?? 'infc (unknown version)'}, commit ${local.commit ?? 'unknown'}, ABI ${local.abiVersion ?? 'unknown'}.`,
                `Accepted: ${accepted.version}, commit ${accepted.commit.slice(0, 12)}, ABI ${accepted.abiVersion}.`,
                '',
                version
                    ? `Install ${version} and run Prove again.`
                    : 'The accepted compiler is not a published release. Ask the proof-server operator for that build.',
            ].join('\n');
            log.warn(`Prover: compiler mismatch for ${source}: ${reasons.join('; ')}`);
            const install = version ? `Install infc ${version}` : undefined;
            const choice = await vscode.window.showErrorMessage(
                'Your compiler is not the one this proof server accepts.',
                { modal: true, detail },
                ...(install ? [install] : []),
            );
            const infs = detectInfs();
            if (install && choice === install && version && infs) {
                await performVersionChange(infs.path, version, log, 'Switching to');
            }
            return;
        }
        case 'build-failed':
            await showOutput(
                outcome.timedOut
                    ? `Inference: compiling ${source} timed out.`
                    : `Inference: compiling ${source} failed (exit ${outcome.exitCode}).`,
            );
            return;
        case 'no-output':
            await showOutput(`Inference: infs build finished but reported no .v file for ${source}.`);
            return;
        case 'preflight-failed': {
            const action = await vscode.window.showErrorMessage(`Inference: ${outcome.problem}`, 'Open File');
            if (action === 'Open File') {
                await vscode.window.showTextDocument(vscode.Uri.file(outcome.vPath));
            }
            return;
        }
        case 'cancelled':
            log.info(`Prover: proving ${source} was cancelled.`);
            return;
    }
}

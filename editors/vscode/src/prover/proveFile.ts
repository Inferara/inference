import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';

import { performVersionChange } from '../commands/versionChange';
import { detectInfs } from '../toolchain/detection';
import { runDoctor } from '../toolchain/doctor';
import { run } from '../utils/spawn';
import { ProverApi } from './api';
import { requireConfig } from './config';
import { parseInfcDiagnostics, summarizeDiagnostics, type ParsedDiagnostics } from './infcDiagnostics';
import { proveInfFile, type ProveOutcome } from './proveFlow';
import type { DuplicateDefinition } from './submission';
import { confirmUpload, notifyReplay, pickFile, saveBeforeRun, submitErrorMessage, type SubmitHooks } from './submitProof';
import { installableVersion, resolvedInfcPath } from './toolchainIdentity';

/** Last compiler check, shown in the Configuration view. */
export interface CompilerCheck {
    state: 'match' | 'mismatch' | 'unchecked';
    detail: string;
}

const UNCHECKED_OK_KEY = 'inference.prover.uncheckedCompilerOk';

/**
 * A progress notification that steps aside for questions: `pause()` closes it
 * before a dialog, and the next `report()` opens a new one, so a modal never
 * sits on top of a progress toast.
 */
class StepProgress {
    private report?: (message: string) => void;
    private finish?: () => void;
    private latest = '';
    private readonly abort = new AbortController();

    constructor(private readonly title: string) {}

    get signal(): AbortSignal {
        return this.abort.signal;
    }

    get cancelled(): boolean {
        return this.abort.signal.aborted;
    }

    step(message: string): void {
        this.latest = message;
        if (this.report) {
            this.report(message);
            return;
        }
        if (this.finish) {
            return; // opening; the task reports `latest` when it starts
        }
        let open = true;
        this.finish = () => {
            open = false;
        };
        void vscode.window.withProgress(
            { location: vscode.ProgressLocation.Notification, title: this.title, cancellable: true },
            (progress, token) => {
                token.onCancellationRequested(() => this.abort.abort());
                return new Promise<void>((resolve) => {
                    if (!open) {
                        resolve(); // paused before the notification appeared
                        return;
                    }
                    this.report = (m) => progress.report({ message: m });
                    this.finish = resolve;
                    progress.report({ message: this.latest });
                });
            },
        );
    }

    /** Close the notification (before a dialog, or when done). */
    pause(): void {
        const finish = this.finish;
        this.finish = undefined;
        this.report = undefined;
        finish?.();
    }
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
    const problems = new ProveProblems();

    const command = vscode.commands.registerCommand('inference.proveFile', async (arg?: unknown) => {
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
        if (!uri || uri.scheme !== 'file' || !(await saveBeforeRun(uri))) {
            return;
        }
        running = true;
        const source = path.basename(uri.fsPath);
        const api = new ProverApi(config.serverUrl, config.apiKey);
        const progress = new StepProgress(`Proving ${source}`);
        problems.start(uri);
        let again = false;
        try {
            log.info(`Prover: proving ${uri.fsPath}`);
            const outcome = await proveInfFile(uri.fsPath, {
                locateInfs: () => detectInfs()?.path ?? null,
                resolveInfc: async (infsPath) => resolvedInfcPath(await runDoctor(infsPath)),
                run: (cmd, args, options) =>
                    run(cmd, args, {
                        ...options,
                        signal: progress.signal,
                        onLine: (line) => log.info(`  ${line}`),
                    }),
                readFile: (file) => Promise.resolve(vscode.workspace.fs.readFile(vscode.Uri.file(file))),
                api,
                confirmUnchecked: async () => {
                    const key = `${UNCHECKED_OK_KEY}:${config.serverUrl}`;
                    if (context.globalState.get<boolean>(key)) {
                        return true;
                    }
                    progress.pause();
                    const choice = await vscode.window.showWarningMessage(
                        "This proof server doesn't say which compiler it accepts.",
                        {
                            modal: true,
                            detail: "Your compiler can't be checked against it. If the server uses a different one, the uploaded file won't compile there.\n\nYou won't be asked again for this server.",
                        },
                        'Continue for This Server',
                    );
                    if (choice !== 'Continue for This Server') {
                        return false;
                    }
                    await context.globalState.update(key, true);
                    return true;
                },
                confirmUpload: async () => {
                    progress.pause();
                    return confirmUpload(context, config.serverUrl, `${path.basename(source, '.inf')}.v`);
                },
                progress: (message) => {
                    progress.step(message);
                    log.info(`Prover: ${message}`);
                },
                cancelled: () => progress.cancelled,
            });
            progress.pause();
            again = (await report(outcome, uri, log, hooks, onCompilerCheck, problems)) === 'again';
        } catch (err) {
            progress.pause();
            log.error(`Prover: prove ${source}: ${err instanceof Error ? err.message : err}`);
            vscode.window.showErrorMessage(`Inference: ${submitErrorMessage(err)}`);
        } finally {
            problems.finish();
            running = false;
        }
        // After the guard is released, or the command would refuse to run.
        if (again) {
            await vscode.commands.executeCommand('inference.proveFile', uri);
        }
    });
    return vscode.Disposable.from(command, problems);
}

/**
 * The Problems entries from proof builds. Each build owns what it published
 * (the entry file and any imported files) and replaces it on the next build
 * of the same entry; an edit clears a file's entries, and errors for a file
 * edited while its build ran are not published (they describe old text).
 */
class ProveProblems implements vscode.Disposable {
    private readonly collection = vscode.languages.createDiagnosticCollection('inference-prove');
    private readonly owned = new Map<string, vscode.Uri[]>();
    private edited: Set<string> | null = null;
    private readonly onEdit = vscode.workspace.onDidChangeTextDocument((e) => {
        if (e.contentChanges.length === 0) {
            return;
        }
        this.edited?.add(e.document.uri.toString());
        if (this.collection.has(e.document.uri)) {
            this.collection.delete(e.document.uri);
        }
    });

    /** A build of `source` starts: drop what its previous build published. */
    start(source: vscode.Uri): void {
        for (const uri of this.owned.get(source.toString()) ?? []) {
            this.collection.delete(uri);
        }
        this.collection.delete(source);
        this.owned.delete(source.toString());
        this.edited = new Set();
    }

    finish(): void {
        this.edited = null;
    }

    /** Publish `source`'s build errors per file; returns how many were published. */
    publish(source: vscode.Uri, byFile: ReadonlyMap<string, { uri: vscode.Uri; items: vscode.Diagnostic[] }>): number {
        const owned = this.owned.get(source.toString()) ?? [];
        let count = 0;
        for (const [key, { uri, items }] of byFile) {
            if (this.edited?.has(key)) {
                continue;
            }
            this.collection.set(uri, items);
            owned.push(uri);
            count += items.length;
        }
        this.owned.set(source.toString(), owned);
        return count;
    }

    dispose(): void {
        this.onEdit.dispose();
        this.collection.dispose();
    }
}

/** The file a diagnostic belongs to: the source, or a submodule `a::b` → `<dir>/a/b.inf`. */
function diagnosticFile(source: vscode.Uri, module: string | undefined): vscode.Uri {
    if (!module) {
        return source;
    }
    const candidate = path.join(path.dirname(source.fsPath), ...module.split('::')) + '.inf';
    return fs.existsSync(candidate) ? vscode.Uri.file(candidate) : source;
}

/** Publish compiler errors to the Problems panel; returns how many. */
function publish(
    problems: ProveProblems,
    source: vscode.Uri,
    parsed: ParsedDiagnostics,
): number {
    const byFile = new Map<string, { uri: vscode.Uri; items: vscode.Diagnostic[] }>();
    const add = (uri: vscode.Uri, d: vscode.Diagnostic) => {
        const entry = byFile.get(uri.toString()) ?? { uri, items: [] };
        entry.items.push(d);
        byFile.set(uri.toString(), entry);
    };
    for (const d of parsed.located) {
        const line = Math.max(0, d.line - 1);
        const column = Math.max(0, d.column - 1);
        const diagnostic = new vscode.Diagnostic(
            new vscode.Range(line, column, line, column + 1),
            d.message,
            vscode.DiagnosticSeverity.Error,
        );
        diagnostic.source = 'infc (proof build)';
        if (d.code) {
            diagnostic.code = d.code;
        }
        add(diagnosticFile(source, d.module), diagnostic);
    }
    for (const message of parsed.unlocated) {
        const diagnostic = new vscode.Diagnostic(new vscode.Range(0, 0, 0, 0), message, vscode.DiagnosticSeverity.Error);
        diagnostic.source = 'infc (proof build)';
        add(source, diagnostic);
    }
    return problems.publish(source, byFile);
}

/** Point at `fn <name>` in the source, for the module-named-after-the-file clash. */
async function duplicateInSource(
    problems: ProveProblems,
    source: vscode.Uri,
    duplicate: DuplicateDefinition,
): Promise<vscode.Range | undefined> {
    try {
        const doc = await vscode.workspace.openTextDocument(source);
        const pattern = new RegExp(`\\bfn\\s+${duplicate.name.replace(/[^\w]/g, '')}\\b`);
        for (let i = 0; i < doc.lineCount; i++) {
            const m = pattern.exec(doc.lineAt(i).text);
            if (m) {
                const range = new vscode.Range(i, m.index, i, m.index + m[0].length);
                const diagnostic = new vscode.Diagnostic(
                    range,
                    `“${duplicate.name}” is also the module name (the file is named ${path.basename(source.fsPath)}). Rename the file or this function.`,
                    vscode.DiagnosticSeverity.Error,
                );
                diagnostic.source = 'infc (proof build)';
                problems.publish(source, new Map([[source.toString(), { uri: source, items: [diagnostic] }]]));
                return range;
            }
        }
    } catch {
        // The source cannot be read; the notification still explains.
    }
    return undefined;
}

async function report(
    outcome: ProveOutcome,
    sourceUri: vscode.Uri,
    log: vscode.LogOutputChannel,
    hooks: SubmitHooks,
    onCompilerCheck: (check: CompilerCheck) => void,
    problems: ProveProblems,
): Promise<'again' | void> {
    const infPath = sourceUri.fsPath;
    const source = path.basename(infPath);
    switch (outcome.kind) {
        case 'submitted': {
            onCompilerCheck(
                outcome.checked
                    ? { state: 'match', detail: 'matches the accepted compiler' }
                    : { state: 'unchecked', detail: 'server does not publish an accepted compiler' },
            );
            log.info(`Prover: ${source} → ${outcome.vPath} (${outcome.holes} holes) → job ${outcome.job.id}${outcome.replayed ? ' (existing job)' : ''}`);
            if (outcome.replayed) {
                notifyReplay(outcome.job, source, hooks);
            }
            await hooks.showJob(outcome.job, { infPath, vPath: outcome.vPath, vSha256: outcome.vSha256 });
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
                    ? `Installing ${version} makes it your default toolchain and restarts the language server; Prove then runs again.`
                    : 'The accepted compiler is not a published release. Ask the proof server operator for that build.',
            ].join('\n');
            log.warn(`Prover: compiler mismatch for ${source}: ${reasons.join('; ')}`);
            const install = version ? `Install ${version} and Prove` : undefined;
            const copy = 'Copy Commit';
            const choice = await vscode.window.showErrorMessage(
                "Your compiler isn't the one this proof server accepts.",
                { modal: true, detail },
                ...(install ? [install] : [copy]),
            );
            const infs = detectInfs();
            if (install && choice === install && version && infs) {
                if (await performVersionChange(infs.path, version, log, 'Switching to')) {
                    return 'again';
                }
            } else if (choice === copy) {
                await vscode.env.clipboard.writeText(accepted.commit);
                vscode.window.setStatusBarMessage('Copied the accepted compiler commit', 3000);
            }
            return;
        }
        case 'build-failed': {
            if (outcome.timedOut) {
                const action = await vscode.window.showErrorMessage(
                    `Inference: compiling ${source} took longer than 5 minutes and was stopped.`,
                    'Show Output',
                );
                if (action === 'Show Output') {
                    log.show();
                }
                return;
            }
            const parsed = parseInfcDiagnostics(outcome.output);
            const count = publish(problems, sourceUri, parsed);
            const summary = summarizeDiagnostics(parsed);
            const action = await vscode.window.showErrorMessage(
                summary
                    ? `Inference: ${source} has ${summary}`
                    : `Inference: compiling ${source} failed (exit ${outcome.exitCode}).`,
                ...(count > 0 ? ['Show Problems'] : []),
                'Show Output',
            );
            if (action === 'Show Problems') {
                await vscode.commands.executeCommand('workbench.actions.view.problems');
            } else if (action === 'Show Output') {
                log.show();
            }
            return;
        }
        case 'no-output': {
            const action = await vscode.window.showErrorMessage(
                `Inference: the build finished but reported no Rocq file for ${source}.`,
                'Show Output',
            );
            if (action === 'Show Output') {
                log.show();
            }
            return;
        }
        case 'preflight-failed': {
            if (outcome.duplicate) {
                const range = await duplicateInSource(problems, sourceUri, outcome.duplicate);
                const goTo = `Go to fn ${outcome.duplicate.name}`;
                const action = await vscode.window.showErrorMessage(
                    `Inference: the generated Rocq file defines “${outcome.duplicate.name}” twice, so the proof server cannot compile it. The compiler names the module after the file; rename ${source} or the function “${outcome.duplicate.name}”.`,
                    ...(range ? [goTo] : ['Open Generated File']),
                );
                if (action === goTo && range) {
                    await vscode.window.showTextDocument(sourceUri, { selection: range });
                } else if (action === 'Open Generated File') {
                    await vscode.window.showTextDocument(vscode.Uri.file(outcome.vPath));
                }
                return;
            }
            const action = await vscode.window.showErrorMessage(`Inference: ${outcome.problem}`, 'Open Generated File');
            if (action === 'Open Generated File') {
                await vscode.window.showTextDocument(vscode.Uri.file(outcome.vPath));
            }
            return;
        }
        case 'cancelled':
            log.info(`Prover: proving ${source} was cancelled.`);
            return;
    }
}

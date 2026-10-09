import * as vscode from 'vscode';

import { hasApiKey, resolveConfig, SERVER_URL_SETTING, updateConfiguredContext } from './config';
import {
    cancelJobInteractive,
    compareProof,
    deleteJobInteractive,
    openInPortal,
    resubmitJob,
} from './jobActions';
import { JOB_PANEL_VIEW_TYPE, JobDetailViewManager } from './jobDetailView';
import { JobOrigins, type JobOrigin } from './jobOrigins';
import { FILTER_STATUSES } from './jobList';
import { JobTreeItem, ProofJobsProvider } from './jobsTree';
import { ProofDocuments } from './proofDocuments';
import { registerProverAuthCommands } from './proofAuth';
import { registerProveFileCommand, type CompilerCheck } from './proveFile';
import { registerSubmitProofCommand, type SubmitHooks } from './submitProof';
import type { JobResponse, JobStatus } from './types';

/** What the Configuration view shows about the proof server. */
export interface ProverStatusSource {
    hasApiKey(): Promise<boolean>;
    lastCompilerCheck(): CompilerCheck | undefined;
}

/**
 * Register proof-job support: the Proof Jobs view, the job panel, and the
 * prove/submit/manage commands. Server contract: Inferara/inference-ai-prover
 * `docs/CONTRACTS.md` (mirrored in ./types.ts).
 */
export function registerProver(
    context: vscode.ExtensionContext,
    log: vscode.LogOutputChannel,
    onStatusChanged: () => void,
): { disposable: vscode.Disposable; status: ProverStatusSource } {
    const secrets = context.secrets;
    const disposables: vscode.Disposable[] = [];
    let compilerCheck: CompilerCheck | undefined;

    const jobs = new ProofJobsProvider(secrets, log);
    const view = vscode.window.createTreeView('inference.proofJobsView', { treeDataProvider: jobs });
    const documents = new ProofDocuments(secrets);
    const origins = new JobOrigins(context.workspaceState);
    /** The local files a job was uploaded from, when this workspace knows them. */
    const originOf = async (jobId: string): Promise<JobOrigin | undefined> => {
        const config = await resolveConfig(secrets).catch(() => null);
        return config ? origins.get(config.serverUrl, jobId) : undefined;
    };
    const runAgain = async (job: JobResponse) => {
        const fresh = await resubmitJob(secrets, log, job);
        if (fresh) {
            await showJob(fresh, await originOf(job.id));
        }
    };
    const panels = new JobDetailViewManager(
        context.extensionUri,
        secrets,
        log,
        documents,
        origins,
        () => void jobs.refresh(),
        runAgain,
    );
    disposables.push(
        jobs,
        view,
        documents,
        panels,
        vscode.window.registerWebviewPanelSerializer(JOB_PANEL_VIEW_TYPE, panels),
    );

    disposables.push(view.onDidChangeVisibility((e) => jobs.setVisible(e.visible)));
    if (view.visible) {
        jobs.setVisible(true);
    }

    const showJob: SubmitHooks['showJob'] = async (job, origin) => {
        if (origin) {
            const config = await resolveConfig(secrets).catch(() => null);
            if (config) {
                await origins.set(config.serverUrl, job.id, origin);
            }
        }
        await jobs.refresh();
        const node = jobs.findNode(job.id);
        if (node && view.visible) {
            await view.reveal(node, { select: true, focus: false });
        }
        await panels.open(job, origin);
    };
    const jobOf = (arg: unknown): JobResponse | undefined =>
        arg instanceof JobTreeItem ? arg.job : undefined;
    const command = (id: string, handler: (...args: unknown[]) => unknown) =>
        disposables.push(vscode.commands.registerCommand(id, handler));

    command('inference.refreshProofJobs', () => jobs.refresh());
    command('inference.loadMoreProofJobs', () => jobs.loadMore());
    command('inference.filterProofJobs', async () => {
        const all = 'All statuses';
        const picked = await vscode.window.showQuickPick([all, ...FILTER_STATUSES], {
            title: 'Show proof jobs with status',
            placeHolder: jobs.statusFilter ?? all,
        });
        if (picked === undefined) {
            return;
        }
        const status = picked === all ? undefined : (picked as JobStatus);
        view.description = status ? `Status: ${status}` : undefined;
        await jobs.setFilter(status);
    });
    command('inference.openProofJob', async (arg) => {
        const job = jobOf(arg);
        if (job) {
            await panels.open(job);
        }
    });
    command('inference.cancelProofJob', async (arg) => {
        const job = jobOf(arg);
        if (job && (await cancelJobInteractive(secrets, log, job))) {
            await jobs.refresh();
        }
    });
    command('inference.deleteProofJob', async (arg) => {
        const job = jobOf(arg);
        if (job && (await deleteJobInteractive(secrets, log, job))) {
            await jobs.refresh();
        }
    });
    command('inference.resubmitProofJob', async (arg) => {
        const job = jobOf(arg);
        if (job) {
            await runAgain(job);
        }
    });
    command('inference.copyProofJobId', async (arg) => {
        const job = jobOf(arg);
        if (job) {
            await vscode.env.clipboard.writeText(job.id);
            vscode.window.showInformationMessage(`Copied job ID ${job.id}`);
        }
    });
    command('inference.compareProof', async (arg) => {
        const job = jobOf(arg);
        if (job) {
            await compareProof(secrets, log, documents, job);
        }
    });
    command('inference.openProofJobInPortal', async (arg) => {
        const job = jobOf(arg);
        if (job) {
            await openInPortal(secrets, job);
        }
    });

    // A different server or key means different jobs: drop what is loaded
    // and every open panel before loading again.
    const changed = () => {
        jobs.reset();
        panels.closeAll();
        void jobs.refresh();
        onStatusChanged();
    };
    disposables.push(
        registerSubmitProofCommand(context, log, { showJob }),
        registerProveFileCommand(context, log, { showJob }, (check) => {
            compilerCheck = check;
            onStatusChanged();
        }),
        registerProverAuthCommands(context, log, changed),
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration(SERVER_URL_SETTING)) {
                compilerCheck = undefined;
                void updateConfiguredContext(secrets).then(changed);
            }
        }),
    );

    updateConfiguredContext(secrets).catch((err) => log.error(`Prover: context init failed: ${err}`));

    return {
        disposable: vscode.Disposable.from(...disposables),
        status: {
            hasApiKey: () => hasApiKey(secrets),
            lastCompilerCheck: () => compilerCheck,
        },
    };
}

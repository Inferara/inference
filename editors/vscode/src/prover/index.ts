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
import { FILTER_LABELS, FILTER_STATUSES, hasCertificate, isActiveJob } from './jobList';
import { JobTreeItem, ProofJobsProvider } from './jobsTree';
import { ProverStatus } from './proverStatusBar';
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

    const origins = new JobOrigins(context.workspaceState);
    const jobs = new ProofJobsProvider(secrets, log, origins);
    const view = vscode.window.createTreeView('inference.proofJobsView', { treeDataProvider: jobs, showCollapseAll: false });
    const documents = new ProofDocuments(secrets);
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
    const status = new ProverStatus({
        nameOf: (job) => jobs.displayName(job),
        openJob: async (job) => panels.open(job, await originOf(job.id)),
        runAgain,
        isPanelActive: (jobId) => panels.isActive(jobId),
    });
    disposables.push(
        jobs,
        view,
        documents,
        panels,
        status,
        jobs.onDidLoad((snapshot) => status.update(snapshot)),
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
        if (isActiveJob(job)) {
            jobs.noteSubmitted();
            status.watch(job);
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
    const setFilter = async (filter: JobStatus | undefined) => {
        view.description = filter ? `Showing: ${FILTER_LABELS[filter]}` : undefined;
        await jobs.setFilter(filter);
    };
    command('inference.filterProofJobs', async () => {
        type Item = vscode.QuickPickItem & { status?: JobStatus };
        const option = (s: JobStatus): Item => ({
            label: FILTER_LABELS[s],
            description: s,
            status: s,
            picked: jobs.statusFilter === s,
        });
        const items: Item[] = [
            { label: 'All jobs', description: jobs.statusFilter ? undefined : 'current' },
            { label: 'In progress', kind: vscode.QuickPickItemKind.Separator },
            ...FILTER_STATUSES.filter((s) => isActiveJob({ status: s }) && s !== 'Lost').map(option),
            { label: 'Finished', kind: vscode.QuickPickItemKind.Separator },
            ...FILTER_STATUSES.filter((s) => !isActiveJob({ status: s }) || s === 'Lost').map(option),
        ];
        const picked = await vscode.window.showQuickPick(items, {
            title: 'Show proof jobs',
            placeHolder: jobs.statusFilter ? `Showing: ${FILTER_LABELS[jobs.statusFilter]}` : 'Showing all jobs',
            matchOnDescription: true,
        });
        if (picked) {
            await setFilter(picked.status);
        }
    });
    command('inference.clearProofJobsFilter', () => setFilter(undefined));
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
            await openInPortal(secrets, job, hasCertificate(job) ? 'certificate' : 'job');
        }
    });

    // A different server or key means different jobs: drop what is loaded
    // and every open panel before loading again.
    const changed = () => {
        status.reset();
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

import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { describe, it } from 'node:test';

import { FILTER_STATUSES, jobContextValue } from '../prover/jobList';

const pkg = JSON.parse(fs.readFileSync(path.resolve(__dirname, '..', '..', 'package.json'), 'utf-8'));
const menus = pkg.contributes.menus;

/** Evaluate the `viewItem =~ /re/` part of a when-clause against a context value. */
function matchesViewItem(when: string, contextValue: string): boolean {
    const m = when.match(/viewItem =~ \/(.+?)\//);
    return m ? new RegExp(m[1]).test(contextValue) : false;
}

const itemMenu = (command: string) =>
    menus['view/item/context'].filter((e: { command: string }) => e.command === command);

describe('proof jobs manifest wiring', () => {
    it('adds the Proof Jobs view to the Inference container with a welcome', () => {
        assert.ok(pkg.contributes.views.inference.some((v: { id: string }) => v.id === 'inference.proofJobsView'));
        assert.ok(pkg.contributes.viewsWelcome.some(
            (w: { view: string; contents: string }) =>
                w.view === 'inference.proofJobsView' && w.contents.includes('command:inference.setProverApiKey')));
        assert.ok(pkg.activationEvents.includes('onView:inference.proofJobsView'));
    });

    it('offers Prove on .inf and Submit on .v in the editor title, editor and explorer menus', () => {
        for (const menu of ['editor/title', 'editor/context', 'explorer/context']) {
            const entries = menus[menu];
            assert.ok(entries.some((e: { command: string; when: string }) =>
                e.command === 'inference.proveFile' && e.when === 'resourceExtname == .inf'), menu);
            assert.ok(entries.some((e: { command: string; when: string }) =>
                e.command === 'inference.submitProof' && e.when === 'resourceExtname == .v'), menu);
        }
    });

    it('shows each job action exactly for the statuses the server accepts', () => {
        const allowed: Record<string, (s: string) => boolean> = {
            'inference.cancelProofJob': (s) => ['Accepted', 'Queued', 'Provisioning', 'Booting', 'Running'].includes(s),
            'inference.deleteProofJob': (s) => !['Accepted', 'Queued', 'Provisioning', 'Booting', 'Running', 'Verifying', 'Canceling'].includes(s),
            'inference.resubmitProofJob': (s) => !['Accepted', 'Queued', 'Provisioning', 'Booting', 'Running', 'Verifying', 'Canceling'].includes(s),
            'inference.compareProof': (s) => s === 'Succeeded' || s === 'PartialSuccess',
        };
        for (const [command, expected] of Object.entries(allowed)) {
            const entries = itemMenu(command);
            assert.ok(entries.length > 0, command);
            for (const status of FILTER_STATUSES) {
                const shown = entries.some((e: { when: string }) => matchesViewItem(e.when, jobContextValue({ status })));
                assert.strictEqual(shown, expected(status), `${command} for ${status}`);
            }
        }
    });

    it('hides commands that need a job argument from the palette', () => {
        const hidden = menus.commandPalette
            .filter((e: { when: string }) => e.when === 'false')
            .map((e: { command: string }) => e.command);
        for (const id of ['inference.openProofJob', 'inference.cancelProofJob', 'inference.deleteProofJob',
            'inference.resubmitProofJob', 'inference.copyProofJobId', 'inference.compareProof',
            'inference.openProofJobInPortal']) {
            assert.ok(hidden.includes(id), id);
        }
        assert.ok(!hidden.includes('inference.proveFile'));
    });

    it('registers every contributed prover command id in the source', () => {
        const source = fs.readdirSync(path.resolve(__dirname, '..', 'prover'))
            .map((f) => fs.readFileSync(path.resolve(__dirname, '..', 'prover', f), 'utf-8'))
            .join('\n');
        const ids = pkg.contributes.commands
            .map((c: { command: string }) => c.command)
            .filter((id: string) => /Proof|Prover|proveFile|submitProof|compareProof/.test(id));
        assert.strictEqual(ids.length, 13);
        for (const id of ids) {
            assert.ok(source.includes(`'${id}'`), id);
        }
    });
});

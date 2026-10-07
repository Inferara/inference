import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    compareIdentity,
    installableVersion,
    parseAbiVersion,
    parseCommitHash,
    parseInfcVersion,
    resolvedInfcPath,
} from '../prover/toolchainIdentity';
import type { AcceptedToolchain } from '../prover/types';

const ACCEPTED: AcceptedToolchain = {
    repository: 'Inferara/inference',
    commit: 'd71a9c3eb78d1ba1535098a7ea0d5d0e8f324ece',
    version: 'infc 0.0.1',
    abiVersion: '1.3',
    releaseTag: null,
};

describe('infc identity parsing', () => {
    it('reads a short or full commit hash and rejects "unknown"', () => {
        assert.strictEqual(parseCommitHash('d71a9c3e\n'), 'd71a9c3e');
        assert.strictEqual(parseCommitHash('D71A9C3EB78D1BA1535098A7EA0D5D0E8F324ECE'), ACCEPTED.commit);
        assert.strictEqual(parseCommitHash('unknown\n'), null);
        assert.strictEqual(parseCommitHash(''), null);
    });

    it('reads the version and ABI lines exactly', () => {
        assert.strictEqual(parseInfcVersion('infc 0.0.6\n'), 'infc 0.0.6');
        assert.strictEqual(parseInfcVersion('0.0.6'), null);
        assert.strictEqual(parseAbiVersion('1.10\n'), '1.10');
        assert.strictEqual(parseAbiVersion('v1'), null);
    });

    it('takes the compiler path only from an OK "Resolved infc" doctor line', () => {
        const doctor = (status: 'ok' | 'warn' | 'fail', message: string) => ({
            checks: [{ name: 'Resolved infc', status, message }],
            hasErrors: status === 'fail',
            hasWarnings: status === 'warn',
            summary: '',
        });
        assert.strictEqual(
            resolvedInfcPath(doctor('ok', '/home/u/.inference/bin/infc (source: sibling of infs)')),
            '/home/u/.inference/bin/infc',
        );
        assert.strictEqual(resolvedInfcPath(doctor('fail', '/x/infc (source: PATH) is not executable')), null);
        assert.strictEqual(resolvedInfcPath(doctor('warn', 'infc not found')), null);
        assert.strictEqual(resolvedInfcPath(null), null);
    });
});

describe('compareIdentity', () => {
    it('matches a short hash prefix with the same ABI', () => {
        assert.deepStrictEqual(
            compareIdentity({ commit: 'd71a9c3e', version: 'infc 0.0.1', abiVersion: '1.3' }, ACCEPTED),
            { kind: 'match' },
        );
    });

    it('reports every reason a release compiler is refused', () => {
        const verdict = compareIdentity(
            { commit: '72b50dc', version: 'infc 0.0.6', abiVersion: '1.8' },
            ACCEPTED,
        );
        assert.strictEqual(verdict.kind, 'mismatch');
        assert.strictEqual(verdict.kind === 'mismatch' && verdict.reasons.length, 2);
    });

    it('refuses an unidentifiable or too-short commit', () => {
        for (const commit of [null, 'd71a9c']) {
            const verdict = compareIdentity({ commit, version: null, abiVersion: '1.3' }, ACCEPTED);
            assert.strictEqual(verdict.kind, 'mismatch', String(commit));
        }
    });

    it('is unchecked when the server publishes no accepted compiler', () => {
        assert.deepStrictEqual(
            compareIdentity({ commit: 'd71a9c3e', version: null, abiVersion: '1.3' }, undefined),
            { kind: 'unchecked' },
        );
    });
});

describe('installableVersion', () => {
    it('offers a release tag without its v prefix, never a branch commit', () => {
        assert.strictEqual(installableVersion({ ...ACCEPTED, releaseTag: 'v0.0.6' }), '0.0.6');
        assert.strictEqual(installableVersion(ACCEPTED), null);
        assert.strictEqual(installableVersion({ ...ACCEPTED, releaseTag: 'refs/heads/main' }), null);
    });
});

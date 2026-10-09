import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    countHoles,
    generatedVPath,
    HOLE_MARKER,
    idempotencyKey,
    preflight,
    submitVFile,
} from '../prover/submission';
import type { JobResponse, SubmitJobOptions } from '../prover/types';

const V = `Theorem t : True.\nProof. ${HOLE_MARKER} Qed.\n`;
const bytes = (s: string) => new TextEncoder().encode(s);

describe('preflight', () => {
    it('counts holes and accepts a valid file', () => {
        assert.strictEqual(countHoles(`${V}${V}`), 2);
        const checked = preflight('a.v', bytes(V));
        assert.ok(checked.ok && checked.holes === 1);
    });

    it('mirrors the server rejections', () => {
        const cases: Array<[string, Uint8Array, number | undefined, RegExp]> = [
            ['a.inf', bytes(V), undefined, /not a Rocq \.v file/],
            ['a.v', bytes('Theorem t : True. Proof. trivial. Qed.'), undefined, /no proof holes/],
            ['a.v', new Uint8Array([0xff, 0xfe]), undefined, /not valid UTF-8/],
            ['a.v', bytes(`${V}\u0000`), undefined, /NUL/],
            ['a.v', bytes(V), 10, /accepts at most 10/],
        ];
        for (const [name, content, max, problem] of cases) {
            const checked = preflight(name, content, max);
            assert.ok(!checked.ok && problem.test(checked.problem), `${name}: ${problem}`);
        }
    });
});

describe('idempotencyKey', () => {
    it('equals the portal key for the same content (sha256 of options + content, UUIDv4-shaped)', () => {
        // Reference value computed independently: sha256('{} ' + V), version/variant bits set.
        assert.strictEqual(idempotencyKey(V), 'ebf54dda-1601-4f13-9d82-387cbc3d7a13');
    });

    it('changes with the content or the options', () => {
        assert.notStrictEqual(idempotencyKey(V), idempotencyKey(`${V} `));
        assert.notStrictEqual(idempotencyKey(V), idempotencyKey(V, { maxWallClockSeconds: 60 }));
    });
});

describe('generatedVPath', () => {
    it('resolves the reported path against the build directory; the last report wins', () => {
        const stdout = 'WASM generated at: out/m.wasm\nV generated at: out/old.v\nV generated at: out/m.v\n';
        assert.strictEqual(generatedVPath(stdout, '/w/src'), '/w/src/out/m.v');
        assert.strictEqual(generatedVPath('V generated at: /abs/m.v', '/w'), '/abs/m.v');
        assert.strictEqual(generatedVPath('WASM generated at: out/m.wasm', '/w'), null);
    });
});

describe('submitVFile', () => {
    it('submits with the content key and no options; never submits a doomed file', async () => {
        const calls: Array<{ filename: string; options: SubmitJobOptions; key?: string }> = [];
        const api = {
            submitJobWithMeta: async (filename: string, _c: Uint8Array, options: SubmitJobOptions = {}, key?: string) => {
                calls.push({ filename, options, key });
                return { job: { id: 'job-1', status: 'Accepted' } as JobResponse, replayHeader: null };
            },
        };
        const ok = await submitVFile(api, 'm.v', bytes(V));
        assert.strictEqual(ok.kind, 'submitted');
        assert.ok(ok.kind === 'submitted' && !ok.replayed);
        assert.deepStrictEqual(calls, [{ filename: 'm.v', options: {}, key: idempotencyKey(V) }]);

        const bad = await submitVFile(api, 'm.v', bytes('no holes'));
        assert.strictEqual(bad.kind, 'preflight-failed');
        assert.strictEqual(calls.length, 1);
    });
});

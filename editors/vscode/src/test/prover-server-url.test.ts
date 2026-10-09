import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { checkServerUrl, DEFAULT_SERVER_URL, portalJobUrl } from '../prover/serverUrl';

describe('checkServerUrl', () => {
    it('uses the hosted service when empty', () => {
        assert.deepStrictEqual(checkServerUrl(''), { ok: true, url: DEFAULT_SERVER_URL });
        assert.deepStrictEqual(checkServerUrl(undefined), { ok: true, url: DEFAULT_SERVER_URL });
    });

    it('accepts https anywhere and http only on loopback', () => {
        assert.deepStrictEqual(checkServerUrl(' https://prover.example.com/ '), {
            ok: true,
            url: 'https://prover.example.com',
        });
        for (const local of ['http://localhost:8088', 'http://127.0.0.1:8088', 'http://[::1]:8088']) {
            assert.ok(checkServerUrl(local).ok, local);
        }
        assert.ok(!checkServerUrl('http://prover.example.com').ok);
        assert.ok(!checkServerUrl('http://192.168.1.10:8088').ok);
    });

    it('rejects credentials, queries, other schemes and junk', () => {
        for (const bad of [
            'https://user:pw@prover.example.com',
            'https://prover.example.com/?k=v',
            'ftp://prover.example.com',
            'not a url',
        ]) {
            assert.ok(!checkServerUrl(bad).ok, bad);
        }
    });
});

describe('portalJobUrl', () => {
    it('links the job page and its certificate tab', () => {
        assert.strictEqual(
            portalJobUrl('https://p.example.com/', 'abc'),
            'https://p.example.com/#/job/abc',
        );
        assert.strictEqual(
            portalJobUrl('http://localhost:8088', 'abc', 'certificate'),
            'http://localhost:8088/#/job/abc/certificate',
        );
    });
});

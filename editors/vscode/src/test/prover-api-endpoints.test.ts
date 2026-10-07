import * as assert from 'node:assert';
import * as http from 'node:http';
import { after, before, describe, it } from 'node:test';

import { ApiError, MAX_ARTIFACT_BYTES, ProverApi } from '../prover/api';

/** Endpoints added for the Inference extension: meta, me, paging, delete, artifacts. */
describe('ProverApi endpoints', () => {
    let server: http.Server;
    let api: ProverApi;
    const seen: Array<{ method?: string; url?: string }> = [];
    let artifactBytes = Buffer.from('proof');

    before(async () => {
        server = http.createServer((req, res) => {
            seen.push({ method: req.method, url: req.url });
            const json = (status: number, body: unknown) => {
                res.statusCode = status;
                res.setHeader('Content-Type', 'application/json');
                res.end(JSON.stringify(body));
            };
            const url = req.url ?? '';
            if (url === '/api/v1/meta') {
                json(200, { schemaVersion: 1, maxUploadBytes: 4194304, defaultMaxWallClockSeconds: 1800,
                    maxWallClockSecondsCap: 7200, acceptedToolchain: { repository: 'Inferara/inference',
                        commit: 'd71a9c3eb78d1ba1535098a7ea0d5d0e8f324ece', version: 'infc 0.0.1', abiVersion: '1.3',
                        releaseTag: null } });
            } else if (url === '/api/v1/me') {
                json(200, { schemaVersion: 1, userId: 'u1', role: 'User', keyId: 'k1' });
            } else if (url.startsWith('/api/v1/jobs?') || url === '/api/v1/jobs') {
                json(200, { jobs: [{ id: 'j1', status: 'Running' }] });
            } else if (req.method === 'DELETE' && url === '/api/v1/jobs/done') {
                res.statusCode = 204;
                res.end();
            } else if (req.method === 'DELETE' && url === '/api/v1/jobs/live') {
                json(409, { code: 'NOT_TERMINAL', detail: 'job is Running' });
            } else if (url === '/api/v1/jobs/j1/artifacts') {
                json(200, { artifacts: [{ id: 'a1', kind: 'CompletedV', filename: 'm.v', sizeBytes: 5, sha256: 'x' }] });
            } else if (url === '/api/v1/jobs/j1/artifacts/a1') {
                res.statusCode = 200;
                res.end(artifactBytes);
            } else {
                json(404, {});
            }
        });
        await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
        api = new ProverApi(`http://127.0.0.1:${(server.address() as { port: number }).port}`, 'k');
    });

    after(() => server.close());

    it('reads meta with the accepted toolchain, and the key owner', async () => {
        assert.strictEqual((await api.getMeta()).acceptedToolchain?.abiVersion, '1.3');
        assert.strictEqual((await api.getMe()).role, 'User');
    });

    it('lists with status, limit and keyset cursor', async () => {
        await api.listJobs();
        await api.listJobs({ status: 'Running', limit: 50, before: '2026-10-07T00:00:00+00:00' });
        const urls = seen.filter((s) => s.url?.startsWith('/api/v1/jobs')).map((s) => s.url);
        assert.deepStrictEqual(urls.slice(-2), [
            '/api/v1/jobs',
            '/api/v1/jobs?status=Running&limit=50&before=2026-10-07T00%3A00%3A00%2B00%3A00',
        ]);
    });

    it('deletes a finished job and surfaces NOT_TERMINAL', async () => {
        await api.deleteJob('done');
        await assert.rejects(api.deleteJob('live'), (err: unknown) =>
            err instanceof ApiError && err.status === 409 && err.code === 'NOT_TERMINAL');
    });

    it('never carries an upload to another origin on a redirect', async () => {
        let foreignRequests = 0;
        const foreign = http.createServer((_req, res) => {
            foreignRequests++;
            res.end('{}');
        });
        await new Promise<void>((resolve) => foreign.listen(0, '127.0.0.1', resolve));
        const foreignUrl = `http://localhost:${(foreign.address() as { port: number }).port}`;
        const redirecting = http.createServer((_req, res) => {
            res.statusCode = 307;
            res.setHeader('Location', `${foreignUrl}/api/v1/jobs`);
            res.end();
        });
        await new Promise<void>((resolve) => redirecting.listen(0, '127.0.0.1', resolve));
        try {
            const client = new ProverApi(`http://127.0.0.1:${(redirecting.address() as { port: number }).port}`, 'k');
            await assert.rejects(
                client.submitJob('m.v', Buffer.from('secret program'), {}, 'key'),
                /Refusing to send the request body to another origin/,
            );
            assert.strictEqual(foreignRequests, 0);
        } finally {
            redirecting.close();
            foreign.close();
        }
    });

    it('lists artifacts and downloads beyond the JSON cap', async () => {
        assert.strictEqual((await api.listArtifacts('j1'))[0].kind, 'CompletedV');
        artifactBytes = Buffer.alloc(11 * 1024 * 1024, 0x61); // > 10 MiB JSON cap
        assert.ok(MAX_ARTIFACT_BYTES > artifactBytes.length);
        assert.strictEqual((await api.downloadArtifact('j1', 'a1')).length, artifactBytes.length);
    });
});

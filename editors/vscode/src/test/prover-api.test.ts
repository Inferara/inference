import * as assert from 'node:assert';
import * as http from 'node:http';
import { describe, it, before, after } from 'node:test';

import { ProverApi, AuthError, ApiError, buildUrl } from '../prover/api';
import type { ArtifactBindingResponse, JobResponse, WorkerImageManifest } from '../prover/types';

const artifactBinding: ArtifactBindingResponse = {
    schemaVersion: 1, kind: 'canonical-fixture-runtime-body-identity',
    fixtureId: 'aerospace-telemetry-limit', catalogId: 'canonical-aerospace-v1', catalogSha256: 'a'.repeat(64),
    proof: { inputArtifactId: 'input-id', inputSha256: 'b'.repeat(64), completedArtifactId: 'completed-id',
        completedSha256: 'c'.repeat(64), verifiedAtUtc: '2026-10-03T00:00:00Z', assumptionPolicyId: 'pinned-policy' },
    comparison: { profile: 'state-independent-i32-v1', compilerCommit: 'd'.repeat(40), sourceSha256: 'e'.repeat(64),
        createdAtUtc: '2026-10-04T00:00:00Z', runtimeFunctions: 1, specFunctions: 1, proofOnlyMemories: 0, proofOnlyGlobals: 0 },
    proofWasm: { asset: 'proof.wasm', filename: 'demo.proof.wasm', sizeBytes: 815, sha256: '1'.repeat(64) },
    runtimeWasm: { asset: 'runtime.wasm', filename: 'demo.runtime.wasm', sizeBytes: 277, sha256: '2'.repeat(64) },
    report: { asset: 'comparison.json', filename: 'comparison-report.json', sizeBytes: 2963, sha256: '3'.repeat(64) },
    formalEquivalenceVerified: false, comparisonReexecutedForJob: false, deploymentVerified: false,
};

const workerImageManifest: WorkerImageManifest = {
    coq: '8.20.1',
    libraryCommit: '0bbf9b723da5478c9df1b3a4f0d787e35e37f6b9',
    coqWasm: '2.2.0',
    coqWasmCommit: '0fd83fa708922721132b6d6737179568d1f1d553',
    infcCommit: '0123456789abcdef0123456789abcdef01234567',
    harness: '0.1.0',
    agentProtocolVersion: 2,
    agentProtocolPolicy: 'agents-v2',
    agentProtocolSha256: 'a'.repeat(64),
    agentCatalogSha256: 'b'.repeat(64),
    agentToolchainSha256: 'c'.repeat(64),
    claudeCode: '2.0.0',
    codexCli: '1.0.0',
    node: '24.8.0',
};

describe('buildUrl', () => {
    it('joins base and path with a single slash', () => {
        assert.strictEqual(
            buildUrl('https://prover.example.com', '/api/v1/jobs'),
            'https://prover.example.com/api/v1/jobs',
        );
    });

    it('tolerates a trailing slash on the base', () => {
        assert.strictEqual(
            buildUrl('https://prover.example.com/', '/api/v1/jobs'),
            'https://prover.example.com/api/v1/jobs',
        );
    });

    it('tolerates a missing leading slash on the path', () => {
        assert.strictEqual(
            buildUrl('https://prover.example.com', 'api/v1/jobs'),
            'https://prover.example.com/api/v1/jobs',
        );
    });

    it('collapses multiple trailing slashes on the base', () => {
        assert.strictEqual(
            buildUrl('http://localhost:8080///', '/api/v1/jobs/abc'),
            'http://localhost:8080/api/v1/jobs/abc',
        );
    });
});

describe('ProverApi', () => {
    let server: http.Server;
    let baseUrl: string;
    /** Captured Authorization header from the last request. */
    let lastAuth: string | undefined;
    /** Captured headers + parsed JSON body of the last POST /api/v1/jobs. */
    let lastSubmit:
        | { idempotencyKey?: string; body: Record<string, unknown> }
        | undefined;

    const jobs: JobResponse[] = [
        {
            id: 'job-1',
            status: 'Running',
            filename: 'main.v',
            mode: 'prove',
            claimClass: 'Unverified',
            holesTotal: 3,
            holesClosed: 1,
        },
    ];

    /** Deterministic binary artifact (every byte value once). */
    const artifactBytes = Buffer.from(
        Array.from({ length: 256 }, (_, i) => i),
    );

    before(async () => {
        server = http.createServer((req, res) => {
            lastAuth = req.headers['authorization'];
            const url = req.url ?? '';
            if (url === '/api/v1/jobs' && req.method === 'POST') {
                const chunks: Buffer[] = [];
                req.on('data', (c: Buffer) => chunks.push(c));
                req.on('end', () => {
                    const body = JSON.parse(
                        Buffer.concat(chunks).toString('utf-8'),
                    ) as Record<string, unknown>;
                    lastSubmit = {
                        idempotencyKey: req.headers['idempotency-key'] as
                            | string
                            | undefined,
                        body,
                    };
                    res.writeHead(202, {
                        'Content-Type': 'application/json',
                        Location: '/api/v1/jobs/job-new',
                    });
                    res.end(
                        JSON.stringify({
                            id: 'job-new',
                            status: 'Queued',
                            filename: body.filename,
                            holesTotal: 2,
                        }),
                    );
                });
            } else if (url === '/api/v1/jobs/job-1/cancel') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ status: 'Canceling' }));
            } else if (url === '/api/v1/jobs/job-done/cancel') {
                res.writeHead(409, {
                    'Content-Type': 'application/problem+json',
                });
                res.end(
                    JSON.stringify({
                        title: 'Conflict',
                        status: 409,
                        detail: 'job is Succeeded',
                        code: 'NOT_CANCELABLE',
                    }),
                );
            } else if (url === '/api/v1/jobs/job-bound/result') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ id: 'job-bound', status: 'Succeeded', artifacts: [], artifactBinding }));
            } else if (url === '/api/v1/jobs/job-1/result') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(
                    JSON.stringify({
                        id: 'job-1',
                        status: 'Succeeded',
                        mode: 'prove',
                        claimClass: 'Verified',
                        verificationOutcome: 'succeeded',
                        verifiedAt: '2026-06-05T10:02:00Z',
                        compileOk: true,
                        assumptionsClosed: true,
                        verifiedClean: true,
                        admitsDetected: false,
                        statementsImmutable: true,
                        markersRemaining: 0,
                        holesTotal: 3,
                        holesClosed: 3,
                        assumptionPolicyId: 'wasm-verifier@test/assumptions-v1',
                        assumptionReports: ['valid_1', 'valid_2', 'valid_3'].map(
                            (target) => ({
                                target,
                                assumptions: [],
                                kernelOutput: 'Closed under the global context',
                                policyPassed: true,
                            }),
                        ),
                        artifacts: [
                            {
                                id: 'art-1',
                                kind: 'CompletedV',
                                filename: 'completed.v',
                                sizeBytes: artifactBytes.length,
                                sha256: 'irrelevant-here',
                            },
                        ],
                        imageManifestJson: JSON.stringify(workerImageManifest),
                    }),
                );
            } else if (url === '/api/v1/jobs/job-1/events?after=5&wait=0') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(
                    JSON.stringify({
                        events: [
                            {
                                schemaVersion: 1,
                                jobId: 'job-1',
                                seq: 6,
                                type: 'log',
                                ts: '2026-06-06T10:00:00Z',
                                payload: { message: 'hi' },
                            },
                        ],
                    }),
                );
            } else if (url === '/api/v1/jobs/job-1/artifacts/art-1') {
                res.writeHead(200, {
                    'Content-Type': 'application/octet-stream',
                });
                res.end(artifactBytes);
            } else if (url === '/api/v1/jobs') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ jobs }));
            } else if (url === '/api/v1/jobs-array') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify(jobs));
            } else if (url === '/api/v1/jobs/job-1') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify(jobs[0]));
            } else if (url === '/api/v1/unauthorized') {
                res.writeHead(401);
                res.end('Unauthorized');
            } else if (url === '/api/v1/forbidden') {
                res.writeHead(403);
                res.end('Forbidden');
            } else if (url === '/api/v1/boom') {
                res.writeHead(500);
                res.end('Internal Server Error');
            } else if (url === '/api/v1/unavailable') {
                // The server's RFC 9457 fail-fast rejection (problem+json with
                // a machine-readable `code` extension), as emitted since
                // prover PR #14 for PROVIDER_UNAVAILABLE / VERIFIER_UNAVAILABLE.
                res.writeHead(503, {
                    'Content-Type': 'application/problem+json',
                });
                res.end(
                    JSON.stringify({
                        title: 'Service Unavailable',
                        status: 503,
                        detail: "compute provider 'docker' is not configured on this deployment (available: kubernetes, ec2)",
                        code: 'PROVIDER_UNAVAILABLE',
                    }),
                );
            } else {
                res.writeHead(404);
                res.end('Not Found');
            }
        });

        await new Promise<void>((resolve) => {
            server.listen(0, '127.0.0.1', () => {
                const addr = server.address();
                if (addr && typeof addr === 'object') {
                    baseUrl = `http://127.0.0.1:${addr.port}`;
                }
                resolve();
            });
        });
    });

    after(async () => {
        await new Promise<void>((resolve) => server.close(() => resolve()));
    });

    it('listJobs returns the jobs array from an envelope', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const result = await api.listJobs();
        assert.strictEqual(result.length, 1);
        assert.strictEqual(result[0].id, 'job-1');
        assert.strictEqual(result[0].filename, 'main.v');
    });

    it('sends the Bearer authorization header', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        await api.listJobs();
        assert.strictEqual(lastAuth, 'Bearer infp_test_secret');
    });

    it('getJob fetches a single job by id', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const job = await api.getJob('job-1');
        assert.strictEqual(job.status, 'Running');
        assert.strictEqual(job.mode, 'prove');
        assert.strictEqual(job.claimClass, 'Unverified');
        assert.strictEqual(job.holesClosed, 1);
    });

    it('maps 401 to AuthError', async () => {
        const api = new ProverApi(baseUrl, 'k');
        await assert.rejects(
            (api as unknown as { get(p: string): Promise<unknown> }).get(
                '/api/v1/unauthorized',
            ),
            (err: unknown) => {
                assert.ok(err instanceof AuthError);
                assert.strictEqual((err as AuthError).status, 401);
                return true;
            },
        );
    });

    it('maps 403 to AuthError', async () => {
        const api = new ProverApi(baseUrl, 'k');
        await assert.rejects(
            (api as unknown as { get(p: string): Promise<unknown> }).get(
                '/api/v1/forbidden',
            ),
            (err: unknown) => {
                assert.ok(err instanceof AuthError);
                assert.strictEqual((err as AuthError).status, 403);
                return true;
            },
        );
    });

    it('surfaces problem+json code and detail on ApiError', async () => {
        const api = new ProverApi(baseUrl, 'k');
        await assert.rejects(
            (api as unknown as { get(p: string): Promise<unknown> }).get(
                '/api/v1/unavailable',
            ),
            (err: unknown) => {
                assert.ok(err instanceof ApiError);
                const e = err as ApiError;
                assert.strictEqual(e.status, 503);
                assert.strictEqual(e.code, 'PROVIDER_UNAVAILABLE');
                assert.ok(e.detail?.includes('available: kubernetes, ec2'));
                // The message leads with the actionable detail, not the URL.
                assert.ok(e.message.startsWith("compute provider 'docker'"));
                assert.ok(e.message.endsWith('(PROVIDER_UNAVAILABLE)'));
                return true;
            },
        );
    });

    it('maps 500 to ApiError', async () => {
        const api = new ProverApi(baseUrl, 'k');
        await assert.rejects(
            (api as unknown as { get(p: string): Promise<unknown> }).get(
                '/api/v1/boom',
            ),
            (err: unknown) => {
                assert.ok(err instanceof ApiError);
                const e = err as ApiError;
                assert.strictEqual(e.status, 500);
                // Non-JSON body: generic message, no code/detail extracted.
                assert.strictEqual(e.code, undefined);
                assert.strictEqual(e.detail, undefined);
                assert.ok(e.message.startsWith('HTTP 500 from '));
                return true;
            },
        );
    });

    it('submitJob sends the documented JSON shape with an Idempotency-Key', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const content = Buffer.from('Theorem t : True. (* TODO *)', 'utf-8');
        const job = await api.submitJob(
            'main.v',
            content,
            {
                maxWallClockSeconds: 900,
                expectedAgentSettingsRevision: 7,
            },
            '0198b001-0000-7000-8000-000000000001',
        );
        assert.strictEqual(job.id, 'job-new');
        assert.strictEqual(job.status, 'Queued');
        assert.ok(lastSubmit);
        assert.strictEqual(
            lastSubmit.idempotencyKey,
            '0198b001-0000-7000-8000-000000000001',
        );
        // Wire shape per docs/CONTRACTS.md "Submit (JSON form)".
        assert.strictEqual(lastSubmit.body.schemaVersion, 1);
        assert.strictEqual(lastSubmit.body.filename, 'main.v');
        assert.strictEqual(
            Buffer.from(
                lastSubmit.body.contentBase64 as string,
                'base64',
            ).toString('utf-8'),
            'Theorem t : True. (* TODO *)',
        );
        assert.deepStrictEqual(lastSubmit.body.options, {
            maxWallClockSeconds: 900,
                expectedAgentSettingsRevision: 7,
        });
    });

    it('cancelJob returns the new status', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const res = await api.cancelJob('job-1');
        assert.strictEqual(res.status, 'Canceling');
    });

    it('cancelJob surfaces NOT_CANCELABLE from problem+json', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        await assert.rejects(api.cancelJob('job-done'), (err: unknown) => {
            assert.ok(err instanceof ApiError);
            assert.strictEqual((err as ApiError).status, 409);
            assert.strictEqual((err as ApiError).code, 'NOT_CANCELABLE');
            return true;
        });
    });

    it('getResult returns the verdict and artifact list', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const result = await api.getResult('job-1');
        assert.strictEqual(result.mode, 'prove');
        assert.strictEqual(result.claimClass, 'Verified');
        assert.strictEqual(result.verificationOutcome, 'succeeded');
        assert.strictEqual(result.compileOk, true);
        assert.strictEqual(result.assumptionsClosed, true);
        assert.strictEqual(result.verifiedClean, true);
        assert.strictEqual(result.markersRemaining, 0);
        assert.strictEqual(result.assumptionPolicyId, 'wasm-verifier@test/assumptions-v1');
        assert.strictEqual(result.assumptionReports?.length, 3);
        assert.strictEqual(result.artifacts.length, 1);
        assert.strictEqual(result.artifacts[0].filename, 'completed.v');
        assert.strictEqual(result.artifactBinding, undefined);
        assert.deepStrictEqual(
            JSON.parse(result.imageManifestJson ?? '{}'),
            workerImageManifest,
        );
    });

    it('preserves the additive canonical binding without changing legacy result handling', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        assert.deepStrictEqual((await api.getResult('job-bound')).artifactBinding, artifactBinding);
    });

    it('getEvents passes the cursor and parses the envelope batch', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const res = await api.getEvents('job-1', 5);
        assert.strictEqual(res.events.length, 1);
        assert.strictEqual(res.events[0].seq, 6);
        assert.strictEqual(res.events[0].type, 'log');
    });

    it('downloadArtifact returns the exact bytes', async () => {
        const api = new ProverApi(baseUrl, 'infp_test_secret');
        const content = await api.downloadArtifact('job-1', 'art-1');
        assert.ok(content.equals(artifactBytes));
    });
});

describe('redirect credential handling', () => {
    it('drops Authorization on a cross-origin redirect, keeps it same-origin', async () => {
        /** Auth headers observed at the foreign-origin target. */
        let foreignAuth: string | undefined | null = null;
        let sameOriginAuth: string | undefined | null = null;

        const foreign = http.createServer((req, res) => {
            foreignAuth = req.headers['authorization'];
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ id: 'x', status: 'Queued' }));
        });
        await new Promise<void>((r) => foreign.listen(0, '127.0.0.1', r));
        const foreignAddr = foreign.address();
        const foreignPort =
            foreignAddr && typeof foreignAddr === 'object'
                ? foreignAddr.port
                : 0;

        const origin = http.createServer((req, res) => {
            if (req.url === '/api/v1/jobs/cross') {
                res.writeHead(302, {
                    Location: `http://127.0.0.1:${foreignPort}/api/v1/jobs/cross`,
                });
                res.end();
            } else if (req.url === '/api/v1/jobs/hop') {
                // Same-origin redirect: credentials must survive.
                res.writeHead(302, { Location: '/api/v1/jobs/final' });
                res.end();
            } else if (req.url === '/api/v1/jobs/final') {
                sameOriginAuth = req.headers['authorization'];
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ id: 'final', status: 'Queued' }));
            } else {
                res.writeHead(404);
                res.end();
            }
        });
        await new Promise<void>((r) => origin.listen(0, '127.0.0.1', r));
        const originAddr = origin.address();
        const originBase =
            originAddr && typeof originAddr === 'object'
                ? `http://127.0.0.1:${originAddr.port}`
                : '';

        try {
            const api = new ProverApi(originBase, 'infp_secret');
            await api.getJob('cross');
            assert.strictEqual(
                foreignAuth,
                undefined,
                'bearer token must not be replayed cross-origin',
            );
            await api.getJob('hop');
            assert.strictEqual(sameOriginAuth, 'Bearer infp_secret');
        } finally {
            await new Promise<void>((r) => origin.close(() => r()));
            await new Promise<void>((r) => foreign.close(() => r()));
        }
    });
});

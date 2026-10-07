import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as http from 'node:http';
import * as os from 'node:os';
import * as path from 'node:path';
import { after, before, describe, it } from 'node:test';

import { ProverApi } from '../prover/api';
import { proveInfFile, type CommandResult, type ProveDeps } from '../prover/proveFlow';
import { HOLE_MARKER, idempotencyKey } from '../prover/submission';
import type { AcceptedToolchain, JobResponse, MetaResponse, SubmitJobOptions } from '../prover/types';
import { run } from '../utils/spawn';

const ACCEPTED: AcceptedToolchain = {
    repository: 'Inferara/inference',
    commit: 'd71a9c3eb78d1ba1535098a7ea0d5d0e8f324ece',
    version: 'infc 0.0.1',
    abiVersion: '1.3',
    releaseTag: null,
};
const V = `Theorem t : True.\nProof. ${HOLE_MARKER} Qed.\n`;
const META: MetaResponse = {
    schemaVersion: 1,
    maxUploadBytes: 4 * 1024 * 1024,
    defaultMaxWallClockSeconds: 1800,
    maxWallClockSecondsCap: 7200,
    acceptedToolchain: ACCEPTED,
};

interface Call { command: string; args: string[]; cwd?: string; env?: Record<string, string> }

/** Fake deps: infc answers with `identity`, infs build with `build`. */
function fakeDeps(options: {
    identity?: { commit: string; version: string; abi: string };
    build?: CommandResult;
    meta?: MetaResponse;
    infs?: string | null;
    infc?: string | null;
    confirm?: boolean;
    vText?: string;
    cancelAfterBuild?: boolean;
}) {
    const calls: Call[] = [];
    const submits: Array<{ filename: string; options: SubmitJobOptions; key?: string }> = [];
    const identity = options.identity ?? { commit: 'd71a9c3e', version: 'infc 0.0.1', abi: '1.3' };
    let confirmations = 0;
    let built = false;
    const deps: ProveDeps = {
        locateInfs: () => (options.infs === undefined ? '/t/infs' : options.infs),
        resolveInfc: async () => (options.infc === undefined ? '/t/infc' : options.infc),
        run: async (command, args, opts) => {
            calls.push({ command, args, cwd: opts.cwd, env: opts.env });
            if (command === '/t/infc') {
                const out = { '--commit-hash': identity.commit, '--version': identity.version, '--abi-version': identity.abi }[args[0]];
                return { exitCode: 0, stdout: `${out}\n`, stderr: '' };
            }
            built = true;
            return options.build ?? { exitCode: 0, stdout: 'WASM generated at: out/m.wasm\nV generated at: out/m.v\n', stderr: '' };
        },
        readFile: async () => new TextEncoder().encode(options.vText ?? V),
        api: {
            getMeta: async () => options.meta ?? META,
            submitJob: async (filename: string, _content: Uint8Array, opts: SubmitJobOptions = {}, key?: string) => {
                submits.push({ filename, options: opts, key });
                return { id: 'job-1', status: 'Accepted', filename } as JobResponse;
            },
        },
        confirmUnchecked: async () => {
            confirmations++;
            return options.confirm ?? false;
        },
        progress: () => undefined,
        cancelled: () => Boolean(options.cancelAfterBuild) && built,
    };
    return { deps, calls, submits, confirmations: () => confirmations };
}

describe('proveInfFile', () => {
    it('checks the compiler, builds with that exact infc, and submits the generated .v', async () => {
        const fake = fakeDeps({});
        const outcome = await proveInfFile('/w/src/m.inf', fake.deps);
        assert.strictEqual(outcome.kind, 'submitted');
        assert.ok(outcome.kind === 'submitted' && outcome.checked && outcome.vPath === '/w/src/out/m.v');
        const build = fake.calls.find((c) => c.command === '/t/infs');
        assert.deepStrictEqual(build, {
            command: '/t/infs',
            args: ['build', '/w/src/m.inf', '-v'],
            cwd: '/w/src',
            env: { INFC_PATH: '/t/infc' },
        });
        assert.deepStrictEqual(fake.submits, [{ filename: 'm.v', options: {}, key: idempotencyKey(V) }]);
    });

    it('refuses a compiler the server does not accept, before building', async () => {
        const fake = fakeDeps({ identity: { commit: '72b50dc', version: 'infc 0.0.6', abi: '1.8' } });
        const outcome = await proveInfFile('/w/m.inf', fake.deps);
        assert.strictEqual(outcome.kind, 'toolchain-mismatch');
        assert.ok(!fake.calls.some((c) => c.command === '/t/infs'));
        assert.strictEqual(fake.submits.length, 0);
    });

    it('asks before building against a server that publishes no accepted compiler', async () => {
        const meta = { ...META, acceptedToolchain: undefined };
        const declined = fakeDeps({ meta, confirm: false });
        assert.deepStrictEqual(await proveInfFile('/w/m.inf', declined.deps), { kind: 'cancelled' });
        assert.strictEqual(declined.confirmations(), 1);
        assert.strictEqual(declined.submits.length, 0);

        const accepted = fakeDeps({ meta, confirm: true });
        const outcome = await proveInfFile('/w/m.inf', accepted.deps);
        assert.ok(outcome.kind === 'submitted' && !outcome.checked);
    });

    it('treats a cancel during the compiler check as a cancel, not a mismatch', async () => {
        const fake = fakeDeps({});
        const run = fake.deps.run;
        fake.deps.run = async (command, args, options) =>
            command === '/t/infc' ? { exitCode: 1, stdout: '', stderr: '', aborted: true } : run(command, args, options);
        assert.deepStrictEqual(await proveInfFile('/w/m.inf', fake.deps), { kind: 'cancelled' });
        assert.ok(!fake.calls.some((c) => c.command === '/t/infs'));
    });

    it('does not upload when the user cancels after the build', async () => {
        const fake = fakeDeps({ cancelAfterBuild: true });
        assert.deepStrictEqual(await proveInfFile('/w/m.inf', fake.deps), { kind: 'cancelled' });
        assert.ok(fake.calls.some((c) => c.command === '/t/infs'));
        assert.strictEqual(fake.submits.length, 0);
    });

    it('reports a missing toolchain or compiler', async () => {
        assert.deepStrictEqual(await proveInfFile('/w/m.inf', fakeDeps({ infs: null }).deps), { kind: 'no-infs' });
        assert.deepStrictEqual(await proveInfFile('/w/m.inf', fakeDeps({ infc: null }).deps), { kind: 'no-infc' });
    });

    it('stops on a failed, timed-out, cancelled or silent build', async () => {
        const failed = await proveInfFile('/w/m.inf', fakeDeps({ build: { exitCode: 1, stdout: '', stderr: 'error: x' } }).deps);
        assert.ok(failed.kind === 'build-failed' && failed.output.includes('error: x') && !failed.timedOut);
        const timedOut = await proveInfFile('/w/m.inf', fakeDeps({ build: { exitCode: 1, stdout: '', stderr: '', timedOut: true } }).deps);
        assert.ok(timedOut.kind === 'build-failed' && timedOut.timedOut);
        const cancelled = await proveInfFile('/w/m.inf', fakeDeps({ build: { exitCode: 1, stdout: '', stderr: '', aborted: true } }).deps);
        assert.deepStrictEqual(cancelled, { kind: 'cancelled' });
        const silent = await proveInfFile('/w/m.inf', fakeDeps({ build: { exitCode: 0, stdout: 'WASM generated at: out/m.wasm', stderr: '' } }).deps);
        assert.strictEqual(silent.kind, 'no-output');
    });

    it('does not submit a generated file without proof holes', async () => {
        const fake = fakeDeps({ vText: 'Theorem t : True. Proof. trivial. Qed.' });
        const outcome = await proveInfFile('/w/m.inf', fake.deps);
        assert.strictEqual(outcome.kind, 'preflight-failed');
        assert.strictEqual(fake.submits.length, 0);
    });
});

// End to end with real processes and a real HTTP server: fake `infs`/`infc`
// scripts stand in for the toolchain (POSIX shells only).
describe('proveInfFile with real processes and HTTP', { skip: process.platform === 'win32' }, () => {
    let dir: string;
    let server: http.Server;
    let baseUrl: string;
    const received: Array<{ key?: string; filename?: string; content?: string; auth?: string }> = [];

    before(async () => {
        dir = fs.mkdtempSync(path.join(os.tmpdir(), 'inf-prove-'));
        const infc = path.join(dir, 'infc');
        fs.writeFileSync(infc, [
            '#!/bin/sh',
            'case "$1" in',
            '  --commit-hash) echo d71a9c3e ;;',
            '  --version) echo "infc 0.0.1" ;;',
            '  --abi-version) echo 1.3 ;;',
            'esac',
        ].join('\n'), { mode: 0o755 });
        // infs build <file> -v: prove it was pinned to the checked infc, then emit out/<stem>.v.
        const infs = path.join(dir, 'infs');
        fs.writeFileSync(infs, [
            '#!/bin/sh',
            '[ "$1" = build ] && [ "$3" = -v ] || exit 2',
            '[ "$INFC_PATH" = "' + infc + '" ] || { echo "wrong infc: $INFC_PATH" >&2; exit 3; }',
            'stem=$(basename "$2" .inf)',
            'mkdir -p out',
            `printf 'Theorem t : True.\\nProof. ${HOLE_MARKER} Qed.\\n' > "out/$stem.v"`,
            'echo "V generated at: out/$stem.v"',
        ].join('\n'), { mode: 0o755 });
        fs.mkdirSync(path.join(dir, 'src'));
        fs.writeFileSync(path.join(dir, 'src', 'controller.inf'), 'fn main() {}\n');

        server = http.createServer((req, res) => {
            let body = '';
            req.on('data', (c) => (body += c));
            req.on('end', () => {
                res.setHeader('Content-Type', 'application/json');
                if (req.method === 'GET' && req.url === '/api/v1/meta') {
                    res.end(JSON.stringify(META));
                } else if (req.method === 'POST' && req.url === '/api/v1/jobs') {
                    const parsed = JSON.parse(body) as { filename: string; contentBase64: string };
                    received.push({
                        key: req.headers['idempotency-key'] as string | undefined,
                        filename: parsed.filename,
                        content: Buffer.from(parsed.contentBase64, 'base64').toString('utf-8'),
                        auth: req.headers.authorization,
                    });
                    res.statusCode = 202;
                    res.end(JSON.stringify({ id: 'job-e2e', status: 'Accepted', filename: parsed.filename }));
                } else {
                    res.statusCode = 404;
                    res.end('{}');
                }
            });
        });
        await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
        baseUrl = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
    });

    after(() => {
        server.close();
        fs.rmSync(dir, { recursive: true, force: true });
    });

    it('compiles in the source folder and uploads the generated file', async () => {
        const infs = path.join(dir, 'infs');
        const source = path.join(dir, 'src', 'controller.inf');
        const outcome = await proveInfFile(source, {
            locateInfs: () => infs,
            resolveInfc: async () => path.join(dir, 'infc'),
            run: (command, args, options) => run(command, args, options),
            readFile: async (file) => fs.readFileSync(file),
            api: new ProverApi(baseUrl, 'infp_key_test'),
            confirmUnchecked: async () => false,
            progress: () => undefined,
            cancelled: () => false,
        });
        assert.strictEqual(outcome.kind, 'submitted', JSON.stringify(outcome));
        assert.ok(outcome.kind === 'submitted' && outcome.vPath === path.join(dir, 'src', 'out', 'controller.v'));
        assert.strictEqual(received.length, 1);
        assert.strictEqual(received[0].filename, 'controller.v');
        assert.strictEqual(received[0].auth, 'Bearer infp_key_test');
        assert.strictEqual(received[0].key, idempotencyKey(received[0].content!));
        assert.ok(received[0].content!.includes(HOLE_MARKER));
    });
});

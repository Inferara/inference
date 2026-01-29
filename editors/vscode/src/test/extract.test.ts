import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it, before, after } from 'node:test';
import { execSync } from 'node:child_process';

import { extractArchive } from '../utils/extract';

describe('extractArchive', () => {
    let tmpDir: string;

    before(() => {
        tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-extract-test-'));
    });

    after(() => {
        fs.rmSync(tmpDir, { recursive: true });
    });

    describe('tar.gz extraction', () => {
        let tarGzPath: string;

        before(() => {
            const sourceDir = path.join(tmpDir, 'tar-source');
            fs.mkdirSync(sourceDir, { recursive: true });
            fs.writeFileSync(
                path.join(sourceDir, 'hello.txt'),
                'hello from tar',
            );
            fs.writeFileSync(
                path.join(sourceDir, 'bin-file'),
                '#!/bin/sh\necho hi\n',
            );

            tarGzPath = path.join(tmpDir, 'test-archive.tar.gz');
            execSync(`tar -czf "${tarGzPath}" -C "${sourceDir}" .`);
        });

        it('extracts a .tar.gz archive', async () => {
            const destDir = path.join(tmpDir, 'tar-dest');
            await extractArchive({ archivePath: tarGzPath, destDir });

            const content = fs.readFileSync(
                path.join(destDir, 'hello.txt'),
                'utf-8',
            );
            assert.strictEqual(content, 'hello from tar');
        });

        it('creates destination directory if it does not exist', async () => {
            const destDir = path.join(tmpDir, 'nested', 'deep', 'tar-dest');
            assert.ok(!fs.existsSync(destDir));

            await extractArchive({ archivePath: tarGzPath, destDir });

            assert.ok(fs.existsSync(destDir));
            assert.ok(
                fs.existsSync(path.join(destDir, 'hello.txt')),
            );
        });

        it('sets executable permissions on Unix', async () => {
            if (process.platform === 'win32') {
                return;
            }

            const destDir = path.join(tmpDir, 'tar-perms');
            await extractArchive({ archivePath: tarGzPath, destDir });

            const stat = fs.statSync(path.join(destDir, 'bin-file'));
            const mode = stat.mode & 0o777;
            assert.ok(
                (mode & 0o111) !== 0,
                `Expected executable permission bits, got ${mode.toString(8)}`,
            );
        });
    });

    describe('unsupported format', () => {
        it('throws for unsupported archive extension', async () => {
            const fakePath = path.join(tmpDir, 'archive.rar');
            fs.writeFileSync(fakePath, 'fake');

            await assert.rejects(
                extractArchive({
                    archivePath: fakePath,
                    destDir: path.join(tmpDir, 'rar-dest'),
                }),
                (err: Error) => {
                    assert.ok(err.message.includes('Unsupported archive format'));
                    return true;
                },
            );
        });
    });
});

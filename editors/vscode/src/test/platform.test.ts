import * as assert from 'node:assert';
import { describe, it } from 'node:test';
import { detectPlatform } from '../toolchain/platform';

describe('platform detection', () => {
    it('detects linux-x64', () => {
        const result = detectPlatform('linux', 'x64');
        assert.deepStrictEqual(result, {
            id: 'linux-x64',
            archiveExtension: '.tar.gz',
            binaryName: 'infs',
        });
    });

    it('detects macos-arm64', () => {
        const result = detectPlatform('darwin', 'arm64');
        assert.deepStrictEqual(result, {
            id: 'macos-arm64',
            archiveExtension: '.tar.gz',
            binaryName: 'infs',
        });
    });

    it('detects windows-x64', () => {
        const result = detectPlatform('win32', 'x64');
        assert.deepStrictEqual(result, {
            id: 'windows-x64',
            archiveExtension: '.zip',
            binaryName: 'infs.exe',
        });
    });

    it('returns null for unsupported linux-arm64', () => {
        assert.strictEqual(detectPlatform('linux', 'arm64'), null);
    });

    it('returns null for unsupported darwin-x64', () => {
        assert.strictEqual(detectPlatform('darwin', 'x64'), null);
    });

    it('returns null for unsupported freebsd-x64', () => {
        assert.strictEqual(detectPlatform('freebsd', 'x64'), null);
    });

    it('uses runtime values when no arguments given', () => {
        const result = detectPlatform();
        // On the CI/dev machine, this should return a valid result or null
        // depending on the platform. We just verify it doesn't throw.
        assert.ok(result === null || typeof result.id === 'string');
    });

    it('returns null for win32-arm64', () => {
        assert.strictEqual(detectPlatform('win32', 'arm64'), null);
    });

    it('windows platform has .exe binary name', () => {
        const result = detectPlatform('win32', 'x64');
        assert.ok(result !== null);
        assert.strictEqual(result.binaryName, 'infs.exe');
    });

    it('linux platform has .tar.gz extension', () => {
        const result = detectPlatform('linux', 'x64');
        assert.ok(result !== null);
        assert.strictEqual(result.archiveExtension, '.tar.gz');
    });

    it('macos platform has .tar.gz extension', () => {
        const result = detectPlatform('darwin', 'arm64');
        assert.ok(result !== null);
        assert.strictEqual(result.archiveExtension, '.tar.gz');
    });

    it('windows platform has .zip extension', () => {
        const result = detectPlatform('win32', 'x64');
        assert.ok(result !== null);
        assert.strictEqual(result.archiveExtension, '.zip');
    });

    it('linux and macos have "infs" as binary name', () => {
        const linux = detectPlatform('linux', 'x64');
        const macos = detectPlatform('darwin', 'arm64');
        assert.ok(linux !== null && linux.binaryName === 'infs');
        assert.ok(macos !== null && macos.binaryName === 'infs');
    });
});

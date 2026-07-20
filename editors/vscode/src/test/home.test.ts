import * as assert from 'node:assert';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it } from 'node:test';

import { inferenceHome } from '../toolchain/home';

/**
 * Per-platform derivation matrix for the managed toolchain root. The
 * expectations mirror `ToolchainPaths::new()` in
 * `apps/infs/src/toolchain/paths.rs`: INFERENCE_HOME override first, then
 * `%APPDATA%\inference` on Windows and `~/.inference` everywhere else
 * (including macOS, which infs does NOT treat as Application Support).
 */
describe('inferenceHome', () => {
    const posixDefault = path.posix.join(os.homedir(), '.inference');
    const windowsFallback = path.win32.join(
        os.homedir(),
        'AppData',
        'Roaming',
        'inference',
    );

    describe('INFERENCE_HOME override', () => {
        const platforms: NodeJS.Platform[] = ['win32', 'darwin', 'linux'];

        for (const platform of platforms) {
            it(`wins on ${platform}`, () => {
                const env = { INFERENCE_HOME: '/custom/inference' };
                assert.strictEqual(
                    inferenceHome(env, platform),
                    '/custom/inference',
                );
            });
        }

        it('wins over APPDATA on win32', () => {
            const env = {
                INFERENCE_HOME: 'D:\\custom\\inference',
                APPDATA: 'C:\\Users\\u\\AppData\\Roaming',
            };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                'D:\\custom\\inference',
            );
        });

        it('is used verbatim, without normalization', () => {
            const env = { INFERENCE_HOME: '/custom/inference/' };
            assert.strictEqual(
                inferenceHome(env, 'linux'),
                '/custom/inference/',
            );
        });

        it('is ignored when set to the empty string', () => {
            const env = { INFERENCE_HOME: '' };
            assert.strictEqual(inferenceHome(env, 'linux'), posixDefault);
        });

        it('is ignored when empty on win32 (APPDATA default applies)', () => {
            const env = {
                INFERENCE_HOME: '',
                APPDATA: 'C:\\Users\\u\\AppData\\Roaming',
            };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                'C:\\Users\\u\\AppData\\Roaming\\inference',
            );
        });
    });

    describe('win32 default (APPDATA)', () => {
        it('derives %APPDATA%\\inference when APPDATA is set', () => {
            const env = { APPDATA: 'C:\\Users\\u\\AppData\\Roaming' };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                'C:\\Users\\u\\AppData\\Roaming\\inference',
            );
        });

        it('does not double the separator for a trailing backslash', () => {
            const env = { APPDATA: 'C:\\Users\\u\\AppData\\Roaming\\' };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                'C:\\Users\\u\\AppData\\Roaming\\inference',
            );
        });

        it('normalizes forward slashes in APPDATA', () => {
            const env = { APPDATA: 'C:/Users/u/AppData/Roaming/' };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                'C:\\Users\\u\\AppData\\Roaming\\inference',
            );
        });

        it('preserves a UNC-style APPDATA prefix', () => {
            const env = { APPDATA: '\\\\srv\\share\\Roaming' };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                '\\\\srv\\share\\Roaming\\inference',
            );
        });

        it('preserves a UNC-style APPDATA prefix with trailing slash', () => {
            const env = { APPDATA: '\\\\srv\\share\\Roaming\\' };
            assert.strictEqual(
                inferenceHome(env, 'win32'),
                '\\\\srv\\share\\Roaming\\inference',
            );
        });

        it('falls back to <homedir>\\AppData\\Roaming\\inference when APPDATA is unset', () => {
            assert.strictEqual(inferenceHome({}, 'win32'), windowsFallback);
        });

        it('treats an empty APPDATA like an unset one', () => {
            const env = { APPDATA: '' };
            assert.strictEqual(inferenceHome(env, 'win32'), windowsFallback);
        });
    });

    describe('non-Windows default', () => {
        it('derives ~/.inference on linux', () => {
            assert.strictEqual(inferenceHome({}, 'linux'), posixDefault);
        });

        it('derives ~/.inference on darwin (not Application Support)', () => {
            const home = inferenceHome({}, 'darwin');
            assert.strictEqual(home, posixDefault);
            assert.ok(!home.includes('Application Support'));
        });

        it('ignores APPDATA on non-Windows platforms', () => {
            const env = { APPDATA: 'C:\\Users\\u\\AppData\\Roaming' };
            assert.strictEqual(inferenceHome(env, 'linux'), posixDefault);
            assert.strictEqual(inferenceHome(env, 'darwin'), posixDefault);
        });
    });

    describe('default parameters', () => {
        it('defaults to process.env and process.platform', () => {
            assert.strictEqual(
                inferenceHome(),
                inferenceHome(process.env, process.platform),
            );
        });
    });
});

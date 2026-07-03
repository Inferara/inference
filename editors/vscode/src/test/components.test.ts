import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    KNOWN_COMPONENTS,
    componentAddArgs,
    wasmOptNeedsAttention,
} from '../toolchain/components';
import { DoctorCheckStatus, DoctorResult } from '../toolchain/doctor';

/** Build a minimal DoctorResult from a list of (name, status) checks. */
function doctorResult(
    checks: Array<{ name: string; status: DoctorCheckStatus }>,
): DoctorResult {
    return {
        checks: checks.map((c) => ({
            name: c.name,
            status: c.status,
            message: '',
        })),
        hasErrors: checks.some((c) => c.status === 'fail'),
        hasWarnings: checks.some((c) => c.status === 'warn'),
        summary: '',
    };
}

describe('componentAddArgs', () => {
    it('builds the argv for `infs component add`', () => {
        assert.deepStrictEqual(componentAddArgs('wasm-opt'), [
            'component',
            'add',
            'wasm-opt',
        ]);
    });

    it('lists wasm-opt as a known component', () => {
        assert.ok(KNOWN_COMPONENTS.includes('wasm-opt'));
    });
});

describe('wasmOptNeedsAttention', () => {
    it('returns false when the wasm-opt check is OK', () => {
        const result = doctorResult([{ name: 'wasm-opt', status: 'ok' }]);
        assert.strictEqual(wasmOptNeedsAttention(result), false);
    });

    it('returns true when the wasm-opt check warns', () => {
        const result = doctorResult([{ name: 'wasm-opt', status: 'warn' }]);
        assert.strictEqual(wasmOptNeedsAttention(result), true);
    });

    it('returns true when the wasm-opt check fails', () => {
        const result = doctorResult([{ name: 'wasm-opt', status: 'fail' }]);
        assert.strictEqual(wasmOptNeedsAttention(result), true);
    });

    it('returns false when there is no wasm-opt check', () => {
        const result = doctorResult([{ name: 'infs binary', status: 'ok' }]);
        assert.strictEqual(wasmOptNeedsAttention(result), false);
    });

    it('returns false when a different check warns', () => {
        const result = doctorResult([
            { name: 'wasm-opt', status: 'ok' },
            { name: 'Default toolchain', status: 'warn' },
        ]);
        assert.strictEqual(wasmOptNeedsAttention(result), false);
    });
});

import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    isProvedSuccess,
} from '../prover/jobPresentation';
import type { ClaimClass, RunMode } from '../prover/types';

describe('job proof presentation', () => {
    it('recognizes behavioral proof success only for a Verified Succeeded/prove result', () => {
        assert.ok(isProvedSuccess('Succeeded', 'prove', 2, 2, 'Verified'));
        for (const mode of [
            'compile-goals',
            'unknown',
            null,
            undefined,
        ] satisfies Array<RunMode | null | undefined>) {
            assert.ok(!isProvedSuccess('Succeeded', mode, 2, 2, 'Verified'));
        }
        assert.ok(!isProvedSuccess('Failed', 'prove', 2, 2, 'Verified'));
        assert.ok(!isProvedSuccess('PartialSuccess', 'prove', 2, 2, 'Verified'));
        assert.ok(!isProvedSuccess('Succeeded', 'prove', 0, 0, 'Verified'));
        assert.ok(!isProvedSuccess('Succeeded', 'prove', 2, 1, 'Verified'));
        assert.ok(!isProvedSuccess('Succeeded', 'prove', 2, 3, 'Verified'));
        assert.ok(
            !isProvedSuccess(
                'Succeeded',
                'prove',
                undefined,
                undefined,
                'Verified',
            ),
        );
        for (const claimClass of [
            'StructuralOnly',
            'PartialVerified',
            'Unverified',
            null,
            undefined,
        ] satisfies Array<ClaimClass | null | undefined>) {
            assert.ok(
                !isProvedSuccess('Succeeded', 'prove', 2, 2, claimClass),
                `${String(claimClass)} must not be behavioral success`,
            );
        }
    });
});

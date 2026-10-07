import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    isProvedSuccess,
    jobStatusIconId,
    jobStatusLabel,
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

    it('uses a check icon only for durable proof success', () => {
        assert.strictEqual(
            jobStatusIconId('Succeeded', 'prove', 2, 2, 'Verified'),
            'check',
        );
        assert.strictEqual(
            jobStatusIconId('Succeeded', 'prove', 2, 2, 'StructuralOnly'),
            'warning',
        );
        assert.strictEqual(
            jobStatusIconId('Succeeded', 'prove', 2, 2, undefined),
            'warning',
        );
        assert.strictEqual(
            jobStatusIconId('Succeeded', 'compile-goals', 2, 2, 'Verified'),
            'warning',
        );
        assert.strictEqual(
            jobStatusIconId('Succeeded', 'unknown', 2, 2, 'Verified'),
            'warning',
        );
        assert.strictEqual(
            jobStatusIconId('Succeeded', null, 2, 2, 'Verified'),
            'warning',
        );
        assert.strictEqual(
            jobStatusIconId('Succeeded', 'prove', 2, 1, 'Verified'),
            'warning',
        );
        assert.strictEqual(
            jobStatusIconId(
                'CompileGoals',
                'compile-goals',
                2,
                0,
                'Unverified',
            ),
            'warning',
        );
    });

    it('qualifies Succeeded text unless mode and counts prove completion', () => {
        assert.strictEqual(
            jobStatusLabel('Succeeded', 'prove', 2, 2, 'Verified'),
            'Succeeded',
        );
        assert.strictEqual(
            jobStatusLabel('Succeeded', 'prove', 2, 2, 'StructuralOnly'),
            'Succeeded (structural only)',
        );
        assert.strictEqual(
            jobStatusLabel('Succeeded', 'prove', 2, 2, undefined),
            'Succeeded (claim unverified)',
        );
        assert.strictEqual(
            jobStatusLabel('Succeeded', 'compile-goals', 2, 2, 'Verified'),
            'Succeeded (compile-goals)',
        );
        assert.strictEqual(
            jobStatusLabel('Succeeded', null, 2, 2, 'Verified'),
            'Succeeded (mode unknown)',
        );
        assert.strictEqual(
            jobStatusLabel('Succeeded', 'prove', 2, 1, 'Verified'),
            'Succeeded (counts invalid)',
        );
        assert.strictEqual(
            jobStatusLabel(
                'CompileGoals',
                'compile-goals',
                2,
                0,
                'Unverified',
            ),
            'CompileGoals',
        );
    });
});

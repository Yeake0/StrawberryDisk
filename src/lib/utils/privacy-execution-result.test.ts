import { describe, expect, it } from 'vitest';

import type { PrivacyExecutionResult, PrivacyScanResult } from '@/lib/models/privacy';

import { applyPrivacyExecutionResult } from './privacy-execution-result';

const scan: PrivacyScanResult = {
  schemaVersion: 6,
  scanId: 'scan-1',
  revision: 'revision-1',
  timeRange: 'allTime',
  scannedAtMs: 1,
  elapsedMs: 2,
  coverage: [
    {
      sourceId: 'browser',
      sourceName: 'Browser',
      iconPath: null,
      capability: 'ready',
      itemCount: 5,
    },
  ],
  items: [
    {
      token: 'cleared',
      sourceId: 'browser',
      sourceName: 'Browser',
      profileId: 'browser:Default',
      profileName: 'Default',
      category: 'browserActivity',
      kind: 'browsingHistory',
      sensitivity: 'activity',
      impact: 'low',
      recommendation: 'recommended',
      capability: 'ready',
      itemCount: 3,
      estimatedBytes: 30,
      selectedByDefault: true,
      requiresBrowserClose: true,
      synchronizationMayPropagate: false,
    },
    {
      token: 'failed',
      sourceId: 'browser',
      sourceName: 'Browser',
      profileId: 'browser:Default',
      profileName: 'Default',
      category: 'browserActivity',
      kind: 'downloadHistory',
      sensitivity: 'activity',
      impact: 'low',
      recommendation: 'recommended',
      capability: 'ready',
      itemCount: 2,
      estimatedBytes: 20,
      selectedByDefault: true,
      requiresBrowserClose: false,
      synchronizationMayPropagate: false,
    },
  ],
};

function result(): PrivacyExecutionResult {
  return {
    planId: 'plan-1',
    affectedItemCount: 3,
    failedItemCount: 1,
    scan: null,
    items: [
      {
        token: 'cleared',
        status: 'cleared',
        affectedItemCount: 3,
        verified: true,
        failureReason: null,
      },
      {
        token: 'failed',
        status: 'failed',
        affectedItemCount: 0,
        verified: false,
        failureReason: 'operationFailed',
      },
    ],
  };
}

describe('privacy execution result synchronization', () => {
  it('empties only verified rows and updates source totals', () => {
    const updated = applyPrivacyExecutionResult(structuredClone(scan), result());

    expect(updated.items[0]).toMatchObject({
      capability: 'empty',
      estimatedBytes: 0,
      itemCount: 0,
      requiresBrowserClose: false,
      selectedByDefault: false,
    });
    expect(updated.items[1]).toEqual(scan.items[1]);
    expect(updated.coverage[0]).toMatchObject({ capability: 'ready', itemCount: 2 });
  });

  it('prefers the authoritative Core snapshot', () => {
    const execution = result();
    const authoritative = structuredClone(scan);
    authoritative.scanId = 'reconciled-scan';
    execution.scan = authoritative;

    expect(applyPrivacyExecutionResult(scan, execution)).toBe(authoritative);
  });
});

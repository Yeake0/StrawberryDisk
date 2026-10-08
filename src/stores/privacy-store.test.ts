import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  PrivacyExecutionPlan,
  PrivacyExecutionResult,
  PrivacyScanProgress,
  PrivacyScanResult,
} from '@/lib/models/privacy';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { LoggerService } from '@/lib/services/logger-service';

import { useAppStore } from './app-store';
import { usePrivacyStore } from './privacy-store';

const serviceMocks = vi.hoisted(() => ({
  scan: vi.fn(),
  scanWithProgress: vi.fn(),
  cancelScan: vi.fn(),
  prepare: vi.fn(),
  closeBrowsers: vi.fn(),
  refreshBrowserStatus: vi.fn(),
  execute: vi.fn(),
  executeWithProgress: vi.fn(),
  cancelExecution: vi.fn(),
}));

vi.mock('@/lib/services/privacy-service', () => ({ PrivacyService: serviceMocks }));

const scanResult: PrivacyScanResult = {
  schemaVersion: 6,
  scanId: 'scan-1',
  revision: 'revision-1',
  timeRange: 'allTime',
  scannedAtMs: 1,
  elapsedMs: 2,
  coverage: [],
  items: [
    {
      token: 'recommended',
      sourceId: 'edge',
      sourceName: 'Microsoft Edge',
      profileId: 'edge:Default',
      profileName: 'Default',
      category: 'browserActivity',
      kind: 'downloadHistory',
      sensitivity: 'activity',
      impact: 'low',
      recommendation: 'recommended',
      capability: 'ready',
      itemCount: 3,
      estimatedBytes: 10,
      selectedByDefault: true,
      requiresBrowserClose: true,
      synchronizationMayPropagate: false,
    },
    {
      token: 'manual',
      sourceId: 'edge',
      sourceName: 'Microsoft Edge',
      profileId: 'edge:Default',
      profileName: 'Default',
      category: 'browserAccountState',
      kind: 'cookies',
      sensitivity: 'accountState',
      impact: 'signOut',
      recommendation: 'manual',
      capability: 'ready',
      itemCount: 2,
      estimatedBytes: 5,
      selectedByDefault: false,
      requiresBrowserClose: true,
      synchronizationMayPropagate: true,
    },
  ],
};

const plan: PrivacyExecutionPlan = {
  schemaVersion: 2,
  planId: 'plan-1',
  scanId: 'scan-1',
  createdAtMs: 1,
  expiresAtMs: 2,
  requiresConfirmation: true,
  requiresBrowserClose: true,
  browserCloseRequirements: [{ sourceId: 'edge', sourceName: 'Microsoft Edge', processes: ['msedge.exe'] }],
  items: [
    {
      token: 'manual',
      sourceId: 'edge',
      sourceName: 'Microsoft Edge',
      profileName: 'Default',
      kind: 'cookies',
      impact: 'signOut',
      itemCount: 2,
      estimatedBytes: 5,
      requiresBrowserClose: true,
      synchronizationMayPropagate: true,
    },
  ],
};

const executionResult: PrivacyExecutionResult = {
  planId: 'plan-1',
  affectedItemCount: 2,
  failedItemCount: 0,
  items: [
    {
      token: 'manual',
      status: 'cleared',
      affectedItemCount: 2,
      verified: true,
      failureReason: null,
    },
  ],
  scan: null,
};

describe('privacy store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    for (const mock of Object.values(serviceMocks)) mock.mockReset();
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
  });

  afterEach(() => vi.restoreAllMocks());

  it('runs scan, selection, plan, and execution as one bounded state flow', async () => {
    serviceMocks.scanWithProgress.mockImplementation(
      async (_request: unknown, progress: (value: PrivacyScanProgress) => void) => {
        progress({ stage: 'browser', sourceName: 'Microsoft Edge', completedSources: 1, totalSources: 2 });
        return structuredClone(scanResult);
      }
    );
    serviceMocks.prepare.mockResolvedValue(structuredClone(plan));
    serviceMocks.executeWithProgress.mockImplementation(
      async (_request: unknown, progress: (value: unknown) => void) => {
        progress({
          stage: 'cleaning',
          currentToken: 'manual',
          currentSourceName: 'Microsoft Edge',
          currentKind: 'cookies',
          completedItemCount: 0,
          totalItemCount: 1,
          affectedItemCount: 0,
          elapsedMs: 1,
          completedItems: [],
        });
        return structuredClone(executionResult);
      }
    );
    const refreshDisk = vi.mocked(useAppStore().refreshSystemDisk);
    const store = usePrivacyStore();

    await store.scan();
    expect(store.selectedTokens).toEqual(['recommended']);

    store.toggle('manual');
    store.setSelection(['manual', 'unknown', 'manual']);
    expect(store.selectedTokens).toEqual(['manual']);

    await expect(store.prepare()).resolves.toEqual(plan);
    expect(serviceMocks.prepare).toHaveBeenCalledWith({ scanId: 'scan-1', tokens: ['manual'] });

    await store.execute();
    expect(refreshDisk).toHaveBeenCalledOnce();
    expect(store.result).toEqual(executionResult);
    expect(store.completedPlan).toEqual(plan);
    expect(store.scanResult?.items[1]).toMatchObject({
      token: 'manual',
      capability: 'empty',
      itemCount: 0,
      estimatedBytes: 0,
    });
    expect(store.selectedTokens).toEqual([]);
    expect(serviceMocks.scanWithProgress).toHaveBeenCalledOnce();
    expect(serviceMocks.scan).not.toHaveBeenCalled();
    expect(store.scanProgress).toBeNull();
    expect(store.executionProgress).toMatchObject({ stage: 'cleaning', currentSourceName: 'Microsoft Edge' });
    expect(store.executionItems).toEqual(plan.items);
    expect(store.executionStartedAtMs).toBeNull();
  });

  it('invalidates old results on range changes and forwards active cancellation', async () => {
    const store = usePrivacyStore();
    store.scanResult = structuredClone(scanResult);
    store.selectedTokens = ['recommended'];
    store.plan = structuredClone(plan);
    store.setTimeRange('today');
    expect(store.scanResult).toBeNull();
    expect(store.plan).toBeNull();

    store.scanning = true;
    serviceMocks.cancelScan.mockResolvedValue(undefined);
    await store.cancelScan();
    expect(serviceMocks.cancelScan).toHaveBeenCalledOnce();

    store.executing = true;
    serviceMocks.cancelExecution.mockResolvedValue(undefined);
    await store.cancelExecution();
    expect(serviceMocks.cancelExecution).toHaveBeenCalledOnce();

    store.executing = false;
    store.plan = structuredClone(plan);
    store.clearPlan();
    expect(store.plan).toBeNull();
  });

  it('closes selected browser sources and executes without another scan', async () => {
    const store = usePrivacyStore();
    store.scanResult = structuredClone(scanResult);
    store.selectedTokens = ['manual'];
    store.plan = structuredClone(plan);
    const closeResult = {
      mode: 'graceful' as const,
      matchedProcessCount: 1,
      requestedProcessCount: 1,
      remainingProcessCount: 0,
      failedTargetCount: 0,
      targets: [
        {
          targetId: 'edge',
          status: 'completed' as const,
          matchedProcessCount: 1,
          requestedProcessCount: 1,
          remainingProcesses: [],
        },
      ],
      elapsedMs: 4,
    };
    serviceMocks.closeBrowsers.mockResolvedValue(closeResult);

    await expect(store.closeBrowsers(['edge'], 'graceful')).resolves.toEqual(closeResult);
    expect(serviceMocks.closeBrowsers).toHaveBeenCalledWith({
      planId: 'plan-1',
      sourceIds: ['edge'],
      mode: 'graceful',
    });

    serviceMocks.executeWithProgress.mockResolvedValue(structuredClone(executionResult));
    await store.execute([]);

    expect(serviceMocks.executeWithProgress).toHaveBeenCalledWith(
      { planId: 'plan-1', excludedSourceIds: [] },
      expect.any(Function)
    );
    expect(serviceMocks.scan).not.toHaveBeenCalled();
    expect(serviceMocks.scanWithProgress).not.toHaveBeenCalled();
  });

  it('omits skipped application items from the execution progress snapshot', async () => {
    const store = usePrivacyStore();
    store.scanResult = structuredClone(scanResult);
    store.selectedTokens = ['manual'];
    store.plan = structuredClone(plan);
    store.plan.items.push({
      ...structuredClone(plan.items[0]!),
      token: 'same-name-other-source',
      sourceId: 'edge-preview',
    });
    serviceMocks.executeWithProgress.mockResolvedValue({
      ...structuredClone(executionResult),
      affectedItemCount: 0,
      items: [],
    });

    await store.execute(['edge']);

    expect(store.executionItems.map(item => item.token)).toEqual(['same-name-other-source']);
    expect(serviceMocks.executeWithProgress).toHaveBeenCalledWith(
      { planId: 'plan-1', excludedSourceIds: ['edge'] },
      expect.any(Function)
    );
  });

  it('refreshes remaining process state without closing applications or rescanning privacy data', async () => {
    const store = usePrivacyStore();
    store.plan = structuredClone(plan);
    serviceMocks.refreshBrowserStatus.mockResolvedValue({
      runningProcessCount: 0,
      elapsedMs: 3,
      targets: [{ sourceId: 'edge', runningProcesses: [] }],
    });

    await store.refreshBrowserStatus(['edge']);

    expect(serviceMocks.refreshBrowserStatus).toHaveBeenCalledWith({ planId: 'plan-1', sourceIds: ['edge'] });
    expect(serviceMocks.closeBrowsers).not.toHaveBeenCalled();
    expect(serviceMocks.scan).not.toHaveBeenCalled();
    expect(serviceMocks.scanWithProgress).not.toHaveBeenCalled();
    expect(store.browserStatusResult?.runningProcessCount).toBe(0);
    expect(store.refreshingBrowserStatus).toBe(false);
  });

  it('recovers automatic process refresh after a transient failure without repeating the error', async () => {
    const reportError = vi.spyOn(useAppStore(), 'reportError').mockImplementation(() => undefined);
    const warn = vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);
    const info = vi.spyOn(LoggerService, 'info').mockImplementation(() => undefined);
    const store = usePrivacyStore();
    store.plan = structuredClone(plan);
    serviceMocks.refreshBrowserStatus
      .mockRejectedValueOnce({ code: 'operationFailed', details: { operation: 'refresh' }, retryable: true })
      .mockResolvedValueOnce({
        runningProcessCount: 0,
        elapsedMs: 3,
        targets: [{ sourceId: 'edge', runningProcesses: [] }],
      });

    await store.refreshBrowserStatus(['edge']);
    await store.refreshBrowserStatus(['edge']);

    expect(serviceMocks.refreshBrowserStatus).toHaveBeenCalledTimes(2);
    expect(reportError).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledWith(LOG_DOMAINS.privacy, LOG_EVENTS.privacyBrowserStatusRefreshFailed, {
      code: 'operationFailed',
      sourceCount: 1,
    });
    expect(info).toHaveBeenCalledWith(LOG_DOMAINS.privacy, LOG_EVENTS.privacyBrowserStatusRefreshRecovered, {
      sourceCount: 1,
      runningProcessCount: 0,
    });
    expect(store.browserStatusRefreshFailed).toBe(false);
  });

  it('does not hide a full rescan behind a failed confirmation request', async () => {
    const store = usePrivacyStore();
    store.scanResult = structuredClone(scanResult);
    store.selectedTokens = ['manual'];
    serviceMocks.prepare.mockRejectedValueOnce({
      code: 'operationFailed',
      details: { reason: 'itemChanged' },
      retryable: true,
    });

    await expect(store.prepare()).resolves.toBeNull();

    expect(serviceMocks.prepare).toHaveBeenCalledOnce();
    expect(serviceMocks.scan).not.toHaveBeenCalled();
    expect(serviceMocks.scanWithProgress).not.toHaveBeenCalled();
    expect(store.selectedTokens).toEqual(['manual']);
    expect(store.preparing).toBe(false);
  });

  it('reports scan failures but treats an explicit cancellation as a quiet terminal state', async () => {
    const reportError = vi.spyOn(useAppStore(), 'reportError').mockImplementation(() => undefined);
    const warn = vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);
    const store = usePrivacyStore();
    const failure = { code: 'operationFailed', details: { operation: 'scan_privacy' }, retryable: true };
    serviceMocks.scanWithProgress.mockRejectedValueOnce(failure);

    await store.scan();

    expect(reportError).toHaveBeenCalledWith(failure);
    expect(warn).toHaveBeenCalledWith(LOG_DOMAINS.privacy, LOG_EVENTS.privacyScanFailed, {
      code: 'operationFailed',
    });
    expect(store.scanning).toBe(false);
    expect(store.cancellingScan).toBe(false);
    expect(store.scanProgress).toBeNull();

    reportError.mockClear();
    warn.mockClear();
    serviceMocks.scanWithProgress.mockRejectedValueOnce({
      code: 'operationCancelled',
      details: { operation: 'scan_privacy' },
      retryable: false,
    });

    await store.scan();

    expect(reportError).not.toHaveBeenCalled();
    expect(warn).not.toHaveBeenCalled();
    expect(store.scanning).toBe(false);
  });

  it('restores cancellation flags when the native cancellation request fails', async () => {
    const reportError = vi.spyOn(useAppStore(), 'reportError').mockImplementation(() => undefined);
    const store = usePrivacyStore();
    const failure = { code: 'operationFailed', details: {}, retryable: true };

    store.scanning = true;
    serviceMocks.cancelScan.mockRejectedValueOnce(failure);
    await store.cancelScan();
    expect(store.cancellingScan).toBe(false);

    store.executing = true;
    serviceMocks.cancelExecution.mockRejectedValueOnce(failure);
    await store.cancelExecution();
    expect(store.cancellingExecution).toBe(false);
    expect(reportError).toHaveBeenCalledTimes(2);
  });

  it('keeps the scanned selection reviewable when application close or execution fails', async () => {
    const reportError = vi.spyOn(useAppStore(), 'reportError').mockImplementation(() => undefined);
    const warn = vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);
    const store = usePrivacyStore();
    const failure = { code: 'operationFailed', details: {}, retryable: true };
    store.scanResult = structuredClone(scanResult);
    store.selectedTokens = ['manual'];
    store.plan = structuredClone(plan);

    serviceMocks.closeBrowsers.mockRejectedValueOnce(failure);
    await expect(store.closeBrowsers(['edge'], 'graceful')).resolves.toBeNull();
    expect(store.closingBrowsers).toBe(false);
    expect(store.plan).toEqual(plan);
    expect(store.selectedTokens).toEqual(['manual']);

    serviceMocks.executeWithProgress.mockRejectedValueOnce(failure);
    await store.execute();

    expect(store.executing).toBe(false);
    expect(store.plan).toBeNull();
    expect(store.scanResult).toEqual(scanResult);
    expect(store.selectedTokens).toEqual(['manual']);
    expect(reportError).toHaveBeenCalledTimes(2);
    expect(warn).toHaveBeenCalledWith(LOG_DOMAINS.privacy, LOG_EVENTS.privacyExecutionFailed, {
      code: 'operationFailed',
    });
  });
});

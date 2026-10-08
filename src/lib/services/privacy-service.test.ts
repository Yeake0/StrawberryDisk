import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PrivacyService } from './privacy-service';

const { invokeMock, listenMock, unlistenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
  unlistenMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

describe('PrivacyService', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    unlistenMock.mockReset();
  });

  it('uses the stable scan and cancellation commands', async () => {
    invokeMock.mockResolvedValue(undefined);

    await PrivacyService.scan({ timeRange: 'today' });
    expect(invokeMock).toHaveBeenLastCalledWith('scan_privacy', { request: { timeRange: 'today' } });

    await PrivacyService.cancelScan();
    expect(invokeMock).toHaveBeenLastCalledWith('cancel_privacy_scan');

    await PrivacyService.details({ scanId: 'scan', token: 'opaque', offset: 100, limit: 100 });
    expect(invokeMock).toHaveBeenLastCalledWith('get_privacy_details', {
      request: { scanId: 'scan', token: 'opaque', offset: 100, limit: 100 },
    });
  });

  it('subscribes before scanning and releases the progress listener', async () => {
    const progress = {
      stage: 'browser' as const,
      sourceName: 'Google Chrome',
      completedSources: 1,
      totalSources: 4,
    };
    const handler = vi.fn();
    listenMock.mockImplementation(
      async (_eventName: string, listener: (event: { payload: typeof progress }) => void) => {
        listener({ payload: progress });
        return unlistenMock;
      }
    );
    invokeMock.mockResolvedValue({ scanId: 'scan' });

    await PrivacyService.scanWithProgress({ timeRange: 'allTime' }, handler);

    expect(listenMock).toHaveBeenCalledWith('privacy-scan-progress', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(progress);
    expect(unlistenMock).toHaveBeenCalledOnce();
  });

  it('subscribes before privacy cleanup and releases the progress listener', async () => {
    const progress = {
      stage: 'cleaning' as const,
      currentToken: 'history-token',
      currentSourceName: 'Firefox',
      currentKind: 'browsingHistory' as const,
      completedItemCount: 1,
      totalItemCount: 2,
      affectedItemCount: 3,
      elapsedMs: 20,
      completedItems: [{ token: 'previous-token', status: 'cleared' as const }],
    };
    const handler = vi.fn();
    listenMock.mockImplementation(
      async (_eventName: string, listener: (event: { payload: typeof progress }) => void) => {
        listener({ payload: progress });
        return unlistenMock;
      }
    );
    invokeMock.mockResolvedValue({ planId: 'plan' });

    await PrivacyService.executeWithProgress({ planId: 'plan', excludedSourceIds: [] }, handler);

    expect(listenMock).toHaveBeenCalledWith('privacy-execution-progress', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(progress);
    expect(unlistenMock).toHaveBeenCalledOnce();
  });

  it('forwards only scan identity, opaque tokens, and plan identity', async () => {
    invokeMock.mockResolvedValue(undefined);

    await PrivacyService.prepare({ scanId: 'scan', tokens: ['opaque'] });
    expect(invokeMock).toHaveBeenLastCalledWith('prepare_privacy_execution', {
      request: { scanId: 'scan', tokens: ['opaque'] },
    });

    await PrivacyService.closeBrowsers({ planId: 'plan', sourceIds: ['edge'], mode: 'graceful' });
    expect(invokeMock).toHaveBeenLastCalledWith('close_privacy_browsers', {
      request: { planId: 'plan', sourceIds: ['edge'], mode: 'graceful' },
    });

    await PrivacyService.refreshBrowserStatus({ planId: 'plan', sourceIds: ['edge'] });
    expect(invokeMock).toHaveBeenLastCalledWith('refresh_privacy_browser_status', {
      request: { planId: 'plan', sourceIds: ['edge'] },
    });

    await PrivacyService.execute({ planId: 'plan', excludedSourceIds: ['edge'] });
    expect(invokeMock).toHaveBeenLastCalledWith('execute_privacy', {
      request: { planId: 'plan', excludedSourceIds: ['edge'] },
    });

    await PrivacyService.cancelExecution();
    expect(invokeMock).toHaveBeenLastCalledWith('cancel_privacy_execution');
  });
});

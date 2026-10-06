import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { StartupArtifact } from '@/lib/models/startup';
import { LOG_EVENTS } from '@/lib/models/telemetry';
import { MacOsSystemSettingsService } from '@/lib/services/macos-system-settings-service';

const { invokeMock, nativeInfo, nativeError } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  nativeInfo: vi.fn().mockResolvedValue(undefined),
  nativeError: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock, isTauri: () => true }));
vi.mock('@tauri-apps/plugin-log', () => ({ info: nativeInfo, error: nativeError, warn: vi.fn() }));

const artifact: StartupArtifact = {
  itemId: 'mock-login-record',
  displayName: 'StrawberryDisk Startup Demo',
  sourceId: 'macos.background_tasks',
  sourceKind: 'backgroundTask',
  scope: 'currentUser',
  triggers: ['userLogon'],
  configurationPath: null,
  target: { kind: 'application', path: '/Applications/Missing.app', executableName: null, arguments: [] },
  ownerName: null,
  publisher: null,
  summary: null,
  summarySource: 'sourceLabel',
  version: null,
  iconPath: null,
  identityConfidence: 'strong',
  configuredState: 'disabled',
  runtimeState: 'unknown',
  controlCapability: 'viewOnly',
  trust: 'unknown',
  modifiedAtMs: null,
  diagnostics: ['missingTarget'],
  removalSupported: false,
  removableOrphan: false,
};

function context(message: string): Record<string, unknown> {
  return JSON.parse(message.split(' context=')[1] ?? '{}') as Record<string, unknown>;
}

describe('MacOsSystemSettingsService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockReset();
    vi.spyOn(console, 'info').mockImplementation(() => {});
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  it('opens the fixed Login Items settings destination', async () => {
    await MacOsSystemSettingsService.openLoginItems();
    expect(invokeMock).toHaveBeenCalledWith('open_macos_login_items_settings');
  });

  it('correlates the request and dispatch with the affected record in native logs', async () => {
    await MacOsSystemSettingsService.openLoginItems([artifact]);

    expect(nativeInfo).toHaveBeenCalledTimes(2);
    const request = String(nativeInfo.mock.calls[0]?.[0]);
    const dispatched = String(nativeInfo.mock.calls[1]?.[0]);
    expect(request).toContain(LOG_EVENTS.startupSystemSettingsOpenRequested);
    expect(dispatched).toContain(LOG_EVENTS.startupSystemSettingsOpenDispatched);
    expect(context(request)).toEqual({
      requestId: expect.any(String),
      destination: 'macos.loginItems',
      items: [
        {
          itemId: artifact.itemId,
          displayName: artifact.displayName,
          sourceId: artifact.sourceId,
          sourceKind: artifact.sourceKind,
          controlCapability: artifact.controlCapability,
          diagnostics: ['missingTarget'],
          targetPath: artifact.target.path,
        },
      ],
    });
    expect(context(dispatched)).toEqual(context(request));
    expect(nativeError).not.toHaveBeenCalled();
  });

  it('retains the native error and record identity without reporting a successful dispatch', async () => {
    const error = { code: 'permissionDenied', message: 'Mock settings launch denied', nativeCode: 13 };
    invokeMock.mockRejectedValue(error);

    await expect(MacOsSystemSettingsService.openLoginItems([artifact])).rejects.toBe(error);

    expect(nativeInfo).toHaveBeenCalledTimes(1);
    expect(nativeError).toHaveBeenCalledTimes(1);
    const request = String(nativeInfo.mock.calls[0]?.[0]);
    const failure = String(nativeError.mock.calls[0]?.[0]);
    expect(failure).toContain(LOG_EVENTS.startupSystemSettingsOpenFailed);
    expect(context(failure)).toEqual({ ...context(request), error });
  });
});

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { WindowsStartupToolService } from './windows-startup-tool-service';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('Windows startup tool launch', () => {
  beforeEach(() => vi.resetAllMocks());

  it.each(['services', 'taskScheduler'] as const)('opens the fixed %s destination', async tool => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await WindowsStartupToolService.open(tool);
    expect(invoke).toHaveBeenCalledWith('open_windows_startup_tool', { tool });
  });

  it('does not show an operation failure when UAC is cancelled', async () => {
    vi.mocked(invoke).mockRejectedValue({ code: 'operationCancelled', details: {}, retryable: false });
    await expect(WindowsStartupToolService.open('services')).resolves.toBeUndefined();
  });

  it('preserves permission errors for the normal error presenter', async () => {
    const error = { code: 'permissionDenied', details: {}, retryable: false };
    vi.mocked(invoke).mockRejectedValue(error);
    await expect(WindowsStartupToolService.open('services')).rejects.toEqual(error);
  });
});

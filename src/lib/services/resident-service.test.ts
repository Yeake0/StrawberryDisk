import { preferencesFixture } from '@/tests/fixtures/resident';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ResidentService } from './resident-service';

const { invoke, listen, windowListen, isVisible } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  windowListen: vi.fn(),
  isVisible: vi.fn().mockResolvedValue(false),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ isVisible, listen: windowListen }),
}));

describe('resident desktop protocol', () => {
  beforeEach(() => vi.clearAllMocks());

  it('seeds visibility without focus and preserves an event received during the query', async () => {
    const dispose = vi.fn();
    windowListen.mockResolvedValue(dispose);
    isVisible.mockResolvedValueOnce(true);
    const handler = vi.fn();
    const stop = await ResidentService.onPanelVisibility(handler);
    expect(handler).toHaveBeenLastCalledWith(true);
    stop();
    let resolve: (value: boolean) => void = () => {};
    isVisible.mockReturnValueOnce(
      new Promise<boolean>(done => {
        resolve = done;
      })
    );
    handler.mockClear();
    const pending = ResidentService.onPanelVisibility(handler);
    await Promise.resolve();
    windowListen.mock.calls.at(-1)![1]({ payload: false });
    resolve(true);
    (await pending)();
    expect(handler).toHaveBeenCalledExactlyOnceWith(false);
  });

  it('disposes visibility subscription if initial visibility cannot be read', async () => {
    const dispose = vi.fn();
    windowListen.mockResolvedValueOnce(dispose);
    isVisible.mockRejectedValueOnce(new Error('window unavailable'));
    await expect(ResidentService.onPanelVisibility(vi.fn())).rejects.toThrow('window unavailable');
    expect(dispose).toHaveBeenCalledOnce();
  });

  it.each([
    ['reading', 'monitoring_get_reading'],
    ['refresh', 'monitoring_refresh'],
    ['preferences', 'resident_get_preferences'],
    ['autostartEnabled', 'resident_get_autostart'],
    ['openPanel', 'resident_open_panel'],
    ['panelReady', 'resident_panel_ready'],
    ['hidePanel', 'resident_hide_panel'],
    ['quit', 'resident_quit'],
  ] as const)('binds %s to its registered command', async (method, command) => {
    invoke.mockResolvedValueOnce({ marker: command });
    expect(await ResidentService[method]()).toEqual({ marker: command });
    expect(invoke).toHaveBeenCalledWith(command);
  });

  it('passes typed preferences and navigation without accepting process names or paths', async () => {
    const preferences = { ...preferencesFixture(), enabled: false };
    await ResidentService.savePreferences(preferences);
    await ResidentService.openMain('applications');
    expect(invoke.mock.calls).toEqual([
      ['resident_save_preferences', { preferences }],
      ['resident_open_main', { destination: 'applications' }],
    ]);
  });

  it('sends login registration changes to the native adapter', async () => {
    await ResidentService.setAutostart(true);
    expect(invoke).toHaveBeenCalledWith('resident_set_autostart', { enabled: true });
    await ResidentService.openMain('main');
    expect(invoke).toHaveBeenLastCalledWith('resident_open_main', { destination: 'main' });
  });

  it('requests application quit using only its opaque row identity', async () => {
    invoke.mockResolvedValueOnce('requested');
    expect(await ResidentService.quitApplication('application-id')).toBe('requested');
    expect(invoke).toHaveBeenCalledWith('monitoring_quit_application', { applicationId: 'application-id' });
  });

  it('requests native memory reclamation without an authorization payload', async () => {
    await ResidentService.releaseMemory();
    expect(invoke).toHaveBeenCalledWith('monitoring_release_memory');
  });

  it('unwraps event payloads and preserves listener disposal', async () => {
    const dispose = vi.fn();
    listen.mockResolvedValue(dispose);
    windowListen.mockResolvedValue(dispose);
    const handler = vi.fn();
    expect(await ResidentService.onReading(handler)).toBe(dispose);
    expect(listen).not.toHaveBeenCalled();
    expect(windowListen).toHaveBeenCalledWith('resident-reading', expect.any(Function));
    windowListen.mock.calls[0]?.[1]({ payload: { revision: 2 } });
    expect(handler).toHaveBeenLastCalledWith({ revision: 2 });
    expect(await ResidentService.onNavigate(handler)).toBe(dispose);
    listen.mock.calls[0]?.[1]({ payload: 'settings' });
    expect(handler).toHaveBeenLastCalledWith('settings');
    expect(await ResidentService.onPanelVisibility(handler)).toBe(dispose);
    expect(windowListen).toHaveBeenLastCalledWith('resident-panel-visibility', expect.any(Function));
    handler.mockClear();
    windowListen.mock.calls.at(-1)?.[1]({ payload: false });
    expect(handler).toHaveBeenLastCalledWith(false);
    windowListen.mock.calls.at(-1)?.[1]({ payload: true });
    expect(handler).toHaveBeenLastCalledWith(true);
  });
});

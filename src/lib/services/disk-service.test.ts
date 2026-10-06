import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const native = vi.hoisted(() => ({ invoke: vi.fn(), focus: vi.fn(), isFocused: vi.fn(), stop: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onFocusChanged: native.focus, isFocused: native.isFocused }),
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));

import { DiskService } from './disk-service';

let focus: (event: { payload: boolean }) => void;
beforeEach(() => {
  vi.resetAllMocks();
  vi.useFakeTimers();
  native.focus.mockImplementation(async callback => {
    focus = callback;
    return native.stop;
  });
  native.isFocused.mockResolvedValue(true);
});
afterEach(() => vi.useRealTimers());

it('refreshes on focus and at a low frequency, pauses in the background and releases observers', async () => {
  const refresh = vi.fn().mockResolvedValue(true);
  const stop = await DiskService.watch(refresh);
  await vi.advanceTimersByTimeAsync(15_000);
  expect(refresh).toHaveBeenCalledTimes(1);
  focus({ payload: false });
  await vi.advanceTimersByTimeAsync(60_000);
  expect(refresh).toHaveBeenCalledTimes(1);
  focus({ payload: true });
  await vi.advanceTimersByTimeAsync(0);
  expect(refresh).toHaveBeenCalledTimes(2);
  stop();
  focus({ payload: true });
  await vi.advanceTimersByTimeAsync(60_000);
  expect(refresh).toHaveBeenCalledTimes(2);
  expect(native.stop).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});

it('coalesces focus and timer requests while native capacity is pending', async () => {
  let complete!: () => void;
  const refresh = vi.fn(
    () =>
      new Promise<void>(resolve => {
        complete = resolve;
      })
  );
  const stop = await DiskService.watch(refresh);
  focus({ payload: true });
  await vi.advanceTimersByTimeAsync(30_000);
  focus({ payload: true });
  expect(refresh).toHaveBeenCalledOnce();
  complete();
  await vi.advanceTimersByTimeAsync(0);
  focus({ payload: true });
  expect(refresh).toHaveBeenCalledTimes(2);
  complete();
  stop();
});

it('keeps a focus event newer than the initial focus query', async () => {
  let complete!: (focused: boolean) => void;
  native.isFocused.mockImplementation(
    () =>
      new Promise<boolean>(resolve => {
        complete = resolve;
      })
  );
  const refresh = vi.fn().mockResolvedValue(true);
  const watching = DiskService.watch(refresh);
  await vi.advanceTimersByTimeAsync(0);
  focus({ payload: false });
  complete(true);
  const stop = await watching;
  await vi.advanceTimersByTimeAsync(30_000);
  expect(refresh).not.toHaveBeenCalled();
  stop();
});

it('releases the focus observer when initial state cannot be read', async () => {
  native.isFocused.mockRejectedValue(new Error('window closed'));
  await expect(DiskService.watch(vi.fn())).rejects.toThrow('window closed');
  expect(native.stop).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});

it('requests cache invalidation only for an explicit refresh', async () => {
  await DiskService.getSystemDisk();
  await DiskService.getSystemDisk(true);
  expect(native.invoke.mock.calls).toEqual([
    ['get_system_disk', { refresh: false }],
    ['get_system_disk', { refresh: true }],
  ]);
});

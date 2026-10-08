// @vitest-environment happy-dom
import { expect, it, vi } from 'vitest';

const startup = vi.hoisted(() => ({
  mount: vi.fn(),
  loadSettings: vi.fn(() => new Promise<void>(() => {})),
}));
vi.mock('vue', () => ({
  createApp: () => ({
    use() {
      return this;
    },
    mount: startup.mount,
  }),
}));
vi.mock('pinia', () => ({ createPinia: () => ({}) }));
vi.mock('./i18n', () => ({ i18n: {} }));
vi.mock('./layouts/md-tray-panel-shell.vue', () => ({ default: {} }));
vi.mock('./stores/app-store', () => ({ useAppStore: () => ({ loadSettings: startup.loadSettings }) }));

it('mounts the tray panel without waiting for preference storage', async () => {
  await import('./tray-panel');
  expect(startup.mount).toHaveBeenCalledWith('#app');
  expect(startup.loadSettings).toHaveBeenCalledOnce();
  expect(startup.mount.mock.invocationCallOrder[0]).toBeLessThan(startup.loadSettings.mock.invocationCallOrder[0]!);
});

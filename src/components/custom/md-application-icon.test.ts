// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const fallback = vi.hoisted(() => ({ peek: vi.fn(), resolve: vi.fn() }));
vi.mock('@/lib/services/application-icon-service', () => ({
  ApplicationIconService: { peekFallback: fallback.peek, resolveFallback: fallback.resolve },
}));
vi.mock('@/lib/services/operating-system-service', () => ({
  OperatingSystemService: { isWindows: () => false, isLinux: () => false },
}));

import MdApplicationIcon from '@/components/custom/md-application-icon.vue';

describe('MdApplicationIcon', () => {
  beforeEach(() => {
    fallback.peek.mockReset().mockReturnValue(null);
    fallback.resolve.mockReset().mockResolvedValue('data:image/png;base64,native');
  });

  it('keeps an available application image without loading the fallback', () => {
    const wrapper = mount(MdApplicationIcon, { props: { src: 'original.png' } });
    expect(wrapper.get('img').attributes('src')).toBe('original.png');
    expect(fallback.resolve).not.toHaveBeenCalled();
  });

  it('uses a neutral process fallback on Linux without requesting a macOS icon', async () => {
    fallback.resolve.mockResolvedValue(null);
    const wrapper = mount(MdApplicationIcon, { props: { platform: 'linuxPackage', size: 30 } });
    await flushPromises();
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.classes()).toContain('linux-icon');
    expect(wrapper.get('.process-icon').attributes('style')).toContain('width: 24px');
    expect(fallback.resolve).toHaveBeenCalledExactlyOnceWith('linuxPackage');
  });

  it('uses the full native canvas for a missing macOS application icon', async () => {
    const wrapper = mount(MdApplicationIcon, { props: { size: 40 } });
    await flushPromises();
    expect(wrapper.get('img').attributes('src')).toBe('data:image/png;base64,native');
    expect(wrapper.get('img').attributes('style')).toContain('width: 40px; height: 40px');
    expect(wrapper.classes()).toContain('macos-icon');
    expect(wrapper.find('svg').exists()).toBe(false);
  });

  it('replaces a broken original image even when the caller retains its URL', async () => {
    const wrapper = mount(MdApplicationIcon, { props: { src: 'broken.png' } });
    await wrapper.get('img').trigger('error');
    await flushPromises();
    expect(wrapper.get('img').attributes('src')).toBe('data:image/png;base64,native');
    expect(wrapper.emitted('error')).toHaveLength(1);

    await wrapper.setProps({ src: 'replacement.png' });
    expect(wrapper.get('img').attributes('src')).toBe('replacement.png');
  });

  it('does not overwrite a newly resolved original with a late fallback response', async () => {
    let resolve!: (value: string) => void;
    fallback.resolve.mockReturnValue(new Promise<string>(done => (resolve = done)));
    const wrapper = mount(MdApplicationIcon);
    await wrapper.setProps({ src: 'new-original.png' });
    resolve('data:image/png;base64,late');
    await flushPromises();
    expect(wrapper.get('img').attributes('src')).toBe('new-original.png');
  });

  it('does not loop or emit an application failure when the native fallback fails', async () => {
    const wrapper = mount(MdApplicationIcon);
    await flushPromises();
    await wrapper.get('img').trigger('error');
    await flushPromises();
    expect(wrapper.find('img').exists()).toBe(false);
    expect(fallback.resolve).toHaveBeenCalledTimes(1);
    expect(wrapper.emitted('error')).toBeUndefined();
  });

  it('uses the native Windows fallback when the original image fails', async () => {
    const wrapper = mount(MdApplicationIcon, { props: { platform: 'windowsRegistry', src: 'broken.ico' } });
    await wrapper.get('img').trigger('error');
    await flushPromises();
    expect(wrapper.get('img').attributes('src')).toBe('data:image/png;base64,native');
    expect(wrapper.find('svg').exists()).toBe(false);
    expect(wrapper.classes()).not.toContain('macos-icon');
    expect(fallback.resolve).toHaveBeenCalledExactlyOnceWith('windowsRegistry');
  });

  it('paints a cached Windows fallback immediately and ignores unrelated updates', async () => {
    fallback.peek.mockReturnValue('data:image/png;base64,cached-windows');
    const wrapper = mount(MdApplicationIcon, { props: { platform: 'windowsRegistry', size: 30 } });
    expect(wrapper.get('img').attributes('src')).toBe('data:image/png;base64,cached-windows');
    await wrapper.setProps({ size: 32 });
    expect(fallback.resolve).toHaveBeenCalledTimes(1);
  });

  it('discards a pending fallback when the platform changes', async () => {
    let resolve!: (value: string) => void;
    fallback.resolve.mockImplementation((platform: string) =>
      platform === 'macosBundle'
        ? new Promise<string>(done => (resolve = done))
        : Promise.resolve('data:image/png;base64,windows')
    );
    const wrapper = mount(MdApplicationIcon, { props: { platform: 'macosBundle' } });
    await wrapper.setProps({ platform: 'windowsRegistry' });
    resolve('data:image/png;base64,late-mac');
    await flushPromises();
    expect(wrapper.get('img').attributes('src')).toBe('data:image/png;base64,windows');
  });
});

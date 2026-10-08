// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';
import Trend from './md-resource-trend.vue';

const point = (sampledAtMs: number) => ({ sampledAtMs, primary: 50, secondary: null });
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('resource trend rendering', () => {
  it('bounds frame callbacks on high-refresh displays while preserving live scrolling', async () => {
    vi.useFakeTimers();
    let clock = 0;
    const pending: FrameRequestCallback[] = [];
    const request = vi.fn((callback: FrameRequestCallback) => {
      pending.push(callback);
      return 1;
    });
    vi.spyOn(performance, 'now').mockImplementation(() => clock);
    vi.stubGlobal('requestAnimationFrame', request);
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
    const wrapper = mount(Trend, {
      props: { metric: 'cpu', label: 'CPU', observedAtMs: 60000, history: [point(58000), point(60000)] },
    });
    await nextTick();
    let paints = 0;
    let previous = wrapper.get('svg > g').attributes('transform');
    for (let n = 1; n <= 120; n++) {
      clock = (n * 1000) / 120;
      vi.advanceTimersByTime(1000 / 120);
      const callback = pending.shift();
      callback?.(clock);
      await nextTick();
      const transform = wrapper.get('svg > g').attributes('transform');
      if (transform !== previous) paints++;
      previous = transform;
    }
    expect(request.mock.calls.length).toBeLessThanOrEqual(31);
    expect(paints).toBeGreaterThanOrEqual(24);
    expect(paints).toBeLessThanOrEqual(31);
    await wrapper.setProps({ active: false });
    const requests = request.mock.calls.length;
    vi.advanceTimersByTime(1000);
    expect(request).toHaveBeenCalledTimes(requests);
    wrapper.unmount();
  });

  it('cancels native-hidden animation and resumes from updated history', async () => {
    const request = vi.fn(() => 1);
    const cancel = vi.fn();
    vi.stubGlobal('requestAnimationFrame', request);
    vi.stubGlobal('cancelAnimationFrame', cancel);
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(false);
    const wrapper = mount(Trend, {
      props: { metric: 'cpu', label: 'CPU', active: false, observedAtMs: 60000, history: [point(58000), point(60000)] },
    });
    await nextTick();
    expect(request).not.toHaveBeenCalled();
    await wrapper.setProps({ active: true });
    await nextTick();
    expect(request).toHaveBeenCalledOnce();
    await wrapper.setProps({ active: false });
    expect(cancel).toHaveBeenCalledWith(1);
    request.mockClear();
    await wrapper.setProps({ observedAtMs: 62000, history: [point(60000), point(62000)] });
    await nextTick();
    expect(request).not.toHaveBeenCalled();
    await wrapper.setProps({ active: true });
    await nextTick();
    expect(request).toHaveBeenCalledOnce();
    expect(wrapper.get('path[stroke]').attributes('d')).not.toContain('NaN');
    wrapper.unmount();
  });

  it('animates only the scale group when a new rate peak changes the range', async () => {
    let clock = 0;
    let animate: FrameRequestCallback = () => {};
    vi.spyOn(performance, 'now').mockImplementation(() => clock);
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      animate = callback;
      return 1;
    });
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
    const old = { sampledAtMs: 60000, primary: 64, secondary: 32 };
    const wrapper = mount(Trend, {
      props: { metric: 'network', label: 'Network', observedAtMs: 60000, history: [old] },
    });
    clock = 1000;
    await wrapper.setProps({
      observedAtMs: 61000,
      history: [old, { sampledAtMs: 61000, primary: 4096, secondary: 2048 }],
    });
    const path = wrapper.get('path[stroke]');
    const geometry = path.attributes('d');
    expect(wrapper.get('svg > g').attributes('transform')).toContain('scale(1 64)');
    clock = 1300;
    animate(clock);
    await nextTick();
    expect(wrapper.get('svg > g').attributes('transform')).toContain('scale(1 32.5)');
    expect(path.attributes('d')).toBe(geometry);
    clock = 1600;
    animate(clock);
    await nextTick();
    expect(wrapper.get('svg > g').attributes('transform')).toContain('scale(1 1)');
    expect(path.attributes('d')).toBe(geometry);
    wrapper.unmount();
  });

  it('leaves invalid or missing intervals blank and stops after the buffered tail exits', () => {
    vi.useFakeTimers();
    let clock = 0;
    let animate: FrameRequestCallback = () => {};
    const request = vi.fn((callback: FrameRequestCallback) => {
      animate = callback;
      return 1;
    });
    vi.spyOn(performance, 'now').mockImplementation(() => clock);
    vi.stubGlobal('requestAnimationFrame', request);
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
    const wrapper = mount(Trend, {
      props: {
        metric: 'network',
        label: 'Network',
        observedAtMs: 60000,
        history: [point(0), { ...point(1000), primary: NaN }, point(2000), point(60000)],
      },
    });
    const geometry = wrapper.get('path[stroke]').attributes('d')!;
    expect(geometry.match(/M/g)).toHaveLength(3);
    expect(geometry).not.toContain('NaN');
    clock = 61200;
    animate(clock);
    vi.advanceTimersByTime(34);
    expect(request).toHaveBeenCalledTimes(2);
    clock = 61251;
    animate(clock);
    expect(request).toHaveBeenCalledTimes(2);
    wrapper.unmount();
  });

  it('moves only the shared strip and retains native-pruned edge segments until they leave the viewport', async () => {
    let clock = 0;
    let animate: FrameRequestCallback = () => {};
    vi.spyOn(performance, 'now').mockImplementation(() => clock);
    vi.stubGlobal(
      'requestAnimationFrame',
      vi.fn((callback: FrameRequestCallback) => {
        animate = callback;
        return 1;
      })
    );
    const cancel = vi.fn();
    vi.stubGlobal('cancelAnimationFrame', cancel);
    const wrapper = mount(Trend, {
      props: { metric: 'cpu', label: 'CPU', observedAtMs: 60_000, history: [point(0), point(2000), point(60_000)] },
    });
    const first = wrapper.get('path[stroke]').element;
    clock = 500;
    animate(clock);
    await nextTick();
    expect(wrapper.get('svg > g').attributes('transform')).toContain('translate(2.9166666666666665 50)');
    expect(wrapper.get('.trend-strip').attributes('style')).toBeUndefined();
    clock = 1000;
    await wrapper.setProps({ observedAtMs: 61_000, history: [point(2000), point(60_000)] });
    expect(wrapper.get('path[stroke]').element).toBe(first);
    expect(wrapper.get('path[stroke]').attributes('d')).toContain('M0,50');
    clock = 2000;
    await wrapper.setProps({ observedAtMs: 62_000, history: [point(2000), point(60_000), point(62_000)] });
    expect(wrapper.get('path[stroke]').attributes('d')).toContain('M0,50');
    for (clock = 3000; clock <= 8000; clock += 1000) {
      await wrapper.setProps({ observedAtMs: 60000 + clock, history: [point(60000 + clock)] });
    }
    expect(wrapper.get('path[stroke]').attributes('d')).not.toContain('M0,50');
    wrapper.unmount();
    expect(cancel).toHaveBeenCalled();
  });

  it('draws healthy idle disk samples as one continuous zero line', () => {
    const wrapper = mount(Trend, {
      props: {
        metric: 'disk',
        label: 'Disk',
        observedAtMs: 60000,
        history: Array.from({ length: 31 }, (_, i) => ({
          sampledAtMs: i * 2000,
          primary: 0,
          secondary: 0,
        })),
      },
    });
    for (const path of wrapper.findAll('path[stroke]')) {
      const geometry = path.attributes('d')!;
      expect(geometry.match(/M/g)).toHaveLength(1);
      expect(geometry.match(/ L/g)).toHaveLength(30);
      expect(geometry).not.toContain('NaN');
    }
    wrapper.unmount();
  });

  it.each(['network', 'disk'] as const)(
    'keeps %s reads/download below and writes/upload above one shared zero',
    metric => {
      const wrapper = mount(Trend, {
        props: {
          metric,
          label: 'I/O',
          observedAtMs: 60000,
          history: [{ sampledAtMs: 60000, primary: 64, secondary: 64 }],
        },
      });
      const paths = wrapper.findAll('path[stroke]');
      const ordinate = (path: string) => Number(path.match(/^M[^,]+,([\d.]+)/)?.[1]);
      expect(ordinate(paths[0]!.attributes('d')!)).toBeGreaterThan(50);
      expect(ordinate(paths[1]!.attributes('d')!)).toBeLessThan(50);
      expect(wrapper.findAll('path[fill]')[0]!.attributes('d')).toContain(',50 Z');
      wrapper.unmount();
    }
  );

  it('stops the frame loop when hidden and when reduced motion is requested', () => {
    const request = vi.fn(() => 1);
    const cancel = vi.fn();
    vi.stubGlobal('requestAnimationFrame', request);
    vi.stubGlobal('cancelAnimationFrame', cancel);
    const wrapper = mount(Trend, {
      props: { metric: 'network', label: 'Network', observedAtMs: 60_000, history: [point(60_000)] },
    });
    expect(request).toHaveBeenCalledOnce();
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(true);
    document.dispatchEvent(new Event('visibilitychange'));
    expect(cancel).toHaveBeenCalled();
    expect(request).toHaveBeenCalledOnce();
    wrapper.unmount();

    vi.spyOn(document, 'hidden', 'get').mockReturnValue(false);
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    Object.defineProperty(media, 'matches', { value: true });
    vi.spyOn(window, 'matchMedia').mockReturnValue(media);
    request.mockClear();
    const reduced = mount(Trend, {
      props: { metric: 'cpu', label: 'CPU', observedAtMs: 60_000, history: [point(60_000)] },
    });
    expect(request).not.toHaveBeenCalled();
    reduced.unmount();
  });
});

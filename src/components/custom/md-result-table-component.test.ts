// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';
import { i18n } from '@/i18n';
import MdResultTable from './md-result-table.vue';

function table(synchronousScroll = true) {
  const wrapper = mount(MdResultTable, { props: { synchronousScroll }, global: { plugins: [i18n] } });
  const scroll = wrapper.get('.result-table-scroll').element as HTMLElement;
  Object.defineProperties(scroll, { clientHeight: { value: 600 }, scrollHeight: { value: 10000 } });
  return { wrapper, scroll };
}
function wheel(scroll: HTMLElement, options: WheelEventInit) {
  const event = new WheelEvent('wheel', { cancelable: true, ...options });
  // happy-dom's WheelEvent omits the MouseEvent modifier fields.
  for (const key of ['ctrlKey', 'metaKey', 'shiftKey'] as const) {
    Object.defineProperty(event, key, { value: options[key] ?? false });
  }
  scroll.dispatchEvent(event);
  return event;
}

describe('virtual table wheel input', () => {
  it('notifies virtual rows in the input task and preserves fractional momentum deltas', () => {
    const { wrapper, scroll } = table();
    const offsets: number[] = [];
    scroll.addEventListener('scroll', () => offsets.push(scroll.scrollTop));
    for (const deltaY of [5000, 31.5, 7.25, -1600]) {
      expect(wheel(scroll, { deltaY }).defaultPrevented).toBe(true);
    }
    expect(offsets).toEqual([5000, 5031.5, 5038.75, 3438.75]);
    wrapper.unmount();
  });
  it('handles line and page deltas without overshooting the content', () => {
    const { wrapper, scroll } = table();
    wheel(scroll, { deltaY: 3, deltaMode: 1 });
    expect(scroll.scrollTop).toBe(48);
    wheel(scroll, { deltaY: 2, deltaMode: 2 });
    expect(scroll.scrollTop).toBe(1248);
    wheel(scroll, { deltaY: 99999 });
    expect(scroll.scrollTop).toBe(9400);
    expect(wheel(scroll, { deltaY: 100 }).defaultPrevented).toBe(true);
    wheel(scroll, { deltaY: -99999 });
    expect(scroll.scrollTop).toBe(0);
    wrapper.unmount();
  });
  it('leaves zoom, horizontal gestures and ordinary tables to the browser', () => {
    const { wrapper, scroll } = table();
    for (const options of [{ ctrlKey: true }, { metaKey: true }, { shiftKey: true }, { deltaX: 200 }]) {
      expect(wheel(scroll, { deltaY: 100, ...options }).defaultPrevented).toBe(false);
    }
    expect(scroll.scrollTop).toBe(0);
    wrapper.unmount();
    const ordinary = table(false);
    expect(wheel(ordinary.scroll, { deltaY: 100 }).defaultPrevented).toBe(false);
    ordinary.wrapper.unmount();
  });
});

describe('virtual table scrollbar input', () => {
  it('renders large drag jumps in the same input task and keeps tracking outside the thumb', async () => {
    const { wrapper, scroll } = table();
    await flushPromises();
    scroll.dispatchEvent(new Event('scroll'));
    await flushPromises();
    const track = wrapper.get('[role="scrollbar"]').element as HTMLElement;
    let captured: number | null = null;
    track.setPointerCapture = vi.fn(id => {
      captured = id;
    });
    track.hasPointerCapture = vi.fn(id => captured === id);
    track.releasePointerCapture = vi.fn(() => {
      captured = null;
    });
    const offsets: number[] = [];
    scroll.addEventListener('scroll', () => offsets.push(scroll.scrollTop));
    track.dispatchEvent(new PointerEvent('pointerdown', { button: 0, pointerId: 7, clientY: 10, cancelable: true }));
    expect(captured).toBe(7);
    // Each move jumps far beyond both overscan windows, including reversals
    // and leaving the track. Notifications must precede any animation frame.
    for (const clientY of [599, 10, 350, -200, 800]) {
      track.dispatchEvent(new PointerEvent('pointermove', { pointerId: 7, clientY }));
      expect(offsets.at(-1)).toBe(scroll.scrollTop);
    }
    expect(offsets).toEqual([9400, 0, (340 / 564) * 9400, 0, 9400]);
    track.dispatchEvent(new PointerEvent('pointercancel', { pointerId: 7 }));
    expect(captured).toBeNull();
    track.dispatchEvent(new PointerEvent('pointermove', { pointerId: 7, clientY: 10 }));
    expect(scroll.scrollTop).toBe(9400);
    wrapper.unmount();
  });

  it('supports track clicks and keyboard navigation with an accessible viewport relationship', async () => {
    const { wrapper, scroll } = table();
    await flushPromises();
    scroll.dispatchEvent(new Event('scroll'));
    await flushPromises();
    const bar = wrapper.get('[role="scrollbar"]');
    const track = bar.element as HTMLElement;
    track.setPointerCapture = vi.fn();
    track.hasPointerCapture = vi.fn(() => false);
    expect(bar.attributes('aria-controls')).toBe(scroll.id);
    expect(bar.attributes('aria-valuemax')).toBe('9400');
    track.dispatchEvent(new PointerEvent('pointerdown', { button: 0, pointerId: 1, clientY: 300, cancelable: true }));
    expect(scroll.scrollTop).toBe(4700);
    track.dispatchEvent(new PointerEvent('pointerup', { pointerId: 1 }));
    for (const [key, expected] of [
      ['Home', 0],
      ['ArrowDown', 40],
      ['PageDown', 640],
      ['PageUp', 40],
      ['ArrowUp', 0],
      ['End', 9400],
    ] as const) {
      await bar.trigger('keydown', { key });
      expect(scroll.scrollTop).toBe(expected);
      expect(bar.attributes('aria-valuenow')).toBe(String(expected));
    }
    wrapper.unmount();
  });

  it('attaches and releases the thumb when synchronous scrolling is enabled dynamically', async () => {
    const { wrapper, scroll } = table(false);
    await wrapper.setProps({ synchronousScroll: true });
    await flushPromises();
    const bar = wrapper.get('[role="scrollbar"]');
    expect(bar.attributes('aria-valuemax')).toBe('9400');
    const track = bar.element as HTMLElement;
    track.setPointerCapture = vi.fn();
    track.hasPointerCapture = vi.fn(() => true);
    track.releasePointerCapture = vi.fn();
    track.dispatchEvent(new PointerEvent('pointerdown', { button: 0, pointerId: 9, clientY: 10 }));
    await wrapper.setProps({ synchronousScroll: false });
    expect(track.releasePointerCapture).toHaveBeenCalledWith(9);
    expect(wrapper.find('[role="scrollbar"]').exists()).toBe(false);
    expect(wheel(scroll, { deltaY: 100 }).defaultPrevented).toBe(false);
    wrapper.unmount();
  });

  it('keeps native scrollbars on ordinary tables', () => {
    const { wrapper } = table(false);
    expect(wrapper.find('[role="scrollbar"]').exists()).toBe(false);
    expect(wrapper.get('.result-table-scroll').classes()).toContain('scrollbar-stable');
    wrapper.unmount();
  });
});

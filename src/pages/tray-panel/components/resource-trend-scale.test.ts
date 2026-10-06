import { describe, expect, it } from 'vitest';
import { ResourceTrendScale } from './resource-trend-scale';

describe('resource trend scale', () => {
  it('preserves visible amplitudes at a new peak and settles smoothly', () => {
    const scale = new ResourceTrendScale();
    scale.update(64, 0);
    expect(scale.amplitude(0)).toBe(1);
    scale.update(4096, 1000);
    const height = (clock: number) => (32 / scale.ceiling) * scale.amplitude(clock);
    expect(height(1000)).toBe(0.5);
    expect(height(1300)).toBeGreaterThan(height(1600));
    expect(height(1300)).toBeLessThan(height(1000));
    expect(height(1600)).toBe(32 / 4096);
  });

  it('retargets during a transition without changing the current visual height', () => {
    const scale = new ResourceTrendScale();
    scale.update(64, 0);
    scale.update(256, 1000);
    const inverse = scale.amplitude(1200) / scale.ceiling;
    scale.update(4096, 1200);
    expect(scale.amplitude(1200) / scale.ceiling).toBeCloseTo(inverse);
    expect(scale.amplitude(1800)).toBe(1);
  });

  it('holds peaks and eases down only after the range shrinks substantially', () => {
    const scale = new ResourceTrendScale();
    scale.update(1024, 0);
    scale.update(16, 29000);
    expect(scale.ceiling).toBe(1024);
    scale.update(16, 30000);
    expect(scale.ceiling).toBe(16);
    expect(scale.amplitude(30000)).toBe(16 / 1024);
    expect(scale.amplitude(30600)).toBe(1);
  });

  it('finishes immediately for reduced motion and resets the first real reading', () => {
    const scale = new ResourceTrendScale();
    scale.update(64, 0);
    scale.update(4096, 1000, true);
    expect(scale.amplitude(1000)).toBe(1);
    scale.reset();
    scale.update(1, 2000);
    expect(scale.ceiling).toBe(1);
    expect(scale.amplitude(2000)).toBe(1);
  });
});

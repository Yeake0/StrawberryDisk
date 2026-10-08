import { describe, expect, it } from 'vitest';
import { summarizeGpuHistory } from './gpu-history-summary';

describe('GPU history statistics', () => {
  it('uses only valid actual observations within the minute, preserving real idle values', () => {
    const history = [
      [9999, 100],
      [10000, 0],
      [30000, 30],
      [70000, 60],
      [70001, 100],
      [50000, NaN],
      [50001, -1],
      [50002, 101],
    ].map(([sampledAtMs, primary]) => ({ sampledAtMs: sampledAtMs!, primary: primary!, secondary: null }));
    expect(summarizeGpuHistory(history, 70000)).toEqual({ average: 30, peak: 60 });
  });
  it('does not invent values for an empty or expired history', () => {
    expect(summarizeGpuHistory([], 100000)).toBeNull();
    expect(summarizeGpuHistory([{ sampledAtMs: 0, primary: 50, secondary: null }], 100000)).toBeNull();
  });
});

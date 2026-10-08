import { describe, expect, it } from 'vitest';
import { ResourceTrendTimeline } from './resource-trend-timeline';
const point = (sampledAtMs: number, primary = 50) => ({ sampledAtMs, primary, secondary: null });

describe('continuous resource timeline', () => {
  it('keeps sample coordinates stable across deliveries while the shared strip moves', () => {
    const timeline = new ResourceTrendTimeline();
    timeline.accept([point(58_000), point(60_000)], 60_000, 0, 2000);
    const original = timeline.position(58_000);
    expect(timeline.position(60_000)).toBe(100);
    expect(timeline.offset(500)).toBeCloseTo((1750 / 60000) * 100);
    expect(timeline.offset(1500)).toBeCloseTo((750 / 60000) * 100);
    // A slightly delayed native update cannot restart or reverse the display clock.
    timeline.accept([point(60_000), point(62_000)], 62_000, 2050, 2000);
    expect(timeline.position(58_000)).toBe(original);
    expect(timeline.time(2050)).toBe(62_050);
    expect(timeline.position(62_000) + timeline.offset(2050)).toBeGreaterThan(100);
    expect(timeline.position(60_000) + timeline.offset(2050)).toBeGreaterThan(100);
  });

  it.each([1000, 2000, 3000])('retains a clipped edge sample until its entire %d ms segment exits', interval => {
    const timeline = new ResourceTrendTimeline();
    const next = interval * 2;
    timeline.accept([point(0, 0), point(next, 100), point(60_000)], 60_000, 0, interval);
    for (let clock = interval; clock <= interval * 2; clock += interval) {
      timeline.accept([point(next, 100), point(60000 + clock)], 60000 + clock, clock, interval);
    }
    expect(timeline.points.map(p => p.sampledAtMs)).toContain(0);
    expect(timeline.position(0) + timeline.offset(interval * 2)).toBeLessThan(0);
    expect(timeline.position(next) + timeline.offset(interval * 2)).toBeGreaterThan(0);
    for (let clock = interval * 3; clock <= interval * 4; clock += interval) {
      timeline.accept([point(next, 100), point(60000 + clock)], 60000 + clock, clock, interval);
    }
    expect(timeline.points.map(p => p.sampledAtMs)).not.toContain(0);
    expect(timeline.points[0]!.primary).toBe(100);
  });

  it('does not outrun a delayed sample or jump when it finally arrives', () => {
    const timeline = new ResourceTrendTimeline();
    timeline.accept([point(58000), point(60000)], 60000, 0, 2000);
    for (let clock = 0; clock <= 4800; clock += 16) timeline.offset(clock);
    const before = timeline.offset(4800);
    expect(timeline.position(60000) + before).toBe(100);
    timeline.accept([point(60000), point(63000)], 64800, 4800, 2000);
    expect(timeline.offset(4800)).toBe(before);
    expect(timeline.offset(4816)).toBeCloseTo(before - 17.6 / 600);
    expect(timeline.position(63000) + timeline.offset(4816)).toBeGreaterThan(100);
    // An actual outage is not represented as an indefinitely current flat line.
    expect(timeline.position(63000) + timeline.offset(12000)).toBeLessThan(100);
  });

  it('reopens a complete buffered minute from native offscreen history', () => {
    const history = Array.from({ length: 81 }, (_, i) => point(20000 + i * 1000));
    const timeline = new ResourceTrendTimeline();
    timeline.accept(history, 100000, 0, 3000);
    timeline.reset();
    timeline.accept(history, 100200, 200, 3000);
    expect(timeline.position(timeline.points[0]!.sampledAtMs) + timeline.offset(200)).toBeLessThan(0);
    expect(timeline.position(100000) + timeline.offset(200)).toBeGreaterThan(100);
  });

  it('bounds retained samples and clears stale device history on reset', () => {
    const timeline = new ResourceTrendTimeline();
    timeline.accept(
      Array.from({ length: 1000 }, (_, i) => point(i)),
      1000,
      0,
      1000
    );
    expect(timeline.points).toHaveLength(128);
    timeline.accept([], 1001, 1, 1000);
    expect(timeline.points).toEqual([]);
    timeline.accept([point(90_000)], 90_000, 2, 1000);
    expect(timeline.origin).toBe(90_000);
    expect(timeline.offset(2)).toBeCloseTo((1250 / 60000) * 100);
    timeline.reset();
    expect(timeline.points).toEqual([]);
  });

  it('crosses multiple minute boundaries without restarting the strip', () => {
    const timeline = new ResourceTrendTimeline();
    for (let second = 0; second <= 150; second++) {
      const observed = 60_000 + second * 1000;
      const history = Array.from({ length: 61 }, (_, index) => point(observed - (60 - index) * 1000));
      timeline.accept(history, observed, second * 1000, 1000);
      expect(timeline.origin).toBe(60_000);
      expect(timeline.offset(second * 1000)).toBeCloseTo(((1250 - second * 1000) / 60000) * 100);
      expect(timeline.points.length).toBeLessThanOrEqual(64);
    }
  });

  it('does not invalidate geometry for another metric update but does for a corrected value', () => {
    const timeline = new ResourceTrendTimeline();
    timeline.accept([point(60000)], 60000, 0, 2000);
    const initial = timeline.points;
    timeline.accept([point(60000)], 61000, 1000, 2000);
    expect(timeline.points).toBe(initial);
    timeline.accept([point(60000, 75)], 61001, 1001, 2000);
    expect(timeline.points).not.toBe(initial);
    expect(timeline.points[0]!.primary).toBe(75);
  });

  it('rebases hourly without moving an existing sample on screen', () => {
    const timeline = new ResourceTrendTimeline();
    for (let clock = 0; clock <= 3600000; clock += 4000) {
      timeline.accept([point(60000 + clock)], 60000 + clock, clock, 2000);
    }
    const timestamp = 3660000;
    const position = timeline.position(timestamp) + timeline.offset(3600001);
    const points = timeline.points;
    timeline.accept([point(timestamp)], 3660001, 3600001, 2000);
    expect(timeline.position(timestamp) + timeline.offset(3600001)).toBeCloseTo(position);
    expect(timeline.points).not.toBe(points);
  });
});

import { describe, it, expect } from 'vitest';
import { customGpuActivities, summarizeGpuActivities } from './gpu-activity';
import type { GpuActivity } from '@/lib/models/gpu-details';
describe('GPU engine presentation', () => {
  it('uses the busiest same-kind engine without summing parallel work or including custom engines', () => {
    const values: GpuActivity[] = [
      { id: '0', kind: 'copy', name: null, usedPercent: 40, includedInSummary: true },
      { id: '1', kind: 'copy', name: null, usedPercent: 70, includedInSummary: true },
      { id: '12', kind: 'other', name: 'Graphics_1', usedPercent: 56, includedInSummary: false },
      { id: 'renderer', kind: 'renderer', name: null, usedPercent: 12, includedInSummary: false },
    ];
    expect(summarizeGpuActivities(values).map(value => [value.kind, value.usedPercent])).toEqual([
      ['copy', 70],
      ['renderer', 12],
    ]);
    expect(values[1]!.id).toBe('1');
  });
});

describe('stable GPU engine order', () => {
  const engine = (
    id: string,
    kind: GpuActivity['kind'],
    name: string | null = null,
    includedInSummary = true
  ): GpuActivity => ({ id, kind, name, usedPercent: 0, includedInSummary });
  it('orders only observed standard kinds by function while retaining distinct other nodes', () => {
    const input = [
      engine('node:10', 'other'),
      engine('node:2', 'other'),
      engine('e', 'videoEncode'),
      engine('p', 'videoProcessing'),
      engine('g', 'graphics'),
      engine('d', 'videoDecode'),
      engine('c', 'copy'),
    ];
    const expected = ['graphics', 'copy', 'videoDecode', 'videoEncode', 'videoProcessing', 'node:2', 'node:10'];
    expect(summarizeGpuActivities(input).map(value => value.id)).toEqual(expected);
    expect(summarizeGpuActivities([...input].reverse()).map(value => value.id)).toEqual(expected);
    expect(summarizeGpuActivities([engine('c', 'copy')]).map(value => value.kind)).toEqual(['copy']);
  });
  it('keeps macOS renderer before tiler without treating them as summary inputs', () => {
    expect(
      summarizeGpuActivities([engine('t', 'tiler', null, false), engine('r', 'renderer', null, false)]).map(
        value => value.kind
      )
    ).toEqual(['renderer', 'tiler']);
  });
  it('uses natural native names independent of activity, input order and numeric node IDs', () => {
    const input = ['VR', 'Compute_10', 'Compute_2', 'Compute_1', 'Cuda', 'Graphics_1', 'OFA_0', 'Security'].map(
      (name, index) => engine(`node:${index}`, 'other', name, false)
    );
    const expected = ['Compute_1', 'Compute_2', 'Compute_10', 'Cuda', 'Graphics_1', 'OFA_0', 'Security', 'VR'];
    const original = structuredClone(input);
    expect(customGpuActivities(input).map(value => value.name)).toEqual(expected);
    expect(input).toEqual(original);
    expect(
      customGpuActivities(input.map((value, index) => ({ ...value, usedPercent: index * 10 })).reverse()).map(
        value => value.name
      )
    ).toEqual(expected);
  });
  it('breaks equivalent names with stable natural IDs and excludes standard engines', () => {
    const input = [
      engine('node:10', 'other', 'Compute', false),
      engine('node:2', 'other', 'compute', false),
      engine('node:1', 'graphics', null),
      engine('r', 'renderer', null, false),
      engine('t', 'tiler', null, false),
    ];
    expect(customGpuActivities(input).map(value => value.id)).toEqual(['node:2', 'node:10']);
    expect(
      customGpuActivities([engine('node:10', 'other', '', false), engine('node:2', 'other', null, false)]).map(
        value => value.id
      )
    ).toEqual(['node:2', 'node:10']);
  });
});

import { describe, expect, it } from 'vitest';

import * as AnalysisViewPreferenceUtils from './analysis-view-preference';

describe('analysis display preferences', () => {
  it.each([null, undefined, [], true, 'sunburst', {}, { schemaVersion: 2 }])(
    'uses defaults for missing or unsupported documents: %j',
    value => {
      expect(AnalysisViewPreferenceUtils.parse(value)).toEqual({
        schemaVersion: 1,
        viewMode: 'treemap',
        treemapDepth: 1,
        sunburstDepth: 3,
      });
    }
  );

  it('restores valid fields independently of missing and invalid fields', () => {
    expect(AnalysisViewPreferenceUtils.parse({ schemaVersion: 1, viewMode: 'sunburst', treemapDepth: 6 })).toEqual({
      schemaVersion: 1,
      viewMode: 'sunburst',
      treemapDepth: 6,
      sunburstDepth: 3,
    });
    expect(
      AnalysisViewPreferenceUtils.parse({ schemaVersion: 1, viewMode: 'unknown', treemapDepth: 4, sunburstDepth: 1 })
    ).toEqual({ schemaVersion: 1, viewMode: 'treemap', treemapDepth: 4, sunburstDepth: 3 });
  });

  it.each([0, 7, -1, 2.5, NaN, Infinity, '3', null])('rejects invalid depth %j', value => {
    expect(AnalysisViewPreferenceUtils.isDepth('treemap', value)).toBe(false);
    expect(AnalysisViewPreferenceUtils.isDepth('sunburst', value)).toBe(false);
  });

  it('accepts both depth boundaries and keeps the sunburst minimum at two', () => {
    expect(AnalysisViewPreferenceUtils.isDepth('treemap', 1)).toBe(true);
    expect(AnalysisViewPreferenceUtils.isDepth('treemap', 6)).toBe(true);
    expect(AnalysisViewPreferenceUtils.isDepth('sunburst', 1)).toBe(false);
    expect(AnalysisViewPreferenceUtils.isDepth('sunburst', 2)).toBe(true);
    expect(AnalysisViewPreferenceUtils.isDepth('sunburst', 6)).toBe(true);
  });
});

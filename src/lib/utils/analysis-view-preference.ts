import {
  ANALYSIS_CHART_MAX_DEPTH,
  ANALYSIS_VIEW_IDS,
  ANALYSIS_VIEW_PREFERENCES_SCHEMA_VERSION,
  type AnalysisViewId,
  type AnalysisViewPreferences,
} from '@/lib/models/analysis';

export function defaults(): AnalysisViewPreferences {
  return {
    schemaVersion: ANALYSIS_VIEW_PREFERENCES_SCHEMA_VERSION,
    viewMode: ANALYSIS_VIEW_IDS.treemap,
    treemapDepth: 1,
    sunburstDepth: 3,
  };
}

export function isViewMode(value: unknown): value is AnalysisViewId {
  return value === ANALYSIS_VIEW_IDS.treemap || value === ANALYSIS_VIEW_IDS.sunburst;
}

export function isDepth(viewMode: AnalysisViewId, value: unknown): value is number {
  const minimum = viewMode === ANALYSIS_VIEW_IDS.treemap ? 1 : 2;
  return typeof value === 'number' && Number.isInteger(value) && value >= minimum && value <= ANALYSIS_CHART_MAX_DEPTH;
}

/** Invalid display preferences fall back independently without resetting other settings. */
export function parse(value: unknown): AnalysisViewPreferences {
  const result = defaults();
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return result;
  const record = value as Record<string, unknown>;
  // Unknown versions must not be interpreted as the current display contract.
  if (record.schemaVersion !== ANALYSIS_VIEW_PREFERENCES_SCHEMA_VERSION) return result;
  if (isViewMode(record.viewMode)) result.viewMode = record.viewMode;
  if (isDepth(ANALYSIS_VIEW_IDS.treemap, record.treemapDepth)) result.treemapDepth = record.treemapDepth;
  if (isDepth(ANALYSIS_VIEW_IDS.sunburst, record.sunburstDepth)) result.sunburstDepth = record.sunburstDepth;
  return result;
}

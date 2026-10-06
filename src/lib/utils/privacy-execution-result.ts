import type {
  PrivacyCapabilityState,
  PrivacyExecutionResult,
  PrivacyItem,
  PrivacyScanResult,
} from '@/lib/models/privacy';

const COVERAGE_CAPABILITY_ORDER: readonly PrivacyCapabilityState[] = [
  'schemaUnsupported',
  'permissionRequired',
  'unavailable',
  'unsupported',
  'applicationRunning',
  'browserRunning',
];

/**
 * Applies verified execution evidence to a scan snapshot without pretending to
 * perform another scan. Current Core versions return an authoritative updated
 * snapshot; this deterministic fallback keeps the UI stable for an older
 * backend or an exceptional snapshot-publication failure.
 */
export function applyPrivacyExecutionResult(
  scan: PrivacyScanResult,
  result: PrivacyExecutionResult
): PrivacyScanResult {
  if (result.scan) return result.scan;
  const completedTokens = new Set(
    result.items
      .filter(item => item.verified && (item.status === 'cleared' || item.status === 'unchanged'))
      .map(item => item.token)
  );
  if (!completedTokens.size) return scan;

  const items = scan.items.map(item => (completedTokens.has(item.token) ? emptiedItem(item) : item));
  const coverage = scan.coverage.map(source => {
    const sourceItems = items.filter(item => item.sourceId === source.sourceId);
    const itemCount = sourceItems.reduce((total, item) => total + item.itemCount, 0);
    return {
      ...source,
      capability: coverageCapability(
        sourceItems.map(item => item.capability),
        itemCount
      ),
      itemCount,
    };
  });
  return { ...scan, items, coverage };
}

function emptiedItem(item: PrivacyItem): PrivacyItem {
  return {
    ...item,
    capability: 'empty',
    estimatedBytes: 0,
    itemCount: 0,
    requiresBrowserClose: false,
    selectedByDefault: false,
  };
}

function coverageCapability(
  capabilities: readonly PrivacyCapabilityState[],
  itemCount: number
): PrivacyCapabilityState {
  return (
    COVERAGE_CAPABILITY_ORDER.find(capability => capabilities.includes(capability)) ?? (itemCount ? 'ready' : 'empty')
  );
}

import type { PrivacyDataKind, PrivacyItem } from '@/lib/models/privacy';

export interface PrivacySelectionSummary {
  itemCount: number;
  estimatedBytes: number;
  pendingScanCount: number;
}

export interface PrivacySelectionIdentity {
  sourceId: string;
  profileId: string | null;
  kind: PrivacyDataKind;
}

export type PrivacySelectionMode = 'smart' | 'all' | 'none' | 'manual';

export function isPrivacyItemActionable(item: PrivacyItem): boolean {
  const ownerRunning = item.capability === 'browserRunning' || item.capability === 'applicationRunning';
  return (
    item.recommendation !== 'reviewOnly' &&
    item.recommendation !== 'unsupported' &&
    (item.capability === 'ready' || ownerRunning) &&
    // A running application can exclusively lock its storage. A zero count in this typed state
    // means “scan after close”, not “no traces”; selection drives the close-and-rescan workflow.
    (item.itemCount > 0 || ownerRunning)
  );
}

export function initialPrivacySelection(items: PrivacyItem[]): string[] {
  return recommendedPrivacyTokens(items);
}

export function recommendedPrivacyTokens(items: readonly PrivacyItem[]): string[] {
  return items.filter(item => item.selectedByDefault && isPrivacyItemActionable(item)).map(item => item.token);
}

export function actionablePrivacyTokens(items: readonly PrivacyItem[]): string[] {
  return items.filter(isPrivacyItemActionable).map(item => item.token);
}

/**
 * Derives the current quick-selection mode from opaque scan tokens. Exact set comparison keeps a
 * manual selection visible even when its trace count happens to equal a predefined mode.
 */
export function privacySelectionMode(
  items: readonly PrivacyItem[],
  selectedTokens: readonly string[]
): PrivacySelectionMode {
  const selected = new Set(selectedTokens);
  if (!selected.size) return 'none';
  const matches = (tokens: readonly string[]) => {
    const expected = new Set(tokens);
    return selected.size === expected.size && tokens.every(token => selected.has(token));
  };
  if (matches(recommendedPrivacyTokens(items))) return 'smart';
  if (matches(actionablePrivacyTokens(items))) return 'all';
  return 'manual';
}

export function summarizePrivacySelection(items: PrivacyItem[], tokens: Iterable<string>): PrivacySelectionSummary {
  const selected = new Set(tokens);
  return items.reduce<PrivacySelectionSummary>(
    (summary, item) => {
      if (!selected.has(item.token)) return summary;
      summary.itemCount += item.itemCount;
      summary.estimatedBytes += item.estimatedBytes;
      if ((item.capability === 'browserRunning' || item.capability === 'applicationRunning') && item.itemCount === 0) {
        summary.pendingScanCount += 1;
      }
      return summary;
    },
    { itemCount: 0, estimatedBytes: 0, pendingScanCount: 0 }
  );
}

/**
 * Captures a selection without retaining scan-bound opaque tokens. Browser
 * shutdown can flush profile databases, so the close workflow must rescan and
 * restore intent by stable source/profile/kind identity before it prepares a
 * fresh destructive plan.
 */
export function capturePrivacySelection(
  items: readonly PrivacyItem[],
  tokens: Iterable<string>
): PrivacySelectionIdentity[] {
  const selected = new Set(tokens);
  return items
    .filter(item => selected.has(item.token))
    .map(item => ({ sourceId: item.sourceId, profileId: item.profileId, kind: item.kind }));
}

export function restorePrivacySelection(
  items: readonly PrivacyItem[],
  identities: readonly PrivacySelectionIdentity[],
  excludedSourceIds: Iterable<string> = []
): string[] {
  const expected = new Set(identities.map(selectionIdentityKey));
  const excluded = new Set(excludedSourceIds);
  return items
    .filter(
      item => !excluded.has(item.sourceId) && expected.has(selectionIdentityKey(item)) && isPrivacyItemActionable(item)
    )
    .map(item => item.token);
}

function selectionIdentityKey(value: PrivacySelectionIdentity): string {
  return JSON.stringify([value.sourceId, value.profileId, value.kind]);
}

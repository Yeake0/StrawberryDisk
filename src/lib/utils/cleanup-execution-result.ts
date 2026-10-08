import type { CleanupResult, CleanupScanResult, CleanupSourceSelection, ScanRuleResult } from '@/lib/models/cleanup';
export function completedRuleIds(result: CleanupResult): Set<string> {
  const statusesByRule = new Map<string, CleanupResult['actions'][number]['status'][]>();
  for (const action of result.actions) {
    const statuses = statusesByRule.get(action.ruleId) ?? [];
    statuses.push(action.status);
    statusesByRule.set(action.ruleId, statuses);
  }
  return new Set(
    [...statusesByRule]
      .filter(([, statuses]) => statuses.length > 0 && statuses.every(status => status === 'completed'))
      .map(([ruleId]) => ruleId)
  );
}
export function apply(
  scan: CleanupScanResult,
  result: CleanupResult,
  sourceSelections: readonly CleanupSourceSelection[]
): CleanupScanResult {
  if (result.dryRun) return scan;
  const completedIds = completedRuleIds(result);
  const effectsByRule = new Map<
    string,
    {
      affectedItemCount: number;
      releasedBytes: number;
    }
  >();
  for (const action of result.actions) {
    const effect = effectsByRule.get(action.ruleId) ?? { affectedItemCount: 0, releasedBytes: 0 };
    effect.affectedItemCount += action.affectedItemCount;
    effect.releasedBytes += action.releasedBytes;
    effectsByRule.set(action.ruleId, effect);
  }
  if (!completedIds.size && ![...effectsByRule.values()].some(hasVerifiedEffect)) return scan;
  const selectionsByRule = new Map(sourceSelections.map(selection => [selection.ruleId, selection]));
  let changed = false;
  const rules = scan.rules.map(rule => {
    if (completedIds.has(rule.ruleId)) {
      changed = true;
      return remainingRule(rule, selectionsByRule.get(rule.ruleId));
    }
    const effect = effectsByRule.get(rule.ruleId);
    if (!effect || !hasVerifiedEffect(effect)) return rule;
    changed = true;
    return remainingPartialRule(rule, effect);
  });
  if (!changed) return scan;
  return {
    ...scan,
    rules,
    safeBytes: rules
      .filter(rule => rule.selectable && rule.risk === 'safe')
      .reduce((total, rule) => total + rule.bytes, 0),
    reclaimableBytes: rules.filter(rule => rule.selectable).reduce((total, rule) => total + rule.bytes, 0),
  };
}
export function invalidatedSourceRuleIds(result: CleanupResult): Set<string> {
  const completedIds = completedRuleIds(result);
  return new Set(
    result.actions
      .filter(
        action =>
          !completedIds.has(action.ruleId) &&
          hasVerifiedEffect({
            affectedItemCount: action.affectedItemCount,
            releasedBytes: action.releasedBytes,
          })
      )
      .map(action => action.ruleId)
  );
}
function remainingRule(rule: ScanRuleResult, selection: CleanupSourceSelection | undefined): ScanRuleResult {
  if (!selection) return cleanedRule(rule);
  const selectedPaths = new Set(selection.paths);
  const sources =
    selection.mode === 'include'
      ? rule.sources.filter(source => !selectedPaths.has(source.path))
      : rule.sources.filter(source => selectedPaths.has(source.path));
  if (!sources.length) return cleanedRule(rule);
  return {
    ...rule,
    bytes: sources.reduce((total, source) => total + source.bytes, 0),
    fileCount: sources.reduce((total, source) => total + source.fileCount, 0),
    sources,
    sourceCount: sources.length,
    selectable: true,
    status: rule.requiresAppClose ? 'requiresClose' : 'found',
  };
}
function remainingPartialRule(
  rule: ScanRuleResult,
  effect: {
    affectedItemCount: number;
    releasedBytes: number;
  }
): ScanRuleResult {
  const bytes = Math.max(0, rule.bytes - effect.releasedBytes);
  const fileCount = Math.max(0, rule.fileCount - effect.affectedItemCount);
  if (bytes === 0 && fileCount === 0) return cleanedRule(rule);
  return {
    ...rule,
    bytes,
    fileCount,
    // Core verifies the aggregate effect, but a partial result cannot prove
    // which source paths survived. Remove stale path rows so retrying the
    // whole rule remains safe and the UI never presents deleted locations.
    sources: [],
    sourceCount: 0,
    sourcesTruncated: true,
    selectable: true,
    status: rule.requiresAppClose ? 'requiresClose' : 'found',
  };
}
function hasVerifiedEffect(effect: { affectedItemCount: number; releasedBytes: number }): boolean {
  return effect.affectedItemCount > 0 || effect.releasedBytes > 0;
}
function cleanedRule(rule: ScanRuleResult): ScanRuleResult {
  return {
    ...rule,
    bytes: 0,
    fileCount: 0,
    sources: [],
    sourceCount: 0,
    selectable: false,
    status: 'clean',
  };
}

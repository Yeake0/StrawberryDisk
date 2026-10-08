import type {
  StartupArtifact,
  StartupChangePlan,
  StartupDesiredState,
  StartupOwnerGroup,
  StartupSourceCoverage,
} from '@/lib/models/startup';

export type StartupManageableState = 'enabled' | 'disabled' | 'mixed' | 'unknown';
export type StartupStateFilter = 'all' | 'enabled' | 'disabled' | 'leftover';
export type StartupStartTiming = 'boot' | 'userLogon' | 'background' | 'automatic';
export type StartupManualCleanupTool = 'services' | 'taskScheduler';

export interface StartupFilterCounts {
  all: number;
  enabled: number;
  disabled: number;
  leftover: number;
}

export function indexStartupArtifacts(artifacts: readonly StartupArtifact[]): ReadonlyMap<string, StartupArtifact> {
  return new Map(artifacts.map(artifact => [artifact.itemId, artifact]));
}

export function artifactsForStartupGroup(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupArtifact[] {
  return group.itemIds.flatMap(itemId => {
    const artifact = artifactsById.get(itemId);
    return artifact ? [artifact] : [];
  });
}

export function canManageStartupArtifact(artifact: StartupArtifact): boolean {
  return (
    (artifact.controlCapability === 'toggleable' || artifact.controlCapability === 'elevationRequired') &&
    (artifact.configuredState === 'enabled' || artifact.configuredState === 'disabled')
  );
}

export function manageableArtifactsForGroup(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupArtifact[] {
  return artifactsForStartupGroup(group, artifactsById).filter(canManageStartupArtifact);
}

export function isRemovableOrphanStartupArtifact(artifact: StartupArtifact): boolean {
  return artifact.removableOrphan;
}

export function supportsStartupRemoval(artifact: StartupArtifact): boolean {
  return artifact.removalSupported;
}

export function isStaleMacOsLoginRecord(artifact: StartupArtifact): boolean {
  return (
    (artifact.sourceKind === 'backgroundTask' || artifact.sourceKind === 'loginItem') &&
    artifact.diagnostics.includes('missingTarget')
  );
}

export function isManualCleanupStartupArtifact(artifact: StartupArtifact): boolean {
  return (
    artifact.diagnostics.includes('missingTarget') &&
    !artifact.removalSupported &&
    (artifact.controlCapability !== 'systemManaged' || isStaleMacOsLoginRecord(artifact)) &&
    artifact.controlCapability !== 'policyManaged'
  );
}

export function isLeftoverStartupArtifact(artifact: StartupArtifact): boolean {
  return isRemovableOrphanStartupArtifact(artifact) || isManualCleanupStartupArtifact(artifact);
}

export function manualCleanupToolForStartupArtifacts(
  artifacts: readonly StartupArtifact[]
): StartupManualCleanupTool | null {
  // Ordinary read-only services retain a system-tool exit. Protected services
  // are not permission failures and must not suggest an editable alternative.
  const sourceKinds = new Set(
    artifacts
      .filter(
        artifact =>
          isManualCleanupStartupArtifact(artifact) ||
          (artifact.sourceKind === 'service' &&
            artifact.controlCapability !== 'systemManaged' &&
            !canManageStartupArtifact(artifact) &&
            !supportsStartupRemoval(artifact))
      )
      .map(artifact => artifact.sourceKind)
  );
  if (sourceKinds.size !== 1) return null;
  if (sourceKinds.has('service')) return 'services';
  if (sourceKinds.has('scheduledTask')) return 'taskScheduler';
  return null;
}

export function removableOrphanArtifactsForGroup(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupArtifact[] {
  return artifactsForStartupGroup(group, artifactsById).filter(isRemovableOrphanStartupArtifact);
}

export function removableStartupArtifactsForGroup(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupArtifact[] {
  return artifactsForStartupGroup(group, artifactsById).filter(supportsStartupRemoval);
}

export function isInformativeReadOnlyStartupArtifact(artifact: StartupArtifact): boolean {
  // Keep read-only service details available. Group visibility separately hides
  // protected/system entries by default without granting mutation capabilities.
  if (
    artifact.sourceKind === 'service' &&
    (artifact.controlCapability === 'viewOnly' || artifact.controlCapability === 'systemManaged')
  )
    return true;
  return (
    artifact.sourceKind === 'backgroundTask' &&
    artifact.target.kind === 'application' &&
    !artifact.diagnostics.includes('missingTarget') &&
    (artifact.configuredState === 'enabled' || artifact.configuredState === 'disabled')
  );
}

export function displayedArtifactsForGroup(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupArtifact[] {
  return artifactsForStartupGroup(group, artifactsById).filter(
    artifact =>
      canManageStartupArtifact(artifact) ||
      supportsStartupRemoval(artifact) ||
      isRemovableOrphanStartupArtifact(artifact) ||
      isManualCleanupStartupArtifact(artifact) ||
      isInformativeReadOnlyStartupArtifact(artifact)
  );
}

export function startupRevealPath(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): string | null {
  const artifacts = displayedArtifactsForGroup(group, artifactsById);
  for (const artifact of artifacts) {
    if (artifact.configurationPath) return artifact.configurationPath;
  }
  const artifactsWithTargets = artifacts.filter(artifact => !artifact.diagnostics.includes('missingTarget'));
  if (group.iconPath && artifactsWithTargets.length) return group.iconPath;
  for (const artifact of artifactsWithTargets) {
    if (artifact.target.path) return artifact.target.path;
  }
  return null;
}

export function isDefaultStartupGroup(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): boolean {
  return !isSystemStartupGroup(group, artifactsById) && displayedArtifactsForGroup(group, artifactsById).length > 0;
}

function isSystemStartupGroup(group: StartupOwnerGroup, artifactsById: ReadonlyMap<string, StartupArtifact>): boolean {
  const artifacts = artifactsForStartupGroup(group, artifactsById);
  // Protected services may live outside the Windows directory. Do not infer
  // their identity from paths or hide unrelated third-party read-only entries.
  return (
    group.systemItem ||
    (artifacts.length > 0 &&
      artifacts.every(artifact => artifact.sourceKind === 'service' && artifact.controlCapability === 'systemManaged'))
  );
}

export function displayedStartupGroups(
  groups: readonly StartupOwnerGroup[],
  artifactsById: ReadonlyMap<string, StartupArtifact>,
  showSystemItems = false
): StartupOwnerGroup[] {
  return groups.filter(group =>
    showSystemItems
      ? displayedArtifactsForGroup(group, artifactsById).length > 0
      : isDefaultStartupGroup(group, artifactsById)
  );
}

export function manageableState(artifacts: readonly StartupArtifact[]): StartupManageableState {
  const known = artifacts.filter(
    artifact => artifact.configuredState === 'enabled' || artifact.configuredState === 'disabled'
  );
  if (!known.length || known.length !== artifacts.length) return 'unknown';
  const enabled = known.filter(artifact => artifact.configuredState === 'enabled').length;
  if (enabled === 0) return 'disabled';
  if (enabled === known.length) return 'enabled';
  return 'mixed';
}

export function startupGroupManageableState(
  group: StartupOwnerGroup,
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupManageableState {
  const manageable = manageableArtifactsForGroup(group, artifactsById);
  return manageableState(manageable.length ? manageable : displayedArtifactsForGroup(group, artifactsById));
}

export function startupFilterCounts(
  groups: readonly StartupOwnerGroup[],
  artifactsById: ReadonlyMap<string, StartupArtifact>
): StartupFilterCounts {
  let enabled = 0;
  let disabled = 0;
  let leftover = 0;
  for (const group of groups) {
    const state = startupGroupManageableState(group, artifactsById);
    if (state === 'enabled') enabled += 1;
    if (state === 'disabled') disabled += 1;
    if (artifactsForStartupGroup(group, artifactsById).some(isLeftoverStartupArtifact)) leftover += 1;
  }
  return { all: groups.length, enabled, disabled, leftover };
}

export function filterAndSortStartupGroups(
  groups: readonly StartupOwnerGroup[],
  artifactsById: ReadonlyMap<string, StartupArtifact>,
  query: string,
  stateFilter: StartupStateFilter,
  locale: string
): StartupOwnerGroup[] {
  const normalizedQuery = query.trim().toLocaleLowerCase(locale);
  return groups
    .filter(group => {
      const state = startupGroupManageableState(group, artifactsById);
      if (
        stateFilter === 'leftover'
          ? !artifactsForStartupGroup(group, artifactsById).some(isLeftoverStartupArtifact)
          : stateFilter !== 'all' && state !== stateFilter
      )
        return false;
      if (!normalizedQuery) return true;
      const artifactValues = artifactsForStartupGroup(group, artifactsById).flatMap(artifact => [
        artifact.displayName,
        artifact.target.executableName ?? '',
      ]);
      return [group.name, group.publisher ?? '', group.summary ?? '', ...artifactValues].some(value =>
        value.toLocaleLowerCase(locale).includes(normalizedQuery)
      );
    })
    .sort((left, right) => left.name.localeCompare(right.name, locale));
}

export function nextStartupDesiredState(
  state: StartupManageableState | StartupArtifact['configuredState']
): StartupDesiredState {
  return state === 'disabled' ? 'enabled' : 'disabled';
}

export function startupArtifactRevealPath(artifact: StartupArtifact): string | null {
  if (artifact.configurationPath) return artifact.configurationPath;
  return artifact.diagnostics.includes('missingTarget') ? null : artifact.target.path;
}

export function startupGroupSubtitle(group: StartupOwnerGroup): string | null {
  if (group.summary) return group.summary;
  if (group.publisher && !/^[A-Z0-9]{10}$/.test(group.publisher)) return group.publisher;
  return null;
}

export function startupGroupStartTiming(group: StartupOwnerGroup): StartupStartTiming {
  if (group.triggers.includes('boot')) return 'boot';
  if (group.triggers.includes('userLogon')) return 'userLogon';
  if (group.triggers.includes('keepAlive')) return 'background';
  return 'automatic';
}

export function startupPlanRequiresReview(
  plan: StartupChangePlan,
  requestedItemCount: number,
  groupedChangeRequiresReview = false
): boolean {
  if (plan.desiredState === 'removed') return true;
  if (groupedChangeRequiresReview) return true;
  if (plan.items.length !== requestedItemCount || plan.skippedItems.length > 0) return true;
  return plan.items.some(item => item.warnings.includes('affectsOtherTriggers'));
}

export function needsBackgroundTaskPermission(isMacOs: boolean, coverage: readonly StartupSourceCoverage[]): boolean {
  return (
    isMacOs &&
    coverage.some(
      source =>
        source.sourceId === 'macos.background_tasks' &&
        source.status === 'unavailable' &&
        source.reason === 'accessDenied'
    )
  );
}

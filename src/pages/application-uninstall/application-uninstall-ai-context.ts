import type { AiContext, AiSubject } from '@/lib/models/ai';
import type {
  ApplicationIdentityMetadata,
  ApplicationUninstallCandidate,
  ApplicationUninstallScanResult,
} from '@/lib/models/application';
import { applicationCanRemoveRecord } from './application-uninstall-catalog';
import { defaultApplicationComponentIds } from './application-uninstall-selection';

export function applicationUninstallAiContext(
  candidate: ApplicationUninstallCandidate,
  catalog: Pick<ApplicationUninstallScanResult, 'executionSupported' | 'catalogActionable'>,
  platform: AiContext['platform'],
  selectedComponentIds?: readonly string[],
  identity?: ApplicationIdentityMetadata
): AiContext {
  const selected = new Set(selectedComponentIds ?? defaultApplicationComponentIds(candidate));
  // Group equal facts without exposing component IDs, private data paths or installer commands.
  // Selected and retained data remain separate even when their kinds and risks match.
  const components = new Map<string, Extract<AiSubject, { module: 'applicationUninstall' }>['components'][number]>();
  for (const component of candidate.components) {
    const included = selected.has(component.componentId);
    const key = `${component.kind}:${component.risk}:${included}`;
    const existing = components.get(key);
    if (existing) existing.bytes += component.bytes;
    else
      components.set(key, { kind: component.kind, risk: component.risk, bytes: component.bytes, selected: included });
  }
  return {
    schemaVersion: 2,
    platform,
    title: candidate.name,
    description: '',
    subject: {
      module: 'applicationUninstall',
      ...(identity
        ? { identity }
        : candidate.platform === 'macosBundle' &&
            /^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/.test(candidate.primaryIdentifier)
          ? {
              identity: {
                platform: 'macos',
                bundleIdentifier: candidate.primaryIdentifier,
                productName: null,
                category: null,
                signing: { kind: 'unavailable', certificateSubject: null, teamIdentifier: null },
              } satisfies ApplicationIdentityMetadata,
            }
          : {}),
      ...(candidate.sourceIdentities.length
        ? { installationSources: [...new Set(candidate.sourceIdentities.map(source => source.source))] }
        : {}),
      platform: candidate.platform,
      publisher: candidate.publisher,
      version: candidate.version,
      applicationPath: candidate.applicationPath,
      capability: candidate.capability,
      recordState: candidate.recordState,
      systemKind: candidate.systemKind,
      installerKind: candidate.installerKind,
      executionMode: candidate.executionMode,
      associatedDataComplete: candidate.associatedDataComplete,
      executionSupported: catalog.executionSupported,
      catalogActionable: catalog.catalogActionable,
      recordRemovalAvailable: applicationCanRemoveRecord(candidate),
      selectionKind: selectedComponentIds === undefined ? 'default' : 'current',
      components: [...components.values()],
    },
  };
}

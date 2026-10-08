import { describe, expect, it } from 'vitest';
import type { ApplicationUninstallCandidate } from '@/lib/models/application';
import fixtures from '../../../tests/fixtures/ai-context-v2.json';
import { applicationUninstallAiContext } from './application-uninstall-ai-context';

const candidate: ApplicationUninstallCandidate = {
  applicationId: 'private-application-id',
  primaryIdentifier: 'private-registration',
  sourceIdentities: [],
  name: 'Example Writer',
  version: '1.0',
  publisher: 'Example',
  systemKind: 'unclassified',
  platform: 'macosBundle',
  installerKind: null,
  executionMode: null,
  capability: 'ready',
  recordState: 'installed',
  uninstallDiagnostic: null,
  applicationPath: '/Applications/Example Writer.app',
  possibleRelatedPaths: ['/private/data'],
  iconPath: '/private/icon',
  runningProcesses: [],
  estimatedBytes: 0,
  lastUsedAtMs: null,
  installedAtMs: null,
  totalBytes: 104858624,
  defaultSelectedBytes: 104857600,
  associatedDataComplete: false,
  components: [
    {
      componentId: 'binary',
      kind: 'applicationBinary',
      risk: 'required',
      path: '/Applications/Example Writer.app',
      bytes: 104857600,
      fileCount: 10,
      defaultSelected: true,
    },
    {
      componentId: 'work',
      kind: 'applicationSupport',
      risk: 'userData',
      path: '/private/local-work',
      bytes: 1024,
      fileCount: 1,
      defaultSelected: false,
    },
  ],
};
const catalog = { executionSupported: true, catalogActionable: true };

describe('application uninstall explanation scope', () => {
  it('retains macOS identity hints and never sends Windows registration keys as bundle identities', () => {
    const mac = applicationUninstallAiContext(
      {
        ...candidate,
        primaryIdentifier: 'com.example.game',
        sourceIdentities: [{ source: 'macosBundle', identifier: 'com.example.game' }],
      },
      catalog,
      'macos'
    );
    expect(mac.subject).toMatchObject({
      identity: { platform: 'macos', bundleIdentifier: 'com.example.game', signing: { kind: 'unavailable' } },
      installationSources: ['macosBundle'],
    });
    const windows = applicationUninstallAiContext(
      { ...candidate, platform: 'windowsRegistry', primaryIdentifier: 'HKEY_PRIVATE_REGISTRATION' },
      catalog,
      'windows'
    );
    expect(windows.subject).not.toHaveProperty('identity');
    expect(JSON.stringify(windows)).not.toContain('HKEY_PRIVATE_REGISTRATION');
  });
  it('matches the shared protocol fixture without executable or data-path metadata', () => {
    const context = applicationUninstallAiContext(candidate, catalog, 'macos');
    expect(context).toEqual(fixtures.find(fixture => fixture.subject.module === 'applicationUninstall'));
    expect(JSON.stringify(context)).not.toContain('/private/');
    expect(JSON.stringify(context)).not.toContain('private-registration');
    expect(JSON.stringify(context)).not.toContain('componentId');
  });

  it('keeps explicit selections distinct from defaults and retains unselected user data', () => {
    const selected = applicationUninstallAiContext(candidate, catalog, 'macos', ['binary', 'work']).subject;
    expect(selected).toMatchObject({
      selectionKind: 'current',
      components: [
        expect.objectContaining({ selected: true }),
        expect.objectContaining({ risk: 'userData', selected: true }),
      ],
    });
    const empty = applicationUninstallAiContext(candidate, catalog, 'macos', []).subject;
    expect(empty).toMatchObject({
      selectionKind: 'current',
      components: [expect.objectContaining({ selected: false }), expect.objectContaining({ selected: false })],
    });
  });

  it('aggregates repeated kinds without merging selected and retained data', () => {
    const extra = { ...candidate.components[1]!, componentId: 'retained-work', bytes: 64 };
    const context = applicationUninstallAiContext(
      { ...candidate, components: [...candidate.components, extra] },
      catalog,
      'macos',
      ['binary', 'work']
    );
    expect(context.subject).toMatchObject({
      components: [
        expect.objectContaining({ kind: 'applicationBinary', selected: true }),
        { kind: 'applicationSupport', risk: 'userData', bytes: 1024, selected: true },
        { kind: 'applicationSupport', risk: 'userData', bytes: 64, selected: false },
      ],
    });
    const defaults = applicationUninstallAiContext(
      { ...candidate, components: [...candidate.components, extra] },
      catalog,
      'macos'
    );
    expect(defaults.subject).toMatchObject({
      components: [expect.anything(), { kind: 'applicationSupport', risk: 'userData', bytes: 1088, selected: false }],
    });
  });

  it('exposes record-only availability independently of catalog uninstall execution', () => {
    const context = applicationUninstallAiContext(
      {
        ...candidate,
        platform: 'windowsRegistry',
        capability: 'viewOnly',
        recordState: 'orphanedRegistration',
        components: [],
      },
      { executionSupported: false, catalogActionable: false },
      'windows'
    );
    expect(context.subject).toMatchObject({
      recordRemovalAvailable: true,
      executionSupported: false,
      catalogActionable: false,
      recordState: 'orphanedRegistration',
      components: [],
    });
    expect(
      applicationUninstallAiContext({ ...candidate, capability: 'protectedApplication' }, catalog, 'macos').subject
    ).toMatchObject({ recordRemovalAvailable: false });
  });
});

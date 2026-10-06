import { describe, expect, it } from 'vitest';

import type { ApplicationUninstallInstallerKind } from '@/lib/models/application';

import { applicationPublisherLabel, applicationSizeHintKey } from './application-uninstall-presentation';

describe('applicationSizeHintKey', () => {
  it('uses the shared package explanation for Windows AppX', () => {
    expect(applicationSizeHintKey('windowsAppx')).toBe('applicationUninstall.windowsAppPackageSizeHint');
  });

  it('uses the general estimate explanation for every other installer kind', () => {
    const installerKinds: Array<ApplicationUninstallInstallerKind | null> = [
      null,
      'windowsMsi',
      'windowsScoop',
      'windowsChocolatey',
      'windowsRegistered',
    ];
    for (const installerKind of installerKinds) {
      expect(applicationSizeHintKey(installerKind)).toBe('applicationUninstall.applicationSizeEstimateHint');
    }
  });
});

describe('applicationPublisherLabel', () => {
  const candidate = {
    installerKind: 'windowsAppx' as const,
    publisher: 'CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US',
    primaryIdentifier: 'Microsoft.Edge.GameAssist',
  };

  it('shows the certificate common name for an AppX publisher fallback', () => {
    expect(applicationPublisherLabel(candidate)).toBe('Microsoft Corporation');
    expect(applicationPublisherLabel({ ...candidate, publisher: 'O=Example, CN=Example\\, Inc., C=US' })).toBe(
      'Example, Inc.'
    );
  });

  it.each([
    ['CN="Example, Inc.", O=Example, C=US', 'Example, Inc.'],
    ['CN="William ""Bill"" Smith", O=Example, C=US', 'William "Bill" Smith'],
    ['O="Example, CN=Other Name", CN=Independent Studio, C=US', 'Independent Studio'],
    ['CN=" Example Studio ", O=Example, C=US', ' Example Studio '],
  ])('reads quoted Windows publisher attributes in %s', (publisher, expected) => {
    expect(applicationPublisherLabel({ ...candidate, publisher })).toBe(expected);
  });

  it.each(['CN="Example, Inc., O=Example', 'CN="Example"suffix, C=US', 'CN=Example,', 'CN=Example, invalid attribute'])(
    'retains the original publisher when the subject is malformed: %s',
    publisher => {
      expect(applicationPublisherLabel({ ...candidate, publisher })).toBe(publisher);
    }
  );

  it('preserves ordinary publishers and non-AppX registry values', () => {
    expect(applicationPublisherLabel({ ...candidate, publisher: 'Microsoft Corporation' })).toBe(
      'Microsoft Corporation'
    );
    expect(applicationPublisherLabel({ ...candidate, installerKind: 'windowsRegistered' })).toBe(candidate.publisher);
    expect(applicationPublisherLabel({ ...candidate, publisher: null })).toBe(candidate.primaryIdentifier);
  });
});

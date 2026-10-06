import { describe, expect, it } from 'vitest';

import { LANGUAGE_IDS, THEME_IDS } from '@/lib/models/settings';
import { DUPLICATE_KEEPER_RULE_IDS } from '@/lib/models/duplicate-file';
import * as AppSettingsUtils from '@/lib/utils/app-settings';
import { BYTE_UNIT_BASES } from '@/lib/utils/format';

describe('AppSettingsUtils', () => {
  it('uses English and a 100 MB large-file threshold by default', () => {
    const settings = AppSettingsUtils.defaults();

    expect(settings.language).toBe(LANGUAGE_IDS.enUS);
    expect(settings.largeFileMinimumBytes).toBe(100 * 1024 * 1024);
    expect(settings.duplicateFileMinimumBytes).toBe(200 * 1024);
  });

  it('uses decimal preset bytes for macOS settings', () => {
    const settings = AppSettingsUtils.defaults(LANGUAGE_IDS.enUS, BYTE_UNIT_BASES.decimal);

    expect(settings.largeFileMinimumBytes).toBe(100_000_000);
    expect(settings.duplicateFileMinimumBytes).toBe(200_000);
  });

  it('normalizes saved binary thresholds to their decimal presets', () => {
    const savedSettings = AppSettingsUtils.defaults(LANGUAGE_IDS.enUS, BYTE_UNIT_BASES.binary);
    const settings = AppSettingsUtils.parse(savedSettings, BYTE_UNIT_BASES.decimal);

    expect(settings.largeFileMinimumBytes).toBe(100_000_000);
    expect(settings.duplicateFileMinimumBytes).toBe(200_000);
    expect(settings.language).toBe(savedSettings.language);
    expect(settings.theme).toBe(savedSettings.theme);
  });

  it('rejects incomplete persisted settings', () => {
    expect(() => AppSettingsUtils.parse({})).toThrow('Invalid app settings document');
  });

  it('shows read-failure alerts by default and accepts the hide choice', () => {
    const settings = AppSettingsUtils.defaults(LANGUAGE_IDS.zhCN);
    expect(settings.hideCleanupReadFailureAlerts).toBe(false);
    expect(AppSettingsUtils.parse(settings)).toEqual(settings);
    expect(
      AppSettingsUtils.parse({ ...settings, hideCleanupReadFailureAlerts: true }).hideCleanupReadFailureAlerts
    ).toBe(true);
  });

  it.each([
    [BYTE_UNIT_BASES.binary, 500 * 1024 * 1024, 10 * 1024 * 1024],
    [BYTE_UNIT_BASES.decimal, 500_000_000, 10_000_000],
  ])(
    'loads released legacy settings without losing preferences with base %s',
    (unitBase, largeBytes, duplicateBytes) => {
      const legacy = {
        language: LANGUAGE_IDS.zhTW,
        theme: THEME_IDS.dark,
        largeFileMinimumBytes: 500 * 1024 * 1024,
        duplicateFileMinimumBytes: 10 * 1024 * 1024,
        duplicateKeeperRule: DUPLICATE_KEEPER_RULE_IDS.newestModified,
      };

      expect(AppSettingsUtils.parse(legacy, unitBase)).toEqual({
        ...legacy,
        hideCleanupReadFailureAlerts: false,
        largeFileMinimumBytes: largeBytes,
        duplicateFileMinimumBytes: duplicateBytes,
      });
      expect(legacy).not.toHaveProperty('hideCleanupReadFailureAlerts');
    }
  );

  it('rejects the obsolete permission alert key', () => {
    const { hideCleanupReadFailureAlerts, ...legacy } = AppSettingsUtils.defaults(LANGUAGE_IDS.zhCN);
    expect(hideCleanupReadFailureAlerts).toBe(false);
    expect(() => AppSettingsUtils.parse({ ...legacy, ignoreCleanupPermissionWarnings: true })).toThrow(
      'Invalid app settings document'
    );
  });

  it('rejects non-boolean read-failure alert preferences', () => {
    expect(() =>
      AppSettingsUtils.parse({ ...AppSettingsUtils.defaults(), hideCleanupReadFailureAlerts: 'true' })
    ).toThrow('Invalid app settings value');
  });

  it.each([undefined, null, 0, 'false'])('rejects an explicitly invalid alert preference %s', value => {
    expect(() =>
      AppSettingsUtils.parse({ ...AppSettingsUtils.defaults(), hideCleanupReadFailureAlerts: value })
    ).toThrow('Invalid app settings value');
  });

  it.each([
    { language: 'unsupported' },
    { theme: 'unsupported' },
    { largeFileMinimumBytes: -1 },
    { duplicateFileMinimumBytes: Number.NaN },
    { duplicateKeeperRule: 'unsupported' },
  ])('still rejects invalid core values in both document formats: %j', invalid => {
    const settings = AppSettingsUtils.defaults();
    const { hideCleanupReadFailureAlerts, ...legacy } = settings;
    expect(hideCleanupReadFailureAlerts).toBe(false);
    for (const document of [legacy, settings]) {
      expect(() => AppSettingsUtils.parse({ ...document, ...invalid })).toThrow('Invalid app settings value');
    }
  });

  it.each(['language', 'theme', 'largeFileMinimumBytes', 'duplicateFileMinimumBytes', 'duplicateKeeperRule'])(
    'still rejects a missing core field %s in both document formats',
    key => {
      const settings = AppSettingsUtils.defaults();
      const { hideCleanupReadFailureAlerts, ...legacy } = settings;
      expect(hideCleanupReadFailureAlerts).toBe(false);
      for (const document of [legacy, settings]) {
        const incomplete: Record<string, unknown> = { ...document };
        delete incomplete[key];
        expect(() => AppSettingsUtils.parse(incomplete)).toThrow('Invalid app settings document');
      }
    }
  );
});

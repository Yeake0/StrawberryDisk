import { afterEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { LANGUAGE_IDS, LANGUAGE_OPTIONS } from '@/lib/models/settings';
import { LanguageService } from '@/lib/services/language-service';
import enUS from '@/locales/en-US.json';
import ptBR from '@/locales/pt-BR.json';

const localeResources = [enUS, ptBR];

function leafEntries(value: unknown, prefix = ''): Array<[string, unknown]> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return [[prefix, value]];
  return Object.entries(value).flatMap(([key, child]) => leafEntries(child, prefix ? `${prefix}.${key}` : key));
}

describe('i18n resources', () => {
  afterEach(() => {
    i18n.global.locale.value = LANGUAGE_IDS.enUS;
    vi.unstubAllGlobals();
  });

  it('keeps Portuguese and English schema keys aligned', () => {
    const englishKeys = new Set(leafEntries(enUS).map(([key]) => key));
    const portugueseKeys = new Set(leafEntries(ptBR).map(([key]) => key));
    expect([...englishKeys].filter(key => !portugueseKeys.has(key))).toEqual([]);
    expect(
      [...portugueseKeys].filter(key => !englishKeys.has(key) && !key.startsWith('cleanupRules.entries.'))
    ).toEqual([]);
  });

  it('preserves interpolation arguments in Portuguese', () => {
    const englishEntries = new Map(leafEntries(enUS));
    const argumentsIn = (value: unknown) =>
      typeof value === 'string' ? [...new Set(value.match(/\{[a-zA-Z][a-zA-Z0-9_]*\}/gu) ?? [])].sort() : [];
    const mismatchedKeys = leafEntries(ptBR)
      .filter(
        ([key, value]) => JSON.stringify(argumentsIn(value)) !== JSON.stringify(argumentsIn(englishEntries.get(key)))
      )
      .map(([key]) => key);
    expect(mismatchedKeys).toEqual([]);
  });

  it('bundles only the selectable languages', () => {
    expect([...i18n.global.availableLocales].sort()).toEqual(LANGUAGE_OPTIONS.map(option => option.id).sort());
    for (const locale of i18n.global.availableLocales) {
      for (const option of LANGUAGE_OPTIONS) expect(i18n.global.te(option.labelKey, locale)).toBe(true);
    }
  });

  it('keeps localized strings and curated rule descriptions complete', () => {
    for (const resource of localeResources) {
      expect(leafEntries(resource).filter(([, value]) => typeof value !== 'string' || !value.trim())).toEqual([]);
      expect(
        Object.entries(resource.cleanupRules.entries)
          .filter(([, rule]) => !rule.name.trim() || !rule.description.trim() || !rule.impact.trim())
          .map(([ruleId]) => ruleId)
      ).toEqual([]);
    }
  });

  it('applies the selected language to the interface', () => {
    const documentStub = { documentElement: { lang: '' } };
    vi.stubGlobal('document', documentStub);
    LanguageService.apply(LANGUAGE_IDS.ptBR);
    expect(i18n.global.locale.value).toBe(LANGUAGE_IDS.ptBR);
    expect(documentStub.documentElement.lang).toBe(LANGUAGE_IDS.ptBR);
    expect(i18n.global.t('common.open')).toBe('Abrir');
  });

  it('detects Portuguese and English and falls back to English', () => {
    expect(LanguageService.resolveSupportedLanguage(['pt-BR'])).toBe(LANGUAGE_IDS.ptBR);
    expect(LanguageService.resolveSupportedLanguage(['pt-PT'])).toBe(LANGUAGE_IDS.ptBR);
    expect(LanguageService.resolveSupportedLanguage(['en-GB'])).toBe(LANGUAGE_IDS.enUS);
    expect(LanguageService.resolveSupportedLanguage(['zh-CN', 'en-US'])).toBe(LANGUAGE_IDS.enUS);
    expect(LanguageService.resolveSupportedLanguage(['ja-JP'])).toBe(LANGUAGE_IDS.enUS);
    expect(LanguageService.resolveSupportedLanguage(['tricky', 'ptolemy'])).toBe(LANGUAGE_IDS.enUS);
  });

  it('uses the Portuguese plural forms', () => {
    i18n.global.locale.value = LANGUAGE_IDS.ptBR;
    expect(i18n.global.t('common.fileCount', { count: 1 }, 1)).toBe('1 arquivo');
    expect(i18n.global.t('common.fileCount', { count: 2 }, 2)).toBe('2 arquivos');
  });
});

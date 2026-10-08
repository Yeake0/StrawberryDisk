import type { DuplicateKeeperRuleId } from './duplicate-file';

export const LANGUAGE_IDS = {
  enUS: 'en-US',
  ptBR: 'pt-BR',
} as const;

export type LanguageId = (typeof LANGUAGE_IDS)[keyof typeof LANGUAGE_IDS];

export const LANGUAGE_OPTIONS = [
  {
    id: LANGUAGE_IDS.ptBR,
    labelKey: 'settings.languageNames.ptBR',
    browserLanguagePrefixes: ['pt'],
  },
  {
    id: LANGUAGE_IDS.enUS,
    labelKey: 'settings.languageNames.enUS',
    browserLanguagePrefixes: ['en'],
  },
] as const satisfies readonly {
  id: LanguageId;
  labelKey: string;
  browserLanguagePrefixes: readonly string[];
}[];

export function isLanguageId(value: unknown): value is LanguageId {
  return typeof value === 'string' && LANGUAGE_OPTIONS.some(option => option.id === value);
}

export const THEME_IDS = {
  system: 'system',
  light: 'light',
  dark: 'dark',
} as const;

export type ThemeId = (typeof THEME_IDS)[keyof typeof THEME_IDS];

export function isThemeId(value: unknown): value is ThemeId {
  return typeof value === 'string' && Object.values(THEME_IDS).some(theme => theme === value);
}

export interface AppSettings {
  hideCleanupReadFailureAlerts: boolean;
  language: LanguageId;
  theme: ThemeId;
  largeFileMinimumBytes: number;
  duplicateFileMinimumBytes: number;
  duplicateKeeperRule: DuplicateKeeperRuleId;
}

import { createI18n } from 'vue-i18n';

import { LANGUAGE_IDS, type LanguageId } from '@/lib/models/settings';
import enUS from '@/locales/modules/en-us';
import ptBR from '@/locales/modules/pt-br';

export type MessageSchema = typeof enUS;
export type SupportedLocale = LanguageId;

/**
 * All locale resources are imported into one message graph so every view can switch languages offline.
 * Their bounded size does not justify asynchronous loading, extra failure states, or switch latency.
 */
export const i18n = createI18n<[MessageSchema], SupportedLocale, false>({
  legacy: false,
  globalInjection: false,
  locale: LANGUAGE_IDS.enUS,
  fallbackLocale: LANGUAGE_IDS.enUS,
  messages: {
    [LANGUAGE_IDS.enUS]: enUS,
    [LANGUAGE_IDS.ptBR]: ptBR,
  },
});

// Locale modules can update while Vue retains the active composer. Replace
// its messages too, so new keys do not appear as raw labels during development.
if (import.meta.hot) {
  import.meta.hot.accept(['./locales/modules/en-us', './locales/modules/pt-br'], updatedModules => {
    const locales = [LANGUAGE_IDS.enUS, LANGUAGE_IDS.ptBR];
    updatedModules.forEach((updatedModule, index) => {
      const locale = locales[index];
      const messages = updatedModule?.default as MessageSchema | undefined;
      if (locale && messages) i18n.global.setLocaleMessage(locale, messages);
    });
  });
}

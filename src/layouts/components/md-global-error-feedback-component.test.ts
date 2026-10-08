// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import { toast } from 'vue-sonner';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { LANGUAGE_IDS } from '@/lib/models/settings';
import { useAppStore } from '@/stores/app-store';

import MdGlobalErrorFeedback from './md-global-error-feedback.vue';

vi.mock('vue-sonner', () => ({ toast: { error: vi.fn(), dismiss: vi.fn() } }));

const originalLocale = i18n.global.locale.value;

afterEach(() => {
  i18n.global.locale.value = originalLocale;
  vi.clearAllMocks();
});

describe('global error feedback', () => {
  it('explains an excluded analysis root in every supported language', async () => {
    setActivePinia(createPinia());
    const wrapper = mount(MdGlobalErrorFeedback, { global: { plugins: [i18n] } });
    const store = useAppStore();
    store.errorCode = 'invalidInput';
    store.errorReason = 'analysisRootExcluded';

    for (const locale of Object.values(LANGUAGE_IDS)) {
      i18n.global.locale.value = locale;
      vi.mocked(toast.error).mockClear();
      await nextTick();
      expect(toast.error).toHaveBeenCalledWith(i18n.global.t('errorReasons.analysisRootExcluded.title'), {
        id: 'application-error',
        description: i18n.global.t('errorReasons.analysisRootExcluded.message'),
        duration: Infinity,
        onDismiss: expect.any(Function),
      });
    }

    wrapper.unmount();
  });
  it.each([
    'directoryNotEmpty',
    'deleteIncomplete',
    'deleteRecoveryFailed',
    'analysisRefreshFailedAfterDelete',
  ] as const)('explains %s in every language', async reason => {
    setActivePinia(createPinia());
    const wrapper = mount(MdGlobalErrorFeedback, { global: { plugins: [i18n] } });
    const store = useAppStore();
    store.errorCode = 'operationFailed';
    store.errorReason = reason;
    for (const locale of Object.values(LANGUAGE_IDS)) {
      i18n.global.locale.value = locale;
      vi.mocked(toast.error).mockClear();
      await nextTick();
      expect(toast.error).toHaveBeenCalledWith(
        i18n.global.t(`errorReasons.${reason}.title`),
        expect.objectContaining({ description: i18n.global.t(`errorReasons.${reason}.message`) })
      );
    }
    wrapper.unmount();
  });
});

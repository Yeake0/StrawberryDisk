<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { Button } from '@/components/ui/button';
import MdSettingsRow from '@/components/custom/md-settings-row.vue';
import MdSwitch from '@/components/custom/md-switch.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';
import { useAiStore } from '@/stores/ai-store';

const { t } = useI18n({ useScope: 'global' });
const ai = useAiStore();
const emit = defineEmits<{ configure: [] }>();
</script>

<template>
  <div class="ai-feature-toggle" :aria-busy="ai.preferencesBusy">
    <MdSettingsRow
      :title="t('ai.providerTitle')"
      :description="t('ai.settingsDescription')"
      description-id="ai-enabled-hint"
    >
      <template #icon><MdIcon :name="ICON_NAMES.sparkles" /></template>
      <Button
        v-if="ai.enabled"
        variant="ghost"
        size="sm"
        class="text-muted-foreground"
        :disabled="ai.preferencesBusy"
        aria-haspopup="dialog"
        @click="emit('configure')"
      >
        {{ t('ai.configureAction') }}
      </Button>
      <MdSwitch
        id="ai-enabled"
        :model-value="ai.enabled"
        :disabled="ai.preferencesBusy || !ai.preferencesLoaded"
        :aria-label="t('ai.enableFeature')"
        aria-describedby="ai-enabled-hint"
        @update:model-value="ai.setEnabled"
      />
    </MdSettingsRow>
    <div v-if="ai.preferencesError" class="px-5 pb-4 text-sm text-destructive" role="alert">
      {{ t(ai.preferencesError === 'load' ? 'ai.featureLoadFailed' : 'ai.featureSaveFailed') }}
      <button
        v-if="ai.preferencesError === 'load'"
        type="button"
        class="cursor-pointer underline"
        :disabled="ai.preferencesBusy"
        @click="ai.loadPreferences"
      >
        {{ t('ai.retry') }}
      </button>
    </div>
  </div>
</template>

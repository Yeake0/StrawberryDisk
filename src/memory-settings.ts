import { createPinia } from 'pinia';
import { createApp } from 'vue';
import './assets/main.css';
import { i18n } from './i18n';
import MemorySettingsPage from './pages/memory-settings/index.vue';
import { useAppStore } from './stores/app-store';

document.documentElement.dataset.skin = 'strawberrydisk';
const pinia = createPinia();
const app = createApp(MemorySettingsPage).use(pinia).use(i18n);
void useAppStore(pinia)
  .loadSettings()
  .finally(() => app.mount('#app'));

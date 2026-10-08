import { createPinia } from 'pinia';
import { createApp } from 'vue';

import './assets/main.css';
import { i18n } from './i18n';
import MdTrayPanelShell from './layouts/md-tray-panel-shell.vue';
import { useAppStore } from './stores/app-store';

document.documentElement.dataset.skin = 'strawberrydisk';
const pinia = createPinia();
const app = createApp(MdTrayPanelShell).use(pinia).use(i18n);

// Mount independently of preference I/O so even an unusually early tray click
// can render immediately. Saved appearance updates reactively in the hidden panel.
app.mount('#app');
void useAppStore(pinia).loadSettings();

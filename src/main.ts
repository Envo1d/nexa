import { createApp } from "vue";

import App from "./App.vue";
import { i18n, initializeLocale } from "./i18n";

await initializeLocale();

createApp(App).use(i18n).mount("#app");

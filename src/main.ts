import { createApp } from "vue";

import App from "./App.vue";
import { i18n, initializeInterface } from "./i18n";

import "./themes.css";

await initializeInterface();

createApp(App).use(i18n).mount("#app");

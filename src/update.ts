import { createApp } from "vue";
import { invoke } from "@tauri-apps/api/core";

import UpdaterWindow from "./components/UpdaterWindow.vue";
import { applyLocale, i18n } from "./i18n";

import type { InterfaceLocale } from "./types/config";

try {
  const snapshot = await invoke<{ locale: InterfaceLocale }>("update_snapshot");

  await applyLocale(snapshot.locale);
} catch {
  // The mounted helper owns reconnection and continues to report its status.
  console.warn("Could not initialize updater language; using English.");
}

createApp(UpdaterWindow).use(i18n).mount("#app");

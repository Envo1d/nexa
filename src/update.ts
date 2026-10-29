import { createApp } from "vue";
import { invoke } from "@tauri-apps/api/core";

import UpdaterWindow from "./components/UpdaterWindow.vue";
import { applyLocale, i18n } from "./i18n";
import { applyTheme } from "./themes";

import "./themes.css";
import type { InterfaceLocale } from "./types/config";
import type { AppTheme } from "./themes";

try {
  const snapshot = await invoke<{ locale: InterfaceLocale; theme: AppTheme }>(
    "update_snapshot",
  );

  applyTheme(snapshot.theme);

  await applyLocale(snapshot.locale);
} catch {
  // The mounted helper owns reconnection and continues to report its status.
  console.warn("Could not initialize updater preferences; using defaults.");
}

createApp(UpdaterWindow).use(i18n).mount("#app");

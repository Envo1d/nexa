import { computed, ref, watch } from "vue";
import { createI18n } from "vue-i18n";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { locale as getSystemLocale } from "@tauri-apps/plugin-os";

import en from "./en.json";
import ru from "./ru.json";
import { resolveInterfaceLocale } from "./locale";
import { applyTheme } from "../themes";
import { isNativeData } from "../runtime";

import type { AppLocale, ConfigSnapshot } from "../types/config";

export const i18n = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages: { en, ru: ru satisfies typeof en },
});

export const localePreference = ref<AppLocale>("system");

export const languageOptions = computed<{ value: AppLocale; label: string }[]>(
  () => [
    { value: "system", label: i18n.global.t("systemLanguage") },
    { value: "en", label: "English" },
    { value: "ru", label: "Русский" },
  ],
);

export async function applyLocale(preference: AppLocale) {
  let systemLocale: string | null = null;

  if (preference === "system") {
    try {
      systemLocale = isTauri() ? await getSystemLocale() : navigator.language;
    } catch {
      console.warn("Could not read system language; using English.");
    }
  }

  i18n.global.locale.value = resolveInterfaceLocale(preference, systemLocale);
  localePreference.value = preference;
}

watch(
  i18n.global.locale,
  (locale) => {
    document.documentElement.lang = locale;
  },
  { immediate: true },
);

export async function initializeInterface() {
  if (!isNativeData()) {
    await applyLocale("system");

    return;
  }

  try {
    const snapshot = await invoke<ConfigSnapshot>("get_config");

    applyTheme(snapshot.settings.theme);

    await applyLocale(snapshot.settings.locale);
  } catch (cause) {
    // ConfigStore owns native error tracing; keep the UI usable for recovery.
    console.warn(
      "Could not initialize interface preferences; using default appearance.",
      cause,
    );
  }
}

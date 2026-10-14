import { ref } from "vue";

export const themeIds = [
  "midnight-violet",
  "graphite",
  "arctic-blue",
  "forest",
  "warm-amber",
  "rosewood",
  "paper",
] as const;

export type AppTheme = (typeof themeIds)[number];

export const themePreference = ref<AppTheme>("midnight-violet");

export function applyTheme(theme: AppTheme) {
  const resolved = themeIds.includes(theme) ? theme : "midnight-violet";

  document.documentElement.dataset.theme = resolved;
  themePreference.value = resolved;
}

import type { AppLocale, InterfaceLocale } from "../types/config";

export function resolveInterfaceLocale(
  preference: AppLocale,
  systemLocale: string | null,
): InterfaceLocale {
  if (preference !== "system") return preference;

  const language = systemLocale?.split(/[-_]/)[0]?.toLowerCase();

  return language === "ru" ? "ru" : "en";
}

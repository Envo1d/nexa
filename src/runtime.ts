import { isTauri } from "@tauri-apps/api/core";

export function isNativeData() {
  const demo = (window as Window & { __NEXA_DEMO__?: boolean }).__NEXA_DEMO__;

  return isTauri() && !demo;
}

import { ref } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../runtime";
import { i18n } from "../i18n";

import type { ConfigSnapshot } from "../types/config";

interface UpdateInfo {
  version: string;
  notes: string | null;
}

const available = ref<UpdateInfo | null>(null);
const busy = ref(false);
const status = ref("Not checked");
const percent = ref<number | null>(null);
const confirmation = ref<UpdateInfo | null>(null);

let startupChecked = false;

async function check() {
  if (busy.value || confirmation.value) return;

  if (!isNativeData()) {
    status.value = "Updates are available in the desktop application.";

    return;
  }

  busy.value = true;
  available.value = null;
  percent.value = null;
  status.value = "Checking for updates…";

  try {
    available.value = await invoke<UpdateInfo | null>("check_for_updates");
    status.value = available.value
      ? `Nexa ${available.value.version} is available.`
      : "You are using the latest version.";
  } catch (error) {
    status.value = String(error);
  } finally {
    busy.value = false;
  }
}

function install() {
  if (busy.value || !available.value || !isNativeData()) return;

  confirmation.value = { ...available.value };
}

function cancelInstall() {
  confirmation.value = null;
}

async function confirmInstall() {
  const requested = confirmation.value;

  if (busy.value || !requested || !isNativeData()) return;

  confirmation.value = null;

  if (available.value?.version !== requested.version) return;

  busy.value = true;

  try {
    percent.value = 0;
    status.value = "Downloading and verifying update…";

    const progress = new Channel<{
      percent: number;
    }>();

    progress.onmessage = (value) => {
      percent.value = Math.max(0, Math.min(100, value.percent));
    };

    await invoke("install_update", {
      progress,
      locale: i18n.global.locale.value,
    });

    status.value = "Restarting Nexa to apply the update…";
  } catch (error) {
    status.value = String(error);
  } finally {
    busy.value = false;
  }
}

async function checkOnStartup(notify: (message: string) => void) {
  if (!isNativeData() || startupChecked) return;

  startupChecked = true;

  try {
    const config = await invoke<ConfigSnapshot>("get_config");

    if (!config.settings.autoUpdateCheck) return;

    await check();

    if (available.value) notify(status.value);
  } catch {
    status.value = "Could not load the startup update preference.";
  }
}

export function useUpdater() {
  return {
    available,
    busy,
    status,
    percent,
    confirmation,
    check,
    install,
    confirmInstall,
    cancelInstall,
    checkOnStartup,
  };
}

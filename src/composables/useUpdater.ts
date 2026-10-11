import { computed, ref } from "vue";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import type { ConfigSnapshot } from "../types/config";

interface UpdateInfo {
  version: string;
  notes: string | null;
}
const available = ref<UpdateInfo | null>(null);
const busy = ref(false);
const status = ref("Not checked");
const downloaded = ref(0);
const total = ref<number | null>(null);
let startupChecked = false;

async function check() {
  if (busy.value) return;
  if (!isTauri()) {
    status.value = "Updates are available in the desktop application.";
    return;
  }
  busy.value = true;
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

async function install() {
  if (busy.value || !available.value || !isTauri()) return;
  busy.value = true;
  try {
    const accepted = await confirm(
      `Install Nexa ${available.value.version}? Nexa will close and restart after installation. Save any unfinished edits first.`,
      { title: "Install update", kind: "info" },
    );
    if (!accepted) return;
    downloaded.value = 0;
    total.value = null;
    status.value = "Downloading and verifying update…";
    const progress = new Channel<{
      downloaded: number;
      total: number | null;
    }>();
    progress.onmessage = (value) => {
      downloaded.value = value.downloaded;
      total.value = value.total;
    };
    await invoke("install_update", { progress });
    status.value = "Update installer started.";
  } catch (error) {
    status.value = String(error);
  } finally {
    busy.value = false;
  }
}

async function checkOnStartup(notify: (message: string) => void) {
  if (!isTauri() || startupChecked) return;
  startupChecked = true;
  try {
    const config = await invoke<ConfigSnapshot>("get_config");
    if (!config.settings.autoUpdateCheck) return;
    await check();
    if (available.value)
      notify(`${status.value} Open Settings → Updates to install.`);
  } catch {
    status.value = "Could not load the startup update preference.";
  }
}

export function useUpdater() {
  return {
    available,
    busy,
    status,
    downloaded,
    total,
    percent: computed(() =>
      total.value
        ? Math.min(100, Math.round((downloaded.value / total.value) * 100))
        : null,
    ),
    check,
    install,
    checkOnStartup,
  };
}

import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { isNativeData } from "../runtime";

export interface ScanStatus {
  running: boolean;
  scanned: number;
  extracted: number;
  errors: number;
  message: string;
}

const status = ref<ScanStatus>({
  running: false,
  scanned: 0,
  extracted: 0,
  errors: 0,
  message: "No scans this session",
});

let listening: Promise<void> | undefined;
let revision = 0;

async function refreshStatus() {
  const requestedRevision = ++revision;

  const snapshot = await invoke<ScanStatus>("get_stage_scan_status");

  if (requestedRevision === revision) status.value = snapshot;
}

export function useStageScan() {
  async function connect() {
    if (!isNativeData()) return;

    listening ??= (async () => {
      const stop = await listen<ScanStatus>("stage-scan-status", (event) => {
        ++revision;
        status.value = event.payload;
      });

      try {
        await refreshStatus();
      } catch (error) {
        stop();

        throw error;
      }
    })().catch((error: unknown) => {
      listening = undefined;

      throw error;
    });

    await listening;
  }

  async function start(library = false) {
    if (!isNativeData()) return;

    await connect();

    await invoke(library ? "start_library_scan" : "start_stage_scan");

    await refreshStatus();
  }

  return {
    status,
    connect,
    start,
  };
}

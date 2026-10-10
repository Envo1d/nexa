import { ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ScanStatus {
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
export function useStageScan() {
  async function connect() {
    if (!isTauri()) return;
    listening ??= (async () => {
      let received = false;
      await listen<ScanStatus>("stage-scan-status", (event) => {
        received = true;
        status.value = event.payload;
      });
      const snapshot = await invoke<ScanStatus>("get_stage_scan_status");
      if (!received) status.value = snapshot;
    })().catch((error: unknown) => {
      listening = undefined;
      throw error;
    });
    await listening;
  }
  async function start(library = false) {
    await connect();
    await invoke(library ? "start_library_scan" : "start_stage_scan");
    status.value = await invoke<ScanStatus>("get_stage_scan_status");
  }
  return { status, connect, start };
}

import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { isNativeData } from "../runtime";
import { useCacheUsage } from "../composables/useCacheUsage";
import { useStageScan } from "../composables/useStageScan";
import { useUpdater } from "../composables/useUpdater";

import type { Notify } from "../shared/composables/useToast";

export function useDesktopServices(
  refreshAll: () => Promise<void>,
  notify: Notify,
  openSettings: (section: "updates" | "cache" | "indexing") => void,
) {
  const cacheUsage = useCacheUsage();

  const stageScan = useStageScan();

  const updater = useUpdater();

  let disposed = false;
  let stopCacheUsage: (() => void) | undefined;
  let stopDiskSync: (() => void) | undefined;

  const diskSync = ref<{
    running: boolean;
    completed: number;
    total: number;
    error: string | null;
  }>({
    running: false,
    completed: 0,
    total: 0,
    error: null,
  });

  async function retryDiskSync() {
    if (!isNativeData()) return;

    try {
      await invoke("retry_disk_sync");
    } catch {
      notify("Could not retry disk synchronization.", { kind: "error" });
    }
  }

  watch(
    () => stageScan.status.value.running,
    (running, previous) => {
      if (previous && !running) void refreshAll();
    },
  );

  onMounted(() => {
    void updater.checkOnStartup((message) => {
      notify(message, {
        id: "update-available",
        kind: "update",
        description: "Review what is new and install from Settings.",
        duration: 0,
        action: { label: "View update", run: () => openSettings("updates") },
      });
    });

    void cacheUsage
      .connect((message) => {
        notify(message, {
          id: "cache-limit",
          action: { label: "Manage cache", run: () => openSettings("cache") },
        });
      })
      .then((stop) => {
        if (disposed) stop();
        else stopCacheUsage = stop;
      })
      .catch(() => {
        if (!disposed) notify("Could not connect cache usage notifications.");
      });

    void stageScan.connect().catch(() => {
      if (!disposed) notify("Could not connect Stage scan progress.");
    });

    void refreshAll();

    if (isNativeData()) void connectDiskSync();
  });

  async function connectDiskSync() {
    try {
      let received = false;

      const stop = await listen<typeof diskSync.value>(
        "disk-sync-status",
        (event) => {
          if (disposed) return;

          received = true;

          const previous = diskSync.value;

          diskSync.value = event.payload;

          if (previous.running && !event.payload.running) void refreshAll();
        },
      );

      if (disposed) {
        stop();

        return;
      }

      stopDiskSync = stop;

      const snapshot = await invoke<typeof diskSync.value>(
        "get_disk_sync_status",
      );

      if (!disposed && !received) diskSync.value = snapshot;
    } catch {
      if (!disposed) notify("Could not connect disk synchronization progress.");
    }
  }

  onBeforeUnmount(() => {
    disposed = true;
    stopCacheUsage?.();
    stopDiskSync?.();
  });

  return {
    diskSync,
    retryDiskSync,
    indexing: stageScan.status,
    openIndexing: () => openSettings("indexing"),
  };
}

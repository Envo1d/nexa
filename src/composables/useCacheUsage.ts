import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { isNativeData } from "../runtime";
interface CacheStatus {
  bytes: number;
  maxBytes: number;
}

interface CacheClearResult {
  usage: CacheStatus;
  failedRemovals: number;
}

const status = ref<CacheStatus | null>(null);
const error = ref("");
const limitReached = ref(false);
const clearing = ref(false);

let generation = 0;

const label = computed(() =>
  status.value
    ? `${(status.value.bytes / 1024 / 1024).toLocaleString("en-US", { maximumFractionDigits: 2 })} / 512 MB`
    : isNativeData()
      ? "Unavailable / 512 MB"
      : "Desktop only / 512 MB",
);

const pressure = computed(() =>
  limitReached.value ||
  (status.value && status.value.bytes >= status.value.maxBytes)
    ? "full"
    : status.value && status.value.bytes >= status.value.maxBytes * 0.9
      ? "warning"
      : "normal",
);

export function useCacheUsage() {
  async function clear() {
    if (!isNativeData() || clearing.value) return;

    clearing.value = true;
    generation += 1;

    try {
      const result = await invoke<CacheClearResult>("clear_thumbnail_cache");

      status.value = result.usage;
      error.value = "";
      limitReached.value = result.usage.bytes >= result.usage.maxBytes;

      return result;
    } finally {
      generation += 1;
      clearing.value = false;
    }
  }

  async function connect(notify: (message: string) => void) {
    if (!isNativeData()) return () => {};

    let stopped = false;
    let alerted = false;

    const alert = () => {
      limitReached.value = true;

      if (!alerted)
        notify(
          "Thumbnail cache reached 512 MB. Old thumbnails are automatically removed to free space.",
        );

      alerted = true;
    };

    const unlisten = await listen<CacheStatus>("cache-limit-reached", () =>
      alert(),
    );

    async function refresh() {
      if (clearing.value) return;

      const requestedGeneration = generation;

      try {
        const snapshot = await invoke<CacheStatus>("get_cache_usage");

        if (stopped || requestedGeneration !== generation) return;

        status.value = snapshot;
        error.value = "";

        if (snapshot.bytes >= snapshot.maxBytes) alert();
        else {
          limitReached.value = false;

          if (snapshot.bytes < snapshot.maxBytes * 0.9) alerted = false;
        }
      } catch {
        if (stopped || requestedGeneration !== generation) return;

        status.value = null;
        error.value = "Could not measure thumbnail cache. Retrying…";
      }
    }

    await refresh();

    let timer: ReturnType<typeof setTimeout>;

    const poll = async () => {
      await refresh();

      if (!stopped) timer = setTimeout(() => void poll(), 5000);
    };

    timer = setTimeout(() => void poll(), 5000);

    return () => {
      stopped = true;
      clearTimeout(timer);
      unlisten();
    };
  }

  return {
    status,
    label,
    pressure,
    error,
    clearing,
    clear,
    connect,
  };
}

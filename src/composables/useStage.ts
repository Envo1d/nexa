import { computed, ref, shallowRef, watch, onScopeDispose } from "vue";
import { useMediaSelection } from "./useMediaSelection";
import { createMockMedia } from "../data/mockMedia";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { MediaItem, MediaKind } from "../types/media";

export function useStage() {
  const makeMediaRef = isTauri() ? shallowRef : ref;
  const media = makeMediaRef<MediaItem[]>(
    (isTauri() ? [] : createMockMedia()).slice(0, 96).map((item, index) => {
      const filename = `Capture ${String(index + 1).padStart(3, "0")}.${item.extension.toLowerCase()}`;
      return {
        ...item,
        id: `stage-${index + 1}`,
        filename,
        path: `C:\\Media\\Nexa staging\\${item.kind === "video" ? "Videos" : "Images"}\\${filename}`,
        favorite: false,
        copyrights: [],
        artist: "",
        characters: [],
        tags: [],
      };
    }),
  );
  const search = ref("");
  const appliedSearch = ref("");
  const total = ref(0);
  const loading = ref(false);
  const error = ref("");
  const hasMore = computed(() => isTauri() && media.value.length < total.value);
  let cursor: string | null = null;
  let refreshVersion = 0;
  async function loadPage(append = false) {
    if (!isTauri()) return;
    if (append && loading.value) return;
    const version = append ? refreshVersion : ++refreshVersion;
    loading.value = true;
    error.value = "";
    try {
      type NativeFile = {
        id: string;
        path: string;
        filename: string;
        nativeRedistribution: boolean;
        revision: string | null;
        availability: string;
        kind: MediaKind;
        extension: string;
        size: number | null;
        width: number | null;
        height: number | null;
        modifiedAt: number | null;
        createdAt: number | null;
        duration: number | null;
        fps: number | null;
        codec: string | null;
        bitrate: number | null;
      };
      const page = await invoke<{ files: NativeFile[]; total: number }>(
        "get_stage_files",
        { after: append ? cursor : null, search: appliedSearch.value },
      );
      if (version !== refreshVersion) return;
      const queued: MediaItem[] = page.files.map((file) => ({
        id: `database-${file.id}`,
        nativeId: file.id,
        nativeRevision: file.revision ?? "imported",
        availability: file.availability,
        nativeRedistribution: file.nativeRedistribution,
        filename: file.filename,
        path: file.path,
        kind: file.kind,
        extension: file.extension,
        preview: "",
        size: file.size ?? -1,
        width: file.width ?? 0,
        height: file.height ?? 0,
        modifiedAt: file.modifiedAt ?? 0,
        createdAt: file.createdAt ?? 0,
        duration: file.duration === null ? undefined : file.duration / 1000,
        fps: file.fps ?? undefined,
        codec: file.codec ?? undefined,
        bitrate: file.bitrate ?? undefined,
        favorite: false,
        copyrights: [],
        artist: "",
        characters: [],
        tags: [],
      }));
      total.value = page.total;
      cursor = page.files[page.files.length - 1]?.id ?? cursor;
      media.value = append ? [...media.value, ...queued] : queued;
    } catch (cause) {
      if (version === refreshVersion)
        error.value = `Could not load Stage: ${String(cause)}`;
    } finally {
      if (version === refreshVersion) loading.value = false;
    }
  }
  async function refreshRedistribution() {
    await loadPage();
  }
  async function loadMore() {
    if (hasMore.value) await loadPage(true);
  }
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  watch(search, () => {
    clearTimeout(searchTimer);
    if (!search.value.trim()) {
      appliedSearch.value = "";
      return;
    }
    searchTimer = setTimeout(() => {
      appliedSearch.value = search.value.trim();
    }, 350);
  });
  watch(appliedSearch, () => {
    if (isTauri()) {
      cursor = null;
      void loadPage();
    }
  });
  onScopeDispose(() => {
    ++refreshVersion;
    clearTimeout(searchTimer);
  });
  const results = computed(() => {
    if (isTauri()) return media.value;
    const query = appliedSearch.value.toLocaleLowerCase().replace(/\\/g, "/");
    return media.value.filter((item) =>
      [item.filename, item.path].some((value) =>
        value.toLocaleLowerCase().replace(/\\/g, "/").includes(query),
      ),
    );
  });
  const selection = useMediaSelection(results);
  function clearSearch() {
    search.value = "";
  }
  return {
    media,
    total,
    hasMore,
    loading,
    error,
    loadMore,
    refreshRedistribution,
    search,
    results,
    clearSearch,
    ...selection,
  };
}

import { computed, ref, shallowRef, watch, onScopeDispose } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../runtime";
import { useMediaSelection } from "./useMediaSelection";
import { createMockMedia } from "../data/mockMedia";
import { nameSearchRank } from "../utils/metadataSearch";

import type { MediaItem, MediaKind } from "../types/media";

export function useStage() {
  const makeMediaRef = isNativeData() ? shallowRef : ref;

  const media = makeMediaRef<MediaItem[]>(
    (isNativeData() ? [] : createMockMedia())
      .slice(0, 96)
      .map((item, index) => {
        const filename = `Capture ${String(index + 1).padStart(3, "0")}.${item.extension.toLowerCase()}`;

        return {
          ...item,
          id: `stage-${index + 1}`,
          filename,
          path: `C:\\Media\\Nexa staging\\${item.kind === "video" ? "Videos" : "Images"}\\${filename}`,
          favorite: false,
          title: undefined,
          videoTitle: undefined,
          copyrights: [],
          artist: "",
          characters: [],
          tags: [],
        };
      }),
  );

  const search = ref("");
  const appliedSearch = ref("");
  const sortMode = ref<"newest" | "older" | "name-asc" | "name-desc">("newest");

  const queryKey = computed(() =>
    JSON.stringify([appliedSearch.value, sortMode.value]),
  );

  const total = ref(0);
  const loading = ref(false);
  const error = ref("");

  const hasMore = computed(
    () => isNativeData() && media.value.length < total.value,
  );

  let refreshVersion = 0;

  async function loadPage(append = false, reset = false) {
    if (!isNativeData()) return;

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

      const readPage = (offset: number) =>
        invoke<{ files: NativeFile[]; total: number }>("get_stage_files", {
          offset,
          search: appliedSearch.value,
          sort: sortMode.value,
        });

      const target = append || reset ? 0 : media.value.length;

      const page = await readPage(append ? media.value.length : 0);

      if (version !== refreshVersion) return;

      while (!append && page.files.length < Math.min(target, page.total)) {
        const next = await readPage(page.files.length);

        if (version !== refreshVersion) return;

        if (!next.files.length) break;

        page.files.push(...next.files);
        page.total = next.total;
      }

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

  watch([appliedSearch, sortMode], () => {
    if (isNativeData()) {
      void loadPage(false, true);
    }
  });

  onScopeDispose(() => {
    ++refreshVersion;
    clearTimeout(searchTimer);
  });

  const results = computed(() => {
    if (isNativeData()) return media.value;

    const query = appliedSearch.value.toLocaleLowerCase().replace(/\\/g, "/");

    return media.value
      .filter(
        (item) => nameSearchRank(item.path.replace(/\\/g, "/"), query) < 2,
      )
      .sort((a, b) => {
        const relevance =
          Math.min(nameSearchRank(a.filename, query), 1) -
          Math.min(nameSearchRank(b.filename, query), 1);

        if (relevance) return relevance;

        if (sortMode.value === "name-asc" || sortMode.value === "name-desc") {
          const order = a.filename.localeCompare(b.filename, "en", {
            numeric: true,
            sensitivity: "base",
          });

          return (
            (sortMode.value === "name-asc" ? order : -order) ||
            a.id.localeCompare(b.id)
          );
        }

        return (
          (sortMode.value === "newest"
            ? b.modifiedAt - a.modifiedAt
            : a.modifiedAt - b.modifiedAt) || a.id.localeCompare(b.id)
        );
      });
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
    appliedSearch,
    sortMode,
    queryKey,
    results,
    clearSearch,
    ...selection,
  };
}

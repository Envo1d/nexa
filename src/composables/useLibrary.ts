import { computed, ref, watch, onScopeDispose } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../runtime";
import { useMediaSelection } from "./useMediaSelection";
import { createMockMedia } from "../data/mockMedia";
import {
  chooseMetadataSuggestion,
  metadataSuggestions,
  normalizeMetadataSearch,
  metadataSearchRank,
  type MetadataSuggestion,
} from "../utils/metadataSearch";

import type { MediaItem, MediaKind } from "../types/media";

export function useLibrary() {
  const { t } = useI18n();

  const native = isNativeData();

  const media = ref<MediaItem[]>(native ? [] : createMockMedia());
  const total = ref(0);
  const loading = ref(false);
  const error = ref("");
  const nativeCounts = ref({
    all: 0,
    video: 0,
    image: 0,
  });
  const nativeTags = ref<{ name: string; count: number }[]>([]);
  const characterCatalog = ref<{ name: string; copyright: string }[]>([]);
  const nativeSuggestions = ref<{ name: string; category: string }[]>([]);

  const hasMore = computed(() => native && media.value.length < total.value);

  let version = 0;

  async function refresh(append = false, reset = false) {
    if (!native || (append && loading.value)) return;

    const request = append ? version : ++version;

    loading.value = true;
    error.value = "";

    try {
      type NativeFile = Omit<
        MediaItem,
        | "id"
        | "preview"
        | "characters"
        | "modifiedAt"
        | "createdAt"
        | "size"
        | "width"
        | "height"
      > & {
        id: string;
        revision: string | null;
        modifiedAt: number | null;
        createdAt: number | null;
        size: number | null;
        width: number | null;
        height: number | null;
      };

      const readPage = (offset: number) =>
        invoke<{
          files: NativeFile[];
          total: number;
          typeCounts: [number, number, number];
          tags: { name: string; count: number }[];
          suggestions: { name: string; category: string }[];
          characters: { name: string; copyright: string }[];
        }>("get_library_files", {
          filters: {
            scope: scope.value,
            search: appliedSearch.value,
            kind: kind.value,
            sort: sortMode.value,
            tagged: taggedOnly.value || hideUntagged.value,
            offset,
          },
        });

      const target = append || reset ? 0 : media.value.length;

      const page = await readPage(append ? media.value.length : 0);

      if (request !== version) return;

      // Publish a refreshed prefix atomically so the virtual viewport never collapses.
      while (!append && page.files.length < Math.min(target, page.total)) {
        const next = await readPage(page.files.length);

        if (request !== version) return;

        if (!next.files.length) break;

        page.files.push(...next.files);
        page.total = next.total;
      }

      const items: MediaItem[] = page.files.map((file) => ({
        ...file,
        id: `database-${file.id}`,
        nativeId: file.id,
        nativeRevision: file.revision ?? "imported",
        preview: "",
        size: file.size ?? -1,
        width: file.width ?? 0,
        height: file.height ?? 0,
        modifiedAt: file.modifiedAt ?? 0,
        createdAt: file.createdAt ?? 0,
        duration: file.duration == null ? undefined : file.duration / 1000,
        fps: file.fps ?? undefined,
        codec: file.codec ?? undefined,
        bitrate: file.bitrate ?? undefined,
        videoTitle: file.videoTitle ?? undefined,
        characters: [
          ...new Set(
            file.characterCopyrights?.map((entry) => entry.name) ?? [],
          ),
        ],
      }));

      media.value = append ? [...media.value, ...items] : items;
      total.value = page.total;
      nativeCounts.value = {
        all: page.typeCounts[0],
        video: page.typeCounts[1],
        image: page.typeCounts[2],
      };
      nativeTags.value = page.tags;
      nativeSuggestions.value = page.suggestions;
      characterCatalog.value = page.characters;
    } catch (cause) {
      if (request === version)
        error.value = `Could not load Library: ${String(cause)}`;
    } finally {
      if (request === version) loading.value = false;
    }
  }

  async function loadMore() {
    if (hasMore.value) await refresh(true);
  }

  onScopeDispose(() => {
    ++version;
  });

  const scope = ref<"gallery" | "favorites">("gallery");
  const search = ref("");
  const appliedSearch = ref("");
  const kind = ref<"all" | MediaKind>("all");
  const sortMode = ref<"newest" | "older" | "name-asc" | "name-desc">("newest");
  const taggedOnly = ref(false);
  const hideUntagged = ref(false);

  const scopedMedia = computed(() =>
    media.value.filter((item) => scope.value === "gallery" || item.favorite),
  );

  const typeCounts = computed(() =>
    native
      ? nativeCounts.value
      : {
          all: scopedMedia.value.length,
          video: scopedMedia.value.filter((item) => item.kind === "video")
            .length,
          image: scopedMedia.value.filter((item) => item.kind === "image")
            .length,
        },
  );

  const tags = computed(() => {
    if (native) return nativeTags.value;

    const counts = new Map<string, number>();

    for (const item of scopedMedia.value)
      for (const tag of item.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);

    return [...counts]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  });

  const results = computed(() => {
    if (native) return media.value;

    const queries = appliedSearch.value
      .split(";")
      .map((value) => value.trim().toLocaleLowerCase())
      .filter(Boolean);

    const rank = (item: MediaItem) =>
      metadataSearchRank(
        [
          ...item.copyrights,
          ...(item.artists ?? [item.artist]),
          ...item.characters,
          ...item.tags,
        ],
        queries,
      );

    return scopedMedia.value
      .filter(
        (item) =>
          (kind.value === "all" || item.kind === kind.value) &&
          (!(taggedOnly.value || hideUntagged.value) || item.tags.length > 0) &&
          rank(item) < 2,
      )
      .sort((a, b) => {
        const relevance = rank(a) - rank(b);

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

  const heading = computed(() =>
    appliedSearch.value
      ? appliedSearch.value
      : scope.value === "favorites"
        ? t("favorites")
        : t("allFiles"),
  );

  const activeFilterCount = computed(
    () =>
      Number(Boolean(appliedSearch.value.trim())) +
      Number(kind.value !== "all") +
      Number(taggedOnly.value || hideUntagged.value),
  );

  const suggestions = computed(() => {
    const values = new Map<string, MetadataSuggestion>();

    const add = (value: MetadataSuggestion) => {
      const normalized = value.name.toLocaleLowerCase();

      values.set(
        JSON.stringify([value.category, normalized, value.copyright ?? ""]),
        value,
      );
    };

    if (native) {
      for (const value of nativeSuggestions.value)
        if (value.category !== "Character") add(value);

      for (const value of characterCatalog.value)
        add({ ...value, category: "Character" });
    } else {
      for (const item of scopedMedia.value) {
        for (const [category, names] of [
          ["Copyright", item.copyrights],
          ["Artist", item.artists ?? [item.artist]],
          ["Tag", item.tags],
        ] as const) {
          for (const name of names) add({ name, category });
        }

        for (const value of item.characterCopyrights ??
          item.characters.map((name) => ({
            name,
            copyright: item.copyrights[0] ?? "",
          })))
          add({ ...value, category: "Character" });
      }
    }

    return metadataSuggestions(search.value, [...values.values()]);
  });

  function commitSearch() {
    search.value = normalizeMetadataSearch(search.value);
    appliedSearch.value = search.value;
  }

  function chooseSuggestion(name: string) {
    search.value = chooseMetadataSuggestion(search.value, name);
    commitSearch();
  }

  function addSearch(name: string) {
    const parts = appliedSearch.value
      .split(";")
      .map((value) => value.trim())
      .filter(Boolean);

    if (
      !parts.some(
        (value) => value.toLocaleLowerCase() === name.toLocaleLowerCase(),
      )
    )
      parts.push(name);

    search.value = parts.join("; ");
    commitSearch();
  }

  function clearSearch() {
    search.value = "";
    appliedSearch.value = "";
  }

  const favoriteBusy = new Set<string>();

  async function toggleFavorite(item: MediaItem) {
    if (favoriteBusy.has(item.id)) return;

    if (!native) {
      item.favorite = !item.favorite;

      return;
    }

    favoriteBusy.add(item.id);

    try {
      await invoke("set_library_favorite", {
        id: item.nativeId,
        favorite: !item.favorite,
      });

      await refresh();
    } catch (cause) {
      error.value = `Could not save favorite: ${String(cause)}`;
    } finally {
      favoriteBusy.delete(item.id);
    }
  }

  function reset() {
    clearSearch();
    kind.value = "all";
    sortMode.value = "newest";
    taggedOnly.value = false;
    hideUntagged.value = false;
  }

  const collectionKey = computed(() =>
    JSON.stringify([
      scope.value,
      appliedSearch.value,
      kind.value,
      sortMode.value,
      taggedOnly.value || hideUntagged.value,
    ]),
  );

  watch(
    [scope, appliedSearch, kind, sortMode, taggedOnly, hideUntagged],
    () => {
      if (native) void refresh(false, true);
    },
  );

  const catalog = computed(() =>
    native
      ? {
          copyrights: nativeSuggestions.value
            .filter((entry) => entry.category === "Copyright")
            .map((entry) => entry.name),
          artists: nativeSuggestions.value
            .filter((entry) => entry.category === "Artist")
            .map((entry) => entry.name),
          tags: nativeSuggestions.value
            .filter((entry) => entry.category === "Tag")
            .map((entry) => entry.name),
          characters: characterCatalog.value,
        }
      : undefined,
  );

  return {
    collectionKey,
    catalog,
    total,
    loading,
    error,
    hasMore,
    refresh,
    loadMore,
    media,
    scope,
    search,
    appliedSearch,
    kind,
    sortMode,
    suggestions,
    taggedOnly,
    hideUntagged,
    ...selection,
    results,
    heading,
    tags,
    typeCounts,
    activeFilterCount,
    commitSearch,
    chooseSuggestion,
    addSearch,
    clearSearch,
    toggleFavorite,
    reset,
  };
}

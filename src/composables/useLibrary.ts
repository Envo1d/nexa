import { computed, ref } from "vue";
import { useMediaSelection } from "./useMediaSelection";
import { createMockMedia } from "../data/mockMedia";
import type { MediaItem, MediaKind } from "../types/media";

export function useLibrary() {
  const media = ref(createMockMedia());
  const scope = ref<"gallery" | "favorites">("gallery");
  const search = ref("");
  const appliedSearch = ref("");
  const kind = ref<"all" | MediaKind>("all");
  const sortMode = ref<"newest" | "older" | "name-asc" | "name-desc">("newest");
  const favoritesOnly = ref(false);
  const taggedOnly = ref(false);
  const hideUntagged = ref(false);
  const scopedMedia = computed(() =>
    media.value.filter((item) => scope.value === "gallery" || item.favorite),
  );
  const typeCounts = computed(() => ({
    all: scopedMedia.value.length,
    video: scopedMedia.value.filter((item) => item.kind === "video").length,
    image: scopedMedia.value.filter((item) => item.kind === "image").length,
  }));
  const tags = computed(() => {
    const counts = new Map<string, number>();
    for (const item of scopedMedia.value)
      for (const tag of item.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    return [...counts]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  });
  const results = computed(() => {
    const queries = appliedSearch.value
      .split(";")
      .map((value) => value.trim().toLocaleLowerCase())
      .filter(Boolean);
    return scopedMedia.value
      .filter(
        (item) =>
          (kind.value === "all" || item.kind === kind.value) &&
          (!favoritesOnly.value || item.favorite) &&
          (!(taggedOnly.value || hideUntagged.value) || item.tags.length > 0) &&
          queries.every((query) =>
            [
              ...item.copyrights,
              ...(item.artists ?? [item.artist]),
              ...item.characters,
              ...item.tags,
            ].some((value) => value.toLocaleLowerCase() === query),
          ),
      )
      .sort((a, b) => {
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
        ? "Favorites"
        : "All files",
  );
  const activeFilterCount = computed(
    () =>
      Number(Boolean(appliedSearch.value.trim())) +
      Number(kind.value !== "all") +
      Number(favoritesOnly.value) +
      Number(taggedOnly.value || hideUntagged.value),
  );

  const suggestions = computed(() => {
    const parts = search.value.split(";");
    const query = parts.pop()?.trim().toLocaleLowerCase() ?? "";
    if (!query) return [];
    const existing = parts.map((value) => value.trim().toLocaleLowerCase());
    const values = new Map<string, { name: string; category: string }>();
    for (const item of scopedMedia.value) {
      for (const [category, names] of [
        ["Copyright", item.copyrights],
        ["Artist", item.artists ?? [item.artist]],
        ["Character", item.characters],
        ["Tag", item.tags],
      ] as const) {
        for (const name of names) {
          const normalized = name.toLocaleLowerCase();
          if (normalized.includes(query) && !existing.includes(normalized))
            values.set(`${category}:${normalized}`, { name, category });
        }
      }
    }
    return [...values.values()]
      .sort((a, b) => a.name.localeCompare(b.name))
      .slice(0, 30);
  });
  function commitSearch() {
    search.value = search.value
      .split(";")
      .map((value) => value.trim())
      .filter(Boolean)
      .join("; ");
    appliedSearch.value = search.value;
  }
  function chooseSuggestion(name: string) {
    const parts = search.value.split(";");
    parts.pop();
    search.value = [...parts, name].join("; ");
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
  function toggleFavorite(item: MediaItem) {
    item.favorite = !item.favorite;
  }
  function reset() {
    clearSearch();
    kind.value = "all";
    sortMode.value = "newest";
    favoritesOnly.value = false;
    taggedOnly.value = false;
    hideUntagged.value = false;
  }
  return {
    media,
    scope,
    search,
    kind,
    sortMode,
    suggestions,
    favoritesOnly,
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

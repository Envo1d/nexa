import { computed, ref, watch } from "vue";
import { createMockMedia } from "../data/mockMedia";
import type { MediaItem, MediaKind } from "../types/media";

export function useLibrary() {
  const media = ref(createMockMedia());
  const scope = ref<"gallery" | "favorites">("gallery");
  const search = ref("");
  const kind = ref<"all" | MediaKind>("all");
  const sortDirection = ref<"desc" | "asc">("desc");
  const selectedTags = ref<string[]>([]);
  const favoritesOnly = ref(false);
  const taggedOnly = ref(false);
  const hideUntagged = ref(false);
  const selectedId = ref<string | null>(null);
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
    const query = search.value.trim().toLocaleLowerCase();
    return scopedMedia.value
      .filter(
        (item) =>
          (kind.value === "all" || item.kind === kind.value) &&
          (!favoritesOnly.value || item.favorite) &&
          (!(taggedOnly.value || hideUntagged.value) || item.tags.length > 0) &&
          selectedTags.value.every((tag) => item.tags.includes(tag)) &&
          (!query ||
            [item.filename, item.title ?? "", ...item.tags]
              .join(" ")
              .toLocaleLowerCase()
              .includes(query)),
      )
      .sort((a, b) =>
        sortDirection.value === "desc"
          ? b.modifiedAt - a.modifiedAt
          : a.modifiedAt - b.modifiedAt,
      );
  });
  const selected = computed(
    () => media.value.find((item) => item.id === selectedId.value) ?? null,
  );
  const heading = computed(() =>
    selectedTags.value.length
      ? selectedTags.value.join(" + ")
      : scope.value === "favorites"
        ? "Favorites"
        : "All files",
  );
  const activeFilterCount = computed(
    () =>
      Number(Boolean(search.value.trim())) +
      Number(kind.value !== "all") +
      selectedTags.value.length +
      Number(favoritesOnly.value) +
      Number(taggedOnly.value || hideUntagged.value),
  );

  watch(results, (items) => {
    if (selectedId.value && !items.some((item) => item.id === selectedId.value))
      selectedId.value = null;
  });

  function toggleTag(tag: string) {
    selectedTags.value = selectedTags.value.includes(tag)
      ? selectedTags.value.filter((name) => name !== tag)
      : [...selectedTags.value, tag];
  }
  function toggleFavorite(item: MediaItem) {
    item.favorite = !item.favorite;
  }
  function reset() {
    search.value = "";
    kind.value = "all";
    sortDirection.value = "desc";
    selectedTags.value = [];
    favoritesOnly.value = false;
    taggedOnly.value = false;
    hideUntagged.value = false;
  }
  return {
    media,
    scope,
    search,
    kind,
    sortDirection,
    selectedTags,
    favoritesOnly,
    taggedOnly,
    hideUntagged,
    selectedId,
    selected,
    results,
    heading,
    tags,
    typeCounts,
    activeFilterCount,
    toggleTag,
    toggleFavorite,
    reset,
  };
}

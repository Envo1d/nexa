import { computed, ref, watch } from "vue";
import { createMockMedia } from "../data/mockMedia";

export function useStage() {
  const media = ref(
    createMockMedia()
      .slice(0, 96)
      .map((item, index) => {
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
  const selectedId = ref<string | null>(null);
  const selectedIds = ref<string[]>([]);
  const results = computed(() => {
    const query = search.value.trim().toLocaleLowerCase().replace(/\\/g, "/");
    return media.value.filter((item) =>
      [item.filename, item.path].some((value) =>
        value.toLocaleLowerCase().replace(/\\/g, "/").includes(query),
      ),
    );
  });
  const selected = computed(
    () => results.value.find((item) => item.id === selectedId.value) ?? null,
  );
  watch(results, (items) => {
    selectedIds.value = selectedIds.value.filter((id) =>
      items.some((item) => item.id === id),
    );
    if (!items.some((item) => item.id === selectedId.value))
      selectedId.value = null;
  });
  function clearSearch() {
    search.value = "";
  }
  function select(item: { id: string }, event?: MouseEvent) {
    if (event?.ctrlKey || event?.metaKey) {
      selectedIds.value = selectedIds.value.includes(item.id)
        ? selectedIds.value.filter((id) => id !== item.id)
        : [...selectedIds.value, item.id];
    } else selectedIds.value = [item.id];
    selectedId.value =
      selectedIds.value.length === 1 ? selectedIds.value[0]! : null;
  }
  function selectMany(ids: string[], focus = true) {
    selectedIds.value = ids;
    if (focus) selectedId.value = ids.length === 1 ? ids[0]! : null;
  }
  function clearSelection() {
    selectedIds.value = [];
    selectedId.value = null;
  }
  return {
    search,
    selectedId,
    selectedIds,
    results,
    selected,
    clearSearch,
    select,
    selectMany,
    clearSelection,
  };
}

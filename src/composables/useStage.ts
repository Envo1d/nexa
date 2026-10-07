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
    if (!items.some((item) => item.id === selectedId.value))
      selectedId.value = null;
  });
  function clearSearch() {
    search.value = "";
  }
  return { search, selectedId, results, selected, clearSearch };
}

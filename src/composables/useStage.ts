import { computed, ref } from "vue";
import { useMediaSelection } from "./useMediaSelection";
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
  const results = computed(() => {
    const query = search.value.trim().toLocaleLowerCase().replace(/\\/g, "/");
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
    search,
    results,
    clearSearch,
    ...selection,
  };
}

import { computed, ref } from "vue";
import { useMediaSelection } from "./useMediaSelection";
import { createMockMedia } from "../data/mockMedia";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { MediaItem, MediaKind } from "../types/media";

export function useStage() {
  const media = ref<MediaItem[]>(
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
  let refreshVersion = 0;
  async function refreshRedistribution() {
    if (!isTauri()) return;
    const version = ++refreshVersion;
    const files = await invoke<
      {
        id: string;
        root: string;
        relativePath: string;
        kind: MediaKind;
        extension: string;
        size: number | null;
        width: number | null;
        height: number | null;
        modifiedAt: number | null;
        createdAt: number | null;
        duration: number | null;
      }[]
    >("get_redistribution_files");
    if (version !== refreshVersion) return;
    const queued: MediaItem[] = files.map((file) => ({
      id: `database-${file.id}`,
      nativeRedistribution: true,
      filename: file.relativePath.split(/[\\/]/).pop() ?? file.relativePath,
      path: `${file.root.replace(/[\\/]$/, "")}/${file.relativePath}`,
      kind: file.kind,
      extension: file.extension,
      preview: "",
      size: file.size ?? -1,
      width: file.width ?? 0,
      height: file.height ?? 0,
      modifiedAt: file.modifiedAt ?? 0,
      createdAt: file.createdAt ?? 0,
      duration: file.duration === null ? undefined : file.duration / 1000,
      favorite: false,
      copyrights: [],
      artist: "",
      characters: [],
      tags: [],
    }));
    media.value = [
      ...media.value.filter((item) => !item.nativeRedistribution),
      ...queued,
    ];
  }
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
    media,
    refreshRedistribution,
    search,
    results,
    clearSearch,
    ...selection,
  };
}

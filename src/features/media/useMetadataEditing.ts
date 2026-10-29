import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../../runtime";
import type { MediaItem, MediaMetadataDraft } from "../../types/media";
import type { LibraryModel, StageModel, Notify } from "./model";

export function useMetadataEditing(
  library: LibraryModel,
  stage: StageModel,
  refreshAll: () => Promise<void>,
  closeContext: () => void,
  notify: Notify,
) {
  const distributionQueue = ref<MediaItem[]>([]);
  const replacementTarget = ref<MediaItem | null>(null);

  async function replacementCompleted() {
    stage.clearSelection();

    await refreshAll();

    notify("File replaced. Old version deleted.");
  }

  const editingMetadata = ref(false);

  function distribute(item?: MediaItem) {
    editingMetadata.value = false;

    if (item && !stage.selectedIds.value.includes(item.id)) stage.select(item);

    distributionQueue.value = stage.results.value.filter((entry) =>
      stage.selectedIds.value.includes(entry.id),
    );

    closeContext();
  }

  async function saveMetadata(ids: string[], draft: MediaMetadataDraft) {
    if (isNativeData()) {
      const nativeIds = ids.map(
        (id) =>
          distributionQueue.value.find((item) => item.id === id)?.nativeId,
      );

      await invoke(
        editingMetadata.value ? "save_library_metadata" : "distribute_files",
        { ids: nativeIds, draft },
      );

      await refreshAll();

      return;
    }

    if (!editingMetadata.value) {
      const distributed = stage.media.value.filter((item) =>
        ids.includes(item.id),
      );

      stage.media.value = stage.media.value.filter(
        (item) => !ids.includes(item.id),
      );

      library.media.value.push(...distributed);
    }

    const alphabetically = (values: string[]) =>
      [...values].sort((a, b) => a.localeCompare(b));

    for (const item of library.media.value) {
      if (!ids.includes(item.id)) continue;

      item.copyrights = alphabetically(draft.copyrights);
      item.artists = [...draft.artists];
      item.artist = draft.artists.join(", ");

      item.characterCopyrights = draft.characters
        .map((entry) => ({ ...entry }))
        .sort(
          (a, b) =>
            a.name.localeCompare(b.name) ||
            a.copyright.localeCompare(b.copyright),
        );

      item.characters = alphabetically([
        ...new Set(draft.characters.map((entry) => entry.name)),
      ]);

      item.tags = alphabetically(draft.tags);

      if (item.kind === "video") item.videoTitle = draft.videoTitle;
    }
  }

  return {
    distributionQueue,
    replacementTarget,
    editingMetadata,
    distribute,
    saveMetadata,
    replacementCompleted,
  };
}

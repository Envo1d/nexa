import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../../runtime";
import type { MediaItem } from "../../types/media";
import type { LibraryModel, StageModel } from "./model";

export function useMediaViewer(library: LibraryModel, stage: StageModel) {
  const viewerItems = ref<MediaItem[]>([]);
  const viewerIndex = ref(0);
  const viewerStage = ref(false);
  const viewerOpen = ref(false);
  const viewerPaging = ref(false);
  const viewerTotal = ref(0);
  const viewerError = ref("");

  let generation = 0;

  const viewerItem = computed(() => viewerItems.value[viewerIndex.value]);

  const viewerHasNext = computed(
    () => viewerIndex.value + 1 < viewerTotal.value,
  );

  async function openViewer(item: MediaItem, fromStage: boolean) {
    const request = ++generation;

    viewerStage.value = fromStage;

    const model = fromStage ? stage : library;

    viewerItems.value = model.results.value.map((entry) => ({
      ...entry,
      tags: [...entry.tags],
    }));
    viewerIndex.value = viewerItems.value.findIndex(
      (entry) => entry.id === item.id,
    );

    if (viewerIndex.value < 0) {
      viewerItems.value = [{ ...item, tags: [...item.tags] }];
      viewerIndex.value = 0;
    }

    viewerTotal.value = isNativeData()
      ? Math.max(model.total.value, viewerItems.value.length)
      : viewerItems.value.length;
    viewerError.value = "";
    viewerPaging.value = false;
    viewerOpen.value = true;

    if (!isNativeData()) return;

    viewerPaging.value = true;

    try {
      const entries = await invoke<
        Pick<MediaItem, "id" | "filename" | "kind" | "extension">[]
      >("get_viewer_sequence", {
        library: !fromStage,
        search: stage.appliedSearch.value,
        filters: {
          scope: library.scope.value,
          search: library.appliedSearch.value,
          kind: library.kind.value,
          sort: fromStage ? stage.sortMode.value : library.sortMode.value,
          tagged: library.taggedOnly.value || library.hideUntagged.value,
          offset: 0,
        },
      });

      if (request !== generation) return;

      const known = new Map(
        viewerItems.value.map((entry) => [entry.nativeId, entry]),
      );

      const sequence = entries.map(
        (entry): MediaItem =>
          known.get(entry.id) ?? {
            ...entry,
            id: `database-${entry.id}`,
            nativeId: entry.id,
            path: "",
            preview: "",
            modifiedAt: 0,
            createdAt: 0,
            size: -1,
            width: 0,
            height: 0,
            favorite: false,
            copyrights: [],
            artist: "",
            characters: [],
            tags: [],
          },
      );

      const index = sequence.findIndex((entry) => entry.id === item.id);

      if (index >= 0) {
        viewerItems.value = sequence;
        viewerIndex.value = index;
      }

      viewerTotal.value = viewerItems.value.length;
    } catch (cause) {
      if (request === generation) {
        viewerError.value = String(cause);
        viewerTotal.value = viewerItems.value.length;
      }
    } finally {
      if (request === generation) viewerPaging.value = false;
    }
  }

  function closeViewer() {
    ++generation;
    viewerOpen.value = false;
    viewerItems.value = [];

    if (isNativeData()) {
      if (viewerStage.value) void stage.refreshRedistribution();
      else void library.refresh();
    }
  }

  function moveViewer(direction: number) {
    if (viewerPaging.value || !viewerOpen.value) return;

    const next = viewerIndex.value + direction;

    if (next < 0 || next >= viewerTotal.value) return;

    viewerError.value = "";

    if (next < viewerItems.value.length) viewerIndex.value = next;
  }

  function viewerAnnotations(annotations: {
    favorite: boolean;
    tags: string[];
  }) {
    const item = viewerItem.value;

    if (!item) return;

    Object.assign(item, annotations);

    const model = viewerStage.value ? stage : library;

    const original = model.media.value.find((entry) => entry.id === item.id);

    if (original)
      Object.assign(original, { ...annotations, tags: [...annotations.tags] });
  }

  return {
    viewerOpen,
    viewerItem,
    viewerStage,
    viewerIndex,
    viewerTotal,
    viewerHasNext,
    viewerPaging,
    viewerError,
    openViewer,
    closeViewer,
    moveViewer,
    viewerAnnotations,
  };
}

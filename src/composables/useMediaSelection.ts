import { computed, ref, watch, type ComputedRef } from "vue";

import type { MediaItem } from "../types/media";

export function useMediaSelection(results: ComputedRef<MediaItem[]>) {
  const selectedId = ref<string | null>(null);
  const selectedIds = ref<string[]>([]);
  const anchorId = ref<string | null>(null);

  const selected = computed(
    () => results.value.find((item) => item.id === selectedId.value) ?? null,
  );

  watch(results, (items) => {
    const visible = new Set(items.map((item) => item.id));

    selectedIds.value = selectedIds.value.filter((id) => visible.has(id));

    if (selectedId.value && !visible.has(selectedId.value))
      selectedId.value = null;

    if (anchorId.value && !visible.has(anchorId.value)) anchorId.value = null;
  });

  function beginSelection() {
    selectedId.value = null;
    anchorId.value = null;
  }

  function select(item: { id: string }, event?: MouseEvent) {
    const multiple = event?.ctrlKey || event?.metaKey;

    if (event?.shiftKey) {
      const items = results.value;

      const target = items.findIndex((entry) => entry.id === item.id);

      const anchor = items.findIndex((entry) => entry.id === anchorId.value);

      if (anchor >= 0 && target >= 0) {
        const ids = items
          .slice(Math.min(anchor, target), Math.max(anchor, target) + 1)
          .map((entry) => entry.id);

        selectedIds.value = multiple
          ? [...new Set([...selectedIds.value, ...ids])]
          : ids;
        selectedId.value = null;

        return;
      }
    }

    anchorId.value = item.id;

    if (multiple) {
      selectedId.value = null;

      selectedIds.value = selectedIds.value.includes(item.id)
        ? selectedIds.value.filter((id) => id !== item.id)
        : [...selectedIds.value, item.id];
    } else {
      selectedIds.value = [item.id];
      selectedId.value = item.id;
    }
  }

  function selectMany(ids: string[], focus = true) {
    selectedIds.value = ids;
    selectedId.value = focus && ids.length === 1 ? ids[0]! : null;
    anchorId.value = ids[ids.length - 1] ?? null;
  }

  function clearSelection() {
    selectedIds.value = [];
    beginSelection();
  }

  return {
    selectedId,
    selectedIds,
    selected,
    beginSelection,
    select,
    selectMany,
    clearSelection,
  };
}

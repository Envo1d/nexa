import { nextTick, ref, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../../runtime";
import type { MediaItem } from "../../types/media";
import type { LibraryModel, Notify } from "./model";

export function useTagEditor(
  library: LibraryModel,
  closeContext: () => void,
  notify: Notify,
  tagInput: Readonly<Ref<HTMLInputElement | null>>,
) {
  const tagTarget = ref<MediaItem | null>(null);
  const draftTags = ref<string[]>([]);
  const newTag = ref("");
  const tagError = ref("");
  const tagSaving = ref(false);

  async function editTags(item: MediaItem) {
    tagTarget.value = item;
    draftTags.value = [...item.tags];
    newTag.value = "";
    tagError.value = "";
    closeContext();

    await nextTick();

    tagInput.value?.focus();
  }

  function addDraftTag() {
    const value = newTag.value.trim();

    if (!value) return;

    if (value.length > 24) {
      tagError.value = "Keep tag names within 24 characters.";

      return;
    }

    if (
      !draftTags.value.some((tag) => tag.toLowerCase() === value.toLowerCase())
    )
      draftTags.value.push(value);

    newTag.value = "";
    tagError.value = "";
  }

  async function saveTags() {
    if (tagSaving.value) return;

    addDraftTag();

    if (tagError.value || !tagTarget.value) return;

    if (isNativeData()) {
      tagSaving.value = true;

      try {
        await invoke("save_library_tags", {
          id: tagTarget.value.nativeId,
          tags: draftTags.value,
        });

        await library.refresh();
      } catch (cause) {
        tagError.value = `Could not save tags: ${String(cause)}`;

        return;
      } finally {
        tagSaving.value = false;
      }
    } else tagTarget.value.tags = [...draftTags.value];

    tagTarget.value = null;
    notify("Tags updated.");
  }

  return {
    tagInput,
    tagTarget,
    draftTags,
    newTag,
    tagError,
    tagSaving,
    editTags,
    addDraftTag,
    saveTags,
  };
}

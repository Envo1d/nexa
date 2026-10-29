<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import { useModalFocus } from "../shared/composables/useModalFocus";
import AppIcon from "./AppIcon.vue";
import MediaPreview from "./MediaPreview.vue";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  items: MediaItem[];
  trigger: HTMLElement | null;
  showPreviews: boolean;
}>();

const emit = defineEmits<{ answer: [accepted: boolean] }>();

const dialog = ref<HTMLElement | null>(null);
const cancelButton = ref<HTMLButtonElement | null>(null);
const open = ref(true);

const visibleItems = computed(() => props.items.slice(0, 5));

const title = computed(() =>
  props.items.length === 1
    ? "Delete file?"
    : `Delete ${props.items.length} files?`,
);

const { trapFocus } = useModalFocus(open, cancelButton, dialog);

function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("answer", false);

    return;
  }

  trapFocus(event);
}

onMounted(() => cancelButton.value?.focus());

onBeforeUnmount(() => {
  const target = props.trigger?.isConnected
    ? props.trigger
    : document.querySelector<HTMLElement>(".collection-scroll");

  target?.focus({ preventScroll: true });
});
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="emit('answer', false)">
      <section
        ref="dialog"
        class="modal delete-dialog"
        role="alertdialog"
        tabindex="-1"
        aria-modal="true"
        aria-labelledby="delete-title"
        aria-describedby="delete-description delete-warning"
        @keydown.stop="keyboard"
      >
        <header class="panel-heading">
          <div class="delete-heading">
            <span class="delete-icon"><AppIcon name="trash" :size="22" /></span>
            <h2 id="delete-title">{{ ui(title) }}</h2>
          </div>
          <button
            type="button"
            class="icon-button"
            :aria-label="ui('Cancel deletion')"
            @click="emit('answer', false)"
          >
            <AppIcon name="close" />
          </button>
        </header>
        <p id="delete-description" class="delete-description">
          {{
            ui(
              `${items.length === 1 ? "This file" : "These files"} will be deleted from disk and removed from Nexa.`,
            )
          }}
        </p>
        <ul
          class="delete-files"
          :aria-label="ui('Files to delete')"
          tabindex="0"
        >
          <li v-for="item in visibleItems" :key="item.id" class="delete-file">
            <div class="delete-preview">
              <MediaPreview
                :item="item"
                :show-previews="showPreviews"
                variant="wizard"
              />
            </div>
            <span class="delete-filename">{{ item.filename }}</span>
          </li>
        </ul>
        <p v-if="items.length > visibleItems.length" class="delete-more">
          {{ ui(`And ${items.length - visibleItems.length} more files.`) }}
        </p>
        <p id="delete-warning" class="delete-warning">
          {{
            ui(
              "Files will be moved to the Recycle Bin when available. Windows will ask before permanently deleting files that cannot be recycled.",
            )
          }}
        </p>
        <footer class="modal-footer">
          <button
            ref="cancelButton"
            type="button"
            class="secondary-button"
            @click="emit('answer', false)"
          >
            {{ ui("Cancel") }}
          </button>
          <button
            type="button"
            class="primary-button delete-confirm"
            @click="emit('answer', true)"
          >
            <AppIcon name="trash" :size="15" />
            {{
              ui(
                items.length === 1
                  ? "Delete file"
                  : `Delete ${items.length} files`,
              )
            }}
          </button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.delete-dialog {
  width: min(500px, calc(100vw - 40px));
}
.delete-heading {
  display: flex;
  align-items: center;
  gap: 12px;
}
.delete-icon {
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  border: 1px solid var(--danger-line);
  border-radius: 10px;
  background: color-mix(in srgb, var(--danger-bg) 33%, transparent);
  color: var(--danger);
}
.delete-description,
.delete-more {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  margin: 8px 0 16px;
}
.delete-files {
  display: grid;
  gap: 8px;
  max-height: 260px;
  overflow: auto;
  list-style: none;
  padding: 0;
  margin: 0;
}
.delete-file {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-alt);
}
.delete-preview {
  flex: 0 0 96px;
}
.delete-preview :deep(.wizard-preview) {
  height: 64px;
  min-height: 0;
  margin: 0;
}
.delete-preview :deep(.wizard-preview img) {
  height: 100%;
}
.delete-preview :deep(.preview-placeholder) {
  width: 30px;
  height: 30px;
}
.delete-filename {
  min-width: 0;
  overflow-wrap: anywhere;
  color: var(--text);
  font-size: 12px;
  line-height: 1.5;
}
.delete-warning {
  padding: 12px;
  margin: 16px 0 0;
  border: 1px solid var(--danger-line);
  border-radius: 8px;
  background: color-mix(in srgb, var(--danger-bg) 20%, transparent);
  color: var(--danger);
  font-size: 12px;
  line-height: 1.7;
}
.delete-confirm {
  border-color: var(--danger-line);
  background: linear-gradient(
    120deg,
    var(--danger-button),
    var(--danger-button-end)
  );
}
.delete-confirm:hover {
  background: var(--danger-button-hover);
}
.delete-dialog button:focus-visible,
.delete-files:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 3px;
}
</style>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { translateUi as ui } from "../i18n/presentation";
import { useFocusTrap } from "../shared/composables/useFocusTrap";
import AppIcon from "./AppIcon.vue";
import DuplicateOrderRow from "./DuplicateOrderRow.vue";

import type { MediaItem, MediaKind } from "../types/media";

const props = defineProps<{ item: MediaItem; showPreviews: boolean }>();

const emit = defineEmits<{ close: []; saved: [] }>();

type Group = {
  key: string;
  files: {
    id: string;
    filename: string;
    kind: MediaKind;
    revision: string | null;
  }[];
};

const dialog = ref<HTMLElement | null>(null);

const { trapFocus } = useFocusTrap(dialog);

const files = ref<MediaItem[]>([]);
const originalIds = ref<string[]>([]);
const key = ref("");
const loading = ref(false);
const saving = ref(false);
const error = ref("");
const dragging = ref<string | null>(null);
const dropTarget = ref<string | null>(null);

const busy = computed(() => loading.value || saving.value);

const changed = computed(() =>
  files.value.some((file, index) => file.nativeId !== originalIds.value[index]),
);

let trigger: HTMLElement | null = null;
let disposed = false;

async function load() {
  loading.value = true;
  error.value = "";
  dragging.value = null;
  dropTarget.value = null;

  try {
    const group = await invoke<Group>("get_duplicate_order", {
      id: props.item.nativeId,
    });

    if (disposed) return;

    key.value = group.key;

    originalIds.value = group.files.map((file) => file.id);

    files.value = group.files.map((file) => ({
      ...props.item,
      id: `order-${file.id}`,
      nativeId: file.id,
      nativeRevision: file.revision ?? "",
      filename: file.filename,
      kind: file.kind,
      availability: "present",
      preview: "",
      duration: undefined,
    }));
  } catch (cause) {
    if (!disposed) error.value = String(cause);
  } finally {
    if (!disposed) loading.value = false;
  }
}

async function move(id: string, destination: number) {
  if (busy.value || error.value) return;

  const source = files.value.findIndex((file) => file.id === id);

  if (
    source < 0 ||
    destination < 0 ||
    destination >= files.value.length ||
    source === destination
  )
    return;

  const [file] = files.value.splice(source, 1);

  files.value.splice(destination, 0, file!);

  await nextTick();

  const row = dialog.value?.querySelector<HTMLElement>(
    `[data-file-id="${id}"]`,
  );

  row?.focus({ preventScroll: true });
  row?.scrollIntoView({ block: "nearest" });
}

function dragStart(event: DragEvent, id: string) {
  if (busy.value || !event.dataTransfer) {
    event.preventDefault();

    return;
  }

  dragging.value = id;
  event.dataTransfer.effectAllowed = "move";
  event.dataTransfer.setData("text/plain", id);
}

function drop(index: number) {
  if (dragging.value) void move(dragging.value, index);

  dragging.value = null;
  dropTarget.value = null;
}

async function save() {
  if (busy.value || !changed.value || error.value) return;

  saving.value = true;

  try {
    await invoke("save_duplicate_order", {
      key: key.value,
      expectedIds: originalIds.value,
      ids: files.value.map((file) => file.nativeId),
    });

    emit("saved");
    emit("close");
  } catch (cause) {
    error.value = String(cause);
  } finally {
    saving.value = false;
  }
}

function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();

    if (!saving.value) emit("close");
  }

  trapFocus(event);
}

onMounted(() => {
  trigger =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  dialog.value?.querySelector<HTMLButtonElement>("button")?.focus();
  void load();
});

onBeforeUnmount(() => {
  disposed = true;

  if (trigger?.isConnected) trigger.focus();
});
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="!saving && emit('close')">
      <section
        ref="dialog"
        class="order-dialog"
        role="dialog"
        tabindex="-1"
        aria-modal="true"
        aria-labelledby="order-title"
        :aria-busy="busy"
        @keydown.stop="keyboard"
      >
        <header class="panel-heading">
          <h2 id="order-title">{{ ui("Change file order") }}</h2>
          <button
            type="button"
            class="icon-button"
            :aria-label="ui('Close order editor')"
            :disabled="saving"
            @click="emit('close')"
          >
            <AppIcon name="close" />
          </button>
        </header>
        <p class="order-hint muted">
          {{
            ui(
              "Drag files or use the arrows to change their filename indices. Renaming runs when Sync with disk is enabled.",
            )
          }}
        </p>
        <p v-if="loading" class="order-hint" role="status">
          {{ ui("Loading the full group…") }}
        </p>
        <div v-if="error" class="order-error">
          <p class="error-message" role="alert">{{ ui(error) }}</p>
          <button
            type="button"
            class="secondary-button"
            :disabled="busy"
            @click="load"
          >
            {{ ui("Reload group") }}
          </button>
        </div>
        <p
          v-if="!loading && !error && files.length < 2"
          class="order-hint muted"
        >
          {{ ui("This file has no matching files to reorder.") }}
        </p>
        <ol
          v-if="files.length"
          class="order-list"
          :aria-label="ui('File index order')"
        >
          <li
            v-for="(file, index) in files"
            :key="file.id"
            :data-file-id="file.id"
            tabindex="0"
            :draggable="!busy"
            :class="{
              dragging: dragging === file.id,
              'drop-target': dropTarget === file.id,
            }"
            @dragstart="dragStart($event, file.id)"
            @dragover.prevent="dragging && (dropTarget = file.id)"
            @drop.prevent="drop(index)"
            @dragend="
              dragging = null;
              dropTarget = null;
            "
            @keydown.alt.up.prevent="move(file.id, index - 1)"
            @keydown.alt.down.prevent="move(file.id, index + 1)"
          >
            <DuplicateOrderRow
              :item="file"
              :index="index"
              :total="files.length"
              :busy="busy || Boolean(error)"
              :show-previews="showPreviews"
              @move="move(file.id, index + $event)"
            />
          </li>
        </ol>
        <footer class="order-footer">
          <span class="muted" aria-live="polite"
            >{{ files.length }} {{ ui("files")
            }}{{ changed ? " · Order changed" : "" }}</span
          >
          <button
            type="button"
            class="secondary-button"
            :disabled="saving"
            @click="emit('close')"
          >
            {{ ui("Cancel") }}
          </button>
          <button
            type="button"
            class="primary-button"
            :disabled="busy || !changed || Boolean(error)"
            @click="save"
          >
            {{ ui(saving ? "Saving…" : "Save order") }}
          </button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.order-dialog {
  width: min(720px, calc(100vw - 48px));
  max-height: calc(100dvh - 48px);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface);
  box-shadow: 0 24px 80px var(--shadow);
}
.order-hint {
  padding: 0 18px 14px;
  line-height: 1.6;
  flex-shrink: 0;
}
.order-error {
  padding: 0 18px 14px;
  display: grid;
  gap: 10px;
}
.order-list {
  list-style: none;
  padding: 4px;
  margin: 0 14px;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  border: 1px solid var(--line);
  border-radius: 8px;
}
.order-list li {
  border: 1px solid transparent;
  border-radius: 6px;
  cursor: grab;
}
.order-list li + li {
  margin-top: 4px;
}
.order-list li.dragging {
  opacity: 0.45;
}
.order-list li.drop-target {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}
.order-list li:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.order-footer {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 16px 18px;
  flex-shrink: 0;
}
.order-footer > span {
  margin-right: auto;
}
</style>

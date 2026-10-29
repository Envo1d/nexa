<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { translateUi as ui } from "../i18n/presentation";
import { useFocusTrap } from "../shared/composables/useFocusTrap";
import { useSearchSuggestions } from "../shared/composables/useSearchSuggestions";
import ReplacementFile from "./ReplacementFile.vue";
import AppIcon from "./AppIcon.vue";
import {
  catalogSuggestions,
  chooseMetadataSuggestion,
  metadataSuggestions,
  normalizeMetadataSearch,
  type MetadataCatalog,
} from "../utils/metadataSearch";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  item: MediaItem;
  showPreviews: boolean;
  opening: boolean;
  catalog?: MetadataCatalog;
  preferredMainId?: string;
}>();

const emit = defineEmits<{
  close: [];
  replaced: [];
  open: [item: MediaItem, library: boolean];
}>();

type NativeFile = Omit<
  MediaItem,
  "size" | "width" | "height" | "modifiedAt" | "createdAt" | "artist"
> & {
  token: string;
  revision: string | null;
  size: number | null;
  width: number | null;
  height: number | null;
  modifiedAt: number | null;
  createdAt: number | null;
  artist: string | null;
};

type Choice = { item: MediaItem; token: string };

function choice(file: NativeFile): Choice {
  return {
    token: file.token,
    item: {
      ...file,
      id: `replacement-${file.id}`,
      nativeId: file.id,
      nativeRevision: file.revision ?? "imported",
      preview: "",
      favorite: false,
      artist: file.artist ?? "",
      copyrights: [],
      characters: [],
      tags: [],
      size: file.size ?? -1,
      width: file.width ?? 0,
      height: file.height ?? 0,
      modifiedAt: file.modifiedAt ?? 0,
      createdAt: file.createdAt ?? 0,
      duration: file.duration == null ? undefined : file.duration / 1000,
    },
  };
}

const dialog = ref<HTMLElement | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const source = ref<Choice | null>(null);
const selected = ref<Choice | null>(null);
const files = ref<Choice[]>([]);
const query = ref("");
const appliedQuery = ref("");

const suggestions = computed(() =>
  metadataSuggestions(query.value, catalogSuggestions(props.catalog)),
);

const cursor = ref<string | null>(null);
const syncWithDisk = ref(false);
const hasAnnotations = ref(false);
const loading = ref(false);
const saving = ref(false);
const error = ref("");
const needsReload = ref(false);
const acknowledged = ref(false);

const busy = computed(() => loading.value || saving.value || props.opening);

let version = 0;
let disposed = false;
let trigger: HTMLElement | null = null;

const { trapFocus } = useFocusTrap(dialog);

const {
  searchFocused,
  suggestionIndex,
  chooseSearchSuggestion,
  searchKeydown,
} = useSearchSuggestions(
  query,
  suggestions,
  chooseSuggestion,
  commitSearch,
  searchInput,
);

async function load(append = false) {
  if (
    saving.value ||
    needsReload.value ||
    !source.value ||
    (append && loading.value)
  )
    return;

  const request = ++version;

  loading.value = true;
  error.value = "";

  if (!append) {
    files.value = [];
    cursor.value = null;
  }

  try {
    const page = await invoke<{ files: NativeFile[]; next: string | null }>(
      "get_replacement_candidates",
      {
        kind: source.value.item.kind,
        search: appliedQuery.value,
        after: append ? cursor.value : null,
      },
    );

    if (disposed || request !== version) return;

    files.value = append
      ? [...files.value, ...page.files.map(choice)]
      : page.files.map(choice);
    cursor.value = page.next;
  } catch (cause) {
    if (!disposed && request === version) error.value = String(cause);
  } finally {
    if (!disposed && request === version) loading.value = false;
  }
}

async function reload() {
  if (loading.value || saving.value) return;

  const request = ++version;

  source.value = null;
  files.value = [];
  cursor.value = null;
  selected.value = null;
  acknowledged.value = false;
  loading.value = true;
  error.value = "";

  try {
    const result = await invoke<{
      file: NativeFile;
      syncWithDisk: boolean;
      hasAnnotations: boolean;
      preferred: NativeFile | null;
    }>("get_replacement_source", {
      id: props.item.nativeId,
      preferredMainId: props.preferredMainId ?? null,
    });

    if (disposed || request !== version) return;

    source.value = choice(result.file);
    syncWithDisk.value = result.syncWithDisk;
    hasAnnotations.value = result.hasAnnotations;
    needsReload.value = false;
    selected.value = result.preferred ? choice(result.preferred) : null;

    await load();
  } catch (cause) {
    if (!disposed && request === version) {
      error.value = String(cause);
      loading.value = false;
    }
  }
}

function commitSearch() {
  if (saving.value || needsReload.value) return;

  query.value = normalizeMetadataSearch(query.value);
  appliedQuery.value = query.value;
  searchFocused.value = false;
  selected.value = null;
  acknowledged.value = false;
  void load();
}

function chooseSuggestion(name: string) {
  query.value = chooseMetadataSuggestion(query.value, name);
  commitSearch();
}

function select(file: Choice) {
  if (busy.value || needsReload.value) return;

  selected.value = file;
  acknowledged.value = false;
}

async function save() {
  if (
    !source.value ||
    !selected.value ||
    busy.value ||
    !acknowledged.value ||
    error.value ||
    needsReload.value
  )
    return;

  saving.value = true;

  try {
    await invoke("replace_library_file", {
      request: {
        mainId: selected.value.item.nativeId,
        stageId: source.value.item.nativeId,
        mainToken: selected.value.token,
        stageToken: source.value.token,
        syncWithDisk: syncWithDisk.value,
      },
    });

    emit("replaced");
    emit("close");
  } catch (cause) {
    needsReload.value = true;
    error.value = `${String(cause)}. If replacement already started, Retry disk synchronization completes recovery before further file actions.`;
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

onMounted(async () => {
  trigger =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  dialog.value?.querySelector<HTMLButtonElement>("button")?.focus();

  await reload();

  await nextTick();

  if (!disposed) searchInput.value?.focus();
});

onBeforeUnmount(() => {
  disposed = true;
  ++version;

  if (trigger?.isConnected) trigger.focus();
});
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="!saving && emit('close')">
      <section
        ref="dialog"
        class="replacement-dialog"
        role="dialog"
        tabindex="-1"
        aria-modal="true"
        aria-labelledby="replacement-title"
        :aria-busy="busy"
        @keydown.stop="keyboard"
      >
        <header class="panel-heading">
          <h2 id="replacement-title">{{ ui("Replace in Main Library") }}</h2>
          <button
            type="button"
            class="icon-button"
            :aria-label="ui('Close replacement')"
            :disabled="saving"
            @click="emit('close')"
          >
            <AppIcon name="close" />
          </button>
        </header>
        <div class="replacement-body">
          <div v-if="source" class="version-panel">
            <h3>{{ ui("New version · Stage") }}</h3>
            <ReplacementFile
              :item="source.item"
              :show-previews="showPreviews"
              :open-label="ui('Open new Stage version')"
              :open-disabled="saving || opening"
              @open="emit('open', source.item, false)"
            />
          </div>
          <template v-if="source && !selected">
            <label class="search-label" for="replacement-search">{{
              ui("Choose the file to replace in Main")
            }}</label>
            <div class="metadata-search">
              <div class="search-box">
                <AppIcon name="search" :size="17" />
                <input
                  id="replacement-search"
                  ref="searchInput"
                  v-model="query"
                  type="text"
                  :placeholder="ui('Metadata, separated by semicolons')"
                  :disabled="saving"
                  maxlength="128"
                  role="combobox"
                  aria-autocomplete="list"
                  :aria-expanded="searchFocused && suggestions.length > 0"
                  aria-controls="replacement-suggestions"
                  :aria-activedescendant="
                    searchFocused && suggestionIndex >= 0
                      ? `replacement-option-${suggestionIndex}`
                      : undefined
                  "
                  @focus="searchFocused = true"
                  @input="searchFocused = true"
                  @blur="searchFocused = false"
                  @keydown="searchKeydown"
                />
                <button
                  v-if="query"
                  type="button"
                  class="icon-button"
                  :aria-label="ui('Clear replacement search')"
                  :disabled="saving"
                  @click="
                    query = '';
                    commitSearch();
                    searchInput?.focus();
                  "
                >
                  <AppIcon name="close" :size="14" />
                </button>
              </div>
              <div
                v-if="searchFocused && suggestions.length"
                id="replacement-suggestions"
                class="metadata-suggestions"
                role="listbox"
                :aria-label="ui('Matching metadata')"
              >
                <button
                  v-for="(suggestion, index) in suggestions"
                  :id="`replacement-option-${index}`"
                  :key="
                    JSON.stringify([
                      suggestion.category,
                      suggestion.name,
                      suggestion.copyright,
                    ])
                  "
                  type="button"
                  role="option"
                  :aria-selected="suggestionIndex === index"
                  :class="{ active: suggestionIndex === index }"
                  @mousedown.prevent
                  @click="chooseSearchSuggestion(suggestion.name)"
                >
                  <span
                    >{{ suggestion.name
                    }}{{
                      suggestion.copyright ? ` (${suggestion.copyright})` : ""
                    }}</span
                  ><small>{{ ui(suggestion.category) }}</small>
                </button>
              </div>
            </div>
            <p class="muted">
              {{
                ui(
                  "Enter artist, character, copyright or tag names separated by semicolons. Every value must match, just like Main. Press Enter or choose a suggestion to search the entire Main Library.",
                )
              }}
            </p>
            <div class="candidate-list" :aria-label="ui('Main Library files')">
              <button
                v-for="file in files"
                :key="file.item.id"
                type="button"
                class="candidate-button"
                :disabled="busy || needsReload"
                @click="select(file)"
              >
                <ReplacementFile
                  :item="file.item"
                  :show-previews="showPreviews"
                />
              </button>
            </div>
            <p v-if="!loading && !files.length && !error" class="muted">
              {{ ui("No matching files in Main.") }}
            </p>
            <button
              v-if="cursor"
              type="button"
              class="secondary-button"
              :disabled="busy"
              @click="load(true)"
            >
              {{ ui("Load more files") }}
            </button>
          </template>
          <template v-if="selected && source">
            <div class="version-panel">
              <h3>{{ ui("Current version · Main") }}</h3>
              <ReplacementFile
                :item="selected.item"
                :show-previews="showPreviews"
                :open-label="ui('Open current Main version')"
                :open-disabled="saving || opening"
                @open="emit('open', selected.item, true)"
              />
            </div>
            <button
              type="button"
              class="secondary-button"
              :disabled="saving"
              @click="
                selected = null;
                acknowledged = false;
                nextTick(() => searchInput?.focus());
              "
            >
              {{ ui("Choose another file") }}
            </button>
            <p>
              {{
                ui(
                  "Main keeps its artist, copyrights, characters, tags, favorite, video title and file order. The new content, format and technical details replace the current version.",
                )
              }}
            </p>
            <p v-if="hasAnnotations" class="replacement-warning">
              {{
                ui(
                  "The Stage file's existing annotations will be discarded. Main's annotations take precedence.",
                )
              }}
            </p>
            <p v-if="syncWithDisk" class="muted">
              {{
                ui(
                  "The new version will move into the managed Library structure through disk synchronization.",
                )
              }}
            </p>
            <p v-else class="muted">
              {{
                ui(
                  "Sync with disk is off. The new version stays at its current location and becomes the Main file.",
                )
              }}
            </p>
            <p class="replacement-warning">
              {{
                ui(
                  "After the replacement is saved successfully, the old file is permanently deleted. Temporary recovery backups are removed after cleanup.",
                )
              }}
            </p>
            <label class="replacement-confirm"
              ><input
                v-model="acknowledged"
                type="checkbox"
                :disabled="saving"
              />
              {{
                ui("Replace this Main file with the selected Stage version.")
              }}</label
            >
          </template>
          <p v-if="loading" role="status" class="muted">{{ ui("Loading…") }}</p>
          <p v-if="error" class="error-message" role="alert">{{ ui(error) }}</p>
          <button
            v-if="error"
            type="button"
            class="secondary-button"
            :disabled="busy"
            @click="reload"
          >
            {{ ui("Reload files") }}
          </button>
        </div>
        <footer class="replacement-footer">
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
            :disabled="busy || !selected || !acknowledged || Boolean(error)"
            @click="save"
          >
            {{ ui(saving ? "Replacing…" : "Replace file") }}
          </button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.replacement-dialog {
  width: min(780px, calc(100vw - 48px));
  max-height: calc(100dvh - 48px);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface);
  box-shadow: 0 24px 80px var(--shadow);
}
.replacement-body {
  overflow-y: auto;
  min-height: 0;
  padding: 0 18px 16px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.replacement-body p {
  line-height: 1.6;
}
.version-panel {
  padding: 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
}
.version-panel h3 {
  margin: 0 0 12px;
  font-size: 13px;
  color: var(--accent);
}
.search-label {
  font-weight: 600;
}
.candidate-list {
  display: grid;
  gap: 6px;
}
.candidate-button {
  width: 100%;
  padding: 10px;
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 8px;
  color: inherit;
  cursor: pointer;
}
.candidate-button:hover {
  background: color-mix(in srgb, var(--accent) 8%, transparent);
  border-color: var(--accent);
}
.replacement-confirm {
  display: flex;
  gap: 10px;
  align-items: center;
  line-height: 1.6;
}
.replacement-confirm input {
  accent-color: var(--accent);
}
.replacement-warning {
  color: var(--warning);
}
.replacement-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 16px 18px;
  border-top: 1px solid var(--line);
}
</style>

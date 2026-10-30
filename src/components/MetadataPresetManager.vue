<script setup lang="ts">
import { computed, nextTick, onMounted, ref, useId, watch } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import { searchByName } from "../utils/metadataSearch";
import { useFocusTrap } from "../shared/composables/useFocusTrap";
import AppIcon from "./AppIcon.vue";
import MetadataInput, { type MetadataValue } from "./MetadataInput.vue";

import type { MetadataPreset, PresetMetadata } from "../types/metadataPresets";

const props = defineProps<{
  presets: MetadataPreset[];
  metadata: PresetMetadata;
  create: boolean;
  busy: boolean;
  error: string;
  notice: string;
  demo: boolean;
  catalog?: {
    artists: string[];
    copyrights: string[];
    tags: string[];
    characters: { name: string; copyright: string }[];
  };
}>();

const emit = defineEmits<{
  close: [];
  save: [id: string | null, name: string, metadata: PresetMetadata];
  delete: [id: string];
}>();

const id = useId();

const dialog = ref<HTMLElement | null>(null);

const { trapFocus } = useFocusTrap(dialog);

const selected = ref<string | null>(null);
const name = ref("");
const artist = ref("");
const copyrights = ref<MetadataValue[]>([]);
const characters = ref<MetadataValue[]>([]);
const tags = ref<MetadataValue[]>([]);
const query = ref("");
const confirmingDelete = ref(false);

const visiblePresets = computed(() => searchByName(props.presets, query.value));

function values(names: string[]): MetadataValue[] {
  return names.map((name) => ({ id: name, name }));
}

const copyrightOptions = computed(() =>
  values(props.catalog?.copyrights ?? []),
);

const artistOptions = computed(() => values(props.catalog?.artists ?? []));

const tagOptions = computed(() => values(props.catalog?.tags ?? []));

const characterOptions = computed(() =>
  (props.catalog?.characters ?? [])
    .filter((entry) =>
      copyrights.value.some((value) => value.name === entry.copyright),
    )
    .map((entry) => ({
      ...entry,
      id: JSON.stringify([entry.copyright, entry.name]),
    })),
);

function fill(metadata: PresetMetadata) {
  artist.value = metadata.artist;
  copyrights.value = values(metadata.copyrights);
  characters.value = metadata.characters.map((entry) => ({
    ...entry,
    id: JSON.stringify([entry.copyright, entry.name]),
  }));
  tags.value = values(metadata.tags);
  confirmingDelete.value = false;
}

function choose(preset: MetadataPreset) {
  if (props.busy) return;

  selected.value = preset.id;
  name.value = preset.name;
  fill(preset.metadata);
}

function createFromCurrent() {
  if (props.busy) return;

  selected.value = null;
  name.value = "";
  fill(props.metadata);
}

function duplicate() {
  if (props.busy) return;

  selected.value = null;
  name.value = "";
  confirmingDelete.value = false;
}

function save() {
  if (props.busy || !name.value.trim() || !artist.value.trim()) return;

  emit("save", selected.value, name.value, {
    artist: artist.value,
    copyrights: copyrights.value.map((value) => value.name),
    characters: characters.value.map((value) => ({
      name: value.name,
      copyright: value.copyright ?? "",
    })),
    tags: tags.value.map((value) => value.name),
  });
}

function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape" && !event.defaultPrevented) {
    event.preventDefault();

    if (confirmingDelete.value) confirmingDelete.value = false;
    else if (!props.busy) emit("close");

    return;
  }

  trapFocus(event);
}

watch(copyrights, (entries) => {
  characters.value = characters.value.filter((character) =>
    entries.some((entry) => entry.name === character.copyright),
  );
});

watch(
  () => props.presets,
  (presets) => {
    if (
      selected.value &&
      !presets.some((preset) => preset.id === selected.value)
    )
      createFromCurrent();
  },
);

onMounted(async () => {
  const first = props.presets[0];

  if (!props.create && first) choose(first);
  else createFromCurrent();

  await nextTick();

  dialog.value?.focus();
});
</script>

<template>
  <Teleport to="body">
    <div
      class="modal-backdrop preset-manager-backdrop"
      @click.self="!busy && emit('close')"
    >
      <section
        ref="dialog"
        class="preset-manager"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="`${id}-title`"
        :aria-busy="busy"
        tabindex="-1"
        @keydown.stop="keyboard"
      >
        <header class="panel-heading">
          <div>
            <h2 :id="`${id}-title`">{{ ui("Manage metadata presets") }}</h2>
            <p class="muted">
              {{
                ui("Edit reusable metadata independently of the current file.")
              }}
            </p>
          </div>
          <button
            class="icon-button"
            :aria-label="ui('Close')"
            :disabled="busy"
            @click="emit('close')"
          >
            <AppIcon name="close" :size="18" />
          </button>
        </header>
        <div class="preset-manager-body">
          <aside class="preset-library" :aria-label="ui('Metadata presets')">
            <button
              class="secondary-button"
              :disabled="busy"
              @click="createFromCurrent"
            >
              <AppIcon name="plus" :size="15" />{{
                ui("New from current details")
              }}
            </button>
            <input
              v-model="query"
              type="search"
              :aria-label="ui('Search presets')"
              :placeholder="ui('Search presets')"
            />
            <div class="preset-library-list">
              <button
                v-for="preset in visiblePresets"
                :key="preset.id"
                class="preset-library-item"
                :class="{ active: selected === preset.id }"
                :aria-pressed="selected === preset.id"
                :disabled="busy"
                @click="choose(preset)"
              >
                <strong>{{ preset.name }}</strong
                ><small>{{ preset.metadata.artist }}</small>
              </button>
              <p v-if="!visiblePresets.length" class="muted">
                {{
                  ui(
                    presets.length ? "No matching presets." : "No presets yet.",
                  )
                }}
              </p>
            </div>
          </aside>
          <div class="preset-editor">
            <p class="muted">
              {{ ui(selected ? "Edit preset" : "New preset") }}
            </p>
            <label class="preset-name"
              >{{ ui("Preset name")
              }}<input v-model="name" :disabled="busy" maxlength="256"
            /></label>
            <MetadataInput
              v-model:text-value="artist"
              text-input
              label="Artist"
              :options="artistOptions"
              :disabled="busy"
            />
            <MetadataInput
              v-model="copyrights"
              label="Copyrights"
              :options="copyrightOptions"
              allow-custom
              :disabled="busy"
            />
            <MetadataInput
              v-model="characters"
              label="Characters"
              :options="characterOptions"
              :custom-copyrights="copyrights.map((value) => value.name)"
              :disabled="busy || !copyrights.length"
            />
            <MetadataInput
              v-model="tags"
              label="Tags"
              :options="tagOptions"
              allow-custom
              :disabled="busy"
            />
            <p class="muted">
              {{
                ui("Saving a preset does not change the current form or files.")
              }}
            </p>
            <p v-if="demo" class="muted">
              {{ ui("Demo presets last only while this dialog is open.") }}
            </p>
            <p v-if="notice" role="status" class="muted">{{ ui(notice) }}</p>
            <p v-if="error" role="alert" class="error-message">
              {{ ui(error) }}
            </p>
          </div>
        </div>
        <footer class="preset-manager-footer">
          <div
            v-if="confirmingDelete"
            class="preset-delete-confirmation"
            role="group"
            :aria-label="ui('Confirm deletion')"
          >
            <p>
              {{
                ui(
                  "Delete the selected preset? Your form and files will be kept.",
                )
              }}
            </p>
            <button
              class="secondary-button"
              :disabled="busy"
              @click="selected && emit('delete', selected)"
            >
              {{ ui("Confirm deletion") }}
            </button>
            <button
              class="secondary-button"
              :disabled="busy"
              @click="confirmingDelete = false"
            >
              {{ ui("Cancel") }}
            </button>
          </div>
          <template v-else>
            <button
              class="secondary-button"
              :disabled="busy || !selected"
              @click="confirmingDelete = true"
            >
              <AppIcon name="trash" :size="15" />{{ ui("Delete preset") }}
            </button>
            <button
              class="secondary-button"
              :disabled="busy || !selected"
              @click="duplicate"
            >
              <AppIcon name="copy" :size="15" />{{ ui("Duplicate preset") }}
            </button>
            <span class="preset-footer-spacer"></span>
            <button
              class="secondary-button"
              :disabled="busy"
              @click="emit('close')"
            >
              {{ ui("Cancel") }}
            </button>
            <button
              class="primary-button"
              :disabled="busy || !name.trim() || !artist.trim()"
              @click="save"
            >
              {{ ui(selected ? "Update selected preset" : "Save new preset") }}
            </button>
          </template>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.preset-manager-backdrop {
  z-index: 220;
}
.preset-manager {
  position: relative;
  display: flex;
  flex-direction: column;
  width: min(900px, calc(100vw - 40px));
  max-height: calc(100dvh - 40px);
  border: 1px solid var(--line);
  border-radius: 14px;
  background: var(--surface);
  box-shadow: 0 20px 70px var(--shadow);
}
.preset-manager .panel-heading {
  flex-shrink: 0;
  gap: 16px;
  padding: 18px 22px;
  border-bottom: 1px solid var(--line);
}
.preset-manager h2 {
  margin: 0 0 6px;
  font-size: 18px;
}
.preset-manager p {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
}
.preset-manager-body {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  min-height: 0;
  overflow: auto;
}
.preset-library {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
  padding: 18px;
  border-right: 1px solid var(--line);
  background: var(--surface-alt);
}
.preset-library-list {
  display: flex;
  flex-direction: column;
  gap: 5px;
  overflow-y: auto;
  max-height: 390px;
}
.preset-library-item {
  display: grid;
  gap: 4px;
  padding: 10px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text);
  text-align: left;
  overflow-wrap: anywhere;
}
.preset-library-item small {
  color: var(--muted);
}
.preset-library-item:hover {
  background: var(--surface-hover);
}
.preset-library-item.active {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.preset-editor {
  display: grid;
  align-content: start;
  gap: 16px;
  min-width: 0;
  padding: 20px 22px;
}
.preset-name {
  display: grid;
  gap: 7px;
  color: var(--muted);
  font-size: 12px;
}
.preset-manager input {
  width: 100%;
  min-width: 0;
  height: 35px;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface-input);
  color: var(--text);
}
.preset-manager input:focus-visible {
  border-color: var(--accent);
}
.preset-manager-footer {
  display: flex;
  flex-shrink: 0;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 16px 22px;
  border-top: 1px solid var(--line);
}
.preset-footer-spacer {
  flex: 1;
}
.preset-delete-confirmation {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.preset-delete-confirmation p {
  flex-basis: 100%;
}
@media (max-width: 680px) {
  .preset-manager-body {
    grid-template-columns: minmax(0, 1fr);
  }
  .preset-library {
    border-right: 0;
    border-bottom: 1px solid var(--line);
  }
  .preset-library-list {
    max-height: 130px;
  }
}
</style>

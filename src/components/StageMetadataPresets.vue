<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";

import { translateUi as ui } from "../i18n/presentation";
import AppIcon from "./AppIcon.vue";
import AppSelect from "./AppSelect.vue";

import type { MetadataPreset, PresetMetadata } from "../types/metadataPresets";

const props = defineProps<{
  metadata: PresetMetadata;
  disabled: boolean;
}>();

const emit = defineEmits<{ apply: [metadata: PresetMetadata] }>();

const presets = ref<MetadataPreset[]>([]);

const selected = ref("");

const name = ref("");

const busy = ref(false);

const loaded = ref(false);

const error = ref("");

const notice = ref("");

const confirmingDelete = ref(false);

const blocked = computed(() => props.disabled || busy.value || !loaded.value);

const options = computed(() => [
  { value: "", label: ui("Choose a metadata preset") },
  ...presets.value.map((preset) => ({ value: preset.id, label: preset.name })),
]);

async function load() {
  busy.value = true;
  error.value = "";

  try {
    presets.value = isTauri()
      ? await invoke<MetadataPreset[]>("get_metadata_presets")
      : [];
    loaded.value = true;
  } catch (cause) {
    error.value = `Could not load presets: ${String(cause)}`;
  } finally {
    busy.value = false;
  }
}

function apply(id: string) {
  if (blocked.value) return;

  selected.value = id;
  confirmingDelete.value = false;

  const preset = presets.value.find((preset) => preset.id === id);

  if (!preset) return;

  name.value = preset.name;
  error.value = "";
  notice.value = "Preset applied to the form. Review before distributing.";
  emit("apply", JSON.parse(JSON.stringify(preset.metadata)) as PresetMetadata);
}

async function save(update: boolean) {
  if (blocked.value || (update && !selected.value)) return;

  busy.value = true;
  error.value = "";
  notice.value = "";
  confirmingDelete.value = false;

  try {
    const presetName = name.value.trim();

    if (!presetName || !props.metadata.artist.trim())
      throw new Error(ui("Enter a preset name and an artist."));

    const id = update ? selected.value : null;

    const metadata = JSON.parse(
      JSON.stringify(props.metadata),
    ) as PresetMetadata;

    if (
      !isTauri() &&
      presets.value.some(
        (preset) =>
          preset.id !== id &&
          preset.name.toLocaleLowerCase() === presetName.toLocaleLowerCase(),
      )
    )
      throw new Error(ui("A preset with this name already exists."));

    const preset = isTauri()
      ? await invoke<MetadataPreset>("save_metadata_preset", {
          id,
          name: presetName,
          metadata,
        })
      : { id: id ?? crypto.randomUUID(), name: presetName, metadata };

    presets.value = [
      ...presets.value.filter((entry) => entry.id !== preset.id),
      preset,
    ].sort((a, b) => a.name.localeCompare(b.name));
    selected.value = preset.id;
    name.value = preset.name;
    notice.value = "Metadata preset saved.";
  } catch (cause) {
    error.value = `Could not save preset: ${String(cause)}`;
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (blocked.value || !selected.value) return;

  busy.value = true;
  error.value = "";
  notice.value = "";

  try {
    if (isTauri())
      await invoke("delete_metadata_preset", { id: selected.value });

    presets.value = presets.value.filter(
      (preset) => preset.id !== selected.value,
    );
    selected.value = "";
    name.value = "";
    confirmingDelete.value = false;
    notice.value = "Metadata preset deleted. The form is unchanged.";
  } catch (cause) {
    error.value = `Could not delete preset: ${String(cause)}`;
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section
    class="stage-presets"
    :aria-label="ui('Metadata presets')"
    :aria-busy="busy"
  >
    <div class="preset-picker">
      <AppSelect
        :model-value="selected"
        :options="options"
        :label="ui('Metadata presets')"
        :disabled="blocked"
        @update:model-value="apply"
      />
      <button
        v-if="!loaded && !busy"
        class="secondary-button"
        :disabled="disabled"
        @click="load"
      >
        {{ ui("Retry") }}
      </button>
    </div>
    <details>
      <summary tabindex="0" class="preset-management-toggle">
        <AppIcon name="settings" :size="15" aria-hidden="true" />
        <span>{{ ui("Save and manage presets") }}</span>
        <AppIcon
          name="down"
          :size="15"
          class="preset-management-chevron"
          aria-hidden="true"
        />
      </summary>
      <p class="muted">
        {{
          ui(
            "Presets replace copyrights, artist, characters and tags. Video titles stay unchanged.",
          )
        }}
      </p>
      <div class="preset-actions">
        <label
          >{{ ui("Preset name")
          }}<input v-model="name" :disabled="blocked" maxlength="256"
        /></label>
        <button
          class="secondary-button"
          :disabled="blocked || !name.trim() || !metadata.artist.trim()"
          @click="save(false)"
        >
          {{ ui("Save new preset") }}
        </button>
        <button
          class="secondary-button"
          :disabled="
            blocked || !selected || !name.trim() || !metadata.artist.trim()
          "
          @click="save(true)"
        >
          {{ ui("Update selected preset") }}
        </button>
        <button
          class="secondary-button"
          :disabled="blocked || !selected"
          @click="confirmingDelete = !confirmingDelete"
        >
          {{ ui("Delete preset") }}
        </button>
      </div>
      <div v-if="confirmingDelete" class="preset-actions">
        <span>{{
          ui("Delete the selected preset? Your form and files will be kept.")
        }}</span>
        <button class="secondary-button" :disabled="blocked" @click="remove">
          {{ ui("Confirm deletion") }}
        </button>
        <button
          class="secondary-button"
          :disabled="blocked"
          @click="confirmingDelete = false"
        >
          {{ ui("Cancel") }}
        </button>
      </div>
      <p v-if="!isTauri()" class="muted">
        {{ ui("Demo presets last only while this dialog is open.") }}
      </p>
    </details>
    <p v-if="notice" role="status" class="muted">{{ ui(notice) }}</p>
    <p v-if="error" role="alert" class="error-message">{{ ui(error) }}</p>
  </section>
</template>

<style scoped>
.stage-presets {
  flex-shrink: 0;
  margin: 12px 22px 0;
  padding: 14px;
  border: 1px solid var(--line);
  border-radius: 10px;
}

.preset-picker,
.preset-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 8px;
}

.preset-picker > :first-child {
  flex: 1 1 220px;
  min-width: 0;
}

.preset-picker :deep(.sort-select-options) {
  max-height: min(240px, 35vh);
  overflow-y: auto;
}

.stage-presets details,
.stage-presets p,
.preset-actions {
  margin-top: 10px;
}

.preset-management-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 35px;
  padding: 8px 10px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface-input);
  color: var(--muted);
  font-size: 12px;
  list-style: none;
  cursor: pointer;
  transition:
    background 0.14s,
    color 0.14s,
    border-color 0.14s;
}

.preset-management-toggle::-webkit-details-marker {
  display: none;
}

.preset-management-toggle span {
  flex: 1;
}

.preset-management-toggle svg {
  flex-shrink: 0;
}

.preset-management-toggle:hover {
  border-color: var(--accent-line);
  background: var(--surface-hover);
  color: var(--text);
}

.preset-management-toggle:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.stage-presets details[open] > .preset-management-toggle {
  border-color: var(--accent-line);
  color: var(--text-accent);
}

.preset-management-chevron {
  transition: transform 0.14s;
}

.stage-presets details[open] .preset-management-chevron {
  transform: rotate(180deg);
}

.stage-presets details {
  max-height: 140px;
  overflow-y: auto;
}

.preset-actions label {
  display: grid;
  flex: 1 1 180px;
  gap: 6px;
  color: var(--muted);
  font-size: 12px;
}

.preset-actions input {
  width: 100%;
  min-width: 0;
  height: 35px;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface-input);
  color: var(--text);
}

.preset-actions input:hover:not(:disabled) {
  border-color: var(--accent-line);
}

.preset-actions input:focus-visible {
  border-color: var(--accent);
}

.preset-actions input:disabled {
  opacity: 0.5;
}
</style>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
} from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../runtime";
import { searchByName } from "../utils/metadataSearch";
import { translateUi as ui } from "../i18n/presentation";
import AppIcon from "./AppIcon.vue";
import MetadataPresetManager from "./MetadataPresetManager.vue";

import type { MetadataPreset, PresetMetadata } from "../types/metadataPresets";

const props = defineProps<{
  metadata: PresetMetadata;
  disabled: boolean;
  catalog?: {
    artists: string[];
    copyrights: string[];
    tags: string[];
    characters: { name: string; copyright: string }[];
  };
}>();

const emit = defineEmits<{
  apply: [metadata: PresetMetadata];
  managing: [open: boolean];
}>();

const id = useId();

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const search = ref<HTMLInputElement | null>(null);
const open = ref(false);
const managing = ref(false);
const creating = ref(false);
const query = ref("");

let returnFocus: HTMLElement | null = null;

const presets = ref<MetadataPreset[]>([]);
const selected = ref("");
const name = ref("");
const busy = ref(false);
const loaded = ref(false);
const error = ref("");
const notice = ref("");

const blocked = computed(() => props.disabled || busy.value || !loaded.value);

const available = computed(() => !blocked.value);

const visiblePresets = computed(() => searchByName(presets.value, query.value));

async function toggle() {
  if (props.disabled || managing.value) return;

  open.value = !open.value;

  if (open.value) {
    query.value = "";

    await nextTick();

    search.value?.focus();
  }
}

function dismiss(event: Event) {
  if (event.target instanceof Node && !root.value?.contains(event.target))
    open.value = false;
}

function closePicker() {
  open.value = false;
  trigger.value?.focus();
}

function pickerKeyboard(event: KeyboardEvent) {
  if (event.key !== "Escape" || !open.value) return;

  event.preventDefault();
  event.stopPropagation();
  closePicker();
}

function dismissFocus(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    !root.value?.contains(event.relatedTarget)
  )
    open.value = false;
}

function openManager(create = false) {
  if (blocked.value) return;

  returnFocus =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  open.value = false;
  creating.value = create;
  error.value = "";
  notice.value = "";
  managing.value = true;
  emit("managing", true);
}

async function closeManager() {
  if (busy.value) return;

  managing.value = false;
  emit("managing", false);

  await nextTick();

  if (returnFocus?.isConnected && !returnFocus.closest("[inert]"))
    returnFocus.focus();
  else trigger.value?.focus();
}

async function saveManaged(
  id: string | null,
  presetName: string,
  metadata: PresetMetadata,
) {
  if (blocked.value) return;

  selected.value = id ?? "";
  name.value = presetName;

  await save(id !== null, metadata);

  if (!error.value) await closeManager();
}

async function deleteManaged(id: string) {
  if (blocked.value) return;

  selected.value = id;

  await remove();
}

defineExpose({ openManager, available });

async function load() {
  busy.value = true;
  error.value = "";

  try {
    presets.value = isNativeData()
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

  const preset = presets.value.find((preset) => preset.id === id);

  if (!preset) return;

  name.value = preset.name;
  error.value = "";
  notice.value = "Preset applied to the form. Review before distributing.";
  emit("apply", JSON.parse(JSON.stringify(preset.metadata)) as PresetMetadata);
  closePicker();
}

async function save(update: boolean, source = props.metadata) {
  if (blocked.value || (update && !selected.value)) return;

  busy.value = true;
  error.value = "";
  notice.value = "";

  try {
    const presetName = name.value.trim();

    if (!presetName || !source.artist.trim())
      throw new Error(ui("Enter a preset name and an artist."));

    const id = update ? selected.value : null;
    const metadata = JSON.parse(JSON.stringify(source)) as PresetMetadata;

    if (
      !isNativeData() &&
      presets.value.some(
        (preset) =>
          preset.id !== id &&
          preset.name.toLocaleLowerCase() === presetName.toLocaleLowerCase(),
      )
    )
      throw new Error(ui("A preset with this name already exists."));

    const preset = isNativeData()
      ? await invoke<MetadataPreset>("save_metadata_preset", {
          id,
          name: presetName,
          metadata,
        })
      : {
          id: id ?? crypto.randomUUID(),
          name: presetName,
          metadata,
        };

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
    if (isNativeData())
      await invoke("delete_metadata_preset", { id: selected.value });

    presets.value = presets.value.filter(
      (preset) => preset.id !== selected.value,
    );
    selected.value = "";
    name.value = "";
    notice.value = "Metadata preset deleted. The form is unchanged.";
  } catch (cause) {
    error.value = `Could not delete preset: ${String(cause)}`;
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  void load();
  document.addEventListener("pointerdown", dismiss);
});

onBeforeUnmount(() => document.removeEventListener("pointerdown", dismiss));
</script>

<template>
  <div
    ref="root"
    class="stage-presets"
    @keydown="pickerKeyboard"
    @focusout="dismissFocus"
  >
    <button
      ref="trigger"
      type="button"
      class="secondary-button preset-trigger"
      :disabled="disabled"
      :aria-expanded="open"
      :aria-controls="`${id}-picker`"
      @click="toggle"
    >
      <AppIcon name="copy" :size="15" />{{ ui("Presets")
      }}<AppIcon name="down" :size="14" />
    </button>
    <section
      v-if="open"
      :id="`${id}-picker`"
      class="preset-picker"
      :aria-label="ui('Metadata presets')"
      :aria-busy="busy"
    >
      <h3>{{ ui("Choose a metadata preset") }}</h3>
      <input
        ref="search"
        v-model="query"
        type="search"
        :aria-label="ui('Search presets')"
        :placeholder="ui('Search presets')"
      />
      <div class="preset-picker-list">
        <button
          v-for="preset in visiblePresets"
          :key="preset.id"
          class="preset-choice"
          :disabled="blocked"
          @click="apply(preset.id)"
        >
          <strong>{{ preset.name }}</strong>
          <span>{{ preset.metadata.artist }}</span>
          <small>{{
            [
              ...preset.metadata.copyrights,
              ...preset.metadata.characters.map((entry) => entry.name),
              ...preset.metadata.tags,
            ].join(" · ") || "—"
          }}</small>
        </button>
        <p v-if="loaded && !visiblePresets.length" class="muted">
          {{ ui(presets.length ? "No matching presets." : "No presets yet.") }}
        </p>
        <p v-if="busy" class="muted" role="status">
          {{ ui("Loading presets…") }}
        </p>
      </div>
      <p class="muted">
        {{
          ui(
            "Presets replace copyrights, artist, characters and tags. Video titles stay unchanged.",
          )
        }}
      </p>
      <p v-if="error" role="alert" class="error-message">{{ ui(error) }}</p>
      <button
        v-if="!loaded && !busy"
        class="secondary-button"
        :disabled="disabled"
        @click="load"
      >
        {{ ui("Retry") }}
      </button>
      <div class="preset-picker-actions">
        <button
          class="secondary-button"
          :disabled="blocked || !metadata.artist.trim()"
          @click="openManager(true)"
        >
          <AppIcon name="plus" :size="14" />{{ ui("Save as preset…") }}
        </button>
        <button
          class="secondary-button"
          :disabled="blocked"
          @click="openManager()"
        >
          <AppIcon name="settings" :size="14" />{{ ui("Manage presets…") }}
        </button>
      </div>
    </section>
    <span class="preset-status" role="status">{{ ui(notice) }}</span>
    <MetadataPresetManager
      v-if="managing"
      :presets="presets"
      :metadata="metadata"
      :create="creating"
      :busy="busy"
      :error="error"
      :notice="notice"
      :catalog="catalog"
      :demo="!isNativeData()"
      @close="closeManager"
      @save="saveManaged"
      @delete="deleteManaged"
    />
  </div>
</template>

<style scoped>
.stage-presets {
  position: relative;
  flex-shrink: 0;
}
.preset-trigger {
  white-space: nowrap;
}
.preset-picker {
  position: absolute;
  z-index: 30;
  top: calc(100% + 8px);
  right: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: min(360px, calc(100vw - 64px));
  max-height: min(560px, calc(100dvh - 150px));
  overflow-y: auto;
  padding: 16px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--surface-raised);
  box-shadow: 0 20px 70px var(--shadow);
}
.preset-picker h3 {
  margin: 0;
  font-size: 13px;
}
.preset-picker p {
  margin: 0;
  max-width: none;
  font-size: 12px;
  line-height: 1.5;
  white-space: normal;
}
.preset-picker input {
  width: 100%;
  min-height: 35px;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface-input);
  color: var(--text);
}
.preset-picker-list {
  display: grid;
  gap: 4px;
  min-height: 0;
  max-height: 240px;
  overflow-y: auto;
  flex-shrink: 0;
}
.preset-choice {
  display: grid;
  gap: 4px;
  min-width: 0;
  padding: 10px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text);
  text-align: left;
  overflow-wrap: anywhere;
}
.preset-choice:hover,
.preset-choice:focus-visible {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.preset-choice span,
.preset-choice small {
  color: var(--muted);
  font-size: 12px;
}
.preset-choice small {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.preset-picker-actions {
  display: grid;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--line);
}
.preset-status {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
}
</style>

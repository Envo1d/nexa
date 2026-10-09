<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import AppIcon from "./AppIcon.vue";
import {
  categoryKey,
  type TaxonomyCatalog,
  type TaxonomyCategory,
  type TaxonomyEntry,
} from "../types/taxonomy";
const props = defineProps<{
  demoCatalog: TaxonomyCatalog;
  changeDemo: (
    category: TaxonomyCategory,
    old: string,
    replacement?: string,
  ) => number;
}>();
const emit = defineEmits<{ changed: []; notice: [message: string] }>();
const native = isTauri();
const catalog = ref<TaxonomyCatalog>({ copyrights: [], artists: [], tags: [] });
const category = ref<TaxonomyCategory>("copyright");
const categories = [
  { id: "copyright", label: "Copyrights", key: "copyrights" },
  { id: "artist", label: "Artists", key: "artists" },
  { id: "tag", label: "Tags", key: "tags" },
] as const;
const search = ref("");
const loading = ref(false);
const busy = ref(false);
const error = ref("");
const pending = ref<{ entry: TaxonomyEntry; action: "edit" | "delete" } | null>(
  null,
);
const name = ref("");
const data = computed(() => (native ? catalog.value : props.demoCatalog));
const entries = computed(() =>
  data.value[categoryKey[category.value]].filter((entry) =>
    entry.name
      .toLocaleLowerCase()
      .includes(search.value.trim().toLocaleLowerCase()),
  ),
);
async function load() {
  if (!native) return;
  loading.value = true;
  error.value = "";
  try {
    catalog.value = await invoke<TaxonomyCatalog>("get_taxonomy");
  } catch (cause) {
    error.value = String(cause);
  } finally {
    loading.value = false;
  }
}
function begin(entry: TaxonomyEntry, action: "edit" | "delete") {
  pending.value = { entry, action };
  name.value = entry.name;
  error.value = "";
}
watch([category, search], () => {
  pending.value = null;
  error.value = "";
});
async function apply() {
  const operation = pending.value;
  if (!operation || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    let count = 0;
    if (native) {
      if (operation.action === "edit")
        await invoke("rename_taxonomy", {
          category: category.value,
          id: Number(operation.entry.id),
          name: name.value,
        });
      else
        count = await invoke<number>("delete_taxonomy", {
          category: category.value,
          id: Number(operation.entry.id),
        });
    } else
      count = props.changeDemo(
        category.value,
        operation.entry.name,
        operation.action === "edit" ? name.value : undefined,
      );
    pending.value = null;
    emit("changed");
    emit(
      "notice",
      operation.action === "edit"
        ? "Name updated."
        : `Entry deleted.${count ? ` ${count} file(s) queued in Stage for redistribution.` : ""}`,
    );
    if (native) await load();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}
onMounted(load);
</script>

<template>
  <section
    class="taxonomy-workspace"
    aria-label="Tags management"
    :aria-busy="loading || busy"
  >
    <header class="taxonomy-header">
      <div>
        <h1>Tags</h1>
        <p class="muted">
          Manage copyrights, artists and tags across your library.
        </p>
      </div>
      <button
        v-if="native"
        class="secondary-button"
        :disabled="loading || busy"
        @click="load"
      >
        Refresh
      </button>
    </header>
    <div class="taxonomy-tools">
      <nav class="taxonomy-categories" aria-label="Metadata categories">
        <button
          v-for="tab in categories"
          :key="tab.id"
          :class="{ active: category === tab.id }"
          :aria-pressed="category === tab.id"
          :disabled="busy"
          @click="category = tab.id"
        >
          {{ tab.label }} <span>{{ data[tab.key].length }}</span>
        </button>
      </nav>
      <div class="search-box">
        <AppIcon name="search" :size="17" /><input
          v-model="search"
          :disabled="busy"
          aria-label="Search dictionary"
          placeholder="Search names…"
        />
      </div>
    </div>
    <p class="taxonomy-note muted">
      Deleting a copyright or artist sends linked files to Stage for
      redistribution. Files stay at their current paths, including when disk
      synchronization is enabled. Deleting a tag only removes its links.
    </p>
    <p v-if="!native" class="muted">
      Demo library · changes last for this session.
    </p>
    <p v-else class="muted">
      Saved database dictionaries · Main still uses demo records. Queued
      database files appear in Stage; distribution currently remains a preview.
    </p>
    <p v-if="error" class="error-message" role="alert">{{ error }}</p>
    <div class="taxonomy-list">
      <p v-if="loading" class="taxonomy-empty" role="status">
        Loading dictionaries…
      </p>
      <p v-else-if="!entries.length" class="taxonomy-empty muted">
        {{ search ? "No matching names." : "No entries in this category yet." }}
      </p>
      <template v-else>
        <article
          v-for="entry in entries"
          :key="entry.id"
          class="taxonomy-entry"
        >
          <div class="taxonomy-entry-main">
            <AppIcon name="tag" :size="18" />
            <div class="taxonomy-entry-label">
              <strong>{{ entry.name }}</strong
              ><span class="muted"
                >{{ entry.count }} linked file{{
                  entry.count === 1 ? "" : "s"
                }}</span
              >
            </div>
            <button
              class="secondary-button"
              :disabled="busy || loading"
              :aria-label="`Edit ${entry.name}`"
              @click="begin(entry, 'edit')"
            >
              Edit
            </button>
            <button
              class="icon-button taxonomy-delete"
              :disabled="busy || loading"
              :aria-label="`Delete ${entry.name}`"
              @click="begin(entry, 'delete')"
            >
              <AppIcon name="trash" :size="17" />
            </button>
          </div>
          <form
            v-if="pending?.entry.id === entry.id"
            class="taxonomy-editor"
            @submit.prevent="apply"
            @keydown.esc.stop="!busy && (pending = null)"
          >
            <template v-if="pending.action === 'edit'"
              ><label :for="`name-${entry.id}`">Name</label
              ><input
                :id="`name-${entry.id}`"
                v-model="name"
                :disabled="busy"
                required
            /></template>
            <p v-else>
              Delete “{{ entry.name }}”?
              {{
                category === "tag"
                  ? "This tag will be removed from all linked files."
                  : `All ${entry.count} linked file(s) will need redistribution. ${category === "copyright" ? "Characters belonging to this copyright will also be removed." : ""}`
              }}
              Files on disk stay unchanged.
            </p>
            <div class="taxonomy-editor-actions">
              <button
                type="button"
                class="secondary-button"
                :disabled="busy"
                @click="pending = null"
              >
                Cancel</button
              ><button
                type="submit"
                class="primary-button"
                :disabled="busy || (pending.action === 'edit' && !name.trim())"
              >
                {{
                  busy
                    ? "Saving…"
                    : pending.action === "edit"
                      ? "Save name"
                      : "Delete entry"
                }}
              </button>
            </div>
          </form>
        </article>
      </template>
    </div>
  </section>
</template>

<style scoped>
.taxonomy-workspace {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 26px 32px;
}
.taxonomy-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.taxonomy-header h1 {
  font-size: 22px;
  margin-bottom: 7px;
}
.taxonomy-tools {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
}
.taxonomy-tools .search-box {
  width: 270px;
}
.taxonomy-categories {
  display: flex;
  gap: 8px;
}
.taxonomy-categories button {
  display: flex;
  gap: 10px;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: 9px;
  padding: 10px 14px;
  color: var(--muted);
}
.taxonomy-categories button.active {
  background: #272039;
  border-color: #69518e;
  color: #d3bfff;
}
.taxonomy-categories span {
  font-size: 11px;
  background: #ffffff0b;
  border-radius: 5px;
  padding: 2px 6px;
}
.taxonomy-note {
  line-height: 1.6;
  max-width: 850px;
}
.taxonomy-list {
  flex: 1;
  overflow: auto;
  min-height: 0;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface);
}
.taxonomy-entry + .taxonomy-entry {
  border-top: 1px solid var(--line);
}
.taxonomy-entry-main {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 13px 18px;
}
.taxonomy-entry-main > svg {
  color: var(--accent);
}
.taxonomy-entry-label {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.taxonomy-entry-label strong {
  overflow-wrap: anywhere;
}
.taxonomy-delete {
  color: #dc929d;
}
.taxonomy-editor {
  margin: 0 18px 16px 50px;
  padding: 16px;
  background: #1c1a2a;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.taxonomy-editor input {
  border: 1px solid var(--line);
  background: #11111b;
  color: inherit;
  padding: 9px 12px;
  border-radius: 7px;
}
.taxonomy-editor p {
  line-height: 1.6;
  overflow-wrap: anywhere;
}
.taxonomy-editor-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.taxonomy-empty {
  padding: 32px;
  text-align: center;
}
@media (max-width: 1100px) {
  .taxonomy-workspace {
    padding: 22px;
  }
  .taxonomy-tools {
    flex-wrap: wrap;
    gap: 14px;
  }
}
</style>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";

import { isNativeData } from "../runtime";
import { searchByName } from "../utils/metadataSearch";
import { translateUi as ui } from "../i18n/presentation";
import AppIcon from "./AppIcon.vue";
import AppSelect from "./AppSelect.vue";
import MetadataInput from "./MetadataInput.vue";
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
    copyright?: string,
    replacementCopyright?: string,
  ) => number;
}>();

const emit = defineEmits<{ changed: []; notice: [message: string] }>();

const native = isNativeData();

const catalog = ref<TaxonomyCatalog>({
  copyrights: [],
  artists: [],
  tags: [],
  characters: [],
});
const category = ref<TaxonomyCategory>("copyright");

const categories = [
  {
    id: "copyright",
    label: "Copyrights",
    key: "copyrights",
    icon: "copyright",
    singular: "Copyright",
  },
  {
    id: "artist",
    label: "Artists",
    key: "artists",
    icon: "artist",
    singular: "Artist",
  },
  {
    id: "character",
    label: "Characters",
    key: "characters",
    icon: "person",
    singular: "Character",
  },
  {
    id: "tag",
    label: "Tags",
    key: "tags",
    icon: "tag",
    singular: "Tag",
  },
] as const;

const repeatedOnly = ref(false);
const linkedCount = ref<"all" | 0 | 1>("all");

const linkedCountOptions = [
  { value: "all", label: "All links" },
  { value: 0, label: "0 links" },
  { value: 1, label: "1 link" },
] as const;

const search = ref("");
const searchInput = ref<HTMLInputElement | null>(null);

function clearSearch() {
  search.value = "";
  searchInput.value?.focus();
}

const loading = ref(false);
const busy = ref(false);
const error = ref("");
const selectedId = ref<string | null>(null);
const deleting = ref(false);
const name = ref("");
const copyright = ref("");

const data = computed(() => (native ? catalog.value : props.demoCatalog));

const selectedCategory = computed(() =>
  categories.find((tab) => tab.id === category.value)!,
);

const copyrightOptions = computed(() =>
  data.value.copyrights.map((entry) => ({
    value: entry.name,
    label: entry.name,
  })),
);

const characterNameKey = (name: string) =>
  name.normalize("NFC").toLocaleLowerCase();

const repeatedNames = computed(() => {
  const copyrightsByName = new Map<string, Set<string>>();

  for (const entry of data.value.characters) {
    const key = characterNameKey(entry.name);

    const copyrights = copyrightsByName.get(key) ?? new Set<string>();

    copyrights.add(entry.copyright ?? "");
    copyrightsByName.set(key, copyrights);
  }

  return new Set(
    [...copyrightsByName]
      .filter(([, copyrights]) => copyrights.size > 1)
      .map(([name]) => name),
  );
});

const filtersActive = computed(
  () =>
    linkedCount.value !== "all" ||
    (category.value === "character" && repeatedOnly.value),
);

const entries = computed(() => {
  const query = search.value.trim().toLocaleLowerCase();

  const values = data.value[categoryKey[category.value]].filter((entry) => {
    const matchesLinkedCount =
      linkedCount.value === "all" || entry.count === linkedCount.value;

    if (!matchesLinkedCount) return false;

    if (category.value === "character") {
      const matchesRepeatedName =
        !repeatedOnly.value ||
        repeatedNames.value.has(characterNameKey(entry.name));

      if (!matchesRepeatedName) return false;
    }

    return true;
  });

  return searchByName(values, query);
});

const selectedEntry = computed(
  () =>
    entries.value.find((entry) => entry.id === selectedId.value) ??
    entries.value[0] ??
    null,
);

const dirty = computed(() => {
  const entry = selectedEntry.value;

  return Boolean(
    entry &&
    (name.value !== entry.name ||
      (category.value === "character" &&
        copyright.value !== (entry.copyright ?? ""))),
  );
});

const relatedCharacters = computed(() =>
  category.value === "copyright" && selectedEntry.value
    ? data.value.characters.filter(
        (entry) => entry.copyright === selectedEntry.value?.name,
      )
    : [],
);

function resetDraft() {
  name.value = selectedEntry.value?.name ?? "";
  copyright.value = selectedEntry.value?.copyright ?? "";
  deleting.value = false;
}

function cancelEdit() {
  if (busy.value) return;

  resetDraft();
  error.value = "";
}

function selectEntry(entry: TaxonomyEntry) {
  if (busy.value || loading.value || selectedEntry.value?.id === entry.id)
    return;

  selectedId.value = entry.id;
  cancelEdit();
}

function showCharacter(entry: TaxonomyEntry) {
  if (busy.value || loading.value) return;

  category.value = "character";
  search.value = "";
  linkedCount.value = "all";
  repeatedOnly.value = false;
  selectedId.value = entry.id;
  cancelEdit();
}

function confirmDeletion() {
  if (busy.value || loading.value || !selectedEntry.value) return;

  resetDraft();
  deleting.value = true;
  error.value = "";
}

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

watch([category, search, repeatedOnly, linkedCount], () => {
  resetDraft();
  error.value = "";
});

watch(selectedEntry, resetDraft, { immediate: true });

async function apply() {
  const entry = selectedEntry.value;

  if (
    !entry ||
    busy.value ||
    loading.value ||
    (!deleting.value && !dirty.value)
  )
    return;

  const operation = {
    entry,
    category: category.value,
    action: deleting.value ? "delete" : "edit",
    name: name.value,
    copyright: copyright.value,
  };

  if (operation.action === "edit" && !operation.name.trim()) return;

  busy.value = true;
  error.value = "";

  try {
    let count = 0;

    if (native) {
      if (operation.action === "edit" && operation.category === "character") {
        const target = data.value.copyrights.find(
          (entry) => entry.name === operation.copyright,
        );

        if (!target) throw new Error("Select the character’s copyright");

        await invoke("update_taxonomy_character", {
          id: Number(operation.entry.id),
          name: operation.name,
          copyrightId: Number(target.id),
        });
      } else if (operation.action === "edit")
        await invoke("rename_taxonomy", {
          category: operation.category,
          id: Number(operation.entry.id),
          name: operation.name,
        });
      else
        count = await invoke<number>("delete_taxonomy", {
          category: operation.category,
          id: Number(operation.entry.id),
        });
    } else
      count = props.changeDemo(
        operation.category,
        operation.entry.name,
        operation.action === "edit" ? operation.name : undefined,
        operation.entry.copyright,
        operation.action === "edit" ? operation.copyright : undefined,
      );

    if (!native && operation.action === "edit") {
      const renamed = data.value[categoryKey[operation.category]].find(
        (entry) =>
          entry.name === operation.name.trim() &&
          (operation.category !== "character" ||
            entry.copyright === operation.copyright),
      );

      selectedId.value = renamed?.id ?? null;
    }

    emit("changed");
    emit(
      "notice",
      operation.action === "edit"
        ? "Entry updated."
        : `Entry deleted.${count ? ` ${count} file(s) queued in Stage for redistribution.` : ""}`,
    );

    if (native) await load();

    resetDraft();
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
    :aria-label="ui('Tags management')"
    :aria-busy="loading || busy"
  >
    <header class="taxonomy-header">
      <div>
        <p class="taxonomy-eyebrow">{{ ui("Collection metadata") }}</p>
        <h1>{{ ui("Dictionaries") }}</h1>
        <p class="muted">
          {{ ui("Select an entry on the left to edit its details.") }}
        </p>
      </div>
      <button
        v-if="native"
        class="secondary-button"
        :disabled="loading || busy"
        @click="load"
      >
        <AppIcon name="reset" :size="15" />{{ ui("Refresh") }}
      </button>
    </header>
    <nav class="taxonomy-categories" :aria-label="ui('Metadata categories')">
      <button
        v-for="tab in categories"
        :key="tab.id"
        :class="[
          'taxonomy-category',
          `category-${tab.id}`,
          { active: category === tab.id },
        ]"
        :aria-pressed="category === tab.id"
        :disabled="busy || loading"
        @click="category = tab.id"
      >
        <AppIcon :name="tab.icon" :size="17" />
        {{ ui(tab.label) }}<span>{{ data[tab.key].length }}</span>
      </button>
    </nav>
    <div class="taxonomy-tools">
      <div class="search-box">
        <AppIcon name="search" :size="17" />
        <input
          ref="searchInput"
          v-model="search"
          :disabled="busy || loading"
          :aria-label="ui('Search dictionary')"
          :placeholder="ui('Search names…')"
        />
        <button
          v-if="search"
          type="button"
          class="icon-button"
          :disabled="busy || loading"
          :aria-label="ui('Clear search')"
          @click="clearSearch"
        >
          <AppIcon name="close" :size="16" />
        </button>
      </div>
      <div class="taxonomy-filters">
        <div
          class="taxonomy-link-filters"
          role="group"
          :aria-label="ui('Filter entries by linked-file count')"
        >
          <button
            v-for="option in linkedCountOptions"
            :key="option.value"
            type="button"
            :class="{ active: linkedCount === option.value }"
            :aria-pressed="linkedCount === option.value"
            :disabled="busy || loading"
            @click="linkedCount = option.value"
          >
            {{ ui(option.label) }}
          </button>
        </div>
        <button
          v-if="category === 'character'"
          type="button"
          class="taxonomy-repeat-filter"
          :class="{ active: repeatedOnly }"
          :aria-pressed="repeatedOnly"
          :disabled="busy || loading"
          :title="
            ui('Show only the same character name in different copyrights')
          "
          @click="repeatedOnly = !repeatedOnly"
        >
          <span class="taxonomy-filter-check"
            ><AppIcon v-if="repeatedOnly" name="check" :size="12"
          /></span>
          {{ ui("Same name in different copyrights")
          }}<span class="taxonomy-filter-count">{{ repeatedNames.size }}</span>
        </button>
      </div>
    </div>
    <div class="taxonomy-results">
      <span
        >{{ ui(selectedCategory.label) }} · {{ entries.length }} /
        {{ data[categoryKey[category]].length }}</span
      >
      <span v-if="category === 'character'">{{
        ui("Names are qualified by copyright.")
      }}</span>
    </div>
    <p v-if="error" class="error-message" role="alert">{{ ui(error) }}</p>
    <div class="taxonomy-split">
      <div class="taxonomy-list-pane">
        <div class="taxonomy-list-heading">
          <span>{{ ui("Name") }}</span
          ><span>{{ ui("Linked files") }}</span>
        </div>
        <div class="taxonomy-list" :aria-label="ui(selectedCategory.label)">
          <p v-if="loading" class="taxonomy-empty" role="status">
            {{ ui("Loading dictionaries…") }}
          </p>
          <p
            v-else-if="!entries.length"
            class="taxonomy-empty muted"
            role="status"
          >
            {{
              ui(
                search || filtersActive
                  ? "No matching names."
                  : "No entries in this category yet.",
              )
            }}
          </p>
          <template v-else>
            <button
              v-for="entry in entries"
              :key="entry.id"
              type="button"
              class="taxonomy-entry"
              :class="{ selected: selectedEntry?.id === entry.id }"
              :aria-pressed="selectedEntry?.id === entry.id"
              aria-controls="taxonomy-detail"
              :disabled="busy || loading"
              @click="selectEntry(entry)"
            >
              <span class="taxonomy-initial">{{
                entry.name.slice(0, 1).toLocaleUpperCase()
              }}</span>
              <span class="taxonomy-entry-label">
                <strong>{{ entry.name }}</strong
                ><small class="muted">{{
                  entry.copyright || ui(selectedCategory.singular)
                }}</small>
              </span>
              <span
                class="taxonomy-entry-count"
                :class="{ unused: entry.count === 0 }"
                >{{ entry.count }}</span
              >
            </button>
          </template>
        </div>
      </div>
      <aside
        id="taxonomy-detail"
        class="taxonomy-detail"
        :aria-label="ui('Entry details')"
      >
        <template v-if="selectedEntry && !loading">
          <header class="taxonomy-detail-heading">
            <p :class="['taxonomy-detail-category', `category-${category}`]">
              <AppIcon :name="selectedCategory.icon" :size="16" />{{
                ui(selectedCategory.singular)
              }}
            </p>
            <h2>{{ selectedEntry.name }}</h2>
            <p v-if="selectedEntry.copyright" class="muted">
              {{ selectedEntry.copyright }}
            </p>
          </header>
          <form
            class="taxonomy-editor"
            @submit.prevent="apply"
            @keydown.esc.stop="cancelEdit"
          >
            <template v-if="!deleting">
              <MetadataInput
                v-model:text-value="name"
                text-input
                label="Name"
                :options="[]"
                :disabled="busy"
                required
              />
              <div v-if="category === 'character'">
                <p class="metadata-label">{{ ui("Copyright") }}</p>
                <AppSelect
                  v-model="copyright"
                  :label="ui('Copyright')"
                  :options="copyrightOptions"
                  :disabled="busy"
                />
              </div>
              <p class="muted">
                {{
                  ui(
                    category === "character"
                      ? "Changing copyright updates all linked files. The previous copyright is removed if no characters on the file still use it."
                      : "Changes apply to all linked files.",
                  )
                }}
              </p>
              <span v-if="dirty" class="taxonomy-draft-status" role="status">{{
                ui("Unsaved changes")
              }}</span>
            </template>
            <template v-else>
              <h3>{{ ui(`Delete “${selectedEntry.name}”?`) }}</h3>
              <p>
                {{
                  ui(
                    category === "tag" || category === "character"
                      ? `This ${category} will be removed from all linked files.`
                      : `All ${selectedEntry.count} linked file(s) will need redistribution. ${category === "copyright" ? "Characters belonging to this copyright will also be removed." : ""}`,
                  )
                }}
                {{ ui("Files on disk stay unchanged.") }}
              </p>
            </template>
            <div class="taxonomy-editor-actions">
              <button
                type="button"
                class="secondary-button"
                :disabled="busy || (!dirty && !deleting)"
                @click="cancelEdit"
              >
                {{ ui("Cancel") }}
              </button>
              <button
                type="submit"
                :class="[
                  'primary-button',
                  { 'taxonomy-confirm-delete': deleting },
                ]"
                :disabled="
                  busy ||
                  (!deleting &&
                    (!dirty ||
                      !name.trim() ||
                      (category === 'character' && !copyright)))
                "
              >
                {{
                  ui(
                    busy
                      ? "Saving…"
                      : deleting
                        ? "Delete entry"
                        : "Save changes",
                  )
                }}
              </button>
            </div>
          </form>
          <div class="taxonomy-detail-links">
            <AppIcon name="files" :size="16" />{{
              ui(
                `${selectedEntry.count} linked file${selectedEntry.count === 1 ? "" : "s"}`,
              )
            }}
          </div>
          <button
            v-if="!deleting"
            type="button"
            class="taxonomy-delete secondary-button"
            :disabled="busy"
            @click="confirmDeletion"
          >
            <AppIcon name="trash" :size="16" />{{ ui("Delete entry") }}
          </button>
          <section v-if="category === 'copyright'" class="taxonomy-related">
            <h3>
              {{ ui("Characters") }}<span>{{ relatedCharacters.length }}</span>
            </h3>
            <div v-if="relatedCharacters.length" class="taxonomy-related-list">
              <button
                v-for="character in relatedCharacters"
                :key="character.id"
                type="button"
                :disabled="busy"
                @click="showCharacter(character)"
              >
                <AppIcon name="person" :size="14" /><span>{{
                  character.name
                }}</span
                ><small>{{ character.count }}</small>
              </button>
            </div>
            <p v-else class="muted">
              {{ ui("No characters in this copyright yet.") }}
            </p>
          </section>
        </template>
        <div v-else class="taxonomy-detail-empty muted">
          <AppIcon name="list" :size="26" />
          <p>
            {{
              ui(
                loading
                  ? "Loading dictionaries…"
                  : "Select an entry to edit its details.",
              )
            }}
          </p>
        </div>
      </aside>
    </div>
    <p v-if="!native" class="taxonomy-demo muted">
      {{ ui("Demo library · changes last for this session.") }}
    </p>
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
  font-size: 24px;
  margin-bottom: 6px;
}
.taxonomy-eyebrow {
  font-size: 10px;
  letter-spacing: 1.4px;
  text-transform: uppercase;
  color: var(--muted);
  margin-bottom: 6px;
}
.taxonomy-categories {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.taxonomy-category {
  display: flex;
  align-items: center;
  gap: 9px;
  border: 1px solid transparent;
  border-radius: 8px;
  padding: 10px 13px;
  color: var(--muted);
}
.taxonomy-category.active {
  background: var(--surface-hover);
  border-color: var(--line);
  color: var(--text);
}
.taxonomy-category:hover:not(:disabled):not(.active) {
  background: var(--hover-overlay);
}
.taxonomy-category span {
  font-size: 11px;
  color: var(--muted);
}
.category-copyright > svg {
  color: var(--copyright-text);
}
.category-artist > svg {
  color: var(--info);
}
.category-character > svg {
  color: var(--character-text);
}
.category-tag > svg {
  color: var(--accent);
}
.taxonomy-tools {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 14px;
}
.taxonomy-tools .search-box {
  width: 280px;
  max-width: 100%;
}
.taxonomy-filters {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
}
.taxonomy-repeat-filter,
.taxonomy-link-filters button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--muted);
  font-size: 12px;
}
.taxonomy-repeat-filter:hover:not(:disabled):not(.active),
.taxonomy-link-filters button:hover:not(:disabled):not(.active) {
  background: var(--hover-overlay);
  color: var(--text);
}
.taxonomy-repeat-filter.active,
.taxonomy-link-filters button.active {
  background: var(--surface-hover);
  color: var(--text-accent);
}
.taxonomy-filter-check {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  border: 1px solid currentColor;
  border-radius: 4px;
}
.taxonomy-repeat-filter.active .taxonomy-filter-check {
  background: var(--accent-strong);
  border-color: var(--accent);
  color: var(--on-accent);
}
.taxonomy-filter-count {
  color: var(--muted);
  font-size: 11px;
}
.taxonomy-link-filters {
  display: flex;
  gap: 3px;
  padding: 3px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
}
.taxonomy-results {
  display: flex;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
  color: var(--muted);
  font-size: 11px;
}
.taxonomy-split {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 330px;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface);
  overflow: hidden;
}
.taxonomy-list-pane {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.taxonomy-list-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 13px 20px;
  background: var(--surface-alt);
  color: var(--muted);
  font-size: 11px;
  border-bottom: 1px solid var(--line);
}
.taxonomy-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 2px;
}
.taxonomy-entry {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 13px;
  text-align: left;
  padding: 13px 18px;
  min-height: 66px;
  border-radius: 5px;
}
.taxonomy-entry + .taxonomy-entry {
  border-top: 1px solid var(--line);
}
.taxonomy-entry:hover:not(:disabled):not(.selected) {
  background: var(--hover-overlay);
}
.taxonomy-entry.selected {
  background: var(--accent-soft);
  box-shadow: inset 3px 0 var(--accent);
}
.taxonomy-initial {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  flex-shrink: 0;
  background: var(--surface-hover);
  border-radius: 8px;
  color: var(--muted);
  font-size: 13px;
}
.taxonomy-entry-label {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-wrap: anywhere;
}
.taxonomy-entry-label strong {
  font-weight: 500;
}
.taxonomy-entry-label small {
  font-size: 11px;
}
.taxonomy-entry-count {
  color: var(--muted);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
.taxonomy-entry-count.unused {
  background: var(--surface-hover);
  padding: 2px 7px;
  border-radius: 5px;
  font-size: 11px;
}
.taxonomy-detail {
  min-width: 0;
  min-height: 0;
  overflow: auto;
  padding: 24px;
  background: var(--surface-alt);
  border-left: 1px solid var(--line);
}
.taxonomy-detail-heading {
  display: grid;
  gap: 8px;
  margin-bottom: 24px;
  overflow-wrap: anywhere;
}
.taxonomy-detail-heading h2 {
  font-size: 20px;
}
.taxonomy-detail-category {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 11px;
}
.taxonomy-editor {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.taxonomy-editor p {
  line-height: 1.6;
  overflow-wrap: anywhere;
  font-size: 12px;
}
.taxonomy-editor :deep(.sort-select-options) {
  max-height: min(240px, 40vh);
  overflow-y: auto;
}
.taxonomy-editor-actions {
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 8px;
}
.taxonomy-draft-status {
  color: var(--text-accent);
  font-size: 11px;
}
.taxonomy-detail-links {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 12px;
  margin: 22px 0 14px;
}
.taxonomy-delete {
  color: var(--danger);
}
.taxonomy-confirm-delete {
  background: var(--danger-button);
}
.taxonomy-confirm-delete:hover:not(:disabled) {
  background: var(--danger-button-hover);
}
.taxonomy-related {
  border-top: 1px solid var(--line);
  margin-top: 24px;
  padding-top: 18px;
}
.taxonomy-related h3 {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}
.taxonomy-related h3 span {
  color: var(--muted);
  font-size: 11px;
  font-weight: 400;
}
.taxonomy-related-list {
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
}
.taxonomy-related-list button {
  display: flex;
  gap: 7px;
  align-items: center;
  padding: 7px 9px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
  max-width: 100%;
  text-align: left;
}
.taxonomy-related-list button:hover:not(:disabled) {
  background: var(--surface-hover);
}
.taxonomy-related-list button > svg {
  color: var(--character-text);
}
.taxonomy-related-list button > span {
  min-width: 0;
  overflow-wrap: anywhere;
}
.taxonomy-related-list small {
  color: var(--muted);
  font-size: 11px;
}
.taxonomy-detail-empty {
  min-height: 180px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  gap: 14px;
  text-align: center;
}
.taxonomy-empty {
  padding: 32px;
  text-align: center;
}
.taxonomy-demo {
  font-size: 11px;
}
@media (max-width: 1100px) {
  .taxonomy-workspace {
    padding: 22px;
    gap: 14px;
  }
  .taxonomy-split {
    grid-template-columns: minmax(0, 1fr) 310px;
  }
  .taxonomy-detail {
    padding: 20px;
  }
  .taxonomy-filters {
    gap: 6px;
  }
}
@media (max-width: 760px) {
  .taxonomy-workspace {
    overflow: auto;
  }
  .taxonomy-split {
    flex: none;
    grid-template-columns: minmax(0, 1fr);
    overflow: visible;
  }
  .taxonomy-list {
    max-height: 320px;
  }
  .taxonomy-detail {
    overflow: visible;
    border-left: 0;
    border-top: 1px solid var(--line);
  }
}
</style>

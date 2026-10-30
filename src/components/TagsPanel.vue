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
  },
  {
    id: "artist",
    label: "Artists",
    key: "artists",
  },
  {
    id: "character",
    label: "Characters",
    key: "characters",
  },
  {
    id: "tag",
    label: "Tags",
    key: "tags",
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
const pending = ref<{ entry: TaxonomyEntry; action: "edit" | "delete" } | null>(
  null,
);
const name = ref("");
const copyright = ref("");

const data = computed(() => (native ? catalog.value : props.demoCatalog));

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
  copyright.value = entry.copyright ?? "";
  error.value = "";
}

watch([category, search, repeatedOnly, linkedCount], () => {
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
      if (operation.action === "edit" && category.value === "character") {
        const target = data.value.copyrights.find(
          (entry) => entry.name === copyright.value,
        );

        if (!target) throw new Error("Select the character’s copyright");

        await invoke("update_taxonomy_character", {
          id: Number(operation.entry.id),
          name: name.value,
          copyrightId: Number(target.id),
        });
      } else if (operation.action === "edit")
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
        operation.entry.copyright,
        operation.action === "edit" ? copyright.value : undefined,
      );

    pending.value = null;
    emit("changed");
    emit(
      "notice",
      operation.action === "edit"
        ? "Entry updated."
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
    :aria-label="ui('Tags management')"
    :aria-busy="loading || busy"
  >
    <header class="taxonomy-header">
      <div>
        <h1>{{ ui("Tags") }}</h1>
        <p class="muted">
          {{
            ui(
              "Manage copyrights, artists, characters and tags across your library.",
            )
          }}
        </p>
      </div>
      <button
        v-if="native"
        class="secondary-button"
        :disabled="loading || busy"
        @click="load"
      >
        {{ ui("Refresh") }}
      </button>
    </header>
    <div class="taxonomy-tools">
      <nav class="taxonomy-categories" :aria-label="ui('Metadata categories')">
        <button
          v-for="tab in categories"
          :key="tab.id"
          :class="{ active: category === tab.id }"
          :aria-pressed="category === tab.id"
          :disabled="busy"
          @click="category = tab.id"
        >
          {{ ui(tab.label) }} <span>{{ data[tab.key].length }}</span>
        </button>
      </nav>
      <div class="search-box">
        <AppIcon name="search" :size="17" /><input
          ref="searchInput"
          v-model="search"
          :disabled="busy"
          :aria-label="ui('Search dictionary')"
          :placeholder="ui('Search names…')"
        />
        <button
          v-if="search"
          type="button"
          class="icon-button"
          :disabled="busy"
          :aria-label="ui('Clear search')"
          :title="ui('Clear search')"
          @click="clearSearch"
        >
          <AppIcon name="close" :size="16" />
        </button>
      </div>
    </div>
    <div class="taxonomy-filters">
      <button
        v-if="category === 'character'"
        type="button"
        class="taxonomy-repeat-filter"
        :class="{ active: repeatedOnly }"
        :aria-pressed="repeatedOnly"
        :disabled="busy"
        :title="ui('Show only the same character name in different copyrights')"
        @click="repeatedOnly = !repeatedOnly"
      >
        <span class="taxonomy-filter-check">
          <AppIcon v-if="repeatedOnly" name="check" :size="12" />
        </span>
        {{ ui("Same name in different copyrights") }}
        <span class="taxonomy-filter-count">{{ repeatedNames.size }}</span>
      </button>
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
          :disabled="busy"
          @click="linkedCount = option.value"
        >
          {{ ui(option.label) }}
        </button>
      </div>
    </div>
    <p class="taxonomy-note muted">
      {{
        ui(
          "Deleting a copyright or artist sends linked files to Stage for redistribution. Files stay at their current paths, including when disk synchronization is enabled. Deleting a tag or character only removes that entry and its links.",
        )
      }}
    </p>
    <p v-if="!native" class="muted">
      {{ ui("Demo library · changes last for this session.") }}
    </p>
    <p v-else class="muted">
      {{
        ui(
          "Saved database dictionaries · Main still uses demo records. Queued database files appear in Stage; distribution currently remains a preview.",
        )
      }}
    </p>
    <p v-if="error" class="error-message" role="alert">{{ ui(error) }}</p>
    <div class="taxonomy-list">
      <p v-if="loading" class="taxonomy-empty" role="status">
        {{ ui("Loading dictionaries…") }}
      </p>
      <p v-else-if="!entries.length" class="taxonomy-empty muted">
        {{
          ui(
            search || filtersActive
              ? "No matching names."
              : "No entries in this category yet.",
          )
        }}
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
              <strong
                >{{ entry.name
                }}{{ entry.copyright ? ` (${entry.copyright})` : "" }}</strong
              ><span class="muted">{{
                ui(`${entry.count} linked file${entry.count === 1 ? "" : "s"}`)
              }}</span>
            </div>
            <button
              class="secondary-button"
              :disabled="busy || loading"
              :aria-label="ui(`Edit ${entry.name}`)"
              @click="begin(entry, 'edit')"
            >
              {{ ui("Edit") }}
            </button>
            <button
              class="icon-button taxonomy-delete"
              :disabled="busy || loading"
              :aria-label="ui(`Delete ${entry.name}`)"
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
            <template v-if="pending.action === 'edit'">
              <MetadataInput
                v-model:text-value="name"
                text-input
                label="Name"
                :options="[]"
                :disabled="busy"
                required
              />
              <template v-if="category === 'character'">
                <div>
                  <p class="metadata-label">{{ ui("Copyrights") }}</p>
                  <AppSelect
                    v-model="copyright"
                    :label="ui('Copyrights')"
                    :options="copyrightOptions"
                    :disabled="busy"
                  />
                </div>
                <p class="muted">
                  {{
                    ui(
                      "Changing copyright updates all linked files. The previous copyright is removed if no characters on the file still use it.",
                    )
                  }}
                </p>
              </template>
            </template>
            <p v-else>
              {{ ui(`Delete “${entry.name}”?`) }}
              {{
                ui(
                  category === "tag" || category === "character"
                    ? `This ${category} will be removed from all linked files.`
                    : `All ${entry.count} linked file(s) will need redistribution. ${category === "copyright" ? "Characters belonging to this copyright will also be removed." : ""}`,
                )
              }}
              {{ ui("Files on disk stay unchanged.") }}
            </p>
            <div class="taxonomy-editor-actions">
              <button
                type="button"
                class="secondary-button"
                :disabled="busy"
                @click="pending = null"
              >
                {{ ui("Cancel") }}</button
              ><button
                type="submit"
                class="primary-button"
                :disabled="
                  busy ||
                  (pending.action === 'edit' &&
                    (!name.trim() || (category === 'character' && !copyright)))
                "
              >
                {{
                  ui(
                    busy
                      ? "Saving…"
                      : pending.action === "edit"
                        ? "Save changes"
                        : "Delete entry",
                  )
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
  background: var(--surface-hover);
  border-color: var(--accent-line);
  color: var(--text-accent);
}
.taxonomy-categories span {
  font-size: 11px;
  background: var(--hover-overlay);
  border-radius: 5px;
  padding: 2px 6px;
}
.taxonomy-filters {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}
.taxonomy-repeat-filter,
.taxonomy-link-filters button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  padding: 9px 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  color: var(--muted);
  font-size: 12px;
  transition:
    background 0.15s,
    border-color 0.15s,
    color 0.15s;
}
.taxonomy-repeat-filter:hover:not(:disabled):not(.active),
.taxonomy-link-filters button:hover:not(:disabled):not(.active) {
  background: var(--hover-overlay);
  color: var(--text);
}
.taxonomy-repeat-filter.active,
.taxonomy-link-filters button.active {
  background: var(--surface-hover);
  border-color: var(--accent-line);
  color: var(--text-accent);
}
.taxonomy-filter-check {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 16px;
  height: 16px;
  border: 1px solid currentColor;
  border-radius: 4px;
}
.taxonomy-repeat-filter.active .taxonomy-filter-check {
  background: var(--accent-strong);
  border-color: var(--accent);
  color: var(--on-accent);
}
.taxonomy-filter-count {
  border-radius: 5px;
  padding: 2px 6px;
  background: var(--hover-overlay);
  font-size: 11px;
}
.taxonomy-link-filters {
  display: flex;
  gap: 4px;
  padding: 3px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--surface);
}
.taxonomy-link-filters button {
  border-color: transparent;
  padding: 6px 12px;
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
  color: var(--danger);
}
.taxonomy-editor {
  margin: 0 18px 16px 50px;
  padding: 16px;
  background: var(--surface-raised);
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.taxonomy-editor p {
  line-height: 1.6;
  overflow-wrap: anywhere;
}
.taxonomy-editor :deep(.sort-select-options) {
  max-height: min(240px, 40vh);
  overflow-y: auto;
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

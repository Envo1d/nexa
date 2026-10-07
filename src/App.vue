<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform, version } from "@tauri-apps/plugin-os";
import { usesSystemWindowCorners } from "./utils/windowCorners";
import AppIcon from "./components/AppIcon.vue";
import SortSelect from "./components/SortSelect.vue";
import MediaCollection from "./components/MediaCollection.vue";
import FileDetails from "./components/FileDetails.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import { useLibrary } from "./composables/useLibrary";
import type { MediaItem, MediaKind } from "./types/media";
import "./styles.css";

const library = useLibrary();
const {
  scope,
  search,
  kind,
  sortMode,
  suggestions,
  favoritesOnly,
  taggedOnly,
  hideUntagged,
  selectedId,
  selected,
  results,
  heading,
  tags,
  typeCounts,
  activeFilterCount,
} = library;
const view = ref<"main" | "stage" | "settings">("main");
const mode = ref<"grid" | "list">("grid");
const showPreviews = ref(true);
const sidebarExpanded = ref(false);
const showAllTags = ref(false);
const searchInput = ref<HTMLInputElement | null>(null);
const searchFocused = ref(false);
const suggestionIndex = ref(-1);
watch(search, () => {
  suggestionIndex.value = -1;
});
function chooseSearchSuggestion(name: string) {
  library.chooseSuggestion(name);
  searchFocused.value = false;
}
function searchKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    searchFocused.value = false;
  } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    searchFocused.value = true;
    const count = suggestions.value.length;
    suggestionIndex.value = count
      ? (suggestionIndex.value +
          (event.key === "ArrowDown" ? 1 : count - 1) +
          count) %
        count
      : -1;
  } else if (event.key === "Enter") {
    event.preventDefault();
    const suggestion = searchFocused.value
      ? suggestions.value[suggestionIndex.value]
      : undefined;
    if (suggestion) chooseSearchSuggestion(suggestion.name);
    else {
      library.commitSearch();
      searchFocused.value = false;
    }
  }
}
const tagInput = ref<HTMLInputElement | null>(null);
const tagTarget = ref<MediaItem | null>(null);
const draftTags = ref<string[]>([]);
const newTag = ref("");
const tagError = ref("");

const optionsTarget = ref<MediaItem | null>(null);
const toast = ref("");
const maximized = ref(false);
const systemWindowCorners =
  isTauri() && usesSystemWindowCorners(platform(), version());
let toastTimer: ReturnType<typeof setTimeout> | undefined;
let unlistenResize: (() => void) | undefined;
const displayedTags = computed(() =>
  showAllTags.value ? tags.value : tags.value.slice(0, 8),
);
const typeOptions: { value: "all" | MediaKind; label: string; icon: string }[] =
  [
    { value: "all", label: "All files", icon: "files" },
    { value: "video", label: "Videos", icon: "video" },
    { value: "image", label: "Images", icon: "image" },
  ];

function notify(message: string) {
  toast.value = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toast.value = "";
  }, 4500);
}
function navigate(nextScope: "gallery" | "favorites") {
  scope.value = nextScope;
  view.value = "main";
}
function previewAction(action: string, item: MediaItem) {
  if (action === "Copy path") {
    notify(`Demo path: ${item.path}`);
  } else {
    notify(
      `${action} is a UI preview. Real file actions will be connected after UI approval.`,
    );
  }
  optionsTarget.value = null;
}
async function editTags(item: MediaItem) {
  tagTarget.value = item;
  draftTags.value = [...item.tags];
  newTag.value = "";
  tagError.value = "";
  optionsTarget.value = null;
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
  if (!draftTags.value.some((tag) => tag.toLowerCase() === value.toLowerCase()))
    draftTags.value.push(value);
  newTag.value = "";
  tagError.value = "";
}
function saveTags() {
  addDraftTag();
  if (tagError.value || !tagTarget.value) return;
  tagTarget.value.tags = [...draftTags.value];
  tagTarget.value = null;
  notify("Tags updated in the demo library.");
}
async function windowAction(action: "minimize" | "maximize" | "close") {
  if (!isTauri()) {
    notify("Window controls are available in the Nexa desktop app.");
    return;
  }
  try {
    const appWindow = getCurrentWindow();
    if (action === "minimize") await appWindow.minimize();
    else if (action === "close") await appWindow.close();
    else {
      await appWindow.toggleMaximize();
      maximized.value = await appWindow.isMaximized();
    }
  } catch {
    notify("The window action could not be completed.");
  }
}
async function startDrag(event: PointerEvent) {
  if (event.button !== 0 || !isTauri()) return;
  try {
    await getCurrentWindow().startDragging();
  } catch {
    notify("Window dragging is unavailable.");
  }
}
function keyboard(event: KeyboardEvent) {
  if (
    (event.ctrlKey || event.metaKey) &&
    event.key.toLowerCase() === "f" &&
    !tagTarget.value &&
    view.value !== "settings" &&
    !optionsTarget.value
  ) {
    event.preventDefault();
    view.value = "main";
    nextTick(() => searchInput.value?.focus());
  }
  if (event.key === "Escape") {
    if (tagTarget.value) tagTarget.value = null;
    else if (view.value === "settings") view.value = "main";
    else if (optionsTarget.value) optionsTarget.value = null;
    else selectedId.value = null;
  }
}
// Keep keyboard focus inside modal previews and restore it when they close.
const modalElement = ref<HTMLElement | null>(null);
let modalTrigger: HTMLElement | null = null;
const modalOpen = computed(() =>
  Boolean(tagTarget.value || optionsTarget.value),
);
watch(modalOpen, async (open) => {
  if (open) {
    modalTrigger =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    await nextTick();
    const focusTarget = tagTarget.value
      ? tagInput.value
      : modalElement.value?.querySelector<HTMLElement>("button, input");
    focusTarget?.focus();
  } else modalTrigger?.focus();
});
function trapFocus(event: KeyboardEvent) {
  if (event.key !== "Tab" || !modalElement.value) return;
  const focusable = [
    ...modalElement.value.querySelectorAll<HTMLElement>(
      "button:not([disabled]), input, [tabindex='0']",
    ),
  ];
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
onMounted(async () => {
  document.addEventListener("keydown", keyboard);
  if (isTauri()) {
    const appWindow = getCurrentWindow();
    maximized.value = await appWindow.isMaximized();
    unlistenResize = await appWindow.onResized(async () => {
      maximized.value = await appWindow.isMaximized();
    });
  }
});
onBeforeUnmount(() => {
  document.removeEventListener("keydown", keyboard);
  clearTimeout(toastTimer);
  unlistenResize?.();
});
</script>

<template>
  <div
    class="app-shell"
    :class="{ maximized, 'system-window-corners': systemWindowCorners }"
  >
    <header class="titlebar">
      <div
        class="brand drag-region"
        @pointerdown="startDrag"
        @dblclick="windowAction('maximize')"
      >
        <img class="brand-mark" src="/icon.png" alt="" draggable="false" /><span
          >Nexa</span
        >
      </div>
      <div
        class="titlebar-drag drag-region"
        @pointerdown="startDrag"
        @dblclick="windowAction('maximize')"
      ></div>
      <nav class="view-tabs" aria-label="Workspace">
        <button
          :class="{ active: view === 'main' }"
          :aria-pressed="view === 'main'"
          @click="view = 'main'"
        >
          Main <span>View</span></button
        ><button
          :class="{ active: view === 'stage' }"
          :aria-pressed="view === 'stage'"
          @click="view = 'stage'"
        >
          Stage <span>View</span>
        </button>
      </nav>
      <div
        class="titlebar-drag drag-region"
        @pointerdown="startDrag"
        @dblclick="windowAction('maximize')"
      ></div>
      <div class="layout-toggle" aria-label="Collection layout">
        <button
          :class="{ active: mode === 'grid' }"
          :aria-pressed="mode === 'grid'"
          aria-label="Grid view"
          :disabled="view !== 'main'"
          @click="mode = 'grid'"
        >
          <AppIcon name="grid" :size="17" /></button
        ><button
          :class="{ active: mode === 'list' }"
          :aria-pressed="mode === 'list'"
          aria-label="List view"
          :disabled="view !== 'main'"
          @click="mode = 'list'"
        >
          <AppIcon name="list" :size="19" />
        </button>
      </div>
      <div class="window-controls">
        <button aria-label="Minimize window" @click="windowAction('minimize')">
          <AppIcon name="minimize" :size="17" /></button
        ><button
          :aria-label="maximized ? 'Restore window' : 'Maximize window'"
          @click="windowAction('maximize')"
        >
          <AppIcon
            :name="maximized ? 'restore' : 'maximize'"
            :size="15"
          /></button
        ><button
          class="window-close"
          aria-label="Close window"
          @click="windowAction('close')"
        >
          <AppIcon name="close" :size="18" />
        </button>
      </div>
    </header>
    <main class="workspace">
      <nav
        class="navigation"
        :class="{ expanded: sidebarExpanded }"
        aria-label="Library navigation"
      >
        <button
          class="nav-item collapse-button"
          :aria-label="sidebarExpanded ? 'Collapse sidebar' : 'Expand sidebar'"
          :aria-expanded="sidebarExpanded"
          @click="sidebarExpanded = !sidebarExpanded"
        >
          <AppIcon name="menu" /><span v-if="sidebarExpanded">Library</span>
        </button>
        <div class="nav-main">
          <button
            class="nav-item"
            :class="{ active: scope === 'gallery' && view === 'main' }"
            title="Gallery"
            :aria-current="
              scope === 'gallery' && view === 'main' ? 'page' : undefined
            "
            @click="navigate('gallery')"
          >
            <AppIcon name="gallery" /><span v-if="sidebarExpanded">Gallery</span
            ><span v-else class="sr-only">Gallery</span></button
          ><button
            class="nav-item"
            :class="{ active: scope === 'favorites' && view === 'main' }"
            title="Favorites"
            :aria-current="
              scope === 'favorites' && view === 'main' ? 'page' : undefined
            "
            @click="navigate('favorites')"
          >
            <AppIcon name="star" /><span v-if="sidebarExpanded">Favorites</span
            ><span v-else class="sr-only">Favorites</span>
          </button>
        </div>
        <button
          class="nav-item settings-button"
          title="Settings"
          :class="{ active: view === 'settings' }"
          :aria-current="view === 'settings' ? 'page' : undefined"
          @click="view = 'settings'"
        >
          <AppIcon name="settings" /><span v-if="sidebarExpanded">Settings</span
          ><span v-else class="sr-only">Settings</span>
        </button>
      </nav>
      <template v-if="view === 'main'">
        <aside class="filters-panel" aria-label="Library filters">
          <header class="panel-heading">
            <h2>Filters</h2>
            <span v-if="activeFilterCount" class="filter-count">{{
              activeFilterCount
            }}</span>
          </header>
          <div class="filters-scroll">
            <div class="metadata-search">
              <div class="search-box">
                <AppIcon name="search" :size="17" /><input
                  ref="searchInput"
                  v-model="search"
                  aria-label="Search metadata"
                  placeholder="Metadata, separated by semicolons"
                  role="combobox"
                  aria-autocomplete="list"
                  :aria-expanded="searchFocused && suggestions.length > 0"
                  aria-controls="metadata-suggestions"
                  :aria-activedescendant="
                    searchFocused && suggestionIndex >= 0
                      ? `metadata-option-${suggestionIndex}`
                      : undefined
                  "
                  @focus="searchFocused = true"
                  @input="searchFocused = true"
                  @blur="searchFocused = false"
                  @keydown="searchKeydown"
                /><button
                  v-if="search"
                  class="icon-button"
                  aria-label="Clear search"
                  @click="library.clearSearch()"
                >
                  <AppIcon name="close" :size="14" /></button
                ><kbd v-else>Ctrl F</kbd>
              </div>
              <div
                v-if="searchFocused && suggestions.length"
                id="metadata-suggestions"
                class="metadata-suggestions"
                role="listbox"
                aria-label="Matching metadata"
              >
                <button
                  v-for="(suggestion, index) in suggestions"
                  :id="`metadata-option-${index}`"
                  :key="`${suggestion.category}:${suggestion.name}`"
                  type="button"
                  role="option"
                  :aria-selected="suggestionIndex === index"
                  :class="{ active: suggestionIndex === index }"
                  @mousedown.prevent
                  @click="chooseSearchSuggestion(suggestion.name)"
                >
                  <span>{{ suggestion.name }}</span
                  ><small>{{ suggestion.category }}</small>
                </button>
              </div>
            </div>
            <section class="filter-section type-section">
              <h3>File type</h3>
              <button
                v-for="option in typeOptions"
                :key="option.value"
                class="type-option"
                :class="{ active: kind === option.value }"
                :aria-pressed="kind === option.value"
                @click="kind = option.value"
              >
                <AppIcon :name="option.icon" :size="17" /><span>{{
                  option.label
                }}</span
                ><span class="type-count">{{ typeCounts[option.value] }}</span>
              </button>
            </section>
            <section class="filter-section">
              <h3>Sorting</h3>
              <SortSelect v-model="sortMode" />
            </section>
            <section class="filter-section">
              <h3>Tags</h3>
              <div class="filter-tags">
                <button
                  v-for="tag in displayedTags"
                  :key="tag.name"
                  class="filter-tag"
                  @click="library.addSearch(tag.name)"
                >
                  <span>{{ tag.name }}</span
                  ><small>{{ tag.count }}</small>
                </button>
              </div>
              <button
                v-if="tags.length > 8"
                class="text-button show-tags"
                @click="showAllTags = !showAllTags"
              >
                {{ showAllTags ? "Show fewer" : "Show all tags"
                }}<AppIcon name="down" :size="13" />
              </button>
            </section>
            <section class="filter-section additional-section">
              <h3>Additional</h3>
              <label
                ><input
                  v-model="favoritesOnly"
                  type="checkbox"
                  role="switch"
                />Favorites only</label
              ><label
                ><input
                  v-model="taggedOnly"
                  type="checkbox"
                  role="switch"
                />Tagged only</label
              ><label
                ><input
                  v-model="hideUntagged"
                  type="checkbox"
                  role="switch"
                />Hide untagged</label
              >
              <label
                ><input
                  v-model="showPreviews"
                  type="checkbox"
                  role="switch"
                />Show previews</label
              >
            </section>
          </div>
          <div class="filter-reset">
            <button class="secondary-button" @click="library.reset()">
              <AppIcon name="reset" :size="16" />Reset filters
            </button>
          </div>
        </aside>
        <MediaCollection
          :items="results"
          :show-previews="showPreviews"
          :mode="mode"
          :selected-id="selectedId"
          :heading="heading"
          :filtered="activeFilterCount > 0"
          @select="selectedId = $event.id"
          @favorite="library.toggleFavorite"
          @options="optionsTarget = $event"
          @reset="library.reset()"
        />
        <FileDetails
          v-if="selected"
          :item="selected"
          :show-previews="showPreviews"
          @close="selectedId = null"
          @favorite="library.toggleFavorite(selected)"
          @tags="editTags(selected)"
          @search="library.addSearch"
          @action="previewAction($event, selected)"
        />
      </template>
      <SettingsPanel
        v-else-if="view === 'settings'"
        @close="view = 'main'"
        @preview="
          notify(`${$event} is a UI preview. No settings or files are changed.`)
        "
      />
      <section v-else class="stage-placeholder">
        <span class="empty-icon"><AppIcon name="play" :size="38" /></span
        ><span class="eyebrow">YOUR NEXT WORKSPACE</span>
        <h1>Stage is coming next</h1>
        <p>
          A space for your media, ready for its next chapter.<br />The Stage
          workflow will be designed separately.
        </p>
        <button class="primary-button" @click="view = 'main'">
          Back to library<AppIcon name="arrow" :size="17" /></button
        ><span class="placeholder-note"
          >Main is ready to explore with demo media.</span
        >
      </section>
    </main>
    <Transition name="toast"
      ><div v-if="toast" class="toast-message" role="status">
        <AppIcon name="info" :size="18" /><span>{{ toast }}</span
        ><button
          class="icon-button"
          aria-label="Dismiss message"
          @click="toast = ''"
        >
          <AppIcon name="close" :size="16" />
        </button></div
    ></Transition>
    <div
      v-if="modalOpen"
      class="modal-backdrop"
      @click.self="
        tagTarget = null;

        optionsTarget = null;
      "
    >
      <section
        ref="modalElement"
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        @keydown="trapFocus"
      >
        <template v-if="tagTarget"
          ><header class="panel-heading">
            <h2 id="modal-title">Edit tags</h2>
            <button
              class="icon-button"
              aria-label="Close tag editor"
              @click="tagTarget = null"
            >
              <AppIcon name="close" :size="18" />
            </button>
          </header>
          <p class="modal-subtitle">{{ tagTarget.filename }}</p>
          <div class="draft-tags">
            <button
              v-for="tag in draftTags"
              :key="tag"
              class="tag-chip"
              :aria-label="`Remove ${tag} tag`"
              @click="draftTags = draftTags.filter((name) => name !== tag)"
            >
              {{ tag }}<AppIcon name="close" :size="12" /></button
            ><span v-if="!draftTags.length" class="muted"
              >Add a little context to this file.</span
            >
          </div>
          <form class="tag-form" @submit.prevent="addDraftTag">
            <input
              ref="tagInput"
              v-model="newTag"
              aria-label="New tag"
              placeholder="Add a tag..."
              maxlength="25"
            /><button class="secondary-button" type="submit">Add</button>
          </form>
          <p v-if="tagError" class="error-message" role="alert">
            {{ tagError }}
          </p>
          <p class="modal-note">
            Choose an existing tag or create your own. Changes are stored for
            this demo session.
          </p>
          <div class="suggested-tags">
            <button
              v-for="tag in tags
                .filter((entry) => !draftTags.includes(entry.name))
                .slice(0, 8)"
              :key="tag.name"
              class="tag-chip"
              @click="draftTags.push(tag.name)"
            >
              <AppIcon name="plus" :size="12" />{{ tag.name }}
            </button>
          </div>
          <footer class="modal-footer">
            <button class="secondary-button" @click="tagTarget = null">
              Cancel</button
            ><button class="primary-button" @click="saveTags">Save tags</button>
          </footer></template
        >
        <template v-else-if="optionsTarget"
          ><header class="panel-heading">
            <h2 id="modal-title">File options</h2>
            <button
              class="icon-button"
              aria-label="Close file options"
              @click="optionsTarget = null"
            >
              <AppIcon name="close" :size="18" />
            </button>
          </header>
          <p class="modal-subtitle">{{ optionsTarget.filename }}</p>
          <div class="options-menu">
            <button
              @click="
                selectedId = optionsTarget.id;
                optionsTarget = null;
              "
            >
              <AppIcon name="info" :size="18" />View information</button
            ><button
              @click="
                library.toggleFavorite(optionsTarget);
                optionsTarget = null;
              "
            >
              <AppIcon name="heart" :size="18" />{{
                optionsTarget.favorite
                  ? "Remove from favorites"
                  : "Add to favorites"
              }}</button
            ><button @click="editTags(optionsTarget)">
              <AppIcon name="tag" :size="18" />Edit tags</button
            ><button @click="previewAction('Copy path', optionsTarget)">
              <AppIcon name="copy" :size="18" />Preview path
            </button>
          </div></template
        >
      </section>
    </div>
  </div>
</template>

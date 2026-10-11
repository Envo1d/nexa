<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { confirm } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform, version } from "@tauri-apps/plugin-os";
import { usesSystemWindowCorners } from "./utils/windowCorners";
import AppIcon from "./components/AppIcon.vue";
import ToggleSwitch from "./components/ToggleSwitch.vue";
import SortSelect from "./components/SortSelect.vue";
import MediaCollection from "./components/MediaCollection.vue";
import FileDetails from "./components/FileDetails.vue";
import CardContextMenu from "./components/CardContextMenu.vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import SettingsPanel from "./components/SettingsPanel.vue";
import DistributionDialog from "./components/DistributionDialog.vue";
import TagsPanel from "./components/TagsPanel.vue";
import { useDemoTaxonomy } from "./composables/useDemoTaxonomy";
import { useLibrary } from "./composables/useLibrary";
import { useStage } from "./composables/useStage";
import { useStageScan } from "./composables/useStageScan";
import { useCacheUsage } from "./composables/useCacheUsage";
import { useUpdater } from "./composables/useUpdater";
const cacheUsage = useCacheUsage();
let stopCacheUsage: (() => void) | undefined;
let cacheUsageDisposed = false;
import type { MediaItem, MediaKind, MediaMetadataDraft } from "./types/media";
import "./styles.css";

const diskSync = ref<{
  running: boolean;
  completed: number;
  total: number;
  error: string | null;
}>({ running: false, completed: 0, total: 0, error: null });
let stopDiskSync: (() => void) | undefined;
async function retryDiskSync() {
  await invoke("retry_disk_sync");
}
const library = useLibrary();
const stage = useStage();
const stageScan = useStageScan();
watch(
  () => stageScan.status.value.running,
  (running, previous) => {
    if (previous && !running) {
      void refreshRedistribution();
      void library.refresh();
    }
  },
);
const demoTaxonomy = useDemoTaxonomy(library.media, stage.media);
async function refreshAll() {
  await Promise.all([refreshRedistribution(), library.refresh()]);
}
async function refreshRedistribution() {
  try {
    await stage.refreshRedistribution();
  } catch {
    notify("Could not load Stage files. Reopen Stage to retry.");
  }
}
const distributionQueue = ref<MediaItem[]>([]);
const editingMetadata = ref(false);
function distribute(item?: MediaItem) {
  editingMetadata.value = false;
  if (item && !stage.selectedIds.value.includes(item.id)) stage.select(item);
  distributionQueue.value = stage.results.value.filter((entry) =>
    stage.selectedIds.value.includes(entry.id),
  );
  context.value = null;
}
const {
  scope,
  search,
  kind,
  sortMode,
  suggestions,
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
const view = ref<"main" | "stage" | "settings" | "tags">("main");
watch(view, (next) => {
  if (next === "stage") void refreshRedistribution();
  if (next === "main") void library.refresh();
});
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
const tagSaving = ref(false);

const context = ref<{
  item: MediaItem;
  x: number;
  y: number;
  stage: boolean;
} | null>(null);
function openContext(item: MediaItem, event: MouseEvent | KeyboardEvent) {
  const selection = view.value === "stage" ? stage : library;
  if (!selection.selectedIds.value.includes(item.id))
    selection.selectMany([item.id], false);
  const bounds = (event.target as HTMLElement).getBoundingClientRect();
  context.value = {
    item,
    x:
      event instanceof MouseEvent && event.clientX
        ? event.clientX
        : bounds.left,
    y:
      event instanceof MouseEvent && event.clientY
        ? event.clientY
        : bounds.bottom,
    stage: view.value === "stage",
  };
}
async function contextAction(action: string) {
  const item = context.value?.item;
  const stageContext = context.value?.stage;
  const selection = context.value?.stage ? stage : library;
  const targets = selection.results.value.filter((entry) =>
    selection.selectedIds.value.includes(entry.id),
  );
  context.value = null;
  if (!item) return;
  if (action === "Edit metadata") {
    editingMetadata.value = true;
    distributionQueue.value = targets.length ? targets : [item];
    return;
  }
  if (action === "Delete file") {
    if (isTauri() && item.nativeId) {
      await stageFileAction(
        action,
        targets.length ? targets : [item],
        !stageContext,
      );
      return;
    }
    notify(
      `Delete ${targets.length || 1} file(s) is a UI preview. No files were deleted.`,
    );
    return;
  }
  if (action === "Copy filename") {
    try {
      if (isTauri()) await writeText(item.filename);
      else await navigator.clipboard.writeText(item.filename);
      notify("Filename copied.");
    } catch {
      notify("Could not copy the filename.");
    }
  } else previewAction(action, item);
}
async function saveMetadata(ids: string[], draft: MediaMetadataDraft) {
  if (isTauri()) {
    const nativeIds = ids.map(
      (id) => distributionQueue.value.find((item) => item.id === id)?.nativeId,
    );
    await invoke(
      editingMetadata.value ? "save_library_metadata" : "distribute_files",
      { ids: nativeIds, draft },
    );
    await refreshAll();
    return;
  }
  if (!editingMetadata.value) {
    const distributed = stage.media.value.filter((item) =>
      ids.includes(item.id),
    );
    stage.media.value = stage.media.value.filter(
      (item) => !ids.includes(item.id),
    );
    library.media.value.push(...distributed);
  }
  const alphabetically = (values: string[]) =>
    [...values].sort((a, b) => a.localeCompare(b));
  for (const item of library.media.value) {
    if (!ids.includes(item.id)) continue;
    item.copyrights = alphabetically(draft.copyrights);
    item.artists = [...draft.artists];
    item.artist = draft.artists.join(", ");
    item.characterCopyrights = draft.characters
      .map((entry) => ({ ...entry }))
      .sort(
        (a, b) =>
          a.name.localeCompare(b.name) ||
          a.copyright.localeCompare(b.copyright),
      );
    item.characters = alphabetically([
      ...new Set(draft.characters.map((entry) => entry.name)),
    ]);
    item.tags = alphabetically(draft.tags);
    if (item.kind === "video") item.videoTitle = draft.videoTitle;
  }
}
watch([view, mode, results, stage.results], () => {
  context.value = null;
});
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
const fileActionBusy = ref(false);
async function stageFileAction(
  action: string,
  items: MediaItem[],
  main = view.value === "main",
) {
  if (fileActionBusy.value) return;
  const operation =
    action === "Share"
      ? "share"
      : action === "Delete file"
        ? "delete"
        : action === "Open file" || action === "Open"
          ? "open"
          : "reveal";
  fileActionBusy.value = true;
  context.value = null;
  try {
    if (operation === "delete") {
      const names = items
        .slice(0, 5)
        .map((item) => item.filename)
        .join("\n");
      const accepted = await confirm(
        `Delete ${items.length} file(s) from disk?\n\n${names}${items.length > 5 ? "\n…" : ""}\n\nFiles will be moved to the Recycle Bin when available. Windows will ask before permanently deleting files that cannot be recycled.`,
        {
          title: "Delete files",
          kind: "warning",
          okLabel: "Delete files",
          cancelLabel: "Cancel",
        },
      );
      if (!accepted) return;
    }
    const report = await invoke<{ succeeded: string[]; failed: string[] }>(
      "stage_file_action",
      {
        ids: items.map((item) => item.nativeId),
        action: operation,
        library: main,
      },
    );
    if (operation === "delete") {
      if (main) library.clearSelection();
      else stage.clearSelection();
      await Promise.all([stage.refreshRedistribution(), library.refresh()]);
    }
    if (report.failed.length)
      notify(
        `${report.succeeded.length} completed; ${report.failed.length} failed. ${report.failed[0]}`,
      );
    else if (operation === "delete")
      notify(`${report.succeeded.length} file(s) deleted.`);
  } catch (cause) {
    notify(`File action failed: ${String(cause)}`);
  } finally {
    fileActionBusy.value = false;
  }
}
async function previewAction(action: string, item: MediaItem) {
  if (action === "Distribute" && view.value === "stage") {
    distribute(item);
    return;
  }
  if (
    isTauri() &&
    item.nativeId &&
    ["Open", "Open file", "Open folder", "Show in Explorer", "Share"].includes(
      action,
    )
  ) {
    await stageFileAction(action, [item]);
    return;
  }
  if (action === "File options") {
    context.value = {
      item,
      stage: view.value === "stage",
      x: window.innerWidth - 320,
      y: window.innerHeight - 250,
    };
    return;
  }
  if (action === "Copy path") {
    try {
      if (isTauri()) await writeText(item.path);
      else await navigator.clipboard.writeText(item.path);
      notify("Path copied.");
    } catch {
      notify("Could not copy the path.");
    }
  } else {
    notify(
      `${action} is a UI preview. Real file actions will be connected after UI approval.`,
    );
  }
  context.value = null;
}
async function editTags(item: MediaItem) {
  tagTarget.value = item;
  draftTags.value = [...item.tags];
  newTag.value = "";
  tagError.value = "";
  context.value = null;
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
async function saveTags() {
  if (tagSaving.value) return;
  addDraftTag();
  if (tagError.value || !tagTarget.value) return;
  if (isTauri()) {
    tagSaving.value = true;
    try {
      await invoke("save_library_tags", {
        id: tagTarget.value.nativeId,
        tags: draftTags.value,
      });
      await library.refresh();
    } catch (cause) {
      tagError.value = `Could not save tags: ${String(cause)}`;
      return;
    } finally {
      tagSaving.value = false;
    }
  } else tagTarget.value.tags = [...draftTags.value];
  tagTarget.value = null;
  notify("Tags updated.");
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
function preventBrowserContextMenu(event: MouseEvent) {
  event.preventDefault();
}
function keyboard(event: KeyboardEvent) {
  if (
    (event.ctrlKey || event.metaKey) &&
    event.key.toLowerCase() === "f" &&
    !tagTarget.value &&
    !distributionQueue.value.length &&
    (view.value === "main" || view.value === "stage") &&
    !context.value
  ) {
    event.preventDefault();
    nextTick(() => searchInput.value?.focus());
  }
  if (event.key === "Escape") {
    if (distributionQueue.value.length) distributionQueue.value = [];
    else if (tagTarget.value && !tagSaving.value) tagTarget.value = null;
    else if (view.value === "settings" || view.value === "tags")
      view.value = "main";
    else if (context.value) context.value = null;
    else if (view.value === "stage") stage.clearSelection();
    else library.clearSelection();
  }
}
// Keep keyboard focus inside modal previews and restore it when they close.
const modalElement = ref<HTMLElement | null>(null);
let modalTrigger: HTMLElement | null = null;
const modalOpen = computed(() => Boolean(tagTarget.value));
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
  void useUpdater().checkOnStartup(notify);
  void cacheUsage
    .connect(notify)
    .then((stop) => {
      if (cacheUsageDisposed) stop();
      else stopCacheUsage = stop;
    })
    .catch(() => notify("Could not connect cache usage notifications."));
  void stageScan
    .connect()
    .catch(() => notify("Could not connect Stage scan progress."));
  void refreshRedistribution();
  void library.refresh();
  document.addEventListener("contextmenu", preventBrowserContextMenu, true);
  document.addEventListener("keydown", keyboard);
  if (isTauri()) {
    let received = false;
    stopDiskSync = await listen<typeof diskSync.value>(
      "disk-sync-status",
      (event) => {
        received = true;
        const previous = diskSync.value;
        diskSync.value = event.payload;
        if (previous.running && !event.payload.running) void refreshAll();
      },
    );
    if (cacheUsageDisposed) {
      stopDiskSync();
      return;
    }
    const snapshot = await invoke<typeof diskSync.value>(
      "get_disk_sync_status",
    );
    if (!received) diskSync.value = snapshot;
    const appWindow = getCurrentWindow();
    maximized.value = await appWindow.isMaximized();
    unlistenResize = await appWindow.onResized(async () => {
      maximized.value = await appWindow.isMaximized();
    });
  }
});
onBeforeUnmount(() => {
  cacheUsageDisposed = true;
  stopCacheUsage?.();
  stopDiskSync?.();
  document.removeEventListener("contextmenu", preventBrowserContextMenu, true);
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
          :disabled="view === 'settings' || view === 'tags'"
          @click="mode = 'grid'"
        >
          <AppIcon name="grid" :size="17" /></button
        ><button
          :class="{ active: mode === 'list' }"
          :aria-pressed="mode === 'list'"
          aria-label="List view"
          :disabled="view === 'settings' || view === 'tags'"
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
          <button
            class="nav-item"
            title="Tags"
            :class="{ active: view === 'tags' }"
            :aria-current="view === 'tags' ? 'page' : undefined"
            @click="view = 'tags'"
          >
            <AppIcon name="tag" /><span v-if="sidebarExpanded">Tags</span
            ><span v-else class="sr-only">Tags</span>
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
                :aria-expanded="showAllTags"
                @click="showAllTags = !showAllTags"
              >
                {{ showAllTags ? "Show fewer" : "Show all tags"
                }}<AppIcon
                  name="down"
                  :size="13"
                  class="show-tags-arrow"
                  :class="{ expanded: showAllTags }"
                />
              </button>
            </section>
            <section class="filter-section additional-section">
              <h3>Additional</h3>
              <ToggleSwitch v-model="taggedOnly">Tagged only</ToggleSwitch
              ><ToggleSwitch v-model="hideUntagged">Hide untagged</ToggleSwitch>
              <ToggleSwitch v-model="showPreviews">Show previews</ToggleSwitch>
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
          :native="isTauri()"
          :total="isTauri() ? library.total.value : undefined"
          :has-more="library.hasMore.value"
          :loading="library.loading.value"
          :error="library.error.value"
          :show-previews="showPreviews"
          :mode="mode"
          :selected-id="selectedId"
          :selected-ids="library.selectedIds.value"
          :heading="heading"
          :filtered="activeFilterCount > 0"
          @load-more="library.loadMore"
          @retry="library.refresh()"
          @select="library.select"
          @selection-start="library.beginSelection"
          @select-many="library.selectMany($event, false)"
          @selection-end="library.selectMany"
          @favorite="library.toggleFavorite"
          @context="openContext"
          @reset="library.reset()"
        />
        <FileDetails
          v-if="selected"
          :item="selected"
          :show-previews="showPreviews"
          @close="library.clearSelection()"
          @favorite="library.toggleFavorite(selected)"
          @tags="editTags(selected)"
          @search="library.addSearch"
          @action="previewAction($event, selected)"
        />
      </template>
      <TagsPanel
        v-else-if="view === 'tags'"
        :demo-catalog="demoTaxonomy.catalog.value"
        :change-demo="demoTaxonomy.change"
        @changed="refreshAll"
        @notice="notify"
      />
      <SettingsPanel
        v-else-if="view === 'settings'"
        @close="view = 'main'"
        @saved="
          refreshAll();
          notify('Settings saved.');
        "
        @preview="
          notify(`${$event} is a UI preview. No settings or files are changed.`)
        "
      />
      <template v-else>
        <aside class="filters-panel" aria-label="Stage filters">
          <header class="panel-heading"><h2>Filters</h2></header>
          <div class="filters-scroll">
            <div class="search-box">
              <AppIcon name="search" :size="17" />
              <input
                ref="searchInput"
                v-model="stage.search.value"
                aria-label="Search files by name or path"
                placeholder="File name or path"
              />
              <button
                v-if="stage.search.value"
                class="icon-button"
                aria-label="Clear search"
                @click="stage.clearSearch()"
              >
                <AppIcon name="close" :size="14" />
              </button>
              <kbd v-else>Ctrl F</kbd>
            </div>
            <section class="filter-section additional-section">
              <h3>Additional</h3>
              <ToggleSwitch v-model="showPreviews">Show previews</ToggleSwitch>
            </section>
          </div>
        </aside>
        <MediaCollection
          :items="stage.results.value"
          :native="isTauri()"
          :total="isTauri() ? stage.total.value : undefined"
          :has-more="stage.hasMore.value"
          :loading="stage.loading.value"
          :error="stage.error.value"
          :show-previews="showPreviews"
          :mode="mode"
          :selected-id="stage.selectedId.value"
          :selected-ids="stage.selectedIds.value"
          heading="Stage"
          :filtered="Boolean(stage.search.value.trim())"
          stage
          @load-more="stage.loadMore"
          @retry="refreshRedistribution"
          @select="stage.select"
          @selection-start="stage.beginSelection"
          @select-many="stage.selectMany($event, false)"
          @selection-end="stage.selectMany"
          @distribute="distribute()"
          @context="openContext"
          @reset="stage.clearSearch()"
        />
        <FileDetails
          v-if="stage.selected.value"
          :item="stage.selected.value"
          :show-previews="showPreviews"
          stage
          @close="stage.clearSelection()"
          @action="previewAction($event, stage.selected.value)"
        />
      </template>
    </main>
    <div class="notification-stack">
      <Transition name="toast">
        <div
          v-if="diskSync.running || diskSync.error"
          class="toast-message disk-sync-notification"
          role="status"
        >
          <div class="disk-sync-heading">
            <AppIcon name="info" :size="18" />
            <span>{{
              diskSync.error
                ? `Disk synchronization stopped: ${diskSync.error}`
                : diskSync.total
                  ? `Synchronizing disk: ${diskSync.completed} / ${diskSync.total}`
                  : "Preparing disk synchronization…"
            }}</span>
            <button
              v-if="diskSync.error"
              class="secondary-button"
              @click="retryDiskSync"
            >
              Retry
            </button>
          </div>
          <progress
            aria-label="Disk synchronization progress"
            :max="Math.max(1, diskSync.total)"
            :value="
              diskSync.total || diskSync.error
                ? Math.min(diskSync.completed, Math.max(1, diskSync.total))
                : undefined
            "
          ></progress>
        </div>
      </Transition>
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
    </div>
    <CardContextMenu
      v-if="context"
      :item="context.item"
      :stage="context.stage"
      :selected-count="
        context.stage
          ? stage.selectedIds.value.length
          : library.selectedIds.value.length
      "
      :x="context.x"
      :y="context.y"
      @close="context = null"
      @action="contextAction"
    />
    <DistributionDialog
      v-if="distributionQueue.length"
      :items="distributionQueue"
      :library="library.media.value"
      :show-previews="showPreviews"
      :editing="editingMetadata"
      :catalog="library.catalog.value"
      :persist="saveMetadata"
      @close="distributionQueue = []"
      @open="previewAction('Open file', $event)"
      @processed="
        notify(
          editingMetadata
            ? `${$event} file(s) updated${isTauri() ? '' : ' for this demo session'}.`
            : `${$event} file(s) distributed${isTauri() ? '' : ' for this demo session'}.`,
        )
      "
    />
    <div
      v-if="modalOpen"
      class="modal-backdrop"
      @click.self="!tagSaving && (tagTarget = null)"
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
              :disabled="tagSaving"
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
            <button
              class="secondary-button"
              :disabled="tagSaving"
              @click="tagTarget = null"
            >
              Cancel</button
            ><button
              class="primary-button"
              :disabled="tagSaving"
              @click="saveTags"
            >
              Save tags
            </button>
          </footer></template
        >
      </section>
    </div>
  </div>
</template>

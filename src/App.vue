<script setup lang="ts">
import { computed, ref, shallowRef, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { isNativeData } from "./runtime";
import { translateUi as ui } from "./i18n/presentation";
import AppIcon from "./components/AppIcon.vue";
import ToggleSwitch from "./components/ToggleSwitch.vue";
import ToastNotifications from "./components/ToastNotifications.vue";
import CollectionControls from "./components/CollectionControls.vue";
import MediaCollection from "./components/MediaCollection.vue";
import MediaViewer from "./components/MediaViewer.vue";
import MetadataInput from "./components/MetadataInput.vue";
import FileDetails from "./components/FileDetails.vue";
import CardContextMenu from "./components/CardContextMenu.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import DistributionDialog from "./components/DistributionDialog.vue";
import TagsPanel from "./components/TagsPanel.vue";
import LibraryHealthPanel from "./components/LibraryHealthPanel.vue";
import DuplicateOrderDialog from "./components/DuplicateOrderDialog.vue";
import ReplacementDialog from "./components/ReplacementDialog.vue";
import ContentDuplicatesDialog from "./components/ContentDuplicatesDialog.vue";
import DeleteFilesDialog from "./components/DeleteFilesDialog.vue";
import { useWorkspace } from "./app/useWorkspace";

import "./styles.css";
import type { CollectionPosition, MediaItem } from "./types/media";

const { t } = useI18n();

const contentDuplicatesOpen = ref(false);
const duplicateReplacementMain = ref<string>();
const mainPosition = shallowRef<CollectionPosition>();
const stagePosition = shallowRef<CollectionPosition>();
const healthRevision = ref(0);

function editHealthMetadata(item: MediaItem, collection: string) {
  editingMetadata.value = collection === "library";
  distributionQueue.value = [item];
}

const searchInput = useTemplateRef<HTMLInputElement>("searchInput");

const tagForm = useTemplateRef<HTMLElement>("tagForm");
const tagInput = computed(() => tagForm.value?.querySelector("input") ?? null);

const modalElement = useTemplateRef<HTMLElement>("modalElement");

const {
  viewerOpen,
  viewerItem,
  viewerStage,
  viewerIndex,
  viewerTotal,
  viewerHasNext,
  viewerPaging,
  viewerError,
  closeViewer,
  moveViewer,
  viewerAnnotations,
  diskSync,
  retryDiskSync,
  indexing,
  openIndexing,
  library,
  stage,
  demoTaxonomy,
  refreshAll,
  refreshRedistribution,
  distributionQueue,
  replacementTarget,
  replacementCompleted,
  editingMetadata,
  distribute,
  scope,
  search,
  kind,
  sortMode,
  suggestions,
  taggedOnly,
  hideUntagged,
  selectedId,
  results,
  heading,
  tags,
  typeCounts,
  activeFilterCount,
  view,
  mode,
  showPreviews,
  sidebarExpanded,
  detailsOpen,
  detailsItem,
  detailsCount,
  closeDetails,
  showAllTags,
  searchFocused,
  suggestionIndex,
  chooseSearchSuggestion,
  searchKeydown,
  tagTarget,
  orderTarget,
  draftTags,
  newTag,
  tagError,
  tagSaving,
  context,
  openContext,
  contextAction,
  saveMetadata,
  toasts,
  dismissToast,
  pauseToasts,
  runToastAction,
  settingsSection,
  maximized,
  systemWindowCorners,
  displayedTags,
  notify,
  navigate,
  fileActionBusy,
  deletionRequest,
  answerDeletion,
  stageFileAction,
  previewAction,
  editTags,
  addDraftTag,
  saveTags,
  windowAction,
  startDrag,
  modalOpen,
  trapFocus,
} = useWorkspace({
  searchInput,
  tagInput,
  modalElement,
});

const tagNames = computed(() => [
  ...new Set([
    ...(library.catalog.value?.tags ??
      demoTaxonomy.catalog.value.tags.map((tag) => tag.name)),
    ...library.media.value.flatMap((item) => item.tags),
    ...stage.media.value.flatMap((item) => item.tags),
  ]),
]);

const tagOptions = computed(() =>
  tagNames.value
    .filter(
      (name) =>
        !draftTags.value.some(
          (selected) =>
            selected.toLocaleLowerCase() === name.toLocaleLowerCase(),
        ),
    )
    .map((name) => ({ id: name.toLocaleLowerCase(), name })),
);

const maxTagCount = computed(() =>
  tags.value.reduce((maximum, tag) => Math.max(maximum, tag.count), 0),
);

function openContentDuplicates() {
  if (isNativeData()) {
    contentDuplicatesOpen.value = true;
    return;
  }

  notify(t("duplicatesDemoNotice"));
}
function resetStageFilters() {
  stage.clearSearch();
  stage.kind.value = "all";
}
</script>

<template>
  <div
    class="app-shell"
    :inert="viewerOpen || contentDuplicatesOpen || undefined"
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
      <nav class="view-tabs" :aria-label="t('workspace')">
        <button
          :class="{ active: view === 'main' }"
          :aria-pressed="view === 'main'"
          @click="view = 'main'"
        >
          {{ t("mainView") }}</button
        ><button
          :class="{ active: view === 'stage' }"
          :aria-pressed="view === 'stage'"
          @click="view = 'stage'"
        >
          {{ t("stageView") }}
        </button>
      </nav>
      <div
        class="titlebar-drag drag-region"
        @pointerdown="startDrag"
        @dblclick="windowAction('maximize')"
      ></div>
      <div class="window-controls">
        <button :aria-label="t('minimize')" @click="windowAction('minimize')">
          <AppIcon name="minimize" :size="17" /></button
        ><button
          :aria-label="maximized ? t('restore') : t('maximize')"
          @click="windowAction('maximize')"
        >
          <AppIcon
            :name="maximized ? 'restore' : 'maximize'"
            :size="15"
          /></button
        ><button
          class="window-close"
          :aria-label="t('closeWindow')"
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
        :aria-label="t('navigation')"
      >
        <button
          class="nav-item collapse-button"
          :aria-label="
            sidebarExpanded ? t('collapseSidebar') : t('expandSidebar')
          "
          :aria-expanded="sidebarExpanded"
          @click="sidebarExpanded = !sidebarExpanded"
        >
          <AppIcon name="menu" /><span v-if="sidebarExpanded">{{
            t("library")
          }}</span>
        </button>
        <div class="nav-main">
          <button
            class="nav-item"
            :class="{ active: scope === 'gallery' && view === 'main' }"
            :title="t('gallery')"
            :aria-current="
              scope === 'gallery' && view === 'main' ? 'page' : undefined
            "
            @click="navigate('gallery')"
          >
            <AppIcon name="gallery" /><span v-if="sidebarExpanded">{{
              t("gallery")
            }}</span
            ><span v-else class="sr-only">{{ t("gallery") }}</span></button
          ><button
            class="nav-item"
            :class="{ active: scope === 'favorites' && view === 'main' }"
            :title="t('favorites')"
            :aria-current="
              scope === 'favorites' && view === 'main' ? 'page' : undefined
            "
            @click="navigate('favorites')"
          >
            <AppIcon name="star" /><span v-if="sidebarExpanded">{{
              t("favorites")
            }}</span
            ><span v-else class="sr-only">{{ t("favorites") }}</span>
          </button>
          <button
            class="nav-item"
            :title="t('tags')"
            :class="{ active: view === 'tags' }"
            :aria-current="view === 'tags' ? 'page' : undefined"
            @click="view = 'tags'"
          >
            <AppIcon name="tag" /><span v-if="sidebarExpanded">{{
              t("tags")
            }}</span
            ><span v-else class="sr-only">{{ t("tags") }}</span>
          </button>
          <button
            class="nav-item"
            :title="t('libraryHealth')"
            :class="{ active: view === 'health' }"
            :aria-current="view === 'health' ? 'page' : undefined"
            @click="view = 'health'"
          >
            <AppIcon name="warning" /><span v-if="sidebarExpanded">{{
              t("libraryHealthNav")
            }}</span>
            <span v-else class="sr-only">{{ t("libraryHealth") }}</span>
          </button>
        </div>
        <button
          class="nav-item settings-button"
          :title="t('settings')"
          :class="{ active: view === 'settings' }"
          :aria-current="view === 'settings' ? 'page' : undefined"
          @click="view = 'settings'"
        >
          <AppIcon name="settings" /><span v-if="sidebarExpanded">{{
            t("settings")
          }}</span
          ><span v-else class="sr-only">{{ t("settings") }}</span>
        </button>
      </nav>
      <template v-if="view === 'main'">
        <aside
          class="filters-panel main-filters-panel"
          :aria-label="t('libraryFilters')"
        >
          <header class="panel-heading">
            <h2>{{ t("tags") }}</h2>
            <span v-if="activeFilterCount" class="filter-count">{{
              activeFilterCount
            }}</span>
          </header>
          <div class="filters-scroll">
            <section class="filter-section tag-filter-section">
              <div class="tag-view-toggle" :aria-label="t('tagDisplay')">
                <button
                  :class="{ active: !showAllTags }"
                  :aria-pressed="!showAllTags"
                  @click="showAllTags = false"
                >
                  {{ t("frequentTags") }}
                </button>
                <button
                  :class="{ active: showAllTags }"
                  :aria-pressed="showAllTags"
                  @click="showAllTags = true"
                >
                  {{ t("allTags") }}
                </button>
              </div>
              <div
                class="filter-tags"
                tabindex="0"
                :aria-label="t('filterTags')"
              >
                <button
                  v-for="tag in displayedTags"
                  :key="tag.name"
                  class="filter-tag"
                  @click="library.addSearch(tag.name)"
                >
                  <span class="filter-tag-name">{{ tag.name }}</span>
                  <span class="tag-usage" aria-hidden="true"
                    ><span
                      :style="{
                        width:
                          (maxTagCount ? (tag.count / maxTagCount) * 100 : 0) +
                          '%',
                      }"
                    ></span
                  ></span>
                  <small>{{ tag.count }}</small>
                </button>
              </div>
            </section>
            <section class="filter-section additional-section">
              <h3>{{ t("additional") }}</h3>
              <ToggleSwitch v-model="taggedOnly">{{
                t("taggedOnly")
              }}</ToggleSwitch
              ><ToggleSwitch v-model="hideUntagged">{{
                t("hideUntagged")
              }}</ToggleSwitch>
              <ToggleSwitch v-model="showPreviews">{{
                t("showPreviews")
              }}</ToggleSwitch>
            </section>
          </div>
          <div class="filter-reset">
            <button class="secondary-button" @click="library.reset()">
              <AppIcon name="reset" :size="16" />{{ t("resetFilters") }}
            </button>
            <button class="secondary-button" @click="openContentDuplicates">
              <AppIcon name="copy" :size="16" />{{ t("findDuplicates") }}
            </button>
          </div>
        </aside>
        <MediaCollection
          class="main-collection"
          :items="results"
          :position="mainPosition"
          :collection-key="library.collectionKey.value"
          :native="isNativeData()"
          :total="isNativeData() ? library.total.value : undefined"
          :has-more="library.hasMore.value"
          :loading="library.loading.value"
          :error="library.error.value"
          :show-previews="showPreviews"
          :mode="mode"
          :selected-id="selectedId"
          :selected-ids="library.selectedIds.value"
          :details-open="detailsOpen"
          :heading="heading"
          :filtered="activeFilterCount > 0"
          @load-more="library.loadMore"
          @save-position="mainPosition = $event"
          @retry="library.refresh()"
          @select="library.select"
          @open="previewAction('Open file', $event)"
          @toggle-details="detailsOpen ? closeDetails() : (detailsOpen = true)"
          @selection-start="library.beginSelection"
          @select-many="library.selectMany($event, false)"
          @selection-end="library.selectMany"
          @favorite="library.toggleFavorite"
          @context="openContext"
          @reset="library.reset()"
        >
          <template #toolbar>
            <div class="metadata-search">
              <div class="search-box">
                <AppIcon name="search" :size="17" /><input
                  ref="searchInput"
                  v-model="search"
                  :aria-label="t('metadataSearch')"
                  :placeholder="t('metadataPlaceholder')"
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
                  :aria-label="t('clearSearch')"
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
                :aria-label="t('matches')"
              >
                <button
                  v-for="(suggestion, index) in suggestions"
                  :id="`metadata-option-${index}`"
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
            <CollectionControls
              v-model:kind="kind"
              v-model:sort="sortMode"
              v-model:mode="mode"
              :counts="typeCounts"
              :details-open="detailsOpen"
              @toggle-details="
                detailsOpen ? closeDetails() : (detailsOpen = true)
              "
            />
          </template>
        </MediaCollection>
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
        :section-request="settingsSection"
        @close="view = 'main'"
        @saved="
          refreshAll();
          notify(t('saved'), { kind: 'success' });
        "
        @preview="
          notify(`${$event} is a UI preview. No settings or files are changed.`)
        "
      />
      <LibraryHealthPanel
        v-else-if="view === 'health'"
        :revision="healthRevision"
        @changed="refreshAll"
        @edit="editHealthMetadata"
        @indexing="
          settingsSection = { id: 'indexing' };
          view = 'settings';
        "
      />
      <template v-else>
        <aside
          class="filters-panel stage-queue-panel"
          :aria-label="t('stageQueue')"
        >
          <header class="panel-heading">
            <h2>{{ t("stageQueue") }}</h2>
          </header>
          <div class="filters-scroll">
            <section class="stage-queue-summary">
              <strong>{{ stage.queueTotal.value }}</strong
              ><span>{{ t("stageQueueFiles") }}</span>
              <p>{{ t("stageQueueHint") }}</p>
            </section>
            <section class="filter-section additional-section">
              <ToggleSwitch v-model="showPreviews">{{
                t("showPreviews")
              }}</ToggleSwitch>
            </section>
          </div>
          <div class="filter-reset stage-queue-actions">
            <button
              class="primary-button"
              :disabled="!stage.selectedIds.value.length"
              @click="distribute()"
            >
              <AppIcon name="staging" :size="17" />{{
                t("stageDistributeSelected", {
                  count: stage.selectedIds.value.length,
                })
              }}
            </button>
            <button class="secondary-button" @click="resetStageFilters">
              <AppIcon name="reset" :size="16" />{{ t("resetFilters") }}
            </button>
          </div>
        </aside>
        <MediaCollection
          class="main-collection stage-collection"
          :items="stage.results.value"
          :position="stagePosition"
          :collection-key="stage.queryKey.value"
          :native="isNativeData()"
          :total="isNativeData() ? stage.total.value : undefined"
          :has-more="stage.hasMore.value"
          :loading="stage.loading.value"
          :error="stage.error.value"
          :show-previews="showPreviews"
          :mode="mode"
          :selected-id="stage.selectedId.value"
          :selected-ids="stage.selectedIds.value"
          :details-open="detailsOpen"
          :heading="t('stage')"
          :filtered="
            Boolean(stage.search.value.trim()) || stage.kind.value !== 'all'
          "
          stage
          @load-more="stage.loadMore"
          @save-position="stagePosition = $event"
          @retry="refreshRedistribution"
          @select="stage.select"
          @open="previewAction('Open file', $event)"
          @toggle-details="detailsOpen ? closeDetails() : (detailsOpen = true)"
          @selection-start="stage.beginSelection"
          @select-many="stage.selectMany($event, false)"
          @selection-end="stage.selectMany"
          @distribute="distribute()"
          @context="openContext"
          @reset="resetStageFilters"
        >
          <template #toolbar>
            <div class="search-box">
              <AppIcon name="search" :size="17" />
              <input
                ref="searchInput"
                v-model="stage.search.value"
                :aria-label="t('searchFiles')"
                :placeholder="t('filePlaceholder')"
              />
              <button
                v-if="stage.search.value"
                class="icon-button"
                :aria-label="t('clearSearch')"
                @click="stage.clearSearch()"
              >
                <AppIcon name="close" :size="14" /></button
              ><kbd v-else>Ctrl F</kbd>
            </div>
            <CollectionControls
              v-model:kind="stage.kind.value"
              v-model:sort="stage.sortMode.value"
              v-model:mode="mode"
              :counts="stage.typeCounts.value"
              :details-open="detailsOpen"
              @toggle-details="
                detailsOpen ? closeDetails() : (detailsOpen = true)
              "
            />
          </template>
          <template #selection>
            <div class="stage-selection-bar" :aria-label="t('stageSelection')">
              <span role="status">{{
                stage.selectedIds.value.length
                  ? t("selectedCount", {
                      count: stage.selectedIds.value.length,
                    })
                  : t("stageSelectionHint")
              }}</span>
              <button
                class="secondary-button"
                :disabled="!stage.selectedIds.value.length"
                @click="stage.clearSelection()"
              >
                {{ t("clearSelection") }}
              </button>
              <button
                class="primary-button"
                :disabled="!stage.selectedIds.value.length"
                @click="distribute()"
              >
                <AppIcon name="staging" :size="16" />{{ t("distribute") }}
              </button>
            </div>
          </template>
        </MediaCollection>
      </template>
      <Transition name="details-drawer">
        <div
          v-if="detailsOpen && (view === 'main' || view === 'stage')"
          id="file-details-drawer"
          class="details-drawer"
        >
          <FileDetails
            v-if="detailsItem"
            :item="detailsItem"
            :show-previews="showPreviews"
            :stage="view === 'stage'"
            @close="closeDetails"
            @favorite="library.toggleFavorite(detailsItem)"
            @tags="editTags(detailsItem)"
            @search="library.addSearch"
            @action="previewAction($event, detailsItem)"
          />
          <aside
            v-else
            class="details-panel"
            :aria-label="t('fileInformation')"
          >
            <header class="panel-heading">
              <h2>{{ t("fileInformation") }}</h2>
              <button
                class="icon-button"
                :aria-label="t('closeFileInformation')"
                @click="closeDetails"
              >
                <AppIcon name="close" :size="18" />
              </button>
            </header>
            <div class="details-empty" role="status">
              <AppIcon name="info" :size="32" />
              <h3>
                {{
                  detailsCount
                    ? t("selectedCount", { count: detailsCount })
                    : t("noSelection")
                }}
              </h3>
              <p>{{ t("selectOne") }}</p>
            </div>
          </aside>
        </div>
      </Transition>
    </main>
    <ToastNotifications
      :toasts="toasts"
      :disk-sync="diskSync"
      :indexing="indexing"
      @dismiss="dismissToast"
      @action="runToastAction"
      @pause="pauseToasts"
      @retry="retryDiskSync"
      @indexing="openIndexing"
    />
    <MediaViewer
      v-if="viewerOpen && viewerItem"
      :tag-names="tagNames"
      :item="viewerItem"
      :stage="viewerStage"
      :index="viewerIndex"
      :total="viewerTotal"
      :has-next="viewerHasNext"
      :paging="viewerPaging"
      :paging-error="viewerError"
      @close="closeViewer"
      @move="moveViewer"
      @annotations="viewerAnnotations"
      @external="stageFileAction('Open file', [$event], !viewerStage)"
    />
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
    <DeleteFilesDialog
      v-if="deletionRequest"
      :items="deletionRequest.items"
      :trigger="deletionRequest.trigger"
      :show-previews="showPreviews"
      @answer="answerDeletion"
    />
    <DuplicateOrderDialog
      v-if="orderTarget"
      :item="orderTarget"
      :show-previews="showPreviews"
      @close="orderTarget = null"
      @saved="
        notify('File order saved.');
        library.refresh();
      "
    />
    <ReplacementDialog
      v-if="replacementTarget"
      :item="replacementTarget"
      :preferred-main-id="duplicateReplacementMain"
      :opening="fileActionBusy"
      :catalog="library.catalog.value"
      :show-previews="showPreviews"
      @close="
        replacementTarget = null;
        duplicateReplacementMain = undefined;
      "
      @replaced="replacementCompleted"
      @open="(item, main) => stageFileAction('Open file', [item], main)"
    />
    <ContentDuplicatesDialog
      v-if="contentDuplicatesOpen"
      :show-previews="showPreviews"
      :opening="fileActionBusy"
      @close="contentDuplicatesOpen = false"
      @open="(item, main) => stageFileAction('Open file', [item], main, true)"
      @remove="
        (item, main) => {
          contentDuplicatesOpen = false;
          stageFileAction('Delete file', [item], main, true);
        }
      "
      @replace="
        (stageItem, mainItem) => {
          contentDuplicatesOpen = false;
          duplicateReplacementMain = mainItem.nativeId;
          replacementTarget = stageItem;
        }
      "
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
        healthRevision++;
        notify(
          editingMetadata
            ? `${$event} file(s) updated${isNativeData() ? '' : ' for this demo session'}.`
            : `${$event} file(s) distributed${isNativeData() ? '' : ' for this demo session'}.`,
        );
      "
    />
    <div
      v-if="modalOpen"
      class="modal-backdrop"
      @click.self="!tagSaving && (tagTarget = null)"
    >
      <section
        ref="modalElement"
        class="modal tag-editor"
        role="dialog"
        tabindex="-1"
        aria-modal="true"
        aria-labelledby="modal-title"
        @keydown="trapFocus"
      >
        <template v-if="tagTarget"
          ><header class="panel-heading">
            <div class="tag-editor-heading">
              <span class="tag-editor-icon"
                ><AppIcon name="tag" :size="20"
              /></span>
              <div>
                <h2 id="modal-title">{{ t("editTags") }}</h2>
                <p class="modal-subtitle">{{ tagTarget.filename }}</p>
              </div>
            </div>
            <button
              class="icon-button"
              :aria-label="t('closeTagEditor')"
              :disabled="tagSaving"
              @click="tagTarget = null"
            >
              <AppIcon name="close" :size="18" />
            </button>
          </header>
          <section class="tag-editor-selection" :aria-label="t('selectedTags')">
            <div class="tag-editor-selection-heading">
              <h3>{{ t("selectedTags") }}</h3>
              <span>{{ draftTags.length }}</span>
            </div>
            <div class="draft-tags">
              <button
                v-for="tag in draftTags"
                :key="tag"
                class="tag-chip"
                :disabled="tagSaving"
                :aria-label="t('removeTag', { tag })"
                @click="draftTags = draftTags.filter((name) => name !== tag)"
              >
                {{ tag }}<AppIcon name="close" :size="12" /></button
              ><span v-if="!draftTags.length" class="muted">{{
                t("noTagsYet")
              }}</span>
            </div>
          </section>
          <div class="tag-editor-search">
            <p class="tag-editor-hint">{{ t("tagSearchHint") }}</p>
            <form ref="tagForm" class="tag-form" @submit.prevent="addDraftTag">
              <MetadataInput
                v-model:text-value="newTag"
                label="Tags"
                hide-label
                :placeholder="t('addTagPlaceholder')"
                :options="tagOptions"
                text-input
                :disabled="tagSaving"
                @keydown.enter="!$event.isComposing && addDraftTag()"
              /><button
                class="secondary-button"
                type="submit"
                :disabled="tagSaving || !newTag.trim()"
              >
                <AppIcon name="plus" :size="16" />
                {{ t("add") }}
              </button>
            </form>
          </div>
          <p v-if="tagError" class="error-message" role="alert">
            {{ ui(tagError) }}
          </p>
          <p v-if="!isNativeData()" class="modal-note">
            {{ t("tagDemoNotice") }}
          </p>
          <footer class="modal-footer">
            <button
              class="secondary-button"
              :disabled="tagSaving"
              @click="tagTarget = null"
            >
              {{ t("cancel") }}</button
            ><button
              class="primary-button"
              :disabled="tagSaving"
              @click="saveTags"
            >
              {{ t("saveTags") }}
            </button>
          </footer></template
        >
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import { isTauri } from "@tauri-apps/api/core";

import AppIcon from "./components/AppIcon.vue";
import ToggleSwitch from "./components/ToggleSwitch.vue";
import SortSelect from "./components/SortSelect.vue";
import MediaCollection from "./components/MediaCollection.vue";
import FileDetails from "./components/FileDetails.vue";
import CardContextMenu from "./components/CardContextMenu.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import DistributionDialog from "./components/DistributionDialog.vue";
import TagsPanel from "./components/TagsPanel.vue";
import DuplicateOrderDialog from "./components/DuplicateOrderDialog.vue";
import ReplacementDialog from "./components/ReplacementDialog.vue";
import DeleteFilesDialog from "./components/DeleteFilesDialog.vue";

import { useWorkspace } from "./app/useWorkspace";

import "./styles.css";

const { t } = useI18n();

const searchInput = useTemplateRef<HTMLInputElement>("searchInput");

const tagInput = useTemplateRef<HTMLInputElement>("tagInput");

const modalElement = useTemplateRef<HTMLElement>("modalElement");

const {
  diskSync,
  retryDiskSync,
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
  toast,
  maximized,
  systemWindowCorners,
  displayedTags,
  typeOptions,
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
} = useWorkspace({ searchInput, tagInput, modalElement });
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
      <div class="layout-toggle" :aria-label="t('layout')">
        <button
          :class="{ active: mode === 'grid' }"
          :aria-pressed="mode === 'grid'"
          :aria-label="t('grid')"
          :disabled="view === 'settings' || view === 'tags'"
          @click="mode = 'grid'"
        >
          <AppIcon name="grid" :size="17" /></button
        ><button
          :class="{ active: mode === 'list' }"
          :aria-pressed="mode === 'list'"
          :aria-label="t('list')"
          :disabled="view === 'settings' || view === 'tags'"
          @click="mode = 'list'"
        >
          <AppIcon name="list" :size="19" />
        </button>
      </div>
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
            <h2>{{ t("filters") }}</h2>
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
                  ><small>{{ suggestion.category }}</small>
                </button>
              </div>
            </div>
            <section class="filter-section type-section">
              <h3>{{ t("fileType") }}</h3>
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
              <h3>{{ t("sorting") }}</h3>
              <SortSelect v-model="sortMode" />
            </section>
            <section class="filter-section tag-filter-section">
              <h3>{{ t("tags") }}</h3>
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
                {{ showAllTags ? t("showFewer") : t("showAllTags")
                }}<AppIcon
                  name="down"
                  :size="13"
                  class="show-tags-arrow"
                  :class="{ expanded: showAllTags }"
                />
              </button>
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
          :details-open="detailsOpen"
          :heading="heading"
          :filtered="activeFilterCount > 0"
          @load-more="library.loadMore"
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
          notify(t('saved'));
        "
        @preview="
          notify(`${$event} is a UI preview. No settings or files are changed.`)
        "
      />
      <template v-else>
        <aside class="filters-panel" :aria-label="t('stageFilters')">
          <header class="panel-heading">
            <h2>{{ t("filters") }}</h2>
          </header>
          <div class="filters-scroll">
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
                <AppIcon name="close" :size="14" />
              </button>
              <kbd v-else>Ctrl F</kbd>
            </div>
            <section class="filter-section additional-section">
              <h3>{{ t("additional") }}</h3>
              <ToggleSwitch v-model="showPreviews">{{
                t("showPreviews")
              }}</ToggleSwitch>
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
          :details-open="detailsOpen"
          :heading="t('stage')"
          :filtered="Boolean(stage.search.value.trim())"
          stage
          @load-more="stage.loadMore"
          @retry="refreshRedistribution"
          @select="stage.select"
          @open="previewAction('Open file', $event)"
          @toggle-details="detailsOpen ? closeDetails() : (detailsOpen = true)"
          @selection-start="stage.beginSelection"
          @select-many="stage.selectMany($event, false)"
          @selection-end="stage.selectMany"
          @distribute="distribute()"
          @context="openContext"
          @reset="stage.clearSearch()"
        />
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
                ? t("syncStopped", { error: diskSync.error })
                : diskSync.total
                  ? t("syncCount", diskSync)
                  : t("preparingSync")
            }}</span>
            <button
              v-if="diskSync.error"
              class="secondary-button"
              @click="retryDiskSync"
            >
              {{ t("retry") }}
            </button>
          </div>
          <progress
            :aria-label="t('syncProgress')"
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
            :aria-label="t('dismissMessage')"
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
      :opening="fileActionBusy"
      :catalog="library.catalog.value"
      :show-previews="showPreviews"
      @close="replacementTarget = null"
      @replaced="replacementCompleted"
      @open="(item, main) => stageFileAction('Open file', [item], main)"
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
            <h2 id="modal-title">{{ t("editTags") }}</h2>
            <button
              class="icon-button"
              :aria-label="t('closeTagEditor')"
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
              :aria-label="t('removeTag', { tag })"
              @click="draftTags = draftTags.filter((name) => name !== tag)"
            >
              {{ tag }}<AppIcon name="close" :size="12" /></button
            ><span v-if="!draftTags.length" class="muted">{{
              t("tagContext")
            }}</span>
          </div>
          <form class="tag-form" @submit.prevent="addDraftTag">
            <input
              ref="tagInput"
              v-model="newTag"
              :aria-label="t('newTag')"
              :placeholder="t('addTagPlaceholder')"
              maxlength="25"
            /><button class="secondary-button" type="submit">
              {{ t("add") }}
            </button>
          </form>
          <p v-if="tagError" class="error-message" role="alert">
            {{ tagError }}
          </p>
          <p class="modal-note">{{ t("tagDemoNotice") }}</p>
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

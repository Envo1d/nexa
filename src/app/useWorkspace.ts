import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type Ref,
} from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useI18n } from "vue-i18n";

import { useDemoTaxonomy } from "../composables/useDemoTaxonomy";
import { useLibrary } from "../composables/useLibrary";
import { useStage } from "../composables/useStage";
import { useToast } from "../shared/composables/useToast";
import { useSearchSuggestions } from "../shared/composables/useSearchSuggestions";
import { useModalFocus } from "../shared/composables/useModalFocus";
import { useFileActions } from "../features/media/useFileActions";
import { useMetadataEditing } from "../features/media/useMetadataEditing";
import { useTagEditor } from "../features/media/useTagEditor";
import { useMediaViewer } from "../features/media/useMediaViewer";
import { useDesktopServices } from "./useDesktopServices";
import { useWindowChrome } from "./useWindowChrome";
import { isNativeData } from "../runtime";

import type { MediaItem, MediaKind } from "../types/media";

interface WorkspaceElements {
  searchInput: Readonly<Ref<HTMLInputElement | null>>;
  tagInput: Readonly<Ref<HTMLInputElement | null>>;
  modalElement: Readonly<Ref<HTMLElement | null>>;
}

export function useWorkspace(elements: WorkspaceElements) {
  const { t } = useI18n();

  const { toasts, notify, dismissToast, pauseToasts, runToastAction } =
    useToast();

  const library = useLibrary();

  const stage = useStage();

  const viewer = useMediaViewer(library, stage);

  const { diskSync, retryDiskSync, indexing, openIndexing } =
    useDesktopServices(refreshAll, notify, (id) => {
      settingsSection.value = { id };
      view.value = "settings";
    });

  const { maximized, systemWindowCorners, windowAction, startDrag } =
    useWindowChrome(notify);

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
  const settingsSection = ref<{ id: string } | null>(null);

  watch(view, (next) => {
    if (next !== "settings") settingsSection.value = null;
  });

  watch(view, (next) => {
    if (next === "stage") void refreshRedistribution();

    if (next === "main") void library.refresh();
  });

  const mode = ref<"grid" | "list">("grid");
  const showPreviews = ref(true);
  const sidebarExpanded = ref(false);
  const detailsOpen = ref(false);

  const detailsSelection = computed(() =>
    view.value === "stage" ? stage : library,
  );

  const detailsCount = computed(
    () => detailsSelection.value.selectedIds.value.length,
  );

  const detailsItem = computed(() => {
    const selection = detailsSelection.value;

    if (selection.selectedIds.value.length !== 1) return null;

    return (
      selection.results.value.find(
        (item) => item.id === selection.selectedIds.value[0],
      ) ?? null
    );
  });

  function closeDetails() {
    detailsOpen.value = false;

    nextTick(() =>
      document.querySelector<HTMLButtonElement>(".details-toggle")?.focus(),
    );
  }

  const showAllTags = ref(false);
  const context = ref<{
    item: MediaItem;
    x: number;
    y: number;
    stage: boolean;
  } | null>(null);

  const closeContext = () => {
    context.value = null;
  };

  const { fileActionBusy, deletionRequest, answerDeletion, stageFileAction } =
    useFileActions(
      library,
      stage,
      () => view.value === "main",
      closeContext,
      notify,
    );

  const {
    distributionQueue,
    replacementTarget,
    editingMetadata,
    distribute,
    saveMetadata,
    replacementCompleted,
  } = useMetadataEditing(library, stage, refreshAll, closeContext, notify);

  const {
    tagInput,
    tagTarget,
    draftTags,
    newTag,
    tagError,
    tagSaving,
    editTags,
    addDraftTag,
    saveTags,
  } = useTagEditor(library, closeContext, notify, elements.tagInput);

  const orderTarget = ref<MediaItem | null>(null);

  const {
    searchInput,
    searchFocused,
    suggestionIndex,
    chooseSearchSuggestion,
    searchKeydown,
  } = useSearchSuggestions(
    search,
    suggestions,
    library.chooseSuggestion,
    library.commitSearch,
    elements.searchInput,
  );

  const modalOpen = computed(() => Boolean(tagTarget.value));

  const { trapFocus } = useModalFocus(
    modalOpen,
    tagInput,
    elements.modalElement,
  );

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

    if (action === "Replace in Main Library" && stageContext && item.nativeId) {
      replacementTarget.value = item;

      return;
    }

    if (action === "Change file order") {
      orderTarget.value = item;

      return;
    }

    if (action === "Edit metadata") {
      editingMetadata.value = true;
      distributionQueue.value = targets.length ? targets : [item];

      return;
    }

    if (action === "Delete file") {
      if (isNativeData() && item.nativeId) {
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

        notify("Filename copied.", { kind: "success" });
      } catch {
        notify("Could not copy the filename.", { kind: "error" });
      }
    } else previewAction(action, item);
  }

  watch([view, mode, results, stage.results], () => {
    context.value = null;
  });

  const displayedTags = computed(() =>
    showAllTags.value ? tags.value : tags.value.slice(0, 8),
  );

  const typeOptions = computed<
    {
      value: "all" | MediaKind;
      label: string;
      icon: string;
    }[]
  >(() => [
    {
      value: "all",
      label: t("allFiles"),
      icon: "files",
    },
    {
      value: "video",
      label: t("videos"),
      icon: "video",
    },
    {
      value: "image",
      label: t("images"),
      icon: "image",
    },
  ]);

  function navigate(nextScope: "gallery" | "favorites") {
    scope.value = nextScope;
    view.value = "main";
  }

  async function previewAction(action: string, item: MediaItem) {
    if (action === "Open" || action === "Open file") {
      closeContext();
      viewer.openViewer(item, view.value === "stage");

      return;
    }

    if (action === "Distribute" && view.value === "stage") {
      distribute(item);

      return;
    }

    if (
      isNativeData() &&
      item.nativeId &&
      [
        "Open",
        "Open file",
        "Open folder",
        "Show in Explorer",
        "Share",
      ].includes(action)
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

        notify("Path copied.", { kind: "success" });
      } catch {
        notify("Could not copy the path.", { kind: "error" });
      }
    } else {
      notify(
        `${action} is a UI preview. Real file actions will be connected after UI approval.`,
      );
    }

    context.value = null;
  }

  function preventBrowserContextMenu(event: MouseEvent) {
    event.preventDefault();
  }

  function keyboard(event: KeyboardEvent) {
    if (
      event.defaultPrevented ||
      viewer.viewerOpen.value ||
      deletionRequest.value ||
      replacementTarget.value
    )
      return;

    if (
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === "f" &&
      !tagTarget.value &&
      !orderTarget.value &&
      !distributionQueue.value.length &&
      (view.value === "main" || view.value === "stage") &&
      !context.value
    ) {
      event.preventDefault();

      nextTick(() => searchInput.value?.focus());
    }

    if (event.key === "Escape") {
      if (orderTarget.value) return;

      if (tagTarget.value) {
        if (!tagSaving.value) tagTarget.value = null;

        return;
      }

      if (distributionQueue.value.length) distributionQueue.value = [];
      else if (view.value === "settings" || view.value === "tags")
        view.value = "main";
      else if (context.value) context.value = null;
      else if (detailsOpen.value) closeDetails();
      else if (view.value === "stage") stage.clearSelection();
      else library.clearSelection();
    }
  }

  onMounted(() => {
    document.addEventListener("contextmenu", preventBrowserContextMenu, true);
    document.addEventListener("keydown", keyboard);
  });

  onBeforeUnmount(() => {
    document.removeEventListener(
      "contextmenu",
      preventBrowserContextMenu,
      true,
    );
    document.removeEventListener("keydown", keyboard);
  });

  return {
    ...viewer,
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
    selected,
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
  };
}

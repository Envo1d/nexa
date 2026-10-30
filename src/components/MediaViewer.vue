<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

import { isNativeData } from "../runtime";
import AppIcon from "./AppIcon.vue";
import MetadataInput from "./MetadataInput.vue";
import NexaVideoPlayer from "./NexaVideoPlayer.vue";
import { useFocusTrap } from "../shared/composables/useFocusTrap";
import { translateUi as ui } from "../i18n/presentation";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  item: MediaItem;
  tagNames: string[];
  stage: boolean;
  index: number;
  total: number;
  hasNext: boolean;
  paging: boolean;
  pagingError: string;
}>();

const emit = defineEmits<{
  close: [];
  move: [direction: number];
  external: [item: MediaItem];
  annotations: [value: { favorite: boolean; tags: string[] }];
}>();

const { t, locale } = useI18n();

type ViewerDetails = {
  videoTitle: string | null;
  artist: string | null;
  copyrights: string[];
  characters: { name: string; copyright: string }[];
  width: number | null;
  height: number | null;
};

const details = ref<ViewerDetails | null>(null);
const activePanel = ref<"tags" | "info" | "help" | null>(null);
const panel = ref<HTMLElement | null>(null);
let panelTrigger: HTMLButtonElement | null = null;

const displayTitle = computed(() =>
  props.item.kind === "video"
    ? (details.value?.videoTitle ?? props.item.videoTitle)?.trim() || t("video")
    : t("image"),
);

const information = computed(() => {
  const current = details.value;
  const width = current ? current.width : props.item.width;
  const height = current ? current.height : props.item.height;

  return [
    ...(props.item.kind === "video"
      ? [
          {
            label: t("uiVideoTitle"),
            values: [current?.videoTitle ?? props.item.videoTitle ?? ""],
          },
        ]
      : []),
    {
      label: t("copyright"),
      values: current ? current.copyrights : props.item.copyrights,
    },
    {
      label: t("artist"),
      values: current
        ? [current.artist ?? ""]
        : (props.item.artists ?? [props.item.artist]),
    },
    {
      label: t("characters"),
      values: current
        ? current.characters.map(
            (entry) => `${entry.name} · ${entry.copyright}`,
          )
        : (props.item.characterCopyrights?.map(
            (entry) => `${entry.name} · ${entry.copyright}`,
          ) ?? props.item.characters),
      characters: true,
    },
    {
      label: t("resolution"),
      values: [width && height ? `${width} × ${height}` : ""],
      technical: true,
    },
    ...(props.item.kind === "video"
      ? [
          {
            label: "FPS",
            values: [
              sourceFps.value?.toLocaleString(locale.value, {
                maximumFractionDigits: 3,
              }) ?? "",
            ],
          },
        ]
      : []),
  ];
});

function dismissPanel(restoreFocus = false) {
  activePanel.value = null;

  if (restoreFocus) panelTrigger?.focus({ preventScroll: true });
}

async function togglePanel(name: "tags" | "info" | "help", event: MouseEvent) {
  if (activePanel.value === name) {
    dismissPanel(true);

    return;
  }

  panelTrigger = event.currentTarget as HTMLButtonElement;
  activePanel.value = name;
  await nextTick();
  panel.value?.focus({ preventScroll: true });
}

function outsidePanel(event: PointerEvent) {
  const target = event.target;

  if (
    target instanceof Element &&
    !target.closest(
      ".viewer-popover, [data-viewer-panel], .distribution-metadata-popup",
    )
  )
    dismissPanel(panel.value?.contains(document.activeElement) ?? false);
}

const dialog = ref<HTMLElement | null>(null);
const closeButton = ref<HTMLButtonElement | null>(null);
const canvas = ref<HTMLElement | null>(null);
const image = ref<HTMLImageElement | null>(null);
const videoPlayer = ref<InstanceType<typeof NexaVideoPlayer> | null>(null);

const sourceFps = ref<number | undefined>(props.item.fps);
const source = ref("");
const loading = ref(true);
const mediaReady = ref(false);
const mediaError = ref("");

const saveError = ref("");
const saving = ref(false);
const annotationsReady = ref(false);
const favorite = ref(false);
const tags = ref<string[]>([]);
const newTag = ref("");

const tagOptions = computed(() =>
  props.tagNames
    .filter(
      (name) =>
        !tags.value.some(
          (selected) =>
            selected.toLocaleLowerCase() === name.toLocaleLowerCase(),
        ),
    )
    .map((name) => ({ id: name.toLocaleLowerCase(), name })),
);

const zoom = ref(1);
const fittedSize = ref<{ width: number; height: number } | null>(null);
const pan = ref({ x: 0, y: 0 });
const dragging = ref(false);

const imageStyle = computed(() => ({
  transform: `translate(${pan.value.x}px, ${pan.value.y}px) scale(${zoom.value})`,
  width: fittedSize.value ? `${fittedSize.value.width}px` : undefined,
  height: fittedSize.value ? `${fittedSize.value.height}px` : undefined,
}));

const trigger =
  document.activeElement instanceof HTMLElement ? document.activeElement : null;

const { trapFocus } = useFocusTrap(dialog);

let requestGeneration = 0;
let observer: ResizeObserver | undefined;
let pointer: {
  id: number;
  x: number;
  y: number;
  startX: number;
  startY: number;
} | null = null;

function stopVideo() {
  videoPlayer.value?.stop();
}

function resetImage() {
  zoom.value = 1;
  pan.value = { x: 0, y: 0 };
  endPan();
}

function fitImage() {
  const element = image.value;
  const viewport = canvas.value;

  if (!element?.naturalWidth || !viewport || !mediaReady.value) return;

  const scale = Math.min(
    viewport.clientWidth / element.naturalWidth,
    viewport.clientHeight / element.naturalHeight,
  );
  fittedSize.value = {
    width: element.naturalWidth * scale,
    height: element.naturalHeight * scale,
  };

  void nextTick(clampPan);
}

function clampPan() {
  if (!canvas.value || !image.value) return;

  const x = Math.max(
    0,
    (image.value.clientWidth * zoom.value - canvas.value.clientWidth) / 2,
  );

  const y = Math.max(
    0,
    (image.value.clientHeight * zoom.value - canvas.value.clientHeight) / 2,
  );

  pan.value = {
    x: Math.max(-x, Math.min(x, pan.value.x)),
    y: Math.max(-y, Math.min(y, pan.value.y)),
  };
}

function changeZoom(value: number) {
  zoom.value = Math.max(0.25, Math.min(16, value));
  clampPan();
}

function originalSize() {
  if (image.value?.clientWidth)
    changeZoom(image.value.naturalWidth / image.value.clientWidth);
}

function wheel(event: WheelEvent) {
  if (props.item.kind !== "image" || !mediaReady.value) return;

  event.preventDefault();
  changeZoom(zoom.value * (event.deltaY < 0 ? 1.15 : 1 / 1.15));
}

function startPan(event: PointerEvent) {
  if (event.button !== 0 || props.item.kind !== "image" || !mediaReady.value)
    return;

  event.preventDefault();
  pointer = {
    id: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    startX: pan.value.x,
    startY: pan.value.y,
  };
  canvas.value?.setPointerCapture(event.pointerId);
  dragging.value = true;
}

function movePan(event: PointerEvent) {
  if (!pointer || pointer.id !== event.pointerId) return;

  pan.value = {
    x: pointer.startX + event.clientX - pointer.x,
    y: pointer.startY + event.clientY - pointer.y,
  };
  clampPan();
}

function endPan() {
  const id = pointer?.id;

  pointer = null;
  dragging.value = false;

  if (id !== undefined && canvas.value?.hasPointerCapture(id))
    canvas.value.releasePointerCapture(id);
}

const imageInteractions = {
  wheel,
  pointerdown: startPan,
  pointermove: movePan,
  pointerup: endPan,
  pointercancel: endPan,
  lostpointercapture: endPan,
  dblclick: resetImage,
};

async function loadSource() {
  if (saving.value) return;

  const request = ++requestGeneration;

  stopVideo();
  source.value = "";
  loading.value = true;
  mediaReady.value = false;
  annotationsReady.value = false;
  mediaError.value = "";
  saveError.value = "";
  newTag.value = "";
  resetImage();
  fittedSize.value = null;
  sourceFps.value = props.item.fps;
  details.value = null;

  try {
    if (isNativeData() && props.item.nativeId) {
      const result = await invoke<{
        path: string;
        favorite: boolean;
        tags: string[];
        fps: number | null;
        details: ViewerDetails;
      }>("get_viewer_source", {
        id: props.item.nativeId,
        library: !props.stage,
      });

      if (request !== requestGeneration) return;

      source.value = convertFileSrc(result.path);
      favorite.value = result.favorite;
      tags.value = result.tags;
      sourceFps.value = result.fps ?? undefined;
      details.value = result.details;
    } else {
      favorite.value = props.item.favorite;
      tags.value = [...props.item.tags];

      if (props.item.kind === "video") {
        mediaError.value = t("viewerDemoVideo");
      } else source.value = props.item.preview;
    }

    annotationsReady.value = true;
    emit("annotations", { favorite: favorite.value, tags: [...tags.value] });
  } catch (cause) {
    if (request === requestGeneration)
      mediaError.value = `${t("viewerUnavailable")} ${ui(String(cause))}`;
  } finally {
    if (request === requestGeneration) loading.value = false;
  }
}

function currentMediaEvent(event: Event) {
  const element = event.currentTarget;

  return (
    element instanceof HTMLElement &&
    element === image.value &&
    element.getAttribute("src") === source.value &&
    Boolean(source.value)
  );
}

function ready(event: Event) {
  if (!currentMediaEvent(event)) return;

  mediaReady.value = true;
  fitImage();
}

function failed(event: Event) {
  if (!currentMediaEvent(event)) return;

  mediaReady.value = false;
  mediaError.value = t("viewerUnsupported");
}

async function saveAnnotations(
  nextFavorite: boolean | null,
  nextTags: string[] | null,
) {
  if (saving.value || props.paging || !annotationsReady.value) return;

  saving.value = true;
  saveError.value = "";

  try {
    if (isNativeData())
      await invoke("save_viewer_annotations", {
        id: props.item.nativeId,
        library: !props.stage,
        favorite: nextFavorite,
        tags: nextTags,
      });

    if (nextFavorite !== null) favorite.value = nextFavorite;

    if (nextTags !== null) {
      tags.value = nextTags;
      newTag.value = "";
    }

    emit("annotations", { favorite: favorite.value, tags: [...tags.value] });
  } catch (cause) {
    saveError.value = `${t("viewerSaveFailed")} ${ui(String(cause))}`;
  } finally {
    saving.value = false;
  }
}

function addTag() {
  const tag = newTag.value.trim().normalize("NFC");

  if (!tag) return;

  if ([...tag].length > 24 || /\p{Cc}/u.test(tag) || tags.value.length >= 100) {
    saveError.value = t("viewerTagLimit");

    return;
  }

  if (
    tags.value.some(
      (entry) => entry.toLocaleLowerCase() === tag.toLocaleLowerCase(),
    )
  ) {
    newTag.value = "";

    return;
  }

  void saveAnnotations(null, [...tags.value, tag]);
}

function move(direction: number) {
  if (saving.value || props.paging) return;

  emit("move", direction);
}

function close() {
  if (!saving.value) emit("close");
}

function keyboard(event: KeyboardEvent) {
  const playerShortcut =
    event.code === "Space" ||
    event.code === "KeyF" ||
    event.code === "KeyM" ||
    event.code === "KeyT" ||
    event.code === "KeyR" ||
    [" ", "f", "m", "t", "r", "k"].includes(event.key.toLowerCase());

  if (props.item.kind === "video" && playerShortcut)
    videoPlayer.value?.keyboard(event);

  trapFocus(event);

  if (event.defaultPrevented || event.isComposing) return;

  if (event.key === "Escape") {
    event.preventDefault();

    if (activePanel.value) dismissPanel(true);
    else close();

    return;
  }

  const target = event.target as HTMLElement;

  // Preserve text editing and the video player's native seeking/volume keys.
  if (
    target.closest(
      "input, textarea, select, video, [role='listbox'], [contenteditable='true']",
    ) ||
    event.ctrlKey ||
    event.metaKey ||
    event.altKey
  )
    return;

  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    event.preventDefault();
    move(event.key === "ArrowLeft" ? -1 : 1);
  } else if (props.item.kind === "image" && mediaReady.value) {
    if (event.key === "+" || event.key === "=") changeZoom(zoom.value * 1.25);
    else if (event.key === "-") changeZoom(zoom.value / 1.25);
    else if (event.key === "0") resetImage();
    else return;

    event.preventDefault();
  }
}

watch(() => [props.item.id, props.item.nativeRevision], loadSource, {
  immediate: true,
});

onMounted(async () => {
  await nextTick();

  closeButton.value?.focus();
  observer = new ResizeObserver(fitImage);

  if (canvas.value) observer.observe(canvas.value);

  document.addEventListener("pointerdown", outsidePanel);
});

onBeforeUnmount(() => {
  ++requestGeneration;
  endPan();
  stopVideo();
  observer?.disconnect();
  document.removeEventListener("pointerdown", outsidePanel);

  const target = trigger?.isConnected
    ? trigger
    : document.querySelector<HTMLElement>(".collection-scroll");

  void nextTick(() => {
    if (target?.isConnected) target.focus({ preventScroll: true });
  });
});
</script>

<template>
  <Teleport to="body">
    <div class="viewer-backdrop">
      <section
        ref="dialog"
        class="media-viewer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="viewer-title"
        tabindex="-1"
        @keydown.stop="keyboard"
      >
        <header class="viewer-header">
          <h2 id="viewer-title" :title="displayTitle">{{ displayTitle }}</h2>
          <span class="viewer-counter" aria-live="polite"
            >{{ index + 1 }} / {{ total }}</span
          >
          <nav class="viewer-actions" :aria-label="t('viewerAnnotations')">
            <button
              class="icon-button viewer-favorite"
              :class="{ 'is-favorite': favorite }"
              :aria-pressed="favorite"
              :aria-label="
                t(favorite ? 'viewerRemoveFavorite' : 'viewerAddFavorite')
              "
              :title="
                t(favorite ? 'viewerRemoveFavorite' : 'viewerAddFavorite')
              "
              :disabled="saving || paging || !annotationsReady"
              @click="saveAnnotations(!favorite, null)"
            >
              <AppIcon name="heart" />
            </button>
            <button
              v-for="entry in [
                { name: 'tags', icon: 'tag', label: t('tags') },
                { name: 'info', icon: 'info', label: t('fileInformation') },
                { name: 'help', icon: 'keyboard', label: t('viewerShortcuts') },
              ] as const"
              :key="entry.name"
              class="icon-button"
              data-viewer-panel
              :aria-label="entry.label"
              :title="entry.label"
              :aria-expanded="activePanel === entry.name"
              :aria-controls="
                activePanel === entry.name ? 'viewer-panel' : undefined
              "
              :class="{ active: activePanel === entry.name }"
              @click="togglePanel(entry.name, $event)"
            >
              <AppIcon :name="entry.icon" />
            </button>
            <button
              class="icon-button"
              :aria-label="t('viewerExternal')"
              :title="t('viewerExternal')"
              :disabled="!isNativeData()"
              @click="emit('external', item)"
            >
              <AppIcon name="external" />
            </button>
            <button
              ref="closeButton"
              class="icon-button"
              :aria-label="t('viewerClose')"
              :title="t('viewerClose')"
              :disabled="saving"
              @click="close"
            >
              <AppIcon name="close" />
            </button>
          </nav>
        </header>
        <div class="viewer-body">
          <div
            ref="canvas"
            class="viewer-canvas"
            :class="{
              dragging,
              pannable: item.kind === 'image' && mediaReady,
            }"
            v-on="item.kind === 'image' ? imageInteractions : {}"
          >
            <img
              v-if="source && item.kind === 'image' && !mediaError"
              ref="image"
              :key="source"
              :src="source"
              :alt="displayTitle"
              :style="imageStyle"
              draggable="false"
              @load="ready"
              @error="failed"
            />
            <NexaVideoPlayer
              v-else-if="source && item.kind === 'video' && !mediaError"
              ref="videoPlayer"
              :key="source"
              :source="source"
              :filename="displayTitle"
              :source-fps="sourceFps"
              @ready="mediaReady = true"
              @failed="
                mediaReady = false;
                mediaError = t('viewerUnsupported');
              "
            />
            <p
              v-if="loading || (!mediaReady && !mediaError)"
              class="viewer-status"
              role="status"
            >
              {{ t("viewerLoading") }}
            </p>
            <div v-if="mediaError" class="viewer-status" role="alert">
              <p>{{ mediaError }}</p>
              <button class="secondary-button" @click="loadSource">
                {{ t("retry") }}
              </button>
            </div>
          </div>

          <button
            class="viewer-navigation previous icon-button"
            :aria-label="t('viewerPrevious')"
            :title="t('viewerPreviousHint')"
            :disabled="index === 0 || paging || saving"
            @click="move(-1)"
          >
            <AppIcon name="arrow" />
          </button>
          <button
            class="viewer-navigation next icon-button"
            :aria-label="t('viewerNext')"
            :title="t('viewerNextHint')"
            :disabled="!hasNext || paging || saving"
            @click="move(1)"
          >
            <AppIcon name="arrow" />
          </button>
          <div v-if="item.kind === 'image'" class="viewer-zoom">
            <button
              class="icon-button"
              :aria-label="t('viewerZoomOut')"
              :disabled="!mediaReady || zoom <= 0.25"
              @click="changeZoom(zoom / 1.25)"
            >
              <AppIcon name="minimize" />
            </button>
            <output>{{ Math.round(zoom * 100) }}%</output>
            <button
              class="icon-button"
              :aria-label="t('viewerZoomIn')"
              :disabled="!mediaReady || zoom >= 16"
              @click="changeZoom(zoom * 1.25)"
            >
              <AppIcon name="plus" />
            </button>
            <button
              class="secondary-button"
              :disabled="!mediaReady"
              @click="resetImage"
            >
              {{ t("viewerFit") }}
            </button>
            <button
              class="secondary-button"
              :disabled="!mediaReady"
              @click="originalSize"
            >
              1:1
            </button>
          </div>

          <p
            v-if="paging || pagingError"
            class="viewer-page-status"
            role="status"
          >
            {{ paging ? t("viewerLoading") : ui(pagingError) }}
          </p>
          <p
            v-if="saveError && activePanel !== 'tags'"
            class="viewer-save-status error-message"
            role="alert"
          >
            {{ saveError }}
          </p>
          <aside
            v-if="activePanel"
            id="viewer-panel"
            ref="panel"
            class="viewer-popover"
            tabindex="-1"
            aria-labelledby="viewer-panel-title"
          >
            <header class="viewer-panel-header">
              <h3 id="viewer-panel-title">
                {{
                  activePanel === "tags"
                    ? t("tags")
                    : activePanel === "info"
                      ? t("fileInformation")
                      : t("viewerShortcuts")
                }}
              </h3>
              <button
                class="icon-button"
                :aria-label="t('uiClose')"
                @click="dismissPanel(true)"
              >
                <AppIcon name="close" :size="16" />
              </button>
            </header>
            <template v-if="activePanel === 'tags'">
              <div class="viewer-tags">
                <button
                  v-for="tag in tags"
                  :key="tag"
                  class="tag-chip"
                  :disabled="saving || paging || !annotationsReady"
                  :aria-label="t('removeTag', { tag })"
                  @click="
                    saveAnnotations(
                      null,
                      tags.filter((entry) => entry !== tag),
                    )
                  "
                >
                  {{ tag }}<AppIcon name="close" :size="12" />
                </button>
                <p v-if="!tags.length" class="muted">{{ t("noTags") }}</p>
              </div>
              <form class="viewer-tag-form" @submit.prevent="addTag">
                <MetadataInput
                  :key="item.id"
                  v-model:text-value="newTag"
                  label="Tags"
                  hide-label
                  :placeholder="t('addTagPlaceholder')"
                  :options="tagOptions"
                  text-input
                  :disabled="saving || paging || !annotationsReady"
                  @keydown.enter="!$event.isComposing && addTag()"
                /><button
                  class="secondary-button viewer-tag-add"
                  :aria-label="t('add')"
                  :title="t('add')"
                  :disabled="
                    saving || paging || !annotationsReady || !newTag.trim()
                  "
                  type="submit"
                >
                  <AppIcon name="plus" :size="18" />
                </button>
              </form>
              <p v-if="saving" role="status">{{ t("viewerSaving") }}</p>
              <p v-if="saveError" class="error-message" role="alert">
                {{ saveError }}
              </p>

              <p class="viewer-note">
                {{ t(isNativeData() ? "viewerInstantSave" : "viewerDemo") }}
              </p>
            </template>
            <template v-else-if="activePanel === 'info'">
              <p v-if="loading" role="status">{{ t("viewerLoading") }}</p>
              <dl v-else class="viewer-information">
                <div
                  v-for="row in information"
                  :key="row.label"
                  :class="{ 'technical-row': row.technical }"
                >
                  <dt>{{ row.label }}</dt>
                  <dd>
                    <span
                      v-for="(value, valueIndex) in row.values.filter(Boolean)"
                      :key="valueIndex"
                      :class="{ 'viewer-character': row.characters }"
                      >{{ value }}</span
                    ><span v-if="!row.values.some(Boolean)" class="muted">{{
                      t("unavailable")
                    }}</span>
                  </dd>
                </div>
              </dl>
            </template>
            <template v-else>
              <dl class="viewer-shortcuts">
                <div>
                  <dt><kbd>← / →</kbd></dt>
                  <dd>{{ t("viewerSwitchFiles") }}</dd>
                </div>
                <template v-if="item.kind === 'image'">
                  <div>
                    <dt><kbd>+ / −</kbd></dt>
                    <dd>{{ t("viewerZoom") }}</dd>
                  </div>
                  <div>
                    <dt><kbd>0</kbd></dt>
                    <dd>{{ t("viewerFit") }}</dd>
                  </div>
                </template>
                <template v-else>
                  <div
                    v-for="row in [
                      { key: 'Space / K', label: 'viewerPlayback' },
                      { key: 'F', label: 'playerFullscreen' },
                      { key: 'M', label: 'playerVolume' },
                      { key: 'R', label: 'playerRepeat' },
                      { key: 'T', label: 'playerDiagnostics' },
                    ]"
                    :key="row.key"
                  >
                    <dt>
                      <kbd>{{ row.key }}</kbd>
                    </dt>
                    <dd>{{ t(row.label) }}</dd>
                  </div>
                </template>
                <div>
                  <dt><kbd>Esc</kbd></dt>
                  <dd>{{ t("viewerEscapeHelp") }}</dd>
                </div>
              </dl>
              <p class="viewer-note">
                {{
                  t(
                    item.kind === "image"
                      ? "viewerImageGestures"
                      : "viewerFullscreenKeys",
                  )
                }}
              </p>
            </template>
          </aside>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.viewer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 80;
  background: var(--backdrop);
  display: flex;
}
.media-viewer {
  display: flex;
  flex-direction: column;
  width: 100%;
  min-height: 0;
  background: var(--surface);
  color: var(--text);
  overflow: hidden;
}
.viewer-header {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 20px;
  min-height: 60px;
  padding: 8px 20px;
  border-bottom: 1px solid var(--line);
}
.viewer-header h2 {
  font-size: 15px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.viewer-counter {
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}
.viewer-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
.viewer-actions .active,
.viewer-favorite.is-favorite {
  color: var(--accent);
  background: var(--accent-soft);
}
.viewer-favorite.is-favorite :deep(svg) {
  fill: currentColor;
}
.viewer-body {
  position: relative;
  flex: 1;
  min-height: 0;
  min-width: 0;
  background: #090a0e;
}
.viewer-canvas {
  position: absolute;
  inset: 0;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #f4f4f8;
}
.viewer-canvas img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  flex-shrink: 0;
  user-select: none;
}
.viewer-canvas.pannable {
  cursor: grab;
  touch-action: none;
}
.viewer-canvas.dragging {
  cursor: grabbing;
}
.viewer-status {
  position: absolute;
  max-width: 70%;
  text-align: center;
  padding: 20px;
  background: var(--media-overlay);
  border: 1px solid #ffffff20;
  border-radius: 12px;
}
.viewer-status p {
  margin-bottom: 12px;
}
.viewer-navigation {
  position: absolute;
  top: 50%;
  margin-top: -22px;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--media-overlay);
  color: var(--on-media);
  border: 1px solid #ffffff20;
}
.viewer-navigation.previous {
  left: 16px;
}
.viewer-navigation.previous :deep(svg) {
  transform: rotate(180deg);
}
.viewer-navigation.next {
  right: 16px;
}
.viewer-navigation:hover:not(:disabled) {
  color: var(--on-media);
  background: #30303ee6;
}
.viewer-zoom {
  position: absolute;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: 1px solid var(--line-strong);
  border-radius: 32px;
  background: var(--surface);
}
.viewer-zoom output {
  min-width: 46px;
  text-align: center;
  font-variant-numeric: tabular-nums;
}
.viewer-zoom .secondary-button {
  border: 0;
  background: transparent;
}
.viewer-page-status,
.viewer-save-status {
  position: absolute;
  bottom: 12px;
  left: 16px;
  max-width: 35%;
  background: var(--media-overlay);
  color: var(--on-media);
  padding: 8px 12px;
  border-radius: 8px;
}
.viewer-save-status {
  bottom: 52px;
}
.viewer-popover {
  position: absolute;
  top: 12px;
  right: 20px;
  width: min(360px, calc(100% - 40px));
  max-height: calc(100% - 110px);
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 18px;
  border: 1px solid var(--line-strong);
  border-radius: 12px;
  background: var(--surface);
  box-shadow: 0 12px 32px #0005;
  z-index: 2;
}
.viewer-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
}
.viewer-panel-header h3 {
  font-size: 14px;
}
.viewer-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.viewer-tags .tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  overflow-wrap: anywhere;
}
.viewer-tag-form {
  display: flex;
  align-items: stretch;
  gap: 8px;
  margin-top: 18px;
}
.viewer-tag-form .metadata-input {
  flex: 1;
  min-width: 0;
}
.viewer-tag-add {
  display: grid;
  place-items: center;
  flex: 0 0 38px;
  padding: 0;
}
.viewer-note {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.7;
  margin-top: 18px;
}
.viewer-information,
.viewer-shortcuts {
  margin: 0;
}
.viewer-information > div {
  display: grid;
  grid-template-columns: 105px minmax(0, 1fr);
  gap: 12px;
  margin: 14px 0;
}
.viewer-information dt,
.viewer-shortcuts dt {
  color: var(--muted);
}
.viewer-information dd,
.viewer-shortcuts dd {
  margin: 0;
  min-width: 0;
  overflow-wrap: anywhere;
}
.viewer-information dd {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.viewer-information dd > span {
  max-width: 100%;
}
.viewer-character {
  color: var(--character-text);
  background: var(--character-bg);
  border: 1px solid var(--character-line);
  padding: 3px 7px;
  border-radius: 5px;
}
.viewer-information > .technical-row {
  border-top: 1px solid var(--line);
  padding-top: 16px;
}
.viewer-shortcuts > div {
  display: grid;
  grid-template-columns: 84px minmax(0, 1fr);
  gap: 12px;
  align-items: start;
  margin: 12px 0;
  line-height: 1.6;
}
kbd {
  display: inline-block;
  background: var(--surface-raised, var(--accent-soft));
  border: 1px solid var(--line);
  border-radius: 5px;
  padding: 1px 6px;
  font: inherit;
  font-size: 12px;
  color: var(--text);
}
@media (max-width: 1100px) {
  .viewer-header {
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 12px;
  }
  .viewer-actions {
    gap: 4px;
  }
}
</style>

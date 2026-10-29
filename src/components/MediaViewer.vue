<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

import AppIcon from "./AppIcon.vue";
import NexaVideoPlayer from "./NexaVideoPlayer.vue";
import { useFocusTrap } from "../shared/composables/useFocusTrap";
import { translateUi as ui } from "../i18n/presentation";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  item: MediaItem;
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

const { t } = useI18n();

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

const zoom = ref(1);
const pan = ref({ x: 0, y: 0 });
const dragging = ref(false);

const imageStyle = computed(() => ({
  transform: `translate(${pan.value.x}px, ${pan.value.y}px) scale(${zoom.value})`,
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
  sourceFps.value = props.item.fps;

  try {
    if (isTauri() && props.item.nativeId) {
      const result = await invoke<{
        path: string;
        favorite: boolean;
        tags: string[];
        fps: number | null;
      }>("get_viewer_source", {
        id: props.item.nativeId,
        library: !props.stage,
      });

      if (request !== requestGeneration) return;

      source.value = convertFileSrc(result.path);
      favorite.value = result.favorite;
      tags.value = result.tags;
      sourceFps.value = result.fps ?? undefined;
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
  clampPan();
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
    if (isTauri())
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
    close();

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
  observer = new ResizeObserver(clampPan);

  if (canvas.value) observer.observe(canvas.value);
});

onBeforeUnmount(() => {
  ++requestGeneration;
  endPan();
  stopVideo();
  observer?.disconnect();

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
          <div class="viewer-heading">
            <span>{{ t("viewerTitle") }} · {{ index + 1 }} / {{ total }}</span>
            <h2 id="viewer-title" :title="item.filename">
              {{ item.filename }}
            </h2>
          </div>
          <button
            class="secondary-button"
            :disabled="!isTauri()"
            @click="emit('external', item)"
          >
            {{ t("viewerExternal") }}
          </button>
          <button
            ref="closeButton"
            class="icon-button"
            :aria-label="t('viewerClose')"
            :disabled="saving"
            @click="close"
          >
            <AppIcon name="close" />
          </button>
        </header>
        <div class="viewer-body">
          <div class="viewer-media">
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
                :alt="item.filename"
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
                :filename="item.filename"
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
            <footer class="viewer-controls">
              <button
                class="secondary-button"
                :disabled="index === 0 || paging || saving"
                :title="t('viewerPreviousHint')"
                @click="move(-1)"
              >
                ← {{ t("viewerPrevious") }}
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
              <button
                class="secondary-button"
                :disabled="!hasNext || paging || saving"
                :title="t('viewerNextHint')"
                @click="move(1)"
              >
                {{ t("viewerNext") }} →
              </button>
            </footer>
            <p
              v-if="paging || pagingError"
              class="viewer-page-status"
              role="status"
            >
              {{ paging ? t("viewerLoading") : ui(pagingError) }}
            </p>
          </div>
          <aside class="viewer-sidebar" :aria-label="t('viewerAnnotations')">
            <button
              class="secondary-button viewer-favorite"
              :class="{ 'is-favorite': favorite }"
              :aria-pressed="favorite"
              :disabled="saving || paging || !annotationsReady"
              @click="saveAnnotations(!favorite, null)"
            >
              <AppIcon name="heart" :size="18" />{{
                t(favorite ? "viewerRemoveFavorite" : "viewerAddFavorite")
              }}
            </button>
            <h3>{{ t("tags") }}</h3>
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
              <input
                v-model="newTag"
                :disabled="saving || paging || !annotationsReady"
                :aria-label="t('newTag')"
                :placeholder="t('addTagPlaceholder')"
              /><button
                class="secondary-button"
                :disabled="
                  saving || paging || !annotationsReady || !newTag.trim()
                "
                type="submit"
              >
                {{ t("add") }}
              </button>
            </form>
            <p v-if="saving" role="status">{{ t("viewerSaving") }}</p>
            <p v-if="saveError" class="error-message" role="alert">
              {{ saveError }}
            </p>
            <p class="viewer-help">{{ t("viewerHelp") }}</p>
            <p v-if="!isTauri()" class="viewer-help">{{ t("viewerDemo") }}</p>
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
  padding: 16px;
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
  border: 1px solid var(--line-strong);
  border-radius: 14px;
  overflow: hidden;
  box-shadow: 0 18px 60px var(--shadow-deep);
}
.viewer-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--line);
}
.viewer-heading {
  flex: 1;
  min-width: 0;
}
.viewer-heading span {
  color: var(--muted);
  font-size: 12px;
}
.viewer-heading h2 {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 16px;
  margin: 5px 0 0;
}
.viewer-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 250px;
}
.viewer-media {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}
.viewer-canvas {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  background: #090a0e;
  color: #f4f4f8;
}
.viewer-canvas img,
.viewer-canvas video {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}
.viewer-canvas img {
  flex-shrink: 0;
  user-select: none;
}
.viewer-canvas video {
  width: 100%;
  height: 100%;
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
  max-width: 75%;
  text-align: center;
  padding: 16px;
  background: #15161de6;
  border-radius: 12px;
}
.viewer-controls {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 12px;
  border-top: 1px solid var(--line);
}
.viewer-zoom {
  display: flex;
  align-items: center;
  gap: 6px;
}
.viewer-zoom output {
  min-width: 42px;
  text-align: center;
  font-variant-numeric: tabular-nums;
}
.viewer-sidebar {
  overflow-y: auto;
  padding: 18px;
  border-left: 1px solid var(--line);
}
.viewer-favorite {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.viewer-favorite.is-favorite {
  color: var(--accent);
}
.viewer-sidebar h3 {
  margin: 22px 0 12px;
  font-size: 14px;
}
.viewer-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
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
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}
.viewer-tag-form input {
  width: 100%;
  min-width: 0;
  background: var(--surface-input);
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: 7px;
  padding: 9px;
}
.viewer-help {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  margin-top: 22px;
}
.viewer-page-status {
  margin: 0;
  padding: 0 12px 10px;
  color: var(--muted);
}
@media (max-width: 1100px) {
  .viewer-body {
    grid-template-columns: minmax(0, 1fr) 210px;
  }
  .viewer-controls {
    flex-wrap: wrap;
  }
  .viewer-zoom {
    order: 3;
    width: 100%;
    justify-content: center;
  }
}
</style>

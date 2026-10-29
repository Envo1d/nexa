<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { useI18n } from "vue-i18n";

import { translateUi as ui } from "../i18n/presentation";
import MediaCard from "./MediaCard.vue";
import AppIcon from "./AppIcon.vue";

import type { CollectionPosition, MediaItem } from "../types/media";

const { t } = useI18n();

const props = defineProps<{
  items: MediaItem[];
  collectionKey: string;
  position?: CollectionPosition;
  showPreviews: boolean;
  mode: "grid" | "list";
  selectedId: string | null;
  selectedIds?: string[];
  heading: string;
  filtered: boolean;
  stage?: boolean;
  native?: boolean;
  total?: number;
  hasMore?: boolean;
  loading?: boolean;
  error?: string;
  detailsOpen: boolean;
}>();

const emit = defineEmits<{
  select: [item: MediaItem, event: MouseEvent];
  open: [item: MediaItem];
  toggleDetails: [];
  selectMany: [ids: string[]];
  selectionStart: [];
  selectionEnd: [ids: string[]];
  distribute: [];
  favorite: [item: MediaItem];
  context: [item: MediaItem, event: MouseEvent | KeyboardEvent];
  reset: [];
  loadMore: [];
  retry: [];
  savePosition: [position: CollectionPosition];
}>();

const savedPosition =
  props.position?.collectionKey === props.collectionKey &&
  props.position.mode === props.mode
    ? props.position
    : undefined;

const viewport = ref<HTMLElement | null>(null);
const width = ref(savedPosition?.width ?? 660);
const height = ref(savedPosition?.height ?? 600);
const scrollTop = ref(savedPosition?.scrollTop ?? 0);

const showBackToTop = computed(
  () => height.value > 0 && scrollTop.value >= height.value * 2 && !drag.value,
);

const loaded = ref(savedPosition?.loaded ?? 36);
const remSize = ref(savedPosition?.remSize ?? 13);

const cardWidth = computed(() => 17.5 * remSize.value);

const gap = computed(() => remSize.value);

const gridPadding = computed(() => 0.3 * remSize.value);

const columns = computed(() =>
  props.mode === "grid"
    ? Math.max(
        1,
        Math.floor(
          (width.value - gridPadding.value + gap.value) /
            (cardWidth.value + gap.value),
        ),
      )
    : 1,
);

const rowHeight = computed(() =>
  props.mode === "grid"
    ? ((cardWidth.value - 2) * 9) / 16 + 6.5 * remSize.value + 2 + gap.value
    : 88,
);

const loadedCount = computed(() => Math.min(loaded.value, props.items.length));

const totalCount = computed(() => props.total ?? props.items.length);

const rowCount = computed(() => Math.ceil(loadedCount.value / columns.value));

const startRow = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / rowHeight.value) - 2),
);

const endRow = computed(() =>
  Math.min(
    rowCount.value,
    Math.ceil((scrollTop.value + height.value) / rowHeight.value) + 3,
  ),
);

const visibleItems = computed(() =>
  props.items.slice(
    startRow.value * columns.value,
    endRow.value * columns.value,
  ),
);

let observer: ResizeObserver | undefined;
let scrollFrame: number | undefined;

const drag = ref<{
  x: number;
  y: number;
  endX: number;
  endY: number;
  base: string[];
  pointerId: number;
} | null>(null);

let dragFrame: number | undefined;
let pointerY = 0;
let pointerX = 0;
let gesturePointer: number | undefined;

const selectionBox = computed(() =>
  drag.value
    ? {
        left: `${Math.min(drag.value.x, drag.value.endX)}px`,
        top: `${Math.min(drag.value.y, drag.value.endY)}px`,
        width: `${Math.abs(drag.value.x - drag.value.endX)}px`,
        height: `${Math.abs(drag.value.y - drag.value.endY)}px`,
      }
    : {},
);

function updateDrag() {
  const area = viewport.value;
  const current = drag.value;

  if (!area || !current) return;

  const bounds = area.getBoundingClientRect();

  current.endX = Math.max(
    0,
    Math.min(area.clientWidth, pointerX - bounds.left),
  );
  current.endY = Math.max(0, pointerY - bounds.top + area.scrollTop);

  const left = Math.min(current.x, current.endX);

  const right = Math.max(current.x, current.endX);

  const top = Math.min(current.y, current.endY);

  const bottom = Math.max(current.y, current.endY);

  const gridWidth =
    columns.value * cardWidth.value + (columns.value - 1) * gap.value;
  const offset = props.mode === "grid" ? (width.value - gridWidth) / 2 : 0;

  const ids = props.items
    .slice(0, loadedCount.value)
    .filter((_, index) => {
      const x =
        offset + (index % columns.value) * (cardWidth.value + gap.value);
      const y = Math.floor(index / columns.value) * rowHeight.value;
      const w = props.mode === "grid" ? cardWidth.value : width.value;

      return (
        x < right &&
        x + w > left &&
        y < bottom &&
        y + rowHeight.value - gap.value > top
      );
    })
    .map((item) => item.id);

  emit("selectMany", [...new Set([...current.base, ...ids])]);
}

function dragTick() {
  const area = viewport.value;

  if (!drag.value || !area) return;

  const bounds = area.getBoundingClientRect();

  const speed =
    pointerY < bounds.top + 32 ? -12 : pointerY > bounds.bottom - 32 ? 12 : 0;

  if (speed) {
    area.scrollTop += speed;
    updateScroll();
  }

  updateDrag();
  dragFrame = requestAnimationFrame(dragTick);
}

async function startSelection(event: PointerEvent) {
  if (
    event.button !== 0 ||
    (event.target as HTMLElement).closest("button, .media-card")
  )
    return;

  const area = viewport.value;

  if (!area) return;

  event.preventDefault();
  pointerX = event.clientX;
  pointerY = event.clientY;
  gesturePointer = event.pointerId;
  area.setPointerCapture(event.pointerId);
  emit("selectionStart");
  // Measure after selection state updates before starting the gesture.
  await nextTick();

  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

  if (gesturePointer !== event.pointerId) return;

  width.value = area.clientWidth;
  height.value = area.clientHeight;

  const bounds = area.getBoundingClientRect();

  const x = event.clientX - bounds.left;
  const y = event.clientY - bounds.top + area.scrollTop;

  drag.value = {
    x,
    y,
    endX: x,
    endY: y,
    base: event.ctrlKey || event.metaKey ? [...(props.selectedIds ?? [])] : [],
    pointerId: event.pointerId,
  };
  dragTick();
}

function moveSelection(event: PointerEvent) {
  if (gesturePointer === undefined) return;

  pointerX = event.clientX;
  pointerY = event.clientY;
  updateDrag();
}

function endSelection() {
  if (dragFrame !== undefined) cancelAnimationFrame(dragFrame);

  dragFrame = undefined;

  const id = gesturePointer;

  gesturePointer = undefined;
  drag.value = null;

  if (id !== undefined) emit("selectionEnd", props.selectedIds ?? []);

  if (id !== undefined && viewport.value?.hasPointerCapture(id))
    viewport.value.releasePointerCapture(id);
}

function onScroll() {
  // Read the latest position once per frame, even when scroll events arrive faster.
  if (scrollFrame !== undefined) return;

  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = undefined;
    updateScroll();
  });
}

function cancelScrollFrame() {
  if (scrollFrame === undefined) return;

  cancelAnimationFrame(scrollFrame);
  scrollFrame = undefined;
}

function updateScroll() {
  if (!viewport.value) return;

  scrollTop.value = viewport.value.scrollTop;

  if (
    scrollTop.value + height.value >=
    rowCount.value * rowHeight.value - rowHeight.value * 3
  ) {
    loaded.value = Math.min(loaded.value + 36, props.items.length);

    if (
      props.hasMore &&
      !props.loading &&
      loaded.value >= props.items.length - 36
    )
      emit("loadMore");
  }
}

function resetScroll() {
  endSelection();
  cancelScrollFrame();
  loaded.value = 36;
  scrollTop.value = 0;
  viewport.value?.scrollTo({ top: 0 });
}

function scrollToTop() {
  viewport.value?.focus({ preventScroll: true });
  viewport.value?.scrollTo({
    top: 0,
    behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches
      ? "instant"
      : "smooth",
  });
}

watch(columns, async (count, previous) => {
  if (props.mode !== "grid" || count === previous) return;

  cancelScrollFrame();

  const firstItem = Math.floor(scrollTop.value / rowHeight.value) * previous;

  await nextTick();

  if (!viewport.value) return;

  viewport.value.scrollTop = Math.floor(firstItem / count) * rowHeight.value;
  updateScroll();
});

let resetItems = false;

watch(
  () => props.collectionKey,
  () => {
    resetItems = true;
    resetScroll();
  },
);

watch(
  () => props.items,
  async (items, previous, onCleanup) => {
    let stale = false;

    onCleanup(() => {
      stale = true;
    });

    if (resetItems || !previous.length || !items.length) {
      resetItems = false;
      resetScroll();

      return;
    }

    const appended =
      props.native &&
      items.length > previous.length &&
      previous.length > 0 &&
      previous.every((item, index) => items[index]?.id === item.id);

    if (appended) {
      void nextTick(updateScroll);
    } else {
      endSelection();
      cancelScrollFrame();

      const top = viewport.value?.scrollTop ?? scrollTop.value;

      const first = Math.min(
        previous.length - 1,
        Math.floor(top / rowHeight.value) * columns.value,
      );

      const indices = new Map(items.map((item, index) => [item.id, index]));

      // Keep the first visible survivor, or a nearby predecessor if the tail was removed.
      let anchor = first;

      while (anchor < previous.length && !indices.has(previous[anchor]!.id))
        anchor++;

      if (anchor === previous.length) {
        anchor = Math.min(first - 1, previous.length - 1);

        while (anchor >= 0 && !indices.has(previous[anchor]!.id)) anchor--;
      }

      const index = anchor >= 0 ? indices.get(previous[anchor]!.id) : undefined;

      const nextTop =
        index === undefined
          ? top
          : Math.max(
              0,
              Math.floor(index / columns.value) * rowHeight.value +
                top -
                Math.floor(anchor / columns.value) * rowHeight.value,
            );

      loaded.value = Math.max(
        loaded.value,
        Math.ceil((nextTop + height.value) / rowHeight.value + 3) *
          columns.value,
      );
      scrollTop.value = nextTop;

      await nextTick();

      if (stale || !viewport.value) return;

      viewport.value.scrollTop = Math.min(
        nextTop,
        Math.max(0, rowCount.value * rowHeight.value - height.value),
      );
      updateScroll();
    }
  },
);

watch(
  () => props.mode,
  async () => {
    endSelection();
    cancelScrollFrame();
    scrollTop.value = 0;

    await nextTick();

    viewport.value?.scrollTo({ top: 0 });
  },
);

onMounted(() => {
  if (viewport.value && savedPosition) {
    viewport.value.scrollTop = savedPosition.scrollTop;
    updateScroll();
  }

  observer = new ResizeObserver(([entry]) => {
    if (entry && entry.contentRect.width > 0 && entry.contentRect.height > 0) {
      remSize.value = parseFloat(
        getComputedStyle(document.documentElement).fontSize,
      );
      width.value = entry.contentRect.width;
      height.value = entry.contentRect.height;
    }
  });

  if (viewport.value) observer.observe(viewport.value);
});

onBeforeUnmount(() => {
  emit("savePosition", {
    collectionKey: props.collectionKey,
    mode: props.mode,
    scrollTop: viewport.value?.scrollTop ?? scrollTop.value,
    loaded: loaded.value,
    width: width.value,
    height: height.value,
    remSize: remSize.value,
  });
  endSelection();
  observer?.disconnect();
  cancelScrollFrame();
});
</script>

<template>
  <section class="collection-panel" :aria-label="t('collection')">
    <header class="collection-heading">
      <div>
        <h1>{{ heading }}</h1>
        <span>{{ t("filesCount", { count: totalCount }) }}</span>
      </div>
      <button
        v-if="stage && selectedIds?.length"
        class="secondary-button"
        @click="emit('distribute')"
      >
        {{ t("distributeCount", { count: selectedIds.length }) }}
      </button>
      <span v-else class="demo-badge"
        ><span></span>
        {{
          native
            ? stage
              ? t("stageLibrary")
              : t("library")
            : stage
              ? t("demoStage")
              : t("demoLibrary")
        }}</span
      >
      <button
        type="button"
        class="icon-button details-toggle"
        :class="{ active: detailsOpen }"
        :aria-label="
          detailsOpen ? t('closeFileInformation') : t('openFileInformation')
        "
        :title="
          detailsOpen ? t('closeFileInformation') : t('openFileInformation')
        "
        :aria-expanded="detailsOpen"
        aria-controls="file-details-drawer"
        @click="emit('toggleDetails')"
      >
        <AppIcon name="info" :size="20" />
      </button>
    </header>
    <div class="collection-viewport">
      <div
        ref="viewport"
        class="collection-scroll"
        tabindex="0"
        :aria-label="t('loadedMedia')"
        @scroll.passive="onScroll"
        @pointerdown="startSelection"
        @pointermove="moveSelection"
        @pointerup="endSelection"
        @pointercancel="endSelection"
        @lostpointercapture="endSelection"
      >
        <div
          v-if="items.length"
          class="virtual-space"
          :style="{ height: `${rowCount * rowHeight}px` }"
        >
          <div
            class="virtual-items"
            :class="mode"
            :style="{
              transform: `translateY(${startRow * rowHeight}px)`,
              gridAutoRows: `${rowHeight - gap}px`,
              gridTemplateColumns:
                mode === 'grid' ? `repeat(${columns}, 17.5rem)` : undefined,
            }"
          >
            <MediaCard
              v-for="item in visibleItems"
              :key="item.id"
              :item="item"
              :stage="stage"
              :show-previews="showPreviews"
              :mode="mode"
              :selected="
                selectedIds
                  ? selectedIds.includes(item.id)
                  : selectedId === item.id
              "
              @select="$emit('select', item, $event)"
              @open="emit('open', item)"
              @favorite="$emit('favorite', item)"
              @context="$emit('context', item, $event)"
            />
          </div>
        </div>
        <div
          v-if="drag"
          class="selection-rectangle"
          :style="selectionBox"
        ></div>
        <div v-if="!items.length" class="empty-state">
          <span class="empty-icon"><AppIcon name="search" :size="32" /></span>
          <h2>
            {{
              loading
                ? t("loadingStage")
                : error
                  ? t("stageFailed")
                  : filtered
                    ? t("noMatches")
                    : stage
                      ? t("noStage")
                      : t("noFavorites")
            }}
          </h2>
          <p>
            {{
              ui(error ?? "") ||
              (loading
                ? t("readingFiles")
                : filtered
                  ? t("adjustFilters")
                  : stage
                    ? t("stageEmpty")
                    : t("favoritesEmpty"))
            }}
          </p>
          <button v-if="error" class="secondary-button" @click="emit('retry')">
            {{ t("retry") }}
          </button>
          <button
            v-if="filtered"
            class="secondary-button"
            @click="$emit('reset')"
          >
            {{ t("resetFilters") }}
          </button>
        </div>
      </div>
      <button
        v-if="showBackToTop"
        type="button"
        class="back-to-top"
        :title="t('backToTop')"
        :aria-label="t('backToTop')"
        @click="scrollToTop"
      >
        <AppIcon name="up" :size="20" />
      </button>
    </div>
    <footer class="collection-footer">
      <span>{{
        t("loadedCount", { loaded: loadedCount, total: totalCount })
      }}</span>
      <div class="load-track">
        <span
          :style="{
            width: `${totalCount ? (loadedCount / totalCount) * 100 : 0}%`,
          }"
        ></span>
      </div>
      <span v-if="loadedCount && loadedCount === totalCount" class="load-done"
        ><AppIcon name="check" :size="14" />{{ t("allLoaded") }}</span
      >
    </footer>
  </section>
</template>

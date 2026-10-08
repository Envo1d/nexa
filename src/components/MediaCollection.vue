<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import type { MediaItem } from "../types/media";
import MediaCard from "./MediaCard.vue";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  items: MediaItem[];
  showPreviews: boolean;
  mode: "grid" | "list";
  selectedId: string | null;
  selectedIds?: string[];
  heading: string;
  filtered: boolean;
  stage?: boolean;
}>();
const emit = defineEmits<{
  select: [item: MediaItem, event: MouseEvent];
  selectMany: [ids: string[]];
  selectionEnd: [ids: string[]];
  distribute: [];
  favorite: [item: MediaItem];
  context: [item: MediaItem, event: MouseEvent | KeyboardEvent];
  reset: [];
}>();
const viewport = ref<HTMLElement | null>(null);
const width = ref(660);
const height = ref(600);
const scrollTop = ref(0);
const loaded = ref(36);
const remSize = ref(13);
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
function startSelection(event: PointerEvent) {
  if (
    !props.stage ||
    event.button !== 0 ||
    (event.target as HTMLElement).closest("button, .media-card")
  )
    return;
  const area = viewport.value;
  if (!area) return;
  event.preventDefault();
  const bounds = area.getBoundingClientRect();
  pointerX = event.clientX;
  pointerY = event.clientY;
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
  area.setPointerCapture(event.pointerId);
  dragTick();
}
function moveSelection(event: PointerEvent) {
  if (!drag.value) return;
  pointerX = event.clientX;
  pointerY = event.clientY;
  updateDrag();
}
function endSelection() {
  if (dragFrame !== undefined) cancelAnimationFrame(dragFrame);
  dragFrame = undefined;
  const id = drag.value?.pointerId;
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
  )
    loaded.value = Math.min(loaded.value + 36, props.items.length);
}
function resetScroll() {
  endSelection();
  cancelScrollFrame();
  loaded.value = 36;
  scrollTop.value = 0;
  viewport.value?.scrollTo({ top: 0 });
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
watch(() => props.items.map((item) => item.id).join(","), resetScroll);
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
  observer = new ResizeObserver(([entry]) => {
    if (entry) {
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
  endSelection();
  observer?.disconnect();
  cancelScrollFrame();
});
</script>

<template>
  <section class="collection-panel" aria-label="Media collection">
    <header class="collection-heading">
      <div>
        <h1>{{ heading }}</h1>
        <span>{{ items.length.toLocaleString() }} files</span>
      </div>
      <button
        v-if="stage && selectedIds?.length"
        class="secondary-button"
        @click="emit('distribute')"
      >
        Distribute {{ selectedIds.length }} files
      </button>
      <span v-else class="demo-badge"
        ><span></span> {{ stage ? "Demo staging" : "Demo library" }}</span
      >
    </header>
    <div
      ref="viewport"
      class="collection-scroll"
      tabindex="0"
      aria-label="Loaded media"
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
              stage
                ? Boolean(selectedIds?.includes(item.id))
                : selectedId === item.id
            "
            @select="$emit('select', item, $event)"
            @favorite="$emit('favorite', item)"
            @context="$emit('context', item, $event)"
          />
        </div>
      </div>
      <div v-if="drag" class="selection-rectangle" :style="selectionBox"></div>
      <div v-if="!items.length" class="empty-state">
        <span class="empty-icon"><AppIcon name="search" :size="32" /></span>
        <h2>
          {{
            filtered
              ? "No matching files"
              : stage
                ? "No staged files"
                : "No favorites yet"
          }}
        </h2>
        <p>
          {{
            filtered
              ? "Try another search or adjust your filters."
              : stage
                ? "Your staging files will appear here."
                : "Tap the heart on a file to keep it here."
          }}
        </p>
        <button
          v-if="filtered"
          class="secondary-button"
          @click="$emit('reset')"
        >
          Reset filters
        </button>
      </div>
    </div>
    <footer class="collection-footer">
      <span
        >{{ loadedCount.toLocaleString() }} of
        {{ items.length.toLocaleString() }} files loaded</span
      >
      <div class="load-track">
        <span
          :style="{
            width: `${items.length ? (loadedCount / items.length) * 100 : 0}%`,
          }"
        ></span>
      </div>
      <span v-if="loadedCount && loadedCount === items.length" class="load-done"
        ><AppIcon name="check" :size="14" /> All loaded</span
      >
    </footer>
  </section>
</template>

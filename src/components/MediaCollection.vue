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
  heading: string;
  filtered: boolean;
  stage?: boolean;
}>();
defineEmits<{
  select: [item: MediaItem];
  favorite: [item: MediaItem];
  options: [item: MediaItem];
  reset: [];
}>();
const viewport = ref<HTMLElement | null>(null);
const width = ref(660);
const height = ref(600);
const scrollTop = ref(0);
const loaded = ref(36);
const columns = computed(() => (props.mode === "grid" ? 3 : 1));
const rowHeight = computed(() =>
  props.mode === "grid" ? (((width.value - 32) / 3) * 9) / 16 + 104 : 88,
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
  cancelScrollFrame();
  loaded.value = 36;
  scrollTop.value = 0;
  viewport.value?.scrollTo({ top: 0 });
}
watch(() => props.items.map((item) => item.id).join(","), resetScroll);
watch(
  () => props.mode,
  async () => {
    cancelScrollFrame();
    scrollTop.value = 0;
    await nextTick();
    viewport.value?.scrollTo({ top: 0 });
  },
);
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    if (entry) {
      width.value = entry.contentRect.width;
      height.value = entry.contentRect.height;
    }
  });
  if (viewport.value) observer.observe(viewport.value);
});
onBeforeUnmount(() => {
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
      <span class="demo-badge"
        ><span></span> {{ stage ? "Demo staging" : "Demo library" }}</span
      >
    </header>
    <div
      ref="viewport"
      class="collection-scroll"
      tabindex="0"
      aria-label="Loaded media"
      @scroll.passive="onScroll"
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
            gridAutoRows: `${rowHeight - 16}px`,
          }"
        >
          <MediaCard
            v-for="item in visibleItems"
            :key="item.id"
            :item="item"
            :stage="stage"
            :show-previews="showPreviews"
            :mode="mode"
            :selected="selectedId === item.id"
            @select="$emit('select', item)"
            @favorite="$emit('favorite', item)"
            @options="$emit('options', item)"
          />
        </div>
      </div>
      <div v-else class="empty-state">
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

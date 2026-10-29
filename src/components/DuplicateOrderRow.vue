<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import MediaPreview from "./MediaPreview.vue";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  item: MediaItem;
  index: number;
  total: number;
  busy: boolean;
  showPreviews: boolean;
}>();

const emit = defineEmits<{ move: [offset: number] }>();

const row = ref<HTMLElement | null>(null);
const visible = ref(false);

const previewEnabled = computed(() => props.showPreviews && visible.value);

let observer: IntersectionObserver | undefined;

onMounted(() => {
  if (!row.value) return;

  observer = new IntersectionObserver(
    ([entry]) => {
      visible.value = Boolean(entry?.isIntersecting);
    },
    { root: row.value.closest(".order-list"), rootMargin: "96px" },
  );

  observer.observe(row.value);
});

onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <div ref="row" class="order-row">
    <span class="order-number">{{ index + 1 }}</span>
    <div class="order-preview">
      <MediaPreview
        :item="item"
        :show-previews="previewEnabled"
        variant="card"
      />
    </div>
    <span class="order-filename">{{ item.filename }}</span>
    <div class="order-row-actions">
      <button
        type="button"
        class="icon-button"
        :disabled="busy || index === 0"
        :aria-label="ui(`Move ${item.filename} up`)"
        :title="ui('Move up')"
        @click="emit('move', -1)"
      >
        <AppIcon name="up" :size="17" />
      </button>
      <button
        type="button"
        class="icon-button"
        :disabled="busy || index === total - 1"
        :aria-label="ui(`Move ${item.filename} down`)"
        :title="ui('Move down')"
        @click="emit('move', 1)"
      >
        <AppIcon name="down" :size="17" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.order-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px;
}
.order-number {
  flex: 0 0 26px;
  text-align: center;
  color: var(--accent);
  font-variant-numeric: tabular-nums;
}
.order-preview {
  flex: 0 0 112px;
}
.order-preview :deep(.card-preview) {
  width: 112px;
  height: 64px;
  border-radius: 6px;
}
.order-preview :deep(img) {
  object-fit: contain;
}
.order-filename {
  min-width: 0;
  flex: 1;
  overflow-wrap: anywhere;
  font-size: 12px;
}
.order-row-actions {
  display: flex;
  flex-shrink: 0;
  gap: 4px;
}
</style>

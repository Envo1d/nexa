<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import { formatDuration, formatSize } from "../types/media";
import MediaPreview from "./MediaPreview.vue";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const props = withDefaults(
  defineProps<{
    item: MediaItem;
    showPreviews: boolean;
    openLabel?: string;
    openDisabled?: boolean;
  }>(),
  { openLabel: "", openDisabled: false },
);

const emit = defineEmits<{ open: [] }>();

const element = ref<HTMLElement | null>(null);
const visible = ref(false);

const previewEnabled = computed(() => props.showPreviews && visible.value);

let observer: IntersectionObserver | undefined;

onMounted(() => {
  observer = new IntersectionObserver(([entry]) => {
    visible.value = Boolean(entry?.isIntersecting);
  });

  if (element.value) observer.observe(element.value);
});

onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <div ref="element" class="replacement-file">
    <div class="replacement-preview">
      <MediaPreview
        :item="item"
        :show-previews="previewEnabled"
        variant="card"
      />
    </div>
    <div class="replacement-description">
      <strong>{{ item.filename }}</strong>
      <span v-if="item.artist || item.videoTitle" class="muted">
        {{ [item.artist, item.videoTitle].filter(Boolean).join(" · ") }}
      </span>
      <span class="muted">
        {{
          ui(
            item.width && item.height
              ? `${item.width} × ${item.height}`
              : "Resolution unavailable",
          )
        }}
        · {{ ui(formatSize(item.size)) }} · {{ item.extension.toUpperCase() }}
        <template v-if="item.kind === 'video'">
          ·
          {{
            ui(
              item.duration == null
                ? "Duration unavailable"
                : formatDuration(item.duration, true),
            )
          }}
        </template>
      </span>
    </div>
    <button
      v-if="openLabel"
      type="button"
      class="secondary-button replacement-open"
      :aria-label="ui(openLabel)"
      :disabled="openDisabled"
      @click="emit('open')"
    >
      <AppIcon name="arrow" :size="14" />{{ ui("Open file") }}
    </button>
  </div>
</template>

<style scoped>
.replacement-file {
  display: flex;
  gap: 12px;
  align-items: center;
  min-width: 0;
}
.replacement-preview {
  width: 112px;
  flex-shrink: 0;
}
.replacement-preview :deep(.card-preview) {
  width: 112px;
  height: 72px;
  border-radius: 6px;
}
.replacement-preview :deep(img) {
  object-fit: contain;
}
.replacement-description {
  flex: 1;
  min-width: 0;
  display: grid;
  gap: 6px;
  text-align: left;
  overflow-wrap: anywhere;
}
.replacement-open {
  margin-left: auto;
  flex-shrink: 0;
}
.replacement-description span {
  font-size: 12px;
  line-height: 1.5;
}
</style>

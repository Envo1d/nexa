<script setup lang="ts">
import { computed, toRef } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import { useNativePreview } from "../composables/useNativePreview";
import { formatDuration } from "../types/media";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  item: MediaItem;
  showPreviews: boolean;
  variant: "card" | "detail" | "wizard";
}>();

const duration = computed(() => formatDuration(props.item.duration ?? 0));

const nativePreview = useNativePreview(
  toRef(props, "item"),
  toRef(props, "showPreviews"),
  props.variant !== "card",
);

const preview = computed(() => props.item.preview || nativePreview.value);
</script>

<template>
  <div :class="variant === 'wizard' ? 'wizard-preview' : `${variant}-preview`">
    <img
      v-if="showPreviews && preview"
      :src="preview"
      :alt="variant === 'wizard' ? ui('Source media preview') : item.filename"
      :loading="variant === 'card' ? 'lazy' : undefined"
      :decoding="variant === 'card' ? 'async' : undefined"
      :width="variant === 'card' ? 640 : undefined"
      :height="variant === 'card' ? 360 : undefined"
      :draggable="variant === 'card' ? false : undefined"
    />
    <AppIcon v-else class="preview-placeholder" :name="item.kind" :size="56" />
    <span v-if="variant === 'detail'" class="preview-label">{{
      ui(item.kind === "video" ? "Video preview" : "Image preview")
    }}</span>
    <span
      v-else
      :class="variant === 'wizard' ? 'wizard-extension' : 'extension'"
      >{{ item.extension }}</span
    >
    <span
      v-if="
        variant === 'wizard'
          ? Boolean(item.duration)
          : item.duration !== undefined
      "
      :class="variant === 'wizard' ? 'wizard-duration' : 'duration'"
      >{{ duration }}</span
    >
  </div>
</template>

<style scoped>
.preview-placeholder {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
}
</style>

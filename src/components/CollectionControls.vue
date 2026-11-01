<script setup lang="ts">
import { useI18n } from "vue-i18n";

import AppIcon from "./AppIcon.vue";
import SortSelect from "./SortSelect.vue";

import type { MediaKind } from "../types/media";

defineProps<{
  counts: { all: number; video: number; image: number };
  detailsOpen: boolean;
}>();

const kind = defineModel<"all" | MediaKind>("kind", { required: true });
const sort = defineModel<"newest" | "older" | "name-asc" | "name-desc">(
  "sort",
  { required: true },
);
const mode = defineModel<"grid" | "list">("mode", { required: true });

defineEmits<{ toggleDetails: [] }>();

const { t } = useI18n();

const types = [
  { value: "all", label: "allFiles", icon: "files" },
  { value: "video", label: "videos", icon: "video" },
  { value: "image", label: "images", icon: "image" },
] as const;
</script>

<template>
  <div class="collection-controls">
    <section class="collection-types" :aria-label="t('fileType')">
      <button
        v-for="option in types"
        :key="option.value"
        class="type-option"
        :class="{ active: kind === option.value }"
        :aria-pressed="kind === option.value"
        @click="kind = option.value"
      >
        <AppIcon :name="option.icon" :size="17" /><span>{{
          t(option.label)
        }}</span
        ><span class="type-count">{{ counts[option.value] }}</span>
      </button>
    </section>
    <SortSelect v-model="sort" />
    <div class="layout-toggle" :aria-label="t('layout')">
      <button
        :class="{ active: mode === 'grid' }"
        :aria-pressed="mode === 'grid'"
        :aria-label="t('grid')"
        @click="mode = 'grid'"
      >
        <AppIcon name="grid" :size="18" />
      </button>
      <button
        :class="{ active: mode === 'list' }"
        :aria-pressed="mode === 'list'"
        :aria-label="t('list')"
        @click="mode = 'list'"
      >
        <AppIcon name="list" :size="18" />
      </button>
    </div>
    <button
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
      @click="$emit('toggleDetails')"
    >
      <AppIcon name="info" :size="20" />
    </button>
  </div>
</template>

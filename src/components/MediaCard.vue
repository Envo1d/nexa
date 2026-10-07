<script setup lang="ts">
import { computed } from "vue";
import type { MediaItem } from "../types/media";
import { formatDate, formatDuration } from "../types/media";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{
  item: MediaItem;
  showPreviews: boolean;
  selected: boolean;
  mode: "grid" | "list";
}>();
const displayName = computed(() =>
  props.mode === "list"
    ? props.item.filename
    : props.item.filename.replace(/\.[^.]+$/, ""),
);
const modifiedDate = computed(() => formatDate(props.item.modifiedAt));
const modifiedDateTime = computed(() =>
  new Date(props.item.modifiedAt).toISOString(),
);
const duration = computed(() =>
  props.item.duration === undefined
    ? undefined
    : formatDuration(props.item.duration),
);
defineEmits<{ select: []; favorite: []; options: [] }>();
</script>

<template>
  <article class="media-card" :class="[mode, { selected }]">
    <button
      class="card-select"
      :aria-label="`View ${item.filename}`"
      :aria-pressed="selected"
      @click="$emit('select')"
    >
      <div class="card-preview">
        <img
          v-if="showPreviews"
          :src="item.preview"
          :alt="item.filename"
          loading="lazy"
          decoding="async"
          width="640"
          height="360"
          draggable="false"
        />
        <span class="extension">{{ item.extension }}</span>
        <span v-if="duration !== undefined" class="duration">{{
          duration
        }}</span>
      </div>
      <div class="card-body">
        <div class="card-title" :title="displayName">
          {{ displayName }}
        </div>
        <time :datetime="modifiedDateTime">{{ modifiedDate }}</time>
        <div class="card-tags">
          <span v-for="tag in item.tags" :key="tag" class="tag-chip">{{
            tag
          }}</span
          ><span v-if="!item.tags.length" class="no-tags">No tags</span>
        </div>
      </div>
    </button>
    <button
      class="card-favorite icon-button"
      :class="{ 'is-favorite': item.favorite }"
      :aria-label="`${item.favorite ? 'Remove' : 'Add'} ${item.filename} ${item.favorite ? 'from' : 'to'} favorites`"
      :aria-pressed="item.favorite"
      @click="$emit('favorite')"
    >
      <AppIcon name="heart" :size="17" />
    </button>
    <button
      class="card-options icon-button"
      :aria-label="`Options for ${item.filename}`"
      @click="$emit('options')"
    >
      <AppIcon name="more" :size="18" />
    </button>
  </article>
</template>

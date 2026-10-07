<script setup lang="ts">
import type { MediaItem } from "../types/media";
import { formatDate, formatDuration } from "../types/media";
import AppIcon from "./AppIcon.vue";
defineProps<{ item: MediaItem; selected: boolean; mode: "grid" | "list" }>();
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
          :src="item.preview"
          :alt="item.filename"
          loading="lazy"
          draggable="false"
        />
        <span class="extension">{{ item.extension }}</span>
        <span v-if="item.duration !== undefined" class="duration">{{
          formatDuration(item.duration)
        }}</span>
      </div>
      <div class="card-body">
        <div v-if="mode === 'list' || item.title" class="card-title">
          {{ mode === "list" ? item.filename : item.title }}
        </div>
        <time :datetime="new Date(item.modifiedAt).toISOString()">{{
          formatDate(item.modifiedAt)
        }}</time>
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

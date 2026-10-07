<script setup lang="ts">
import { computed } from "vue";
import type { MediaItem } from "../types/media";
import { formatDate, formatDuration, formatSize } from "../types/media";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{ item: MediaItem }>();
defineEmits<{ close: []; favorite: []; tags: []; action: [name: string] }>();
const metadata = computed(() => [
  {
    icon: "files",
    label: "File type",
    value: `${props.item.extension} (${props.item.kind === "video" ? "Video" : "Image"})`,
  },
  { icon: "files", label: "Size", value: formatSize(props.item.size) },
  {
    icon: "size",
    label: "Resolution",
    value: `${props.item.width} × ${props.item.height}`,
  },
  ...(props.item.kind === "video"
    ? [
        {
          icon: "clock",
          label: "Duration",
          value:
            props.item.duration !== undefined
              ? formatDuration(props.item.duration, true)
              : "Unavailable",
        },
        {
          icon: "film",
          label: "FPS",
          value: props.item.fps?.toString() ?? "Unavailable",
        },
      ]
    : []),
  {
    icon: "calendar",
    label: "Modified",
    value: formatDate(props.item.modifiedAt),
  },
  {
    icon: "calendar",
    label: "Created",
    value: formatDate(props.item.createdAt),
  },
  ...(props.item.kind === "video"
    ? [
        {
          icon: "film",
          label: "Codec",
          value: props.item.codec ?? "Unavailable",
        },
        {
          icon: "film",
          label: "Bitrate",
          value:
            props.item.bitrate !== undefined
              ? `${props.item.bitrate} Mbit/s`
              : "Unavailable",
        },
      ]
    : []),
]);
</script>

<template>
  <aside class="details-panel" aria-label="File information">
    <header class="panel-heading">
      <h2>File information</h2>
      <button
        class="icon-button"
        aria-label="Close file information"
        @click="$emit('close')"
      >
        <AppIcon name="close" :size="18" />
      </button>
    </header>
    <div class="details-scroll">
      <h3 class="detail-filename">{{ item.filename }}</h3>
      <div class="detail-preview">
        <img :src="item.preview" :alt="item.filename" /><span
          class="preview-label"
          >{{ item.kind === "video" ? "Video preview" : "Image preview" }}</span
        ><span v-if="item.duration !== undefined" class="duration">{{
          formatDuration(item.duration)
        }}</span>
      </div>
      <section class="detail-tags">
        <h3>Tags</h3>
        <div>
          <span v-for="tag in item.tags" :key="tag" class="tag-chip">{{
            tag
          }}</span
          ><span v-if="!item.tags.length" class="muted">No tags yet</span
          ><button
            class="tag-add icon-button"
            aria-label="Edit file tags"
            @click="$emit('tags')"
          >
            <AppIcon name="plus" :size="16" />
          </button>
        </div>
      </section>
      <dl class="metadata">
        <div v-for="field in metadata" :key="field.label">
          <dt><AppIcon :name="field.icon" :size="16" />{{ field.label }}</dt>
          <dd>{{ field.value }}</dd>
        </div>
      </dl>
      <div class="detail-actions">
        <button class="primary-button" @click="$emit('action', 'Open file')">
          <AppIcon name="arrow" :size="17" />Open file</button
        ><button
          class="secondary-button"
          @click="$emit('action', 'Open folder')"
        >
          <AppIcon name="gallery" :size="17" />Open folder</button
        ><button class="secondary-button" @click="$emit('action', 'Copy path')">
          <AppIcon name="copy" :size="16" />Copy path
        </button>
        <div class="action-row">
          <button
            class="secondary-button"
            :class="{ 'is-favorite': item.favorite }"
            :aria-label="
              item.favorite ? 'Remove from favorites' : 'Add to favorites'
            "
            :aria-pressed="item.favorite"
            @click="$emit('favorite')"
          >
            <AppIcon name="heart" :size="18" /></button
          ><button
            class="secondary-button"
            aria-label="Share file"
            @click="$emit('action', 'Share')"
          >
            <AppIcon name="share" :size="18" /></button
          ><button
            class="secondary-button"
            aria-label="Edit tags"
            @click="$emit('tags')"
          >
            <AppIcon name="tag" :size="18" /></button
          ><button
            class="secondary-button"
            aria-label="More file options"
            @click="$emit('action', 'File options')"
          >
            <AppIcon name="more" :size="18" />
          </button>
        </div>
      </div>
    </div>
  </aside>
</template>

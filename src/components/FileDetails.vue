<script setup lang="ts">
import { computed } from "vue";
import type { MediaItem } from "../types/media";
import { formatDate, formatDuration, formatSize } from "../types/media";
import MediaPreview from "./MediaPreview.vue";
import MetadataRow from "./MetadataRow.vue";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{
  item: MediaItem;
  showPreviews: boolean;
  stage?: boolean;
}>();
defineEmits<{
  close: [];
  favorite: [];
  tags: [];
  action: [name: string];
  search: [value: string];
}>();
const attribution = computed(() => [
  { label: "Copyright", category: "copyright", values: props.item.copyrights },
  {
    label: "Artist",
    category: "artist",
    values: props.item.artists ?? [props.item.artist],
  },
  {
    label: "Characters",
    category: "characters",
    values: props.item.characters,
  },
  { label: "Tags", category: "tags", values: props.item.tags },
]);
const metadata = computed(() => [
  {
    icon: "files",
    label: "File type",
    value: `${props.item.extension} (${props.item.kind === "video" ? "Video" : "Image"})`,
  },
  {
    icon: "files",
    label: "Size",
    value: props.item.size < 0 ? "Unavailable" : formatSize(props.item.size),
  },
  {
    icon: "size",
    label: "Resolution",
    value:
      props.item.width && props.item.height
        ? `${props.item.width} × ${props.item.height}`
        : "Unavailable",
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
    value:
      props.item.nativeRedistribution && !props.item.modifiedAt
        ? "Unavailable"
        : formatDate(props.item.modifiedAt),
  },
  {
    icon: "calendar",
    label: "Created",
    value:
      props.item.nativeRedistribution && !props.item.createdAt
        ? "Unavailable"
        : formatDate(props.item.createdAt),
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
      <p v-if="item.nativeRedistribution" class="muted">
        Needs redistribution · file remains at {{ item.path }}. Distribution is
        currently a preview.
      </p>
      <MediaPreview
        v-if="showPreviews"
        :item="item"
        :show-previews="showPreviews"
        variant="detail"
      />
      <div v-if="!stage" class="detail-attribution">
        <section
          v-for="group in attribution"
          :key="group.category"
          class="detail-tag-group"
          :class="`detail-${group.category}`"
        >
          <h3>{{ group.label }}</h3>
          <div>
            <button
              v-for="value in group.values"
              :key="value"
              type="button"
              class="tag-chip"
              @click="$emit('search', value)"
            >
              {{ value }}
            </button>
            <span v-if="!group.values.length" class="muted">{{
              group.category === "tags" ? "No tags yet" : "Unavailable"
            }}</span>
          </div>
        </section>
      </div>
      <dl class="metadata">
        <MetadataRow
          v-for="field in metadata"
          :key="field.label"
          :label="field.label"
          :icon="field.icon"
          :value="field.value"
        />
      </dl>
      <div class="detail-actions">
        <button class="primary-button" @click="$emit('action', 'Open file')">
          <AppIcon name="arrow" :size="17" />Open file</button
        ><button
          class="secondary-button"
          @click="$emit('action', 'Open folder')"
        >
          <AppIcon name="gallery" :size="17" />Open folder</button
        ><button
          v-if="stage"
          class="secondary-button"
          @click="$emit('action', 'Distribute')"
        >
          <AppIcon name="share" :size="17" />Distribute</button
        ><button
          v-else
          class="secondary-button"
          @click="$emit('action', 'Copy path')"
        >
          <AppIcon name="copy" :size="16" />Copy path
        </button>
        <div v-if="!stage" class="action-row">
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

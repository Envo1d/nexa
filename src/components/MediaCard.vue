<script setup lang="ts">
import { computed } from "vue";

import type { MediaItem } from "../types/media";

import { formatDate } from "../types/media";

import MediaPreview from "./MediaPreview.vue";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  item: MediaItem;
  showPreviews: boolean;
  selected: boolean;
  mode: "grid" | "list";
  stage?: boolean;
}>();

const displayName = computed(() =>
  props.stage
    ? props.mode === "list"
      ? props.item.filename
      : props.item.filename.replace(/\.[^.]+$/, "")
    : props.item.kind === "video"
      ? props.item.videoTitle?.trim()
      : props.item.characters.join(", "),
);

const modifiedDate = computed(() =>
  props.item.nativeId && !props.item.modifiedAt
    ? "Date unavailable"
    : formatDate(props.item.modifiedAt),
);

const showCharacterChips = computed(
  () => props.item.kind === "video" && props.item.characters.length > 0,
);

const metadataChips = computed(() =>
  showCharacterChips.value ? props.item.characters : props.item.tags,
);

const modifiedDateTime = computed(() =>
  props.item.nativeId && !props.item.modifiedAt
    ? undefined
    : new Date(props.item.modifiedAt).toISOString(),
);

defineEmits<{
  select: [event: MouseEvent];
  favorite: [];
  context: [event: MouseEvent | KeyboardEvent];
}>();
</script>

<template>
  <article
    class="media-card"
    :data-media-id="item.id"
    :class="[mode, { selected }]"
    @contextmenu.prevent.stop="$emit('context', $event)"
    @keydown.shift.f10.prevent.stop="$emit('context', $event)"
  >
    <button
      class="card-select"
      :aria-label="`View ${item.filename}`"
      :aria-pressed="selected"
      @click="$emit('select', $event)"
    >
      <MediaPreview :item="item" :show-previews="showPreviews" variant="card" />
      <div class="card-body">
        <div v-if="displayName" class="card-title" :title="displayName">
          {{ displayName }}
        </div>
        <time :datetime="modifiedDateTime">{{ modifiedDate }}</time>
        <div
          v-if="!stage"
          class="card-tags"
          :class="{ 'card-characters': showCharacterChips }"
        >
          <span
            v-for="(label, index) in metadataChips"
            :key="index"
            class="tag-chip"
            >{{ label }}</span
          ><span v-if="!metadataChips.length" class="no-tags">No tags</span>
        </div>
      </div>
    </button>
    <button
      v-if="!stage"
      class="card-favorite icon-button"
      :class="{ 'is-favorite': item.favorite }"
      :aria-label="`${item.favorite ? 'Remove' : 'Add'} ${item.filename} ${item.favorite ? 'from' : 'to'} favorites`"
      :aria-pressed="item.favorite"
      @click="$emit('favorite')"
    >
      <AppIcon name="heart" :size="17" />
    </button>
  </article>
</template>

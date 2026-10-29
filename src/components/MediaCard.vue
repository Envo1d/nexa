<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { formatDate } from "../types/media";
import MediaPreview from "./MediaPreview.vue";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const { t } = useI18n();

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
    ? t("dateUnavailable")
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
  open: [];
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
      :aria-label="t('selectFile', { filename: item.filename })"
      :aria-pressed="selected"
      @click="$emit('select', $event)"
      @dblclick="$emit('open')"
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
          ><span v-if="!metadataChips.length" class="no-tags">{{
            t("noTags")
          }}</span>
        </div>
      </div>
    </button>
    <button
      v-if="!stage"
      class="card-favorite icon-button"
      :class="{ 'is-favorite': item.favorite }"
      :aria-label="
        t(item.favorite ? 'favoriteFileRemove' : 'favoriteFileAdd', {
          filename: item.filename,
        })
      "
      :aria-pressed="item.favorite"
      @click="$emit('favorite')"
    >
      <AppIcon name="heart" :size="17" />
    </button>
  </article>
</template>

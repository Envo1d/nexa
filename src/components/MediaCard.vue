<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { formatDate, formatSize } from "../types/media";
import MediaPreview from "./MediaPreview.vue";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const { t, locale } = useI18n();

const props = defineProps<{
  item: MediaItem;
  showPreviews: boolean;
  selected: boolean;
  mode: "grid" | "list";
  stage?: boolean;
}>();

const displayName = computed(() =>
  props.stage
    ? props.item.filename
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

const compactDate = computed(() => {
  if (props.item.nativeId && !props.item.modifiedAt)
    return t("dateUnavailable");

  return new Intl.DateTimeFormat(locale.value, {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(props.item.modifiedAt));
});

const metadataSummary = computed(() => metadataChips.value.join(", "));

const stageFolder = computed(() => {
  const relative = props.item.relativePath?.replace(/\\/g, "/") ?? "";
  const separator = relative.lastIndexOf("/");

  return separator > 0 ? relative.slice(0, separator) : "";
});

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
    :class="[mode, { selected, 'stage-card': stage }]"
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
        <div :class="{ 'stage-card-heading': stage }">
          <div class="card-title" :title="displayName">
            {{ displayName }}
          </div>
          <div
            v-if="stage && stageFolder"
            class="card-folder"
            :title="stageFolder"
          >
            <AppIcon name="gallery" :size="12" /><span>{{ stageFolder }}</span>
          </div>
        </div>
        <div class="card-footer">
          <time :datetime="modifiedDateTime" :title="modifiedDate">{{
            mode === "grid" ? compactDate : modifiedDate
          }}</time>
          <div
            v-if="!stage"
            class="card-tags"
            :class="{ 'card-characters': showCharacterChips }"
            :title="metadataSummary || t('noTags')"
          >
            <AppIcon :name="showCharacterChips ? 'person' : 'tag'" :size="15" />
            <span>{{ metadataSummary || t("noTags") }}</span>
          </div>
          <span v-else class="card-size">{{
            item.size < 0 ? t("unavailable") : formatSize(item.size)
          }}</span>
        </div>
      </div>
    </button>
    <button
      class="card-more icon-button"
      :aria-label="t('cardActions', { filename: item.filename })"
      aria-haspopup="menu"
      @click="$emit('context', $event)"
      @dblclick.stop
    >
      <AppIcon name="more" :size="17" />
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

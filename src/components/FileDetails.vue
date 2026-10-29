<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { translateUi as ui } from "../i18n/presentation";
import { formatDate, formatDuration, formatSize } from "../types/media";
import MediaPreview from "./MediaPreview.vue";
import MetadataRow from "./MetadataRow.vue";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const { t, locale } = useI18n();

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
  {
    label: t("copyright"),
    category: "copyright",
    values: props.item.copyrights,
  },
  {
    label: t("artist"),
    category: "artist",
    values:
      props.item.artists ?? (props.item.artist ? [props.item.artist] : []),
  },
  {
    label: t("characters"),
    category: "characters",
    values: props.item.characters,
  },
  {
    label: t("tags"),
    category: "tags",
    values: props.item.tags,
  },
]);

const metadata = computed(() => [
  {
    icon: "files",
    label: t("fileType"),
    value: `${props.item.extension} (${props.item.kind === "video" ? t("video") : t("image")})`,
  },
  {
    icon: "files",
    label: t("size"),
    value: props.item.size < 0 ? t("unavailable") : formatSize(props.item.size),
  },
  {
    icon: "size",
    label: t("resolution"),
    value:
      props.item.width && props.item.height
        ? `${props.item.width} × ${props.item.height}`
        : t("unavailable"),
  },
  ...(props.item.kind === "video"
    ? [
        {
          icon: "clock",
          label: t("duration"),
          value:
            props.item.duration !== undefined
              ? formatDuration(props.item.duration, true)
              : t("unavailable"),
        },
        {
          icon: "film",
          label: "FPS",
          value:
            props.item.fps?.toLocaleString(locale.value, {
              maximumFractionDigits: 3,
            }) ?? t("unavailable"),
        },
      ]
    : []),
  {
    icon: "calendar",
    label: t("modified"),
    value:
      props.item.nativeId && !props.item.modifiedAt
        ? t("unavailable")
        : formatDate(props.item.modifiedAt),
  },
  {
    icon: "calendar",
    label: t("created"),
    value:
      props.item.nativeId && !props.item.createdAt
        ? t("unavailable")
        : formatDate(props.item.createdAt),
  },
  ...(props.item.kind === "video"
    ? [
        {
          icon: "film",
          label: t("codec"),
          value: props.item.codec ?? t("unavailable"),
        },
        {
          icon: "film",
          label: t("bitrate"),
          value:
            props.item.bitrate !== undefined
              ? `${props.item.bitrate.toFixed(2)} Mbit/s`
              : t("unavailable"),
        },
      ]
    : []),
]);
</script>

<template>
  <aside class="details-panel" :aria-label="t('fileInformation')">
    <header class="panel-heading">
      <h2>{{ t("fileInformation") }}</h2>
      <button
        class="icon-button"
        :aria-label="t('closeFileInformation')"
        @click="$emit('close')"
      >
        <AppIcon name="close" :size="18" />
      </button>
    </header>
    <div class="details-scroll">
      <h3 class="detail-filename">{{ item.filename }}</h3>
      <p v-if="item.nativeRedistribution" class="muted">
        {{ ui("Needs redistribution · file remains at") }} {{ item.path
        }}{{ ui(". Distribution is currently a preview.") }}
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
          <h3>{{ ui(group.label) }}</h3>
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
              group.category === "tags" ? t("noTagsYet") : t("unavailable")
            }}</span>
          </div>
        </section>
      </div>
      <dl class="metadata">
        <MetadataRow
          v-for="field in metadata"
          :key="field.label"
          :label="ui(field.label)"
          :icon="field.icon"
          :value="ui(field.value)"
        />
      </dl>
      <div class="detail-actions">
        <button class="primary-button" @click="$emit('action', 'Open file')">
          <AppIcon name="arrow" :size="17" />{{ t("openFile") }}</button
        ><button
          class="secondary-button"
          @click="$emit('action', 'Open folder')"
        >
          <AppIcon name="gallery" :size="17" />{{ t("openFolder") }}</button
        ><button
          v-if="stage"
          class="secondary-button"
          @click="$emit('action', 'Distribute')"
        >
          <AppIcon name="share" :size="17" />{{ t("distribute") }}</button
        ><button
          v-else
          class="secondary-button"
          @click="$emit('action', 'Copy path')"
        >
          <AppIcon name="copy" :size="16" />{{ t("copyPath") }}
        </button>
        <div v-if="!stage" class="action-row">
          <button
            class="secondary-button"
            :class="{ 'is-favorite': item.favorite }"
            :aria-label="item.favorite ? t('removeFavorite') : t('addFavorite')"
            :aria-pressed="item.favorite"
            @click="$emit('favorite')"
          >
            <AppIcon name="heart" :size="18" /></button
          ><button
            class="secondary-button"
            :aria-label="t('shareFile')"
            @click="$emit('action', 'Share')"
          >
            <AppIcon name="share" :size="18" /></button
          ><button
            class="secondary-button"
            :aria-label="t('editTags')"
            @click="$emit('tags')"
          >
            <AppIcon name="tag" :size="18" /></button
          ><button
            class="secondary-button"
            :aria-label="t('moreOptions')"
            @click="$emit('action', 'File options')"
          >
            <AppIcon name="more" :size="18" />
          </button>
        </div>
      </div>
    </div>
  </aside>
</template>

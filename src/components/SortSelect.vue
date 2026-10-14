<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import AppSelect from "./AppSelect.vue";

type SortMode = "newest" | "older" | "name-asc" | "name-desc";

const model = defineModel<SortMode>({ required: true });

const { t } = useI18n();

const options = computed<{ value: SortMode; label: string }[]>(() => [
  { value: "newest", label: t("newest") },
  { value: "older", label: t("older") },
  { value: "name-asc", label: t("nameAsc") },
  { value: "name-desc", label: t("nameDesc") },
]);
</script>

<template>
  <AppSelect v-model="model" :options="options" :label="t('sorting')" />
</template>

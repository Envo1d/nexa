<script setup lang="ts">
import AppIcon from "./AppIcon.vue";
import { translateUi as ui } from "../i18n/presentation";

defineProps<{ label: string; icon?: string; value?: string | number }>();
</script>

<template>
  <div>
    <dt><AppIcon v-if="icon" :name="icon" :size="16" />{{ ui(label) }}</dt>
    <dd>
      <slot>{{ value }}</slot>
    </dd>
  </div>
</template>

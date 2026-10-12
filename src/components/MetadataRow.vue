<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

defineProps<{ label: string; icon?: string; value?: string | number }>();
</script>

<template>
  <div>
    <dt><AppIcon v-if="icon" :name="icon" :size="16" />{{ label }}</dt>
    <dd>
      <slot>{{ value }}</slot>
    </dd>
  </div>
</template>

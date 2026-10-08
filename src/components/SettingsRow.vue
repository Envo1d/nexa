<script setup lang="ts">
withDefaults(
  defineProps<{ title: string; description: string; as?: "div" | "label" }>(),
  { as: "div" },
);
</script>

<template>
  <component :is="as" class="settings-card-row">
    <span
      ><strong>{{ title }}</strong
      ><small>{{ description }}</small></span
    >
    <slot />
  </component>
</template>

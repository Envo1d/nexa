<script setup lang="ts">
withDefaults(defineProps<{ trailing?: boolean }>(), { trailing: false });

const model = defineModel<boolean>({ required: true });
</script>

<template>
  <label>
    <slot v-if="trailing" />
    <input v-model="model" type="checkbox" role="switch" />
    <slot v-if="!trailing" />
  </label>
</template>

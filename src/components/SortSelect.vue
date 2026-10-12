<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
} from "vue";

import AppIcon from "./AppIcon.vue";

type SortMode = "newest" | "older" | "name-asc" | "name-desc";

const model = defineModel<SortMode>({ required: true });

const options: { value: SortMode; label: string }[] = [
  { value: "newest", label: "Newest first" },
  { value: "older", label: "Older first" },
  { value: "name-asc", label: "Name A → Z" },
  { value: "name-desc", label: "Name Z → A" },
];

const id = useId();

const root = ref<HTMLElement | null>(null);

const trigger = ref<HTMLButtonElement | null>(null);

const open = ref(false);

const active = ref(0);

const label = computed(
  () => options.find((option) => option.value === model.value)?.label,
);

async function show(
  index = options.findIndex((option) => option.value === model.value),
) {
  active.value = index;
  open.value = true;
  await nextTick();
  root.value?.querySelector<HTMLElement>('[role="listbox"]')?.focus();
}

function close() {
  open.value = false;
  trigger.value?.focus();
}

function choose(index: number) {
  model.value = options[index]!.value;
  close();
}

function onKey(event: KeyboardEvent) {
  switch (event.key) {
    case "ArrowDown":
    case "ArrowUp":
      event.preventDefault();
      active.value =
        (active.value + (event.key === "ArrowDown" ? 1 : -1) + options.length) %
        options.length;
      break;
    case "Home":
    case "End":
      event.preventDefault();
      active.value = event.key === "Home" ? 0 : options.length - 1;
      break;
    case "Enter":
    case " ":
      event.preventDefault();
      choose(active.value);
      break;
    case "Escape":
      event.preventDefault();
      event.stopPropagation();
      close();
      break;
    case "Tab":
      close();
      break;
    default: {
      const index = options.findIndex((option) =>
        option.label.toLowerCase().startsWith(event.key.toLowerCase()),
      );

      if (event.key.length === 1 && index >= 0) {
        event.preventDefault();
        active.value = index;
      }
    }
  }
}

function dismiss(event: Event) {
  if (event.target instanceof Node && !root.value?.contains(event.target))
    open.value = false;
}

onMounted(() => document.addEventListener("pointerdown", dismiss));

onBeforeUnmount(() => document.removeEventListener("pointerdown", dismiss));
</script>

<template>
  <div ref="root" class="sort-select" @focusout="dismiss">
    <button
      :id="`${id}-trigger`"
      ref="trigger"
      class="sort-select-trigger"
      aria-label="Sorting"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="`${id}-options`"
      @click="open ? close() : show()"
      @keydown.down.prevent="show()"
      @keydown.up.prevent="show()"
    >
      <span>{{ label }}</span
      ><AppIcon name="down" :size="15" />
    </button>
    <div
      v-if="open"
      :id="`${id}-options`"
      class="sort-select-options"
      role="listbox"
      aria-label="Sorting"
      tabindex="-1"
      :aria-activedescendant="`${id}-option-${active}`"
      @keydown="onKey"
      @focusout="open = false"
    >
      <div
        v-for="(option, index) in options"
        :id="`${id}-option-${index}`"
        :key="option.value"
        role="option"
        :aria-selected="model === option.value"
        class="sort-select-option"
        :class="{ active: active === index, selected: model === option.value }"
        @pointermove="active = index"
        @mousedown.prevent
        @click="choose(index)"
      >
        <span>{{ option.label }}</span>
        <AppIcon v-if="model === option.value" name="check" :size="14" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts" generic="T extends string">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
  watch,
} from "vue";

import AppIcon from "./AppIcon.vue";

const model = defineModel<T>({ required: true });

const props = defineProps<{
  options: readonly { value: T; label: string }[];
  label: string;
  disabled?: boolean;
}>();

const id = useId();

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const open = ref(false);
const active = ref(0);

const label = computed(
  () => props.options.find((option) => option.value === model.value)?.label,
);

async function show(
  index = props.options.findIndex((option) => option.value === model.value),
) {
  if (props.disabled || !props.options.length) return;

  active.value = Math.max(0, index);
  open.value = true;

  await nextTick();

  if (!open.value || props.disabled) return;

  root.value?.querySelector<HTMLElement>('[role="listbox"]')?.focus();
}

function close() {
  open.value = false;
  trigger.value?.focus();
}

function choose(index: number) {
  const option = props.options[index];

  if (props.disabled || !option) return;

  model.value = option.value;
  close();
}

function onKey(event: KeyboardEvent) {
  if (event.isComposing || props.disabled || !props.options.length) return;

  switch (event.key) {
    case "ArrowDown":
    case "ArrowUp":
      event.preventDefault();
      active.value =
        (active.value +
          (event.key === "ArrowDown" ? 1 : -1) +
          props.options.length) %
        props.options.length;
      break;

    case "Home":
    case "End":
      event.preventDefault();
      active.value = event.key === "Home" ? 0 : props.options.length - 1;
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
      open.value = false;
      break;

    default: {
      const index = props.options.findIndex((option) =>
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

function dismissFocus(event: FocusEvent) {
  const nextTarget = event.relatedTarget;

  if (!(nextTarget instanceof Node) || !root.value?.contains(nextTarget)) {
    open.value = false;
  }
}

onMounted(() => document.addEventListener("pointerdown", dismiss));

onBeforeUnmount(() => document.removeEventListener("pointerdown", dismiss));

watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) open.value = false;
  },
);

watch(
  () => props.options,
  (options) => {
    if (!options.length) open.value = false;

    active.value = Math.max(0, Math.min(active.value, options.length - 1));
  },
);
</script>

<template>
  <div ref="root" class="sort-select" @focusout="dismissFocus">
    <button
      :id="`${id}-trigger`"
      ref="trigger"
      type="button"
      class="sort-select-trigger"
      :aria-label="props.label"
      :disabled="disabled"
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
      :aria-label="props.label"
      tabindex="-1"
      :aria-activedescendant="`${id}-option-${active}`"
      @keydown="onKey"
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

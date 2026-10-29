<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";

import { translateUi as ui } from "../i18n/presentation";
import AppIcon from "./AppIcon.vue";

import type { MediaItem } from "../types/media";

const props = defineProps<{
  item: MediaItem;
  stage: boolean;
  selectedCount?: number;
  x: number;
  y: number;
}>();

const emit = defineEmits<{ close: []; action: [action: string] }>();

const menu = ref<HTMLElement | null>(null);
const left = ref(props.x);
const top = ref(props.y);

let trigger: HTMLElement | null = null;

const actions = computed<{ label: string; action?: string; icon: string }[]>(
  () => [
    { label: props.stage ? "Open" : "Open file", icon: "arrow" },
    { label: "Show in Explorer", icon: "gallery" },
    {
      label: props.stage
        ? (props.selectedCount ?? 1) > 1
          ? `Distribute ${props.selectedCount} files`
          : "Distribute"
        : "Edit metadata",
      action: props.stage ? "Distribute" : "Edit metadata",
      icon: props.stage ? "share" : "tag",
    },
    ...(!props.stage && props.item.kind === "video"
      ? [{ label: "Copy filename", icon: "copy" }]
      : []),
    ...(props.stage && props.item.nativeId && (props.selectedCount ?? 1) === 1
      ? [
          {
            label: "Replace in Main Library…",
            action: "Replace in Main Library",
            icon: "sort",
          },
        ]
      : []),
    ...(!props.stage && props.item.nativeId
      ? [{ label: "Change file order", icon: "sort" }]
      : []),
    {
      label:
        (props.selectedCount ?? 1) > 1
          ? `Delete ${props.selectedCount} files`
          : "Delete file",
      action: "Delete file",
      icon: "trash",
    },
  ],
);

async function position() {
  await nextTick();

  if (!menu.value) return;

  left.value = Math.max(
    8,
    Math.min(props.x, window.innerWidth - menu.value.offsetWidth - 8),
  );
  top.value = Math.max(
    8,
    Math.min(props.y, window.innerHeight - menu.value.offsetHeight - 8),
  );
  menu.value.querySelector<HTMLButtonElement>("button")?.focus();
}

function outside(event: PointerEvent) {
  if (!menu.value?.contains(event.target as Node)) emit("close");
}

function dismiss() {
  emit("close");
}

function keyboard(event: KeyboardEvent) {
  const buttons = [
    ...menu.value!.querySelectorAll<HTMLButtonElement>("button"),
  ];

  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);

  if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    event.preventDefault();

    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? buttons.length - 1
          : (index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) %
            buttons.length;

    buttons[next]?.focus();
  } else if (event.key === "Escape" || event.key === "Tab") {
    if (event.key === "Escape") event.preventDefault();

    event.stopPropagation();
    emit("close");
  }
}

watch(() => [props.x, props.y, props.item.id], position);

onMounted(() => {
  trigger = document.activeElement as HTMLElement;
  void position();
  document.addEventListener("pointerdown", outside);
  document.addEventListener("scroll", dismiss, true);
  window.addEventListener("resize", dismiss);
  window.addEventListener("blur", dismiss);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", outside);
  document.removeEventListener("scroll", dismiss, true);
  window.removeEventListener("resize", dismiss);
  window.removeEventListener("blur", dismiss);

  if (menu.value?.contains(document.activeElement)) trigger?.focus();
});
</script>
<template>
  <Teleport to="body">
    <div
      ref="menu"
      class="card-context-menu"
      role="menu"
      :aria-label="ui(`Actions for ${item.filename}`)"
      :style="{ left: `${left}px`, top: `${top}px` }"
      @keydown="keyboard"
      @contextmenu.prevent
    >
      <button
        v-for="action in actions"
        :key="action.label"
        role="menuitem"
        tabindex="-1"
        :class="{ destructive: action.action === 'Delete file' }"
        @click="$emit('action', action.action ?? action.label)"
      >
        <AppIcon :name="action.icon" :size="16" />{{ ui(action.label) }}
      </button>
    </div>
  </Teleport>
</template>

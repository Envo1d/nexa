<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import AppIcon from "./AppIcon.vue";
import { translateUi as ui } from "../i18n/presentation";

import type { ToastMessage } from "../shared/composables/useToast";
import type { ScanStatus } from "../composables/useStageScan";

defineProps<{
  toasts: ToastMessage[];
  indexing: ScanStatus;
  diskSync: {
    running: boolean;
    completed: number;
    total: number;
    error: string | null;
  };
}>();

const emit = defineEmits<{
  dismiss: [id: string];
  action: [toast: ToastMessage];
  pause: [paused: boolean];
  retry: [];
  indexing: [];
}>();

const { t } = useI18n();

const hovered = ref(false);
const focused = ref(false);

function setInteraction(kind: "hover" | "focus", value: boolean) {
  if (kind === "hover") hovered.value = value;
  else focused.value = value;

  emit("pause", hovered.value || focused.value);
}

function focusOut(event: FocusEvent) {
  const region = event.currentTarget as HTMLElement;

  if (!region.contains(event.relatedTarget as Node | null))
    setInteraction("focus", false);
}
</script>

<template>
  <section
    class="notification-stack"
    :aria-label="t('notifications')"
    @pointerenter="setInteraction('hover', true)"
    @pointerleave="setInteraction('hover', false)"
    @focusin="setInteraction('focus', true)"
    @focusout="focusOut"
  >
    <TransitionGroup name="toast" tag="div" class="toast-list">
      <div
        v-if="indexing.running"
        key="file-indexing"
        class="toast-card toast-operation"
      >
        <AppIcon class="toast-icon toast-spinner" name="loader" :size="18" />
        <div class="toast-content">
          <p class="toast-title" role="status">{{ t("indexingFiles") }}</p>
          <p class="toast-description">{{ ui(indexing.message) }}</p>
          <p class="toast-description">
            {{
              t("scanCounts", {
                scanned: indexing.scanned,
                extracted: indexing.extracted,
                errors: indexing.errors,
              })
            }}
          </p>
          <progress :aria-label="t('indexingFiles')"></progress>
        </div>
        <button class="toast-action" @click="emit('indexing')">
          {{ t("indexingDetails") }}
        </button>
      </div>
      <div
        v-if="diskSync.running || diskSync.error"
        key="disk-sync"
        class="toast-card toast-operation"
        :class="{ 'toast-error': diskSync.error }"
      >
        <AppIcon
          class="toast-icon"
          :name="diskSync.error ? 'warning' : 'loader'"
          :size="18"
          :class="{ 'toast-spinner': diskSync.running && !diskSync.error }"
        />
        <div class="toast-content">
          <p class="toast-title" role="status">
            {{ t(diskSync.error ? "syncFailedTitle" : "syncTitle") }}
          </p>
          <p class="toast-description">
            {{
              diskSync.error
                ? ui(diskSync.error)
                : diskSync.total
                  ? t("syncCount", diskSync)
                  : t("preparingSync")
            }}
          </p>
          <progress
            :aria-label="t('syncProgress')"
            :max="Math.max(1, diskSync.total)"
            :value="
              diskSync.total || diskSync.error
                ? Math.min(diskSync.completed, Math.max(1, diskSync.total))
                : undefined
            "
          ></progress>
        </div>
        <button
          v-if="diskSync.error"
          class="toast-action"
          @click="emit('retry')"
        >
          {{ t("retry") }}
        </button>
      </div>
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="toast-card"
        :class="`toast-${toast.kind ?? 'info'}`"
      >
        <AppIcon
          class="toast-icon"
          :name="
            toast.kind === 'success'
              ? 'check-circle'
              : toast.kind === 'error'
                ? 'warning'
                : toast.kind === 'update'
                  ? 'download'
                  : 'info'
          "
          :size="18"
        />
        <div class="toast-content">
          <p class="toast-title" role="status">{{ ui(toast.message) }}</p>
          <p v-if="toast.description" class="toast-description">
            {{ ui(toast.description) }}
          </p>
        </div>
        <button
          v-if="toast.action"
          class="toast-action"
          @click="emit('action', toast)"
        >
          {{ ui(toast.action.label) }}
        </button>
        <button
          class="toast-dismiss"
          :aria-label="t('dismissMessage')"
          @click="emit('dismiss', toast.id)"
        >
          <AppIcon name="close" :size="14" />
        </button>
      </div>
    </TransitionGroup>
  </section>
</template>

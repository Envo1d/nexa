<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, onUnmounted, ref } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import { applyLocale, i18n } from "../i18n";
import { applyTheme, themePreference } from "../themes";
import AppIcon from "./AppIcon.vue";

import type { InterfaceLocale } from "../types/config";
import type { AppTheme } from "../themes";

interface Snapshot {
  phase: "preparing" | "verifying" | "installing" | "restarting" | "error";
  version: string;
  message: string;
  locale: InterfaceLocale;
  theme: AppTheme;
}

const state = ref<Snapshot>({
  phase: "preparing",
  version: "",
  message: "Preparing your update…",
  locale: "en",
  theme: "midnight-violet",
});
const connectionLost = ref(false);

const title = computed(() => {
  if (connectionLost.value) return "Waiting for the updater";

  const titles = {
    preparing: "Getting ready",
    verifying: "Verifying update",
    installing: "Updating Nexa",
    restarting: "Ready to go",
    error: "Update needs attention",
  };

  return titles[state.value.phase];
});

const steps = ["Verify", "Install", "Open Nexa"];

const step = computed(() => {
  if (state.value.phase === "restarting") return 2;

  return state.value.phase === "installing" ? 1 : 0;
});

let timer: ReturnType<typeof setTimeout> | undefined;
let disposed = false;

async function refresh() {
  try {
    state.value = await invoke<Snapshot>("update_snapshot");

    if (themePreference.value !== state.value.theme) {
      applyTheme(state.value.theme);
    }

    if (state.value.locale && i18n.global.locale.value !== state.value.locale) {
      await applyLocale(state.value.locale);
    }

    connectionLost.value = false;
  } catch {
    connectionLost.value = true;
  }

  if (!disposed) timer = setTimeout(refresh, 300);
}

async function close() {
  await invoke("close_update_window");
}

onMounted(async () => {
  await refresh();

  try {
    await invoke("update_ui_ready");
  } catch {
    connectionLost.value = true;
  }
});

onUnmounted(() => {
  disposed = true;
  clearTimeout(timer);
});
</script>

<template>
  <main class="update-window" :class="{ failed: state.phase === 'error' }">
    <header data-tauri-drag-region>
      <span data-tauri-drag-region>Nexa</span>
      <button
        v-if="state.phase === 'error'"
        type="button"
        :aria-label="ui('Close updater')"
        @click="close"
      >
        <AppIcon name="close" :size="16" />
      </button>
    </header>

    <div class="emblem" aria-hidden="true">
      <img src="/icon.png" alt="" draggable="false" />
      <span v-if="state.phase === 'error'" class="error-mark">
        <AppIcon name="warning" :size="16" />
      </span>
    </div>

    <section aria-live="polite" aria-atomic="true">
      <p class="version">
        {{ ui(state.version ? `Version ${state.version}` : "Updating") }}
      </p>
      <h1>{{ ui(title) }}</h1>
      <p class="message">
        {{
          ui(connectionLost ? "Reconnecting to the updater…" : state.message)
        }}
      </p>
    </section>

    <template v-if="state.phase !== 'error'">
      <div
        class="activity"
        role="progressbar"
        :aria-label="ui('Update in progress')"
      >
        <span></span>
      </div>
      <ol class="steps" :aria-label="ui('Update stages')">
        <li
          v-for="(label, index) in steps"
          :key="label"
          :class="{ active: index === step, complete: index < step }"
          :aria-current="index === step ? 'step' : undefined"
        >
          <span>
            <AppIcon v-if="index < step" name="check" :size="12" />
            <template v-else>{{ index + 1 }}</template> </span
          >{{ ui(label) }}
        </li>
      </ol>
      <p class="footnote">{{ ui("Nexa will reopen automatically.") }}</p>
    </template>
    <button v-else type="button" class="close-button" @click="close">
      {{ ui("Close") }}
    </button>
  </main>
</template>

<style>
:root {
  font-family: "Segoe UI", Inter, sans-serif;
  color: var(--text);
  background: var(--shell);
  font-synthesis: none;
  -webkit-font-smoothing: antialiased;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  overflow: hidden;
  user-select: none;
}

button {
  font: inherit;
  color: inherit;
  cursor: pointer;
}

button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 3px;
}

.update-window {
  min-height: 440px;
  max-height: 100dvh;
  overflow-y: auto;
  padding: 0 32px 24px;
  text-align: center;
  background: var(--shell);
  border: 1px solid var(--line);
}

.update-window header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 44px;
  margin: 0 -16px;
  font-size: 12px;
  font-weight: 500;
  color: var(--muted);
}

.update-window header button {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 6px;
  background: var(--hover-overlay);
  font-size: 22px;
}

.emblem {
  position: relative;
  display: grid;
  place-items: center;
  width: 132px;
  height: 132px;
  margin: 16px auto 20px;
}

.emblem img {
  width: 68px;
  height: 68px;
  object-fit: contain;
}

.version {
  margin: 0 0 8px;
  color: var(--muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

h1 {
  margin: 0 0 8px;
  font-size: 24px;
  font-weight: 600;
}

.message {
  min-height: 38px;
  margin: 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
}

.activity {
  height: 4px;
  margin: 24px 0 16px;
  overflow: hidden;
  border-radius: 4px;
  background: var(--surface-pressed);
}

.activity span {
  display: block;
  width: 38%;
  height: 100%;
  border-radius: inherit;
  background: var(--accent);
  animation: sweep 2s ease-in-out infinite;
}

.steps {
  display: flex;
  justify-content: space-between;
  padding: 0;
  margin: 0;
  list-style: none;
  color: var(--subtle);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.steps li {
  display: flex;
  gap: 8px;
  align-items: center;
}

.steps li span {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border: 1px solid currentColor;
  border-radius: 50%;
  font-size: 11px;
}

.steps .active,
.steps .complete {
  color: var(--text-accent);
}

.footnote {
  margin: 20px 0 0;
  color: var(--subtle);
  font-size: 12px;
}

.error-mark {
  position: absolute;
  display: grid;
  place-items: center;
  right: 24px;
  bottom: 24px;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--warning);
  color: var(--warning-bg);
}

.close-button {
  min-width: 112px;
  min-height: 36px;
  margin-top: 24px;
  padding: 8px 16px;
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  background: var(--surface-raised);
  font-size: 12px;
}

.close-button:hover {
  background: var(--surface-hover);
}

@keyframes sweep {
  from {
    transform: translateX(-100%);
  }
  to {
    transform: translateX(365%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .activity span {
    animation: none;
  }
}
</style>

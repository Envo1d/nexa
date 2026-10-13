<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, onUnmounted, ref } from "vue";

interface Snapshot {
  phase: "preparing" | "verifying" | "installing" | "restarting" | "error";
  version: string;
  message: string;
}

const state = ref<Snapshot>({
  phase: "preparing",
  version: "",
  message: "Preparing your update…",
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
      <span data-tauri-drag-region>NEXA</span>
      <button
        v-if="state.phase === 'error'"
        type="button"
        aria-label="Close updater"
        @click="close"
      >
        ×
      </button>
    </header>

    <div class="emblem" aria-hidden="true">
      <div class="orbit orbit-one"></div>
      <div class="orbit orbit-two"></div>
      <img src="/icon.png" alt="" draggable="false" />
      <span v-if="state.phase === 'error'" class="error-mark">!</span>
    </div>

    <section aria-live="polite" aria-atomic="true">
      <p class="version">
        {{ state.version ? `VERSION ${state.version}` : "UPDATING" }}
      </p>
      <h1>{{ title }}</h1>
      <p class="message">
        {{ connectionLost ? "Reconnecting to the updater…" : state.message }}
      </p>
    </section>

    <template v-if="state.phase !== 'error'">
      <div class="activity" role="progressbar" aria-label="Update in progress">
        <span></span>
      </div>
      <ol class="steps" aria-label="Update stages">
        <li
          v-for="(label, index) in steps"
          :key="label"
          :class="{ active: index === step, complete: index < step }"
          :aria-current="index === step ? 'step' : undefined"
        >
          <span>{{ index < step ? "✓" : index + 1 }}</span
          >{{ label }}
        </li>
      </ol>
      <p class="footnote">Nexa will reopen automatically.</p>
    </template>
    <button v-else type="button" class="close-button" @click="close">
      Close
    </button>
  </main>
</template>

<style>
:root {
  font-family: "Segoe UI", Inter, sans-serif;
  color: #eeedf6;
  background: #10101b;
  color-scheme: dark;
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
  outline: 2px solid #bda7ff;
  outline-offset: 3px;
}

.update-window {
  min-height: 440px;
  padding: 0 32px 24px;
  text-align: center;
  background: radial-gradient(ellipse at 50% 36%, #26203980, transparent 62%);
  border: 1px solid #302a40;
}

.update-window header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 44px;
  margin: 0 -16px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 3px;
  color: #92899e;
}

.update-window header button {
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 6px;
  background: #ffffff08;
  font-size: 22px;
}

.emblem {
  position: relative;
  display: grid;
  place-items: center;
  width: 132px;
  height: 132px;
  margin: 16px auto 22px;
}

.emblem img {
  width: 68px;
  height: 68px;
  object-fit: contain;
  filter: drop-shadow(0 8px 24px #9c72ff40);
  animation: breathe 3s ease-in-out infinite;
}

.orbit {
  position: absolute;
  inset: 0;
  border: 1px solid #a786ff25;
  border-top-color: #a786ff;
  border-radius: 50%;
  animation: orbit 3s linear infinite;
}

.orbit-two {
  inset: 10px;
  border-top-color: #a786ff25;
  border-bottom-color: #d4bfff;
  animation-direction: reverse;
  animation-duration: 4s;
}

.version {
  margin: 0 0 10px;
  color: #a786ff;
  font-size: 10px;
  letter-spacing: 2px;
}

h1 {
  margin: 0 0 10px;
  font-size: 23px;
  font-weight: 600;
  letter-spacing: -0.5px;
}

.message {
  min-height: 38px;
  margin: 0;
  color: #aaa3ba;
  font-size: 12px;
  line-height: 1.6;
}

.activity {
  height: 3px;
  margin: 22px 0 18px;
  overflow: hidden;
  border-radius: 4px;
  background: #292235;
}

.activity span {
  display: block;
  width: 38%;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, transparent, #bda7ff, transparent);
  animation: sweep 2s ease-in-out infinite;
}

.steps {
  display: flex;
  justify-content: space-between;
  padding: 0;
  margin: 0;
  list-style: none;
  color: #716b82;
  font-size: 10px;
}

.steps li {
  display: flex;
  gap: 6px;
  align-items: center;
}

.steps li span {
  display: grid;
  place-items: center;
  width: 17px;
  height: 17px;
  border: 1px solid currentColor;
  border-radius: 50%;
  font-size: 9px;
}

.steps .active,
.steps .complete {
  color: #c5b2f6;
}

.footnote {
  margin: 20px 0 0;
  color: #777084;
  font-size: 10px;
}

.failed .orbit,
.failed .emblem img {
  animation: none;
}

.error-mark {
  position: absolute;
  right: 24px;
  bottom: 24px;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: #f3b89d;
  color: #341e14;
  font-weight: 700;
}

.close-button {
  min-width: 110px;
  margin-top: 22px;
  padding: 9px 18px;
  border: 1px solid #66547e;
  border-radius: 7px;
  background: #a786ff20;
  font-size: 12px;
}

@keyframes orbit {
  to {
    transform: rotate(360deg);
  }
}

@keyframes breathe {
  50% {
    transform: translateY(-4px);
  }
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
  .orbit,
  .emblem img,
  .activity span {
    animation: none;
  }
}
</style>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { useI18n } from "vue-i18n";

import AppIcon from "./AppIcon.vue";
import AppSelect from "./AppSelect.vue";
import { useFocusTrap } from "../shared/composables/useFocusTrap";
import { usePlayerFullscreen } from "../features/media/usePlayerFullscreen";
import {
  bufferedSeconds,
  frameRates,
  seekPosition,
} from "../features/media/playbackMetrics";
import { formatDuration } from "../types/media";

import type { FrameSample } from "../features/media/playbackMetrics";

const props = defineProps<{
  source: string;
  filename: string;
  sourceFps?: number;
}>();

const emit = defineEmits<{ ready: []; failed: [] }>();

const { t } = useI18n();

const player = ref<HTMLElement | null>(null);
const video = ref<HTMLVideoElement | null>(null);
const fullscreenButton = ref<HTMLButtonElement | null>(null);
const controlsVisible = ref(false);

const { trapFocus } = useFocusTrap(player);

const playing = ref(false);

const toolbarVisible = computed(() => controlsVisible.value || !playing.value);

const waiting = ref(false);
const duration = ref(0);
const position = ref(0);
const volume = ref(1);
const muted = ref(false);
const repeat = ref(false);
const speed = ref(1);

const speedOptions = [0.5, 0.75, 1, 1.25, 1.5, 2].map((rate) => ({
  value: String(rate),
  label: `${rate}×`,
}));

const {
  fullscreen,
  fullscreenBusy,
  toggleFullscreen,
  fullscreenChanged,
  disposeFullscreen,
} = usePlayerFullscreen(player, resetFullscreenMetrics, () => {
  if (!disposed) feedback.value = t("playerFullscreenFailed");
});

const feedback = ref("");

const diagnostics = ref(false);
const submittedFps = ref<number | null>(null);
const callbackFps = ref<number | null>(null);
const droppedFrames = ref<number | null>(null);
const totalFrames = ref<number | null>(null);
const buffer = ref(0);
const decodeMs = ref<number | null>(null);

const progress = computed(() =>
  duration.value ? (position.value / duration.value) * 100 : 0,
);

const timeLabel = computed(
  () => `${formatDuration(position.value)} / ${formatDuration(duration.value)}`,
);

let frameCallback: number | null = null;
let sampleTimer: ReturnType<typeof setInterval> | undefined;
let previousSample: FrameSample | null = null;
let latestFrame: { time: number; frames: number } | null = null;
let callbacks = 0;

let disposed = false;
let lastPositionUpdate = -Infinity;

let latestDecodeMs: number | null = null;

let controlsTimer: ReturnType<typeof setTimeout> | undefined;

interface FrameMetadata {
  presentationTime: number;
  presentedFrames: number;
  processingDuration?: number;
}

function cancelMetrics() {
  if (frameCallback !== null)
    video.value?.cancelVideoFrameCallback?.(frameCallback);

  frameCallback = null;
  clearInterval(sampleTimer);
  sampleTimer = undefined;
  previousSample = null;
  latestFrame = null;
  callbacks = 0;
  latestDecodeMs = null;
  submittedFps.value = null;
  callbackFps.value = null;
}

function updateQuality() {
  const element = video.value;

  if (!element) return;

  const quality = element.getVideoPlaybackQuality?.();

  droppedFrames.value = quality?.droppedVideoFrames ?? null;
  totalFrames.value = quality?.totalVideoFrames ?? null;
  buffer.value = bufferedSeconds(element.buffered, element.currentTime);

  if (element.paused || element.seeking || waiting.value || document.hidden) {
    previousSample = null;
    submittedFps.value = null;
    callbackFps.value = null;

    return;
  }

  if (!latestFrame) return;

  const sample = {
    time: performance.now(),
    frames: latestFrame.frames,
    callbacks,
  };

  if (previousSample) {
    const rates = frameRates(previousSample, sample);

    submittedFps.value = rates?.submitted ?? null;
    callbackFps.value = rates?.callbacks ?? null;
  }

  previousSample = sample;
}

function beginMetrics() {
  cancelMetrics();

  const element = video.value;

  if (!diagnostics.value || !element || disposed) return;

  droppedFrames.value = null;
  totalFrames.value = null;
  decodeMs.value = null;

  if (element.requestVideoFrameCallback) {
    const frame = (_now: number, metadata: FrameMetadata) => {
      if (disposed || !diagnostics.value || element !== video.value) return;

      // Keep high-frequency callbacks out of Vue rendering. Publish once per second.
      callbacks++;
      latestFrame = {
        time: metadata.presentationTime,
        frames: metadata.presentedFrames,
      };

      const processing = metadata.processingDuration;

      // Decode timing is sampled by the slow timer below, not a reactive per-frame write.
      latestDecodeMs = processing === undefined ? null : processing * 1000;
      frameCallback = element.requestVideoFrameCallback(frame);
    };

    frameCallback = element.requestVideoFrameCallback(frame);
  }

  updateQuality();
  sampleTimer = setInterval(() => {
    decodeMs.value = latestDecodeMs;
    updateQuality();
  }, 1000);
}

function sync() {
  const element = video.value;

  if (!element) return;

  lastPositionUpdate = performance.now();
  duration.value = Number.isFinite(element.duration) ? element.duration : 0;
  position.value = element.currentTime;

  const nextPlaying = !element.paused && !element.ended;

  if (playing.value !== nextPlaying) hideFullscreenControls();

  playing.value = nextPlaying;
  volume.value = element.volume;
  muted.value = element.muted;
  speed.value = element.playbackRate;
}

function timeUpdate() {
  const element = video.value;

  if (!element || disposed) return;

  const now = performance.now();

  if (now - lastPositionUpdate < 250) return;

  lastPositionUpdate = now;
  position.value = element.currentTime;
}

function ready() {
  if (disposed || video.value?.getAttribute("src") !== props.source) return;

  sync();
  emit("ready");
}

function active() {
  waiting.value = false;
  previousSample = null;
  sync();
}

function seeking() {
  previousSample = null;
  submittedFps.value = null;
  callbackFps.value = null;
  sync();
}

async function togglePlayback() {
  const element = video.value;

  if (!element) return;

  feedback.value = "";

  if (!element.paused) {
    element.pause();

    return;
  }

  try {
    await element.play();
  } catch (cause) {
    if (
      !disposed &&
      element === video.value &&
      !(cause instanceof DOMException && cause.name === "AbortError")
    )
      feedback.value = t("playerPlayFailed");
  }
}

function seek(value: number) {
  const next = seekPosition(value, duration.value);

  if (next === null || !video.value) return;

  video.value.currentTime = next;
  position.value = next;
  previousSample = null;
  showFullscreenControls();
}

function setVolume(value: number) {
  if (!video.value) return;

  video.value.volume = Math.max(0, Math.min(1, value));
  video.value.muted = video.value.volume === 0;
  sync();
  showFullscreenControls();
}

function toggleRepeat() {
  if (!video.value) return;

  repeat.value = !repeat.value;
  video.value.loop = repeat.value;
  showFullscreenControls();
}

function toggleMute() {
  if (!video.value) return;

  video.value.muted = !video.value.muted;
  sync();
  showFullscreenControls();
}

function hideFullscreenControls() {
  clearTimeout(controlsTimer);
  controlsTimer = undefined;
  controlsVisible.value = false;
}

function showFullscreenControls() {
  if (!fullscreen.value || disposed) return;

  clearTimeout(controlsTimer);
  controlsVisible.value = true;
  controlsTimer = setTimeout(hideFullscreenControls, 2000);
}

function setSpeed(value: string) {
  if (!video.value || !speedOptions.some((option) => option.value === value))
    return;

  video.value.playbackRate = Number(value);
  previousSample = null;
  sync();
}

function resetFullscreenMetrics() {
  hideFullscreenControls();
  feedback.value = "";
  previousSample = null;
  submittedFps.value = null;
  callbackFps.value = null;
  void nextTick(() => {
    if (disposed) return;

    const target = fullscreen.value ? player.value : fullscreenButton.value;

    target?.focus({ preventScroll: true });
  });
}

function keyboard(event: KeyboardEvent) {
  if (fullscreen.value) trapFocus(event);

  if (event.key === "Escape" && fullscreen.value) {
    event.preventDefault();
    event.stopPropagation();
    void toggleFullscreen();

    return;
  }

  if (
    event.defaultPrevented ||
    event.isComposing ||
    event.ctrlKey ||
    event.metaKey ||
    event.altKey
  )
    return;

  const target = event.target as HTMLElement;

  if (
    target.closest(
      "input:not([type='range']), textarea, select, [role='listbox'], [contenteditable='true']",
    )
  )
    return;

  const key =
    event.code === "KeyF"
      ? "f"
      : event.code === "KeyM"
        ? "m"
        : event.code === "KeyT"
          ? "t"
          : event.code === "KeyR"
            ? "r"
            : event.key.toLowerCase();

  if (key === " " || event.code === "Space" || key === "k") {
    if (!event.repeat) void togglePlayback();
  } else if (key === "f") {
    if (!event.repeat) void toggleFullscreen();
  } else if (key === "m") {
    if (!event.repeat) toggleMute();
  } else if (key === "r") {
    if (!event.repeat) toggleRepeat();
  } else if (key === "t") {
    if (!event.repeat) diagnostics.value = !diagnostics.value;
  } else {
    if (
      !fullscreen.value &&
      target.closest("input, select, button, [role='listbox']")
    )
      return;

    const element = video.value;

    if (!element) return;

    if (event.key === "ArrowLeft") seek(element.currentTime - 5);
    else if (event.key === "ArrowRight") seek(element.currentTime + 5);
    else if (fullscreen.value && event.key === "ArrowUp")
      setVolume((element.muted ? 0 : element.volume) + 0.05);
    else if (fullscreen.value && event.key === "ArrowDown")
      setVolume((element.muted ? 0 : element.volume) - 0.05);
    else return;
  }

  event.preventDefault();
  event.stopPropagation();
}

function stop() {
  hideFullscreenControls();
  cancelMetrics();

  if (!video.value) return;

  video.value.pause();
  video.value.removeAttribute("src");
  video.value.load();
}

function failed() {
  if (disposed || video.value?.getAttribute("src") !== props.source) return;

  cancelMetrics();
  emit("failed");
}

watch(diagnostics, beginMetrics);

onMounted(() => {
  document.addEventListener("fullscreenchange", fullscreenChanged);
});

onBeforeUnmount(() => {
  disposed = true;
  document.removeEventListener("fullscreenchange", fullscreenChanged);

  void disposeFullscreen();

  stop();
});

defineExpose({ stop, keyboard });
</script>

<template>
  <div
    ref="player"
    class="nexa-video-player"
    tabindex="-1"
    @keydown.capture="keyboard"
  >
    <div class="video-surface">
      <video
        ref="video"
        :src="source"
        :loop="repeat"
        :aria-label="filename"
        tabindex="0"
        playsinline
        preload="auto"
        @loadedmetadata="ready"
        @durationchange="sync"
        @timeupdate="timeUpdate"
        @play="sync"
        @pause="sync"
        @ended="sync"
        @playing="active"
        @canplay="active"
        @waiting="waiting = true"
        @seeking="seeking"
        @seeked="active"
        @volumechange="sync"
        @ratechange="sync"
        @error="failed"
        @click="togglePlayback"
        @dblclick="toggleFullscreen"
      ></video>
      <span v-if="waiting" class="video-buffering" role="status">{{
        t("playerBuffering")
      }}</span>
      <div
        v-if="diagnostics"
        class="playback-diagnostics"
        role="group"
        :aria-label="t('playerDiagnostics')"
      >
        <dl>
          <div>
            <dt>{{ t("playerSourceFps") }}</dt>
            <dd>{{ sourceFps?.toFixed(1) ?? "—" }}</dd>
          </div>
          <div>
            <dt>{{ t("playerSubmittedFps") }}</dt>
            <dd>{{ submittedFps?.toFixed(1) ?? "—" }}</dd>
          </div>
          <div>
            <dt>{{ t("playerCallbackFps") }}</dt>
            <dd>{{ callbackFps?.toFixed(1) ?? "—" }}</dd>
          </div>
          <div>
            <dt>{{ t("playerDropped") }}</dt>
            <dd>{{ droppedFrames ?? "—" }} / {{ totalFrames ?? "—" }}</dd>
          </div>
          <div>
            <dt>{{ t("playerBuffer") }}</dt>
            <dd>{{ buffer.toFixed(1) }} s</dd>
          </div>
          <div>
            <dt>{{ t("playerDecode") }}</dt>
            <dd>{{ decodeMs?.toFixed(1) ?? "—" }} ms</dd>
          </div>
        </dl>
        <p>{{ t("playerMetricsNote") }}</p>
      </div>
    </div>
    <div
      class="video-toolbar"
      :class="{ 'is-visible': toolbarVisible }"
      :inert="fullscreen || undefined"
    >
      <input
        class="video-seek"
        type="range"
        min="0"
        :max="duration || 1"
        step="0.1"
        :value="position"
        :disabled="!duration"
        :aria-label="t('playerSeek')"
        :aria-valuetext="timeLabel"
        :style="{ '--progress': `${progress}%` }"
        @input="seek(Number(($event.target as HTMLInputElement).value))"
      />
      <div class="video-actions">
        <button
          class="icon-button"
          :aria-label="t(playing ? 'playerPause' : 'playerPlay')"
          :title="t(playing ? 'playerPause' : 'playerPlay')"
          @click="togglePlayback"
        >
          <AppIcon :name="playing ? 'pause' : 'play'" />
        </button>
        <button
          class="icon-button"
          :class="{ active: repeat }"
          :aria-label="t('playerRepeat')"
          :title="`${t('playerRepeat')} (R)`"
          :aria-pressed="repeat"
          @click="toggleRepeat"
        >
          <AppIcon name="repeat" :size="18" />
        </button>
        <output class="video-time">{{ timeLabel }}</output>
        <button
          class="icon-button"
          :aria-label="t(muted ? 'playerUnmute' : 'playerMute')"
          :title="t(muted ? 'playerUnmute' : 'playerMute')"
          @click="toggleMute"
        >
          <AppIcon
            :name="muted || !volume ? 'volume-off' : 'volume'"
            :size="18"
          />
        </button>
        <input
          class="video-volume"
          type="range"
          min="0"
          max="1"
          step="0.01"
          :value="muted ? 0 : volume"
          :style="{ '--progress': `${(muted ? 0 : volume) * 100}%` }"
          :aria-label="t('playerVolume')"
          @input="setVolume(Number(($event.target as HTMLInputElement).value))"
        />
        <output v-if="fullscreen" class="video-time">
          {{ Math.round((muted ? 0 : volume) * 100) }}%
        </output>
        <span class="video-spacer"></span>
        <AppSelect
          class="video-speed"
          :model-value="String(speed)"
          :options="speedOptions"
          :label="t('playerSpeed')"
          :disabled="fullscreen"
          @update:model-value="setSpeed"
        />
        <button
          class="icon-button"
          :aria-label="t('playerDiagnostics')"
          :title="`${t('playerDiagnostics')} (T)`"
          :aria-pressed="diagnostics"
          @click="diagnostics = !diagnostics"
        >
          <AppIcon name="info" :size="18" />
        </button>
        <button
          ref="fullscreenButton"
          class="icon-button"
          :disabled="fullscreenBusy"
          :aria-label="
            t(fullscreen ? 'playerExitFullscreen' : 'playerFullscreen')
          "
          :title="t(fullscreen ? 'playerExitFullscreen' : 'playerFullscreen')"
          @click="toggleFullscreen"
        >
          <AppIcon
            :name="fullscreen ? 'fullscreen-exit' : 'fullscreen'"
            :size="19"
          />
        </button>
      </div>
      <p v-if="feedback" class="video-feedback" role="alert">{{ feedback }}</p>
    </div>
  </div>
</template>

<style scoped>
.nexa-video-player {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  background: #090a0e;
  color: var(--text);
}
.video-surface {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
}
.video-surface video {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.video-toolbar {
  position: absolute;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  width: min(760px, calc(100% - 144px));
  padding: 12px 18px;
  background: var(--surface);
  border: 1px solid var(--line-strong);
  border-radius: 24px;
}
.video-seek {
  display: block;
  width: 100%;
  margin: 0 0 10px;
  accent-color: var(--accent);
  cursor: pointer;
}

.video-toolbar input[type="range"] {
  appearance: none;
  height: 5px;
  border-radius: 8px;
  background: linear-gradient(
    to right,
    var(--accent) var(--progress, 0%),
    var(--line-strong) var(--progress, 0%)
  );
}

.video-toolbar input[type="range"]::-webkit-slider-runnable-track {
  height: 5px;
  border-radius: 8px;
  background: transparent;
}

.video-toolbar input[type="range"]::-webkit-slider-thumb {
  appearance: none;
  width: 12px;
  height: 12px;
  margin-top: -3.5px;
  border-radius: 50%;
  background: var(--accent);
  cursor: pointer;
}

.video-toolbar input[type="range"]::-moz-range-track {
  height: 5px;
  border-radius: 8px;
  background: transparent;
}

.video-toolbar input[type="range"]::-moz-range-progress {
  background: transparent;
}

.video-toolbar input[type="range"]::-moz-range-thumb {
  width: 12px;
  height: 12px;
  border: 0;
  border-radius: 50%;
  background: var(--accent);
  cursor: pointer;
}

.video-toolbar input[type="range"]:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 5px;
}
.video-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}
.video-actions .icon-button.active {
  color: var(--accent);
  background: var(--accent-soft);
}
.video-time {
  color: var(--muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.video-volume {
  width: 76px;
  accent-color: var(--accent);
}
.video-spacer {
  flex: 1;
}
.video-speed {
  width: 88px;
  flex-shrink: 0;
}

.video-speed :deep(.sort-select-options) {
  top: auto;
  bottom: calc(100% + 6px);
}
.video-actions .icon-button {
  flex-shrink: 0;
  color: var(--text);
}
.video-actions .icon-button[aria-pressed="true"] {
  color: var(--accent);
  background: var(--accent-soft);
}
.video-buffering {
  position: absolute;
  padding: 10px 16px;
  border-radius: 8px;
  background: var(--media-overlay);
  color: var(--on-media);
}
.playback-diagnostics {
  position: absolute;
  top: 12px;
  left: 12px;
  width: min(330px, calc(100% - 24px));
  padding: 12px;
  background: var(--media-overlay);
  color: var(--on-media);
  border: 1px solid #ffffff30;
  border-radius: 8px;
  font-size: 12px;
  pointer-events: none;
}
.playback-diagnostics dl {
  margin: 0;
  font-variant-numeric: tabular-nums;
}
.playback-diagnostics dl div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin: 4px 0;
}
.playback-diagnostics dd {
  margin: 0;
}
.playback-diagnostics p {
  margin: 10px 0 0;
  line-height: 1.5;
}
.video-feedback {
  margin: 8px 0 0;
  color: var(--danger);
  font-size: 12px;
}
.nexa-video-player:fullscreen {
  width: 100vw;
  height: 100vh;
  border: 0;
  border-radius: 0;
  box-shadow: none;
  outline: none;
}

.nexa-video-player:fullscreen .video-toolbar {
  position: absolute;
  inset: auto 0 0;
  transform: none;
  width: auto;
  border-radius: 0;
  visibility: hidden;
  pointer-events: none;
}

.nexa-video-player:fullscreen .video-toolbar.is-visible {
  visibility: visible;
}

.nexa-video-player:fullscreen .video-surface {
  pointer-events: none;
}

.nexa-video-player:fullscreen .video-toolbar button {
  transition: none;
}

.nexa-video-player:fullscreen::backdrop {
  background: #090a0e;
}

.nexa-video-player:fullscreen,
.nexa-video-player:fullscreen * {
  cursor: none !important;
}

.nexa-video-player:fullscreen input[type="range"]::-webkit-slider-thumb {
  cursor: none !important;
}

.nexa-video-player:fullscreen input[type="range"]::-moz-range-thumb {
  cursor: none !important;
}

@media (max-width: 1000px) {
  .video-actions {
    gap: 5px;
  }
  .video-volume {
    width: 54px;
  }
  .video-toolbar {
    padding: 10px;
  }
}
</style>

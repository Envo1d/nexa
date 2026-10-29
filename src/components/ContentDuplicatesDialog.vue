<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
} from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

import AppIcon from "./AppIcon.vue";
import MediaPreview from "./MediaPreview.vue";
import {
  createDuplicateMatcher,
  duplicatePairKey,
} from "../features/media/duplicateMatching";
import { formatDate, formatSize } from "../types/media";
import { translateUi as ui } from "../i18n/presentation";
import { useFocusTrap } from "../shared/composables/useFocusTrap";

import type {
  DuplicateFile,
  DuplicatePair,
} from "../features/media/duplicateMatching";
import type { MediaItem } from "../types/media";

defineProps<{ showPreviews: boolean; opening: boolean }>();

const emit = defineEmits<{
  close: [];
  open: [item: MediaItem, main: boolean];
  remove: [item: MediaItem, main: boolean];
  replace: [stage: MediaItem, main: MediaItem];
}>();

type NativeFile = Omit<
  MediaItem,
  "size" | "width" | "height" | "modifiedAt" | "createdAt"
> & {
  revision: string | null;
  size: number | null;
  width: number | null;
  height: number | null;
  modifiedAt: number | null;
  createdAt: number | null;
  main: boolean;
  sha256: string;
  visual: string | null;
};

interface FingerprintPage {
  files: NativeFile[];
  after: string;
  more: boolean;
  skipped: number;
  visualUnavailable: number;
  scope: string;
  processed: number;
  reused: number;
  hashed: number;
}

interface ScanInventory {
  total: number;
  upperId: string;
  scope: string;
  rejections: [string, string][];
  generation: number;
}

interface SavedPair {
  leftId: string;
  rightId: string;
  leftHash: string;
  rightHash: string;
  exact: boolean;
  distance: number;
}

interface ScanReport {
  total: number;
  processed: number;
  checked: number;
  skipped: number;
  visualUnavailable: number;
  reused: number;
  hashed: number;
}

interface SavedResults {
  snapshot: { updatedAt: number; status: string; report: ScanReport } | null;
  files: NativeFile[];
  pairs: SavedPair[];
  rejections: [string, string][];
  stale: number;
}

const { t } = useI18n();

const dialog = ref<HTMLElement | null>(null);

const { trapFocus } = useFocusTrap(dialog);

const pairs = shallowRef<DuplicatePair[]>([]);
const selected = shallowRef<DuplicatePair>();

const running = ref(false);
const loadingSaved = ref(false);

const savedAt = ref<number | null>(null);
const savedStale = ref(0);

const reused = ref(0);
const hashed = ref(0);

let activeInventory: ScanInventory | null = null;

const savingReview = ref(false);

const processed = ref(0);
const total = ref<number | null>(null);

const rejectedCount = ref(0);
const lastRejected = shallowRef<DuplicatePair>();

const rejections = new Set<string>();

const progress = computed(() =>
  total.value
    ? Math.min(100, Math.floor((processed.value / total.value) * 100))
    : total.value === 0 &&
        ["duplicatesComplete", "duplicatesSavedComplete"].includes(status.value)
      ? 100
      : 0,
);

const checked = ref(0);
const skipped = ref(0);
const visualUnavailable = ref(0);

const error = ref("");
const status = ref("duplicatesReady");

const mode = ref("exact");
const shown = ref(50);

const filtered = computed(() =>
  pairs.value
    .filter((pair) => pair.exact === (mode.value === "exact"))
    .sort((left, right) => left.distance - right.distance),
);

const exactCount = computed(
  () => pairs.value.filter((pair) => pair.exact).length,
);

const comparison = computed(() =>
  selected.value ? [selected.value.left, selected.value.right] : [],
);

const replacement = computed(() => {
  const pair = selected.value;

  if (
    !pair ||
    pair.left.main === pair.right.main ||
    pair.left.item.kind !== pair.right.item.kind
  )
    return null;

  return pair.left.main
    ? { main: pair.left, stage: pair.right }
    : { main: pair.right, stage: pair.left };
});

const stopped = ref(false);

let disposed = false;
let scanVersion = 0;
let trigger: HTMLElement | null = null;

function convert(file: NativeFile): DuplicateFile {
  return {
    main: file.main,
    sha256: file.sha256,
    visual: file.visual,
    item: {
      ...file,
      id: `duplicates-${file.id}`,
      nativeId: file.id,
      nativeRevision: file.revision ?? "imported",
      preview: "",
      favorite: false,
      size: file.size ?? -1,
      width: file.width ?? 0,
      height: file.height ?? 0,
      modifiedAt: file.modifiedAt ?? 0,
      createdAt: file.createdAt ?? 0,
      duration: file.duration == null ? undefined : file.duration / 1000,
    },
  };
}

async function scan() {
  if (running.value || savingReview.value || loadingSaved.value) return;

  const version = ++scanVersion;
  let after = "0";

  stopped.value = false;
  running.value = true;
  processed.value = 0;
  total.value = null;
  checked.value = 0;
  skipped.value = 0;
  visualUnavailable.value = 0;
  reused.value = 0;
  hashed.value = 0;
  savedAt.value = null;
  savedStale.value = 0;
  activeInventory = null;
  pairs.value = [];
  selected.value = undefined;
  shown.value = 50;
  error.value = "";
  lastRejected.value = undefined;
  status.value = "duplicatesPreparing";

  try {
    const inventory = await invoke<ScanInventory>("prepare_duplicate_scan");

    if (disposed || version !== scanVersion) return;

    activeInventory = inventory;
    total.value = inventory.total;
    rejections.clear();

    for (const [left, right] of inventory.rejections)
      rejections.add(duplicatePairKey(left, right));

    rejectedCount.value = rejections.size;

    const matcher = createDuplicateMatcher(1000, rejections);

    if (stopped.value || !inventory.total) {
      status.value = stopped.value ? "duplicatesStopped" : "duplicatesComplete";

      await checkpoint(stopped.value ? "stopped" : "complete");

      return;
    }

    status.value = "duplicatesSearching";

    for (;;) {
      const base = processed.value;
      const onProgress = new Channel<{ completed: number }>();

      onProgress.onmessage = (message) => {
        if (!disposed && version === scanVersion && running.value) {
          processed.value = Math.max(processed.value, base + message.completed);
        }
      };

      const page: FingerprintPage = await invoke<FingerprintPage>(
        "fingerprint_duplicate_batch",
        {
          after,
          scope: inventory.scope,
          upperId: inventory.upperId,
          onProgress,
        },
      );

      if (disposed) return;

      for (const file of page.files) matcher.add(convert(file));

      checked.value += page.files.length;
      skipped.value += page.skipped;
      visualUnavailable.value += page.visualUnavailable;
      processed.value = base + page.processed;
      reused.value += page.reused;
      hashed.value += page.hashed;
      pairs.value = [...matcher.pairs];
      after = page.after;

      const limited = matcher.full || checked.value >= 10000;

      await checkpoint(
        limited
          ? "limited"
          : stopped.value
            ? "stopped"
            : !page.more
              ? "complete"
              : "partial",
      );

      if (disposed) return;

      if (limited) {
        status.value = "duplicatesLimit";
        break;
      }

      if (stopped.value || !page.more) {
        status.value = stopped.value
          ? "duplicatesStopped"
          : "duplicatesComplete";
        break;
      }
    }
  } catch (cause) {
    if (!disposed) {
      error.value = String(cause);
      status.value = "duplicatesFailed";
    }
  } finally {
    if (!disposed) running.value = false;
  }
}

async function checkpoint(scanStatus: string) {
  if (!activeInventory) return;

  const report: ScanReport = {
    total: total.value ?? 0,
    processed: processed.value,
    checked: checked.value,
    skipped: skipped.value,
    visualUnavailable: visualUnavailable.value,
    reused: reused.value,
    hashed: hashed.value,
  };

  const savedPairs: SavedPair[] = pairs.value.map((pair) => ({
    leftId: pair.left.item.nativeId!,
    rightId: pair.right.item.nativeId!,
    leftHash: pair.left.sha256,
    rightHash: pair.right.sha256,
    exact: pair.exact,
    distance: pair.distance,
  }));

  await invoke("save_duplicate_search", {
    checkpoint: {
      scope: activeInventory.scope,
      generation: activeInventory.generation,
      status: scanStatus,
      report,
      pairs: savedPairs,
    },
  });

  if (!disposed) savedAt.value = Date.now();
}

async function loadSaved() {
  if (running.value || savingReview.value || loadingSaved.value) return;

  const version = ++scanVersion;

  loadingSaved.value = true;
  error.value = "";

  try {
    const saved = await invoke<SavedResults>("load_duplicate_search");

    if (disposed || version !== scanVersion) return;

    rejections.clear();

    for (const [left, right] of saved.rejections)
      rejections.add(duplicatePairKey(left, right));

    rejectedCount.value = rejections.size;
    savedStale.value = saved.stale;
    pairs.value = [];
    selected.value = undefined;
    lastRejected.value = undefined;
    savedAt.value = saved.snapshot?.updatedAt ?? null;

    if (saved.snapshot) {
      const files = new Map(
        saved.files.map((file) => [file.id, convert(file)]),
      );

      pairs.value = saved.pairs.flatMap((pair) => {
        const left = files.get(pair.leftId);

        const right = files.get(pair.rightId);

        return left && right
          ? [
              {
                left,
                right,
                exact: pair.exact,
                distance: pair.distance,
              },
            ]
          : [];
      });

      const report = saved.snapshot.report;

      total.value = report.total;
      processed.value = report.processed;
      checked.value = report.checked;
      skipped.value = report.skipped;
      visualUnavailable.value = report.visualUnavailable;
      reused.value = report.reused;
      hashed.value = report.hashed;
      status.value =
        saved.snapshot.status === "complete"
          ? "duplicatesSavedComplete"
          : "duplicatesSavedPartial";
    } else {
      total.value = null;
      processed.value = 0;
      checked.value = 0;
      skipped.value = 0;
      visualUnavailable.value = 0;
      reused.value = 0;
      hashed.value = 0;
      status.value = "duplicatesReady";
    }
  } catch (cause) {
    if (!disposed) error.value = String(cause);
  } finally {
    if (!disposed) loadingSaved.value = false;
  }
}

async function reviewPair(rejected: boolean) {
  const pair = rejected ? selected.value : lastRejected.value;

  if (!pair || pair.exact || running.value || savingReview.value) return;

  savingReview.value = true;
  error.value = "";

  try {
    await invoke("set_duplicate_rejection", {
      leftHash: pair.left.sha256,
      rightHash: pair.right.sha256,
      rejected,
    });

    if (disposed) return;

    const key = duplicatePairKey(pair.left.sha256, pair.right.sha256);

    if (rejected) {
      rejections.add(key);
      pairs.value = pairs.value.filter(
        (candidate) =>
          candidate.exact ||
          duplicatePairKey(candidate.left.sha256, candidate.right.sha256) !==
            key,
      );
      lastRejected.value = pair;
      selected.value = undefined;
    } else {
      rejections.delete(key);
      pairs.value = [...pairs.value, pair];
      lastRejected.value = undefined;
      mode.value = "similar";
      selected.value = pair;
    }

    rejectedCount.value = rejections.size;
    savingReview.value = false;

    await nextTick();

    if (!disposed) {
      dialog.value
        ?.querySelector<HTMLElement>(
          rejected ? "[data-review-undo]" : "[data-review-reject]",
        )
        ?.focus();
    }
  } catch (cause) {
    if (!disposed) error.value = String(cause);
  } finally {
    if (!disposed) savingReview.value = false;
  }
}

function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();

    if (!savingReview.value) emit("close");
  }

  trapFocus(event);
}

onMounted(() => {
  trigger =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  dialog.value?.focus();
  void loadSaved();
});

onBeforeUnmount(() => {
  disposed = true;
  ++scanVersion;
  stopped.value = true;
  trigger?.focus();
});
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="!savingReview && emit('close')">
      <section
        ref="dialog"
        class="content-duplicates-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="content-duplicates-title"
        tabindex="-1"
        @keydown.stop="keyboard"
      >
        <header class="panel-heading">
          <div class="duplicates-heading">
            <span class="duplicates-mark"
              ><AppIcon name="files" :size="24"
            /></span>
            <div>
              <span class="duplicates-eyebrow">{{
                t("duplicatesWorkspace")
              }}</span>
              <h2 id="content-duplicates-title">
                {{ t("contentDuplicates") }}
              </h2>
            </div>
          </div>
          <button
            class="icon-button"
            :aria-label="t('duplicatesClose')"
            :disabled="savingReview"
            @click="emit('close')"
          >
            <AppIcon name="close" />
          </button>
        </header>
        <div class="duplicates-controls">
          <p class="muted">{{ t("duplicatesExplanation") }}</p>
          <div class="duplicates-actions">
            <button
              class="primary-button"
              :disabled="running || opening || savingReview || loadingSaved"
              @click="scan"
            >
              <AppIcon name="search" :size="16" />{{
                t(savedAt ? "duplicatesUpdate" : "duplicatesScan")
              }}
            </button>
            <button
              v-if="running"
              class="secondary-button"
              :disabled="stopped"
              @click="stopped = true"
            >
              <AppIcon name="pause" :size="15" />{{ t("duplicatesStop") }}
            </button>
            <button
              v-if="error && !running"
              class="secondary-button"
              :disabled="loadingSaved"
              @click="loadSaved"
            >
              {{ t("duplicatesReloadSaved") }}
            </button>
          </div>
          <p v-if="loadingSaved" class="muted" role="status">
            {{ t("duplicatesLoadingSaved") }}
          </p>
          <p v-else-if="savedAt && !running" class="duplicates-saved-notice">
            {{ t("duplicatesSavedAt", { date: formatDate(savedAt) }) }}
            <span>{{ t("duplicatesSavedHint") }}</span>
          </p>
          <p v-if="savedStale" class="muted">
            {{ t("duplicatesStaleSaved", { count: savedStale }) }}
          </p>
          <div class="duplicate-scan-card" :class="{ running }">
            <div class="duplicate-scan-heading">
              <span role="status" aria-live="polite"
                ><AppIcon
                  :class="{ 'duplicate-scan-spinner': running }"
                  :name="
                    running
                      ? 'loader'
                      : status === 'duplicatesComplete'
                        ? 'check-circle'
                        : 'info'
                  "
                  :size="16"
                />{{ t(status) }}</span
              >
              <strong v-if="total !== null">{{ progress }}%</strong>
            </div>
            <div
              class="duplicate-progress-track"
              :class="{ indeterminate: running && total === null }"
              role="progressbar"
              :aria-label="t('duplicatesScanProgress')"
              :aria-valuenow="total === null ? undefined : progress"
              :aria-valuemin="0"
              :aria-valuemax="100"
            >
              <div
                class="duplicate-progress-fill"
                :style="{ width: `${progress}%` }"
              ></div>
            </div>
            <div class="duplicate-scan-details">
              <span>{{
                total === null
                  ? t(running ? "duplicatesPreparing" : "duplicatesReady")
                  : t("duplicatesProgressFiles", { count: processed, total })
              }}</span>
              <span>{{ t("duplicatesChecked", { count: checked }) }}</span>
            </div>
          </div>
          <p v-if="skipped || visualUnavailable" class="muted">
            {{ t("duplicatesSkipped", { skipped, visual: visualUnavailable }) }}
          </p>
          <p v-if="reused || hashed" class="muted">
            {{ t("duplicatesCacheCounts", { reused, hashed }) }}
          </p>
          <p v-if="error" role="alert">{{ error }}</p>
          <div
            v-if="rejectedCount || lastRejected"
            class="duplicate-review-status"
            role="status"
          >
            <AppIcon name="check" :size="15" />
            <span>{{
              t("duplicatesRejectedCount", { count: rejectedCount })
            }}</span>
            <button
              v-if="lastRejected"
              data-review-undo
              class="text-button"
              :disabled="savingReview || running"
              @click="reviewPair(false)"
            >
              {{ t("duplicatesUndo") }}
            </button>
          </div>
          <div class="duplicates-tabs">
            <button
              class="duplicates-tab"
              :disabled="savingReview"
              :aria-pressed="mode === 'exact'"
              @click="
                mode = 'exact';
                shown = 50;
                selected = undefined;
              "
            >
              <AppIcon name="copy" :size="16" />{{ t("duplicatesExact") }}
              <span>{{ exactCount }}</span>
            </button>
            <button
              class="duplicates-tab"
              :disabled="savingReview"
              :aria-pressed="mode === 'similar'"
              @click="
                mode = 'similar';
                shown = 50;
                selected = undefined;
              "
            >
              <AppIcon name="image" :size="16" />{{ t("duplicatesSimilar") }}
              <span>{{ pairs.length - exactCount }}</span>
            </button>
          </div>
        </div>
        <div class="duplicates-body">
          <aside
            class="duplicates-results"
            :aria-label="t('duplicatesMatches')"
          >
            <div class="duplicates-results-heading">
              {{ t("duplicatesMatches") }}<span>{{ filtered.length }}</span>
            </div>
            <div v-if="!filtered.length" class="duplicates-empty">
              <AppIcon :name="running ? 'search' : 'check-circle'" :size="30" />
              <p>
                {{ t(running ? "duplicatesSearching" : "duplicatesNoMatches") }}
              </p>
            </div>
            <button
              v-for="pair in filtered.slice(0, shown)"
              :key="`${pair.left.item.id}:${pair.right.item.id}`"
              class="duplicate-result"
              :aria-pressed="selected === pair"
              :disabled="savingReview"
              @click="selected = pair"
            >
              <div class="duplicate-result-kind">
                <AppIcon :name="pair.exact ? 'copy' : 'image'" :size="14" />{{
                  t(pair.left.main ? "mainView" : "stageView")
                }}<AppIcon name="arrow" :size="12" />{{
                  t(pair.right.main ? "mainView" : "stageView")
                }}
              </div>
              <strong>{{ pair.left.item.filename }}</strong>
              <span>{{ pair.right.item.filename }}</span>
              <small
                >{{ t(pair.exact ? "duplicatesExact" : "duplicatesSimilar")
                }}<template v-if="!pair.exact">
                  ·
                  {{
                    t("duplicatesDistance", { count: pair.distance })
                  }}</template
                ></small
              >
            </button>
            <button
              v-if="filtered.length > shown"
              class="secondary-button"
              @click="shown += 50"
            >
              {{ t("duplicatesMore") }}
            </button>
          </aside>
          <div class="duplicates-comparison">
            <template v-if="selected">
              <p class="duplicate-match-note">
                <AppIcon
                  :name="selected.exact ? 'check-circle' : 'info'"
                  :size="18"
                />
                <span>
                  {{
                    t(
                      selected.exact
                        ? "duplicatesExactNote"
                        : "duplicatesSimilarNote",
                    )
                  }}
                </span>
              </p>
              <div class="duplicates-columns">
                <article
                  v-for="file in comparison"
                  :key="file.item.id"
                  class="duplicate-version"
                >
                  <div class="duplicate-version-heading">
                    <h3>{{ t(file.main ? "mainView" : "stageView") }}</h3>
                    <span>{{ file.item.extension.toUpperCase() }}</span>
                  </div>
                  <MediaPreview
                    :item="file.item"
                    :show-previews="showPreviews"
                    variant="detail"
                  />
                  <strong>{{ file.item.filename }}</strong>
                  <p class="duplicate-technical">
                    {{ file.item.width || "—" }} ×
                    {{ file.item.height || "—" }} ·
                    {{ ui(formatSize(file.item.size)) }} ·
                    {{ file.item.extension.toUpperCase() }}
                  </p>
                  <dl>
                    <dt>{{ t("artist") }}</dt>
                    <dd>{{ file.item.artist || "—" }}</dd>
                    <dt>{{ t("copyright") }}</dt>
                    <dd>{{ file.item.copyrights.join(", ") || "—" }}</dd>
                    <dt>{{ t("characters") }}</dt>
                    <dd>{{ file.item.characters.join(", ") || "—" }}</dd>
                    <dt>{{ t("tags") }}</dt>
                    <dd>{{ file.item.tags.join(", ") || "—" }}</dd>
                    <dt>{{ t("duplicatesTitle") }}</dt>
                    <dd>{{ file.item.videoTitle || "—" }}</dd>
                    <dt>{{ t("duplicatesPath") }}</dt>
                    <dd>{{ file.item.path }}</dd>
                  </dl>
                  <div class="duplicates-actions">
                    <button
                      class="secondary-button"
                      :disabled="running || opening || savingReview"
                      @click="emit('open', file.item, file.main)"
                    >
                      <AppIcon name="arrow" :size="14" />{{
                        t("duplicatesOpen")
                      }}
                    </button>
                    <button
                      class="secondary-button"
                      :disabled="running || opening || savingReview"
                      @click="emit('remove', file.item, file.main)"
                    >
                      <AppIcon name="trash" :size="14" />{{
                        t("duplicatesDelete")
                      }}
                    </button>
                  </div>
                </article>
              </div>
              <div class="duplicate-review-actions">
                <button
                  v-if="!selected.exact"
                  data-review-reject
                  class="secondary-button duplicate-reject"
                  :disabled="running || opening || savingReview"
                  @click="reviewPair(true)"
                >
                  <AppIcon name="close" :size="16" />{{
                    t(
                      savingReview
                        ? "duplicatesSavingReview"
                        : "duplicatesNotSimilar",
                    )
                  }}
                </button>
                <button
                  v-if="replacement"
                  class="primary-button"
                  :disabled="running || opening || savingReview"
                  @click="
                    emit(
                      'replace',
                      replacement.stage.item,
                      replacement.main.item,
                    )
                  "
                >
                  {{ t("duplicatesReplace") }}
                </button>
              </div>
            </template>
            <div v-else class="duplicates-welcome">
              <span class="duplicates-welcome-icon"
                ><AppIcon name="image" :size="38"
              /></span>
              <h3>{{ t("duplicatesCompareHeading") }}</h3>
              <p>{{ t("duplicatesChoose") }}</p>
              <span class="duplicates-safety"
                ><AppIcon name="check-circle" :size="15" />{{
                  t("duplicatesManualChoice")
                }}</span
              >
            </div>
          </div>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.content-duplicates-dialog {
  width: min(1380px, 96vw);
  height: min(900px, 92vh);
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 18px;
  box-shadow:
    0 28px 90px var(--shadow-deep),
    0 0 0 1px var(--hover-overlay);
  overflow: hidden;
}
.panel-heading,
.duplicates-controls {
  background: var(--surface-alt);
  padding: 16px 20px;
}
.duplicates-controls {
  border-bottom: 1px solid var(--line);
}
.duplicates-controls p {
  margin: 0 0 12px;
}
.duplicates-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  align-items: center;
  margin: 10px 0;
}
.duplicates-actions [aria-pressed="true"],
.duplicate-result[aria-pressed="true"] {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.duplicates-body {
  display: grid;
  grid-template-columns: 280px minmax(0, 1fr);
  min-height: 0;
  flex: 1;
}
.duplicates-results {
  background: var(--surface-alt);
  overflow-y: auto;
  padding: 12px;
  border-right: 1px solid var(--line);
}
.duplicate-result {
  width: 100%;
  display: grid;
  gap: 6px;
  text-align: left;
  padding: 12px;
  margin-bottom: 8px;
  border: 1px solid var(--line);
  border-radius: 10px;
  overflow-wrap: anywhere;
  background: var(--surface);
  color: var(--text);
}
.duplicate-result small {
  color: var(--muted);
}
.duplicates-comparison {
  padding: 16px 20px;
  overflow-y: auto;
  background:
    radial-gradient(ellipse at 50% 0%, var(--accent-glow), transparent 65%),
    var(--background);
}
.duplicates-columns {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 20px;
  margin-bottom: 20px;
}
.duplicate-version {
  min-width: 0;
  overflow-wrap: anywhere;
  padding: 16px;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface);
  box-shadow: 0 4px 18px var(--shadow-soft);
}
.duplicate-version :deep(.detail-preview) {
  height: clamp(170px, 23vh, 280px);
  position: relative;
  overflow: hidden;
  background: var(--background);
  border-radius: 8px;
  margin: 12px 0;
}
.duplicate-version :deep(.detail-preview img) {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
dl {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 8px 12px;
  font-size: 12px;
}
dt {
  color: var(--muted);
}
dd {
  margin: 0;
  white-space: pre-wrap;
}
.panel-heading {
  background: linear-gradient(110deg, var(--accent-soft), var(--surface) 65%);
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}
.duplicates-heading {
  display: flex;
  align-items: center;
  gap: 14px;
}
.duplicates-heading h2 {
  font-size: 19px;
  margin: 4px 0 0;
  letter-spacing: -0.3px;
}
.duplicates-mark,
.duplicates-welcome-icon {
  display: grid;
  place-items: center;
  background: var(--accent-soft);
  color: var(--accent);
  border: 1px solid var(--accent-line);
}
.duplicates-mark {
  width: 48px;
  height: 48px;
  border-radius: 14px;
}
.duplicates-eyebrow {
  color: var(--text-accent);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1.5px;
  text-transform: uppercase;
}
.duplicates-controls > .muted {
  max-width: 950px;
  font-size: 12px;
  line-height: 1.6;
}
.duplicates-saved-notice {
  color: var(--text-accent);
  font-size: 12px;
  line-height: 1.6;
}
.duplicates-saved-notice span {
  color: var(--muted);
}
.duplicates-controls > [role="alert"] {
  color: var(--danger);
  background: var(--danger-bg);
  border: 1px solid var(--danger-line);
  border-radius: 8px;
  padding: 10px 12px;
}
.duplicates-controls > .duplicates-actions {
  margin-top: 0;
}
.duplicate-scan-card {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 12px 14px;
  margin-bottom: 14px;
}
.duplicate-scan-card.running {
  border-color: var(--accent-line);
}
.duplicate-scan-heading,
.duplicate-scan-details {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}
.duplicate-scan-heading > span {
  display: flex;
  align-items: center;
  gap: 8px;
}
.duplicate-scan-heading strong {
  color: var(--text-accent);
  font-variant-numeric: tabular-nums;
}
.duplicate-scan-details {
  color: var(--muted);
  font-size: 11px;
  margin-top: 8px;
  font-variant-numeric: tabular-nums;
}
.duplicate-progress-track {
  height: 6px;
  background: var(--surface-pressed);
  border-radius: 20px;
  overflow: hidden;
  margin-top: 12px;
}
.duplicate-progress-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--accent-strong), var(--accent));
  transition: width 160ms ease-out;
}
.duplicate-progress-track.indeterminate .duplicate-progress-fill {
  width: 35% !important;
  animation: duplicate-progress-pulse 1.4s ease-in-out infinite;
}
.duplicate-review-status {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 12px;
  margin-bottom: 12px;
}
.duplicate-review-status > .text-button {
  color: var(--text-accent);
  margin-left: auto;
}
.duplicates-tabs {
  display: flex;
  gap: 4px;
  width: fit-content;
  padding: 4px;
  background: var(--background);
  border: 1px solid var(--line);
  border-radius: 10px;
}
.duplicates-tab {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 7px;
  color: var(--muted);
}
.duplicates-tab[aria-pressed="true"] {
  background: var(--accent-soft);
  color: var(--text-accent);
  box-shadow: inset 0 0 0 1px var(--accent-line);
}
.duplicates-tab > span {
  padding: 2px 6px;
  border-radius: 5px;
  font-size: 11px;
  background: var(--hover-overlay);
  font-variant-numeric: tabular-nums;
}
.duplicates-results-heading {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--muted);
  font-size: 11px;
  font-weight: 600;
  padding: 5px 4px 14px;
}
.duplicate-result:hover {
  border-color: var(--line-strong);
  background: var(--surface-hover);
}
.duplicate-result[aria-pressed="true"] {
  border-color: var(--accent-line);
  background: var(--accent-soft);
  box-shadow: inset 3px 0 0 var(--accent);
}
.duplicate-result > span {
  color: var(--muted);
  font-size: 12px;
}
.duplicate-result-kind {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--text-accent);
  margin-bottom: 2px;
}
.duplicate-result small {
  border-top: 1px solid var(--line);
  padding-top: 8px;
  font-size: 10px;
}
.duplicate-match-note {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  margin: 0 0 16px;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
}
.duplicate-match-note > svg {
  flex-shrink: 0;
  margin-top: 1px;
  color: var(--text-accent);
}
.duplicate-version-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.duplicate-version-heading h3 {
  margin: 0;
  font-size: 12px;
  font-weight: 600;
}
.duplicate-version-heading > span {
  border: 1px solid var(--line-strong);
  border-radius: 5px;
  color: var(--muted);
  font-size: 10px;
  padding: 3px 6px;
}
.duplicate-technical {
  color: var(--text-accent);
  font-size: 11px;
  line-height: 1.6;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--line);
}
.duplicate-version .duplicates-actions {
  margin: 16px 0 0;
}
.duplicate-version .duplicates-actions > button {
  flex: 1;
  font-size: 11px;
}
.duplicate-review-actions {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  padding-bottom: 4px;
}
.duplicate-reject {
  border-color: var(--line-strong);
}
.duplicates-welcome {
  min-height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  padding: 24px;
  color: var(--muted);
}
.duplicates-welcome-icon {
  width: 84px;
  height: 84px;
  border-radius: 24px;
  box-shadow: 0 0 60px var(--accent-glow);
  margin-bottom: 20px;
}
.duplicates-welcome h3 {
  margin: 0 0 10px;
  color: var(--text);
  font-size: 18px;
}
.duplicates-welcome p {
  max-width: 380px;
  line-height: 1.7;
  margin: 0 0 24px;
}
.duplicates-safety {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 11px;
  color: var(--text-accent);
}
.duplicates-empty {
  padding: 30px 14px;
  text-align: center;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.7;
}
.duplicates-empty > svg {
  color: var(--subtle);
}
.duplicate-scan-spinner {
  animation: duplicate-spinner-rotate 0.9s linear infinite;
  transform-origin: center;
}
@keyframes duplicate-spinner-rotate {
  to {
    transform: rotate(360deg);
  }
}
@keyframes duplicate-progress-pulse {
  from {
    transform: translateX(-100%);
  }
  to {
    transform: translateX(390%);
  }
}
@media (prefers-reduced-motion: reduce) {
  .duplicate-scan-spinner,
  .duplicate-progress-track.indeterminate .duplicate-progress-fill {
    animation: none;
  }
  .duplicate-progress-fill {
    transition: none;
  }
}
@media (max-width: 850px) {
  .duplicates-body {
    grid-template-columns: 190px minmax(0, 1fr);
  }
  .duplicates-columns {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>

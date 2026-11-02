<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";

import AppIcon from "./AppIcon.vue";
import { isNativeData } from "../runtime";
import { translateUi as ui } from "../i18n/presentation";
import { retryNativePreview } from "../composables/useNativePreview";
import { healthMediaItem } from "../types/health";

import type {
  HealthCategory,
  HealthIssue,
  HealthMedia,
  HealthPage,
  RelinkProposal,
} from "../types/health";
import type { MediaItem } from "../types/media";

const props = defineProps<{ revision: number }>();

const emit = defineEmits<{
  changed: [];
  edit: [item: MediaItem, collection: string];
  indexing: [];
}>();

const { t } = useI18n();
const native = isNativeData();
const categories: { id: HealthCategory; icon: string }[] = [
  { id: "missing", icon: "folder" },
  { id: "operations", icon: "structure" },
  { id: "previews", icon: "image" },
  { id: "metadata", icon: "info" },
];

const category = ref<HealthCategory>("missing");
const page = ref<HealthPage>({ counts: [0, 0, 0, 0], total: 0, issues: [] });
const offset = ref(0);
const selectedKey = ref("");
const details = ref<MediaItem | null>(null);
const proposal = ref<RelinkProposal | null>(null);
const confirmed = ref(false);
const loading = ref(false);
const detailLoading = ref(false);
const busy = ref(false);
const error = ref("");
const message = ref("");

const selected = computed(() =>
  page.value.issues.find((issue) => issue.key === selectedKey.value),
);
const totalProblems = computed(() =>
  page.value.counts.reduce((sum, count) => sum + count, 0),
);

let request = 0;
let detailRequest = 0;
let disposed = false;
const stops: (() => void)[] = [];

const demoIssues = ref<HealthIssue[]>(
  categories.map(({ id }, index) => ({
    key: `${id}-demo`,
    mediaId: id === "operations" ? null : `${index + 1}`,
    path:
      id === "operations"
        ? ""
        : `C:\\Media\\${["missing-image.png", "", "preview-image.webp", "untitled-video.mp4"][index]}`,
    kind: id === "metadata" ? "video" : "image",
    reason:
      id === "metadata"
        ? "technical"
        : id === "operations"
          ? "move"
          : id === "previews"
            ? "Preview unavailable"
            : "missing",
    technical: id === "metadata",
    collection: "library",
  })),
);

function filename(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function reason(issue: HealthIssue) {
  const key = `healthReason-${issue.reason}`;

  return [
    "missing",
    "move",
    "replacement",
    "technical",
    "annotations",
    "source",
  ].includes(issue.reason)
    ? t(key)
    : ui(issue.reason);
}

async function refresh() {
  if (busy.value || disposed) return;

  const generation = ++request;
  loading.value = true;
  error.value = "";

  try {
    const next = native
      ? await invoke<HealthPage>("get_library_health", {
          category: category.value,
          offset: offset.value,
        })
      : {
          counts: categories.map(
            ({ id }) =>
              demoIssues.value.filter((issue) => issue.key.startsWith(id))
                .length,
          ) as HealthPage["counts"],
          total: demoIssues.value.filter((issue) =>
            issue.key.startsWith(category.value),
          ).length,
          issues: demoIssues.value.filter((issue) =>
            issue.key.startsWith(category.value),
          ),
        };

    if (generation !== request || disposed) return;

    if (offset.value >= next.total && offset.value > 0) {
      offset.value = Math.max(
        0,
        Math.floor(Math.max(0, next.total - 1) / 100) * 100,
      );
      await refresh();

      return;
    }

    page.value = next;

    if (!next.issues.some((issue) => issue.key === selectedKey.value)) {
      selectedKey.value = next.issues[0]?.key ?? "";
    } else if (!proposal.value) {
      void loadDetails();
    }
  } catch (cause) {
    if (generation === request && !disposed) error.value = ui(String(cause));
  } finally {
    if (generation === request && !disposed) loading.value = false;
  }
}

function chooseCategory(next: HealthCategory) {
  if (busy.value || loading.value) return;

  category.value = next;
  offset.value = 0;
  selectedKey.value = "";
  message.value = "";
  void refresh();
}

async function loadDetails() {
  const generation = ++detailRequest;
  const issue = selected.value;
  details.value = null;
  proposal.value = null;
  confirmed.value = false;
  detailLoading.value = Boolean(issue?.mediaId);

  if (!issue?.mediaId) return;

  try {
    const item = native
      ? healthMediaItem(
          await invoke<HealthMedia>("get_health_media", { id: issue.mediaId }),
        )
      : {
          id: `demo-health-${issue.mediaId}`,
          filename: filename(issue.path),
          path: issue.path,
          kind:
            issue.kind === "video" ? ("video" as const) : ("image" as const),
          extension: "png",
          preview: "",
          size: 1048576,
          width: 1920,
          height: 1080,
          modifiedAt: 0,
          createdAt: 0,
          favorite: true,
          copyrights: ["Demo collection"],
          artist: "Demo artist",
          characters: [],
          tags: ["favorite", "landscape"],
        };

    if (generation === detailRequest && !disposed) details.value = item;
  } catch (cause) {
    if (generation === detailRequest && !disposed)
      error.value = ui(String(cause));
  } finally {
    if (generation === detailRequest && !disposed) detailLoading.value = false;
  }
}

async function perform(action: () => Promise<void>) {
  if (busy.value || loading.value) return;

  busy.value = true;
  error.value = "";
  message.value = "";

  try {
    await action();
  } catch (cause) {
    error.value = ui(String(cause));
  } finally {
    busy.value = false;
    const actionError = error.value;
    await refresh();
    if (actionError) error.value = actionError;
  }
}

function repairDemo() {
  demoIssues.value = demoIssues.value.filter(
    (issue) => issue.key !== selectedKey.value,
  );
  message.value = t("healthDemoRepaired");
}

function checkFiles() {
  void perform(async () => {
    if (!native) {
      message.value = t("healthDemo");

      return;
    }

    const result = await invoke<{
      checked: number;
      missing: number;
      unavailable: number;
    }>("check_library_health");
    message.value = t("healthChecked", result);
    emit("changed");
  });
}

function locate() {
  const issue = selected.value;

  if (!issue?.mediaId) return;

  void perform(async () => {
    if (!native) {
      proposal.value = {
        token: "demo",
        path: "C:\\Media\\Found\\restored-image.png",
        consumesDiscovery: false,
      };

      return;
    }

    const path = await open({
      directory: false,
      multiple: false,
      title: t("healthLocate"),
    });

    if (typeof path !== "string") return;

    proposal.value = await invoke<RelinkProposal>("inspect_health_relink", {
      id: issue.mediaId,
      path,
    });
    confirmed.value = false;
  });
}

function relink() {
  const issue = selected.value;
  const choice = proposal.value;

  if (!issue?.mediaId || !choice || !confirmed.value) return;

  const id = issue.mediaId;

  void perform(async () => {
    if (!native) repairDemo();
    else {
      await invoke("confirm_health_relink", {
        id,
        path: choice.path,
        token: choice.token,
      });
      retryNativePreview(id);
      message.value = t("healthRelinked");
      emit("changed");
    }

    proposal.value = null;
  });
}

function retry() {
  const issue = selected.value;

  if (!issue) return;

  void perform(async () => {
    if (!native) {
      repairDemo();

      return;
    }

    if (category.value === "operations") {
      await invoke("retry_disk_sync");
      message.value = t("healthRecoveryRequested");
    } else if (issue.mediaId) {
      if (category.value === "previews") {
        // Reset client failure suppression even when the native retry still fails.
        try {
          await invoke<ArrayBuffer>("get_stage_preview", { id: issue.mediaId });
        } finally {
          retryNativePreview(issue.mediaId);
        }
      } else {
        await invoke("retry_health_metadata", { id: issue.mediaId });
      }

      message.value = t("healthRetryCompleted");
      emit("changed");
    }
  });
}

function changePage(delta: number) {
  offset.value = Math.max(0, offset.value + delta * 100);
  void refresh();
}

function editMetadata() {
  if (!details.value || !selected.value) return;

  if (!native) {
    repairDemo();
    void refresh();

    return;
  }

  emit("edit", details.value, selected.value.collection);
}

watch(selectedKey, () => void loadDetails());
watch(
  () => props.revision,
  () => void refresh(),
);

onMounted(async () => {
  await refresh();

  if (!native || disposed) return;

  for (const event of [
    "library-health-changed",
    "stage-scan-status",
    "disk-sync-status",
  ]) {
    try {
      const stop = await listen<{ running?: boolean }>(event, ({ payload }) => {
        if (!payload?.running) void refresh();
      });

      if (disposed) stop();
      else stops.push(stop);
    } catch {
      if (!disposed) message.value = t("healthRefreshHint");
    }
  }
});

onBeforeUnmount(() => {
  disposed = true;
  ++request;
  ++detailRequest;
  stops.forEach((stop) => stop());
});
</script>

<template>
  <section class="health-panel" :aria-label="t('libraryHealth')">
    <header class="health-header">
      <div>
        <span class="health-eyebrow">{{ t("healthEyebrow") }}</span>
        <h1>{{ t("libraryHealth") }}</h1>
        <p>{{ t("healthDescription") }}</p>
      </div>
      <div class="health-actions">
        <button
          class="secondary-button"
          :disabled="busy || loading"
          @click="refresh"
        >
          <AppIcon name="reset" :size="16" />{{ t("healthRefresh") }}
        </button>
        <button
          class="primary-button"
          :disabled="busy || loading"
          @click="checkFiles"
        >
          <AppIcon :name="busy ? 'loader' : 'size'" :size="16" />{{
            t("healthCheck")
          }}
        </button>
      </div>
    </header>
    <p v-if="!native" class="health-notice">{{ t("healthDemo") }}</p>
    <div
      class="health-categories"
      role="group"
      :aria-label="t('healthCategories')"
    >
      <button
        v-for="(option, index) in categories"
        :key="option.id"
        :class="{ selected: category === option.id }"
        :aria-pressed="category === option.id"
        :disabled="busy || loading"
        @click="chooseCategory(option.id)"
      >
        <AppIcon :name="option.icon" :size="16" />
        <span>{{ t(`healthCategory-${option.id}`) }}</span
        ><strong>{{ page.counts[index] }}</strong>
      </button>
    </div>
    <div class="health-feedback" aria-live="polite">
      <p v-if="error" class="error-message" role="alert">{{ error }}</p>
      <p v-else-if="message" role="status">{{ message }}</p>
      <p v-else-if="loading">{{ t("loading") }}</p>
      <p v-else>{{ t("healthProblemCount", { count: totalProblems }) }}</p>
    </div>
    <div class="health-body">
      <section
        class="health-list"
        :aria-label="t(`healthCategory-${category}`)"
      >
        <header class="health-list-header">
          <div>
            <h2>{{ t(`healthCategory-${category}`) }}</h2>
            <p>{{ t(`healthHint-${category}`) }}</p>
          </div>
          <span>{{ page.total }}</span>
        </header>
        <div class="health-issues" :aria-busy="loading">
          <button
            v-for="issue in page.issues"
            :key="issue.key"
            class="health-issue"
            :class="{ selected: selectedKey === issue.key }"
            :aria-pressed="selectedKey === issue.key"
            :disabled="busy || loading"
            @click="selectedKey = issue.key"
          >
            <span class="health-issue-icon"
              ><AppIcon :name="issue.kind || 'structure'" :size="22"
            /></span>
            <span class="health-issue-caption"
              ><strong>{{
                issue.path ? filename(issue.path) : t("healthRecovery")
              }}</strong>
              <span :title="issue.path">{{ issue.path || reason(issue) }}</span
              ><small>{{ reason(issue) }}</small></span
            >
            <AppIcon name="arrow" :size="16" />
          </button>
          <div v-if="!page.issues.length && !loading" class="health-empty">
            <AppIcon name="check-circle" :size="40" />
            <h3>{{ t("healthEmpty") }}</h3>
            <p>{{ t("healthEmptyHint") }}</p>
          </div>
        </div>
        <footer class="health-pagination">
          <span>{{
            t("healthPage", {
              first: page.total ? offset + 1 : 0,
              last: Math.min(offset + page.issues.length, page.total),
              total: page.total,
            })
          }}</span>
          <button
            class="secondary-button"
            :disabled="busy || loading || !offset"
            @click="changePage(-1)"
          >
            {{ t("healthPrevious") }}
          </button>
          <button
            class="secondary-button"
            :disabled="
              busy || loading || offset + page.issues.length >= page.total
            "
            @click="changePage(1)"
          >
            {{ t("healthNext") }}
          </button>
        </footer>
      </section>
      <aside class="health-detail" :aria-label="t('healthDetails')">
        <template v-if="selected">
          <span class="health-eyebrow">{{ t("healthDetails") }}</span>
          <h2>
            {{ selected.path ? filename(selected.path) : t("healthRecovery") }}
          </h2>
          <p class="health-path">{{ selected.path }}</p>
          <p class="health-reason">
            <AppIcon name="warning" :size="18" />{{ reason(selected) }}
          </p>
          <p v-if="detailLoading">{{ t("loading") }}</p>
          <dl v-else-if="details" class="health-metadata">
            <div>
              <dt>{{ t("healthArtist") }}</dt>
              <dd>{{ details.artist || "—" }}</dd>
            </div>
            <div>
              <dt>{{ t("healthCopyrights") }}</dt>
              <dd>{{ details.copyrights.join("; ") || "—" }}</dd>
            </div>
            <div>
              <dt>{{ t("healthCharacters") }}</dt>
              <dd>
                {{
                  details.characterCopyrights
                    ?.map((entry) => `${entry.name} (${entry.copyright})`)
                    .join("; ") || "—"
                }}
              </dd>
            </div>
            <div>
              <dt>{{ t("healthTags") }}</dt>
              <dd>{{ details.tags.join("; ") || "—" }}</dd>
            </div>
            <div v-if="details.kind === 'video'">
              <dt>{{ t("healthTitle") }}</dt>
              <dd>{{ details.videoTitle || "—" }}</dd>
            </div>
            <div>
              <dt>{{ t("healthFavorite") }}</dt>
              <dd>{{ details.favorite ? t("healthYes") : t("healthNo") }}</dd>
            </div>
          </dl>
          <div class="health-repairs">
            <template v-if="category === 'missing'">
              <p>{{ t("healthMissingHelp") }}</p>
              <button
                class="primary-button"
                :disabled="busy || loading"
                @click="locate"
              >
                <AppIcon name="staging" :size="16" />{{ t("healthLocate") }}
              </button>
            </template>
            <template v-else-if="category === 'operations'">
              <button
                v-if="selected.reason !== 'source'"
                class="primary-button"
                :disabled="busy || loading"
                @click="retry"
              >
                {{ t("healthRetryRecovery") }}
              </button>
              <button
                class="secondary-button"
                :disabled="busy"
                @click="emit('indexing')"
              >
                {{ t("healthIndexingSettings") }}
              </button>
            </template>
            <button
              v-else-if="category === 'previews'"
              class="primary-button"
              :disabled="busy || loading"
              @click="retry"
            >
              {{ t("healthRetryPreview") }}
            </button>
            <template v-else>
              <button
                v-if="selected.technical"
                class="primary-button"
                :disabled="busy || loading"
                @click="retry"
              >
                {{ t("healthRetryMetadata") }}
              </button>
              <button
                class="secondary-button"
                :disabled="busy || loading || !details"
                @click="editMetadata"
              >
                {{
                  selected.collection === "stage"
                    ? t("healthDistribute")
                    : t("healthEditMetadata")
                }}
              </button>
            </template>
          </div>
          <section
            v-if="proposal"
            class="health-confirmation"
            :aria-label="t('healthRelinkConfirmation')"
          >
            <h3>{{ t("healthRelinkConfirmation") }}</h3>
            <p class="health-path">{{ proposal.path }}</p>
            <p>{{ t("healthRelinkHelp") }}</p>
            <p v-if="proposal.consumesDiscovery" class="health-notice">
              {{ t("healthConsumesDiscovery") }}
            </p>
            <label
              ><input v-model="confirmed" type="checkbox" :disabled="busy" />{{
                t("healthConfirmFile")
              }}</label
            >
            <div class="health-actions">
              <button
                class="secondary-button"
                :disabled="busy"
                @click="proposal = null"
              >
                {{ t("cancel") }}
              </button>
              <button
                class="primary-button"
                :disabled="busy || !confirmed"
                @click="relink"
              >
                {{ t("healthRelink") }}
              </button>
            </div>
          </section>
        </template>
        <div v-else class="health-empty">
          <AppIcon name="info" :size="32" />
          <p>{{ t("healthChooseIssue") }}</p>
        </div>
      </aside>
    </div>
  </section>
</template>

<style scoped>
.health-panel {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  overflow: hidden;
  background: linear-gradient(110deg, var(--surface), var(--surface-alt));
  border: 1px solid var(--line);
  border-radius: 10px;
}
.health-header,
.health-actions,
.health-list-header,
.health-pagination {
  display: flex;
  align-items: center;
  gap: 12px;
}
.health-header {
  justify-content: space-between;
  flex-wrap: wrap;
}
.health-eyebrow {
  color: var(--muted);
  font-size: 12px;
  font-weight: 500;
}
h1 {
  font-size: 20px;
  margin: 4px 0 8px;
}
h2 {
  font-size: 16px;
  margin: 0;
}
h3 {
  font-size: 14px;
  margin: 0;
}
p {
  margin: 5px 0;
  color: var(--muted);
  line-height: 1.5;
}
.health-categories {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.health-categories button {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 36px;
  padding: 8px 12px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface-raised);
  text-align: left;
}
.health-categories button.selected {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.health-categories button:hover {
  border-color: var(--accent-line);
}
.health-categories strong {
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--surface-hover);
  color: var(--muted);
  font-size: 11px;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}
.health-categories span {
  min-width: 0;
  overflow-wrap: anywhere;
  line-height: 1.4;
}
.health-categories svg {
  flex-shrink: 0;
  color: var(--muted);
}
.health-feedback {
  min-height: 24px;
}
.health-feedback p {
  margin: 0;
}
.health-feedback .error-message {
  color: var(--danger);
}
.health-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1.25fr) minmax(290px, 0.8fr);
  gap: 16px;
}
.health-list,
.health-detail {
  min-height: 0;
  min-width: 0;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
}
.health-list {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.health-list-header {
  justify-content: space-between;
  padding: 16px;
  border-bottom: 1px solid var(--line);
}
.health-list-header p {
  font-size: 12px;
}
.health-list-header > span {
  color: var(--muted);
}
.health-issues {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 8px;
}
.health-issue {
  width: 100%;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 12px;
  text-align: left;
  padding: 12px;
  border: 1px solid transparent;
  border-radius: 8px;
}
.health-issue:hover {
  background: var(--surface-hover);
}
.health-issue.selected {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.health-issue-icon {
  padding: 8px;
  border-radius: 6px;
  background: var(--surface-raised);
  color: var(--accent);
}
.health-issue-caption {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.health-issue-caption > * {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.health-issue-caption > span {
  color: var(--muted);
  font-size: 11px;
}
.health-issue-caption > small {
  color: var(--warning);
  font-size: 11px;
}
.health-pagination {
  padding: 12px 16px;
  border-top: 1px solid var(--line);
}
.health-pagination > span {
  flex: 1;
  font-size: 11px;
  color: var(--muted);
}
.health-detail {
  padding: 16px;
  overflow: auto;
}
.health-detail > h2 {
  margin: 12px 0 8px;
  overflow-wrap: anywhere;
}
.health-path {
  overflow-wrap: anywhere;
  font-size: 12px;
}
.health-reason {
  display: flex;
  gap: 8px;
  align-items: start;
  padding: 12px 0;
  color: var(--warning);
}
.health-metadata {
  display: grid;
  gap: 12px;
  padding: 16px 0;
  border-top: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
}
.health-metadata div {
  display: grid;
  gap: 4px;
}
dt {
  color: var(--muted);
  font-size: 11px;
}
dd {
  margin: 0;
  overflow-wrap: anywhere;
  line-height: 1.5;
}
.health-repairs {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}
.health-repairs p {
  width: 100%;
}
.health-confirmation {
  display: grid;
  gap: 12px;
  margin-top: 16px;
  padding: 16px;
  border: 1px solid var(--accent-line);
  border-radius: 8px;
  background: var(--surface-alt);
}
.health-confirmation label {
  display: flex;
  align-items: start;
  gap: 8px;
  line-height: 1.5;
}
.health-confirmation input {
  margin-top: 3px;
}
.health-empty {
  display: grid;
  align-content: center;
  justify-items: center;
  text-align: center;
  min-height: 220px;
  height: 100%;
  gap: 12px;
  padding: 16px;
  color: var(--muted);
}
.health-empty svg {
  color: var(--accent);
}
.health-notice {
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--info-bg);
  color: var(--info);
}
@media (max-width: 1150px) {
  .health-panel {
    gap: 12px;
  }
  .health-header {
    align-items: start;
  }
  .health-header > div:first-child {
    flex: 1;
    min-width: 240px;
  }
}
</style>

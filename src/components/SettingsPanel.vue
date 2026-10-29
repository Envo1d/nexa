<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { isNativeData } from "../runtime";
import { translateUi as ui } from "../i18n/presentation";
import { defaultConfig } from "../types/config";
import { version } from "../../package.json";
import ToggleSwitch from "./ToggleSwitch.vue";
import SettingsRow from "./SettingsRow.vue";
import CatalogSettings from "./CatalogSettings.vue";
import AppIcon from "./AppIcon.vue";
import UpdateConfirmDialog from "./UpdateConfirmDialog.vue";
import AppSelect from "./AppSelect.vue";
import { useStageScan } from "../composables/useStageScan";
import { useCacheUsage } from "../composables/useCacheUsage";
import { useUpdater } from "../composables/useUpdater";
import { applyLocale, localePreference, languageOptions } from "../i18n";
import { applyTheme, themeIds, themePreference } from "../themes";

import type { ConfigSnapshot } from "../types/config";

const { t } = useI18n();

const props = defineProps<{ sectionRequest?: { id: string } | null }>();

const updater = useUpdater();

onUnmounted(updater.cancelInstall);

function answerUpdate(accepted: boolean) {
  if (accepted) void updater.confirmInstall();
  else updater.cancelInstall();
}

const cacheUsage = useCacheUsage();

const cacheMessage = ref("");

async function clearCache() {
  if (!native) {
    emit("preview", t("clearCache"));

    return;
  }

  cacheMessage.value = "";

  try {
    const result = await cacheUsage.clear();

    if (result) {
      cacheMessage.value = result.failedRemovals
        ? t("cacheClearPartial", { count: result.failedRemovals })
        : t("cacheCleared");
    }
  } catch {
    cacheMessage.value = t("cacheClearError");
  }
}

const stageScan = useStageScan();

async function scanStage(library = false) {
  if (!native) {
    emit("preview", library ? t("scanLibrary") : t("scanStage"));

    return;
  }

  error.value = "";

  try {
    await stageScan.start(library);
  } catch (cause) {
    error.value = String(cause);
  }
}

const emit = defineEmits<{ preview: [action: string]; close: []; saved: [] }>();

const native = isNativeData();

const draft = ref({
  ...defaultConfig(),
  locale: localePreference.value,
  theme: themePreference.value,
});

const themeOptions = computed(() =>
  themeIds.map((value) => ({
    value,
    label: t(value === "midnight-violet" ? "themeName" : `theme-${value}`),
  })),
);

const paths = ref<ConfigSnapshot["paths"] | null>(null);
const loading = ref(native);
const ready = ref(!native);
const busy = ref(false);
const error = ref("");
const saved = ref(false);

const legacyFound = ref(false);
const importing = ref(false);
const importMessage = ref("");
const importError = ref("");

async function checkLegacyDatabase() {
  if (!native) return;

  try {
    legacyFound.value = await invoke<boolean>("find_legacy_database");
  } catch (cause) {
    legacyFound.value = false;
    importError.value = t("legacyCheckError", { error: ui(String(cause)) });
  }
}

async function importLegacyDatabase() {
  importing.value = true;
  busy.value = true;
  importError.value = "";
  importMessage.value = t("importingLegacy");

  try {
    const result = await invoke<{
      libraryAdded: number;
      stageAdded: number;
      skipped: number;
    }>("import_legacy_database");

    importMessage.value = t("importResult", result);
  } catch (cause) {
    importMessage.value = "";
    importError.value = t("importError", { error: ui(String(cause)) });

    await checkLegacyDatabase();
  } finally {
    importing.value = false;
    busy.value = false;
  }
}

watch(
  draft,
  () => {
    saved.value = false;
  },
  { deep: true, flush: "sync" },
);

onMounted(async () => {
  if (!native) return;

  try {
    await stageScan.connect();
  } catch (cause) {
    error.value = String(cause);
  }

  await checkLegacyDatabase();

  try {
    const snapshot = await invoke<ConfigSnapshot>("get_config");

    draft.value = snapshot.settings;
    paths.value = snapshot.paths;
    ready.value = true;
  } catch (cause) {
    error.value = t("loadSettingsError", { error: ui(String(cause)) });
  } finally {
    loading.value = false;
  }
});

async function browse(section: string) {
  if (!native) {
    emit("preview", `Browse ${section} directory`);

    return;
  }

  busy.value = true;
  error.value = "";

  try {
    const key = section === "library" ? "libraryPath" : "stagingPath";

    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: draft.value[key] ?? undefined,
    });

    if (typeof selected === "string") {
      draft.value[key] = selected;
      saved.value = false;
    }
  } catch (cause) {
    error.value = t("chooseDirectoryError", { error: ui(String(cause)) });
  } finally {
    busy.value = false;
  }
}

async function saveChanges() {
  if (!native) {
    applyTheme(draft.value.theme);

    await applyLocale(draft.value.locale);

    saved.value = true;

    return;
  }

  busy.value = true;
  error.value = "";
  saved.value = false;

  try {
    await invoke("save_config", { settings: { ...draft.value } });

    applyTheme(draft.value.theme);

    await applyLocale(draft.value.locale);

    saved.value = true;
    emit("saved");
  } catch (cause) {
    error.value = t("saveSettingsError", { error: ui(String(cause)) });
  } finally {
    busy.value = false;
  }
}

async function openLogs() {
  if (!native) {
    emit("preview", "Open logs folder");

    return;
  }

  try {
    await invoke("open_logs");
  } catch (cause) {
    error.value = t("openLogsError", { error: ui(String(cause)) });
  }
}

const scrollArea = ref<HTMLElement | null>(null);
const activeSection = ref("library");

const sections = computed(() => [
  {
    id: "library",
    label: t("library"),
    icon: "gallery",
    description: t("libraryDescription"),
  },
  {
    id: "staging",
    label: t("staging"),
    icon: "staging",
    description: t("stagingDescription"),
  },
  {
    id: "structure",
    label: t("structure"),
    icon: "structure",
    description: t("structureDescription"),
  },
  {
    id: "indexing",
    label: t("indexing"),
    icon: "size",
    description: t("indexingDescription"),
  },
  {
    id: "cache",
    label: t("cache"),
    icon: "cache",
    description: t("cacheDescription"),
  },
  {
    id: "catalog",
    label: t("catalogSection"),
    icon: "copy",
    description: t("catalogDescription"),
  },
  {
    id: "logs",
    label: t("logs"),
    icon: "logs",
    description: t("logsDescription"),
  },
  {
    id: "updates",
    label: t("updates"),
    icon: "reset",
    description: t("updatesDescription"),
  },
  {
    id: "appearance",
    label: t("appearance"),
    icon: "settings",
    description: t("appearanceDescription"),
  },
]);

function goToSection(id: string) {
  if (id === "indexing" && !importing.value) void checkLegacyDatabase();

  const area = scrollArea.value;

  const target = area?.querySelector<HTMLElement>(`#settings-${id}`);

  if (!area || !target) return;

  area.scrollTo({
    top: target.offsetTop - 24,
    behavior: "instant",
  });
  activeSection.value = id;
}

function trackSection() {
  const area = scrollArea.value;

  if (!area) return;

  if (area.scrollTop + area.clientHeight >= area.scrollHeight - 2) {
    activeSection.value = "appearance";

    return;
  }

  for (const section of sections.value) {
    const element = area.querySelector<HTMLElement>(`#settings-${section.id}`);

    if (element && element.offsetTop <= area.scrollTop + 80)
      activeSection.value = section.id;
  }
}

watch(
  [() => props.sectionRequest, loading],
  async ([request, isLoading]) => {
    if (!request || isLoading) return;

    await nextTick();

    goToSection(request.id);
    scrollArea.value
      ?.querySelector<HTMLElement>(`#settings-${request.id} button`)
      ?.focus({ preventScroll: true });
  },
  { immediate: true, flush: "post" },
);

function resetDrafts() {
  draft.value = defaultConfig();
  saved.value = false;
}
</script>

<template>
  <div class="settings-workspace">
    <aside class="settings-sidebar" :aria-label="t('sections')">
      <div class="settings-sidebar-heading">
        {{ t("settings") }}<span v-if="!native">{{ t("preview") }}</span>
      </div>
      <nav class="settings-section-nav">
        <button
          v-for="section in sections"
          :key="section.id"
          :class="{ active: activeSection === section.id }"
          :aria-current="activeSection === section.id ? 'location' : undefined"
          :aria-controls="`settings-${section.id}`"
          @click="goToSection(section.id)"
        >
          <AppIcon :name="section.icon" :size="18" />{{ section.label }}
        </button>
      </nav>
    </aside>
    <div
      ref="scrollArea"
      class="settings-scroll"
      @scroll.passive="trackSection"
    >
      <div class="settings-content">
        <p class="settings-notice" role="status">
          {{
            ui(error) ||
            (loading
              ? t("loadingSettings")
              : !native
                ? t("previewNotice")
                : saved
                  ? t("saved")
                  : t("saveNotice"))
          }}
        </p>
        <fieldset
          :disabled="loading || busy || !ready"
          style="border: 0; padding: 0; margin: 0; min-width: 0"
        >
          <section
            v-for="section in sections"
            :id="`settings-${section.id}`"
            :key="section.id"
            class="settings-section"
            :aria-labelledby="`settings-title-${section.id}`"
          >
            <header class="settings-section-heading">
              <h2 :id="`settings-title-${section.id}`">
                <AppIcon :name="section.icon" :size="24" />{{ section.label }}
              </h2>
              <p>{{ section.description }}</p>
            </header>
            <div class="settings-card">
              <SettingsRow
                v-if="section.id === 'library' || section.id === 'staging'"
                :title="
                  section.id === 'library' ? t('libraryPath') : t('stagingPath')
                "
                :description="
                  (section.id === 'library'
                    ? draft.libraryPath
                    : draft.stagingPath) || t('notSelected')
                "
              >
                <button class="secondary-button" @click="browse(section.id)">
                  <AppIcon name="gallery" :size="16" />{{ t("browse") }}
                </button>
              </SettingsRow>
              <template v-else-if="section.id === 'structure'">
                <ToggleSwitch
                  v-model="draft.copyrightDirectory"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>{{ t("copyrightDirectory") }}</strong
                    ><small>{{ t("copyrightDescription") }}</small></span
                  ></ToggleSwitch
                >
                <ToggleSwitch
                  v-model="draft.artistDirectory"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>{{ t("artistDirectory") }}</strong
                    ><small>{{ t("artistDescription") }}</small></span
                  ></ToggleSwitch
                >
                <SettingsRow
                  for="character-separator"
                  as="label"
                  :title="t('characterSeparator')"
                  :description="t('separatorDescription')"
                  ><input
                    id="character-separator"
                    v-model="draft.characterSeparator"
                    type="text"
                    spellcheck="false"
                /></SettingsRow>
                <SettingsRow
                  for="video-subfolder"
                  as="label"
                  :title="t('videoFolder')"
                  :description="t('videoFolderDescription')"
                  ><input
                    id="video-subfolder"
                    v-model="draft.videoSubfolder"
                    type="text"
                    spellcheck="false"
                /></SettingsRow>
                <SettingsRow
                  for="gif-subfolder"
                  as="label"
                  :title="t('gifFolder')"
                  :description="t('gifDescription')"
                  ><input
                    id="gif-subfolder"
                    v-model="draft.gifSubfolder"
                    type="text"
                    spellcheck="false"
                /></SettingsRow>
                <ToggleSwitch
                  v-model="draft.syncStructure"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>{{ t("sync") }}</strong
                    ><small>{{ t("syncDescription") }}</small></span
                  ></ToggleSwitch
                >
              </template>
              <template v-else-if="section.id === 'indexing'">
                <div class="settings-card-row settings-actions">
                  <button
                    class="secondary-button"
                    :disabled="stageScan.status.value.running"
                    @click="scanStage(true)"
                  >
                    <AppIcon name="size" :size="16" />{{
                      t("scanLibrary")
                    }}</button
                  ><button
                    class="secondary-button"
                    :disabled="stageScan.status.value.running"
                    @click="scanStage(false)"
                  >
                    <AppIcon name="search" :size="16" />{{ t("scanStage") }}
                  </button>
                </div>
                <ToggleSwitch
                  v-model="draft.autoScan"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>{{ t("autoScan") }}</strong
                    ><small>{{ t("autoScanDescription") }}</small></span
                  ></ToggleSwitch
                >
                <div class="settings-scan-summary">
                  <div>
                    <AppIcon name="info" :size="16" /><strong>{{
                      t("lastScan")
                    }}</strong
                    ><span class="settings-status">{{
                      stageScan.status.value.running ? t("scanning") : t("idle")
                    }}</span>
                  </div>
                  <p role="status" aria-live="polite">
                    {{ ui(stageScan.status.value.message) }} ·
                    {{ t("scanCounts", stageScan.status.value) }}
                  </p>
                </div>
                <SettingsRow
                  v-if="legacyFound"
                  :title="t('importTitle')"
                  :description="t('importDescription')"
                >
                  <button
                    class="secondary-button"
                    :disabled="importing"
                    @click="importLegacyDatabase"
                  >
                    <AppIcon name="size" :size="16" />
                    {{ importing ? t("importing") : t("importButton") }}
                  </button>
                </SettingsRow>
                <div
                  v-if="legacyFound || importMessage || importError"
                  class="settings-scan-summary"
                >
                  <p v-if="legacyFound">{{ t("importNotice") }}</p>
                  <p v-if="importMessage" role="status" aria-live="polite">
                    {{ ui(importMessage) }}
                  </p>
                  <p v-if="importError" role="alert">{{ ui(importError) }}</p>
                </div>
              </template>
              <CatalogSettings
                v-else-if="section.id === 'catalog'"
                @working="busy = $event"
                @restored="emit('saved')"
              />
              <template v-else-if="section.id === 'cache'">
                <div class="settings-card-row settings-cache-row">
                  <div class="settings-summary">
                    <span
                      ><strong>{{ t("thumbnailCache") }}</strong
                      ><small>{{
                        ui(cacheUsage.error.value) || t("cacheLimit")
                      }}</small></span
                    ><strong>{{ ui(cacheUsage.label.value) }}</strong>
                  </div>
                  <progress
                    class="settings-progress"
                    :class="`cache-${cacheUsage.pressure.value}`"
                    :value="
                      cacheUsage.status.value
                        ? Math.min(
                            cacheUsage.status.value.bytes,
                            cacheUsage.status.value.maxBytes,
                          )
                        : undefined
                    "
                    :max="cacheUsage.status.value?.maxBytes ?? 536870912"
                    :aria-label="t('cacheUsage')"
                    :aria-valuetext="ui(cacheUsage.label.value)"
                  >
                    {{ ui(cacheUsage.label.value) }}
                  </progress>
                </div>
                <SettingsRow
                  :title="t('clearCache')"
                  :description="t('clearDescription')"
                  ><button
                    class="secondary-button"
                    :disabled="cacheUsage.clearing.value"
                    :aria-busy="cacheUsage.clearing.value"
                    @click="clearCache"
                  >
                    <AppIcon name="reset" :size="16" />{{
                      t(
                        cacheUsage.clearing.value
                          ? "clearingCache"
                          : "clearCache",
                      )
                    }}
                  </button>
                </SettingsRow>
                <p v-if="cacheMessage" class="settings-notice" role="status">
                  {{ cacheMessage }}
                </p>
              </template>
              <SettingsRow
                v-else-if="section.id === 'logs'"
                :title="t('applicationLogs')"
                :description="paths?.logsPath || t('logsMissing')"
                ><button class="secondary-button" @click="openLogs">
                  <AppIcon name="gallery" :size="16" />{{ t("openLogs") }}
                </button>
              </SettingsRow>
              <template v-else-if="section.id === 'updates'">
                <SettingsRow
                  :title="t('version')"
                  :description="ui(`Nexa ${version}`)"
                  ><span class="settings-status" role="status">{{
                    ui(updater.status.value)
                  }}</span>
                </SettingsRow>
                <ToggleSwitch
                  v-model="draft.autoUpdateCheck"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>{{ t("autoUpdate") }}</strong
                    ><small>{{ t("autoUpdateDescription") }}</small></span
                  ></ToggleSwitch
                >
                <SettingsRow
                  :title="t('checkNow')"
                  :description="t('checkDescription')"
                  ><button
                    class="secondary-button"
                    :disabled="updater.busy.value"
                    @click="updater.check"
                  >
                    <AppIcon name="reset" :size="16" />{{ t("checkNow") }}
                  </button>
                </SettingsRow>
                <SettingsRow
                  v-if="updater.available.value"
                  :title="t('installUpdate')"
                  :description="
                    updater.available.value.notes || t('installNotice')
                  "
                >
                  <button
                    class="secondary-button"
                    :disabled="updater.busy.value"
                    @click="updater.install"
                  >
                    {{
                      t("installVersion", {
                        version: updater.available.value.version,
                      })
                    }}
                  </button>
                </SettingsRow>
                <progress
                  v-if="updater.busy.value && updater.percent.value !== null"
                  class="settings-progress"
                  :value="updater.percent.value ?? undefined"
                  max="100"
                  :aria-label="t('downloadProgress')"
                />
              </template>
              <template v-else>
                <SettingsRow
                  :title="t('language')"
                  :description="t('languageDescription')"
                >
                  <AppSelect
                    v-model="draft.locale"
                    class="settings-select"
                    :options="languageOptions"
                    :label="t('language')"
                    :disabled="loading || busy || !ready"
                  />
                </SettingsRow>
                <SettingsRow
                  :title="t('theme')"
                  :description="t('themeDescription')"
                >
                  <AppSelect
                    v-model="draft.theme"
                    class="settings-select settings-theme-select"
                    :options="themeOptions"
                    :label="t('theme')"
                    :disabled="loading || busy || !ready"
                  />
                </SettingsRow>
              </template>
            </div>
          </section>
          <footer class="settings-footer">
            <button class="text-button" @click="resetDrafts">
              <AppIcon name="reset" :size="15" />{{ t("resetDefaults") }}
            </button>
            <div>
              <button class="secondary-button" @click="$emit('close')">
                {{ t("cancel") }}</button
              ><button class="primary-button" @click="saveChanges">
                <AppIcon name="check" :size="15" />{{ t("saveChanges") }}
              </button>
            </div>
          </footer>
        </fieldset>
      </div>
    </div>
  </div>
  <UpdateConfirmDialog
    v-if="updater.confirmation.value"
    :version="updater.confirmation.value.version"
    @answer="answerUpdate"
  />
</template>

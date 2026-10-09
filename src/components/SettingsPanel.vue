<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { defaultConfig, type ConfigSnapshot } from "../types/config";
import { version } from "../../package.json";
import ToggleSwitch from "./ToggleSwitch.vue";
import SettingsRow from "./SettingsRow.vue";
import AppIcon from "./AppIcon.vue";
import { useStageScan } from "../composables/useStageScan";
const stageScan = useStageScan();
async function scanStage() {
  if (!native) {
    emit("preview", "Scan Stage folder");
    return;
  }
  error.value = "";
  try {
    await stageScan.start();
  } catch (cause) {
    error.value = String(cause);
  }
}

const emit = defineEmits<{ preview: [action: string]; close: []; saved: [] }>();
const native = isTauri();
const draft = ref(defaultConfig());
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
    importError.value = `Could not check for old Nexa data: ${String(cause)}`;
  }
}

async function importLegacyDatabase() {
  importing.value = true;
  busy.value = true;
  importError.value = "";
  importMessage.value = "Importing old Nexa data…";
  try {
    const result = await invoke<{
      libraryAdded: number;
      stageAdded: number;
      skipped: number;
    }>("import_legacy_database");
    importMessage.value = `Import complete: ${result.libraryAdded} Library files and ${result.stageAdded} Stage files added. ${result.skipped} existing files skipped.`;
  } catch (cause) {
    importMessage.value = "";
    importError.value = `Import failed: ${String(cause)}`;
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
    error.value = `Could not load settings: ${String(cause)}`;
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
    error.value = `Could not choose directory: ${String(cause)}`;
  } finally {
    busy.value = false;
  }
}
async function saveChanges() {
  if (!native) {
    emit("preview", "Save changes");
    return;
  }
  busy.value = true;
  error.value = "";
  saved.value = false;
  try {
    await invoke("save_config", { settings: { ...draft.value } });
    saved.value = true;
    emit("saved");
  } catch (cause) {
    error.value = `Could not save settings: ${String(cause)}`;
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
    error.value = `Could not open logs: ${String(cause)}`;
  }
}
const scrollArea = ref<HTMLElement | null>(null);
const activeSection = ref("library");
const sections = [
  {
    id: "library",
    label: "Library",
    icon: "gallery",
    description: "Choose the directory for your sorted media library.",
  },
  {
    id: "staging",
    label: "Staging",
    icon: "staging",
    description: "Choose the staging directory for incoming media.",
  },
  {
    id: "structure",
    label: "Structure",
    icon: "structure",
    description: "Configure how your library is organized on disk.",
  },
  {
    id: "indexing",
    label: "Indexing",
    icon: "size",
    description: "Scan your library and staging directories.",
  },
  {
    id: "cache",
    label: "Cache",
    icon: "cache",
    description: "Manage cached thumbnails and disk usage.",
  },
  {
    id: "logs",
    label: "Logs",
    icon: "logs",
    description: "Access application logs.",
  },
  {
    id: "updates",
    label: "Updates",
    icon: "reset",
    description: "Keep track of the current application version.",
  },
  {
    id: "appearance",
    label: "Appearance",
    icon: "settings",
    description: "Personalize your workspace.",
  },
];
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
  for (const section of sections) {
    const element = area.querySelector<HTMLElement>(`#settings-${section.id}`);
    if (element && element.offsetTop <= area.scrollTop + 80)
      activeSection.value = section.id;
  }
}
function resetDrafts() {
  draft.value = defaultConfig();
  saved.value = false;
}
</script>

<template>
  <div class="settings-workspace">
    <aside class="settings-sidebar" aria-label="Settings sections">
      <div class="settings-sidebar-heading">
        Settings <span v-if="!native">UI preview</span>
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
            error ||
            (loading
              ? "Loading settings…"
              : !native
                ? "UI preview · Changes are not applied or saved."
                : saved
                  ? "Settings saved."
                  : "Changes apply after Save changes. Stage scanning uses the saved directory. Library scanning, disk synchronization and updates are not available yet.")
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
                  section.id === 'library'
                    ? 'Sorted library path'
                    : 'Staging folder'
                "
                :description="
                  (section.id === 'library'
                    ? draft.libraryPath
                    : draft.stagingPath) || 'Not selected'
                "
              >
                <button class="secondary-button" @click="browse(section.id)">
                  <AppIcon name="gallery" :size="16" />Browse
                </button>
              </SettingsRow>
              <template v-else-if="section.id === 'structure'">
                <ToggleSwitch
                  v-model="draft.copyrightDirectory"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>Copyright directory</strong
                    ><small
                      >Include a copyright directory in the library
                      structure.</small
                    ></span
                  ></ToggleSwitch
                >
                <ToggleSwitch
                  v-model="draft.artistDirectory"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>Artist directory</strong
                    ><small
                      >Include an artist directory in the library
                      structure.</small
                    ></span
                  ></ToggleSwitch
                >
                <SettingsRow
                  for="character-separator"
                  as="label"
                  title="Character separator"
                  description="Separator between character names in filenames."
                  ><input
                    id="character-separator"
                    v-model="draft.characterSeparator"
                    type="text"
                    spellcheck="false"
                /></SettingsRow>
                <SettingsRow
                  for="video-subfolder"
                  as="label"
                  title="Video subfolder name"
                  description="Name of the subfolder for video files."
                  ><input
                    id="video-subfolder"
                    v-model="draft.videoSubfolder"
                    type="text"
                    spellcheck="false"
                /></SettingsRow>
                <ToggleSwitch
                  v-model="draft.syncStructure"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>Sync structure with disk</strong
                    ><small
                      >When off, folders will not be created from the configured
                      structure.</small
                    ></span
                  ></ToggleSwitch
                >
              </template>
              <template v-else-if="section.id === 'indexing'">
                <div class="settings-card-row settings-actions">
                  <button
                    class="secondary-button"
                    @click="$emit('preview', 'Scan library')"
                  >
                    <AppIcon name="size" :size="16" />Scan library</button
                  ><button
                    class="secondary-button"
                    :disabled="stageScan.status.value.running"
                    @click="scanStage"
                  >
                    <AppIcon name="search" :size="16" />Scan Stage folder
                  </button>
                </div>
                <ToggleSwitch
                  v-model="draft.autoScan"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>Auto scan on startup</strong
                    ><small
                      >Scan Stage each time Nexa opens. Library scanning is
                      pending.</small
                    ></span
                  ></ToggleSwitch
                >
                <div class="settings-scan-summary">
                  <div>
                    <AppIcon name="info" :size="16" /><strong>Last scan</strong
                    ><span class="settings-status">{{
                      stageScan.status.value.running ? "Scanning" : "Idle"
                    }}</span>
                  </div>
                  <p role="status" aria-live="polite">
                    {{ stageScan.status.value.message }} ·
                    {{ stageScan.status.value.scanned }} files ·
                    {{ stageScan.status.value.extracted }} extracted ·
                    {{ stageScan.status.value.errors }} issues
                  </p>
                </div>
                <SettingsRow
                  v-if="legacyFound"
                  title="Import from old Nexa"
                  description="Copy Library, Stage and metadata into the new database. Old data and media files stay unchanged. Existing files are skipped."
                >
                  <button
                    class="secondary-button"
                    :disabled="importing"
                    @click="importLegacyDatabase"
                  >
                    <AppIcon name="size" :size="16" />
                    {{ importing ? "Importing…" : "Import old database" }}
                  </button>
                </SettingsRow>
                <div
                  v-if="legacyFound || importMessage || importError"
                  class="settings-scan-summary"
                >
                  <p v-if="legacyFound">
                    Characters keep their copyright; tags are shared without
                    duplicates. Imported roots are retained without changing
                    your settings. Stage shows files from its saved directory
                    and redistribution queue. Library still shows demo files.
                  </p>
                  <p v-if="importMessage" role="status" aria-live="polite">
                    {{ importMessage }}
                  </p>
                  <p v-if="importError" role="alert">{{ importError }}</p>
                </div>
              </template>
              <template v-else-if="section.id === 'cache'">
                <div class="settings-card-row settings-cache-row">
                  <div class="settings-summary">
                    <span
                      ><strong>Thumbnail cache</strong
                      ><small>Current disk usage · Demo values</small></span
                    ><strong>128 / 512 MB</strong>
                  </div>
                  <progress
                    class="settings-cache-progress"
                    value="128"
                    max="512"
                    aria-label="Thumbnail cache usage"
                    aria-valuetext="128 of 512 MB, demo values"
                  >
                    25%
                  </progress>
                </div>
                <SettingsRow
                  title="Clear cache"
                  description="Remove cached thumbnails and temporary files."
                  ><button
                    class="secondary-button"
                    @click="$emit('preview', 'Clear thumbnail cache')"
                  >
                    <AppIcon name="reset" :size="16" />Clear cache
                  </button>
                </SettingsRow>
              </template>
              <SettingsRow
                v-else-if="section.id === 'logs'"
                title="Application logs"
                :description="paths?.logsPath || 'Logs folder: Not configured'"
                ><button class="secondary-button" @click="openLogs">
                  <AppIcon name="gallery" :size="16" />Open logs
                </button>
              </SettingsRow>
              <template v-else-if="section.id === 'updates'">
                <SettingsRow
                  title="Current version"
                  :description="`Nexa ${version}`"
                  ><span class="settings-status" role="status"
                    >Not checked</span
                  >
                </SettingsRow>
                <ToggleSwitch
                  v-model="draft.autoUpdateCheck"
                  trailing
                  class="settings-card-row settings-switch-row"
                  ><span
                    ><strong>Auto check on startup</strong
                    ><small
                      >Check for updates each time Nexa opens.</small
                    ></span
                  ></ToggleSwitch
                >
                <SettingsRow
                  title="Check now"
                  description="Manually check for a newer version."
                  ><button
                    class="secondary-button"
                    @click="$emit('preview', 'Check for updates')"
                  >
                    <AppIcon name="reset" :size="16" />Check now
                  </button>
                </SettingsRow>
              </template>
              <SettingsRow
                v-else
                for="settings-theme"
                as="label"
                title="Theme"
                description="More themes will be available later."
                ><select id="settings-theme" disabled>
                  <option>Midnight violet</option>
                </select></SettingsRow
              >
            </div>
          </section>
          <footer class="settings-footer">
            <button class="text-button" @click="resetDrafts">
              <AppIcon name="reset" :size="15" />Reset to defaults
            </button>
            <div>
              <button class="secondary-button" @click="$emit('close')">
                Cancel</button
              ><button class="primary-button" @click="saveChanges">
                <AppIcon name="check" :size="15" />Save changes
              </button>
            </div>
          </footer>
        </fieldset>
      </div>
    </div>
  </div>
</template>

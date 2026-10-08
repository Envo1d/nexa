<script setup lang="ts">
import { ref } from "vue";
import { version } from "../../package.json";
import ToggleSwitch from "./ToggleSwitch.vue";
import SettingsRow from "./SettingsRow.vue";
import AppIcon from "./AppIcon.vue";

defineEmits<{ preview: [action: string]; close: [] }>();
const characterSeparator = ref(" х ");
const videoFolder = ref("Videos");
const copyrightDirectory = ref(true);
const artistDirectory = ref(true);
const syncStructure = ref(false);
const autoIndex = ref(false);
const autoUpdate = ref(false);
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
  characterSeparator.value = " х ";
  videoFolder.value = "Videos";
  copyrightDirectory.value = true;
  artistDirectory.value = true;
  syncStructure.value = false;
  autoIndex.value = false;
  autoUpdate.value = false;
}
</script>

<template>
  <div class="settings-workspace">
    <aside class="settings-sidebar" aria-label="Settings sections">
      <div class="settings-sidebar-heading">
        Settings <span>UI preview</span>
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
        <p class="settings-notice">
          UI preview · Changes are not applied or saved.
        </p>
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
              description="Not selected"
            >
              <button
                class="secondary-button"
                @click="
                  $emit(
                    'preview',
                    `Browse ${section.label.toLowerCase()} directory`,
                  )
                "
              >
                <AppIcon name="gallery" :size="16" />Browse
              </button>
            </SettingsRow>
            <template v-else-if="section.id === 'structure'">
              <ToggleSwitch
                v-model="copyrightDirectory"
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
                v-model="artistDirectory"
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
                  v-model="characterSeparator"
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
                  v-model="videoFolder"
                  type="text"
                  spellcheck="false"
              /></SettingsRow>
              <ToggleSwitch
                v-model="syncStructure"
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
                  @click="$emit('preview', 'Scan Stage folder')"
                >
                  <AppIcon name="search" :size="16" />Scan Stage folder
                </button>
              </div>
              <ToggleSwitch
                v-model="autoIndex"
                trailing
                class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Auto scan on startup</strong
                  ><small
                    >Scan both libraries each time Nexa opens.</small
                  ></span
                ></ToggleSwitch
              >
              <div class="settings-scan-summary">
                <div>
                  <AppIcon name="info" :size="16" /><strong>Last scan</strong
                  ><span class="settings-status">Idle</span>
                </div>
                <p>No scans yet. Indexing is available as a UI preview.</p>
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
              description="Logs folder: Not configured"
              ><button
                class="secondary-button"
                @click="$emit('preview', 'Open logs folder')"
              >
                <AppIcon name="gallery" :size="16" />Open logs
              </button>
            </SettingsRow>
            <template v-else-if="section.id === 'updates'">
              <SettingsRow
                title="Current version"
                :description="`Nexa ${version}`"
                ><span class="settings-status" role="status">Not checked</span>
              </SettingsRow>
              <ToggleSwitch
                v-model="autoUpdate"
                trailing
                class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Auto check on startup</strong
                  ><small>Check for updates each time Nexa opens.</small></span
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
            ><button
              class="primary-button"
              @click="$emit('preview', 'Save changes')"
            >
              <AppIcon name="check" :size="15" />Save changes
            </button>
          </div>
        </footer>
      </div>
    </div>
  </div>
</template>

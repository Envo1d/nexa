<script setup lang="ts">
import { ref } from "vue";
import { version } from "../../package.json";
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
            <div
              v-if="section.id === 'library' || section.id === 'staging'"
              class="settings-card-row"
            >
              <span>
                <strong>{{
                  section.id === "library"
                    ? "Sorted library path"
                    : "Staging folder"
                }}</strong>
                <small>Not selected</small>
              </span>
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
            </div>
            <template v-else-if="section.id === 'structure'">
              <label class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Copyright directory</strong
                  ><small
                    >Include a copyright directory in the library
                    structure.</small
                  ></span
                ><input
                  v-model="copyrightDirectory"
                  type="checkbox"
                  role="switch"
              /></label>
              <label class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Artist directory</strong
                  ><small
                    >Include an artist directory in the library
                    structure.</small
                  ></span
                ><input v-model="artistDirectory" type="checkbox" role="switch"
              /></label>
              <label class="settings-card-row" for="character-separator"
                ><span
                  ><strong>Character separator</strong
                  ><small
                    >Separator between character names in filenames.</small
                  ></span
                ><input
                  id="character-separator"
                  v-model="characterSeparator"
                  type="text"
                  spellcheck="false"
              /></label>
              <label class="settings-card-row" for="video-subfolder"
                ><span
                  ><strong>Video subfolder name</strong
                  ><small>Name of the subfolder for video files.</small></span
                ><input
                  id="video-subfolder"
                  v-model="videoFolder"
                  type="text"
                  spellcheck="false"
              /></label>
              <label class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Sync structure with disk</strong
                  ><small
                    >When off, folders will not be created from the configured
                    structure.</small
                  ></span
                ><input v-model="syncStructure" type="checkbox" role="switch"
              /></label>
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
              <label class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Auto scan on startup</strong
                  ><small
                    >Scan both libraries each time Nexa opens.</small
                  ></span
                ><input v-model="autoIndex" type="checkbox" role="switch"
              /></label>
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
              <div class="settings-card-row">
                <span
                  ><strong>Clear cache</strong
                  ><small
                    >Remove cached thumbnails and temporary files.</small
                  ></span
                ><button
                  class="secondary-button"
                  @click="$emit('preview', 'Clear thumbnail cache')"
                >
                  <AppIcon name="reset" :size="16" />Clear cache
                </button>
              </div>
            </template>
            <div v-else-if="section.id === 'logs'" class="settings-card-row">
              <span
                ><strong>Application logs</strong
                ><small>Logs folder: Not configured</small></span
              ><button
                class="secondary-button"
                @click="$emit('preview', 'Open logs folder')"
              >
                <AppIcon name="gallery" :size="16" />Open logs
              </button>
            </div>
            <template v-else-if="section.id === 'updates'">
              <div class="settings-card-row">
                <span
                  ><strong>Current version</strong
                  ><small>Nexa {{ version }}</small></span
                ><span class="settings-status" role="status">Not checked</span>
              </div>
              <label class="settings-card-row settings-switch-row"
                ><span
                  ><strong>Auto check on startup</strong
                  ><small>Check for updates each time Nexa opens.</small></span
                ><input v-model="autoUpdate" type="checkbox" role="switch"
              /></label>
              <div class="settings-card-row">
                <span
                  ><strong>Check now</strong
                  ><small>Manually check for a newer version.</small></span
                ><button
                  class="secondary-button"
                  @click="$emit('preview', 'Check for updates')"
                >
                  <AppIcon name="reset" :size="16" />Check now
                </button>
              </div>
            </template>
            <label v-else class="settings-card-row" for="settings-theme"
              ><span
                ><strong>Theme</strong
                ><small>More themes will be available later.</small></span
              ><select id="settings-theme" disabled>
                <option>Midnight violet</option>
              </select></label
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

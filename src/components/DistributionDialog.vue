<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import type { MediaItem } from "../types/media";
import { formatDate, formatDuration, formatSize } from "../types/media";
import MetadataInput, { type MetadataValue } from "./MetadataInput.vue";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{
  items: MediaItem[];
  library: MediaItem[];
  showPreviews: boolean;
}>();
const emit = defineEmits<{
  close: [];
  open: [item: MediaItem];
  processed: [count: number];
}>();
const position = ref(0);
const current = computed(() => props.items[position.value]!);
const remaining = computed(() => props.items.length - position.value);
const retain = ref(false);
const copyrights = ref<MetadataValue[]>([]);
const artists = ref<MetadataValue[]>([]);
const characters = ref<MetadataValue[]>([]);
const tags = ref<MetadataValue[]>([]);
const title = ref("");
const error = ref("");
const dialog = ref<HTMLElement | null>(null);
const steps = [
  "Information",
  "Copyrights",
  "Tags",
  "Characters",
  "Details",
  "Confirmation",
];
const step = ref(0);
const allFiles = ref(false);
const completedCount = ref(0);
const done = ref(false);
const targets = computed(() =>
  allFiles.value ? props.items.slice(position.value) : [current.value],
);
const summary = computed(() => [
  ["Copyrights", copyrights.value.map((value) => value.name).join(", ")],
  ["Artists", artists.value.map((value) => value.name).join(", ")],
  [
    "Characters",
    characters.value
      .map((value) => `${value.name} (${value.copyright})`)
      .join(", "),
  ],
  ["Tags", tags.value.map((value) => value.name).join(", ")],
  ["Video title", title.value.trim()],
]);
function focusStep() {
  nextTick(() => {
    dialog.value?.querySelector(".distribution-scroll")?.scrollTo({ top: 0 });
    dialog.value?.querySelector<HTMLElement>(".wizard-page-heading")?.focus();
  });
}
watch([step, done], focusStep);
watch(allFiles, () => {
  error.value = "";
});
watch(error, (message) => {
  if (message)
    nextTick(() => {
      const alert = dialog.value?.querySelector<HTMLElement>(".error-message");
      alert?.scrollIntoView({ block: "nearest" });
      alert?.focus();
    });
});
function navigate(value: number) {
  error.value = "";
  step.value = value;
}
function values(names: string[]): MetadataValue[] {
  return [
    ...new Map(
      names
        .filter(Boolean)
        .map((name) => [
          name.toLocaleLowerCase(),
          { id: name.toLocaleLowerCase(), name },
        ]),
    ).values(),
  ].sort((a, b) => a.name.localeCompare(b.name));
}
const copyrightOptions = computed(() =>
  values(props.library.flatMap((item) => item.copyrights)),
);
const artistOptions = computed(() =>
  values(props.library.map((item) => item.artist)),
);
const tagOptions = computed(() =>
  values(props.library.flatMap((item) => item.tags)),
);
const characterOptions = computed(() => {
  const entries = new Map<string, MetadataValue>();
  for (const item of props.library)
    for (const copyright of item.copyrights) {
      if (
        !copyrights.value.some(
          (value) =>
            value.name.toLocaleLowerCase() === copyright.toLocaleLowerCase(),
        )
      )
        continue;
      for (const name of item.characters) {
        const id = JSON.stringify([
          copyright.toLocaleLowerCase(),
          name.toLocaleLowerCase(),
        ]);
        entries.set(id, { id, name, copyright });
      }
    }
  return [...entries.values()].sort(
    (a, b) =>
      a.name.localeCompare(b.name) ||
      (a.copyright ?? "").localeCompare(b.copyright ?? ""),
  );
});
watch(copyrights, () => {
  characters.value = characters.value.filter((character) =>
    copyrights.value.some(
      (value) =>
        value.name.toLocaleLowerCase() ===
        character.copyright?.toLocaleLowerCase(),
    ),
  );
});
const needsTitle = computed(
  () =>
    current.value.kind === "video" ||
    props.items.slice(position.value).some((item) => item.kind === "video"),
);
function process() {
  error.value = "";
  if (
    !artists.value.length ||
    !characters.value.length ||
    ((allFiles.value ? needsTitle.value : current.value.kind === "video") &&
      !title.value.trim())
  ) {
    error.value =
      "Choose an artist and character, and enter a title for videos.";
    return;
  }
  completedCount.value = targets.value.length;
  done.value = true;
  emit("processed", completedCount.value);
}
function nextFile() {
  position.value += completedCount.value;
  if (!retain.value) {
    copyrights.value = [];
    artists.value = [];
    characters.value = [];
    tags.value = [];
    title.value = "";
  }
  step.value = 0;
  done.value = false;
  allFiles.value = false;
  error.value = "";
  focusStep();
}
function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.stopPropagation();
    emit("close");
  }
  if (event.key !== "Tab") return;
  const elements = [
    ...(dialog.value?.querySelectorAll<HTMLElement>(
      "button:not([disabled]):not([tabindex='-1']), input:not([disabled]), select:not([disabled]), textarea:not([disabled])",
    ) ?? []),
  ];
  const first = elements[0];
  const last = elements[elements.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
let trigger: HTMLElement | null = null;
onMounted(() => {
  trigger =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  focusStep();
});
onBeforeUnmount(() => trigger?.focus());
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="emit('close')">
      <section
        ref="dialog"
        class="distribution-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="distribution-title"
        @keydown="keyboard"
      >
        <header class="panel-heading">
          <div class="wizard-identity">
            <span class="wizard-app-icon"
              ><AppIcon name="staging" :size="19"
            /></span>
            <div>
              <h2 id="distribution-title">Distribute to Library</h2>
              <p class="muted">
                {{
                  done
                    ? `${completedCount} file(s) completed · ${remaining - completedCount} remaining`
                    : items.length > 1
                      ? `File ${position + 1} of ${items.length} · ${current.filename}`
                      : current.filename
                }}
              </p>
            </div>
          </div>
          <button
            class="icon-button"
            aria-label="Close distribution"
            @click="emit('close')"
          >
            <AppIcon name="close" :size="18" />
          </button>
        </header>
        <nav class="wizard-steps" aria-label="Distribution steps">
          <button
            v-for="(label, index) in steps"
            :key="label"
            :class="{
              active: !done && step === index,
              complete: done || index < step,
            }"
            :aria-current="!done && step === index ? 'step' : undefined"
            :disabled="done || index > step"
            @click="navigate(index)"
          >
            <span class="wizard-step-number"
              ><AppIcon
                v-if="done || index < step"
                name="check"
                :size="14"
              /><template v-else>{{ index + 1 }}</template></span
            ><span>{{ label }}</span>
          </button>
        </nav>
        <div
          class="distribution-scroll"
          :class="{
            'wizard-selection-page': !done && step >= 1 && step <= 3,
            'wizard-information-page': !done && step === 0,
          }"
        >
          <div v-if="done" class="wizard-success">
            <div class="wizard-success-art">
              <AppIcon name="gallery" :size="92" /><span
                ><AppIcon name="check" :size="28"
              /></span>
            </div>
            <h2 class="wizard-page-heading" tabindex="-1">Done!</h2>
            <p class="muted">
              {{
                completedCount === 1 ? "The file" : `${completedCount} files`
              }}
              completed the distribution demo.
            </p>
            <dl class="wizard-summary">
              <div>
                <dt>Action</dt>
                <dd>Distribution preview completed</dd>
              </div>
              <div>
                <dt>Files</dt>
                <dd>{{ completedCount }}</dd>
              </div>
              <div>
                <dt>Queue</dt>
                <dd>{{ remaining - completedCount }} remaining</dd>
              </div>
              <div v-if="remaining > completedCount">
                <dt>Next file</dt>
                <dd>{{ items[position + completedCount]?.filename }}</dd>
              </div>
              <div v-if="remaining > completedCount">
                <dt>Entered details</dt>
                <dd>
                  {{
                    retain
                      ? "Keep for the next file"
                      : "Clear for the next file"
                  }}
                </dd>
              </div>
            </dl>
          </div>
          <template v-else>
            <template v-if="step === 0">
              <h3 class="wizard-page-heading" tabindex="-1">
                File information
              </h3>
              <p class="wizard-description">
                Review the source file before adding its library details.
              </p>
              <div class="wizard-information">
                <div class="wizard-source">
                  <div class="wizard-preview">
                    <img
                      v-if="showPreviews"
                      :src="current.preview"
                      alt="Source media preview"
                    /><AppIcon v-else :name="current.kind" :size="56" /><span
                      class="wizard-extension"
                      >{{ current.extension }}</span
                    ><span v-if="current.duration" class="wizard-duration">{{
                      formatDuration(current.duration)
                    }}</span>
                  </div>
                  <button
                    class="secondary-button"
                    @click="emit('open', current)"
                  >
                    <AppIcon name="arrow" :size="14" />Open file
                  </button>
                </div>
                <dl class="wizard-technical">
                  <div>
                    <dt>Filename</dt>
                    <dd>{{ current.filename }}</dd>
                  </div>
                  <div>
                    <dt>Staging path</dt>
                    <dd>{{ current.path }}</dd>
                  </div>
                  <div>
                    <dt>File type</dt>
                    <dd>
                      {{ current.kind === "video" ? "Video" : "Image" }} ({{
                        current.extension
                      }})
                    </dd>
                  </div>
                  <div>
                    <dt>Modified</dt>
                    <dd>{{ formatDate(current.modifiedAt) }}</dd>
                  </div>
                  <div class="wizard-technical-pair">
                    <div>
                      <dt>Size</dt>
                      <dd>{{ formatSize(current.size) }}</dd>
                    </div>
                    <div>
                      <dt>Resolution</dt>
                      <dd>{{ current.width }} × {{ current.height }}</dd>
                    </div>
                  </div>
                  <div
                    v-if="current.kind === 'video'"
                    class="wizard-technical-pair"
                  >
                    <div>
                      <dt>Duration</dt>
                      <dd>{{ formatDuration(current.duration ?? 0, true) }}</dd>
                    </div>
                    <div>
                      <dt>FPS</dt>
                      <dd>{{ current.fps ?? "—" }}</dd>
                    </div>
                  </div>
                </dl>
              </div>
            </template>
            <template v-else-if="step === 1">
              <h3 class="wizard-page-heading" tabindex="-1">Copyrights</h3>
              <p class="wizard-description">
                Choose the copyrights that the characters belong to.
              </p>
              <MetadataInput
                v-model="copyrights"
                label="Copyrights"
                :options="copyrightOptions"
                allow-custom
                panel
              />
            </template>
            <template v-else-if="step === 2">
              <h3 class="wizard-page-heading" tabindex="-1">Tags</h3>
              <p class="wizard-description">
                Add tags to help find this media in your library.
              </p>
              <MetadataInput
                v-model="tags"
                label="Tags"
                :options="tagOptions"
                allow-custom
                panel
              />
            </template>
            <template v-else-if="step === 3">
              <h3 class="wizard-page-heading" tabindex="-1">Characters</h3>
              <p class="wizard-description">
                Add characters and assign each one to a selected copyright.
              </p>
              <MetadataInput
                v-model="characters"
                label="Characters"
                :options="characterOptions"
                :custom-copyrights="copyrights.map((value) => value.name)"
                :disabled="!copyrights.length"
                panel
              />
            </template>
            <template v-else-if="step === 4">
              <h3 class="wizard-page-heading" tabindex="-1">
                Additional details
              </h3>
              <p class="wizard-description">
                Set the artist attribution and the title used for video
                filenames.
              </p>
              <div class="wizard-fields">
                <MetadataInput
                  v-model="artists"
                  label="Artists"
                  :options="artistOptions"
                  allow-custom
                /><label v-if="needsTitle" class="distribution-title-input"
                  >{{
                    current.kind === "video"
                      ? "Video title"
                      : "Title for queued videos"
                  }}<input v-model="title" placeholder="Enter a video title"
                /></label>
              </div>
            </template>
            <template v-else>
              <h3 class="wizard-page-heading" tabindex="-1">
                Confirm distribution
              </h3>
              <p class="wizard-description">
                Review the details before confirming.
              </p>
              <fieldset v-if="remaining > 1" class="wizard-scope">
                <legend>Apply these details to</legend>
                <label
                  ><input
                    v-model="allFiles"
                    type="radio"
                    :value="false"
                  />Current file</label
                ><label
                  ><input v-model="allFiles" type="radio" :value="true" />All
                  remaining files ({{ remaining }})</label
                >
              </fieldset>
              <div class="wizard-confirm-file">
                <img v-if="showPreviews" :src="current.preview" alt="" />
                <div>
                  <h3>
                    {{
                      allFiles
                        ? `${remaining} files · Shared details`
                        : current.filename
                    }}
                  </h3>
                  <p class="muted">
                    {{
                      allFiles
                        ? "The same metadata and video title will be applied to every file below."
                        : `${formatSize(current.size)} · ${current.width} × ${current.height}`
                    }}
                  </p>
                </div>
              </div>
              <dl class="wizard-summary">
                <div v-for="[label, value] in summary" :key="label">
                  <dt>{{ label }}</dt>
                  <dd>{{ value || "—" }}</dd>
                </div>
              </dl>
              <details v-if="allFiles" class="wizard-batch-files" open>
                <summary>Files to distribute ({{ targets.length }})</summary>
                <ul>
                  <li v-for="item in targets" :key="item.id">
                    <span>{{ item.filename }}</span
                    ><small>{{ item.extension }}</small>
                  </li>
                </ul>
              </details>
              <p v-if="error" class="error-message" role="alert" tabindex="-1">
                {{ error }}
              </p>
            </template>
          </template>
          <p class="modal-note wizard-demo-note">
            Demo workflow · No files are moved or saved yet.
          </p>
        </div>
        <label
          v-if="items.length > 1 && !done"
          class="settings-switch-row distribution-retain"
          ><span
            >Keep entered details between files<small
              >Copyrights, artists, characters, tags and video title</small
            ></span
          ><input v-model="retain" type="checkbox" role="switch"
        /></label>
        <footer class="modal-footer">
          <template v-if="done"
            ><button
              v-if="remaining > completedCount"
              class="secondary-button"
              @click="emit('close')"
            >
              Close</button
            ><span v-else class="wizard-footer-spacer"></span
            ><button
              class="primary-button"
              @click="remaining > completedCount ? nextFile() : emit('close')"
            >
              {{ remaining > completedCount ? "Next file" : "Done"
              }}<AppIcon
                v-if="remaining > completedCount"
                name="arrow"
                :size="16"
              /></button
          ></template>
          <template v-else
            ><button
              v-if="step > 0"
              class="secondary-button"
              @click="navigate(step - 1)"
            >
              <AppIcon
                class="wizard-back-icon"
                name="arrow"
                :size="15"
              />Back</button
            ><span v-else class="wizard-footer-spacer"></span
            ><button class="secondary-button" @click="emit('close')">
              Cancel</button
            ><button
              v-if="step < steps.length - 1"
              class="primary-button"
              @click="navigate(step + 1)"
            >
              Next<AppIcon name="arrow" :size="16" /></button
            ><button v-else class="primary-button" @click="process()">
              {{ allFiles ? `Distribute all (${remaining})` : "Distribute file"
              }}<AppIcon name="check" :size="16" /></button
          ></template>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

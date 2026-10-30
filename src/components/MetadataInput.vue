<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { I18nT } from "vue-i18n";

import { translateUi as ui } from "../i18n/presentation";
import AppIcon from "./AppIcon.vue";
import { searchByName } from "../utils/metadataSearch";

export interface MetadataValue {
  id: string;
  name: string;
  copyright?: string;
}

const props = withDefaults(
  defineProps<{
    modelValue?: MetadataValue[];
    textValue?: string;
    textInput?: boolean;
    required?: boolean;
    options: MetadataValue[];
    label: string;
    hideLabel?: boolean;
    placeholder?: string;
    disabled?: boolean;
    allowCustom?: boolean;
    customCopyrights?: string[];
    panel?: boolean;
    single?: boolean;
  }>(),
  {
    modelValue: () => [],
    textValue: "",
    placeholder: undefined,
    customCopyrights: undefined,
  },
);

const emit = defineEmits<{
  "update:modelValue": [values: MetadataValue[]];
  "update:textValue": [value: string];
}>();

const search = ref("");

const query = computed({
  get: () => (props.textInput ? (props.textValue ?? "") : search.value),
  set: (value: string) => {
    if (props.textInput) emit("update:textValue", value);
    else search.value = value;
  },
});

const focused = ref(false);
const active = ref(-1);
const input = ref<HTMLInputElement | null>(null);

const assignment = ref<MetadataValue[]>([]);

const matches = computed(() => {
  const name = query.value.trim().toLocaleLowerCase();

  if (!name) return [];

  const options = props.options.filter(
    (value) =>
      props.textInput ||
      !props.modelValue.some((selected) => selected.id === value.id),
  );

  return searchByName(options, name).slice(0, 30);
});

const showSuggestions = computed(
  () =>
    focused.value &&
    !props.disabled &&
    (!props.textInput || props.options.length > 0) &&
    Boolean(query.value.trim()) &&
    !assignment.value.length,
);

const canCreate = computed(
  () =>
    !props.textInput &&
    (props.allowCustom || Boolean(props.customCopyrights?.length)),
);

const entry = ref<HTMLElement | null>(null);
const popup = ref<HTMLElement | null>(null);
const popupTarget = ref<HTMLElement | null>(null);
const anchorVisible = ref(true);
const popupStyle = ref({
  left: "0px",
  top: "0px",
  bottom: "auto",
  width: "0px",
  maxHeight: "200px",
});

let observer: ResizeObserver | undefined;
let frame: number | undefined;

const popupOpen = computed(
  () => showSuggestions.value || Boolean(assignment.value.length),
);

function positionPopup() {
  const anchor = entry.value;
  const target = popupTarget.value;

  if (!anchor || !target || !popupOpen.value) return;

  const bounds = anchor.getBoundingClientRect();

  const parent = target.getBoundingClientRect();

  const scroll = anchor
    .closest(
      ".distribution-scroll, .preset-manager-body, .modal, .viewer-sidebar",
    )
    ?.getBoundingClientRect();

  anchorVisible.value =
    !scroll || (bounds.bottom > scroll.top && bounds.bottom <= scroll.bottom);
  popupStyle.value = {
    left: `${bounds.left - parent.left - target.clientLeft}px`,
    top: props.panel
      ? "auto"
      : `${bounds.bottom - parent.top - target.clientTop + 4}px`,
    bottom: props.panel
      ? `${parent.bottom - bounds.top - target.clientTop + 4}px`
      : "auto",
    width: `${bounds.width}px`,
    maxHeight: `${Math.max(0, Math.min(220, props.panel ? bounds.top - (scroll?.top ?? 16) - 4 : window.innerHeight - bounds.bottom - 16))}px`,
  };
}

function schedulePosition() {
  if (frame !== undefined) return;

  frame = requestAnimationFrame(() => {
    frame = undefined;
    positionPopup();
  });
}

function dismissAssignment(event: PointerEvent) {
  if (
    assignment.value.length &&
    event.target instanceof Node &&
    !popup.value?.contains(event.target) &&
    !entry.value?.contains(event.target)
  )
    assignment.value = [];
}

watch([popupOpen, query, () => props.modelValue.length], async () => {
  await nextTick();

  positionPopup();
});

onMounted(() => {
  popupTarget.value =
    entry.value?.closest<HTMLElement>(
      ".distribution-dialog, .preset-manager, .modal-backdrop, .viewer-backdrop",
    ) ?? null;
  document.addEventListener("scroll", schedulePosition, true);
  document.addEventListener("pointerdown", dismissAssignment, true);
  window.addEventListener("resize", schedulePosition);
  observer = new ResizeObserver(schedulePosition);

  if (entry.value) observer.observe(entry.value);
});

onBeforeUnmount(() => {
  document.removeEventListener("scroll", schedulePosition, true);
  document.removeEventListener("pointerdown", dismissAssignment, true);
  window.removeEventListener("resize", schedulePosition);
  observer?.disconnect();

  if (frame !== undefined) cancelAnimationFrame(frame);
});

watch(active, async () => {
  await nextTick();

  const option = popup.value?.querySelector<HTMLElement>(
    "[aria-selected='true']",
  );

  if (!option || !popup.value) return;

  if (option.offsetTop < popup.value.scrollTop)
    popup.value.scrollTop = option.offsetTop;
  else if (
    option.offsetTop + option.offsetHeight >
    popup.value.scrollTop + popup.value.clientHeight
  )
    popup.value.scrollTop =
      option.offsetTop + option.offsetHeight - popup.value.clientHeight;
});

watch(query, () => {
  active.value = -1;
  assignment.value = [];
});

watch(
  () => props.customCopyrights,
  (copyrights) => {
    assignment.value = assignment.value.filter((value) =>
      copyrights?.includes(value.copyright ?? ""),
    );
  },
);

function choose(value: MetadataValue) {
  if (props.disabled) return;

  if (props.textInput) {
    query.value = value.name;
    focused.value = false;
    active.value = -1;

    return;
  }

  if (!props.modelValue.some((entry) => entry.id === value.id))
    emit(
      "update:modelValue",
      props.single ? [value] : [...props.modelValue, value],
    );

  assignment.value = [];
  query.value = "";
  active.value = -1;
}

async function assign(choices: MetadataValue[]) {
  assignment.value = choices;
  focused.value = false;

  await nextTick();

  positionPopup();

  await nextTick();

  popup.value?.querySelector<HTMLButtonElement>("button")?.focus();
}

function commit() {
  if (props.textInput) {
    focused.value = false;

    return;
  }

  const name = query.value.trim();

  if (!name || props.disabled) return;

  const exact = props.options.filter(
    (value) => value.name.toLocaleLowerCase() === name.toLocaleLowerCase(),
  );

  if (exact.length === 1) {
    choose(exact[0]!);

    return;
  }

  if (exact.length > 1) {
    assign(exact);

    return;
  }

  if (props.customCopyrights?.length) {
    const choices = props.customCopyrights.map((copyright) => ({
      id: JSON.stringify([
        copyright.toLocaleLowerCase(),
        name.toLocaleLowerCase(),
      ]),
      name,
      copyright,
    }));

    if (choices.length === 1) choose(choices[0]!);
    else assign(choices);
  } else if (props.allowCustom) choose({ id: name.toLocaleLowerCase(), name });
}

function addValue() {
  commit();

  if (!assignment.value.length) nextTick(() => input.value?.focus());
}

function cancelAssignment() {
  assignment.value = [];
  query.value = "";

  nextTick(() => input.value?.focus());
}

function chooseAssignment(value: MetadataValue) {
  choose(value);

  nextTick(() => input.value?.focus());
}

function keydown(event: KeyboardEvent) {
  if (event.isComposing || props.disabled) return;

  if (props.textInput && !props.options.length) return;

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    focused.value = true;
    active.value = matches.value.length
      ? active.value < 0
        ? event.key === "ArrowDown"
          ? 0
          : matches.value.length - 1
        : (active.value +
            (event.key === "ArrowDown" ? 1 : matches.value.length - 1)) %
          matches.value.length
      : -1;
  } else if (event.key === "Enter") {
    event.preventDefault();

    const value = showSuggestions.value
      ? matches.value[active.value]
      : undefined;

    if (value) choose(value);
    else commit();
  } else if (event.key === "Escape" && showSuggestions.value) {
    event.stopPropagation();
    focused.value = false;
  }
}
</script>

<template>
  <div class="metadata-input" :class="{ 'metadata-panel': panel }">
    <div v-if="!hideLabel" class="metadata-label">
      <label :for="`distribute-${label}`">{{ ui(label) }}</label>
      <small v-if="!textInput && modelValue.length"
        >{{ modelValue.length }} {{ ui("selected") }}</small
      >
    </div>
    <div
      v-if="!textInput && (panel || modelValue.length)"
      class="metadata-selected"
      :aria-label="ui(`Selected ${label.toLowerCase()}`)"
    >
      <p v-if="panel && !modelValue.length" class="metadata-empty">
        {{ ui(`No ${label.toLowerCase()} selected yet.`) }}
      </p>
      <button
        v-for="value in modelValue"
        :key="value.id"
        type="button"
        class="tag-chip"
        :title="`${value.name}${value.copyright ? ` (${value.copyright})` : ''}`"
        :aria-label="
          ui(
            `Remove ${value.name}${value.copyright ? ` (${value.copyright})` : ''}`,
          )
        "
        @click="
          emit(
            'update:modelValue',
            modelValue.filter((entry) => entry.id !== value.id),
          )
        "
      >
        <span
          >{{ value.name
          }}{{ value.copyright ? ` (${value.copyright})` : "" }}</span
        ><AppIcon name="close" :size="12" />
      </button>
    </div>
    <div ref="entry" class="metadata-entry">
      <AppIcon name="search" :size="15" />
      <input
        :id="`distribute-${label}`"
        ref="input"
        v-model="query"
        :aria-label="hideLabel ? ui(label) : undefined"
        :disabled="disabled"
        :required="textInput && required"
        role="combobox"
        aria-autocomplete="list"
        :aria-expanded="showSuggestions && anchorVisible && matches.length > 0"
        :aria-controls="`options-${label}`"
        :aria-activedescendant="
          showSuggestions && anchorVisible && active >= 0
            ? `option-${label}-${active}`
            : undefined
        "
        :placeholder="
          placeholder ??
          ui(
            disabled
              ? 'Choose a copyright first'
              : textInput
                ? `Enter ${label.toLowerCase()}…`
                : `Add ${label.toLowerCase()}…`,
          )
        "
        autocomplete="off"
        @focus="focused = true"
        @blur="focused = false"
        @keydown="keydown"
      />
      <kbd v-if="query.trim() && canCreate">Enter</kbd>
      <button
        v-if="panel && !textInput"
        class="metadata-add"
        type="button"
        :aria-label="ui(`Add ${label.toLowerCase()}`)"
        :disabled="disabled || !query.trim()"
        @mousedown.prevent
        @click="addValue"
      >
        <AppIcon name="plus" :size="18" />
      </button>
    </div>
    <Teleport v-if="popupTarget" :to="popupTarget">
      <div
        v-if="popupOpen && anchorVisible"
        ref="popup"
        class="distribution-metadata-popup"
        :style="popupStyle"
      >
        <div
          v-if="assignment.length"
          class="character-assignment"
          role="group"
          :aria-label="ui(`Choose copyright for ${query.trim()}`)"
          @keydown.esc.stop="cancelAssignment"
        >
          <I18nT keypath="uiWhichCopyrightDoesValueBelongTo" tag="p">
            <template #name
              ><strong>{{ query.trim() }}</strong></template
            >
          </I18nT>
          <div>
            <button
              v-for="value in assignment"
              :key="value.id"
              type="button"
              :title="value.copyright"
              @click="chooseAssignment(value)"
            >
              {{ value.copyright }}
            </button>
            <button type="button" class="text-button" @click="cancelAssignment">
              {{ ui("Cancel") }}
            </button>
          </div>
        </div>
        <div
          v-else
          :id="`options-${label}`"
          class="distribution-metadata-options"
          role="listbox"
          :aria-label="ui(`${label} suggestions`)"
        >
          <button
            v-for="(value, index) in matches"
            :id="`option-${label}-${index}`"
            :key="value.id"
            type="button"
            role="option"
            tabindex="-1"
            :aria-selected="index === active"
            :class="{ active: index === active }"
            :title="`${value.name}${value.copyright ? ` (${value.copyright})` : ''}`"
            @mousedown.prevent
            @click="choose(value)"
          >
            <span
              >{{ value.name
              }}{{ value.copyright ? ` (${value.copyright})` : "" }}</span
            >
          </button>
          <p v-if="!matches.length">
            {{
              ui(
                canCreate
                  ? "Press Enter to add this value."
                  : "No matching values.",
              )
            }}
          </p>
        </div>
      </div>
    </Teleport>
  </div>
</template>

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { computed, nextTick, reactive, ref, watch } from "vue";
import { ModuleKind, transpileModule } from "typescript";

const searchModule = { exports: {} };

vm.runInNewContext(
  transpileModule(
    await readFile(
      new URL("../src/utils/metadataSearch.ts", import.meta.url),
      "utf8",
    ),
    {
      compilerOptions: { module: ModuleKind.CommonJS },
    },
  ).outputText,
  searchModule,
);

// Exercise real script-setup operations with IPC doubles, without a browser.
async function script(file, expose) {
  const source = await readFile(
    new URL(`../src/components/${file}.vue`, import.meta.url),
    "utf8",
  );

  const setup = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];
  const code = transpileModule(setup, {
    compilerOptions: { module: ModuleKind.CommonJS },
  }).outputText;

  return `${code}\nglobalThis.state = { ${expose} };`;
}

const presetsCode = await script(
  "StageMetadataPresets",
  "load, apply, save, remove, presets, selected, name, busy, error, notice, query, visiblePresets, pickerKeyboard, open",
);

const managerCode = await script(
  "MetadataPresetManager",
  "choose, createFromCurrent, duplicate, save, keyboard, selected, name, artist, copyrights, characters, tags, query, visiblePresets, confirmingDelete",
);

const wizardCode = await script(
  "DistributionDialog",
  "applyPreset, presetMetadata, copyrights, artist, characters, tags, title, retain, saving",
);

function fixture(code, props, invoke = async () => []) {
  const events = [];

  const context = {
    exports: {},
    defineProps: () => props,
    defineEmits:
      () =>
      (...args) =>
        events.push(args),
    defineExpose: () => {},
    require: (id) => {
      if (id === "vue")
        return {
          computed,
          ref,
          watch,
          nextTick,
          useId: () => "presets-test",
          onMounted: () => {},
          onBeforeUnmount: () => {},
        };

      if (id === "@tauri-apps/api/core") return { isTauri: () => true, invoke };

      if (id.endsWith("runtime")) return { isNativeData: () => true };

      if (id.endsWith("metadataSearch")) return searchModule.exports;

      if (id.endsWith("presentation")) return { translateUi: (value) => value };

      if (id.endsWith("useFocusTrap"))
        return { useFocusTrap: () => ({ trapFocus: () => {} }) };

      return {};
    },
  };

  vm.runInNewContext(code, context);

  return { ...context.state, events };
}

const metadata = () => ({
  artist: "Artist",
  copyrights: ["World"],
  characters: [{ name: "Alice", copyright: "World" }],
  tags: ["Tag"],
});

const plain = (value) => JSON.parse(JSON.stringify(value));

test("selection applies a detached snapshot without a media write", async () => {
  const calls = [];

  const state = fixture(
    presetsCode,
    { disabled: false, metadata: metadata() },
    async (command) => {
      calls.push(command);
      return [
        {
          id: "1",
          name: "Regular",
          metadata: metadata(),
        },
      ];
    },
  );

  await state.load();

  state.apply("1");
  assert.equal(state.events.length, 1);
  assert.equal(state.events[0][0], "apply");
  state.events[0][1].tags.push("Changed");
  assert.deepEqual(plain(state.presets.value[0].metadata.tags), ["Tag"]);
  assert.deepEqual(calls, ["get_metadata_presets"]);
});

test("save new and update use distinct identities; failures preserve existing presets", async () => {
  const calls = [];

  const state = fixture(
    presetsCode,
    { disabled: false, metadata: metadata() },
    async (command, args) => {
      calls.push([command, args]);

      if (command === "get_metadata_presets") return [];

      if (args.name === "Duplicate")
        throw "A preset with this name already exists.";

      return {
        id: args.id ?? "1",
        name: args.name,
        metadata: args.metadata,
      };
    },
  );

  await state.load();

  state.name.value = "Regular";

  await state.save(false);

  assert.equal(calls[1][1].id, null);
  state.name.value = "Updated";

  await state.save(true);

  assert.equal(calls[2][1].id, "1");
  state.name.value = "Duplicate";

  await state.save(false);

  assert.match(state.error.value, /already exists/);
  assert.equal(state.presets.value.length, 1);
  assert.equal(state.presets.value[0].name, "Updated");
  assert.equal(state.busy.value, false);
});

test("load failure can retry, disabled application is ignored, and deletion keeps the draft", async () => {
  const props = reactive({ disabled: false, metadata: metadata() });

  let fail = true;

  const state = fixture(presetsCode, props, async (command) => {
    if (fail) throw "Unavailable";

    return command === "get_metadata_presets"
      ? [
          {
            id: "1",
            name: "Regular",
            metadata: metadata(),
          },
        ]
      : undefined;
  });

  await state.load();

  assert.match(state.error.value, /Unavailable/);
  fail = false;

  await state.load();

  props.disabled = true;
  state.apply("1");
  assert.equal(state.events.length, 0);
  props.disabled = false;
  state.apply("1");

  await state.remove();

  assert.equal(state.presets.value.length, 0);
  assert.equal(state.events.length, 1);
  assert.deepEqual(plain(props.metadata), metadata());
});

test("wizard restores copyright-qualified characters after its watcher and keeps video title and retention", async () => {
  const state = fixture(wizardCode, {
    items: [{ kind: "video" }],
    library: [],
  });

  state.title.value = "Individual video title";
  state.retain.value = true;
  state.applyPreset(metadata());

  await nextTick();

  assert.deepEqual(plain(state.presetMetadata.value), metadata());
  assert.equal(state.title.value, "Individual video title");
  assert.equal(state.retain.value, true);
  state.saving.value = true;
  state.applyPreset({
    artist: "Other",
    copyrights: [],
    characters: [],
    tags: [],
  });
  assert.equal(state.artist.value, "Artist");
  assert.equal(state.events.length, 0);
});

test("manager edits and duplication stay detached from saved presets and the distribution draft", async () => {
  const preset = {
    id: "1",
    name: "Regular",
    metadata: metadata(),
  };

  const props = reactive({
    presets: [preset],
    metadata: metadata(),
    busy: false,
  });

  const state = fixture(managerCode, props);

  state.choose(props.presets[0]);

  await nextTick();

  state.artist.value = "Other artist";
  state.tags.value.push({ id: "Extra", name: "Extra" });
  assert.deepEqual(plain(props.metadata), metadata());
  assert.deepEqual(plain(props.presets[0].metadata), metadata());
  assert.equal(state.events.length, 0);
  state.duplicate();
  assert.equal(state.selected.value, null);
  assert.equal(state.name.value, "");
  state.name.value = "Copy";
  state.save();
  assert.equal(state.events[0][0], "save");
  assert.equal(state.events[0][1], null);
  assert.equal(state.events[0][2], "Copy");
  assert.equal(state.events[0][3].artist, "Other artist");
  assert.deepEqual(plain(state.events[0][3].tags), ["Tag", "Extra"]);
});

test("manager creates from current details, updates by ID and removes characters whose copyright was removed", async () => {
  const props = reactive({
    presets: [
      {
        id: "7",
        name: "Stored",
        metadata: metadata(),
      },
    ],
    metadata: metadata(),
    busy: false,
  });

  const state = fixture(managerCode, props);

  state.createFromCurrent();

  await nextTick();

  assert.equal(state.selected.value, null);
  assert.equal(state.artist.value, "Artist");
  state.choose(props.presets[0]);

  await nextTick();

  state.copyrights.value = [];

  await nextTick();

  assert.deepEqual(plain(state.characters.value), []);
  state.save();
  assert.equal(state.events[0][1], "7");
  assert.deepEqual(plain(state.events[0][3].characters), []);
  props.busy = true;
  state.createFromCurrent();
  state.duplicate();
  state.save();
  assert.equal(state.selected.value, "7");
  assert.equal(state.events.length, 1);
});

test("saving manager metadata persists its explicit snapshot without applying to the wizard", async () => {
  const draft = metadata();

  const edited = {
    ...metadata(),
    artist: "Edited independently",
    tags: ["Manager tag"],
  };
  let payload;

  const state = fixture(
    presetsCode,
    { disabled: false, metadata: draft },
    async (command, args) => {
      if (command === "get_metadata_presets") return [];

      payload = args;
      return {
        id: "1",
        name: args.name,
        metadata: args.metadata,
      };
    },
  );

  await state.load();

  state.name.value = "Manager snapshot";

  await state.save(false, edited);

  assert.deepEqual(plain(payload.metadata), edited);
  assert.deepEqual(draft, metadata());
  assert.equal(state.events.length, 0);
});

test("preset search filters names and Escape dismisses only an open picker", async () => {
  const state = fixture(
    presetsCode,
    { disabled: false, metadata: metadata() },
    async () => [
      {
        id: "1",
        name: "Regular",
        metadata: metadata(),
      },
      {
        id: "2",
        name: "Other",
        metadata: metadata(),
      },
    ],
  );

  await state.load();

  state.query.value = " REG ";
  assert.deepEqual(
    plain(state.visiblePresets.value.map((preset) => preset.id)),
    ["1"],
  );

  state.query.value = "gular";
  assert.equal(state.visiblePresets.value.length, 1);

  let stopped = 0;
  const event = {
    key: "Escape",
    preventDefault() {},
    stopPropagation() {
      stopped++;
    },
  };

  state.pickerKeyboard(event);
  assert.equal(stopped, 0);
  state.open.value = true;
  state.pickerKeyboard(event);
  assert.equal(stopped, 1);
  assert.equal(state.open.value, false);
});

test("manager Escape cancels delete confirmation before closing and cannot close while saving", () => {
  const props = reactive({
    presets: [],
    metadata: metadata(),
    busy: false,
  });

  const state = fixture(managerCode, props);

  const event = () => ({
    key: "Escape",
    defaultPrevented: false,
    preventDefault() {},
  });

  state.confirmingDelete.value = true;
  state.keyboard(event());
  assert.equal(state.confirmingDelete.value, false);
  assert.equal(state.events.length, 0);
  props.busy = true;
  state.keyboard(event());
  assert.equal(state.events.length, 0);
  props.busy = false;
  state.keyboard(event());
  assert.equal(state.events[0][0], "close");
});

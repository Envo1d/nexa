import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { computed, nextTick, ref, watch } from "vue";
import { ModuleKind, transpileModule } from "typescript";

const keys = {
  copyright: "copyrights",
  artist: "artists",
  tag: "tags",
  character: "characters",
};

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

async function compile(path, expose, component = false) {
  const source = await readFile(new URL(path, import.meta.url), "utf8");

  const script = component
    ? source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
    : source;

  return (
    transpileModule(script, {
      compilerOptions: { module: ModuleKind.CommonJS },
    }).outputText +
    "\nglobalThis.state = { " +
    expose +
    " };"
  );
}

const panelCode = await compile(
  "../src/components/TagsPanel.vue",
  "selectEntry, cancelEdit, confirmDeletion, showCharacter, apply, category, catalog, copyright, copyrightOptions, name, deleting, error, busy, search, entries, selectedEntry, dirty, linkedCount, repeatedOnly, relatedCharacters",
  true,
);

const demoCode = await compile(
  "../src/composables/useDemoTaxonomy.ts",
  "useDemoTaxonomy",
);

const inputCode = await compile(
  "../src/components/MetadataInput.vue",
  "keydown, focused, showSuggestions, query, matches",
  true,
);

test("taxonomy and metadata inputs rank prefixes before substrings without matching parent copyrights", () => {
  const names = ["atomic", "amber", "marvel", "M_hero"];
  const characters = names.map((name) => ({
    name,
    copyright: "marvel world",
    count: 0,
  }));
  const panel = run(panelCode, true);

  panel.catalog.value.characters = characters;
  panel.category.value = "character";
  panel.search.value = " M ";
  assert.deepEqual(
    Array.from(panel.entries.value, (entry) => entry.name),
    ["marvel", "M_hero", "atomic", "amber"],
  );

  const input = run(inputCode, false, undefined, {
    options: names.map((name) => ({ id: name, name })),
    modelValue: [{ id: "marvel", name: "marvel" }],
  });

  input.query.value = "m";
  assert.deepEqual(
    Array.from(input.matches.value, (entry) => entry.name),
    ["M_hero", "atomic", "amber"],
  );
  input.query.value = "_";
  assert.equal(input.matches.value.length, 1);
});

function run(code, native, invoke = async () => ({}), props = {}) {
  const events = [];

  const context = {
    exports: {},
    defineProps: () => props,
    withDefaults: (values, defaults) => ({ ...defaults, ...values }),
    defineEmits:
      () =>
      (...args) =>
        events.push(args),
    require: (id) => {
      if (id === "vue")
        return {
          computed,
          ref,
          watch,
          nextTick,
          onMounted: () => {},
          onBeforeUnmount: () => {},
        };

      if (id === "@tauri-apps/api/core")
        return { isTauri: () => native, invoke };

      if (id.endsWith("runtime")) return { isNativeData: () => native };

      if (id.endsWith("metadataSearch")) return searchModule.exports;

      if (id.endsWith("taxonomy")) return { categoryKey: keys };

      return {};
    },
  };

  vm.runInNewContext(code, context);

  return { ...context.state, events };
}

const catalog = () => ({
  copyrights: [
    {
      id: "1",
      name: "World",
      count: 1,
    },
    {
      id: "2",
      name: "Other",
      count: 0,
    },
  ],
  characters: [
    {
      id: "10",
      name: "Alice",
      copyright: "World",
      count: 1,
    },
  ],
  artists: [],
  tags: [],
});

test("character editor submits name and target copyright together and refreshes", async () => {
  const calls = [];

  const data = catalog();

  const state = run(panelCode, true, async (command, args) => {
    calls.push([command, args]);
    return data;
  });

  state.catalog.value = data;
  state.category.value = "character";

  await nextTick();

  state.selectEntry(data.characters[0]);
  assert.equal(state.copyright.value, "World");
  state.copyright.value = "Other";
  state.name.value = "Hero";

  await state.apply();

  assert.equal(calls[0][0], "update_taxonomy_character");
  assert.deepEqual(JSON.parse(JSON.stringify(calls[0][1])), {
    id: 10,
    name: "Hero",
    copyrightId: 2,
  });
  assert.equal(calls[1][0], "get_taxonomy");
  assert.equal(state.deleting.value, false);
  assert.equal(state.events[0][0], "changed");
});

test("failed update preserves draft and missing target makes no write", async () => {
  const data = catalog();

  let calls = 0;

  const state = run(panelCode, true, async () => {
    calls++;
    throw new Error("collision");
  });

  state.catalog.value = data;
  state.category.value = "character";

  await nextTick();

  state.selectEntry(data.characters[0]);
  state.copyright.value = "Other";

  await state.apply();

  assert.match(state.error.value, /collision/);
  assert.equal(state.copyright.value, "Other");
  assert.equal(state.dirty.value, true);
  assert.equal(state.busy.value, false);
  assert.equal(state.events.length, 0);
  state.copyright.value = "Missing";

  await state.apply();

  assert.equal(calls, 1);
});

test("other categories retain the rename command", async () => {
  const calls = [];

  const state = run(panelCode, true, async (command) => {
    calls.push(command);
    return catalog();
  });

  state.catalog.value.tags = [{ id: "5", name: "Tag", count: 0 }];
  state.category.value = "tag";

  await nextTick();

  state.name.value = "Renamed";

  await state.apply();

  assert.equal(calls[0], "rename_taxonomy");
});

test("list selection, cancellation and empty filters never write or retain another entry's draft", async () => {
  let writes = 0;
  const state = run(panelCode, true, async () => writes++);
  const data = catalog();
  data.characters.push({
    id: "11",
    name: "Alice",
    copyright: "Other",
    count: 0,
  });
  state.catalog.value = data;
  state.category.value = "character";

  await nextTick();

  assert.equal(state.selectedEntry.value.id, "10");
  assert.equal(state.name.value, "Alice");
  state.name.value = "Draft";
  state.copyright.value = "Other";
  assert.equal(state.dirty.value, true);
  state.cancelEdit();
  assert.equal(state.name.value, "Alice");
  assert.equal(state.copyright.value, "World");
  state.name.value = "Discarded";
  state.selectEntry(data.characters[1]);

  await nextTick();

  assert.equal(state.selectedEntry.value.id, "11");
  assert.equal(state.copyright.value, "Other");
  assert.equal(state.dirty.value, false);
  state.confirmDeletion();
  state.search.value = "missing";

  await nextTick();

  assert.equal(state.selectedEntry.value, null);
  assert.equal(state.name.value, "");
  assert.equal(state.deleting.value, false);
  await state.apply();
  assert.equal(writes, 0);
});

test("related character navigation clears incompatible filters and preserves qualified identity", async () => {
  const state = run(panelCode, true);
  const data = catalog();
  data.characters.push({
    id: "11",
    name: "Alice",
    copyright: "Other",
    count: 0,
  });
  state.catalog.value = data;

  await nextTick();

  assert.deepEqual(
    Array.from(state.relatedCharacters.value, (entry) => entry.id),
    ["10"],
  );
  state.linkedCount.value = 1;
  state.repeatedOnly.value = true;
  state.search.value = "World";
  state.showCharacter(data.characters[1]);

  await nextTick();

  assert.equal(state.category.value, "character");
  assert.equal(state.linkedCount.value, "all");
  assert.equal(state.search.value, "");
  assert.equal(state.repeatedOnly.value, false);
  assert.equal(state.selectedEntry.value.id, "11");
  assert.equal(state.copyright.value, "Other");
  state.linkedCount.value = 0;
  state.category.value = "copyright";

  await nextTick();

  assert.equal(state.linkedCount.value, 0);
  assert.equal(state.selectedEntry.value.name, "Other");
  assert.equal(state.dirty.value, false);
});

test("deletion requires confirmation and blocks selection while its command is running", async () => {
  const data = catalog();
  const calls = [];
  let finishDelete;
  const state = run(panelCode, true, async (command, args) => {
    calls.push([command, args]);

    if (command === "delete_taxonomy") {
      await new Promise((resolve) => {
        finishDelete = resolve;
      });
      data.copyrights = data.copyrights.filter((entry) => entry.id !== "1");

      return 1;
    }

    return structuredClone(data);
  });
  state.catalog.value = data;

  await nextTick();
  await state.apply();
  assert.equal(calls.length, 0);
  state.confirmDeletion();
  state.cancelEdit();
  await state.apply();
  assert.equal(calls.length, 0);
  state.confirmDeletion();
  const saving = state.apply();
  state.selectEntry(data.copyrights[1]);
  state.showCharacter(data.characters[0]);
  assert.equal(state.selectedEntry.value.id, "1");
  assert.equal(state.category.value, "copyright");
  assert.equal(calls[0][0], "delete_taxonomy");
  assert.deepEqual(JSON.parse(JSON.stringify(calls[0][1])), {
    category: "copyright",
    id: 1,
  });
  finishDelete();
  await saving;
  await nextTick();
  assert.equal(state.selectedEntry.value.id, "2");
  assert.equal(state.deleting.value, false);
  assert.equal(state.dirty.value, false);
});

test("demo rename keeps the renamed qualified character selected after its identity changes", async () => {
  const library = ref([
    {
      id: "file-1",
      artist: "Artist",
      tags: [],
      copyrights: ["World", "Other"],
      characters: ["Alice"],
      characterCopyrights: [{ name: "Alice", copyright: "World" }],
    },
  ]);
  const demo = run(demoCode, false).useDemoTaxonomy(library, ref([]));
  const state = run(panelCode, false, undefined, {
    get demoCatalog() {
      return demo.catalog.value;
    },
    changeDemo: demo.change,
  });
  state.category.value = "character";

  await nextTick();

  state.name.value = "Hero";
  state.copyright.value = "Other";
  await state.apply();
  await nextTick();
  assert.equal(state.selectedEntry.value.name, "Hero");
  assert.equal(state.selectedEntry.value.copyright, "Other");
  assert.equal(state.name.value, "Hero");
  assert.equal(state.dirty.value, false);
});

test("demo reassigns qualified links, cleans old copyright, and rejects collisions atomically", async () => {
  const item = {
    id: "1",
    copyrights: ["World", "Other"],
    characters: ["Alice"],
    characterCopyrights: [{ name: "Alice", copyright: "World" }],
    tags: [],
    artist: "Artist",
  };

  const library = ref([item]);

  const state = run(demoCode, false).useDemoTaxonomy(library, ref([]));

  state.change("character", "Alice", "Hero", "World", "Other");
  assert.equal(library.value[0].characterCopyrights[0].copyright, "Other");
  assert.deepEqual([...library.value[0].copyrights], ["Other"]);
  state.change("character", "Hero", "Hero", "Other", "World");
  assert.throws(() =>
    state.change("character", "Hero", "Hero", "World", "Missing"),
  );
  assert.equal(library.value[0].characterCopyrights[0].copyright, "World");
  // A same-name character from a different copyright must never be silently merged.
  library.value.push({
    ...item,
    id: "2",
    characters: ["Hero"],
    characterCopyrights: [{ name: "Hero", copyright: "Other" }],
  });

  await nextTick();

  assert.throws(
    () => state.change("character", "Hero", "HERO", "World", "Other"),
    /already exists/,
  );
  assert.equal(library.value[0].characterCopyrights[0].copyright, "World");
});

test("demo adds a new copyright without changing inferred old character identity", () => {
  const library = ref([
    {
      id: "1",
      copyrights: ["World"],
      characters: ["Alice"],
      tags: [],
      artist: "Artist",
    },
    {
      id: "2",
      copyrights: ["Another"],
      characters: [],
      tags: [],
      artist: "Artist",
    },
  ]);

  const state = run(demoCode, false).useDemoTaxonomy(library, ref([]));

  state.change("character", "Alice", "Alice", "World", "Another");
  assert.deepEqual([...library.value[0].copyrights], ["Another"]);
  assert.equal(library.value[0].characterCopyrights[0].copyright, "Another");
  assert.equal(library.value[0].characterCopyrights[0].name, "Alice");
});

test("plain name input keeps form keys and hides suggestions; metadata search still handles Enter", () => {
  const state = run(inputCode, false, undefined, {
    textInput: true,
    options: [],
    textValue: "Alice",
    modelValue: [],
  });

  state.focused.value = true;
  assert.equal(state.showSuggestions.value, false);
  let prevented = 0;
  const enter = { key: "Enter", preventDefault: () => prevented++ };

  state.keydown(enter);
  state.keydown({
    key: "Escape",
    stopPropagation: () => assert.fail("Escape should reach the editor"),
  });
  assert.equal(prevented, 0);
  const searchable = run(inputCode, false, undefined, {
    textInput: true,
    options: [{ id: "1", name: "Alice" }],
    textValue: "Alice",
    modelValue: [],
  });

  searchable.focused.value = true;
  assert.equal(searchable.showSuggestions.value, true);
  searchable.keydown(enter);
  assert.equal(prevented, 1);
});

test("demo keeps previous copyright only where another character uses it", () => {
  const library = ref([
    {
      id: "1",
      copyrights: ["World", "Other", "Unrelated"],
      characters: ["Alice", "Bob"],
      characterCopyrights: [
        { name: "Alice", copyright: "World" },
        { name: "Bob", copyright: "World" },
      ],
      tags: [],
      artist: "Artist",
    },
    {
      id: "2",
      copyrights: ["World", "Other"],
      characters: ["Alice"],
      characterCopyrights: [{ name: "Alice", copyright: "World" }],
      tags: [],
      artist: "Artist",
    },
    {
      id: "3",
      copyrights: ["World"],
      characters: [],
      tags: [],
      artist: "Artist",
    },
  ]);

  const state = run(demoCode, false).useDemoTaxonomy(library, ref([]));

  state.change("character", "Alice", "Alice", "World", "Other");
  assert.deepEqual(
    [...library.value[0].copyrights],
    ["World", "Other", "Unrelated"],
  );
  assert.deepEqual([...library.value[1].copyrights], ["Other"]);
  assert.deepEqual([...library.value[2].copyrights], ["World"]);
  state.change("character", "Alice", "Hero", "Other", "Other");
  assert.deepEqual([...library.value[1].copyrights], ["Other"]);
  state.change("character", "Hero", undefined, "Other");
  assert.deepEqual([...library.value[1].copyrights], ["Other"]);
});

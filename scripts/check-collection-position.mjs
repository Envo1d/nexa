import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { transpileModule, ModuleKind } from "typescript";
import { effectScope, nextTick, reactive, watch } from "vue";

// Real Vue watchers and script operations with DOM/IPC doubles; no browser launch.
let serial = 0;

const vueUrl = JSON.stringify(import.meta.resolve("vue"));

const items = (count) =>
  Array.from({ length: count }, (_, id) => ({ id: String(id) }));

async function settle() {
  for (let i = 0; i < 8; i++) await nextTick();
}

async function collection(options = {}) {
  const props = reactive({
    items: items(300),
    mode: "list",
    collectionKey: "all",
    native: true,
    ...options,
  });

  const scope = effectScope();

  globalThis.__collectionProps = props;
  let source = await readFile(
    new URL("../src/components/MediaCollection.vue", import.meta.url),
    "utf8",
  );

  source = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];
  source = source.replace(
    /import \{[\s\S]*?\} from "vue";/,
    `import { computed, nextTick, ref, watch } from ${vueUrl}; const mounted = []; const unmounting = []; const onMounted = fn => mounted.push(fn); const onBeforeUnmount = fn => unmounting.push(fn);`,
  );
  source = source.replace(
    /import \{ useI18n \} from "vue-i18n";/,
    "const useI18n = () => ({t: x => x});",
  );
  source = source.replace(/import .* from ".*?";/g, (line) =>
    line.includes('from "file:') ? line : "",
  );
  source = `const events = []; const defineProps = () => globalThis.__collectionProps; const defineEmits = () => (...args) => events.push(args);\n${source}\nexport { viewport, loaded, scrollTop, updateScroll, visibleItems, rowHeight, columns, mounted, unmounting, events };`;
  const code = transpileModule(source, {
    compilerOptions: { module: ModuleKind.ESNext },
  }).outputText;

  const create = await import(
    `data:text/javascript;base64,${Buffer.from(`export function setup() { ${code.replace(/import .*?;\s*/, "").replace(/export \{([^}]+)\};/, "return {$1};")} }\nimport { computed, nextTick, ref, watch } from ${vueUrl};\n// ${serial++}`).toString("base64")}`
  );

  const state = scope.run(() => create.setup());

  const viewport = {
    scrollTop: options.position ? 0 : 8807,
    scrollTo({ top }) {
      this.scrollTop = top;
    },
  };

  state.viewport.value = viewport;

  if (!options.position) {
    state.loaded.value = 250;
    state.updateScroll();
  }

  return {
    props,
    scope,
    state,
    viewport,
  };
}

test("view navigation restores independent positions and reads pending scroll before unmount", async () => {
  const main = await collection();

  const stage = await collection({ collectionKey: "stage" });

  const scopes = [main.scope, stage.scope];
  const previousObserver = globalThis.ResizeObserver;

  globalThis.ResizeObserver = class {
    observe() {}
    disconnect() {}
  };

  try {
    // No animation frame has consumed these final native scroll positions.
    main.viewport.scrollTop = 12019;
    stage.viewport.scrollTop = 4403;

    for (const f of [main, stage]) {
      f.state.unmounting.forEach((fn) => fn());
      const position = f.state.events.find(
        ([name]) => name === "savePosition",
      )[1];

      const restored = await collection({
        position,
        collectionKey: f.props.collectionKey,
      });

      scopes.push(restored.scope);
      restored.state.mounted.forEach((fn) => fn());
      assert.equal(restored.viewport.scrollTop, f.viewport.scrollTop);
      assert.equal(restored.state.scrollTop.value, f.viewport.scrollTop);
      assert.ok(restored.state.loaded.value >= position.loaded);
      assert.ok(restored.state.visibleItems.value.length > 0);

      for (const change of [{ collectionKey: "other" }, { mode: "grid" }]) {
        const reset = await collection({ position, ...change });

        scopes.push(reset.scope);
        reset.state.mounted.forEach((fn) => fn());
        assert.equal(reset.state.scrollTop.value, 0);
        assert.equal(reset.viewport.scrollTop, 0);
        assert.equal(reset.state.loaded.value, 36);
      }
    }
  } finally {
    scopes.forEach((scope) => scope.stop());
    globalThis.ResizeObserver = previousObserver;
  }
});

for (const scenario of [
  "refresh",
  "insert",
  "delete-before",
  "delete-visible",
  "append",
  "tail",
  "empty",
  "query",
]) {
  test(`collection retains a valid visible position after ${scenario}`, async () => {
    const f = await collection();

    try {
      if (scenario === "insert")
        f.props.items = [{ id: "new" }, ...f.props.items];
      else if (scenario === "delete-before")
        f.props.items = f.props.items.filter((x) => x.id !== "2");
      else if (scenario === "delete-visible")
        f.props.items = f.props.items.filter((x) => x.id !== "100");
      else if (scenario === "append")
        f.props.items = [...f.props.items, { id: "new" }];
      else if (scenario === "tail") f.props.items = f.props.items.slice(0, 50);
      else if (scenario === "empty") f.props.items = [];
      else if (scenario === "query") {
        f.props.collectionKey = "new-sort";

        await settle();

        f.props.items = [...f.props.items].reverse();
      } else f.props.items = f.props.items.map((x) => ({ ...x }));

      await settle();

      const expected = {
        refresh: 8807,
        insert: 8895,
        "delete-before": 8719,
        "delete-visible": 8719,
        append: 8807,
        tail: 3800,
        empty: 0,
        query: 0,
      }[scenario];

      assert.equal(f.viewport.scrollTop, expected);
      assert.equal(f.state.scrollTop.value, expected);

      if (f.props.items.length)
        assert.ok(f.state.visibleItems.value.length > 0);
    } finally {
      f.scope.stop();
    }
  });
}

async function model(name, invoke) {
  let code = transpileModule(
    await readFile(
      new URL(`../src/composables/${name}.ts`, import.meta.url),
      "utf8",
    ),
    { compilerOptions: { module: ModuleKind.ESNext } },
  ).outputText;

  code = code.replace('from "vue"', `from ${vueUrl}`);
  code = code.replace(
    /import \{ useI18n \} from "vue-i18n";/,
    "const useI18n = () => ({t: x => x});",
  );
  code = code.replace(
    /import \{ invoke, isTauri \} from "@tauri-apps\/api\/core";/,
    "const invoke = (...args) => globalThis.__collectionInvoke(...args); const isTauri = () => true;",
  );
  code = code.replace(
    /import \{ useMediaSelection \} from .*?;/,
    "const useMediaSelection = () => ({});",
  );
  code = code.replace(
    /import \{ createMockMedia \} from .*?;/,
    "const createMockMedia = () => [];",
  );
  code = code.replace(
    /import \{[^;]*?\} from "\.\.\/utils\/metadataSearch";/,
    "const metadataSuggestions = () => [];",
  );
  globalThis.__collectionInvoke = invoke;
  const exports = await import(
    `data:text/javascript;base64,${Buffer.from(`${code}\n// ${serial++}`).toString("base64")}`
  );

  const scope = effectScope();

  return { scope, state: scope.run(() => exports[name]()) };
}

for (const name of ["useLibrary", "useStage"]) {
  test(`${name} refresh publishes all previously loaded pages atomically and resets for a query`, async () => {
    let rows = items(12).map((x) => ({
      ...x,
      filename: x.id,
      tags: [],
    }));

    let fail = false;

    const f = await model(name, async (_command, args) => {
      const offset = args.filters?.offset ?? args.offset;

      if (fail && offset > 0) throw new Error("page failed");

      return {
        files: rows.slice(offset, offset + 3),
        total: rows.length,
        typeCounts: [12, 0, 12],
        tags: [],
        suggestions: [],
        characters: [],
      };
    });

    try {
      const refresh = () =>
        name === "useLibrary"
          ? f.state.refresh()
          : f.state.refreshRedistribution();

      await refresh();

      await f.state.loadMore();

      await f.state.loadMore();

      assert.equal(f.state.media.value.length, 9);
      const publications = [];

      const stop = watch(
        f.state.media,
        (value) => publications.push(value.length),
        { flush: "sync" },
      );

      rows = rows.filter((x) => x.id !== "1");

      await refresh();

      assert.deepEqual(publications, [9]);
      assert.equal(f.state.media.value[1].nativeId, "2");
      const previous = f.state.media.value;

      fail = true;

      await refresh();

      assert.equal(f.state.media.value, previous);
      assert.match(f.state.error.value, /page failed/);
      fail = false;
      f.state.appliedSearch.value = "new query";

      await settle();

      assert.equal(f.state.media.value.length, 3);

      await f.state.loadMore();

      assert.equal(f.state.media.value.length, 6);
      const key = f.state.queryKey ?? f.state.collectionKey;
      const previousKey = key.value;

      f.state.sortMode.value = "name-desc";

      await settle();

      assert.equal(f.state.media.value.length, 3);
      assert.notEqual(key.value, previousKey);
      stop();
    } finally {
      f.scope.stop();
    }
  });
}

test("grid refresh and insertion of a complete row preserve the visible anchor", async () => {
  const f = await collection();

  try {
    f.props.mode = "grid";

    await settle();

    f.state.loaded.value = 250;
    const height = f.state.rowHeight.value;
    const columns = f.state.columns.value;

    f.viewport.scrollTop = height * 40 + 7;
    f.state.updateScroll();
    f.props.items = f.props.items.map((x) => ({ ...x }));

    await settle();

    assert.equal(f.viewport.scrollTop, height * 40 + 7);
    f.props.items = [
      ...items(columns).map((x) => ({ id: "insert-" + x.id })),
      ...f.props.items,
    ];

    await settle();

    assert.ok(Math.abs(f.viewport.scrollTop - (height * 41 + 7)) < 0.001);
    assert.ok(f.state.visibleItems.value.length > 0);
  } finally {
    f.scope.stop();
  }
});

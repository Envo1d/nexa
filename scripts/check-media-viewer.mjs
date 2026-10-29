import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { transpileModule, ModuleKind } from "typescript";
import { computed, ref } from "vue";

// Execute the real session composable with controlled native responses, without a browser.
let code = transpileModule(
  await readFile(new URL("../src/features/media/useMediaViewer.ts", import.meta.url), "utf8"),
  { compilerOptions: { module: ModuleKind.ESNext } },
).outputText;

code = code.replace('from "vue"', `from ${JSON.stringify(import.meta.resolve("vue"))}`);
code = code.replace('import { invoke, isTauri } from "@tauri-apps/api/core";',
  "const invoke = (...args) => globalThis.__nexaViewerTest.invoke(...args); const isTauri = () => globalThis.__nexaViewerTest.native;");

const { useMediaViewer } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);

function item(id) {
  return { id: `database-${id}`, nativeId: String(id), filename: `${id}.png`, kind: "image", extension: "png", tags: [], favorite: true };
}

function models(items) {
  let refreshes = 0;

  const library = {
    media: ref(items), total: ref(505), scope: ref("favorites"),
    appliedSearch: ref("Applied tag"), kind: ref("all"), sortMode: ref("newest"),
    taggedOnly: ref(false), hideUntagged: ref(false),
    refresh: async () => { refreshes++; },
  };

  library.results = computed(() => library.media.value.filter((entry) => entry.favorite));

  const stage = {
    media: ref(items), total: ref(items.length), appliedSearch: ref("Stage query"), sortMode: ref("name-desc"),
    refreshRedistribution: async () => { refreshes++; },
  };

  stage.results = computed(() => stage.media.value);

  return { library, stage, refreshes: () => refreshes };
}

test("captures all ordered files, keeps navigation after removing a favorite, and refreshes on close", async () => {
  const model = models([item(1), item(2)]);

  globalThis.__nexaViewerTest = { native: true, invoke: async (command, args) => {
    assert.equal(command, "get_viewer_sequence");
    assert.equal(args.filters.search, "Applied tag");
    assert.equal(args.filters.scope, "favorites");
    return Array.from({ length: 505 }, (_, i) => ({ id: String(i + 1), filename: `${i + 1}.png`, kind: "image", extension: "png" }));
  } };

  const viewer = useMediaViewer(model.library, model.stage);
  await viewer.openViewer(model.library.media.value[0], false);
  viewer.viewerAnnotations({ favorite: false, tags: ["Changed"] });
  assert.equal(model.library.results.value.length, 1);
  assert.equal(viewer.viewerTotal.value, 505);
  viewer.moveViewer(1);
  assert.equal(viewer.viewerItem.value.nativeId, "2");
  for (let i = 0; i < 503; i++) viewer.moveViewer(1);
  assert.equal(viewer.viewerItem.value.nativeId, "505");
  assert.equal(viewer.viewerHasNext.value, false);
  viewer.moveViewer(1);
  assert.equal(viewer.viewerIndex.value, 504);
  viewer.moveViewer(-1);
  assert.equal(viewer.viewerItem.value.nativeId, "504");
  viewer.closeViewer();
  assert.equal(model.refreshes(), 1);
});

test("closing while capture is pending ignores its response", async () => {
  const model = models([item(1)]);

  let resolve;

  globalThis.__nexaViewerTest = { native: true, invoke: () => new Promise((done) => { resolve = done; }) };

  const viewer = useMediaViewer(model.library, model.stage);

  const pending = viewer.openViewer(model.library.media.value[0], false);
  assert.equal(viewer.viewerPaging.value, true);
  viewer.moveViewer(1);
  assert.equal(viewer.viewerIndex.value, 0);
  viewer.closeViewer();
  resolve([{ id: "9", filename: "9.png", kind: "image", extension: "png" }]);
  await pending;
  assert.equal(viewer.viewerOpen.value, false);
  assert.equal(viewer.viewerItem.value, undefined);
});

test("capture failure retains loaded navigation and reports the failure", async () => {
  const model = models([item(1), item(2)]);

  globalThis.__nexaViewerTest = { native: true, invoke: async () => { throw new Error("Unavailable"); } };

  const viewer = useMediaViewer(model.library, model.stage);
  await viewer.openViewer(model.library.media.value[0], false);
  assert.equal(viewer.viewerTotal.value, 2);
  assert.match(viewer.viewerError.value, /Unavailable/);
  assert.equal(viewer.viewerPaging.value, false);
  viewer.moveViewer(1);
  assert.equal(viewer.viewerItem.value.nativeId, "2");
});

test("Stage captures its applied search and demo retains the loaded order", async () => {
  const model = models([item(1), item(2)]);

  globalThis.__nexaViewerTest = { native: true, invoke: async (_command, args) => {
    assert.equal(args.library, false);
    assert.equal(args.search, "Stage query");
    assert.equal(args.filters.sort, "name-desc");
    assert.equal(model.library.sortMode.value, "newest");
    return [{ id: "2", filename: "2.png", kind: "image", extension: "png" }];
  } };

  const viewer = useMediaViewer(model.library, model.stage);
  await viewer.openViewer(model.stage.media.value[1], true);
  assert.equal(viewer.viewerItem.value.nativeId, "2");
  assert.equal(viewer.viewerTotal.value, 1);
  viewer.closeViewer();
  assert.equal(model.refreshes(), 1);

  globalThis.__nexaViewerTest.native = false;
  await viewer.openViewer(model.library.media.value[0], false);
  assert.equal(viewer.viewerTotal.value, 2);
});

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import vm from "node:vm";
import * as vue from "vue";
import { ModuleKind, transpileModule } from "typescript";

const sourceRoot = fileURLToPath(new URL("../src/", import.meta.url));

function fixture(native, demo) {
  const cache = new Map();
  const calls = [];
  const window = demo ? { __NEXA_DEMO__: true } : {};

  function load(file) {
    if (cache.has(file)) return cache.get(file);

    const exports = {};

    cache.set(file, exports);

    const source = readFileSync(file, "utf8").replaceAll(
      "import.meta.url",
      JSON.stringify(pathToFileURL(file).href),
    );
    const code = transpileModule(source, {
      compilerOptions: { module: ModuleKind.CommonJS },
    }).outputText;

    vm.runInNewContext(code, {
      exports,
      window,
      URL,
      setTimeout,
      clearTimeout,
      require(id) {
        if (id === "vue") return vue;
        if (id === "vue-i18n") return { useI18n: () => ({ t: (key) => key }) };

        if (id === "@tauri-apps/api/event") {
          return {
            listen: () => {
              calls.push("listen");
              throw new Error("Unexpected native event subscription");
            },
          };
        }

        if (id === "@tauri-apps/api/core") {
          return {
            isTauri: () => native,
            invoke: async (command) => {
              calls.push(command);
              throw new Error("Unexpected native data command");
            },
          };
        }

        if (id.startsWith("."))
          return load(path.resolve(path.dirname(file), `${id}.ts`));

        throw new Error(`Unexpected dependency: ${id}`);
      },
    });

    return exports;
  }

  return { load: (file) => load(path.join(sourceRoot, file)), calls };
}

for (const [name, native, demo, expected] of [
  ["browser demo", false, false, false],
  ["native debug demo", true, true, false],
  ["native production", true, false, true],
]) {
  test(`${name} selects the appropriate data backend`, () => {
    const f = fixture(native, demo);

    assert.equal(f.load("runtime.ts").isNativeData(), expected);
  });
}

for (const native of [false, true]) {
  test(`${native ? "native" : "browser"} demo ranks searched metadata and filenames before the selected sort`, () => {
    const f = fixture(native, true);
    const scope = vue.effectScope();
    const library = scope.run(() =>
      f.load("composables/useLibrary.ts").useLibrary(),
    );
    const stage = scope.run(() => f.load("composables/useStage.ts").useStage());
    try {
      const base = {
        ...library.media.value[0],
        copyrights: [],
        artists: [],
        artist: "",
        characters: [],
        tags: [],
      };
      library.media.value = [
        { ...base, id: "contains", tags: ["atomic"], modifiedAt: 3 },
        { ...base, id: "prefix", tags: ["Marvel"], modifiedAt: 1 },
        { ...base, id: "absent", tags: ["Other"], modifiedAt: 2 },
      ];
      library.search.value = "m";
      library.commitSearch();
      stage.media.value = [
        {
          ...base,
          id: "contains",
          filename: "a needle.mp4",
          path: "C:/files/a needle.mp4",
          modifiedAt: 3,
        },
        {
          ...base,
          id: "prefix",
          filename: "Needle.mp4",
          path: "C:/files/Needle.mp4",
          modifiedAt: 1,
        },
        {
          ...base,
          id: "path-only",
          filename: "Other.mp4",
          path: "C:/needle/Other.mp4",
          modifiedAt: 2,
        },
      ];
      stage.appliedSearch.value = "needle";
      for (const sort of ["newest", "older", "name-asc", "name-desc"]) {
        library.sortMode.value = sort;
        stage.sortMode.value = sort;
        assert.deepEqual(
          Array.from(library.results.value, (item) => item.id),
          ["prefix", "contains"],
        );
        assert.deepEqual(
          Array.from(stage.results.value, (item) => item.id),
          sort === "newest" || sort === "name-asc"
            ? ["prefix", "contains", "path-only"]
            : ["prefix", "path-only", "contains"],
        );
      }
      assert.equal(f.calls.length, 0);
    } finally {
      scope.stop();
    }
  });

  test(`${native ? "native" : "browser"} demo loads real mock libraries and saves annotations without IPC`, async () => {
    const f = fixture(native, true);
    const scope = vue.effectScope();
    const library = scope.run(() =>
      f.load("composables/useLibrary.ts").useLibrary(),
    );
    const stage = scope.run(() => f.load("composables/useStage.ts").useStage());

    try {
      assert.equal(library.media.value.length, 480);
      assert.equal(stage.media.value.length, 96);
      assert.equal(library.hasMore.value, false);
      assert.equal(stage.hasMore.value, false);

      await library.refresh();
      await stage.refreshRedistribution();
      await stage.loadMore();

      const item = library.media.value[0];
      const favorite = item.favorite;

      await library.toggleFavorite(item);
      assert.equal(item.favorite, !favorite);

      const editor = f.load("features/media/useTagEditor.ts").useTagEditor(
        library,
        () => {},
        () => {},
        vue.ref(null),
      );

      await editor.editTags(item);
      editor.newTag.value = "Demo-only";
      await editor.saveTags();
      assert.ok(item.tags.includes("Demo-only"));
      assert.equal(editor.tagTarget.value, null);

      const scans = f.load("composables/useStageScan.ts").useStageScan();

      await scans.connect();
      await scans.start();
      assert.equal(f.calls.length, 0);

      const nextSession = fixture(native, true);
      const nextLibrary = scope.run(() =>
        nextSession.load("composables/useLibrary.ts").useLibrary(),
      );

      assert.equal(nextLibrary.media.value[0].favorite, favorite);
      assert.equal(
        nextLibrary.media.value[0].tags.includes("Demo-only"),
        false,
      );
    } finally {
      scope.stop();
    }
  });
}

for (const native of [false, true]) {
  test(`Stage type filters combine with path search and retain the full queue in ${native ? "native" : "browser"} demo`, async () => {
    const f = fixture(native, true);
    const scope = vue.effectScope();
    const stage = scope.run(() => f.load("composables/useStage.ts").useStage());

    try {
      assert.equal(stage.queueTotal.value, 96);
      assert.deepEqual(
        { ...stage.typeCounts.value },
        { all: 96, video: 40, image: 56 },
      );
      stage.kind.value = "video";
      assert.equal(stage.results.value.length, 40);
      assert.ok(stage.results.value.every((item) => item.kind === "video"));
      stage.select(stage.results.value[0]);
      const key = stage.queryKey.value;
      stage.kind.value = "image";
      await vue.nextTick();
      assert.notEqual(stage.queryKey.value, key);
      assert.equal(stage.results.value.length, 56);
      assert.equal(stage.selectedIds.value.length, 0);
      stage.appliedSearch.value = "Videos";
      assert.equal(stage.results.value.length, 0);
      assert.deepEqual(
        { ...stage.typeCounts.value },
        { all: 40, video: 40, image: 0 },
      );
      assert.equal(stage.queueTotal.value, 96);
      stage.kind.value = "all";
      assert.equal(stage.results.value.length, 40);
      assert.ok(
        stage.results.value.every((item) =>
          item.relativePath.startsWith("Videos/"),
        ),
      );
      assert.equal(f.calls.length, 0);
    } finally {
      scope.stop();
    }
  });
}

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

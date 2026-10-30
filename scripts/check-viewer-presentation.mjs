import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { ModuleKind, transpileModule } from "typescript";
import { reactive } from "vue";

const source = await readFile(
  new URL("../src/components/MediaViewer.vue", import.meta.url),
  "utf8",
);
let sequence = 0;

async function fixture() {
  const context = {
    native: false,
    item: {
      id: "1",
      filename: "private-system-name.mp4",
      kind: "video",
      videoTitle: "Assigned title",
      artist: "Author",
      copyrights: ["Series"],
      characters: ["Alice"],
      tags: [],
      favorite: false,
      width: 3840,
      height: 2160,
      fps: 59.94,
    },
    invoke: async () => ({}),
  };
  context.item = reactive(context.item);
  globalThis.__viewerPresentation = context;
  globalThis.document = { activeElement: null };
  globalThis.HTMLElement = class {};
  globalThis.Element = class {};

  const setup = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];
  let code = transpileModule(
    setup +
      "\nexport { displayTitle, information, details, sourceFps, activePanel, panel, togglePanel, dismissPanel, outsidePanel, keyboard, image, canvas, mediaReady, fittedSize, fitImage, loadSource, source };",
    { compilerOptions: { module: ModuleKind.ESNext } },
  ).outputText;
  code = code.replace(/import[\s\S]*?from ["'][^"']+["'];/g, "");
  code = `import { computed, ref, nextTick } from ${JSON.stringify(import.meta.resolve("vue"))};
const context = globalThis.__viewerPresentation;
const defineProps = () => ({ item: context.item });
const defineEmits = () => () => {};
const useI18n = () => ({ t: (key) => key, locale: ref("en") });
const isNativeData = () => context.native;
const invoke = (...args) => context.invoke(...args);
const convertFileSrc = (path) => path;
const ui = (value) => value;
const useFocusTrap = () => ({ trapFocus: () => {} });
const watch = () => {};
const onMounted = () => {};
const onBeforeUnmount = () => {};
${code}\n// fixture ${sequence++}`;

  const viewer = await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  );

  return { viewer, context };
}

test("assigned titles and current native information replace lightweight page placeholders", async () => {
  const { viewer, context } = await fixture();
  assert.equal(viewer.displayTitle.value, "Assigned title");
  context.native = true;
  context.item.nativeId = "501";
  context.invoke = async () => ({
    path: "authorized-source",
    favorite: true,
    tags: ["Travel"],
    fps: 60,
    details: {
      videoTitle: "Beyond page 500",
      artist: "Current author",
      copyrights: ["Current series"],
      characters: [{ name: "Alice", copyright: "Current series" }],
      width: 1920,
      height: 1080,
    },
  });
  await viewer.loadSource();
  assert.equal(viewer.displayTitle.value, "Beyond page 500");
  assert.deepEqual(
    viewer.information.value.map((row) => row.values),
    [
      ["Beyond page 500"],
      ["Current series"],
      ["Current author"],
      ["Alice · Current series"],
      ["1920 × 1080"],
      ["60"],
    ],
  );
  context.item.kind = "image";
  assert.equal(viewer.displayTitle.value, "image");
  assert.equal(
    viewer.information.value.some((row) => row.label === "FPS"),
    false,
  );
});

test("panels are exclusive, Escape restores the trigger, and suggestion clicks retain tags", async () => {
  const { viewer } = await fixture();
  let restored = 0;
  const trigger = { focus: () => restored++ };
  viewer.panel.value = { focus: () => {} };
  await viewer.togglePanel("info", { currentTarget: trigger });
  await viewer.togglePanel("help", { currentTarget: trigger });
  assert.equal(viewer.activePanel.value, "help");
  viewer.keyboard({ key: "Escape", preventDefault() {}, target: {} });
  assert.equal(viewer.activePanel.value, null);
  assert.equal(restored, 1);

  await viewer.togglePanel("tags", { currentTarget: trigger });
  const suggestion = new globalThis.Element();
  suggestion.closest = () => ({});
  viewer.outsidePanel({ target: suggestion });
  assert.equal(viewer.activePanel.value, "tags");
  const outside = new globalThis.Element();
  outside.closest = () => null;
  viewer.panel.value = { contains: () => false };
  viewer.outsidePanel({ target: outside });
  assert.equal(viewer.activePanel.value, null);
});

test("fit scales portrait and landscape media without stretching and recomputes on resize", async () => {
  const { viewer } = await fixture();
  viewer.image.value = { naturalWidth: 640, naturalHeight: 360 };
  viewer.canvas.value = { clientWidth: 1200, clientHeight: 800 };
  viewer.mediaReady.value = true;
  viewer.fitImage();
  assert.deepEqual(viewer.fittedSize.value, { width: 1200, height: 675 });
  viewer.image.value = { naturalWidth: 400, naturalHeight: 800 };
  viewer.fitImage();
  assert.deepEqual(viewer.fittedSize.value, { width: 400, height: 800 });
  viewer.canvas.value = { clientWidth: 600, clientHeight: 400 };
  viewer.fitImage();
  assert.deepEqual(viewer.fittedSize.value, { width: 200, height: 400 });
});

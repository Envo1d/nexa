import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { transpileModule, ModuleKind, ScriptTarget } from "typescript";
import { computed, nextTick, ref, shallowRef } from "vue";

const options = {
  compilerOptions: { module: ModuleKind.ESNext, target: ScriptTarget.ES2022 },
};
const matching = transpileModule(
  await readFile(
    new URL("../src/features/media/duplicateMatching.ts", import.meta.url),
    "utf8",
  ),
  options,
).outputText;

const matcher = await import(
  `data:text/javascript;base64,${Buffer.from(matching).toString("base64")}`
);

const component = await readFile(
  new URL("../src/components/ContentDuplicatesDialog.vue", import.meta.url),
  "utf8",
);

const script = transpileModule(
  component.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1],
  options,
).outputText.replace(/^import[\s\S]*?from\s+"[^"]+";\s*/gm, "");

let sequence = 0;

async function dialog(invoke, saveHandler) {
  const cleanups = [];
  const checkpoints = [];

  globalThis.__duplicateDialogCheck = {
    computed,
    nextTick,
    ref,
    shallowRef,
    ...matcher,
    invoke: (command, args) => {
      if (command === "save_duplicate_search") {
        checkpoints.push(args.checkpoint);
        return saveHandler ? saveHandler(args.checkpoint) : Promise.resolve();
      }

      return invoke(command, args);
    },
    Channel: class {
      onmessage = () => {};
    },
    defineProps: () => ({}),
    defineEmits: () => () => {},
    useI18n: () => ({ t: (key) => key }),
    useFocusTrap: () => ({ trapFocus: () => {} }),
    onMounted: () => {},
    onBeforeUnmount: (callback) => cleanups.push(callback),
  };
  const source = `const { computed, nextTick, ref, shallowRef, createDuplicateMatcher, duplicatePairKey, invoke, Channel, defineProps, defineEmits, useI18n, useFocusTrap, onMounted, onBeforeUnmount } = globalThis.__duplicateDialogCheck;
${script}
export { scan, loadSaved, reviewPair, processed, total, progress, checked, pairs, selected, rejectedCount, lastRejected, stopped, status, running, error, savedAt, reused, hashed };
// instance ${++sequence}`;

  const subject = await import(
    `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`
  );

  return {
    ...subject,
    checkpoints,
    dispose: () => cleanups.forEach((callback) => callback()),
  };
}

function file(id, sha256 = id) {
  return {
    id,
    sha256,
    main: true,
    visual: "0000",
    filename: `${id}.png`,
    kind: "image",
    width: 100,
    height: 100,
    copyrights: [],
    characters: [],
    tags: [],
  };
}

function page(files, processed = files.length, more = false) {
  return {
    files,
    processed,
    after: "16",
    more,
    skipped: processed - files.length,
    visualUnavailable: 0,
    scope: "folders",
    reused: 0,
    hashed: files.length,
  };
}

test("file-level progress counts failed attempts and preserves a captured scan boundary", async () => {
  let subject;

  subject = await dialog(async (command, args) => {
    if (command === "prepare_duplicate_scan")
      return {
        total: 3,
        upperId: "42",
        scope: "folders",
        rejections: [["a", "b"]],
      };

    assert.equal(args.upperId, "42");
    assert.equal(args.scope, "folders");
    args.onProgress.onmessage({ completed: 1 });
    assert.equal(subject.processed.value, 1);
    assert.equal(subject.progress.value, 33);

    return page([file("a"), file("b")], 3);
  });

  await subject.scan();

  assert.equal(subject.processed.value, 3);
  assert.equal(subject.checked.value, 2);
  assert.equal(subject.progress.value, 100);
  assert.equal(subject.rejectedCount.value, 1);
  assert.equal(subject.pairs.value.length, 0);
  assert.equal(subject.status.value, "duplicatesComplete");
});

test("stopping finishes the current batch and never requests another", async () => {
  let resolveBatch;
  let batchCalls = 0;

  const subject = await dialog(async (command) => {
    if (command === "prepare_duplicate_scan")
      return {
        total: 10,
        upperId: "10",
        scope: "folders",
        rejections: [],
      };

    batchCalls++;
    return new Promise((resolve) => {
      resolveBatch = resolve;
    });
  });

  const scanning = subject.scan();

  await Promise.resolve();

  subject.stopped.value = true;
  resolveBatch(page([file("a")], 2, true));

  await scanning;

  assert.equal(batchCalls, 1);
  assert.equal(subject.status.value, "duplicatesStopped");
  assert.equal(subject.progress.value, 20);
});

test("closing during preparation suppresses subsequent fingerprint requests", async () => {
  let resolveInventory;
  let calls = 0;

  const subject = await dialog(() => {
    calls++;
    return new Promise((resolve) => {
      resolveInventory = resolve;
    });
  });

  const scanning = subject.scan();

  subject.dispose();
  resolveInventory({
    total: 10,
    upperId: "10",
    scope: "folders",
    rejections: [],
  });

  await scanning;

  assert.equal(calls, 1);
});

test("failed review saves preserve the pair; successful rejection and undo persist opposite decisions", async () => {
  const decisions = [];
  let fail = true;

  const subject = await dialog(async (command, args) => {
    assert.equal(command, "set_duplicate_rejection");
    decisions.push(args.rejected);

    if (fail) throw new Error("disk unavailable");
  });

  const pair = {
    left: { item: file("a"), sha256: "a" },
    right: { item: file("b"), sha256: "b" },
    exact: false,
    distance: 0,
  };

  subject.pairs.value = [pair];
  subject.selected.value = pair;

  await subject.reviewPair(true);

  assert.equal(subject.pairs.value.length, 1);
  assert.equal(subject.selected.value, pair);
  assert.match(subject.error.value, /disk unavailable/);
  fail = false;

  await subject.reviewPair(true);

  assert.equal(subject.pairs.value.length, 0);
  assert.equal(subject.rejectedCount.value, 1);

  await subject.reviewPair(false);

  assert.equal(subject.pairs.value.length, 1);
  assert.equal(subject.selected.value, pair);
  assert.equal(subject.rejectedCount.value, 0);
  assert.deepEqual(decisions, [true, true, false]);
});

test("exact matches cannot be rejected and an empty inventory completes without hashing", async () => {
  let calls = 0;

  const subject = await dialog(async () => {
    calls++;
    return {
      total: 0,
      upperId: "0",
      scope: "folders",
      rejections: [],
    };
  });

  subject.selected.value = { exact: true };

  await subject.reviewPair(true);

  assert.equal(calls, 0);

  await subject.scan();

  assert.equal(calls, 1);
  assert.equal(subject.status.value, "duplicatesComplete");
  assert.equal(subject.progress.value, 100);
});

test("saved results restore without inventory preparation or hashing and retain partial status", async () => {
  const calls = [];

  const subject = await dialog(async (command) => {
    calls.push(command);
    return {
      snapshot: {
        updatedAt: 123456,
        status: "stopped",
        report: {
          total: 10,
          processed: 4,
          checked: 4,
          skipped: 0,
          visualUnavailable: 0,
          reused: 3,
          hashed: 1,
        },
      },
      files: [file("1", "same"), file("2", "same")],
      pairs: [
        {
          leftId: "1",
          rightId: "2",
          exact: true,
          distance: 0,
        },
      ],
      rejections: [],
      stale: 0,
    };
  });

  await subject.loadSaved();

  assert.deepEqual(calls, ["load_duplicate_search"]);
  assert.equal(subject.pairs.value.length, 1);
  assert.equal(subject.savedAt.value, 123456);
  assert.equal(subject.status.value, "duplicatesSavedPartial");
  assert.equal(subject.progress.value, 40);
  assert.equal(subject.reused.value, 3);
});

test("completed empty saved searches remain distinct from never-scanned folders", async () => {
  const subject = await dialog(async () => ({
    snapshot: {
      updatedAt: 100,
      status: "complete",
      report: {
        total: 0,
        processed: 0,
        checked: 0,
        skipped: 0,
        visualUnavailable: 0,
        reused: 0,
        hashed: 0,
      },
    },
    files: [],
    pairs: [],
    rejections: [],
    stale: 0,
  }));

  await subject.loadSaved();

  assert.equal(subject.status.value, "duplicatesSavedComplete");
  assert.equal(subject.progress.value, 100);
  assert.equal(subject.pairs.value.length, 0);
  assert.equal(subject.savedAt.value, 100);
});

test("updates checkpoint partial and final results with the captured generation", async () => {
  let batch = 0;

  const subject = await dialog(async (command) => {
    if (command === "prepare_duplicate_scan")
      return {
        total: 2,
        upperId: "2",
        scope: "folders",
        generation: 8,
        rejections: [],
      };

    batch++;
    return {
      ...page([file(String(batch), "same")], 1, batch === 1),
      reused: 1,
      hashed: 0,
    };
  });

  await subject.scan();

  assert.deepEqual(
    subject.checkpoints.map((value) => value.status),
    ["partial", "complete"],
  );
  assert(
    subject.checkpoints.every(
      (value) => value.generation === 8 && value.scope === "folders",
    ),
  );
  assert.equal(subject.checkpoints[1].pairs.length, 1);
  assert.equal(subject.checkpoints[1].report.reused, 2);
  assert.equal(subject.checkpoints[1].report.hashed, 0);
});

test("checkpoint failure is visible and stops further batches without claiming completion", async () => {
  let batches = 0;

  const subject = await dialog(
    async (command) => {
      if (command === "prepare_duplicate_scan")
        return {
          total: 20,
          upperId: "20",
          scope: "folders",
          generation: 1,
          rejections: [],
        };

      batches++;
      return page([file("1")], 1, true);
    },
    async () => {
      throw new Error("checkpoint write failed");
    },
  );

  await subject.scan();

  assert.equal(batches, 1);
  assert.equal(subject.status.value, "duplicatesFailed");
  assert.match(subject.error.value, /checkpoint write failed/);
});

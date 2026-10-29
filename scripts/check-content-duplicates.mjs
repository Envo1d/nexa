import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { transpileModule, ModuleKind, ScriptTarget } from "typescript";

const code = transpileModule(
  await readFile(
    new URL("../src/features/media/duplicateMatching.ts", import.meta.url),
    "utf8",
  ),
  {
    compilerOptions: { module: ModuleKind.ESNext, target: ScriptTarget.ES2022 },
  },
).outputText;

const { createDuplicateMatcher, duplicatePairKey, hamming } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
);

function file(
  id,
  sha256,
  visual = null,
  width = 100,
  height = 100,
  kind = "image",
) {
  return {
    item: {
      id,
      filename: `${id}.png`,
      width,
      height,
      kind,
    },
    main: true,
    sha256,
    visual,
  };
}

test("rejected content pairs survive rename and reverse order while changed content can match", () => {
  const rejected = new Set([duplicatePairKey("a", "b")]);

  const matcher = createDuplicateMatcher(1000, rejected);

  matcher.add(file("renamed-b", "b", "0000"));
  matcher.add(file("renamed-a", "a", "0000"));
  assert.equal(matcher.pairs.length, 0);
  matcher.add(file("edited-a", "changed", "0000"));
  assert.equal(matcher.pairs.length, 2);
  rejected.delete(duplicatePairKey("b", "a"));
  const restored = createDuplicateMatcher(1000, rejected);

  restored.add(file("a", "a", "0000"));
  restored.add(file("b", "b", "0000"));
  assert.equal(restored.pairs.length, 1);
});

test("excluded visual candidates do not consume the result budget or hide exact copies", () => {
  const matcher = createDuplicateMatcher(
    1,
    new Set([duplicatePairKey("a", "b")]),
  );

  matcher.add(file("a", "a", "0000"));
  matcher.add(file("b", "b", "0000"));
  assert.equal(matcher.full, false);
  matcher.add(file("copy", "a", "0000"));
  assert.equal(matcher.pairs.length, 1);
  assert.equal(matcher.pairs[0].exact, true);
});

test("renamed exact copies use a stable anchor and take priority over visual matching", () => {
  const matcher = createDuplicateMatcher();

  matcher.add(file("original", "same", "1000"));
  matcher.add(file("renamed", "same", "1000"));
  matcher.add(file("third", "same", "1000"));
  matcher.add(
    file("different-extension-kind", "same", null, 100, 100, "video"),
  );
  assert.equal(matcher.pairs.length, 3);
  assert(
    matcher.pairs.every(
      (pair) => pair.exact && pair.left.item.id === "original",
    ),
  );
});

test("visual candidates preserve direct distance rather than transitive groups", () => {
  const matcher = createDuplicateMatcher();

  matcher.add(file("a", "a", "0000"));
  matcher.add(file("b", "b", "00ff", 200, 200));
  matcher.add(file("c", "c", "ffff"));
  assert.deepEqual(
    matcher.pairs.map((pair) => [
      pair.left.item.id,
      pair.right.item.id,
      pair.distance,
    ]),
    [
      ["a", "b", 8],
      ["b", "c", 8],
    ],
  );
});

test("unknown dimensions, aspect differences and videos do not produce visual matches", () => {
  const matcher = createDuplicateMatcher();

  matcher.add(file("a", "a", "0000"));
  matcher.add(file("portrait", "p", "0000", 100, 300));
  matcher.add(file("unknown", "u", "0000", 0, 0));
  matcher.add(file("video", "v", "0000", 100, 100, "video"));
  assert.equal(matcher.pairs.length, 0);
});

test("result limits stay bounded, including identical visual hashes", () => {
  const matcher = createDuplicateMatcher(5);

  for (let index = 0; index < 20; index++)
    matcher.add(file(String(index), String(index), "0000"));

  assert.equal(matcher.pairs.length, 5);
  assert(matcher.full);
});

test("metric-tree retrieval agrees with exhaustive comparisons across multiple batches", () => {
  const matcher = createDuplicateMatcher();

  const files = Array.from({ length: 150 }, (_, index) =>
    file(
      String(index),
      String(index),
      ((BigInt(index) * 0x9e3779b97f4a7c15n) & ((1n << 63n) - 1n)).toString(16),
    ),
  );

  const expected = [];

  for (let right = 0; right < files.length; right++) {
    for (let left = 0; left < right; left++) {
      if (
        hamming(
          BigInt(`0x${files[left].visual}`),
          BigInt(`0x${files[right].visual}`),
        ) <= 8
      )
        expected.push(`${left}:${right}`);
    }

    matcher.add(files[right]);
  }

  assert.deepEqual(
    matcher.pairs
      .map((pair) => `${pair.left.item.id}:${pair.right.item.id}`)
      .sort(),
    expected.sort(),
  );
});

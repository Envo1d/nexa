import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";
import { transpileModule, ModuleKind } from "typescript";

// Verify real palette values without launching a browser or WebView.
const css = await readFile(
  new URL("../src/themes.css", import.meta.url),
  "utf8",
);

const blocks = [...css.matchAll(/(:root[^{}]*)\{([^{}]+)\}/g)];

function declarations(body) {
  return Object.fromEntries(
    [...body.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [
      name,
      value.trim(),
    ]),
  );
}

const defaults = declarations(blocks[0][2]);

const palettes = new Map([["midnight-violet", defaults]]);

for (const [, selector, body] of blocks.slice(1)) {
  const name = selector.match(/data-theme="([^"]+)"/)[1];

  palettes.set(name, { ...defaults, ...declarations(body) });
}

function color(value, palette) {
  const variable = value.match(/^var\(--([\w-]+)\)$/);

  if (variable) return color(palette[variable[1]], palette);

  if (/^#[\da-f]{6}$/i.test(value)) {
    return value
      .slice(1)
      .match(/../g)
      .map((channel) => parseInt(channel, 16));
  }

  const mix = value.match(
    /^color-mix\(in srgb, (var\(--[\w-]+\)) (\d+)%, (var\(--[\w-]+\))\)$/,
  );

  assert.ok(mix, `Unsupported color expression: ${value}`);

  const first = color(mix[1], palette);

  const second = color(mix[3], palette);

  const weight = Number(mix[2]) / 100;

  return first.map(
    (channel, index) => channel * weight + second[index] * (1 - weight),
  );
}

function luminance(channels) {
  const linear = channels.map((channel) => {
    const scaled = channel / 255;

    return scaled <= 0.04045
      ? scaled / 12.92
      : ((scaled + 0.055) / 1.055) ** 2.4;
  });

  return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
}

function contrast(palette, foreground, background) {
  const first = luminance(color(palette[foreground], palette));

  const second = luminance(color(palette[background], palette));

  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

const pairs = [
  ...[
    "shell",
    "surface",
    "surface-alt",
    "surface-raised",
    "surface-input",
    "surface-hover",
    "surface-pressed",
  ].map((background) => ["text", background]),
  ...[
    "shell",
    "surface",
    "surface-alt",
    "surface-raised",
    "surface-input",
    "surface-hover",
  ].map((background) => ["muted", background]),
  ["subtle", "surface"],
  ["subtle", "shell"],
  ["text-accent", "accent-soft"],
  ["text-accent", "accent-soft-hover"],
  ["on-accent", "accent-strong"],
  ["on-accent", "accent-end"],
  ["on-accent", "accent-hover"],
  ["on-accent", "danger-button"],
  ["on-accent", "danger-button-end"],
  ["on-accent", "danger-button-hover"],
  ["danger", "danger-bg"],
  ["warning", "warning-bg"],
  ["info", "info-bg"],
  ["copyright-text", "copyright-bg"],
  ["artist-text", "artist-bg"],
  ["character-text", "character-bg"],
];

for (const [name, palette] of palettes) {
  test(`${name}: text, actions and semantic states have readable contrast`, () => {
    for (const [foreground, background] of pairs) {
      const ratio = contrast(palette, foreground, background);

      assert.ok(
        ratio >= 4.5,
        `${name}: ${foreground} on ${background} is ${ratio.toFixed(2)}:1`,
      );
    }

    assert.ok(contrast(palette, "accent", "surface") >= 3);
  });
}

test("interface styles reference defined shared color tokens", async () => {
  const directory = new URL("../src/components/", import.meta.url);

  const files = (await readdir(directory)).filter((name) =>
    name.endsWith(".vue"),
  );

  const sources = await Promise.all([
    readFile(new URL("../src/styles.css", import.meta.url), "utf8"),
    ...files.map((name) => readFile(new URL(name, directory), "utf8")),
  ]);

  for (const source of sources) {
    for (const [, token] of source.matchAll(/var\(--([\w-]+)\)/g)) {
      assert.ok(token in defaults, `Undefined theme token: ${token}`);
    }
  }
});

test("theme application uses all palettes and falls back for an unknown value", async () => {
  const source = await readFile(
    new URL("../src/themes.ts", import.meta.url),
    "utf8",
  );

  const code = transpileModule(source, {
    compilerOptions: { module: ModuleKind.ESNext },
  }).outputText.replace(
    'from "vue"',
    `from ${JSON.stringify(import.meta.resolve("vue"))}`,
  );

  const module = await import(
    `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
  );

  assert.deepEqual([...palettes.keys()], [...module.themeIds]);

  const previous = globalThis.document;

  globalThis.document = { documentElement: { dataset: {} } };

  try {
    for (const theme of module.themeIds) {
      module.applyTheme(theme);
      assert.equal(document.documentElement.dataset.theme, theme);
      assert.equal(module.themePreference.value, theme);
    }

    module.applyTheme("unknown");
    assert.equal(document.documentElement.dataset.theme, "midnight-violet");
    assert.equal(module.themePreference.value, "midnight-violet");
  } finally {
    if (previous === undefined) delete globalThis.document;
    else globalThis.document = previous;
  }
});

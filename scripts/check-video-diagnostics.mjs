import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  overlayArgument,
  videoDiagnosticConfig,
} from "./dev-video-diagnostics.mjs";

const config = JSON.parse(
  await readFile(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
    "utf8",
  ),
);

test("diagnostic arguments preserve main window settings and leave production config unchanged", () => {
  const before = JSON.stringify(config);

  const result = videoDiagnosticConfig(config);

  const { additionalBrowserArgs, dataDirectory, ...main } =
    result.app.windows[0];

  assert.deepEqual(main, config.app.windows[0]);
  assert.equal(JSON.stringify(config), before);
  assert.ok(additionalBrowserArgs.includes(overlayArgument));
  assert.ok(
    additionalBrowserArgs.includes(
      "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    ),
  );
  assert.match(dataDirectory, /^NexaNew\//);
  assert.doesNotMatch(
    additionalBrowserArgs,
    /--disable-gpu|--disable-accelerated-video-decode|--ignore-gpu-blocklist/,
  );
  assert.equal(config.app.windows[0].additionalBrowserArgs, undefined);
});

test("baseline omits the workaround and uses a separate profile", () => {
  const baseline = videoDiagnosticConfig(config, "baseline").app.windows[0];
  const diagnostic = videoDiagnosticConfig(config).app.windows[0];

  assert.ok(!baseline.additionalBrowserArgs.includes(overlayArgument));
  assert.notEqual(baseline.dataDirectory, diagnostic.dataDirectory);
});

test("only main is changed and existing explicit arguments survive", () => {
  const custom = {
    app: {
      windows: [
        { label: "other", width: 400 },
        {
          label: "main",
          width: 1000,
          additionalBrowserArgs: "--force-color-profile=srgb",
        },
      ],
    },
  };

  const result = videoDiagnosticConfig(custom);

  assert.deepEqual(result.app.windows[0], custom.app.windows[0]);
  assert.equal(
    result.app.windows[1].additionalBrowserArgs,
    `--force-color-profile=srgb ${overlayArgument}`,
  );
});

test("invalid modes, missing main and an already altered baseline are rejected", () => {
  assert.throws(() => videoDiagnosticConfig(config, "disable-gpu"));
  assert.throws(() => videoDiagnosticConfig({ app: { windows: [] } }));
  assert.throws(() =>
    videoDiagnosticConfig({ app: { windows: [{ label: "other" }] } }),
  );
  assert.throws(() =>
    videoDiagnosticConfig({
      app: { windows: [{ additionalBrowserArgs: overlayArgument }] },
    }),
  );
});

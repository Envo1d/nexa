import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

import {
  githubApi,
  inspectRelease,
  publishRelease,
} from "./github-release.mjs";

const sha = "a".repeat(40);
const version = "2.8.0";

function fixture({
  reference = null,
  release = null,
  latest = null,
  failUpload = false,
} = {}) {
  const operations = [];
  let currentReference = reference;
  let currentRelease = release;

  const api = async (path, options = {}) => {
    if (path === "git/ref/tags/v2.8.0") {
      return currentReference;
    }

    if (path === "git/tags/annotated") {
      return { object: { type: "commit", sha } };
    }

    if (path === "releases/tags/v2.8.0") {
      return currentRelease;
    }

    if (path === "releases/latest") {
      return latest;
    }

    if (path === "git/refs") {
      operations.push({ path, ...options });
      currentReference = { object: { type: "commit", sha: options.body.sha } };
      return currentReference;
    }

    throw new Error(`Unexpected API call: ${path}`);
  };

  const run = (args) => {
    operations.push(args);

    if (args[1] === "create") {
      currentRelease = { draft: true, assets: [], target_commitish: sha };
    }

    if (failUpload && args[1] === "upload") {
      throw new Error("Upload failed");
    }
  };

  return {
    api,
    run,
    operations,
    version,
    sha,
    assets: ["setup.exe", "package.nupkg", "feed.json", "feed.sig"],
    log: () => {},
  };
}

test("creates tag on the built SHA and publishes only after assets upload", async () => {
  const context = fixture();

  await publishRelease(context);

  assert.deepEqual(context.operations[0], {
    path: "git/refs",
    method: "POST",
    body: { ref: "refs/tags/v2.8.0", sha },
  });
  assert.deepEqual(
    context.operations.slice(1).map((args) => args[1]),
    ["create", "upload", "edit"],
  );
  assert.ok(context.operations[1].includes("--draft"));
  assert.ok(context.operations[3].includes("--draft=false"));
  assert.ok(context.operations[3].includes("--latest"));
});

test("upload failure leaves the release unpublished and permits resuming its draft", async () => {
  const context = fixture({ failUpload: true });

  await assert.rejects(publishRelease(context), /Upload failed/);
  assert.equal(
    context.operations.some(
      (operation) => Array.isArray(operation) && operation[1] === "edit",
    ),
    false,
  );

  const retry = fixture({
    reference: { object: { type: "commit", sha } },
    release: { draft: true, assets: [], target_commitish: sha },
  });

  await publishRelease(retry);
  assert.deepEqual(
    retry.operations.map((args) => args[1]),
    ["upload", "edit"],
  );
});

test("rejects a published version without any mutation", async () => {
  const context = fixture({
    reference: { object: { type: "commit", sha } },
    release: { draft: false, assets: [] },
  });

  await assert.rejects(publishRelease(context), /already published/);
  assert.deepEqual(context.operations, []);
});

test("never moves an existing version tag from a different commit", async () => {
  const context = fixture({
    reference: { object: { type: "commit", sha: "b".repeat(40) } },
  });

  await assert.rejects(publishRelease(context), /another commit/);
  assert.deepEqual(context.operations, []);
});

test("accepts an annotated version tag resolving to the built commit", async () => {
  const context = fixture({
    reference: { object: { type: "tag", sha: "annotated" } },
  });

  assert.equal((await inspectRelease(context)).tag, "v2.8.0");
});

test("refuses unrelated tagless drafts and legacy NSIS drafts", async () => {
  const unrelated = fixture({
    release: { draft: true, assets: [], target_commitish: "main" },
  });

  await assert.rejects(publishRelease(unrelated), /does not identify/);
  assert.deepEqual(unrelated.operations, []);

  for (const name of [
    "latest.json",
    "Nexa_2.8.0_x64-setup.exe",
    "Nexa_2.8.0_x64-setup.exe.sig",
  ]) {
    const legacy = fixture({
      reference: { object: { type: "commit", sha } },
      release: { draft: true, assets: [{ name }] },
    });

    await assert.rejects(publishRelease(legacy), /legacy NSIS/);
    assert.deepEqual(legacy.operations, []);
  }
});

test("rejects invalid stable versions and commit identities before remote access", async () => {
  for (const override of [{ version: "2.8.0-beta.1" }, { sha: "main" }]) {
    await assert.rejects(
      inspectRelease({ ...fixture(), ...override }),
      /full commit SHA/,
    );
  }
});

test("prevents older queued builds from replacing a newer stable release", async () => {
  for (const tag_name of ["v2.8.0", "v2.9.0", "v3.0.0"]) {
    const context = fixture({ latest: { tag_name } });

    await assert.rejects(publishRelease(context), /higher than latest stable/);
    assert.deepEqual(context.operations, []);
  }

  for (const tag_name of ["v2.7.9", "v2.7.10", "v1.99.99"]) {
    await inspectRelease(fixture({ latest: { tag_name } }));
  }
});

test("CLI rejects dev, tag and pull-request publication before accessing GitHub", () => {
  for (const [GITHUB_REF, GITHUB_EVENT_NAME] of [
    ["refs/heads/dev", "push"],
    ["refs/tags/v2.8.0", "push"],
    ["refs/heads/main", "pull_request"],
  ]) {
    const result = spawnSync(
      process.execPath,
      ["scripts/github-release.mjs", "publish"],
      {
        env: {
          ...process.env,
          GITHUB_REF,
          GITHUB_EVENT_NAME,
          GH_TOKEN: "",
          GITHUB_TOKEN: "",
        },
        encoding: "utf8",
      },
    );

    assert.equal(result.status, 1);
    assert.match(result.stderr, /only for main pushes/);
  }
});

test("only HTTP 404 means optional absence; authorization and server failures abort", async () => {
  for (const status of [401, 403, 429, 500]) {
    const api = githubApi({
      token: "test",
      repository: "owner/repo",
      fetchImpl: async () => new Response("", { status }),
    });

    await assert.rejects(
      api("releases/tags/v2.8.0", { optional: true }),
      new RegExp(`HTTP ${status}`),
    );
  }

  const api = githubApi({
    token: "test",
    repository: "owner/repo",
    fetchImpl: async () => new Response("", { status: 404 }),
  });

  assert.equal(await api("releases/tags/v2.8.0", { optional: true }), null);
  await assert.rejects(
    api("git/refs", { method: "POST", body: {} }),
    /HTTP 404/,
  );
});

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const releaseNotes =
  "Download Nexa-win-x64-Setup.exe. Existing NSIS users must close Nexa, uninstall the old application and install this setup once; preserve NexaNew settings/database and media folders. Future updates use Velopack packages.";

export function githubApi({ token, repository, fetchImpl = fetch }) {
  return async (path, { method = "GET", body, optional = false } = {}) => {
    const response = await fetchImpl(
      `https://api.github.com/repos/${repository}/${path}`,
      {
        method,
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: `Bearer ${token}`,
          "X-GitHub-Api-Version": "2022-11-28",
          "Content-Type": "application/json",
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      },
    );

    if (optional && response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(
        `GitHub ${method} ${path} failed (HTTP ${response.status}).`,
      );
    }

    return response.status === 204 ? null : response.json();
  };
}

export async function inspectRelease({ api, version, sha }) {
  if (!/^\d+\.\d+\.\d+$/.test(version) || !/^[a-f0-9]{40}$/.test(sha)) {
    throw new Error(
      "Stable release requires an x.y.z version and a full commit SHA.",
    );
  }

  const tag = `v${version}`;
  const reference = await api(`git/ref/tags/${tag}`, { optional: true });
  let object = reference?.object;

  // Annotated tags point to tag objects; compare their final commit, not the tag SHA.
  for (let depth = 0; object?.type === "tag" && depth < 10; depth += 1) {
    object = (await api(`git/tags/${object.sha}`)).object;
  }

  if (reference && (object?.type !== "commit" || object.sha !== sha)) {
    throw new Error(
      `${tag} already points to another commit. Increase the version; tags are never moved.`,
    );
  }

  const release = await api(`releases/tags/${tag}`, { optional: true });

  if (release && !release.draft) {
    throw new Error(`${tag} is already published. Increase the version.`);
  }

  if (release && !reference && release.target_commitish !== sha) {
    throw new Error(
      "Existing draft does not identify the built commit. Remove the draft or increase the version.",
    );
  }

  const legacyAssets = release?.assets.some(
    ({ name }) =>
      name === "latest.json" || /_x64-setup\.exe(\.sig)?$/.test(name),
  );

  if (legacyAssets) {
    throw new Error(
      "Draft contains legacy NSIS assets. Remove the draft or increase the version.",
    );
  }

  const latest = await api("releases/latest", { optional: true });

  if (latest) {
    const latestVersion = latest.tag_name
      .match(/^v(\d+)\.(\d+)\.(\d+)$/)
      ?.slice(1)
      .map(BigInt);
    const nextVersion = version.split(".").map(BigInt);
    const differingPart = nextVersion.findIndex(
      (part, index) => part !== latestVersion?.[index],
    );

    if (
      !latestVersion ||
      differingPart === -1 ||
      nextVersion[differingPart] < latestVersion[differingPart]
    ) {
      throw new Error(
        `Release version must be higher than latest stable ${latest.tag_name}.`,
      );
    }
  }

  return { tag, reference, release };
}

export async function publishRelease({
  api,
  version,
  sha,
  assets,
  run,
  log = console.log,
}) {
  // Recheck remote state after compilation and immediately before mutation.
  const { tag, reference, release } = await inspectRelease({
    api,
    version,
    sha,
  });

  if (!reference) {
    log(`Creating ${tag} on built commit ${sha}.`);
    await api("git/refs", {
      method: "POST",
      body: { ref: `refs/tags/${tag}`, sha },
    });
  }

  if (!release) {
    run([
      "release",
      "create",
      tag,
      "--verify-tag",
      "--draft",
      "--title",
      `Nexa ${tag}`,
      "--target",
      sha,
      "--notes",
      releaseNotes,
    ]);
  }

  log(`Uploading signed Windows assets to draft ${tag}.`);
  run(["release", "upload", tag, ...assets, "--clobber"]);

  // An upload failure throws before this point; incomplete releases stay unpublished.
  await inspectRelease({ api, version, sha });
  run([
    "release",
    "edit",
    tag,
    "--draft=false",
    "--prerelease=false",
    "--latest",
    "--verify-tag",
  ]);
  log(`Published Nexa ${tag} from ${sha}.`);
}

async function main() {
  const mode = process.argv[2];
  const {
    GITHUB_REF: ref,
    GITHUB_EVENT_NAME: event,
    GITHUB_REPOSITORY: repository,
    GITHUB_SHA: sha,
    GH_TOKEN: token,
  } = process.env;

  if (
    ref !== "refs/heads/main" ||
    !["push", "workflow_dispatch"].includes(event)
  ) {
    throw new Error(
      "Releases are allowed only for main pushes or manual runs on main.",
    );
  }

  if (!["check", "publish"].includes(mode) || !token || !repository) {
    throw new Error(
      "Expected check/publish mode and GitHub repository credentials.",
    );
  }

  execFileSync(process.execPath, ["scripts/validate-release.mjs"], {
    stdio: "inherit",
  });

  const { version } = JSON.parse(readFileSync("package.json", "utf8"));
  const api = githubApi({ token, repository });

  if (mode === "check") {
    await inspectRelease({ api, version, sha });
    console.log(`Release preflight passed: v${version} at ${sha}.`);
    return;
  }

  const directory = `src-tauri/target/release/bundle/velopack/${version}`;

  execFileSync(process.execPath, ["scripts/validate-velopack.mjs", directory], {
    stdio: "inherit",
  });

  const assets = [
    "Nexa-win-x64-Setup.exe",
    `Nexa-${version}-win-x64-full.nupkg`,
    "releases.win-x64.json",
    "releases.win-x64.json.sig",
  ].map((name) => resolve(directory, name));

  for (const asset of assets) {
    if (!statSync(asset).isFile() || statSync(asset).size === 0) {
      throw new Error(`Missing or empty release asset: ${asset}`);
    }
  }

  await publishRelease({
    api,
    version,
    sha,
    assets,
    run: (args) => execFileSync("gh", args, { stdio: "inherit" }),
  });
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  main().catch((error) => {
    console.error(`Windows release failed: ${error.message}`);
    process.exitCode = 1;
  });
}

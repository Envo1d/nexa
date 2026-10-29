import { readFileSync } from "node:fs";

const pkg = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);

const config = JSON.parse(
  readFileSync(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
    "utf8",
  ).replace(/^\uFEFF/, ""),
);

const cargo = readFileSync(
  new URL("../src-tauri/Cargo.toml", import.meta.url),
  "utf8",
);

const rustVersion = cargo.match(
  /\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
)?.[1];

if (pkg.version !== config.version || rustVersion !== config.version) {
  throw new Error(
    "Keep package.json, src-tauri/Cargo.toml and tauri.conf.json versions identical.",
  );
}

if (
  process.env.GITHUB_REF_TYPE === "tag" &&
  process.env.GITHUB_REF_NAME !== `v${config.version}`
) {
  throw new Error(`Release tag must be v${config.version}.`);
}

console.log(`Release version validated: ${config.version}`);

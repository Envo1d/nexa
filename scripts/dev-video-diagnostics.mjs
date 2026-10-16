import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

export const overlayArgument = "--disable_direct_composition_video_overlays=1";

const defaultBrowserArguments =
  "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

export function videoDiagnosticConfig(config, mode = "no-overlays") {
  if (!["baseline", "no-overlays"].includes(mode)) {
    throw new Error("Use baseline or no-overlays diagnostic mode.");
  }

  const windows = config.app?.windows;

  if (
    !Array.isArray(windows) ||
    !windows.some(
      (window, index) =>
        (window.label ?? (index === 0 ? "main" : "")) === "main",
    )
  ) {
    throw new Error("The main window configuration is unavailable.");
  }

  return {
    app: {
      windows: windows.map((window, index) => {
        if ((window.label ?? (index === 0 ? "main" : "")) !== "main")
          return structuredClone(window);

        const args =
          window.additionalBrowserArgs?.trim() || defaultBrowserArguments;

        if (args.includes("disable_direct_composition_video_overlays")) {
          throw new Error(
            "The normal window already overrides video overlays; restore a baseline first.",
          );
        }

        return {
          ...structuredClone(window),
          // WebViews with different arguments need distinct data directories.
          dataDirectory: `NexaNew/webview-video-diagnostics/${mode}`,
          additionalBrowserArgs:
            mode === "no-overlays" ? `${args} ${overlayArgument}` : args,
        };
      }),
    },
  };
}

async function main() {
  if (process.platform !== "win32")
    throw new Error("This WebView2 diagnostic launcher is Windows-only.");

  const mode = process.argv[2] ?? "no-overlays";

  const config = JSON.parse(
    await readFile(
      new URL("../src-tauri/tauri.conf.json", import.meta.url),
      "utf8",
    ),
  );

  const override = videoDiagnosticConfig(config, mode);

  const root = fileURLToPath(new URL("../", import.meta.url));

  const cli = fileURLToPath(
    new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url),
  );

  console.info(
    JSON.stringify({ operation: "video_diagnostics", event: "starting", mode }),
  );
  console.info(
    "Close any existing Nexa instance before starting. Compare the same file, fullscreen and hover actions in both modes.",
  );

  if (process.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) {
    console.warn(
      "Inherited WebView2 arguments may affect this comparison. Their values are not logged.",
    );
  }

  const child = spawn(
    process.execPath,
    [cli, "dev", "--config", JSON.stringify(override)],
    {
      cwd: root,
      stdio: "inherit",
      shell: false,
    },
  );

  child.once("error", (error) => {
    console.error(
      JSON.stringify({
        operation: "video_diagnostics",
        event: "launch_failed",
        reason: error.code ?? "unknown",
      }),
    );
    process.exitCode = 1;
  });
  child.once("exit", (code) => {
    console.info(
      JSON.stringify({
        operation: "video_diagnostics",
        event: "finished",
        mode,
        code,
      }),
    );
    process.exitCode = code ?? 1;
  });
}

if (
  process.argv[1] &&
  pathToFileURL(process.argv[1]).href === import.meta.url
) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}

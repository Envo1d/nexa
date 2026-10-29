import { onMounted, onBeforeUnmount, ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform, version } from "@tauri-apps/plugin-os";

import { usesSystemWindowCorners } from "../utils/windowCorners";

export function useWindowChrome(notify: (message: string) => void) {
  const maximized = ref(false);

  const systemWindowCorners =
    isTauri() && usesSystemWindowCorners(platform(), version());

  let disposed = false;
  let unlistenResize: (() => void) | undefined;

  async function windowAction(action: "minimize" | "maximize" | "close") {
    if (!isTauri()) {
      notify("Window controls are available in the Nexa desktop app.");

      return;
    }

    try {
      const appWindow = getCurrentWindow();

      if (action === "minimize") await appWindow.minimize();
      else if (action === "close") await appWindow.close();
      else {
        await appWindow.toggleMaximize();

        maximized.value = await appWindow.isMaximized();
      }
    } catch {
      notify("The window action could not be completed.");
    }
  }

  async function startDrag(event: PointerEvent) {
    if (event.button !== 0 || !isTauri()) return;

    try {
      await getCurrentWindow().startDragging();
    } catch {
      notify("Window dragging is unavailable.");
    }
  }

  onMounted(async () => {
    if (!isTauri()) return;

    try {
      const appWindow = getCurrentWindow();

      const value = await appWindow.isMaximized();

      if (disposed) return;

      maximized.value = value;

      const stop = await appWindow.onResized(async () => {
        try {
          const value = await appWindow.isMaximized();

          if (!disposed) maximized.value = value;
        } catch {
          if (!disposed) notify("Could not read the window state.");
        }
      });

      if (disposed) stop();
      else unlistenResize = stop;
    } catch {
      if (!disposed) notify("Could not connect window controls.");
    }
  });

  onBeforeUnmount(() => {
    disposed = true;
    unlistenResize?.();
  });

  return {
    maximized,
    systemWindowCorners,
    windowAction,
    startDrag,
  };
}

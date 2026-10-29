import { ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";

import type { Ref } from "vue";

export function usePlayerFullscreen(
  player: Readonly<Ref<HTMLElement | null>>,
  changed: () => void,
  failed: () => void,
) {
  const fullscreen = ref(false);
  const fullscreenBusy = ref(false);

  const nativeFullscreen = isTauri();

  const session = crypto.randomUUID();

  let disposed = false;
  let nativeLease = false;
  let ownedElement: HTMLElement | null = null;

  function publishState() {
    const next =
      nativeLease ||
      (ownedElement !== null && document.fullscreenElement === ownedElement);

    if (fullscreen.value === next) return;

    fullscreen.value = next;

    if (!disposed) changed();
  }

  async function releaseNative() {
    await invoke("set_viewer_fullscreen", { session, enabled: false });

    nativeLease = false;
  }

  async function leaveFullscreen() {
    if (ownedElement && document.fullscreenElement === ownedElement)
      await document.exitFullscreen();

    if (nativeLease) await releaseNative();

    ownedElement = null;
  }

  async function transition(action: () => Promise<void>) {
    if (fullscreenBusy.value) return;

    fullscreenBusy.value = true;

    try {
      await action();
    } catch {
      // Native transition failures are traced at the owning command boundary.
      if (!disposed) failed();
    } finally {
      publishState();
      fullscreenBusy.value = false;
    }
  }

  async function enterFullscreen(element: HTMLElement) {
    ownedElement = element;

    if (nativeFullscreen) {
      await invoke("set_viewer_fullscreen", { session, enabled: true });

      nativeLease = true;
    }

    if (disposed) {
      await leaveFullscreen();

      return;
    }

    try {
      // The native window transition alone does not put the player in the top layer.
      await element.requestFullscreen();
    } catch (cause) {
      await leaveFullscreen();

      throw cause;
    }

    // Escape, unmount or a different fullscreen owner can win during the request.
    if (disposed || document.fullscreenElement !== element)
      await leaveFullscreen();
  }

  async function toggleFullscreen() {
    if (fullscreenBusy.value || disposed) return;

    if (fullscreen.value) {
      await transition(leaveFullscreen);

      return;
    }

    const element = player.value;

    if (element) await transition(() => enterFullscreen(element));
  }

  async function fullscreenChanged() {
    if (disposed || fullscreenBusy.value) return;

    if (ownedElement && document.fullscreenElement !== ownedElement) {
      await transition(leaveFullscreen);

      return;
    }

    publishState();
  }

  async function disposeFullscreen() {
    disposed = true;

    // An in-flight transition owns cleanup, using the captured element after unmount.
    if (!fullscreenBusy.value) await transition(leaveFullscreen);
  }

  return {
    fullscreen,
    fullscreenBusy,
    nativeFullscreen,
    toggleFullscreen,
    fullscreenChanged,
    disposeFullscreen,
  };
}

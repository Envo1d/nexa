import { nextTick, watch, type Ref } from "vue";

import { useFocusTrap } from "./useFocusTrap";

export function useModalFocus(
  modalOpen: Readonly<Ref<boolean>>,
  initialFocus: Readonly<Ref<HTMLElement | null>>,
  modalElement: Readonly<Ref<HTMLElement | null>>,
) {
  let modalTrigger: HTMLElement | null = null;

  watch(modalOpen, async (open, _previous, onCleanup) => {
    let cancelled = false;

    onCleanup(() => {
      cancelled = true;
    });

    if (open) {
      modalTrigger =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;

      await nextTick();

      if (cancelled || !modalOpen.value) return;

      const focusTarget =
        initialFocus.value ??
        modalElement.value?.querySelector<HTMLElement>("button, input");

      focusTarget?.focus();
    } else if (modalTrigger?.isConnected) modalTrigger.focus();
  });

  return useFocusTrap(modalElement);
}

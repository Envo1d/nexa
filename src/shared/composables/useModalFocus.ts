import { nextTick, watch, type Ref } from "vue";

export function useModalFocus(
  modalOpen: Readonly<Ref<boolean>>,
  initialFocus: Readonly<Ref<HTMLElement | null>>,
  modalElement: Readonly<Ref<HTMLElement | null>>,
) {
  let modalTrigger: HTMLElement | null = null;

  watch(modalOpen, async (open) => {
    if (open) {
      modalTrigger =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      await nextTick();

      if (!modalOpen.value) return;

      const focusTarget =
        initialFocus.value ??
        modalElement.value?.querySelector<HTMLElement>("button, input");

      focusTarget?.focus();
    } else modalTrigger?.focus();
  });

  function trapFocus(event: KeyboardEvent) {
    if (event.key !== "Tab" || !modalElement.value) return;

    const focusable = [
      ...modalElement.value.querySelectorAll<HTMLElement>(
        "button:not([disabled]), input, [tabindex='0']",
      ),
    ];

    const first = focusable[0];

    const last = focusable[focusable.length - 1];

    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }

  return { trapFocus };
}

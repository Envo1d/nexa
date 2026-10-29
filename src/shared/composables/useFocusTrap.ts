import type { Ref } from "vue";

const focusableSelector =
  "button, input, select, textarea, a[href], [tabindex], [contenteditable='true']";

export function useFocusTrap(element: Readonly<Ref<HTMLElement | null>>) {
  function trapFocus(event: KeyboardEvent) {
    const root = element.value;

    if (event.key !== "Tab" || event.defaultPrevented || !root) return;

    const controls = [
      ...root.querySelectorAll<HTMLElement>(focusableSelector),
    ].filter(
      (control) =>
        control.tabIndex >= 0 &&
        !control.matches(":disabled") &&
        !control.closest("[inert], [hidden]") &&
        control.getClientRects().length > 0,
    );

    const first = controls[0];
    const last = controls[controls.length - 1];
    const active = document.activeElement;

    if (!first || !last) {
      event.preventDefault();
      root.focus({ preventScroll: true });

      return;
    }

    const outsideControls = !controls.some((control) => control === active);

    if (
      outsideControls ||
      (event.shiftKey ? active === first : active === last)
    ) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    }
  }

  return { trapFocus };
}

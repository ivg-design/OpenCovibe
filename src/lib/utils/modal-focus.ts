/** Keep modal keyboard navigation inside its controls, and restore its opener. */
export function modalFocus(node: HTMLElement) {
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const controls = () =>
    [
      ...node.querySelectorAll<HTMLElement>(
        'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex="0"]',
      ),
    ].filter(
      (element) => element.getClientRects().length > 0 && !element.closest("[hidden], [inert]"),
    );
  const focusFirst = () => (controls()[0] ?? node).focus({ preventScroll: true });
  queueMicrotask(focusFirst);
  function keydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;
    const items = controls();
    const first = items[0],
      last = items.at(-1);
    if (!first || !last) {
      event.preventDefault();
      node.focus();
    } else if (!node.contains(document.activeElement)) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    } else if (
      event.shiftKey &&
      (document.activeElement === first || document.activeElement === node)
    ) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
  document.addEventListener("keydown", keydown, true);
  return {
    destroy() {
      document.removeEventListener("keydown", keydown, true);
      if (opener?.isConnected) opener.focus({ preventScroll: true });
    },
  };
}

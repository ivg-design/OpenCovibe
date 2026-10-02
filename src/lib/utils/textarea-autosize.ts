/** Measure a detached mirror so the live composer never collapses during typing. */
export function textareaAutosize(
  node: HTMLTextAreaElement,
  options: string | { value: string; uncapped: boolean },
) {
  let uncapped = typeof options === "object" && options.uncapped;
  const mirror = document.createElement("textarea");
  mirror.tabIndex = -1;
  mirror.setAttribute("aria-hidden", "true");
  mirror.style.cssText =
    "position:fixed;top:0;left:-10000px;visibility:hidden;pointer-events:none;height:0;min-height:0;max-height:none;overflow:hidden;resize:none;";
  document.body.append(mirror);
  let frame = 0;
  let width = -1;
  const resize = () => {
    frame = 0;
    const style = getComputedStyle(node);
    for (const property of [
      "box-sizing",
      "font",
      "line-height",
      "letter-spacing",
      "padding",
      "border-width",
      "border-style",
      "white-space",
      "word-break",
      "overflow-wrap",
      "tab-size",
    ]) {
      mirror.style.setProperty(property, style.getPropertyValue(property));
    }
    mirror.style.width = `${node.getBoundingClientRect().width}px`;
    mirror.value = node.value || " ";
    const border = parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth);
    const minimum = parseFloat(style.minHeight) || 36;
    const maximum = uncapped ? Infinity : Math.min(window.innerHeight * 0.3, 200);
    const height = Math.max(minimum, Math.min(mirror.scrollHeight + border, maximum));
    const next = `${height}px`;
    if (node.style.height !== next) node.style.height = next;
    node.style.overflowY = mirror.scrollHeight + border > maximum ? "auto" : "hidden";
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(resize);
  };
  const observer = new ResizeObserver((entries) => {
    const next = entries[0]?.contentRect.width ?? 0;
    // Ignore height changes caused by our own resize; these used to trigger a loop.
    if (next !== width) {
      width = next;
      schedule();
    }
  });
  observer.observe(node);
  node.addEventListener("input", schedule);
  window.addEventListener("resize", schedule);
  schedule();
  return {
    update(next?: string | { value: string; uncapped: boolean }) {
      if (next !== undefined) uncapped = typeof next === "object" && next.uncapped;
      schedule();
    },
    destroy() {
      cancelAnimationFrame(frame);
      observer.disconnect();
      mirror.remove();
      node.removeEventListener("input", schedule);
      window.removeEventListener("resize", schedule);
    },
  };
}

import { afterEach, expect, it, vi } from "vitest";
import { textareaAutosize } from "./textarea-autosize";
afterEach(() => vi.unstubAllGlobals());
it("measures outside the live layout, grows and shrinks once, and ignores height-only observer notifications", () => {
  const frames = new Map<number, FrameRequestCallback>();
  let frameId = 0;
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    frames.set(++frameId, cb);
    return frameId;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  const flush = () => {
    for (const [id, cb] of [...frames]) {
      frames.delete(id);
      cb(0);
    }
  };
  let observed: ResizeObserverCallback | undefined;
  const disconnect = vi.fn();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: ResizeObserverCallback) {
        observed = cb;
      }
      observe() {}
      disconnect = disconnect;
    },
  );
  const removed = vi.fn();
  const mirror = {
    value: "",
    style: { setProperty: vi.fn(), cssText: "", width: "" },
    setAttribute: vi.fn(),
    remove: removed,
    get scrollHeight() {
      return this.value.split("\n").length * 20 + 12;
    },
  };
  vi.stubGlobal("document", { createElement: () => mirror, body: { append: vi.fn() } });
  vi.stubGlobal("window", {
    innerHeight: 900,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
  vi.stubGlobal("getComputedStyle", () => ({
    getPropertyValue: () => "",
    borderTopWidth: "1",
    borderBottomWidth: "1",
    minHeight: "36",
  }));
  const writes: string[] = [];
  let height = "";
  const input = {
    value: "one line",
    style: {
      get height() {
        return height;
      },
      set height(value: string) {
        writes.push(value);
        height = value;
      },
      overflowY: "",
    },
    getBoundingClientRect: () => ({ width: 500 }),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };
  const action = textareaAutosize(input as unknown as HTMLTextAreaElement, input.value);
  flush();
  expect(height).toBe("36px");
  input.value = "first\nsecond\nthird";
  action.update();
  action.update();
  expect(frames.size).toBe(1);
  flush();
  expect(height).toBe("74px");
  const notify = () =>
    observed?.([{ contentRect: { width: 500 } }] as ResizeObserverEntry[], {} as ResizeObserver);
  notify();
  flush();
  const count = writes.length;
  notify();
  expect(frames.size).toBe(0);
  expect(writes.length).toBe(count);
  input.value = "short";
  action.update();
  flush();
  expect(height).toBe("36px");
  expect(writes).not.toContain("auto");
  expect(writes).not.toContain("0px");
  input.value = Array(20).fill("long answer").join("\n");
  action.update({ value: input.value, uncapped: true });
  flush();
  expect(height).toBe("414px");
  expect(input.style.overflowY).toBe("hidden");
  action.destroy();
  expect(removed).toHaveBeenCalledOnce();
  expect(disconnect).toHaveBeenCalledOnce();
});

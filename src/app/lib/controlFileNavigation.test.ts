import { describe, expect, it, vi } from "vitest";
import { activateControlFileNavigation } from "./controlFileNavigation";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("activateControlFileNavigation", () => {
  it("waits for activation before navigating an already-mounted editor", async () => {
    const activation = deferred<boolean>();
    const gotoLine = vi.fn();
    const run = activateControlFileNavigation({
      activate: () => activation.promise,
      getEditor: () => ({ focus: vi.fn(), gotoLine }),
      setPending: vi.fn(),
      tabId: 4,
      line: 12,
    });

    expect(gotoLine).not.toHaveBeenCalled();
    activation.resolve(true);
    await run;
    expect(gotoLine).toHaveBeenCalledWith(12, { focus: true });
  });

  it("does not focus, navigate, or queue when activation fails", async () => {
    const focus = vi.fn();
    const gotoLine = vi.fn();
    const setPending = vi.fn();

    await activateControlFileNavigation({
      activate: async () => false,
      getEditor: () => ({ focus, gotoLine }),
      setPending,
      tabId: 4,
      line: 12,
    });

    expect(focus).not.toHaveBeenCalled();
    expect(gotoLine).not.toHaveBeenCalled();
    expect(setPending).not.toHaveBeenCalled();
  });

  it("navigates a mounted editor without activating or focusing for focus:false", async () => {
    const activate = vi.fn(async () => true);
    const focus = vi.fn();
    const gotoLine = vi.fn();

    await activateControlFileNavigation({
      activate,
      getEditor: () => ({ focus, gotoLine }),
      setPending: vi.fn(),
      tabId: 4,
      line: 12,
      focus: false,
    });

    expect(activate).not.toHaveBeenCalled();
    expect(focus).not.toHaveBeenCalled();
    expect(gotoLine).toHaveBeenCalledWith(12, { focus: false });
  });

  it("queues unfocused line navigation without activating for a late mount", async () => {
    const activate = vi.fn(async () => true);
    const setPending = vi.fn();

    await activateControlFileNavigation({
      activate,
      getEditor: () => null,
      setPending,
      tabId: 4,
      line: 12,
      focus: false,
    });

    expect(activate).not.toHaveBeenCalled();
    expect(setPending).toHaveBeenCalledWith(4, { line: 12, focus: false });
  });

  it("queues navigation after successful activation when the editor mounts later", async () => {
    const setPending = vi.fn();

    await activateControlFileNavigation({
      activate: async () => true,
      getEditor: () => null,
      setPending,
      tabId: 4,
    });

    expect(setPending).toHaveBeenCalledWith(4, { focus: true });
  });
});

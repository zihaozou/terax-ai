import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const imageResource = vi.hoisted(() => ({
  close: vi.fn<() => Promise<void>>(),
}));
const native = vi.hoisted(() => ({
  readImage: vi.fn<() => Promise<typeof imageResource>>(),
  readText: vi.fn<() => Promise<string>>(),
  writeText: vi.fn<(t: string) => Promise<void>>(),
}));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => native);

const web = {
  readText: vi.fn<() => Promise<string>>(),
  writeText: vi.fn<(t: string) => Promise<void>>(),
};

const original = globalThis.navigator;
const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15";
const MAC =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";

function platform(userAgent: string) {
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { userAgent, clipboard: web },
  });
}

async function load() {
  vi.resetModules();
  return import("./terminalClipboard");
}

describe("terminalClipboard", () => {
  beforeEach(() => {
    imageResource.close.mockReset();
    imageResource.close.mockResolvedValue();
    native.readImage.mockReset();
    native.readText.mockReset();
    native.writeText.mockReset();
    web.readText.mockReset();
    web.writeText.mockReset();
  });

  afterEach(() => {
    Object.defineProperty(globalThis, "navigator", {
      configurable: true,
      value: original,
    });
  });

  it("reads the native clipboard first on Linux", async () => {
    platform(LINUX);
    native.readText.mockResolvedValue("native");
    web.readText.mockResolvedValue("web");
    const { readTerminalClipboard } = await load();
    await expect(readTerminalClipboard()).resolves.toBe("native");
    expect(web.readText).not.toHaveBeenCalled();
  });

  it("falls back to the web clipboard when the native read fails", async () => {
    platform(LINUX);
    native.readText.mockRejectedValue(new Error("no ipc"));
    web.readText.mockResolvedValue("web");
    const { readTerminalClipboard } = await load();
    await expect(readTerminalClipboard()).resolves.toBe("web");
  });

  it("never touches the native clipboard off Linux", async () => {
    platform(MAC);
    web.readText.mockResolvedValue("web");
    const { readTerminalClipboard, writeTerminalClipboard } = await load();
    await expect(readTerminalClipboard()).resolves.toBe("web");
    await writeTerminalClipboard("x");
    expect(native.readText).not.toHaveBeenCalled();
    expect(native.writeText).not.toHaveBeenCalled();
    expect(web.writeText).toHaveBeenCalledWith("x");
  });

  it("writes the native clipboard first on Linux", async () => {
    platform(LINUX);
    native.writeText.mockResolvedValue();
    const { writeTerminalClipboard } = await load();
    await writeTerminalClipboard("copied");
    expect(native.writeText).toHaveBeenCalledWith("copied");
    expect(web.writeText).not.toHaveBeenCalled();
  });

  it("detects native clipboard images and releases the resource", async () => {
    platform(MAC);
    native.readImage.mockResolvedValue(imageResource);
    const { readNativeTerminalClipboardPayload } = await load();

    await expect(readNativeTerminalClipboardPayload()).resolves.toEqual({
      kind: "image",
    });
    expect(imageResource.close).toHaveBeenCalledOnce();
  });

  it("returns native clipboard text when no image is available", async () => {
    platform(MAC);
    native.readImage.mockRejectedValue(new Error("no image"));
    native.readText.mockResolvedValue("hello");
    const { readNativeTerminalClipboardPayload } = await load();

    await expect(readNativeTerminalClipboardPayload()).resolves.toEqual({
      kind: "text",
      text: "hello",
    });
  });

  it("returns empty when the native clipboard has no supported payload", async () => {
    platform(MAC);
    native.readImage.mockRejectedValue(new Error("no image"));
    native.readText.mockRejectedValue(new Error("no text"));
    const { readNativeTerminalClipboardPayload } = await load();

    await expect(readNativeTerminalClipboardPayload()).resolves.toEqual({
      kind: "empty",
    });
  });
});

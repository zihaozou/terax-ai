import { describe, expect, it, vi } from "vitest";
import {
  createTerminalPasteQueue,
  pasteIntoTerminal,
  pasteTerminalClipboardPayload,
  shouldHandlePiCmdPaste,
  shouldHandlePiRightClick,
  terminalPasteRoute,
} from "./terminalPaste";

describe("pasteIntoTerminal", () => {
  it("pastes and focuses the resolved terminal", () => {
    const terminal = { paste: vi.fn(), focus: vi.fn() };

    expect(pasteIntoTerminal(terminal, "/repo/file.ts ")).toBe(true);
    expect(terminal.paste).toHaveBeenCalledWith("/repo/file.ts ");
    expect(terminal.focus).toHaveBeenCalledOnce();
  });

  it("returns false when no terminal is resolved", () => {
    expect(pasteIntoTerminal(null, "/repo/file.ts ")).toBe(false);
  });
});

describe("terminalPasteRoute", () => {
  it("inspects the clipboard before routing Pi paste gestures", () => {
    expect(terminalPasteRoute("pi", null)).toEqual({ kind: "inspect" });
  });

  it("keeps native paste for non-Pi programs", () => {
    expect(terminalPasteRoute("claude", null)).toEqual({ kind: "native" });
    expect(terminalPasteRoute(undefined, null)).toEqual({ kind: "native" });
  });

  it("forwards image payloads through Pi's default Ctrl+V binding", () => {
    expect(terminalPasteRoute("pi", { kind: "image" })).toEqual({
      kind: "pty",
      data: "\x16",
    });
  });

  it("pastes text payloads through xterm", () => {
    expect(
      terminalPasteRoute("pi", { kind: "text", text: "clipboard text" }),
    ).toEqual({ kind: "terminal", text: "clipboard text" });
  });

  it("does nothing for an empty Pi clipboard", () => {
    expect(terminalPasteRoute("pi", { kind: "empty" })).toEqual({
      kind: "none",
    });
  });
});

describe("shouldHandlePiCmdPaste", () => {
  it("handles Pi Cmd+V even when Kitty keyboard mode is inactive", () => {
    expect(
      shouldHandlePiCmdPaste("pi", {
        isMac: true,
        isCmdV: true,
        kittyKeyboardActive: false,
      }),
    ).toBe(true);
    expect(
      shouldHandlePiCmdPaste("pi", {
        isMac: true,
        isCmdV: true,
        kittyKeyboardActive: true,
      }),
    ).toBe(true);
  });

  it("does not intercept non-Pi or non-macOS paste shortcuts", () => {
    expect(
      shouldHandlePiCmdPaste("claude", {
        isMac: true,
        isCmdV: true,
        kittyKeyboardActive: true,
      }),
    ).toBe(false);
    expect(
      shouldHandlePiCmdPaste("pi", {
        isMac: false,
        isCmdV: true,
        kittyKeyboardActive: true,
      }),
    ).toBe(false);
    expect(
      shouldHandlePiCmdPaste("pi", {
        isMac: true,
        isCmdV: false,
        kittyKeyboardActive: true,
      }),
    ).toBe(false);
  });
});

describe("createTerminalPasteQueue", () => {
  it("serializes clipboard deliveries in gesture order", async () => {
    let releaseFirst: (() => void) | undefined;
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const seen: string[] = [];
    const enqueue = createTerminalPasteQueue();

    const first = enqueue(async () => {
      seen.push("first-start");
      await firstGate;
      seen.push("first-end");
    });
    const second = enqueue(async () => {
      seen.push("second");
    });

    await Promise.resolve();
    expect(seen).toEqual(["first-start"]);
    releaseFirst?.();
    await Promise.all([first, second]);
    expect(seen).toEqual(["first-start", "first-end", "second"]);
  });
});

describe("pasteTerminalClipboardPayload", () => {
  it("delivers text through xterm paste", async () => {
    const terminal = { paste: vi.fn(), focus: vi.fn() };

    await expect(
      pasteTerminalClipboardPayload({
        agent: "pi",
        targetLeafId: 4,
        currentLeafId: () => 4,
        readClipboard: async () => ({ kind: "text", text: "hello" }),
        terminal,
        writeToPty: vi.fn(),
      }),
    ).resolves.toBe(true);
    expect(terminal.paste).toHaveBeenCalledWith("hello");
    expect(terminal.focus).toHaveBeenCalledOnce();
  });

  it("delivers images through Pi's default Ctrl+V binding", async () => {
    const writeToPty = vi.fn();

    await expect(
      pasteTerminalClipboardPayload({
        agent: "pi",
        targetLeafId: 4,
        currentLeafId: () => 4,
        readClipboard: async () => ({ kind: "image" }),
        terminal: { paste: vi.fn(), focus: vi.fn() },
        writeToPty,
      }),
    ).resolves.toBe(true);
    expect(writeToPty).toHaveBeenCalledWith("\x16");
  });

  it("drops an async clipboard result after the active leaf changes", async () => {
    const terminal = { paste: vi.fn(), focus: vi.fn() };
    const writeToPty = vi.fn();

    await expect(
      pasteTerminalClipboardPayload({
        agent: "pi",
        targetLeafId: 4,
        currentLeafId: () => 5,
        readClipboard: async () => ({ kind: "text", text: "stale" }),
        terminal,
        writeToPty,
      }),
    ).resolves.toBe(false);
    expect(terminal.paste).not.toHaveBeenCalled();
    expect(writeToPty).not.toHaveBeenCalled();
  });
});

describe("shouldHandlePiRightClick", () => {
  it("handles the macOS secondary button only when Pi owns the terminal", () => {
    expect(shouldHandlePiRightClick("pi", { isMac: true, button: 2 })).toBe(
      true,
    );
    expect(shouldHandlePiRightClick("claude", { isMac: true, button: 2 })).toBe(
      false,
    );
    expect(shouldHandlePiRightClick("pi", { isMac: true, button: 0 })).toBe(
      false,
    );
    expect(shouldHandlePiRightClick("pi", { isMac: false, button: 2 })).toBe(
      false,
    );
  });
});

import type { TerminalClipboardPayload } from "./terminalClipboard";

export type TerminalPasteTarget = {
  paste: (text: string) => void;
  focus: () => void;
};

export type TerminalPasteRoute =
  | { kind: "native" }
  | { kind: "inspect" }
  | { kind: "pty"; data: string }
  | { kind: "terminal"; text: string }
  | { kind: "none" };

export function terminalPasteRoute(
  agent: string | undefined,
  payload: TerminalClipboardPayload | null,
): TerminalPasteRoute {
  if (agent !== "pi") return { kind: "native" };
  if (payload === null) return { kind: "inspect" };
  if (payload.kind === "image") return { kind: "pty", data: "\x16" };
  if (payload.kind === "text") return { kind: "terminal", text: payload.text };
  return { kind: "none" };
}

export function pasteIntoTerminal(
  terminal: TerminalPasteTarget | null,
  text: string,
): boolean {
  if (!terminal) return false;
  terminal.paste(text);
  terminal.focus();
  return true;
}

export function shouldHandlePiCmdPaste(
  agent: string | undefined,
  options: {
    isMac: boolean;
    isCmdV: boolean;
    kittyKeyboardActive: boolean;
  },
): boolean {
  return options.isMac && options.isCmdV && agent === "pi";
}

export function createTerminalPasteQueue() {
  let tail: Promise<void> = Promise.resolve();
  return function enqueue<T>(task: () => Promise<T>): Promise<T> {
    const result = tail.then(task);
    tail = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  };
}

export async function pasteTerminalClipboardPayload(options: {
  agent: string | undefined;
  targetLeafId: number;
  currentLeafId: () => number | null;
  readClipboard: () => Promise<TerminalClipboardPayload>;
  terminal: TerminalPasteTarget;
  writeToPty: (data: string) => void;
}): Promise<boolean> {
  if (terminalPasteRoute(options.agent, null).kind !== "inspect") return false;
  const payload = await options.readClipboard();
  if (options.currentLeafId() !== options.targetLeafId) return false;
  const route = terminalPasteRoute(options.agent, payload);
  if (route.kind === "terminal")
    return pasteIntoTerminal(options.terminal, route.text);
  if (route.kind === "pty") {
    options.writeToPty(route.data);
    return true;
  }
  return route.kind === "none";
}

export function shouldHandlePiRightClick(
  agent: string | undefined,
  options: { isMac: boolean; button: number },
): boolean {
  return options.isMac && options.button === 2 && agent === "pi";
}

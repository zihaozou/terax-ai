export type TerminalPasteTarget = {
  paste: (text: string) => void;
  focus: () => void;
};

export function macKittyPasteRoute(
  agent: string | undefined,
): "pty" | "native" {
  return agent === "pi" ? "pty" : "native";
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

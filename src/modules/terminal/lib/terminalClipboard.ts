// WebKitGTK can't read external text copies, so the ordinary text path uses
// the native plugin on Linux. Pi paste gestures use the native plugin on macOS
// as well so image-vs-text routing does not depend on WKWebView paste events.
const IS_LINUX =
  typeof navigator !== "undefined" &&
  /Linux/.test(navigator.userAgent) &&
  !/Android/.test(navigator.userAgent);

function webClipboard(): Clipboard | null {
  if (typeof navigator === "undefined") return null;
  return navigator.clipboard ?? null;
}

export type TerminalClipboardPayload =
  | { kind: "image" }
  | { kind: "text"; text: string }
  | { kind: "empty" };

export async function readNativeTerminalClipboardPayload(): Promise<TerminalClipboardPayload> {
  let clipboard: typeof import("@tauri-apps/plugin-clipboard-manager");
  try {
    clipboard = await import("@tauri-apps/plugin-clipboard-manager");
  } catch {
    return { kind: "empty" };
  }

  try {
    const image = await clipboard.readImage();
    try {
      await image.close();
    } catch {}
    return { kind: "image" };
  } catch {}

  try {
    const text = await clipboard.readText();
    return text ? { kind: "text", text } : { kind: "empty" };
  } catch {
    return { kind: "empty" };
  }
}

export async function readTerminalClipboard(): Promise<string> {
  if (IS_LINUX) {
    try {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      return await readText();
    } catch {}
  }
  try {
    return (await webClipboard()?.readText()) ?? "";
  } catch {
    return "";
  }
}

export async function writeTerminalClipboard(text: string): Promise<void> {
  if (IS_LINUX) {
    try {
      const { writeText } = await import(
        "@tauri-apps/plugin-clipboard-manager"
      );
      await writeText(text);
      return;
    } catch {}
  }
  try {
    await webClipboard()?.writeText(text);
  } catch {}
}

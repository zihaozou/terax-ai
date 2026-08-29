import { describe, expect, it, vi } from "vitest";
import {
  createPickerRequestGate,
  joinDirectory,
  parentDirectory,
} from "./directoryPicker";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe("directory picker paths", () => {
  it("joins child directories using canonical forward slashes", () => {
    expect(joinDirectory("C:/Users/me", "repo")).toBe("C:/Users/me/repo");
    expect(joinDirectory("/home/me/", "repo")).toBe("/home/me/repo");
  });

  it("preserves Unix and Windows filesystem roots when navigating up", () => {
    expect(parentDirectory("/home/me")).toBe("/home");
    expect(parentDirectory("C:/Users/me")).toBe("C:/Users");
    expect(parentDirectory("C:/")).toBe("C:/");
  });

  it("keeps the newer picker request when an earlier home lookup resolves last", async () => {
    const gate = createPickerRequestGate();
    const first = deferred<string>();
    const second = deferred<string>();
    const selected: string[] = [];
    const open = async (home: Promise<string>) => {
      const id = gate.begin();
      const path = await home;
      if (gate.isCurrent(id)) selected.push(path);
    };

    const firstOpen = open(first.promise);
    const secondOpen = open(second.promise);
    second.resolve("/newer");
    await secondOpen;
    first.resolve("/older");
    await firstOpen;

    expect(selected).toEqual(["/newer"]);
  });

  it("suppresses an error from a superseded home lookup", async () => {
    const gate = createPickerRequestGate();
    const first = deferred<string>();
    const second = deferred<string>();
    const reportError = vi.fn();
    const open = async (home: Promise<string>) => {
      const id = gate.begin();
      try {
        await home;
      } catch (error) {
        if (gate.isCurrent(id)) reportError(error);
      }
    };

    const firstOpen = open(first.promise);
    const secondOpen = open(second.promise);
    second.resolve("/newer");
    await secondOpen;
    first.reject(new Error("stale failure"));
    await firstOpen;

    expect(reportError).not.toHaveBeenCalled();
  });

  it("invalidates an in-flight home lookup when the picker closes", async () => {
    const gate = createPickerRequestGate();
    const home = deferred<string>();
    const selected: string[] = [];
    const id = gate.begin();
    const open = home.promise.then((path) => {
      if (gate.isCurrent(id)) selected.push(path);
    });

    gate.invalidate();
    home.resolve("/cancelled");
    await open;

    expect(selected).toEqual([]);
  });
});

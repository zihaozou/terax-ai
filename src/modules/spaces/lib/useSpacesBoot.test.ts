import { describe, expect, it, vi } from "vitest";
import type { LoadedSpaces, SpaceMeta } from "@/modules/spaces/lib/store";

const mocks = vi.hoisted(() => ({
  canonicalize: vi.fn(),
  workspaceAuthorize: vi.fn(),
  loadAll: vi.fn(),
  hydrate: vi.fn(),
}));

vi.mock("react", () => ({
  useEffect: (effect: () => void) => effect(),
  useRef: <T>(value: T) => ({ current: value }),
}));

vi.mock("@/lib/native", () => ({
  native: {
    canonicalize: mocks.canonicalize,
    workspaceAuthorize: mocks.workspaceAuthorize,
  },
}));

vi.mock("@/modules/settings/preferences", () => ({
  usePreferencesStore: { getState: () => ({ init: vi.fn() }) },
}));

vi.mock("@/modules/spaces/lib/store", () => ({
  loadAll: mocks.loadAll,
  saveActiveId: vi.fn(),
  saveSchemaVersion: vi.fn(),
  saveSpacesList: vi.fn(),
}));

vi.mock("@/modules/spaces/lib/useSpaces", () => ({
  useSpaces: {
    getState: () => ({ hydrate: mocks.hydrate }),
  },
}));

vi.mock("@/modules/tabs/lib/useTabs", () => ({
  DEFAULT_SPACE_ID: "default",
}));

import { useSpacesBoot } from "./useSpacesBoot";

function space(overrides: Partial<SpaceMeta>): SpaceMeta {
  return {
    id: "broken",
    name: "Broken",
    root: "/rejected",
    env: { kind: "local" },
    createdAt: 0,
    updatedAt: 0,
    ...overrides,
  };
}

function loadedWithNoState(): LoadedSpaces {
  return {
    schemaVersion: 2,
    activeId: "broken",
    spaces: [space({})],
    states: new Map(),
  };
}

describe("useSpacesBoot", () => {
  it("installs an actual-Space cold fallback without authorizing an unavailable root", async () => {
    mocks.loadAll.mockResolvedValue(loadedWithNoState());
    mocks.canonicalize.mockRejectedValue(new Error("not found"));
    const replaceTabs = vi.fn();
    const markBooted = vi.fn();
    const setActiveSpaceForNewTabs = vi.fn();
    const adoptWorkspaceEnv = vi.fn().mockResolvedValue("/home");
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});

    useSpacesBoot({
      ready: true,
      launchCwd: "/launch",
      home: "/home",
      allocId: (() => {
        let id = 1;
        return () => id++;
      })(),
      replaceTabs,
      markBooted,
      setActiveSpaceForNewTabs,
      adoptWorkspaceEnv,
    });

    await vi.waitFor(() => expect(markBooted).toHaveBeenCalledOnce());

    expect(consoleError).not.toHaveBeenCalled();
    expect(setActiveSpaceForNewTabs).toHaveBeenCalledWith("broken");
    expect(replaceTabs).toHaveBeenCalledOnce();
    const [tabs] = replaceTabs.mock.calls[0] as [
      Array<{
        spaceId: string;
        cold?: boolean;
        cwd?: string;
        paneTree: { kind: "leaf"; cwd?: string };
      }>,
    ];
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({
      spaceId: "broken",
      cold: true,
      cwd: undefined,
      paneTree: { kind: "leaf" },
    });
    expect(tabs[0].paneTree).not.toHaveProperty("cwd");
    expect(mocks.workspaceAuthorize).not.toHaveBeenCalled();

    consoleError.mockRestore();
  });
});

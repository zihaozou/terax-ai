import { describe, expect, it } from "vitest";
import {
  legacyRootCandidate,
  migrateSpaceRoots,
} from "@/modules/spaces/lib/spaceRoot";
import type {
  LoadedSpaces,
  SpaceMeta,
  SpaceState,
} from "@/modules/spaces/lib/store";

function space(overrides: Partial<SpaceMeta>): SpaceMeta {
  return {
    id: "space",
    name: "Space",
    root: null,
    env: { kind: "local" },
    createdAt: 0,
    updatedAt: 0,
    ...overrides,
  };
}

function terminalState(cwd: string): SpaceState {
  return {
    activeTabIndex: 0,
    tabs: [{ kind: "terminal", tree: { kind: "leaf", cwd, active: true } }],
  };
}

function loadedSpaces(): LoadedSpaces {
  return {
    schemaVersion: 1,
    activeId: "ok",
    spaces: [
      space({ id: "ok", root: "/project" }),
      space({ id: "missing", root: "/missing" }),
    ],
    states: new Map(),
  };
}

describe("legacyRootCandidate", () => {
  it("uses the active terminal leaf before another terminal leaf", () => {
    const state: SpaceState = {
      activeTabIndex: 1,
      tabs: [
        { kind: "terminal", tree: { kind: "leaf", cwd: "/first" } },
        {
          kind: "terminal",
          tree: { kind: "leaf", cwd: "/active", active: true },
        },
      ],
    };
    expect(legacyRootCandidate(space({ root: null }), state, "/home/me")).toBe(
      "/active",
    );
  });

  it("keeps an existing Space root ahead of terminal cwd", () => {
    expect(
      legacyRootCandidate(
        space({ root: "/project" }),
        terminalState("/shell"),
        "/home/me",
      ),
    ).toBe("/project");
  });

  it("uses the first terminal leaf when no active terminal leaf exists", () => {
    const state: SpaceState = {
      activeTabIndex: 0,
      tabs: [
        {
          kind: "terminal",
          tree: {
            kind: "split",
            dir: "row",
            children: [
              { kind: "leaf", cwd: "/first" },
              { kind: "leaf", cwd: "/second" },
            ],
          },
        },
      ],
    };
    expect(legacyRootCandidate(space({}), state, "/home/me")).toBe("/first");
  });

  it("uses the environment home when no terminal cwd exists", () => {
    expect(legacyRootCandidate(space({}), undefined, "/home/me")).toBe(
      "/home/me",
    );
  });
});

describe("migrateSpaceRoots", () => {
  it("writes canonical roots and records unavailable candidates", async () => {
    const result = await migrateSpaceRoots(
      loadedSpaces(),
      async () => "/home/me",
      async (path) => {
        if (path === "/missing") throw new Error("not found");
        return `/canon${path}`;
      },
    );

    expect(result.spaces.find((s) => s.id === "ok")?.root).toBe("/canon/project");
    expect(result.issues.missing).toEqual({
      candidate: "/missing",
      message: "not found",
    });
  });

  it("preserves a terminal candidate when validation fails", async () => {
    const loaded: LoadedSpaces = {
      schemaVersion: 1,
      activeId: "space",
      spaces: [space({ root: null })],
      states: new Map([["space", terminalState("/unavailable")]]),
    };
    const result = await migrateSpaceRoots(
      loaded,
      async () => "/home/me",
      async () => {
        throw new Error("not found");
      },
    );

    expect(result.spaces[0].root).toBe("/unavailable");
    expect(result.issues.space).toEqual({
      candidate: "/unavailable",
      message: "not found",
    });
  });
});

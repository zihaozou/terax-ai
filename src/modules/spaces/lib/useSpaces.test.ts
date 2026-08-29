import { afterEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import type { SpaceMeta } from "@/modules/spaces/lib/store";

vi.mock("@/modules/spaces/lib/store", () => ({
  deleteSpaceData: vi.fn(),
  newSpaceId: () => "generated",
  saveActiveId: vi.fn(),
  saveSpacesList: vi.fn(),
}));

import { useSpaces } from "@/modules/spaces/lib/useSpaces";
import type { SpaceRootIssues } from "@/modules/spaces/lib/spaceRoot";

function makeSpace(id: string, root: string | null): SpaceMeta {
  return {
    id,
    name: id,
    root,
    env: { kind: "local" },
    createdAt: 0,
    updatedAt: 0,
  };
}

function seedSpaces(
  spaces: SpaceMeta[],
  rootIssues: SpaceRootIssues = {},
): void {
  useSpaces.setState({
    spaces,
    activeId: spaces[0]?.id ?? null,
    hydrated: true,
    initialActiveIndex: {},
    rootIssues,
  });
}

afterEach(() => {
  seedSpaces([]);
});

describe("useSpaces root mutations", () => {
  it("changes only the selected Space root and clears its issue", () => {
    seedSpaces([makeSpace("a", "/a"), makeSpace("b", "/b")], {
      a: { candidate: "/missing", message: "not found" },
    });

    useSpaces.getState().setRoot("a", "/next");

    expect(useSpaces.getState().spaces.map((space) => space.root)).toEqual([
      "/next",
      "/b",
    ]);
    expect(useSpaces.getState().rootIssues.a).toBeUndefined();
  });

  it("sets and clears individual root issues", () => {
    seedSpaces([makeSpace("a", "/a"), makeSpace("b", "/b")]);

    useSpaces
      .getState()
      .setRootIssue("a", { candidate: "/missing", message: "not found" });
    useSpaces.getState().clearRootIssue("a");

    expect(useSpaces.getState().rootIssues).toEqual({});
  });

  it("requires a non-empty root when creating a Space", () => {
    type CreateInput = Parameters<
      ReturnType<typeof useSpaces.getState>["create"]
    >[0];
    expectTypeOf<CreateInput>().toEqualTypeOf<{
      id?: string;
      name: string;
      root: string;
      env?: { kind: "local" } | { kind: "wsl"; distro: string };
    }>();
    expectTypeOf<{
      name: string;
      root: null;
    }>().not.toMatchTypeOf<CreateInput>();
  });

  it("does not expose environment mutation", () => {
    expect("setEnv" in useSpaces.getState()).toBe(false);
  });
});

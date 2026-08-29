import { describe, expect, it } from "vitest";
import type { SpaceRootIssues } from "@/modules/spaces/lib/spaceRoot";
import type { Tab } from "@/modules/tabs";
import { canWarmTab } from "./tabWarmPolicy";

function terminalTab(input: { spaceId: string; cold?: boolean }): Tab {
  return {
    id: 1,
    kind: "terminal",
    spaceId: input.spaceId,
    cold: input.cold,
    title: "shell",
    paneTree: { kind: "leaf", id: 10 },
    activeLeafId: 10,
  };
}

function editorTab(input: { spaceId: string }): Tab {
  return {
    id: 2,
    kind: "editor",
    spaceId: input.spaceId,
    title: "outside.ts",
    path: "/outside.ts",
    dirty: false,
    preview: false,
  };
}

function brokenIssues(): SpaceRootIssues {
  return { broken: { candidate: "/missing", message: "not found" } };
}

describe("canWarmTab", () => {
  it("keeps a terminal cold while its Space root is unavailable", () => {
    const tab = terminalTab({ spaceId: "broken", cold: true });

    expect(canWarmTab(tab, brokenIssues())).toBe(false);
  });

  it("does not block editor tabs or valid terminal Spaces", () => {
    expect(canWarmTab(editorTab({ spaceId: "broken" }), brokenIssues())).toBe(
      true,
    );
    expect(canWarmTab(terminalTab({ spaceId: "ok" }), brokenIssues())).toBe(
      true,
    );
  });
});

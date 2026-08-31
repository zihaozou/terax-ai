import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@/modules/workspace", () => ({
  currentWorkspaceEnv: () => ({ kind: "local" }),
}));

import { native } from "@/lib/native";

describe("native workspace validation", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the explicit environment to canonicalization and authorization", () => {
    const workspace = { kind: "wsl", distro: "Ubuntu" } as const;

    native.canonicalize("/home/me/project", workspace);
    native.workspaceAuthorize("/home/me/project", workspace);

    expect(invoke).toHaveBeenNthCalledWith(1, "fs_canonicalize", {
      path: "/home/me/project",
      workspace,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "workspace_authorize", {
      path: "/home/me/project",
      workspace,
    });
  });
});

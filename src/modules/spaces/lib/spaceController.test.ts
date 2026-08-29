import { describe, expect, it, vi } from "vitest";
import { workspaceScopeKey, type WorkspaceEnv } from "@/modules/workspace";
import {
  createSpaceController,
  type PreparedWorkspace,
  type SpaceControllerDeps,
} from "@/modules/spaces/lib/spaceController";
import type { SpaceMeta } from "@/modules/spaces/lib/store";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function makeSpace(
  id: string,
  root: string,
  env: WorkspaceEnv = { kind: "local" },
): SpaceMeta {
  return { id, name: id, root, env, createdAt: 0, updatedAt: 0 };
}

function deps(
  overrides: Partial<SpaceControllerDeps> = {},
): SpaceControllerDeps {
  const spaces = new Map([
    ["a", makeSpace("a", "/a")],
    ["b", makeSpace("b", "/b")],
    ["wsl", makeSpace("wsl", "/work", { kind: "wsl", distro: "Ubuntu" })],
  ]);
  return {
    getSpace: (id) => spaces.get(id) ?? null,
    currentEnv: () => ({ kind: "local" }),
    validateRoot: async (path) => path,
    prepareEnv: async (env) => ({
      env,
      home: env.kind === "local" ? "/Users/me" : "/home/me",
    }),
    applyEnv: () => {},
    commitActive: () => {},
    createMeta: (input) => makeSpace(input.name, input.root, input.env),
    createTerminal: () => 1,
    setRoot: () => {},
    reportError: () => {},
    ...overrides,
  };
}

describe("createSpaceController", () => {
  it("does not expose a Space until its environment is prepared", async () => {
    const events: string[] = [];
    const gate = deferred<PreparedWorkspace>();
    const controller = createSpaceController(
      deps({
        prepareEnv: async () => gate.promise,
        applyEnv: () => events.push("apply-env"),
        commitActive: () => events.push("commit-space"),
      }),
    );

    const pending = controller.activate({ spaceId: "wsl" });
    expect(events).toEqual([]);
    gate.resolve({ env: { kind: "wsl", distro: "Ubuntu" }, home: "/home/me" });
    await pending;
    expect(events).toEqual(["apply-env", "commit-space"]);
  });

  it("commits only the latest activation request", async () => {
    const committed: string[] = [];
    const gates = new Map([
      ["local", deferred<PreparedWorkspace>()],
      ["wsl:Ubuntu", deferred<PreparedWorkspace>()],
    ]);
    const controller = createSpaceController(
      deps({
        prepareEnv: async (env) => gates.get(workspaceScopeKey(env))!.promise,
        commitActive: ({ spaceId }) => committed.push(spaceId),
      }),
    );
    const first = controller.activate({ spaceId: "a" });
    const second = controller.activate({ spaceId: "wsl" });
    gates.get("local")!.resolve({ env: { kind: "local" }, home: "/Users/me" });
    gates.get("wsl:Ubuntu")!.resolve({
      env: { kind: "wsl", distro: "Ubuntu" },
      home: "/home/me",
    });
    await Promise.all([first, second]);
    expect(committed).toEqual(["wsl"]);
  });

  it("skips environment preparation for a same-environment activation", async () => {
    const prepareEnv = vi.fn(async (env: WorkspaceEnv) => ({
      env,
      home: "/Users/me",
    }));
    const commitActive = vi.fn();
    const controller = createSpaceController(
      deps({ prepareEnv, commitActive }),
    );

    await controller.activate({ spaceId: "a" });

    expect(prepareEnv).not.toHaveBeenCalled();
    expect(commitActive).toHaveBeenCalledWith({ spaceId: "a" });
  });

  it("does not apply an obsolete root change", async () => {
    const first = deferred<string>();
    const roots: string[] = [];
    const controller = createSpaceController(
      deps({
        validateRoot: async (path) =>
          path === "/first" ? first.promise : path,
        setRoot: (_, root) => roots.push(root),
      }),
    );

    const changingFirst = controller.changeRoot("a", "/first");
    await Promise.resolve();
    const changingSecond = controller.changeRoot("a", "/second");
    first.resolve("/first");

    await expect(changingFirst).resolves.toBe(false);
    await expect(changingSecond).resolves.toBe(true);
    expect(roots).toEqual(["/second"]);
  });

  it("creates one terminal at the validated root before activating the Space", async () => {
    const events: string[] = [];
    const controller = createSpaceController(
      deps({
        validateRoot: async (path) => {
          events.push(`validate:${path}`);
          return "/canonical";
        },
        prepareEnv: async (env) => {
          events.push("prepare-env");
          return { env, home: "/Users/me" };
        },
        createMeta: (input) => {
          events.push(`create-meta:${input.root}`);
          return makeSpace("created", input.root, input.env);
        },
        createTerminal: (spaceId, root) => {
          events.push(`create-terminal:${spaceId}:${root}`);
          return 1;
        },
        applyEnv: () => events.push("apply-env"),
        commitActive: ({ spaceId }) => events.push(`commit:${spaceId}`),
      }),
    );

    await expect(
      controller.create({
        name: "created",
        root: "/candidate",
        env: { kind: "local" },
      }),
    ).resolves.toMatchObject({ id: "created", root: "/canonical" });
    expect(events).toEqual([
      "validate:/candidate",
      "prepare-env",
      "create-meta:/canonical",
      "create-terminal:created:/canonical",
      "apply-env",
      "commit:created",
    ]);
  });

  it("does not create a partial Space when preparation fails", async () => {
    const createMeta = vi.fn();
    const createTerminal = vi.fn();
    const controller = createSpaceController(
      deps({
        prepareEnv: async () => {
          throw new Error("WSL unavailable");
        },
        createMeta,
        createTerminal,
      }),
    );

    await expect(
      controller.create({
        name: "created",
        root: "/repo",
        env: { kind: "wsl", distro: "Ubuntu" },
      }),
    ).resolves.toBeNull();
    expect(createMeta).not.toHaveBeenCalled();
    expect(createTerminal).not.toHaveBeenCalled();
  });

  it("preserves the previous root and reports a failed root change", async () => {
    const setRoot = vi.fn();
    const reportError = vi.fn();
    const controller = createSpaceController(
      deps({
        validateRoot: async () => {
          throw new Error("not authorized");
        },
        setRoot,
        reportError,
      }),
    );

    await expect(controller.changeRoot("a", "/blocked")).resolves.toBe(false);
    expect(setRoot).not.toHaveBeenCalled();
    expect(reportError).toHaveBeenCalledOnce();
  });

  it("prepares picker homes without applying their environment", async () => {
    const applyEnv = vi.fn();
    const controller = createSpaceController(deps({ applyEnv }));

    await expect(
      controller.homeForEnv({ kind: "wsl", distro: "Ubuntu" }),
    ).resolves.toBe("/home/me");
    expect(applyEnv).not.toHaveBeenCalled();
  });
});

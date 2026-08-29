import { describe, expect, it, vi } from "vitest";
import { planFileTabOpen, type Tab } from "@/modules/tabs/lib/useTabs";
import type { WorkspaceEnv } from "@/modules/workspace";
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
    const applied: string[] = [];
    const committed: string[] = [];
    const firstPreparationEntered = deferred<void>();
    const secondPreparationEntered = deferred<void>();
    const firstGate = deferred<PreparedWorkspace>();
    const secondGate = deferred<PreparedWorkspace>();
    let preparations = 0;
    const controller = createSpaceController(
      deps({
        getSpace: (id) =>
          id === "a"
            ? makeSpace("a", "/a", { kind: "wsl", distro: "Ubuntu" })
            : id === "wsl"
              ? makeSpace("wsl", "/work", { kind: "wsl", distro: "Ubuntu" })
              : null,
        prepareEnv: async () => {
          preparations += 1;
          if (preparations === 1) {
            firstPreparationEntered.resolve();
            return firstGate.promise;
          }
          secondPreparationEntered.resolve();
          return secondGate.promise;
        },
        applyEnv: ({ home }) => applied.push(home),
        commitActive: ({ spaceId }) => committed.push(spaceId),
      }),
    );

    const first = controller.activate({ spaceId: "a" });
    await firstPreparationEntered.promise;
    const second = controller.activate({ spaceId: "wsl" });
    firstGate.resolve({
      env: { kind: "wsl", distro: "Ubuntu" },
      home: "/home/first",
    });
    await secondPreparationEntered.promise;
    secondGate.resolve({
      env: { kind: "wsl", distro: "Ubuntu" },
      home: "/home/second",
    });

    await Promise.all([first, second]);
    expect(applied).toEqual(["/home/second"]);
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

  it("creates Space B terminals at Space B root independent of another shell cwd", async () => {
    const createTerminal = vi.fn();
    const controller = createSpaceController(
      deps({
        createTerminal,
        createMeta: (input) => makeSpace("b", input.root, input.env),
      }),
    );

    await controller.create({
      name: "b",
      root: "/space-b",
      env: { kind: "local" },
    });

    expect(createTerminal).toHaveBeenCalledWith("b", "/space-b");
  });

  it("keeps Space roots stable across external file and cross-Space navigation", async () => {
    const roots = new Map([
      ["a", "/a"],
      ["b", "/b"],
    ]);
    let active: { spaceId: string; tabId?: number } | null = {
      spaceId: "a",
      tabId: 1,
    };
    const setRoot = vi.fn();
    const tabs: Tab[] = [
      {
        id: 1,
        kind: "terminal",
        title: "shell",
        spaceId: "a",
        cwd: "/a/deep",
        paneTree: { kind: "leaf", id: 10 },
        activeLeafId: 10,
      },
      {
        id: 2,
        kind: "editor",
        title: "space-b.ts",
        path: "/b/space-b.ts",
        spaceId: "b",
        dirty: false,
        preview: false,
      },
    ];
    const controller = createSpaceController(
      deps({
        getSpace: (id) => {
          const root = roots.get(id);
          return root ? makeSpace(id, root) : null;
        },
        commitActive: (target) => {
          active = target;
        },
        setRoot,
      }),
    );

    const externalFile = planFileTabOpen(
      tabs,
      "/outside/notes.md",
      true,
      "a",
      () => 3,
    );
    expect(externalFile.tabs[2]).toMatchObject({
      path: "/outside/notes.md",
      spaceId: "a",
    });
    await expect(controller.activate({ spaceId: "b", tabId: 2 })).resolves.toBe(
      true,
    );

    expect(setRoot).not.toHaveBeenCalled();
    expect([...roots.entries()]).toEqual([
      ["a", "/a"],
      ["b", "/b"],
    ]);
    expect(active).toEqual({ spaceId: "b", tabId: 2 });
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

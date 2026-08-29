# Space-Owned Working Directory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each Space own one explicit, persisted working directory and environment, independent of terminal runtime cwd and active tab type.

**Architecture:** `SpaceMeta.root` and `SpaceMeta.env` become the only workspace-level context. A dependency-injected Space controller validates roots, prepares environment transitions, and commits Space activation without tab-derived fallbacks. Explorer, Source Control, status bar, search, and terminal creation consume the active Space root, while OSC 7 remains terminal-local.

**Tech Stack:** React 19, TypeScript, Zustand, Vitest, Tauri 2 IPC, Rust filesystem and workspace commands, pnpm.

**Spec:** `docs/superpowers/specs/2026-08-28-space-owned-cwd-design.md`

## Global Constraints

- Use pnpm only.
- Add no dependency.
- Frontend path imports use `@/` across modules.
- Frontend canonical paths use forward slashes and accept both slash styles at boundaries.
- Do not add em dash characters or emojis.
- Existing terminals must not receive input or change cwd when a Space root changes.
- Files outside the Space root remain openable in the current Space.
- Local and WSL environments are fixed per Space.
- No timer, watcher, or tab-event synchronization may write Space root.
- Native IPC validation must use the Space's explicit `WorkspaceEnv`.
- Keep `App.tsx` as a coordinator and put functional logic under the owning module.
- Every core invariant requires a focused test.

---

## Planned File Structure

### New files

- `src/modules/spaces/lib/spaceRoot.ts`: migration candidates, root availability types, and async migration with injected validation.
- `src/modules/spaces/lib/spaceRoot.test.ts`: migration precedence and unavailable-root tests.
- `src/modules/spaces/lib/rootValidation.ts`: explicit-env canonicalization, stat, and authorization.
- `src/modules/spaces/lib/rootValidation.test.ts`: validation order, normalization, and failure tests.
- `src/modules/spaces/lib/spaceController.ts`: root creation/change transactions and serialized latest-request Space activation.
- `src/modules/spaces/lib/spaceController.test.ts`: creation, root change, activation ordering, and race tests.
- `src/modules/spaces/components/SpaceDirectoryPicker.tsx`: Local/WSL-aware directory browser and typed path entry.
- `src/modules/spaces/components/SpaceRootRecovery.tsx`: unavailable-root sidebar recovery surface.
- `src/modules/spaces/lib/directoryPicker.ts`: pure picker path/reducer helpers.
- `src/modules/spaces/lib/directoryPicker.test.ts`: cross-platform path navigation tests.
- `src/modules/tabs/lib/terminalSpace.ts`: resolve a terminal leaf to its owning Space.
- `src/modules/tabs/lib/terminalSpace.test.ts`: background terminal environment routing tests.

### Removed files

- `src/modules/tabs/lib/useWorkspaceCwd.ts`: active-tab cwd derivation and last-terminal fallback.
- `src/modules/source-control/useRepositoryTargeting.ts`: fixed per-Space repository target state.
- `src/modules/source-control/repositoryTarget.ts`: active-tab and fixed-target routing rules.
- `src/modules/source-control/repositoryTarget.test.ts`: tests for the removed second path state.

### Main modified files

- `src/modules/spaces/lib/store.ts`: schema version persistence.
- `src/modules/spaces/lib/useSpaces.ts`: root issues, explicit root mutation, and no in-place env mutation.
- `src/modules/spaces/lib/useSpacesBoot.ts`: migration, root validation, explicit-env authorization, and recovery hydration.
- `src/modules/spaces/index.ts`: export controller, root, picker, and recovery interfaces.
- `src/app/hooks/useWorkspaceSwitcher.ts`: prepare and synchronously apply a validated env instead of destructive in-place Space migration.
- `src/modules/tabs/lib/useTabs.ts`: root-explicit terminal creation and unavailable-Space warm gate.
- `src/modules/spaces/SpaceSwitcher.tsx`: controller-owned activation and folder-first generic creation.
- `src/app/App.tsx`: active Space wiring only, with no cwd inference.
- `src/modules/statusbar/CwdBreadcrumb.tsx`: stable Space root breadcrumb.
- `src/modules/statusbar/StatusBar.tsx`: Space root and env actions.
- `src/modules/statusbar/WorkspaceEnvSelector.tsx`: create-in-environment semantics.
- `src/modules/explorer/FileExplorer.tsx`: replace folder actions.
- `src/modules/explorer/ExplorerSearch.tsx`: replace search-result folder actions.
- `src/modules/source-control/useSourceControlContext.ts`: active Space root only.
- `src/modules/source-control/SourceControlPanel.tsx`: remove fixed-target pending and follow controls.
- `src/lib/native.ts`: optional explicit-env filesystem and authorization helpers.
- `src/modules/terminal/TerminalStack.tsx` or `src/modules/tabs/lib/useTabs.ts`: prevent cold terminal warmup for unavailable roots.
- `TERAX.md`: document the new Space ownership invariant.

---

### Task 1: Add Versioned Space Root Migration

**Files:**
- Create: `src/modules/spaces/lib/spaceRoot.ts`
- Create: `src/modules/spaces/lib/spaceRoot.test.ts`
- Modify: `src/modules/spaces/lib/store.ts:5-48`
- Modify: `src/modules/spaces/lib/useSpaces.ts:22-51`
- Modify: `src/modules/spaces/lib/useSpacesBoot.ts:24-117`
- Modify: `src/modules/spaces/lib/activeSpace.test.ts:1-88`

**Interfaces:**
- Produces: `SPACE_SCHEMA_VERSION = 2`
- Produces: `SpaceRootIssue`, `SpaceRootIssues`, `RootMigrationResult`
- Produces: `useSpaces.rootIssues` and `hydrate(..., rootIssues)`
- Produces: `legacyRootCandidate(space, state, envHome): string | null`
- Produces: `migrateSpaceRoots(loaded, resolveHome, validateRoot): Promise<RootMigrationResult>`
- Consumes: `SpaceMeta`, `SpaceState`, `SerializedNode`, `WorkspaceEnv`

- [ ] **Step 1: Write failing candidate-precedence tests**

Define the test builders in `spaceRoot.test.ts` so every later example is executable:

```ts
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
    legacyRootCandidate(space({ root: "/project" }), terminalState("/shell"), "/home/me"),
  ).toBe("/project");
});
```

- [ ] **Step 2: Run the focused test and confirm red**

Run: `pnpm test -- src/modules/spaces/lib/spaceRoot.test.ts`

Expected: FAIL because `spaceRoot.ts` and `legacyRootCandidate` do not exist.

- [ ] **Step 3: Implement serialized terminal traversal**

```ts
export function legacyRootCandidate(
  space: SpaceMeta,
  state: SpaceState | undefined,
  envHome: string | null,
): string | null {
  if (space.root?.trim()) return space.root;
  const active = state?.tabs[state.activeTabIndex];
  const activeCwd = active?.kind === "terminal" ? activeLeafCwd(active.tree) : null;
  if (activeCwd) return activeCwd;
  for (const tab of state?.tabs ?? []) {
    if (tab.kind !== "terminal") continue;
    const cwd = firstLeafCwd(tab.tree);
    if (cwd) return cwd;
  }
  return envHome;
}
```

- [ ] **Step 4: Add failing migration success and failure tests**

```ts
it("writes canonical roots and records unavailable candidates", async () => {
  const result = await migrateSpaceRoots(loadedSpaces(), async () => "/home/me", async (path) => {
    if (path === "/missing") throw new Error("not found");
    return `/canon${path}`;
  });

  expect(result.spaces.find((s) => s.id === "ok")?.root).toBe("/canon/project");
  expect(result.issues.missing).toEqual({
    candidate: "/missing",
    message: "not found",
  });
});
```

- [ ] **Step 5: Add schema version persistence and migration result types**

```ts
export const SPACE_SCHEMA_VERSION = 2;
const KEY_SCHEMA = "schemaVersion";

export type LoadedSpaces = {
  schemaVersion: number;
  spaces: SpaceMeta[];
  activeId: string | null;
  states: Map<string, SpaceState>;
};

export async function saveSchemaVersion(version: number): Promise<void> {
  await store.set(KEY_SCHEMA, version);
}
```

Implement `migrateSpaceRoots` with injected `resolveHome` and `validateRoot`. Preserve the original candidate in `SpaceMeta.root` on validation failure and return the issue separately.

- [ ] **Step 6: Integrate migration into boot without tab-derived runtime fallback**

Add `rootIssues: SpaceRootIssues` to the Zustand state and extend `hydrate` to accept the migration issues. In `useSpacesBoot`, migrate before hydrating tabs, save changed Space metadata and schema version once, and pass `issues` into `useSpaces.hydrate`. Remove `freshTabCwd` use for existing Spaces. An empty valid Space gets `freshTerminalTab(space.id, space.root, allocId)`.

For a brand-new store, resolve and validate `launchCwd ?? localHome ?? envHome` before creating the Default Space. If no candidate validates, hydrate one Default Space with `root: null` plus a `SpaceRootIssue` so the normal recovery UI can select a directory. Do not create a terminal until recovery succeeds.

- [ ] **Step 7: Run focused Space tests**

Run: `pnpm test -- src/modules/spaces/lib/spaceRoot.test.ts src/modules/spaces/lib/activeSpace.test.ts src/modules/spaces/lib/serialize.test.ts`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/modules/spaces/lib/store.ts src/modules/spaces/lib/spaceRoot.ts src/modules/spaces/lib/spaceRoot.test.ts src/modules/spaces/lib/useSpaces.ts src/modules/spaces/lib/useSpacesBoot.ts src/modules/spaces/lib/activeSpace.test.ts
git commit -m "refactor: version space root persistence"
```

---

### Task 2: Add Explicit-Environment Root Validation and Store Mutations

**Files:**
- Create: `src/modules/spaces/lib/rootValidation.ts`
- Create: `src/modules/spaces/lib/rootValidation.test.ts`
- Create: `src/modules/spaces/lib/useSpaces.test.ts`
- Modify: `src/lib/native.ts:4-23,140-173`
- Modify: `src/modules/spaces/lib/useSpaces.ts:11-130`
- Modify: `src/modules/spaces/index.ts:1-6`

**Interfaces:**
- Produces: `FileStat = { size: number; mtime: number; kind: "file" | "dir" | "symlink" }`
- Produces: `validateSpaceRoot(path, env, fs): Promise<string>`
- Produces: `useSpaces.getState().setRoot(id, root)`
- Extends: existing `rootIssues` with `setRootIssue` and `clearRootIssue`
- Consumes: `WorkspaceEnv`, `SpaceRootIssue`

- [ ] **Step 1: Write a failing explicit-env validation test**

```ts
it("canonicalizes, verifies a directory, and authorizes with the same env", async () => {
  const env = { kind: "wsl", distro: "Ubuntu" } as const;
  const calls: string[] = [];
  const root = await validateSpaceRoot("/repo/../repo", env, {
    canonicalize: async (path, actualEnv) => {
      calls.push(`canonicalize:${path}:${workspaceScopeKey(actualEnv)}`);
      return "/repo";
    },
    stat: async (path, actualEnv) => {
      calls.push(`stat:${path}:${workspaceScopeKey(actualEnv)}`);
      return { size: 0, mtime: 0, kind: "dir" };
    },
    authorize: async (path, actualEnv) => {
      calls.push(`authorize:${path}:${workspaceScopeKey(actualEnv)}`);
    },
  });
  expect(root).toBe("/repo");
  expect(calls).toEqual([
    "canonicalize:/repo/../repo:wsl:Ubuntu",
    "stat:/repo:wsl:Ubuntu",
    "authorize:/repo:wsl:Ubuntu",
  ]);
});
```

- [ ] **Step 2: Run the root validation test and confirm red**

Run: `pnpm test -- src/modules/spaces/lib/rootValidation.test.ts`

Expected: FAIL because `validateSpaceRoot` does not exist.

- [ ] **Step 3: Extend native helpers with explicit env defaults**

```ts
export type FileStat = {
  size: number;
  mtime: number;
  kind: "file" | "dir" | "symlink";
};

workspaceAuthorize: (path: string, workspace = currentWorkspaceEnv()) =>
  invoke<string>("workspace_authorize", { path, workspace }),
canonicalize: (path: string, workspace = currentWorkspaceEnv()) =>
  invoke<string>("fs_canonicalize", { path, workspace }),
stat: (path: string, workspace = currentWorkspaceEnv()) =>
  invoke<FileStat>("fs_stat", { path, workspace }),
listSubdirs: (path: string, showHidden: boolean, workspace = currentWorkspaceEnv()) =>
  invoke<string[]>("list_subdirs", { path, showHidden, workspace }),
```

- [ ] **Step 4: Implement validation without state mutation**

```ts
export async function validateSpaceRoot(
  path: string,
  env: WorkspaceEnv,
  fs: SpaceRootFs = nativeRootFs,
): Promise<string> {
  const canonical = (await fs.canonicalize(path, env)).replace(/\\/g, "/");
  const stat = await fs.stat(canonical, env);
  if (stat.kind !== "dir") throw new Error("Space root must be a directory.");
  await fs.authorize(canonical, env);
  return canonical;
}
```

- [ ] **Step 5: Write failing store mutation tests**

Define deterministic store builders in `useSpaces.test.ts`:

```ts
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

function seedSpaces(spaces: SpaceMeta[], rootIssues: SpaceRootIssues = {}): void {
  useSpaces.setState({
    spaces,
    activeId: spaces[0]?.id ?? null,
    hydrated: true,
    initialActiveIndex: {},
    rootIssues,
  });
}

it("changes only the selected Space root and clears its issue", () => {
  seedSpaces(
    [makeSpace("a", "/a"), makeSpace("b", "/b")],
    { a: { candidate: "/missing", message: "not found" } },
  );

  useSpaces.getState().setRoot("a", "/next");

  expect(useSpaces.getState().spaces.map((s) => s.root)).toEqual(["/next", "/b"]);
  expect(useSpaces.getState().rootIssues.a).toBeUndefined();
});
```

Also test that `create` requires a non-empty `root: string` and that the store no longer exposes `setEnv`.

- [ ] **Step 6: Implement root issue state and explicit root mutation**

Change `CreateInput.root` from `string | null` to `string`. Add `rootIssues`, hydrate it from boot, implement `setRoot`, `setRootIssue`, and `clearRootIssue`, and delete `setEnv`.

- [ ] **Step 7: Run focused tests and typecheck**

Run: `pnpm test -- src/modules/spaces/lib/rootValidation.test.ts src/modules/spaces/lib/useSpaces.test.ts && pnpm check-types`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/lib/native.ts src/modules/spaces/lib/rootValidation.ts src/modules/spaces/lib/rootValidation.test.ts src/modules/spaces/lib/useSpaces.ts src/modules/spaces/lib/useSpaces.test.ts src/modules/spaces/index.ts
git commit -m "refactor: make space root updates explicit"
```

---

### Task 3: Serialize Space Activation and Root Transactions

**Files:**
- Create: `src/modules/spaces/lib/spaceController.ts`
- Create: `src/modules/spaces/lib/spaceController.test.ts`
- Modify: `src/app/hooks/useWorkspaceSwitcher.ts:13-137`
- Modify: `src/modules/spaces/index.ts`

**Interfaces:**
- Produces: `PreparedWorkspace = { env: WorkspaceEnv; home: string }`
- Produces: `prepareWorkspaceEnv(env): Promise<PreparedWorkspace>`
- Produces: `applyWorkspaceEnv(prepared): void`
- Produces: `createSpaceController(deps): SpaceController`
- Produces: `SpaceController.homeForEnv(env): Promise<string>`
- Produces: `SpaceController.activate({spaceId, tabId?}): Promise<boolean>`
- Produces: `SpaceController.create({name, root, env}): Promise<SpaceMeta | null>`
- Produces: `SpaceController.changeRoot(spaceId, path): Promise<boolean>`

- [ ] **Step 1: Write failing activation-order and race tests**

Define the asynchronous test gate and dependency builder in `spaceController.test.ts`:

```ts
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

function deps(overrides: Partial<SpaceControllerDeps> = {}): SpaceControllerDeps {
  const spaces = new Map([
    ["a", makeSpace("a", "/a")],
    ["b", makeSpace("b", "/b")],
    ["wsl", makeSpace("wsl", "/work", { kind: "wsl", distro: "Ubuntu" })],
  ]);
  return {
    getSpace: (id) => spaces.get(id) ?? null,
    currentEnv: () => ({ kind: "local" }),
    validateRoot: async (path) => path,
    prepareEnv: async (env) => ({ env, home: env.kind === "local" ? "/Users/me" : "/home/me" }),
    applyEnv: () => {},
    commitActive: () => {},
    createMeta: (input) => makeSpace(input.name, input.root, input.env),
    createTerminal: () => 1,
    setRoot: () => {},
    reportError: () => {},
    ...overrides,
  };
}

it("does not expose a Space until its environment is prepared", async () => {
  const events: string[] = [];
  const gate = deferred<PreparedWorkspace>();
  const controller = createSpaceController(deps({
    prepareEnv: async () => gate.promise,
    applyEnv: () => events.push("apply-env"),
    commitActive: () => events.push("commit-space"),
  }));

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
  const controller = createSpaceController(deps({
    prepareEnv: async (env) => gates.get(workspaceScopeKey(env))!.promise,
    commitActive: ({ spaceId }) => committed.push(spaceId),
  }));
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
```

- [ ] **Step 2: Run the controller test and confirm red**

Run: `pnpm test -- src/modules/spaces/lib/spaceController.test.ts`

Expected: FAIL because `createSpaceController` does not exist.

- [ ] **Step 3: Split workspace preparation from synchronous application**

Replace destructive `switchWorkspace` with:

```ts
const prepareWorkspaceEnv = useCallback(async (env: WorkspaceEnv) => {
  const nextHome = await resolveEnvHome(env);
  await native.workspaceAuthorize(nextHome, env);
  return { env: env.kind === "local" ? LOCAL_WORKSPACE : env, home: nextHome };
}, []);

const applyWorkspaceEnv = useCallback((prepared: PreparedWorkspace) => {
  setWorkspaceEnv(prepared.env);
  setHome(prepared.home);
  setLaunchCwd(prepared.home);
}, [setWorkspaceEnv]);
```

Keep a boot helper that prepares and applies the restored active Space env. Do not clear tabs or mutate a Space env.

- [ ] **Step 4: Implement a dependency-injected serialized controller**

```ts
export type SpaceControllerDeps = {
  getSpace(id: string): SpaceMeta | null;
  currentEnv(): WorkspaceEnv;
  validateRoot(path: string, env: WorkspaceEnv): Promise<string>;
  prepareEnv(env: WorkspaceEnv): Promise<PreparedWorkspace>;
  applyEnv(prepared: PreparedWorkspace): void;
  commitActive(target: { spaceId: string; tabId?: number }): void;
  createMeta(input: { name: string; root: string; env: WorkspaceEnv }): SpaceMeta;
  createTerminal(spaceId: string, root: string): number;
  setRoot(spaceId: string, root: string): void;
  reportError(message: string): void;
};

export type SpaceController = {
  homeForEnv(env: WorkspaceEnv): Promise<string>;
  activate(target: { spaceId: string; tabId?: number }): Promise<boolean>;
  create(input: { name: string; root: string; env: WorkspaceEnv }): Promise<SpaceMeta | null>;
  changeRoot(spaceId: string, path: string): Promise<boolean>;
};
```

Use a monotonic request id and one promise chain. Prepare env without global mutation, discard stale results, then call `applyEnv` and `commitActive` in that order. Root changes capture `spaceId`, env, and request id and commit only after validation. `homeForEnv` calls `prepareEnv` but does not call `applyEnv`, so a Local or WSL picker can start at the correct home without changing the active Space.

For `create`, validate the root and prepare the target env before persisting metadata or creating the terminal. Only after both succeed may it call `createMeta`, `createTerminal`, `applyEnv`, and `commitActive`. A preparation failure leaves no partial Space.

- [ ] **Step 5: Add creation and root-change tests**

Test that creation validates before store creation, creates exactly one cold terminal at the canonical root, and activates the new Space. Test that a failed root change preserves the old root and records a user-visible error through the supplied `reportError` dependency.

- [ ] **Step 6: Run controller and workspace tests**

Run: `pnpm test -- src/modules/spaces/lib/spaceController.test.ts src/modules/spaces/lib/rootValidation.test.ts && pnpm check-types`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app/hooks/useWorkspaceSwitcher.ts src/modules/spaces/lib/spaceController.ts src/modules/spaces/lib/spaceController.test.ts src/modules/spaces/index.ts
git commit -m "refactor: serialize space activation"
```

---

### Task 4: Make Active Space Root the Only Workspace Read Path

**Files:**
- Delete: `src/modules/tabs/lib/useWorkspaceCwd.ts`
- Modify: `src/modules/tabs/index.ts`
- Modify: `src/modules/tabs/lib/useTabs.ts:556-886,1115-1195`
- Modify: `src/modules/spaces/SpaceSwitcher.tsx:20-115`
- Modify: `src/app/App.tsx:119-380,540-580,1018-1110`
- Modify: `src/modules/tabs/lib/useWindowTitle.ts`
- Test: `src/modules/tabs/lib/planSpaceRemoval.test.ts`
- Test: `src/modules/spaces/lib/spaceController.test.ts`

**Interfaces:**
- Consumes: `SpaceController`, `SpaceMeta.root`, `rootIssues`
- Produces: root-explicit wrappers for new terminal, block, private, and agent tabs
- Produces: `SpaceSwitcher.onActivateSpace(id)` instead of direct `useSpaces.setActive`

- [ ] **Step 1: Add failing tests for explicit root use**

Extend controller and tab planning tests so a new terminal in Space B receives `/space-b` even when the active terminal in Space A reports `/shell-a`. Extend `planSpaceRemoval.test.ts` so fallback terminal cwd equals the fallback Space root.

- [ ] **Step 2: Run focused tests and confirm red where behavior is still inferred**

Run: `pnpm test -- src/modules/spaces/lib/spaceController.test.ts src/modules/tabs/lib/planSpaceRemoval.test.ts`

Expected: at least one FAIL because App and creation callbacks still read active terminal cwd.

- [ ] **Step 3: Delete `useWorkspaceCwd` and derive active Space directly**

```ts
const activeSpace = useSpaces((state) =>
  state.spaces.find((space) => space.id === state.activeId) ?? null,
);
const activeSpaceRoot = activeSpace?.root ?? null;
const activeRootIssue = activeSpace ? rootIssues[activeSpace.id] : undefined;
```

Pass `activeSpaceRoot` to Explorer, Source Control, window-title logic, search roots, and root-explicit terminal wrappers. Do not use active file path or terminal cwd as fallback.

At this milestone, pass `cwd={activeSpaceRoot}`, omit the active file path from `StatusBar`, delete `sendCd`, and route the existing breadcrumb's `onCd` callback to `controller.changeRoot(activeSpace.id, path)`. Task 5 then narrows and renames the component interface and adds arbitrary folder selection.

- [ ] **Step 4: Replace all terminal creation inheritance**

```ts
const newTerminal = useCallback(() => {
  if (!activeSpace?.root || activeRootIssue) return null;
  return newTab(activeSpace.root);
}, [activeRootIssue, activeSpace, newTab]);
```

Apply the same root rule to block, private, agent, split, fallback, and `newTabInSpace` paths. Keep terminal runtime cwd subtitle and OSC updates unchanged.

- [ ] **Step 5: Route Space activation and cross-Space tab jumps through the controller**

Remove the effect that adopts env after `activeSpaceId` changes. `SpaceSwitcher`, tab move/reorder follow behavior, delete fallback, `jumpToTab`, and keyboard Space selection call `controller.activate`.

`SpaceSwitcher` must stop reading `setActive` directly:

```ts
type Props = {
  onActivateSpace: (id: string) => void;
  // existing props
};
```

- [ ] **Step 6: Keep external file opens in the current Space**

Retain `openFileTab(path, ..., { spaceId: currentSpaceId })` behavior and add a focused `planFileTabOpen` assertion that an external path changes no Space metadata.

- [ ] **Step 7: Run tabs, spaces, typecheck, and LSP diagnostics**

Run: `pnpm test -- src/modules/tabs/lib src/modules/spaces/lib && pnpm check-types`

Then run LSP diagnostics for `src/app/App.tsx`, `src/modules/tabs/lib/useTabs.ts`, and `src/modules/spaces/SpaceSwitcher.tsx`.

Expected: PASS and no primary LSP errors.

- [ ] **Step 8: Commit**

```bash
git add src/app/App.tsx src/modules/spaces/SpaceSwitcher.tsx src/modules/tabs src/modules/spaces/lib/spaceController.test.ts
git commit -m "refactor: derive workspace context from spaces"
```

---

### Task 5: Turn the Bottom Bar into the Space Root Control

**Files:**
- Create: `src/modules/spaces/components/SpaceDirectoryPicker.tsx`
- Create: `src/modules/spaces/lib/directoryPicker.ts`
- Create: `src/modules/spaces/lib/directoryPicker.test.ts`
- Modify: `src/modules/statusbar/CwdBreadcrumb.tsx:29-298`
- Modify: `src/modules/statusbar/StatusBar.tsx:14-60`
- Modify: `src/modules/statusbar/WorkspaceEnvSelector.tsx:17-86`
- Modify: `src/modules/statusbar/index.ts`
- Modify: `src/modules/spaces/index.ts`
- Modify: `src/app/App.tsx:1372-1402`

**Interfaces:**
- Produces: `SpaceRootBreadcrumb({root, home, issue, onChangeRoot, onChooseFolder})`
- Produces: `SpaceDirectoryPicker({open, env, initialPath, mode, onCancel, onSelect})`
- Produces: `DirectoryPickerMode = "change-root" | "create-space"`
- Consumes: `native.listSubdirs`, `validateSpaceRoot`, `SpaceController.changeRoot/create`

- [ ] **Step 1: Write failing path navigation tests**

```ts
it("joins child directories using canonical forward slashes", () => {
  expect(joinDirectory("C:/Users/me", "repo")).toBe("C:/Users/me/repo");
  expect(joinDirectory("/home/me/", "repo")).toBe("/home/me/repo");
});

it("preserves Unix and Windows filesystem roots when navigating up", () => {
  expect(parentDirectory("/home/me")).toBe("/home");
  expect(parentDirectory("C:/Users/me")).toBe("C:/Users");
  expect(parentDirectory("C:/")).toBe("C:/");
});
```

- [ ] **Step 2: Run the picker helper test and confirm red**

Run: `pnpm test -- src/modules/spaces/lib/directoryPicker.test.ts`

Expected: FAIL because the helper does not exist.

- [ ] **Step 3: Implement pure picker helpers and the picker component**

The picker keeps typed path, loaded child directories, loading state, and error state. It calls `native.listSubdirs(path, showHidden, env)` and returns a selected candidate to the controller. Cancel closes without state change.

```ts
type Props = {
  open: boolean;
  env: WorkspaceEnv;
  initialPath: string;
  mode: "change-root" | "create-space";
  onCancel: () => void;
  onSelect: (path: string) => void;
};
```

- [ ] **Step 4: Replace cwd breadcrumb semantics**

Rename the exported component to `SpaceRootBreadcrumb`. Remove `filePath` and `onCd`. Ancestor and subdirectory selection call `onChangeRoot`. Add `Choose Folder...`. When `issue` exists, display its candidate and recovery action instead of silently showing home.

- [ ] **Step 5: Change environment selector semantics**

Replace `onSelect(env)` with `onCreateInEnv(env)`. Keep the current env label, but selecting a different Local or WSL env opens the create-space picker. Selecting the already active env is a no-op.

- [ ] **Step 6: Wire picker state in App**

Use one discriminated picker request:

```ts
type PickerRequest =
  | { mode: "change-root"; spaceId: string; env: WorkspaceEnv; initialPath: string }
  | { mode: "create-space"; env: WorkspaceEnv; initialPath: string };
```

On selection, call `controller.changeRoot` or `controller.create`. Existing terminals receive no writes.

- [ ] **Step 7: Run focused tests and typecheck**

Run: `pnpm test -- src/modules/spaces/lib/directoryPicker.test.ts src/modules/statusbar/lib/pathUtils.test.ts && pnpm check-types`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/app/App.tsx src/modules/spaces/components/SpaceDirectoryPicker.tsx src/modules/spaces/lib/directoryPicker.ts src/modules/spaces/lib/directoryPicker.test.ts src/modules/spaces/index.ts src/modules/statusbar
git commit -m "feat: make status bar control space roots"
```

---

### Task 6: Replace Explorer Folder Actions with Open in New Space

**Files:**
- Modify: `src/modules/explorer/FileExplorer.tsx:55-66,560-855`
- Modify: `src/modules/explorer/ExplorerSearch.tsx:45-75,280-320`
- Modify: `src/modules/explorer/lib/contextActions.ts`
- Modify: `src/modules/explorer/lib/contextActions.test.ts`
- Modify: `src/app/App.tsx:561-576,1029-1050,1302-1322`
- Modify: `src/modules/spaces/SpaceSwitcher.tsx:20-40,300-330`
- Modify: `src/modules/command-palette/commands.ts`

**Interfaces:**
- Produces: `onOpenInNewSpace(path: string)` Explorer prop
- Produces: `spaceNameFromRoot(path): string`
- Consumes: `SpaceController.create`, active Space env, create-space picker

- [ ] **Step 1: Write a failing cross-platform Space-name test**

```ts
it("names a Space from Unix and Windows folder basenames", () => {
  expect(spaceNameFromRoot("/work/terax")).toBe("terax");
  expect(spaceNameFromRoot("C:\\work\\terax")).toBe("terax");
});
```

- [ ] **Step 2: Run the context action test and confirm red**

Run: `pnpm test -- src/modules/explorer/lib/contextActions.test.ts`

Expected: FAIL because `spaceNameFromRoot` does not exist.

- [ ] **Step 3: Replace props and all three menu sites**

Remove `onRevealInTerminal` and `onOpenInSourceControl` from `FileExplorer` and `ExplorerSearch`. Add `onOpenInNewSpace`. Replace the tree-row, root/background, and search-result menu items with:

```tsx
{menuTarget.isDir && onOpenInNewSpace ? (
  <ContextMenuItem onSelect={() => onOpenInNewSpace(menuTarget.path)}>
    Open in New Space
  </ContextMenuItem>
) : null}
```

Keep `Open Git History` unchanged.

- [ ] **Step 4: Delete `cdInNewTab` and wire explicit Space creation**

`handleOpenInNewSpace(path)` calls `controller.create` with the active Space env and `spaceNameFromRoot(path)`. Generic SpaceSwitcher and Command Palette `New Space` open the picker instead of reading active terminal cwd.

- [ ] **Step 5: Run Explorer tests and typecheck**

Run: `pnpm test -- src/modules/explorer/lib/contextActions.test.ts src/modules/explorer/lib && pnpm check-types`

Expected: PASS and no references to `onRevealInTerminal`, `onOpenInSourceControl`, `cdInNewTab`, or the two removed menu labels.

- [ ] **Step 6: Commit**

```bash
git add src/app/App.tsx src/modules/command-palette/commands.ts src/modules/explorer src/modules/spaces/SpaceSwitcher.tsx
git commit -m "feat: open explorer folders as spaces"
```

---

### Task 7: Bind Source Control Only to Active Space Root

**Files:**
- Delete: `src/modules/source-control/useRepositoryTargeting.ts`
- Delete: `src/modules/source-control/repositoryTarget.ts`
- Delete: `src/modules/source-control/repositoryTarget.test.ts`
- Modify: `src/modules/source-control/useSourceControlContext.ts:13-121`
- Modify: `src/modules/source-control/SourceControlPanel.tsx:70-115` and fixed-target banner sites
- Modify: `src/modules/source-control/index.ts`
- Modify: `src/app/App.tsx:675-723,1302-1345`
- Create: `src/modules/source-control/spaceRepository.test.ts`
- Create: `src/modules/source-control/spaceRepository.ts`

**Interfaces:**
- Produces: `sourceControlPathForSpace(root, issue): string | null`
- Consumes: active `SpaceMeta.root`, `SpaceRootIssue | undefined`
- Preserves: explicit `openGitHistory(path)` repository resolution

- [ ] **Step 1: Write the failing Space-root repository test**

```ts
it("uses only the active Space root", () => {
  expect(sourceControlPathForSpace("/repo/packages/app", undefined)).toBe(
    "/repo/packages/app",
  );
});

it("pauses Source Control for an unavailable root", () => {
  expect(
    sourceControlPathForSpace("/missing", {
      candidate: "/missing",
      message: "not found",
    }),
  ).toBeNull();
});
```

- [ ] **Step 2: Run the source-control test and confirm red**

Run: `pnpm test -- src/modules/source-control/spaceRepository.test.ts`

Expected: FAIL because `spaceRepository.ts` does not exist.

- [ ] **Step 3: Simplify Source Control context**

`useSourceControlContext` accepts `spaceRoot` and `rootIssue`, derives one path through `sourceControlPathForSpace`, and retains `toggleSourceControl` plus `openGitGraphFromContext`. It no longer accepts active tab, terminal cwd, Explorer root, launch cwd, repository target, or follow callbacks.

- [ ] **Step 4: Remove fixed-target UI and state**

Remove `repositoryTarget`, `onFollowRepositoryContext`, pending masking, and fixed-repository banner logic from `SourceControlPanel`. Delete the target files and exports. Keep explicit Git tab `repoRoot` behavior in tab planning code.

- [ ] **Step 5: Keep Explorer Git History explicit**

Move or retain a single `openGitHistoryForPath(path)` callback that calls `native.gitResolveRepo(path)` and opens a history tab. It must not update Source Control context.

- [ ] **Step 6: Run Source Control and tab tests**

Run: `pnpm test -- src/modules/source-control src/modules/tabs/lib/planCommitHistoryOpen.test.ts src/modules/tabs/lib/planGitDiffOpen.test.ts && pnpm check-types`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app/App.tsx src/modules/source-control src/modules/tabs/lib/planCommitHistoryOpen.test.ts src/modules/tabs/lib/planGitDiffOpen.test.ts
git commit -m "refactor: bind source control to space roots"
```

---

### Task 8: Route Background Terminal Authorization Through Owning Space

**Files:**
- Create: `src/modules/tabs/lib/terminalSpace.ts`
- Create: `src/modules/tabs/lib/terminalSpace.test.ts`
- Modify: `src/modules/tabs/index.ts`
- Modify: `src/app/App.tsx:950-968`
- Modify: `src/lib/native.ts` if explicit env defaults were not completed in Task 2

**Interfaces:**
- Produces: `spaceIdForLeaf(tabs, leafId): string | null`
- Consumes: `useSpaces.getState().spaces`, `native.workspaceAuthorize(path, env)`

- [ ] **Step 1: Write a failing leaf ownership test**

Define the terminal builder in `terminalSpace.test.ts`:

```ts
function terminalTab(input: { id: number; spaceId: string; leafId: number }): Tab {
  return {
    id: input.id,
    kind: "terminal",
    spaceId: input.spaceId,
    title: "shell",
    paneTree: { kind: "leaf", id: input.leafId },
    activeLeafId: input.leafId,
  };
}

it("finds the Space that owns a background terminal leaf", () => {
  const tabs = [
    terminalTab({ id: 1, spaceId: "local", leafId: 10 }),
    terminalTab({ id: 2, spaceId: "wsl", leafId: 20 }),
  ];
  expect(spaceIdForLeaf(tabs, 20)).toBe("wsl");
});
```

Also test split pane leaves and unknown leaf ids.

- [ ] **Step 2: Run the terminal ownership test and confirm red**

Run: `pnpm test -- src/modules/tabs/lib/terminalSpace.test.ts`

Expected: FAIL because `spaceIdForLeaf` does not exist.

- [ ] **Step 3: Implement ownership lookup using pane-tree membership**

```ts
export function spaceIdForLeaf(tabs: Tab[], leafId: number): string | null {
  return (
    tabs.find((tab) => tab.kind === "terminal" && hasLeaf(tab.paneTree, leafId))
      ?.spaceId ?? null
  );
}
```

- [ ] **Step 4: Use owning Space env in OSC cwd authorization**

```ts
const spaceId = spaceIdForLeaf(tabsRef.current, leafId);
const env = useSpaces.getState().spaces.find((space) => space.id === spaceId)?.env;
if (env) void native.workspaceAuthorize(cwd, env);
```

Keep `setLeafCwd(leafId, cwd)` unchanged. Do not call `setRoot`.

Deduplicate authorization by `workspaceScopeKey(env) + "\0" + cwd`, not cwd alone.

- [ ] **Step 5: Run focused terminal, tab, and OSC tests**

Run: `pnpm test -- src/modules/tabs/lib/terminalSpace.test.ts src/modules/terminal/lib/osc-handlers.test.ts src/modules/terminal/lib/terminalPaste.test.ts && pnpm check-types`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/app/App.tsx src/lib/native.ts src/modules/tabs/index.ts src/modules/tabs/lib/terminalSpace.ts src/modules/tabs/lib/terminalSpace.test.ts
git commit -m "fix: authorize terminal cwd in its space environment"
```

---

### Task 9: Add Unavailable-Root Recovery and Terminal Warm Gate

**Files:**
- Create: `src/modules/spaces/components/SpaceRootRecovery.tsx`
- Modify: `src/modules/spaces/index.ts`
- Modify: `src/modules/tabs/lib/useTabs.ts:556-620`
- Modify: `src/app/App.tsx:119-190,1302-1325`
- Modify: `src/modules/terminal/TerminalStack.tsx` only if the warm gate cannot remain in `useTabs`
- Create: `src/modules/tabs/lib/tabWarmPolicy.test.ts`
- Create: `src/modules/tabs/lib/tabWarmPolicy.ts`

**Interfaces:**
- Produces: `canWarmTab(tab, rootIssues): boolean`
- Produces: `SpaceRootRecovery({space, issue, onChooseFolder})`
- Consumes: `SpaceRootIssues`, picker recovery flow

- [ ] **Step 1: Write a failing terminal warm policy test**

Define complete tab and issue builders in `tabWarmPolicy.test.ts`:

```ts
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

it("keeps a terminal cold while its Space root is unavailable", () => {
  const tab = terminalTab({ spaceId: "broken", cold: true });
  expect(
    canWarmTab(tab, {
      broken: { candidate: "/missing", message: "not found" },
    }),
  ).toBe(false);
});

it("does not block editor tabs or valid terminal Spaces", () => {
  expect(canWarmTab(editorTab({ spaceId: "broken" }), brokenIssues())).toBe(true);
  expect(canWarmTab(terminalTab({ spaceId: "ok" }), brokenIssues())).toBe(true);
});
```

- [ ] **Step 2: Run the warm policy test and confirm red**

Run: `pnpm test -- src/modules/tabs/lib/tabWarmPolicy.test.ts`

Expected: FAIL because the policy does not exist.

- [ ] **Step 3: Gate cold terminal warmup**

Pass a stable `canWarmSpace(spaceId)` callback or `rootIssues` snapshot into `useTabs`. In the cold-tab warming effect, leave an unavailable Space terminal cold. Re-run the effect when its issue is cleared so recovery can warm the active tab.

- [ ] **Step 4: Render explicit Explorer recovery**

When the active Space has a root issue, render `SpaceRootRecovery` instead of `FileExplorer`. Show the candidate, concise error, and `Choose Folder...`. Disable new terminal actions for that Space. Keep editor and file tabs mounted and usable.

- [ ] **Step 5: Recover through the normal root transaction**

The recovery action opens the change-root picker for the affected Space. Successful `controller.changeRoot` clears the issue through `setRoot`; the Explorer mounts and the active cold terminal may warm. Failure preserves the issue.

- [ ] **Step 6: Run recovery, tabs, and Space tests**

Run: `pnpm test -- src/modules/tabs/lib/tabWarmPolicy.test.ts src/modules/tabs/lib src/modules/spaces/lib && pnpm check-types`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app/App.tsx src/modules/spaces/components/SpaceRootRecovery.tsx src/modules/spaces/index.ts src/modules/tabs/lib/tabWarmPolicy.ts src/modules/tabs/lib/tabWarmPolicy.test.ts src/modules/tabs/lib/useTabs.ts src/modules/terminal/TerminalStack.tsx
git commit -m "feat: recover unavailable space roots"
```

---

### Task 10: Remove Legacy Paths, Update Architecture Docs, and Verify the Whole Feature

**Files:**
- Modify: `TERAX.md`
- Modify: `src/modules/spaces/lib/activeSpace.ts` and tests if `freshTabCwd` is now unused
- Modify: any barrel files with removed exports
- Verify: all files changed by Tasks 1 through 9

**Interfaces:**
- Consumes: all prior task interfaces
- Produces: no new runtime interface

- [ ] **Step 1: Search for forbidden legacy ownership paths**

Run:

```bash
rg -n "useWorkspaceCwd|lastTerminalCwd|onRevealInTerminal|onOpenInSourceControl|Open in Terminal|Open in Source Control|repositoryTarget|setEnv\(" src
```

Expected: no runtime matches. Test names and migration comments may mention removed behavior only when explicitly asserting absence.

- [ ] **Step 2: Add final invariant tests for cross-Space navigation**

Extend `spaceController.test.ts` with one scenario that starts with Space A root `/a`, records terminal runtime cwd `/a/deep`, opens an external file, activates a Space B file tab, and asserts the two Space roots remain `/a` and `/b` while the final active context is B.

- [ ] **Step 3: Update `TERAX.md`**

Replace the tabs-module statement that `useWorkspaceCwd` derives Explorer root. Document:

- Space owns stable root and env.
- terminal and pane cwd are runtime-only.
- status bar is the Space root mutation UI.
- Explorer and Source Control consume active Space root.
- external files do not change root.
- inactive terminal authorization uses owning Space env.

- [ ] **Step 4: Run fresh changed-file diagnostics**

Run primary LSP diagnostics for every changed `.ts` and `.tsx` file. Fix all errors introduced by the branch. Record unrelated pre-existing warnings rather than changing unrelated code.

- [ ] **Step 5: Run full frontend verification**

Run:

```bash
pnpm lint
pnpm check-types
pnpm test
```

Expected: all commands exit 0. Record warning count from lint and confirm no new branch-caused warning in changed files.

- [ ] **Step 6: Run Rust verification**

Run:

```bash
cd src-tauri
cargo clippy --all-targets --locked -- -D warnings
cargo nextest run --locked
```

If `cargo nextest` is unavailable, run `cargo test --locked` and record the substitution.

Expected: all commands exit 0.

- [ ] **Step 7: Perform manual macOS acceptance**

Verify:

1. one Space with two terminals at different runtime cwd values keeps one Explorer root
2. terminal, editor, preview, and Git tab switches keep the bottom root stable
3. Space A terminal to Space B file jump shows B root
4. changing root during a foreground TUI sends no terminal input
5. new terminals use the new root and existing terminals remain unchanged
6. external Open With or control-bridge file opens do not change root
7. nested repo and non-repo roots drive Source Control correctly
8. restart and invalid-root recovery follow the spec

Record Windows Local/WSL verification as required follow-up when no Windows environment is available.

- [ ] **Step 8: Commit final cleanup and documentation**

```bash
git add TERAX.md src
git commit -m "docs: record space cwd ownership"
```

- [ ] **Step 9: Run fresh-context review gates**

Dispatch read-only reviewers with distinct scopes:

- correctness and regressions, especially activation ordering and root ownership
- concurrency and Local/WSL environment routing
- UX and recovery behavior
- test quality and missing invariant coverage

Synthesize findings in the parent. Apply accepted fixes with one writer, rerun affected checks, and repeat focused review when fixes are substantial.

- [ ] **Step 10: Inspect final diff and publish the feature branch**

Run:

```bash
git status --short
git diff --check origin/main...HEAD
git log --oneline origin/main..HEAD
git push -u fork feat/space-owned-cwd
```

Expected: clean worktree, no whitespace errors, and successful push.

- [ ] **Step 11: Create the pull request**

Run:

```bash
gh pr create \
  --repo crynta/terax-ai \
  --base main \
  --head zihaozou:feat/space-owned-cwd \
  --title "Make working directory a Space property" \
  --body-file /tmp/terax-space-owned-cwd-pr.md
```

The PR body must summarize reviewer-relevant behavior, migration, tests, manual evidence, and any unverified Windows follow-up. Do not include private workflow constraints or chat-only history.

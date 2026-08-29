import { native } from "@/lib/native";
import { usePreferencesStore } from "@/modules/settings/preferences";
import type { Tab } from "@/modules/tabs";
import { DEFAULT_SPACE_ID } from "@/modules/tabs/lib/useTabs";
import { isLeaf, type PaneNode } from "@/modules/terminal/lib/panes";
import { parseWorkspaceScopeKey, type WorkspaceEnv } from "@/modules/workspace";
import { useEffect, useRef } from "react";
import { activeSpaceEnv } from "./activeSpace";
import { freshTerminalTab, hydrateTabs } from "./serialize";
import {
  hasUsableSpaceRoot,
  migrateSpaceRoots,
  SPACE_SCHEMA_VERSION,
  type SpaceRootIssues,
  validatePersistedSpaceRoots,
} from "./spaceRoot";
import {
  loadAll,
  type SpaceMeta,
  saveActiveId,
  saveSchemaVersion,
  saveSpacesList,
} from "./store";
import { useSpaces } from "./useSpaces";

type Params = {
  ready: boolean;
  launchCwd: string | null;
  home: string | null;
  allocId: () => number;
  replaceTabs: (tabs: Tab[], activeId: number) => void;
  markBooted: () => void;
  setActiveSpaceForNewTabs: (id: string) => void;
  adoptWorkspaceEnv: (env: WorkspaceEnv) => Promise<string | null>;
};

async function validateSpaceRoot(
  candidate: string,
  env: WorkspaceEnv,
): Promise<string> {
  const root = await native.canonicalize(candidate, env);
  await native.workspaceAuthorize(root, env);
  return root;
}

function uniqueCwds(tabs: Tab[]): string[] {
  const set = new Set<string>();
  const walk = (n: PaneNode) => {
    if (isLeaf(n)) {
      if (n.cwd) set.add(n.cwd);
      return;
    }
    for (const c of n.children) walk(c);
  };
  for (const t of tabs) if (t.kind === "terminal") walk(t.paneTree);
  return [...set];
}

export function useSpacesBoot({
  ready,
  launchCwd,
  home,
  allocId,
  replaceTabs,
  markBooted,
  setActiveSpaceForNewTabs,
  adoptWorkspaceEnv,
}: Params) {
  const done = useRef(false);

  useEffect(() => {
    if (!ready || done.current) return;
    done.current = true;

    void (async () => {
      try {
        const loaded = await loadAll();
        let { spaces, activeId, states } = loaded;

        if (spaces.length === 0) {
          await usePreferencesStore
            .getState()
            .init()
            .catch(() => {});
          const env = parseWorkspaceScopeKey(
            usePreferencesStore.getState().defaultWorkspaceEnv,
          );
          const envHome = await adoptWorkspaceEnv(env);
          const candidate = launchCwd ?? home ?? envHome;
          let root: string | null = null;
          let rootIssues: SpaceRootIssues = {};
          if (candidate) {
            try {
              root = await validateSpaceRoot(candidate, env);
            } catch (error) {
              rootIssues = {
                [DEFAULT_SPACE_ID]: {
                  candidate,
                  message:
                    error instanceof Error ? error.message : String(error),
                },
              };
            }
          } else {
            rootIssues = {
              [DEFAULT_SPACE_ID]: {
                candidate: null,
                message: "No directory is available",
              },
            };
          }
          const meta: SpaceMeta = {
            id: DEFAULT_SPACE_ID,
            name: "Default",
            root,
            env,
            createdAt: Date.now(),
            updatedAt: Date.now(),
          };
          await saveSpacesList([meta]);
          await Promise.all([
            saveActiveId(DEFAULT_SPACE_ID),
            saveSchemaVersion(SPACE_SCHEMA_VERSION),
          ]);
          setActiveSpaceForNewTabs(DEFAULT_SPACE_ID);
          useSpaces
            .getState()
            .hydrate([meta], DEFAULT_SPACE_ID, {}, rootIssues);
          return;
        }

        let rootIssues: SpaceRootIssues = {};
        if (loaded.schemaVersion < SPACE_SCHEMA_VERSION) {
          const migration = await migrateSpaceRoots(
            loaded,
            (env) => adoptWorkspaceEnv(env),
            validateSpaceRoot,
          );
          spaces = migration.spaces;
          rootIssues = migration.issues;
          if (spaces.some((space, index) => space !== loaded.spaces[index])) {
            await saveSpacesList(spaces);
          }
          await saveSchemaVersion(SPACE_SCHEMA_VERSION);
        } else {
          rootIssues = await validatePersistedSpaceRoots(
            spaces,
            validateSpaceRoot,
          );
        }

        const restored: Tab[] = [];
        for (const space of spaces) {
          const st = states.get(space.id);
          if (!st) continue;
          restored.push(...hydrateTabs(st.tabs, space.id, allocId));
        }

        const active =
          activeId && spaces.some((s) => s.id === activeId)
            ? activeId
            : spaces[0].id;
        setActiveSpaceForNewTabs(active);

        const env = activeSpaceEnv(spaces, active);
        await adoptWorkspaceEnv(env);

        const activeSpace = spaces.find((space) => space.id === active);
        if (
          hasUsableSpaceRoot(activeSpace, rootIssues) &&
          !restored.some((t) => t.spaceId === active)
        ) {
          restored.push(freshTerminalTab(active, activeSpace.root, allocId));
        }

        await Promise.allSettled(
          uniqueCwds(restored).map((cwd) => native.workspaceAuthorize(cwd)),
        );

        const initialActiveIndex: Record<string, number> = {};
        for (const [id, st] of states)
          initialActiveIndex[id] = st.activeTabIndex;
        useSpaces
          .getState()
          .hydrate(spaces, active, initialActiveIndex, rootIssues);

        const inActive = restored.filter((t) => t.spaceId === active);
        const idx = states.get(active)?.activeTabIndex ?? 0;
        const activeTab = inActive[idx] ?? inActive[0] ?? restored[0];
        replaceTabs(restored, activeTab.id);
      } catch (e) {
        console.error("[terax] spaces boot failed:", e);
      } finally {
        markBooted();
      }
    })();
  }, [
    ready,
    launchCwd,
    home,
    allocId,
    replaceTabs,
    markBooted,
    setActiveSpaceForNewTabs,
    adoptWorkspaceEnv,
  ]);
}

import { create } from "zustand";
import { usePreferencesStore } from "@/modules/settings/preferences";
import { parseWorkspaceScopeKey, type WorkspaceEnv } from "@/modules/workspace";
import type { SpaceRootIssues } from "@/modules/spaces/lib/spaceRoot";
import {
  deleteSpaceData,
  newSpaceId,
  saveActiveId,
  saveSpacesList,
  type SpaceMeta,
} from "./store";

type CreateInput = {
  id?: string;
  name: string;
  root: string;
  env?: WorkspaceEnv;
};

type State = {
  spaces: SpaceMeta[];
  activeId: string | null;
  hydrated: boolean;
  // Per-space active tab index loaded from disk, so persistence preserves it
  // for spaces the user never visits this session.
  initialActiveIndex: Record<string, number>;
  rootIssues: SpaceRootIssues;
  hydrate: (
    spaces: SpaceMeta[],
    activeId: string | null,
    initialActiveIndex?: Record<string, number>,
    rootIssues?: SpaceRootIssues,
  ) => void;
  create: (input: CreateInput) => SpaceMeta;
  rename: (id: string, name: string) => void;
  setRoot: (id: string, root: string) => void;
  setRootIssue: (id: string, issue: SpaceRootIssues[string]) => void;
  clearRootIssue: (id: string) => void;
  setColor: (id: string, color: number | undefined) => void;
  reorder: (orderedIds: string[]) => void;
  remove: (id: string) => string | null;
  setActive: (id: string) => void;
};

export const useSpaces = create<State>((set, get) => ({
  spaces: [],
  activeId: null,
  hydrated: false,
  initialActiveIndex: {},
  rootIssues: {},

  hydrate: (spaces, activeId, initialActiveIndex = {}, rootIssues = {}) => {
    set({ spaces, activeId, initialActiveIndex, rootIssues, hydrated: true });
  },

  create: (input) => {
    const now = Date.now();
    const meta: SpaceMeta = {
      id: input.id ?? newSpaceId(),
      name: input.name,
      root: input.root,
      env:
        input.env ??
        parseWorkspaceScopeKey(
          usePreferencesStore.getState().defaultWorkspaceEnv,
        ),
      createdAt: now,
      updatedAt: now,
    };
    const spaces = [...get().spaces, meta];
    set({ spaces });
    void saveSpacesList(spaces);
    return meta;
  },

  rename: (id, name) => {
    const spaces = get().spaces.map((s) =>
      s.id === id ? { ...s, name, updatedAt: Date.now() } : s,
    );
    set({ spaces });
    void saveSpacesList(spaces);
  },

  setRoot: (id, root) => {
    const spaces = get().spaces.map((s) =>
      s.id === id ? { ...s, root, updatedAt: Date.now() } : s,
    );
    const { [id]: _, ...rootIssues } = get().rootIssues;
    set({ spaces, rootIssues });
    void saveSpacesList(spaces);
  },

  setRootIssue: (id, issue) => {
    set({ rootIssues: { ...get().rootIssues, [id]: issue } });
  },

  clearRootIssue: (id) => {
    const { [id]: _, ...rootIssues } = get().rootIssues;
    set({ rootIssues });
  },

  setColor: (id, color) => {
    const spaces = get().spaces.map((s) =>
      s.id === id ? { ...s, color, updatedAt: Date.now() } : s,
    );
    set({ spaces });
    void saveSpacesList(spaces);
  },

  reorder: (orderedIds) => {
    const byId = new Map(get().spaces.map((s) => [s.id, s]));
    const next: SpaceMeta[] = [];
    for (const id of orderedIds) {
      const s = byId.get(id);
      if (s) next.push(s);
    }
    for (const s of get().spaces) {
      if (!next.includes(s)) next.push(s);
    }
    if (next.length !== get().spaces.length) return;
    set({ spaces: next });
    void saveSpacesList(next);
  },

  remove: (id) => {
    const prev = get();
    const spaces = prev.spaces.filter((s) => s.id !== id);
    let activeId = prev.activeId;
    if (activeId === id) activeId = spaces[0]?.id ?? null;
    const { [id]: _, ...rootIssues } = prev.rootIssues;
    set({ spaces, activeId, rootIssues });
    void saveSpacesList(spaces);
    void deleteSpaceData(id);
    if (activeId !== prev.activeId) void saveActiveId(activeId);
    return activeId;
  },

  setActive: (id) => {
    if (get().activeId === id) return;
    set({ activeId: id });
    void saveActiveId(id);
  },
}));

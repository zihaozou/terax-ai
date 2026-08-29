import { type RefObject, useCallback, useEffect, useState } from "react";
import { homeDir } from "@tauri-apps/api/path";
import { native } from "@/lib/native";
import type { PreparedWorkspace } from "@/modules/spaces";
import type { Tab } from "@/modules/tabs";
import {
  getWslHome,
  LOCAL_WORKSPACE,
  type WorkspaceEnv,
} from "@/modules/workspace";

async function resolveEnvHome(env: WorkspaceEnv): Promise<string> {
  return env.kind === "wsl"
    ? getWslHome(env.distro)
    : (await homeDir()).replace(/\\/g, "/");
}

type Params = {
  tabsRef: RefObject<Tab[]>;
  workspaceEnv: WorkspaceEnv;
  setWorkspaceEnv: (env: WorkspaceEnv) => void;
  resetWorkspace: (home?: string) => void;
  /** Dispose live sessions and clear App-owned pane/handle ref maps. */
  clearWorkspaceState: () => void;
};

export function useWorkspaceSwitcher({
  tabsRef,
  workspaceEnv,
  setWorkspaceEnv,
  resetWorkspace,
  clearWorkspaceState,
}: Params) {
  const [home, setHome] = useState<string | null>(null);
  const [launchCwd, setLaunchCwd] = useState<string | null>(null);
  const [launchCwdResolved, setLaunchCwdResolved] = useState(false);

  useEffect(() => {
    homeDir()
      .then(async (p) => {
        const normalized = p.replace(/\\/g, "/");
        setHome(normalized);
        try {
          await native.workspaceAuthorize(normalized);
        } catch {
          // Bootstrap already authorizes home from Rust; ignore.
        }
      })
      .catch(() => setHome(null));
  }, []);

  useEffect(() => {
    native
      .workspaceCurrentDir()
      .then(setLaunchCwd)
      .catch(() => setLaunchCwd(null))
      .finally(() => setLaunchCwdResolved(true));
  }, []);

  const prepareWorkspaceEnv = useCallback(async (env: WorkspaceEnv) => {
    const nextHome = await resolveEnvHome(env);
    await native.workspaceAuthorize(nextHome, env);
    return {
      env: env.kind === "local" ? LOCAL_WORKSPACE : env,
      home: nextHome,
    };
  }, []);

  const applyWorkspaceEnv = useCallback(
    (prepared: PreparedWorkspace) => {
      setWorkspaceEnv(prepared.env);
      setHome(prepared.home);
      setLaunchCwd(prepared.home);
    },
    [setWorkspaceEnv],
  );

  const switchWorkspace = useCallback(
    async (env: WorkspaceEnv): Promise<boolean> => {
      if (
        env.kind === workspaceEnv.kind &&
        (env.kind === "local" ||
          (workspaceEnv.kind === "wsl" && env.distro === workspaceEnv.distro))
      ) {
        return false;
      }
      const dirty = tabsRef.current.some((t) => t.kind === "editor" && t.dirty);
      if (dirty) {
        window.alert(
          "Save or close unsaved editor tabs before switching workspace.",
        );
        return false;
      }

      let prepared: PreparedWorkspace;
      try {
        prepared = await prepareWorkspaceEnv(env);
      } catch (error) {
        window.alert(String(error));
        return false;
      }

      clearWorkspaceState();
      applyWorkspaceEnv(prepared);
      resetWorkspace(prepared.home);
      return true;
    },
    [
      workspaceEnv,
      resetWorkspace,
      tabsRef,
      clearWorkspaceState,
      prepareWorkspaceEnv,
      applyWorkspaceEnv,
    ],
  );

  const adoptWorkspaceEnv = useCallback(
    async (env: WorkspaceEnv): Promise<string | null> => {
      try {
        const prepared = await prepareWorkspaceEnv(env);
        applyWorkspaceEnv(prepared);
        return prepared.home;
      } catch {
        return null;
      }
    },
    [prepareWorkspaceEnv, applyWorkspaceEnv],
  );

  return {
    home,
    launchCwd,
    launchCwdResolved,
    switchWorkspace,
    prepareWorkspaceEnv,
    applyWorkspaceEnv,
    adoptWorkspaceEnv,
  };
}

import { useCallback } from "react";
import { native } from "@/lib/native";
import type { SidebarViewId } from "@/modules/sidebar";
import type { SpaceRootIssue } from "@/modules/spaces/lib/spaceRoot";
import { sourceControlPathForSpace } from "./spaceRepository";
import { useSourceControl } from "./useSourceControl";

type Params = {
  spaceRoot: string | null;
  rootIssue: SpaceRootIssue | undefined;
  cycleSidebarView: (view: SidebarViewId) => void;
  openCommitHistoryTab: (args: {
    repoRoot: string;
    branch: string | null;
  }) => void;
};

export function useSourceControlContext({
  spaceRoot,
  rootIssue,
  cycleSidebarView,
  openCommitHistoryTab,
}: Params) {
  const sourceControlPath = sourceControlPathForSpace(spaceRoot, rootIssue);
  const sourceControl = useSourceControl(sourceControlPath, true);

  const toggleSourceControl = useCallback(() => {
    cycleSidebarView("source-control");
  }, [cycleSidebarView]);

  const openGitGraphFromContext = useCallback(async () => {
    const known =
      sourceControl.hasRepo && sourceControl.contextPath === sourceControlPath
        ? sourceControl.repo
        : null;
    if (known) {
      openCommitHistoryTab({
        repoRoot: known.repoRoot,
        branch: sourceControl.status?.branch ?? null,
      });
      return;
    }
    if (!sourceControlPath) return;
    try {
      const repo = await native.gitResolveRepo(sourceControlPath);
      if (!repo) return;
      openCommitHistoryTab({ repoRoot: repo.repoRoot, branch: repo.branch });
    } catch {
      /* noop */
    }
  }, [openCommitHistoryTab, sourceControl, sourceControlPath]);

  return { sourceControl, toggleSourceControl, openGitGraphFromContext };
}

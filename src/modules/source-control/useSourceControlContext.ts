import { native } from "@/lib/native";
import type { SidebarViewId } from "@/modules/sidebar";
import type { SpaceRootIssue } from "@/modules/spaces/lib/spaceRoot";
import { useCallback } from "react";
import { sourceControlPathForSpace } from "./spaceRepository";
import {
  type SourceControlSummary,
  useSourceControl,
} from "./useSourceControl";

type Params = {
  spaceRoot: string | null;
  rootIssue: SpaceRootIssue | undefined;
  cycleSidebarView: (view: SidebarViewId) => void;
  openCommitHistoryTab: (args: {
    repoRoot: string;
    branch: string | null;
  }) => void;
};

export function maskSourceControlSummaryForContext(
  summary: SourceControlSummary,
  requestedContextPath: string | null,
): SourceControlSummary {
  if (summary.contextPath === requestedContextPath) return summary;
  return {
    ...summary,
    contextPath: requestedContextPath,
    repo: null,
    status: null,
    changedCount: 0,
    upstream: null,
    ahead: 0,
    behind: 0,
    hasRepo: false,
    isLoading: requestedContextPath !== null,
    localError: null,
    busyAction: null,
    lastRemoteError: null,
    applyStatus: () => {},
    runRemoteAction: async () => ({
      ok: false,
      action: null,
      blocked: "no-repo",
    }),
  };
}

export function useSourceControlContext({
  spaceRoot,
  rootIssue,
  cycleSidebarView,
  openCommitHistoryTab,
}: Params) {
  const sourceControlPath = sourceControlPathForSpace(spaceRoot, rootIssue);
  const loadedSourceControl = useSourceControl(sourceControlPath, true);
  const sourceControl = maskSourceControlSummaryForContext(
    loadedSourceControl,
    sourceControlPath,
  );

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

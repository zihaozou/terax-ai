export type GitHistoryRequest = {
  id: number;
  spaceId: string;
  workspaceScope: string;
};

export type GitHistoryRequestGate = {
  begin: (spaceId: string, workspaceScope: string) => GitHistoryRequest;
  isCurrent: (
    request: GitHistoryRequest,
    activeSpaceId: string,
    activeWorkspaceScope: string,
  ) => boolean;
};

export function createGitHistoryRequestGate(): GitHistoryRequestGate {
  let latestId = 0;
  return {
    begin: (spaceId, workspaceScope) => ({
      id: ++latestId,
      spaceId,
      workspaceScope,
    }),
    isCurrent: (request, activeSpaceId, activeWorkspaceScope) =>
      request.id === latestId &&
      request.spaceId === activeSpaceId &&
      request.workspaceScope === activeWorkspaceScope,
  };
}

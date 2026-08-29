import type { SpaceRootIssues } from "@/modules/spaces/lib/spaceRoot";
import type { Tab } from "./useTabs";

export function canWarmTab(tab: Tab, rootIssues: SpaceRootIssues): boolean {
  return tab.kind !== "terminal" || !rootIssues[tab.spaceId];
}

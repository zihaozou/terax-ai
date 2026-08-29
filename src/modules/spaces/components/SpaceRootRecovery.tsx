import { Button } from "@/components/ui/button";
import type { SpaceRootIssue } from "@/modules/spaces/lib/spaceRoot";
import type { SpaceMeta } from "@/modules/spaces/lib/store";
import { Folder01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

type Props = {
  space: SpaceMeta;
  issue: SpaceRootIssue;
  onChooseFolder: () => void;
};

export function SpaceRootRecovery({ space, issue, onChooseFolder }: Props) {
  return (
    <section
      className="flex h-full min-h-0 flex-col items-start justify-center gap-4 px-5 py-6"
      aria-labelledby="space-root-recovery-title"
    >
      <div className="flex size-9 items-center justify-center rounded-md bg-destructive/10 text-destructive">
        <HugeiconsIcon icon={Folder01Icon} size={19} strokeWidth={1.75} />
      </div>
      <div className="space-y-1.5">
        <h2 id="space-root-recovery-title" className="text-sm font-medium">
          {space.name} needs a folder
        </h2>
        <p className="text-sm text-muted-foreground">
          This Space root is unavailable. Choose a folder to restore Explorer
          and new terminals.
        </p>
      </div>
      {issue.candidate ? (
        <p className="w-full truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
          {issue.candidate}
        </p>
      ) : null}
      <p className="text-xs text-destructive" role="status">
        {issue.message}
      </p>
      <Button onClick={onChooseFolder}>
        <HugeiconsIcon icon={Folder01Icon} size={16} strokeWidth={1.75} />
        Choose Folder...
      </Button>
    </section>
  );
}

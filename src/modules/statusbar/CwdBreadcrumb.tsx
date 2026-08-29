import { Badge } from "@/components/ui/badge";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { native } from "@/lib/native";
import { usePreferencesStore } from "@/modules/settings/preferences";
import type { SpaceRootIssue } from "@/modules/spaces/lib/spaceRoot";
import {
  ArrowDown01Icon,
  Folder01Icon,
  Home03Icon,
  MoreHorizontalIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { useCallback, useEffect, useState } from "react";
import { segmentsFromCwd } from "./lib/pathUtils";

type Props = {
  root: string | null;
  home: string | null;
  issue?: SpaceRootIssue;
  onChangeRoot: (path: string) => void;
  onChooseFolder: () => void;
};

export function SpaceRootBreadcrumb({
  root,
  home,
  issue,
  onChangeRoot,
  onChooseFolder,
}: Props) {
  if (issue) {
    return (
      <div className="flex min-w-0 items-center gap-2 text-xs">
        <span
          className="truncate text-destructive"
          title={issue.candidate ?? undefined}
        >
          Root unavailable{issue.candidate ? `: ${issue.candidate}` : ""}
        </span>
        <button
          type="button"
          className="shrink-0 rounded-sm px-1 text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          onClick={onChooseFolder}
        >
          Choose folder
        </button>
      </div>
    );
  }

  if (!root) {
    return (
      <button
        type="button"
        className="text-xs text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        onClick={onChooseFolder}
      >
        Choose folder
      </button>
    );
  }

  const segments = segmentsFromCwd(root, home);
  const current = segments[segments.length - 1];
  const parents = segments.slice(0, -1);
  const firstParent = parents[0];
  const middleParents = parents.slice(1);

  return (
    <div className="flex min-w-0 items-center gap-1">
      <Breadcrumb>
        <BreadcrumbList className="gap-1 text-xs sm:gap-1.5">
          {firstParent ? (
            <BreadcrumbSegment
              label={firstParent.label}
              isHome={firstParent.isHome}
              onClick={() => onChangeRoot(firstParent.fullPath)}
            />
          ) : null}
          {middleParents.length > 0 ? (
            <CollapsedSegments
              segments={middleParents}
              onChangeRoot={onChangeRoot}
            />
          ) : null}
          {middleParents.map((segment) => (
            <span key={segment.fullPath} className="contents max-md:hidden">
              <BreadcrumbSegment
                label={segment.label}
                isHome={segment.isHome}
                onClick={() => onChangeRoot(segment.fullPath)}
              />
            </span>
          ))}
          <BreadcrumbItem>
            <CurrentSegmentDropdown
              label={current.label}
              path={current.fullPath}
              onChangeRoot={onChangeRoot}
              onChooseFolder={onChooseFolder}
            />
          </BreadcrumbItem>
        </BreadcrumbList>
      </Breadcrumb>
      <button
        type="button"
        className="shrink-0 rounded-sm px-1 text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        onClick={onChooseFolder}
      >
        Choose folder
      </button>
    </div>
  );
}

function BreadcrumbSegment({
  label,
  isHome,
  onClick,
}: {
  label: string;
  isHome: boolean;
  onClick: () => void;
}) {
  return (
    <>
      <BreadcrumbItem>
        <BreadcrumbLink asChild>
          <button type="button" onClick={onClick} className="cursor-pointer">
            <Badge
              variant="outline"
              className="gap-1 text-muted-foreground hover:text-foreground"
            >
              {isHome ? (
                <HugeiconsIcon
                  icon={Home03Icon}
                  className="size-3"
                  strokeWidth={1.75}
                />
              ) : null}
              {isHome ? "Home" : label}
            </Badge>
          </button>
        </BreadcrumbLink>
      </BreadcrumbItem>
      <BreadcrumbSeparator className="[&>svg]:size-3" />
    </>
  );
}

function CurrentSegmentDropdown({
  label,
  path,
  onChangeRoot,
  onChooseFolder,
}: {
  label: string;
  path: string;
  onChangeRoot: (path: string) => void;
  onChooseFolder: () => void;
}) {
  const showHidden = usePreferencesStore((state) => state.showHidden);
  const [open, setOpen] = useState(false);
  const [children, setChildren] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setChildren(await native.listSubdirs(path, showHidden));
    } catch (reason) {
      setError(String(reason));
      setChildren([]);
    }
  }, [path, showHidden]);

  useEffect(() => {
    if (open) void load();
  }, [load, open]);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="flex cursor-pointer items-center gap-1 rounded-sm px-1 py-0.5 text-foreground hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1"
        >
          {label === "~" ? (
            <>
              <HugeiconsIcon
                icon={Home03Icon}
                className="size-3"
                strokeWidth={1.75}
              />
              Home
            </>
          ) : (
            label
          )}
          <HugeiconsIcon
            icon={ArrowDown01Icon}
            className="size-3 opacity-70"
            strokeWidth={2}
          />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="max-h-72 overflow-y-auto">
        {children === null ? (
          <div className="px-2 py-1.5 text-xs text-muted-foreground">
            Loading...
          </div>
        ) : children.length === 0 ? (
          <div className="px-2 py-1.5 text-xs text-muted-foreground">
            {error ?? "No subfolders"}
          </div>
        ) : (
          children.map((name) => (
            <DropdownMenuItem
              key={name}
              onSelect={() =>
                onChangeRoot(`${path.replace(/\/$/, "")}/${name}`)
              }
            >
              <HugeiconsIcon
                icon={Folder01Icon}
                className="size-3.5 text-muted-foreground"
                strokeWidth={1.75}
              />
              {name}
            </DropdownMenuItem>
          ))
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={onChooseFolder}>
          Choose Folder...
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function CollapsedSegments({
  segments,
  onChangeRoot,
}: {
  segments: { fullPath: string; label: string; isHome: boolean }[];
  onChangeRoot: (path: string) => void;
}) {
  return (
    <span className="contents md:hidden">
      <BreadcrumbItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              title="Show parent folders"
              className="flex items-center rounded-sm px-1 text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <HugeiconsIcon
                icon={MoreHorizontalIcon}
                className="size-3"
                strokeWidth={1.75}
              />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="min-w-44">
            {segments.map((segment) => (
              <DropdownMenuItem
                key={segment.fullPath}
                onSelect={() => onChangeRoot(segment.fullPath)}
              >
                <HugeiconsIcon
                  icon={segment.isHome ? Home03Icon : Folder01Icon}
                  className="size-3.5 text-muted-foreground"
                  strokeWidth={1.75}
                />
                <span className="truncate">
                  {segment.isHome ? "Home" : segment.label}
                </span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </BreadcrumbItem>
      <BreadcrumbSeparator className="[&>svg]:size-3" />
    </span>
  );
}

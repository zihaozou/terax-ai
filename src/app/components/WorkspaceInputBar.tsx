import { cn } from "@/lib/utils";
import { useBlockController } from "@/modules/terminal/lib/blockController";
import {
  CommandLineIcon,
  Folder01Icon,
  GitBranchIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { OsIcon } from "./OsIcon";
import { useGitBranch } from "./useGitBranch";
import { useSystemInfo } from "./useSystemInfo";

const ShellInput = lazy(() => import("@/modules/terminal/block/ShellInput"));

type ChipProps = {
  tone: "neutral" | "blue" | "violet" | "emerald";
  icon?: typeof CommandLineIcon;
  iconNode?: React.ReactNode;
  title?: string;
  children?: React.ReactNode;
};

function Chip({ tone, icon, iconNode, title, children }: ChipProps) {
  const toneClasses = {
    neutral: "bg-muted text-muted-foreground",
    blue: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    violet: "bg-violet-500/10 text-violet-600 dark:text-violet-400",
    emerald: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
  };
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10.5px] font-medium",
        toneClasses[tone],
      )}
      title={title}
    >
      {iconNode ??
        (icon && <HugeiconsIcon icon={icon} size={11} strokeWidth={1.75} />)}
      {children}
    </span>
  );
}

type Props = {
  isBlockTab: boolean;
  isTerminalTab: boolean;
  activeLeafId: number | null;
  cwd: string | null;
  home: string | null;
};

export function WorkspaceInputBar({
  isBlockTab,
  isTerminalTab,
  activeLeafId,
  cwd,
  home,
}: Props) {
  const { os, shell } = useSystemInfo();

  const controller = useBlockController(isBlockTab ? activeLeafId : null);
  const blockMode = controller?.blockMode ?? "prompt";

  // Re-resolve the branch chip when a command finishes (covers `git checkout`).
  const [promptNonce, setPromptNonce] = useState(0);
  const prevBlockMode = useRef(blockMode);
  useEffect(() => {
    if (prevBlockMode.current !== "prompt" && blockMode === "prompt") {
      setPromptNonce((n) => n + 1);
    }
    prevBlockMode.current = blockMode;
  }, [blockMode]);
  const branch = useGitBranch(isTerminalTab ? cwd : null, promptNonce);

  if (!isBlockTab) return null;

  const terminalChips = isTerminalTab ? (
    <>
      {os && <Chip tone="neutral" iconNode={<OsIcon os={os} />} title={os} />}
      {cwd && (
        <Chip tone="blue" icon={Folder01Icon} title={cwd}>
          {relPath(cwd, home)}
        </Chip>
      )}
      {branch && (
        <Chip tone="violet" icon={GitBranchIcon} title={`Branch: ${branch}`}>
          {branch}
        </Chip>
      )}
      {shell && (
        <Chip tone="emerald" icon={CommandLineIcon}>
          {shell}
        </Chip>
      )}
    </>
  ) : null;

  return (
    <div data-ai-input-bar data-state="open" className="terax-reveal">
      <div className="shrink-0 border-t border-border/60 bg-foreground/[0.02] px-3 py-2">
        <div className="flex flex-col gap-2 rounded-lg px-1 py-1">
          <div className="flex flex-wrap items-center gap-1.5">
            {terminalChips}
          </div>
          <div className="flex items-end gap-2.5">
            <div className="relative min-w-0 flex-1">
              {isBlockTab && controller && activeLeafId != null && (
                <Suspense fallback={null}>
                  <ShellInput
                    leafId={activeLeafId}
                    mode={blockMode}
                    focused={true}
                    onSubmit={controller.submitCommand}
                    onInterrupt={controller.interrupt}
                    getCwd={controller.getCwd}
                  />
                </Suspense>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function relPath(p: string, home: string | null): string {
  if (!home) return p;
  const h = home.replace(/\/+$/, "");
  if (p === h || p.startsWith(`${h}/`)) return `~${p.slice(h.length)}`;
  return p;
}

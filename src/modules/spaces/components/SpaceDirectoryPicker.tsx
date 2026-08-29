import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { native } from "@/lib/native";
import { usePreferencesStore } from "@/modules/settings/preferences";
import {
  joinDirectory,
  parentDirectory,
} from "@/modules/spaces/lib/directoryPicker";
import type { WorkspaceEnv } from "@/modules/workspace";
import { Folder01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { useCallback, useEffect, useRef, useState } from "react";

export type DirectoryPickerMode = "change-root" | "create-space";

type Props = {
  open: boolean;
  env: WorkspaceEnv;
  initialPath: string;
  mode: DirectoryPickerMode;
  onCancel: () => void;
  onSelect: (path: string) => void;
};

function envLabel(env: WorkspaceEnv): string {
  return env.kind === "wsl" ? `WSL: ${env.distro}` : "Local";
}

export function SpaceDirectoryPicker({
  open,
  env,
  initialPath,
  mode,
  onCancel,
  onSelect,
}: Props) {
  const showHidden = usePreferencesStore((state) => state.showHidden);
  const [path, setPath] = useState(initialPath);
  const [directories, setDirectories] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const load = useCallback(
    async (candidate: string) => {
      const id = ++requestId.current;
      setLoading(true);
      setError(null);
      try {
        const children = await native.listSubdirs(candidate, showHidden, env);
        if (id !== requestId.current) return;
        setDirectories(children);
      } catch (reason) {
        if (id !== requestId.current) return;
        setDirectories([]);
        setError(String(reason));
      } finally {
        if (id === requestId.current) setLoading(false);
      }
    },
    [env, showHidden],
  );

  useEffect(() => {
    if (!open) return;
    setPath(initialPath);
    setDirectories([]);
    setError(null);
    void load(initialPath);
    requestAnimationFrame(() => inputRef.current?.focus());
    return () => {
      requestId.current += 1;
    };
  }, [initialPath, load, open]);

  const goToPath = () => {
    const candidate = path.trim().replace(/\\/g, "/");
    if (!candidate) {
      setError("Enter a folder path.");
      return;
    }
    setPath(candidate);
    void load(candidate);
  };

  const choose = () => {
    const candidate = path.trim().replace(/\\/g, "/");
    if (!candidate) {
      setError("Enter a folder path.");
      return;
    }
    onSelect(candidate);
  };

  const title = mode === "change-root" ? "Change Space root" : "Create Space";
  const action = mode === "change-root" ? "Use as root" : "Create Space";

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onCancel()}>
      <DialogContent className="gap-4 sm:max-w-lg" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <HugeiconsIcon icon={Folder01Icon} size={17} strokeWidth={1.75} />
            {title}
          </DialogTitle>
          <DialogDescription>
            Choose a folder in {envLabel(env)}. The folder is validated before
            it is used.
          </DialogDescription>
        </DialogHeader>
        <div className="flex gap-2">
          <Input
            ref={inputRef}
            aria-label="Folder path"
            value={path}
            onChange={(event) => {
              setPath(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                goToPath();
              }
            }}
            placeholder="/path/to/folder"
          />
          <Button variant="outline" onClick={goToPath} disabled={loading}>
            Go
          </Button>
        </div>
        <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span className="truncate" title={path}>
            {path || "No folder selected"}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              const parent = parentDirectory(path);
              setPath(parent);
              void load(parent);
            }}
            disabled={!path || parentDirectory(path) === path || loading}
          >
            Up
          </Button>
        </div>
        <div
          className="max-h-64 overflow-y-auto rounded-md border"
          aria-live="polite"
        >
          {loading ? (
            <p className="px-3 py-4 text-xs text-muted-foreground">
              Loading folders...
            </p>
          ) : error ? (
            <p className="px-3 py-4 text-xs text-destructive">{error}</p>
          ) : directories.length === 0 ? (
            <p className="px-3 py-4 text-xs text-muted-foreground">
              No subfolders
            </p>
          ) : (
            <ul className="p-1" aria-label="Subfolders">
              {directories.map((directory) => (
                <li key={directory}>
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    onClick={() => {
                      const child = joinDirectory(path, directory);
                      setPath(child);
                      void load(child);
                    }}
                  >
                    <HugeiconsIcon
                      icon={Folder01Icon}
                      size={15}
                      strokeWidth={1.75}
                    />
                    <span className="truncate">{directory}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={choose} disabled={loading || !path.trim()}>
            {action}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

# Space-Owned Working Directory Design

Date: 2026-08-28
Status: Approved for implementation planning

## Summary

Terax currently derives the Explorer root and new-terminal working directory from the active terminal tab. `useWorkspaceCwd` prefers the active terminal cwd, then a render-local last-terminal ref, then any terminal cwd, and finally the launch directory or home. This makes the visible workspace context depend on tab activation order and OSC 7 timing. File and non-terminal tabs do not carry a cwd, so cross-space navigation can leave the Explorer showing a directory from a previously active terminal.

This design makes the working directory an intrinsic property of a Space. `SpaceMeta.root` and `SpaceMeta.env` become the only authoritative workspace-level context. Terminal and pane cwd values remain terminal-local runtime state. No tab event, pane event, file open, or OSC 7 update may modify a Space root.

## Goals

- Give every usable Space one explicit, persisted working directory.
- Make Space root independent of active tab type and terminal runtime cwd.
- Make the bottom status bar the only UI that changes the current Space root.
- Make Explorer, Source Control, workspace search, window context, and new terminals consume the active Space root.
- Keep existing terminals unchanged when the Space root changes.
- Make Local and WSL environment identity fixed per Space.
- Replace folder context actions with `Open in New Space` where appropriate.
- Provide a deterministic migration and explicit recovery path for invalid legacy roots.
- Preserve the ability to open files outside the Space root.

## Non-goals

- Synchronizing terminal cwd values within a Space.
- Sending `cd` to an existing terminal when a Space root changes.
- Restricting editor tabs to files under the Space root.
- Requiring Space roots to be unique.
- Changing explicit repository roots stored by Git history or diff content tabs.
- Rewriting every native filesystem API to remove the active workspace adapter.
- Changing shell OSC 7 tracking for terminal-local behavior.

## Current Architecture and Failure Mode

`SpaceMeta` already stores `root` and `env`, but `root` is used primarily when creating a Space, creating a fallback terminal, or deleting a Space. Switching tabs and updating terminal cwd do not use it as the workspace source of truth.

`useWorkspaceCwd` derives Explorer state from terminal tabs and keeps a single last-terminal ref that is not scoped by Space. `App.tsx` passes the derived value to `FileExplorer`, window-title logic, new-tab inheritance, and Source Control context. The bottom `CwdBreadcrumb` changes meaning by tab type and sends a shell `cd` command when the user selects a path. Source Control also supports a separate per-Space fixed repository target created by `Open in Source Control`.

The result is multiple competing path states:

- persisted `SpaceMeta.root`
- active terminal tab cwd
- active terminal pane cwd
- render-local last terminal cwd
- active file path
- Source Control fixed repository target
- launch directory or home fallback

The behavior cannot be made consistent by adding more switch effects. The model must have one workspace-level owner.

## Domain Model

`SpaceMeta` remains the persisted Space entity:

```ts
type SpaceMeta = {
  id: string;
  name: string;
  root: string | null;
  env: WorkspaceEnv;
  color?: number;
  createdAt: number;
  updatedAt: number;
};
```

`root: null` is permitted only for legacy migration or explicit recovery state. A usable Space must have a normalized, authorized root.

`SpaceState` continues to own persisted tabs and the active tab index. Space identity and root do not move into `SpaceState` because tab persistence is debounced and should not control workspace identity.

Terminal cwd remains on terminal tabs and pane leaves. It describes the real shell process and continues to update from spawn state and OSC 7.

## Invariants

1. `SpaceMeta.root` is the only workspace-level working directory.
2. `SpaceMeta.env` is fixed for the lifetime of a Space.
3. Tab activation, pane focus, OSC 7, file opens, tab movement, and tab restoration cannot write `SpaceMeta.root`.
4. Changing `SpaceMeta.root` cannot write terminal or pane cwd and cannot send terminal input.
5. Every new terminal receives the root of its owning Space explicitly.
6. Explorer root always equals the active Space root.
7. Source Control context always resolves from the active Space root.
8. Files outside the Space root may open in the current Space without changing the root.
9. Space activation must apply the target environment before exposing the target root to workspace consumers.
10. Invalid roots never silently fall back to home.

## Space State API

`useSpaces` becomes the sole mutation boundary for persisted Space context.

The store exposes pure synchronous mutations after validation has succeeded:

- `create({ name, root, env })`
- `setRoot(spaceId, root)`
- existing rename, color, reorder, remove, and active-state operations

The existing in-place `setEnv` behavior is removed. Choosing another environment creates a new Space after an explicit folder selection.

OS-facing work lives in a thin controller or hook within `modules/spaces`:

- canonicalize a candidate root for an explicit `WorkspaceEnv`
- verify that it exists and is a directory
- authorize it
- call the pure store mutation only after validation succeeds
- create the initial terminal with the validated root
- coordinate environment adoption and Space activation

This keeps native IPC out of the Zustand store and keeps validation logic independently testable.

## Active Space Read Path

`useWorkspaceCwd` is removed. A small `useActiveSpace` selector provides the active Space metadata and validation state.

The following consumers read `activeSpace.root` directly:

- `FileExplorer.rootPath`
- Source Control context
- window workspace context
- new terminal creation
- workspace search and glob defaults
- bottom status bar

Active file path remains editor and reveal state only. Active terminal leaf cwd remains terminal-local state only.

`App.tsx` remains a coordinator. It does not contain fallback or inference rules for Space root.

## Space Activation

Space activation is asynchronous because Local and WSL use different native workspace environments. The controller serializes activation requests.

For a target Space:

1. Resolve the target Space and desired tab.
2. If the target env differs, adopt the target env and resolve its home.
3. Discard an obsolete target if a newer activation was requested.
4. Commit the active Space and active tab only when the active native environment matches the target.

Rapid Space selection uses latest-request-wins semantics without allowing overlapping environment adoption to leave the global adapter in a stale environment. Same-env Space switches skip redundant environment IPC.

Cross-Space tab jumps and active-Space deletion use this same controller. They do not set active tab and active Space independently.

## Root Changes

The bottom bar is the only UI that changes the current Space root.

A root-change request captures `{spaceId, env, requestId}`. The candidate is validated without optimistic store mutation. A newer request invalidates the older one. If the active Space or environment changes during validation, the stale result is discarded rather than applied to the wrong Space.

On success, one store mutation updates and persists the normalized root. Existing terminals and terminal tabs remain unchanged. Explorer and Source Control refresh from the new root. New terminals use the new root.

## Bottom Bar

`CwdBreadcrumb` is narrowed to a stable Space-root component. It no longer receives an active file path and no longer sends `cd` to a terminal.

The component:

- always displays the current Space root
- changes the Space root when an ancestor or listed subdirectory is selected
- includes `Choose Folder...` for arbitrary directory selection
- shows an explicit recovery state when the root is unavailable

On Windows, the environment control displays the current Space env. Selecting another Local or WSL environment opens folder selection for that environment and creates a new Space. It never mutates the current Space environment.

Folder selection is workspace-aware and accepts an explicit `WorkspaceEnv`. It uses existing directory listing, canonicalization, and authorization IPC with the supplied environment rather than relying on whichever Space is currently active. This gives Local and WSL the same model.

## Explorer Actions

Folder context menus in the main tree, root or background menu, and Explorer search results are updated consistently.

Remove:

- `Open in Terminal`
- `Open in Source Control`

Add:

- `Open in New Space`

Keep:

- `Open Git History`

`Open in New Space` validates the selected folder, inherits the current Space env, names the new Space from the folder basename, creates one initial terminal rooted there, and activates the new Space. The action calls the same Space controller used by generic Space creation.

Generic `New Space` in SpaceSwitcher and Command Palette first asks for an environment when needed and requires an explicit folder selection. It never reads active terminal cwd.

Multiple Spaces may use the same root because Space identity is the persisted id, not the path.

## Source Control

The Source Control sidebar resolves only the repository containing the active Space root.

The per-Space fixed repository target and `Open in Source Control` flow are removed. Clicking the Git activity item is sufficient to open Source Control for the current Space.

Explicit Git history, diff, and commit-file tabs continue to store their own `repoRoot` because that value describes tab content rather than workspace context.

## Migration

The spaces store gains an explicit schema version.

For each legacy Space:

1. Use a non-empty `SpaceMeta.root` as the first candidate.
2. If root is null, use the active persisted terminal leaf cwd when available.
3. Otherwise use the first persisted terminal leaf cwd.
4. Otherwise use the home directory for the Space env.
5. Canonicalize and authorize the candidate with the Space env.
6. Persist the canonical root on success.
7. Preserve the candidate for display and mark the Space unavailable on failure.

Migration results and the new schema version are written once. Later boots never infer root from terminal cwd.

## Unavailable Root Recovery

An unavailable root does not silently become home.

While unavailable:

- Explorer shows the original path and a folder recovery action.
- the bottom bar shows the same recovery action
- Source Control does not issue repository requests
- cold terminals for that Space do not spawn
- new terminal creation is disabled
- editor and file tabs remain usable, including files outside the root

After the user selects a valid directory, the normal root-change transaction restores the Space. Existing running terminals remain unchanged.

## Terminal Events and Environment Authorization

OSC 7 continues to update terminal tab and pane cwd.

Authorization for a terminal cwd must use the environment of the terminal's owning Space, resolved through `leafId -> tab -> spaceId -> SpaceMeta.env`. It must not use the currently active Space environment. This prevents a background WSL terminal and a foreground Local Space from crossing authorization contexts.

The native wrapper may accept an explicit env for this authorization path while preserving the current active-env default for existing callers.

## Error Handling

- Folder selection cancellation makes no state change.
- Canonicalization, directory validation, and authorization errors surface a concise toast or recovery message and preserve the previous root.
- Space activation errors preserve the previously active Space and tab.
- Obsolete async results are discarded using request identity.
- Source Control treats unavailable Space root as no context rather than falling back.
- Creation is atomic from the user's perspective. A Space is not persisted until its root has validated.

## Performance

- No dependency is added.
- Active root uses a narrow Zustand selector.
- Ordinary tab and pane switching performs no filesystem IPC.
- Same-env Space switching performs no environment IPC.
- A root change causes one Explorer root reset and one Source Control context refresh.
- No timer, watcher, or cwd synchronization broadcast is introduced.
- Existing terminal OSC processing remains terminal-local.

## Test Strategy

### Pure and Store Tests

- schema migration with an existing root
- migration from active persisted terminal cwd
- migration from first terminal cwd
- migration fallback to env home
- invalid candidate recovery state
- create and setRoot validation boundaries
- failed validation leaves store unchanged
- terminal OSC updates tab or leaf cwd but not Space root
- new terminal uses owning Space root
- external file open leaves Space root unchanged
- move, delete, and cross-Space jump preserve every Space root
- Source Control context derives only from Space root

### Activation and Concurrency Tests

- target env is applied before active Space is committed
- rapid requests produce the final requested Space and env
- same-env activation avoids redundant IPC
- stale root validation cannot update another Space
- inactive terminal cwd authorization uses the owning Space env

### UI and Flow Tests

- different terminal cwd values within one Space do not change Explorer or bottom bar
- terminal, editor, preview, and Git tab switches do not change Explorer root
- a jump from a Space A terminal to a Space B file shows Space B root
- bottom-bar root change updates Explorer, Source Control, and new terminal cwd only
- existing terminal session state is unchanged by root change
- all folder menu locations remove the two old actions and add `Open in New Space`
- generic New Space requires explicit folder selection
- Local and WSL creation handle cancel, failure, and rapid repeated input
- unavailable root recovery restores the Space without fallback
- Git History behavior remains unchanged

### Verification Gates

- `pnpm lint`
- `pnpm check-types`
- `pnpm test`
- changed-file LSP diagnostics
- `cd src-tauri && cargo clippy --all-targets --locked -- -D warnings`
- `cd src-tauri && cargo nextest run --locked`, or `cargo test --locked` when nextest is unavailable
- fresh-context correctness, concurrency, UX, and test-quality review
- final parent diff inspection

Manual verification covers macOS with multiple Spaces, terminal cwd divergence, editor cross-Space jumps, root changes during a foreground TUI, external Open With or CLI file opens, nested Git repositories, non-repository roots, restart, migration, and recovery. Windows verification additionally covers Local and WSL Space switching plus background terminal OSC activity.

## Delivery

Development occurs in an isolated worktree on branch `feat/space-owned-cwd`, based on `f740f37` after the current main branch was pushed to `fork/main`.

The implementation will be split into reviewable milestones in the implementation plan. After implementation, full verification and fresh-context review must pass before the feature branch is pushed and a pull request is opened.

# Task 10 Report: Space-owned CWD Cleanup

## Commit

Original Task 10 source and documentation commit: `a272cc5 docs: record space cwd ownership`. The approved Home-first refinement is recorded by subsequent commits on `feat/space-owned-cwd`.

## Search evidence

The required runtime legacy search returned no matches:

```text
rg -n "useWorkspaceCwd|lastTerminalCwd|onRevealInTerminal|onOpenInSourceControl|Open in Terminal|Open in Source Control|repositoryTarget|setEnv\\(" src
```

A broader ownership search found only active-Space root consumers and terminal-local cwd handling. It found no global workspace cwd setter, legacy root inference, Explorer open-in-terminal action, Source Control fixed-target routing, or obsolete environment mutation.

## Changes

- Removed unused `freshTabCwd` fallback plumbing from `activeSpace.ts`.
- Added a cross-Space invariant test: a Space A terminal at `/a/deep` opens an external file, then activates a Space B editor tab; both Space roots remain `/a` and `/b`, `setRoot` is not called, and B becomes active.
- Updated `TERAX.md` to document Space root and environment authority, runtime-only terminal cwd and OSC 7 behavior, the bottom-bar root mutation boundary, Explorer and Source Control root consumption, unavailable-root recovery, Local and WSL creation, authorization, and deletion fallback.

### Approved UX refinement

- Generic `New Space` now creates a collision-free `Space N` at environment Home without a system directory picker.
- First boot removes launch cwd from the boot API and seeds both Space root and the initial cold terminal cwd from environment Home.
- An unavailable persisted root automatically repairs to same-environment Home; failed WSL Home resolution never substitutes Local Home.
- Root issues pass `null` to all workspace consumers. Failed active-environment adoption keeps persisted tabs unmounted, shows one rootless cold fallback, and disables tab-state persistence for that session.
- Restored terminal cwd authorization carries the owning Space environment for every path.
- The bottom bar renders the complete root path plus a trailing environment-scoped child-directory `...`. Long paths use horizontal wheel scrolling and auto-scroll to the current root.
- UNC roots retain `//server/share`, canonical Unix and WSL backslashes remain filename characters, and breadcrumb list items keep valid direct-list semantics.
- StatusBar again receives the active editor or markdown path for LSP and diagnostic indicators.
- Removed the Space directory picker, picker request gate, and explicit recovery panel.
- Added focused tests for Home creation and boot, unavailable-root repair, WSL isolation, restored cwd authorization, naming, canonical path handling, wheel translation, end scrolling, and stale directory-list rejection.

## Validation

| Command | Result | Evidence |
| --- | --- | --- |
| `pnpm test` | passed | 100 test files, 618 tests passed |
| `pnpm check-types` | passed | `tsc --noEmit` exited 0 |
| `pnpm lint` | passed with pre-existing warnings | exited 0 with 65 warnings, none in changed source files |
| `pnpm build` | passed | `tsc && vite build` completed in 4.66 seconds |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | passed | exited 0 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | passed, supplementary | dev profile completed successfully; not a substitute for the planned Rust gates |
| `cargo test --manifest-path src-tauri/Cargo.toml` | passed, supplementary | 312 Rust tests passed across unit and integration suites; not a substitute for the planned Rust gates |
| `git diff --check` | passed | no whitespace errors before commit or in the final worktree |
| `git diff --check origin/main...HEAD` | failed, pre-existing outside Task 10 | trailing whitespace in pre-existing `sidecar/vendor/anemll-swift-cli` branch files; `git show --check a272cc5` passes |
| changed-source Biome lint | passed | all 16 modified TypeScript/TSX files checked with no diagnostics |
| changed-source LSP diagnostics | passed | no primary TypeScript diagnostics in the modified source set |

`pnpm lint` warnings are pre-existing in unrelated markdown, editor, and settings files. The full type check, changed-file Biome lint, and changed-file LSP diagnostics found no introduced diagnostics.

## Required Rust gate follow-up

| Command | Result | Evidence |
| --- | --- | --- |
| `cd src-tauri && cargo clippy --all-targets --locked -- -D warnings` | passed | completed with exit 0 and no warnings |
| `cd src-tauri && cargo nextest run --locked` | passed | 312 tests across 7 binaries: 312 passed, 0 skipped |

`cargo nextest` was installed, so no fallback command was needed. No branch-introduced Clippy findings required changes.

## Manual acceptance

The frozen final build passed all planned macOS scenarios:

1. Generic `New Space` created a collision-free `Space N` at Home without a system picker; trailing `...` changed Space root while the existing terminal cwd stayed unchanged.
2. A long breadcrumb auto-scrolled to the current root, translated wheel input horizontally, and remained usable with LSP, diagnostics, and Private status at narrow widths.
3. Changing root during a foreground TUI left the TUI and existing terminal cwd unchanged; a new terminal started at the new root.
4. The trailing `...` supported Tab, Enter, arrow, and Escape operation; rapid close and ancestor changes showed no stale directory list.
5. Space A to Space B file jumps and terminal, editor, preview, and Git tab switches kept bottom bar, Explorer, and Source Control on the active Space root.
6. Opening and closing a file outside the Space root did not change any workspace consumer.
7. Source Control cleared context for a non-repository root and resolved the containing repository from a repository subdirectory without an independent target.
8. After deleting the persisted root directory and restarting, the Space automatically recovered to Home, no terminal started in the deleted path, and persisted tabs restored.

Windows Local and WSL Space creation, switching, UNC interaction, and background terminal OSC activity remain untested because the acceptance environment is macOS.

## Files

The approved refinement modifies:

- `src/app/App.tsx`
- `src/modules/spaces/index.ts`
- `src/modules/spaces/lib/activeSpace.ts`
- `src/modules/spaces/lib/activeSpace.test.ts`
- `src/modules/spaces/lib/rootValidation.ts`
- `src/modules/spaces/lib/rootValidation.test.ts`
- `src/modules/spaces/lib/spaceController.ts`
- `src/modules/spaces/lib/spaceController.test.ts`
- `src/modules/spaces/lib/useSpaces.ts`
- `src/modules/spaces/lib/useSpaces.test.ts`
- `src/modules/spaces/lib/useSpacesBoot.ts`
- `src/modules/spaces/lib/useSpacesBoot.test.ts`
- `src/modules/statusbar/CwdBreadcrumb.tsx`
- `src/modules/statusbar/StatusBar.tsx`
- `src/modules/statusbar/lib/pathUtils.ts`
- `src/modules/statusbar/lib/pathUtils.test.ts`
- `docs/superpowers/specs/2026-08-28-space-owned-cwd-design.md`
- `docs/superpowers/plans/2026-08-28-space-owned-cwd.md`
- `TERAX.md`
- `.superpowers/sdd/2026-08-28-space-owned-cwd/task-10-report.md`

It removes:

- `src/modules/spaces/components/SpaceDirectoryPicker.tsx`
- `src/modules/spaces/components/SpaceRootRecovery.tsx`
- `src/modules/spaces/lib/directoryPicker.ts`
- `src/modules/spaces/lib/directoryPicker.test.ts`

## Concerns

- Fresh-context lifecycle and breadcrumb re-reviews approved the resolved boot, environment, unavailable-root, persistence, and path fixes.
- The repository retains 65 unrelated lint warnings.
- The full branch range has pre-existing vendor trailing whitespace outside Task 10; Task 10 commits pass their own whitespace checks.
- Windows Local, WSL, UNC, and background OSC validation was unavailable in the macOS acceptance environment.

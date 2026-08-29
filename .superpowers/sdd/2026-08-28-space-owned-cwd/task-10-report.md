# Task 10 Report: Space-owned CWD Cleanup

## Commit

Task 10 source and documentation commit: `a272cc5 docs: record space cwd ownership`.

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

## Validation

| Command | Result | Evidence |
| --- | --- | --- |
| `pnpm test` | passed | 101 test files, 607 tests passed |
| `pnpm check-types` | passed | `tsc --noEmit` exited 0 |
| `pnpm lint` | passed with pre-existing warnings | exited 0 with 70 warnings, none in Task 10 changed source files |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | passed | exited 0 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | passed | dev profile completed successfully |
| `cargo test --manifest-path src-tauri/Cargo.toml` | passed | 312 Rust tests passed across unit and integration suites |
| `git diff --check` | passed | no whitespace errors before commit or in the final worktree |
| `git diff --check origin/main...HEAD` | failed, pre-existing outside Task 10 | trailing whitespace in pre-existing `sidecar/vendor/anemll-swift-cli` branch files; `git show --check a272cc5` passes |
| `pnpm exec biome lint src/modules/spaces/lib/activeSpace.ts src/modules/spaces/lib/spaceController.test.ts` | passed | 2 changed TypeScript files checked with no diagnostics |

`pnpm lint` warnings are pre-existing in unrelated markdown, editor, and settings files. The full type check and changed-file Biome diagnostics found no introduced diagnostics. Primary LSP diagnostics were not available in this non-desktop worker environment.

## Manual limitations

No desktop acceptance was run. Untested scenarios are macOS multi-terminal runtime cwd divergence, tab switches across terminal/editor/preview/Git, Space A to B file jumps, root changes during a foreground TUI, new versus existing terminal cwd after root change, external file opens, nested and non-repository Source Control roots, restart, and invalid-root recovery. Windows Local and WSL Space creation, switching, and background terminal OSC activity also remain untested.

## Files

- `TERAX.md`
- `src/modules/spaces/lib/activeSpace.ts`
- `src/modules/spaces/lib/spaceController.test.ts`
- `.superpowers/sdd/2026-08-28-space-owned-cwd/task-10-report.md`

## Concerns

- No introduced blockers found during self-review.
- The repository retains 70 unrelated lint warnings.
- The full branch range has pre-existing vendor trailing whitespace outside Task 10; Task 10 commits pass their own whitespace checks.
- No manual desktop or Windows validation was available.

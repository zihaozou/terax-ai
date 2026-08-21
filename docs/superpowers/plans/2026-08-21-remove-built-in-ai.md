# Remove Built-in AI Subsystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Terax's built-in AI subsystem (chat, providers, tools, autocomplete, AI commit generation, settings UI) while keeping the terminal coding-agent integration and generic infrastructure reused by non-AI features.

**Architecture:** Deletion + decoupling, executed as sequential tasks on one branch. Two shared files (`native.ts` IPC facade, `markdown-code.tsx` renderer) are relocated out of AI directories first so the mass deletion doesn't break non-AI consumers; then frontend AI code is deleted and mount points unwired; then AI-adjacent features, Rust AI modules, dependencies, tests, and docs are removed. Verification gates run at the end of every task.

**Tech Stack:** React/TypeScript frontend (Vite, Zustand, Vercel AI SDK being removed), Tauri/Rust backend, pnpm, vitest, biome, cargo.

**Spec:** `docs/superpowers/specs/2026-08-21-remove-built-in-ai-design.md`

## Global Constraints

- Work on branch `chore/remove-built-in-ai`. Tasks run **sequentially** in one working tree (heavy file overlap on `App.tsx`, settings, shortcuts — no parallel writers). One commit per task.
- **Keep:** `src/modules/agents/` (detection, notification bell, ⇧⌘A jump, new-tab launcher), `src-tauri/src/modules/agent.rs`, `src-tauri/src/modules/pty/agent_detect.rs`, the `agent.focusAttention` shortcut, the `streamdown` npm dep, and the `TERAX.md` file itself.
- cargo lives at `/opt/homebrew/opt/rustup/bin` — every cargo step must start with `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
- `package.json` currently carries an unrelated pre-existing local modification (pnpm `allowScripts` entries). Do not revert it; when editing dependencies, leave those entries intact.
- `pnpm test` runs vitest; for a single file use `pnpm vitest run <path>`.
- Docs/README edits are public-facing: write for a reviewer who has no context on this session (no internal workflow jargon).
- Verification gates: `pnpm check-types`, `pnpm lint`, `pnpm test`, `pnpm build`; Rust: `cargo test`, `cargo clippy` (in `src-tauri`).

---

### Task 1: Relocate `native.ts` IPC facade out of the AI module

**Files:**

- Move: `src/modules/ai/lib/native.ts` → `src/lib/native.ts`
- Modify (all known importers): `src/app/App.tsx`, `src/app/hooks/useWorkspaceSwitcher.ts`, `src/app/components/useGitBranch.ts`, `src/modules/spaces/lib/useSpacesBoot.ts`, `src/modules/explorer/FileExplorer.tsx`, `src/modules/explorer/lib/useGitStatus.ts`, `src/modules/explorer/lib/gitStatusUtils.ts`, `src/modules/source-control/` (4 files), `src/modules/git-history/`, `src/modules/editor/lib/diffCache.ts`, plus any importers inside `src/modules/ai/` itself.

**Interfaces:**

- Consumes: nothing from earlier tasks (first task).
- Produces: module `@/lib/native` exporting the same `native` object and types as before (export names unchanged). Later tasks and all non-AI modules import from `@/lib/native`.

- [ ] **Step 1: Enumerate every importer**

Run: `grep -rl "ai/lib/native" src/ | sort`
Expected: a list covering the files above (plus possibly AI-internal files). Save this list — every entry must be rewritten.

- [ ] **Step 2: Move the file with history preserved**

```bash
mkdir -p src/lib && git mv src/modules/ai/lib/native.ts src/lib/native.ts
```

- [ ] **Step 3: Rewrite all import specifiers**

For each file from Step 1, replace the import source with `@/lib/native`. This covers both alias form (`@/modules/ai/lib/native`) and relative forms inside the AI module (`./native`, `../lib/native`, `../../lib/native`). Verify none remain:

Run: `grep -rn "ai/lib/native" src/`
Expected: no output.

- [ ] **Step 4: Type-check**

Run: `pnpm check-types`
Expected: PASS (no new errors).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: relocate native IPC facade to src/lib/native"
```

---

### Task 2: Relocate the shared markdown code renderer out of `ai-elements`

**Files:**

- Move: `src/components/ai-elements/markdown-code.tsx` → `src/components/markdown/markdown-code.tsx`
- Move (only if Step 1 shows a dependency): `src/components/ai-elements/chat-code-lezer.ts` → `src/components/markdown/chat-code-lezer.ts`
- Move: `src/components/ai-elements/markdown-code.test.tsx` → `src/components/markdown/markdown-code.test.tsx`
- Modify: `src/modules/markdown/MarkdownPreviewPane.tsx` and any other importers (`src/components/ai-elements/message.tsx`, possibly `chat-code.tsx`).

**Interfaces:**

- Consumes: nothing from Task 1.
- Produces: `src/components/markdown/markdown-code.tsx` exporting the same component(s) as before (export names unchanged). `MarkdownPreviewPane` imports it from `@/components/markdown/markdown-code`.

- [ ] **Step 1: Check the lezer dependency and enumerate importers**

Run: `grep -n "chat-code-lezer" src/components/ai-elements/markdown-code.tsx`
Expected: import line(s) if dependent (then lezer moves too), or no output.
Run: `grep -rln "markdown-code" src/ | grep -v test | sort`
Expected: importers list (at minimum `src/modules/markdown/MarkdownPreviewPane.tsx`, `src/components/ai-elements/message.tsx`).

- [ ] **Step 2: Move the files**

```bash
mkdir -p src/components/markdown
git mv src/components/ai-elements/markdown-code.tsx src/components/markdown/
git mv src/components/ai-elements/markdown-code.test.tsx src/components/markdown/
# only if Step 1 showed a dependency:
git mv src/components/ai-elements/chat-code-lezer.ts src/components/markdown/
```

- [ ] **Step 3: Rewrite imports**

Update every importer from Step 1 to `@/components/markdown/markdown-code`; fix relative imports inside the moved files (e.g. `./chat-code-lezer` stays valid if both moved together). Verify:

Run: `grep -rn "ai-elements/markdown-code\|ai-elements/chat-code-lezer" src/`
Expected: no output.

- [ ] **Step 4: Run the moved test and type-check**

Run: `pnpm vitest run src/components/markdown/markdown-code.test.tsx`
Expected: PASS.
Run: `pnpm check-types`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: relocate shared markdown code renderer to src/components/markdown"
```

---

### Task 3: Delete the frontend AI core and unwire all mount points

**Files:**

- Delete: `src/modules/ai/` (entire directory; `native.ts` already moved out), `src/components/ai-elements/` (remaining files: `conversation.tsx`, `message.tsx`, `chat-code.tsx`, `context.tsx`, `reasoning.tsx`, `snippet.tsx`, `tool.tsx`, `shimmer.tsx`, and `chat-code-lezer.ts` if it didn't move)
- Modify: `src/app/App.tsx`, `src/modules/shortcuts/shortcuts.ts`, `src/modules/command-palette/commands.ts`, `src/modules/statusbar/StatusBar.tsx`, `src/modules/terminal/block/BlockOverlay.tsx`, `src/modules/explorer/FileExplorer.tsx`, `src/modules/explorer/ExplorerSearch.tsx`, `src/modules/agents/lib/review.ts` (or delete it), `src/modules/settings/store.ts` (only if it imports from `@/modules/ai/config` — see Step 6)

**Interfaces:**

- Consumes: `@/lib/native` (Task 1) — App.tsx's `native` import must already point there.
- Produces: no `@/modules/ai` or `@/components/ai-elements` imports anywhere. `src/modules/agents/` still compiles standalone (detection, bell, launcher) with zero chat imports — Task 4 and later tasks rely on this.

- [ ] **Step 1: Delete the two AI directories**

```bash
git rm -r src/modules/ai src/components/ai-elements
```

- [ ] **Step 2: Unwire `src/app/App.tsx`**

Remove: the `@/modules/ai` barrel import; mounts of `AiComposerProvider`, `AgentRunBridge`, `LocalAgentNotificationsBridge`, `AiMiniWindow`, `SelectionAskAi`; shortcut handlers for `ai.toggle`, `ai.toggleMini`, `ai.askSelection`, `editor.aiComplete`; `openAiDiffTab`/`closeAiDiffTab`/`togglePanelAndFocus` usages feeding AI UI; the `hasComposer` fallback to `openSettingsWindow("models")`.
Keep: `agent.focusAttention` handler, `launchAgentGroup` → `invoke("agent_enable_hooks", ...)`, everything unrelated to AI.

- [ ] **Step 3: Remove AI shortcuts**

In `src/modules/shortcuts/shortcuts.ts` delete `ai.toggle`, `ai.toggleMini`, `ai.askSelection`, `editor.aiComplete`. Keep `agent.focusAttention`.

- [ ] **Step 4: Remove AI entries from menus and surfaces**

- `src/modules/command-palette/commands.ts`: delete "Toggle AI agent" and "Ask AI about selection" commands.
- `src/modules/statusbar/StatusBar.tsx`: remove `AiOpenButton`/`AgentStatusPill`/`AiStatusBarControls` usage.
- `src/modules/terminal/block/BlockOverlay.tsx`: remove the "Attach to AI chat" action.
- `src/modules/explorer/FileExplorer.tsx` + `ExplorerSearch.tsx`: remove "Attach to Agent" context-menu items.

- [ ] **Step 5: Decouple the agents module from the deleted chat**

`src/modules/agents/lib/review.ts` pipes review messages into the built-in chat — remove that chat path (delete the file if it has no other purpose). `LocalAgentNotificationsBridge` was deleted with the AI module; remove any remaining reference to it. Keep detection, `NotificationBell` (`src/modules/header/Header.tsx`), `AgentIcon` in `src/modules/tabs/TabBar.tsx`, and the launcher in `src/modules/tabs/NewTabMenu.tsx` fully working.

- [ ] **Step 6: Fix the settings store's AI-config imports**

Run: `grep -n "modules/ai" src/modules/settings/store.ts`
Expected: import lines for defaults (`DEFAULT_MODEL_ID`, `DEFAULT_AUTOCOMPLETE_MODEL`, LM/MLX/Ollama base URLs). Replace those imports by inlining the literal default values where the field still exists; fields deleted in Task 4 lose their defaults there. Do not delete preference fields in this task — only make the file compile.

- [ ] **Step 7: Verify no dangling references**

Run: `grep -rn "@/modules/ai\|components/ai-elements" src/`
Expected: no output.

- [ ] **Step 8: Gates**

Run: `pnpm check-types && pnpm lint`
Expected: PASS. (`pnpm test` may still fail on AI-adjacent code — Task 4 cleans that; note any failures and move on only if they are all in Task 4's file list.)

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "feat: remove built-in AI chat module and UI mount points"
```

---

### Task 4: Remove AI-adjacent features (editor, source control, settings)

**Files:**

- Delete: `src/modules/editor/AiDiffPane.tsx`, `src/modules/editor/AiDiffStack.tsx`, `src/modules/editor/AiDiffStackLazy.tsx`, `src/modules/editor/lib/autocomplete/provider.ts`, `src/settings/sections/ModelsSection.tsx`, `src/settings/sections/AgentsSection.tsx`, `src/settings/components/ProviderIcon.tsx`, `src/settings/components/ProviderKeyCard.tsx`
- Modify: `src/modules/editor/lib/useTabs.ts` (or wherever `openAiDiffTab`/`closeAiDiffTab` live — find with grep), `src/modules/source-control/useSourceControlPanel.ts`, `src/modules/source-control/SourceControlPanel.tsx`, `src/settings/SettingsApp.tsx`, `src/modules/settings/store.ts`

**Interfaces:**

- Consumes: Task 3's clean tree (no `@/modules/ai` imports remain).
- Produces: `useTabs` with no `AiDiff` tab kind; `useSourceControlPanel` exposing only git operations (no `generateCommitMessage`-style API); `SettingsTab`/`VALID_TABS` without `models`/`agents`; `Preferences` in settings store without AI keys.

- [ ] **Step 1: Remove editor AI autocomplete and AI diff tabs**

Delete `src/modules/editor/lib/autocomplete/provider.ts` and any registration of it; delete the three `AiDiff*` files. Find and remove `openAiDiffTab`/`closeAiDiffTab` and the `aiDiff` tab kind:

Run: `grep -rn "openAiDiffTab\|closeAiDiffTab\|AiDiff" src/ --include="*.ts*" -l`
Expected: the files above plus `useTabs`; edit each until the grep returns nothing.

- [ ] **Step 2: Remove AI commit-message generation from source control**

In `src/modules/source-control/useSourceControlPanel.ts` remove the `useChatStore` usage, the `generateText` (`ai` package) call, and the exposed generate-commit-message function. In `SourceControlPanel.tsx` remove the button/menu item that triggered it. All git operations (stage, commit, push, etc.) stay untouched.

- [ ] **Step 3: Remove the Models/Agents settings tabs**

Delete `ModelsSection.tsx`, `AgentsSection.tsx`, `ProviderIcon.tsx`, `ProviderKeyCard.tsx`. In `src/settings/SettingsApp.tsx` remove the `models`/`agents` entries from the tab list, the `SettingsTab` type members, `VALID_TABS` entries, and the legacy `?tab=ai` / `?tab=connections` redirects to `models`.

- [ ] **Step 4: Remove AI preference fields from the settings store**

First verify the two borderline keys:

Run: `grep -rn "agentNotifications" src/ --include="*.ts*" | grep -v settings/store`
Run: `grep -rn "agentLaunchCommands" src/ --include="*.ts*" | grep -v settings/store`

Keep whichever the coding-agent UI still uses (expected: both are used by the agents bell/launcher — keep them); delete all other AI fields: `defaultModelId`, `autocompleteEnabled`, `autocompleteTrigger`, `autocompleteProvider`, `autocompleteModelId`, `lmstudioBaseURL`, `lmstudioModelId`, `mlxBaseURL`, `mlxModelId`, `ollamaBaseURL`, `ollamaModelId`, `openaiCompatibleBaseURL`, `openaiCompatibleModelId`, `openaiCompatibleContextLimit`, `customEndpoints`, `openrouterModelId`, `sttProvider`, `groqSttModel`, `whispercppBaseURL`, `favoriteModelIds`, `recentModelIds`, `customInstructions`, and their persisted `KEY_*` constants and defaults.

- [ ] **Step 5: Gates**

Run: `pnpm check-types && pnpm lint && pnpm test`
Expected: all PASS (delete or update any now-orphaned test referencing removed code).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: remove AI autocomplete, AI diffs, AI commit generation, and AI settings tabs"
```

---

### Task 5: Remove Rust AI modules and crates

**Files:**

- Delete: `src-tauri/src/modules/net.rs`, `src-tauri/src/modules/secrets.rs`
- Modify: `src-tauri/src/lib.rs` (mod declarations + `generate_handler!` block, ~lines 260–325), `src-tauri/Cargo.toml`
- Keep untouched: `src-tauri/src/modules/agent.rs`, `src-tauri/src/modules/pty/agent_detect.rs`

**Interfaces:**

- Consumes: Tasks 3–4 (frontend no longer invokes any `ai_http_*`/`secrets_*` command).
- Produces: IPC surface without `ai_http_request`, `ai_http_stream`, `lm_ping`, `secrets_get`, `secrets_set`, `secrets_delete`, `secrets_get_all`; `agent_enable_hooks`/`agent_hooks_status` remain.

- [ ] **Step 1: Prove the frontend no longer calls these commands**

Run: `grep -rn "ai_http_request\|ai_http_stream\|lm_ping\|secrets_" src/`
Expected: no output. If anything remains, remove that caller before continuing.

- [ ] **Step 2: Delete the modules and registrations**

```bash
git rm src-tauri/src/modules/net.rs src-tauri/src/modules/secrets.rs
```

In `src-tauri/src/lib.rs` remove `mod net;`/`mod secrets;` (or their paths in the modules tree) and the seven handler entries (`net::ai_http_request`, `net::ai_http_stream`, `net::lm_ping`, `secrets::secrets_get`, `secrets::secrets_set`, `secrets::secrets_delete`, `secrets::secrets_get_all`) from `generate_handler!`.

- [ ] **Step 3: Drop AI-only crates**

Verify each is unused elsewhere first:

Run: `cd src-tauri && grep -rn "reqwest\|keyring\|bytes::\|futures_util\|tokio::" src/`
Expected: `tokio` may still appear via `tauri::async_runtime` (fine — that's not direct use); direct `reqwest`/`keyring`/`bytes::`/`futures_util` uses should be gone. Then remove `reqwest`, `keyring`, `bytes`, `futures-util` from `Cargo.toml` (keep `tokio` only if it remains a direct dependency of other modules; otherwise drop it too).

- [ ] **Step 4: Rust gates**

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cd src-tauri && cargo test && cargo clippy
```

Expected: PASS (tests in `agent.rs`/`agent_detect.rs` still run and pass).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: remove AI HTTP proxy and keychain IPC modules"
```

---

### Task 6: Dependencies, tests, docs, build config

**Files:**

- Delete: `docs/architecture/ai-subsystem.md`, `docs/ai-workflow.png`
- Modify: `package.json`, `pnpm-lock.yaml` (via install), `vite.config.ts`, `components.json`, `README.md`, `docs/readme/README.*.md`, `CONTRIBUTING.md`, `docs/README.md`, `docs/architecture/security-model.md`, `docs/architecture/two-process-model.md`, `docs/architecture/cli-control.md`, `docs/architecture/pty-shell-integration.md`, `docs/architecture/terminal-renderer-pool.md`
- Do NOT delete: `TERAX.md`

**Interfaces:**

- Consumes: Tasks 1–5 (all AI code already gone).
- Produces: dependency lists free of AI-only packages; docs describing the app without the built-in AI but with the coding-agent integration intact.

- [ ] **Step 1: Verify and drop AI npm deps**

Run: `grep -rn "from \"zod\"\|from 'zod'\|require(\"zod\")\|@ai-sdk\|from \"ai\"\|from 'ai'" src/ vite.config.ts`
Expected: no output (all AI code is gone). Then remove from `package.json`: `@ai-sdk/anthropic`, `@ai-sdk/cerebras`, `@ai-sdk/google`, `@ai-sdk/groq`, `@ai-sdk/openai`, `@ai-sdk/openai-compatible`, `@ai-sdk/react`, `@ai-sdk/xai`, `ai`, `zod`. Keep `streamdown`. Leave the pre-existing `allowScripts` entries untouched. Then:

```bash
pnpm install
```

Expected: lockfile updates cleanly.

- [ ] **Step 2: Clean build config**

In `vite.config.ts` remove the `@ai-sdk/*` manual chunks (~lines 96–104). In `components.json` remove the `@ai-elements` registry entry.

- [ ] **Step 3: Delete AI docs and scrub mentions**

```bash
git rm docs/architecture/ai-subsystem.md docs/ai-workflow.png
```

Then edit `README.md`, every `docs/readme/README.*.md`, `CONTRIBUTING.md`, `docs/README.md`, and the five architecture docs listed above: remove built-in-AI feature descriptions (chat, providers, models settings, autocomplete, STT, AI commit generation) while keeping coding-agent integration docs (external CLI agents like pi/Claude Code, OSC detection, notification bell) accurate. Remember: public text, reviewer audience, no session jargon.

- [ ] **Step 4: Full verification gates**

```bash
pnpm check-types && pnpm lint && pnpm test && pnpm build
```

Expected: all PASS.

- [ ] **Step 5: Final residue greps**

Run: `grep -rn "@/modules/ai\|ai-elements\|@ai-sdk" src/ vite.config.ts components.json package.json`
Expected: no output.
Run: `grep -rn "ai_http_request\|ai_http_stream\|lm_ping\|secrets_" src/ src-tauri/src/`
Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: drop AI dependencies, tests, docs, and build config"
```

---

### Task 7: Full-diff review

- [ ] **Step 1: Dispatch a fresh `reviewer` subagent over the complete diff**

Diff range: everything on `chore/remove-built-in-ai` after the two spec docs commits (i.e. implementation commits only) against merge-base with `fix/ime-shift-punct-commit`. Review checklist: (1) no non-AI functionality deleted or broken; (2) coding-agent integration intact — detection, bell, ⇧⌘A, launcher, `agent_enable_hooks`, `pty/agent_detect.rs`; (3) no dangling imports/settings keys/shortcut ids/IPC commands; (4) no removed dependency still needed, no kept dependency now dead; (5) spec scope fully covered — cross-check the spec's Delete list item by item.

- [ ] **Step 2: Fix review findings and re-review**

Address each finding (worker fixes, then reviewer re-checks). Max 3 rounds; escalate to the user if a finding conflicts with the spec.

- [ ] **Step 3: Final gate sweep**

Run all Global Constraints verification gates one last time on the final tree; report results to the user.

## Self-Review Notes

- Spec coverage: Keep list → Tasks 1, 2, 3 (agents decoupling), 5 (agent.rs/agent_detect.rs untouched); Delete list → Tasks 3, 4, 5, 6; dependency drops → Tasks 5 (crates), 6 (npm); verification gates → every task + Task 7.
- `agentNotifications`/`agentLaunchCommands`: Task 4 Step 4 verifies usage before deciding (spec requirement).
- `chat-code-lezer.ts`: Task 2 Step 1 decides the move based on an actual dependency check.

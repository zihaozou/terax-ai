# Remove Built-in AI Subsystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Terax's built-in AI chat subsystem (chat, agent runner, subagents, AI commit generation, Agents settings tab) while keeping the terminal coding-agent integration, the **editor inline autocomplete** (user decision 2026-08-21), and generic infrastructure reused by non-AI features.

**Architecture:** Deletion + decoupling, executed as sequential tasks on one branch. Two shared files (`native.ts` IPC facade, markdown renderer chain) were relocated out of AI directories first (Tasks 1–2, done); then frontend chat code is deleted and mount points unwired while the autocomplete provider slice is preserved in place (Task 3); then the slice is extracted to a neutral home and the surviving Models settings tab is decoupled from the chat store (Task 4); then AI-adjacent features, the Rust HTTP proxy, dependencies, tests, and docs are removed (Tasks 5–7); finally a full-diff review (Task 8). Verification gates run at the end of every task.

**Tech Stack:** React/TypeScript frontend (Vite, Zustand, Vercel AI SDK — kept for autocomplete only), Tauri/Rust backend, pnpm, vitest, biome, cargo.

**Spec:** `docs/superpowers/specs/2026-08-21-remove-built-in-ai-design.md` (scope amended 2026-08-21: autocomplete kept — see spec header)

## Global Constraints

- Work on branch `chore/remove-built-in-ai`. Tasks run **sequentially** in one working tree (heavy file overlap on `App.tsx`, settings, shortcuts — no parallel writers). One commit per task.
- **Keep:** `src/modules/agents/` (detection, notification bell, ⇧⌘A jump, new-tab launcher), `src-tauri/src/modules/agent.rs`, `src-tauri/src/modules/pty/agent_detect.rs`, the `agent.focusAttention` shortcut, the `streamdown` npm dep, and the `TERAX.md` file itself.
- **Keep (autocomplete slice, user decision 2026-08-21):** `src/modules/editor/lib/autocomplete/` + `editor.aiComplete` shortcut; the provider slice (`config.ts`, `lib/keyring.ts`, `lib/proxyFetch.ts`, controller-slimmed `lib/agent.ts` = `buildLanguageModel`/`buildConfiguredLanguageModel` only — relocated to `src/lib/models/` in Task 4); the Models settings tab (`ModelsSection.tsx`, `ProviderIcon.tsx`, `ProviderKeyCard.tsx`); `src-tauri/src/modules/secrets.rs` + `secrets_*` handlers + the `keyring` crate; `src-tauri/src/modules/net.rs` + `ai_http_stream`/`lm_ping` handlers + `reqwest`/`bytes`/`futures-util` crates (autocomplete's local/custom providers proxy through Rust; Models tab test-connection calls `lm_ping`); npm `@ai-sdk/*` + `ai` and the `@ai-sdk/*` vite manual chunks.
- cargo lives at `/opt/homebrew/opt/rustup/bin` — every cargo step must start with `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
- `package.json` currently carries an unrelated pre-existing local modification (pnpm `allowScripts` entries). Do not revert it; when editing dependencies, leave those entries intact; NEVER stage it into a task commit.
- `pnpm test` runs vitest; for a single file use `pnpm vitest run <path>`.
- Docs/README edits are public-facing: write for a reviewer who has no context on this session (no internal workflow jargon).
- Before committing, run the formatter over changed files and stage the results, so no formatting leftovers remain in the working tree.
- Verification gates: `pnpm check-types`, `pnpm lint`, `pnpm test`, `pnpm build`; Rust: `cargo test`, `cargo clippy` (in `src-tauri`).

---

### Task 1: Relocate `native.ts` IPC facade out of the AI module ✅ DONE (`f8b03a3`)

(Completed: `src/modules/ai/lib/native.ts` → `src/lib/native.ts`, all importers rewritten.)

---

### Task 2: Relocate the shared markdown code renderer out of `ai-elements` ✅ DONE (`0bdb624`)

(Completed per controller ruling: full chain `markdown-code.tsx` + test + `chat-code.tsx` + `chat-code-lezer.ts` + `shimmer.tsx` → `src/components/markdown/`; `RunInTerminalButton`/`useChatStore` removed from `chat-code.tsx`.)

---

### Task 3: Delete the frontend AI chat core and unwire all mount points

**Files:**

- Delete: `src/modules/ai/` **EXCEPT** these kept-in-place slice files (already restored/slimmed by the controller — see Step 0): `src/modules/ai/config.ts`, `src/modules/ai/lib/keyring.ts`, `src/modules/ai/lib/agent.ts`, `src/modules/ai/lib/proxyFetch.ts`. Everything else under `src/modules/ai/` (chat store, agents, tools, compact, prompt, composer, all components) is deleted in this task.
- Delete: `src/components/ai-elements/` (remaining files: `conversation.tsx`, `message.tsx`, `context.tsx`, `reasoning.tsx`, `snippet.tsx`, `tool.tsx`)
- Modify: `src/app/App.tsx`, `src/modules/shortcuts/shortcuts.ts`, `src/modules/command-palette/commands.ts`, `src/modules/statusbar/StatusBar.tsx`, `src/modules/terminal/block/BlockOverlay.tsx`, `src/modules/explorer/FileExplorer.tsx`, `src/modules/explorer/ExplorerSearch.tsx`, `src/modules/agents/lib/review.ts` (or delete it)

**Interfaces:**

- Consumes: `@/lib/native` (Task 1) — App.tsx's `native` import must already point there.
- Produces: no imports of the deleted chat modules anywhere. `src/modules/agents/` still compiles standalone (detection, bell, launcher) with zero chat imports. The kept slice files still compile (their chat-only imports are kept alive for now). Autocomplete and the Models settings tab still work exactly as before.

- [ ] **Step 0: Note the working-tree starting state**

The controller already prepared the slice: `config.ts`, `lib/keyring.ts`, `lib/proxyFetch.ts` restored verbatim from HEAD; `lib/agent.ts` restored and **slimmed to the autocomplete slice only** (`buildLanguageModel`/`buildConfiguredLanguageModel` + their types — the chat-only `runAgentStream`/tools/compact/prompt machinery is already gone from the file); `keyring.ts` carries a one-line comment fix (empty catch). Include these four files, as they are, in your commit. Everything else under `src/modules/ai/` and `src/components/ai-elements/` is already staged as deleted — verify with `git status` that ONLY these four files remain present under `src/modules/ai/` (plus `config.ts` at its root), and `git rm` anything the brief lists that is not yet staged. `pnpm check-types` currently fails only in Task 4/5 files (`ModelsSection.tsx` chatStore usage, `AgentsSection.tsx`, `useSourceControlPanel.ts`) — that is the expected intermediate state.

- [ ] **Step 1: Delete the chat directories (minus the slice)**

`git rm -r` the remaining chat files under `src/modules/ai/` (everything except the keep-list above) and the remaining files under `src/components/ai-elements/`. Enumerate before deleting:

Run: `git status --short | grep "^D" | wc -l` and `ls src/modules/ai src/modules/ai/lib src/components/ai-elements`
Expected: only the keep-list files remain present under `src/modules/ai/`.

- [ ] **Step 2: Unwire `src/app/App.tsx`**

Remove: the `@/modules/ai` barrel import; mounts of `AiComposerProvider`, `AgentRunBridge`, `LocalAgentNotificationsBridge`, `AiMiniWindow`, `SelectionAskAi`; shortcut handlers for `ai.toggle`, `ai.toggleMini`, `ai.askSelection`; `openAiDiffTab`/`closeAiDiffTab`/`togglePanelAndFocus` usages feeding AI chat UI; the `hasComposer` fallback to `openSettingsWindow("models")`.
Keep: `agent.focusAttention` handler, `editor.aiComplete` handler, `launchAgentGroup` → `invoke("agent_enable_hooks", ...)`, everything unrelated to AI chat.
Constraint: use targeted edits, never a wholesale rewrite of this ~1600-line file.

- [ ] **Step 3: Remove AI chat shortcuts**

In `src/modules/shortcuts/shortcuts.ts` delete `ai.toggle`, `ai.toggleMini`, `ai.askSelection`. Keep `agent.focusAttention` **and** `editor.aiComplete`.

- [ ] **Step 4: Remove AI chat entries from menus and surfaces**

- `src/modules/command-palette/commands.ts`: delete "Toggle AI agent" and "Ask AI about selection" commands.
- `src/modules/statusbar/StatusBar.tsx`: remove `AiOpenButton`/`AgentStatusPill`/`AiStatusBarControls` usage.
- `src/modules/terminal/block/BlockOverlay.tsx`: remove the "Attach to AI chat" action.
- `src/modules/explorer/FileExplorer.tsx` + `ExplorerSearch.tsx`: remove "Attach to Agent" context-menu items.

- [ ] **Step 5: Decouple the agents module from the deleted chat**

`src/modules/agents/lib/review.ts` pipes review messages into the built-in chat — remove that chat path (delete the file if it has no other purpose). `LocalAgentNotificationsBridge` was deleted with the AI module; remove any remaining reference to it. Keep detection, `NotificationBell` (`src/modules/header/Header.tsx`), `AgentIcon` in `src/modules/tabs/TabBar.tsx`, and the launcher in `src/modules/tabs/NewTabMenu.tsx` fully working.

- [ ] **Step 6: Verify no dangling references to deleted modules**

Run: `grep -rn "@/modules/ai\|components/ai-elements" src/`
Expected: only imports of the kept slice files (`@/modules/ai/config`, `@/modules/ai/lib/keyring`, `@/modules/ai/lib/agent`) from: `src/modules/editor/EditorPane.tsx`, `src/modules/editor/lib/autocomplete/provider.ts`, `src/modules/settings/store.ts`, `src/settings/components/ProviderIcon.tsx`, `src/settings/components/ProviderKeyCard.tsx`, `src/settings/sections/ModelsSection.tsx`, `src/modules/source-control/useSourceControlPanel.ts` (Task 5 cleans this one — if it blocks compilation of kept code, flag it; its chat-store usage breaks anyway and is Task 5 scope).

- [ ] **Step 7: Gates**

Run: `pnpm check-types && pnpm lint`
Expected: PASS or failures only inside Task 5's file list (editor AiDiff, source-control AI commit, settings Agents tab, `ModelsSection.tsx`'s `useChatStore` import — that one is Task 4 scope). List every failure and why it's a later task's. `pnpm test` may fail on AI-adjacent code the same way.

- [ ] **Step 8: Format, stage, commit**

Run the formatter on changed files, stage everything belonging to this task (never the pre-existing `M package.json`), then:

```bash
git commit -m "feat: remove built-in AI chat module and UI mount points"
```

---

### Task 4: Extract the autocomplete provider slice to `src/lib/models/`

**Files:**

- Move: `src/modules/ai/config.ts` → `src/lib/models/config.ts`
- Move: `src/modules/ai/lib/keyring.ts` → `src/lib/models/keyring.ts`
- Move: `src/modules/ai/lib/agent.ts` → `src/lib/models/language-model.ts` (already slimmed by the controller to exactly the autocomplete slice — no extraction needed, just move)
- Move: `src/modules/ai/lib/proxyFetch.ts` → `src/lib/models/proxy-fetch.ts`
- Restore + move: `src/modules/ai/config.test.ts` (deleted in Task 3 — the kept slice keeps its test coverage; Task 3 review Minor #5) → `src/lib/models/config.test.ts`. Restore it first: `git show 2ac1cf1:src/modules/ai/config.test.ts > src/modules/ai/config.test.ts`
- After the moves, `src/modules/ai/` must be empty — `git rm` any straggler and remove the directory. After this task `src/modules/ai/` must not exist.
- Modify (importers): `src/modules/editor/EditorPane.tsx`, `src/modules/editor/lib/autocomplete/provider.ts`, `src/modules/settings/store.ts`, `src/settings/components/ProviderIcon.tsx`, `src/settings/components/ProviderKeyCard.tsx`, `src/settings/sections/ModelsSection.tsx`, `src/modules/source-control/useSourceControlPanel.ts` (if it imports the slice — Task 5 deletes that usage; a temporary redirect is fine)

**Interfaces:**

- Consumes: Task 3's tree (slice files in place, chat module otherwise deleted).
- Produces: `@/lib/models/config`, `@/lib/models/keyring`, `@/lib/models/language-model`, `@/lib/models/proxy-fetch` with unchanged export names; zero files under `src/modules/ai/`; Models settings tab no longer imports `useChatStore`.

- [ ] **Step 1: Move the four slice files**

```bash
mkdir -p src/lib/models
git mv src/modules/ai/config.ts src/lib/models/config.ts
git mv src/modules/ai/lib/keyring.ts src/lib/models/keyring.ts
git mv src/modules/ai/lib/agent.ts src/lib/models/language-model.ts
git mv src/modules/ai/lib/proxyFetch.ts src/lib/models/proxy-fetch.ts
git show 2ac1cf1:src/modules/ai/config.test.ts > src/modules/ai/config.test.ts && git add -N src/modules/ai/config.test.ts
git mv src/modules/ai/config.test.ts src/lib/models/config.test.ts
```

Fix internal imports: `language-model.ts` now imports `./config`, `./keyring`, `./proxy-fetch`; `keyring.ts` imports `./config`. (`agent.ts` was already slimmed by the controller — if you find any chat-only exports still in it, STOP and report BLOCKED with specifics.)

- [ ] **Step 4: Decouple ModelsSection from the chat store**

In `src/settings/sections/ModelsSection.tsx` remove the `useChatStore` import and its only usage (line ~248: `const { selectedModelId, setSelectedModelId } = useChatStore.getState();`). Replace the chat-model selection affordance with the autocomplete model selection already in the preferences store (`autocompleteModelId` etc.), or remove the selection UI if it only made sense for chat. Keep every provider/key/endpoint management feature working — that's the tab's surviving purpose. Use judgment; the tab must compile and make sense for configuring autocomplete.

- [ ] **Step 5: Rewire all importers**

Point every importer from Step 0's list at the new `@/lib/models/*` paths. Verify:

Run: `grep -rn "@/modules/ai" src/`
Expected: no output.

- [ ] **Step 6: Gates**

Run: `pnpm vitest run src/modules/editor/lib/autocomplete && pnpm check-types && pnpm lint`
Expected: autocomplete tests PASS; check-types/lint PASS or failures only in Task 5's file list (editor AiDiff, source-control AI commit, settings Agents tab, chat-only settings fields).

- [ ] **Step 7: Format, stage, commit**

```bash
git commit -m "refactor: extract autocomplete provider slice to src/lib/models"
```

(Never stage the pre-existing `M package.json`.)

---

### Task 5: Remove AI-adjacent features (AiDiff, AI commit generation, Agents settings tab, chat-only preferences)

**Files:**

- Delete: `src/modules/editor/AiDiffPane.tsx`, `src/modules/editor/AiDiffStack.tsx`, `src/modules/editor/AiDiffStackLazy.tsx`, `src/settings/sections/AgentsSection.tsx`
- Keep: `src/modules/editor/lib/autocomplete/` (all of it), `src/settings/sections/ModelsSection.tsx`, `src/settings/components/ProviderIcon.tsx`, `src/settings/components/ProviderKeyCard.tsx`
- Modify: `src/modules/editor/lib/useTabs.ts` (or wherever `openAiDiffTab`/`closeAiDiffTab` live — find with grep), `src/modules/source-control/useSourceControlPanel.ts`, `src/modules/source-control/SourceControlPanel.tsx`, `src/settings/SettingsApp.tsx`, `src/modules/settings/store.ts`

**Interfaces:**

- Consumes: Task 4's tree (no `@/modules/ai`; slice at `@/lib/models/*`).
- Produces: `useTabs` with no `AiDiff` tab kind; `useSourceControlPanel` exposing only git operations; `SettingsTab`/`VALID_TABS` without `agents` (keeps `models`); `Preferences` without chat-only keys; autocomplete fully functional.

- [ ] **Step 1: Remove AI diff tabs**

Delete the three `AiDiff*` files. Find and remove `openAiDiffTab`/`closeAiDiffTab` and the `aiDiff` tab kind:

Run: `grep -rn "openAiDiffTab\|closeAiDiffTab\|AiDiff" src/ --include="*.ts*" -l`
Expected: the files above plus `useTabs`; edit each until the grep returns nothing.

- [ ] **Step 2: Remove AI commit-message generation from source control**

In `src/modules/source-control/useSourceControlPanel.ts` remove the chat-store usage, the `generateText` (`ai` package) call, and the exposed generate-commit-message function. In `SourceControlPanel.tsx` remove the button/menu item that triggered it. All git operations (stage, commit, push, etc.) stay untouched.

- [ ] **Step 3: Remove the Agents settings tab (keep Models)**

Delete `AgentsSection.tsx`. In `src/settings/SettingsApp.tsx` remove the `agents` entry from the tab list, the `SettingsTab` type member, the `VALID_TABS` entry, and the legacy `?tab=connections` redirect. Keep `models` and the `?tab=ai` → `models` redirect (or repoint it wherever makes sense — the Models tab survives).

- [ ] **Step 4: Remove chat-only preference fields from the settings store**

First verify the two borderline keys:

Run: `grep -rn "agentNotifications" src/ --include="*.ts*" | grep -v settings/store`
Run: `grep -rn "agentLaunchCommands" src/ --include="*.ts*" | grep -v settings/store`

Keep whichever the coding-agent UI still uses (expected: both — keep them). Then for every other AI field, grep for references in surviving code (autocomplete, ModelsSection, ProviderKeyCard, EditorPane) and delete ONLY fields with no surviving references — expected deletes: `sttProvider`, `groqSttModel`, `whispercppBaseURL`, `customInstructions`, `openrouterModelId`, `defaultModelId` (verify), plus their persisted `KEY_*` constants and defaults. Expected keeps: `autocomplete*`, `lmstudio*`, `mlx*`, `ollama*`, `openaiCompatible*`, `customEndpoints`, `favoriteModelIds`, `recentModelIds`. Update store defaults to import from `@/lib/models/config` instead of the old paths.

- [ ] **Step 5: Gates**

Run: `pnpm check-types && pnpm lint && pnpm test`
Expected: all PASS (delete or update any now-orphaned test referencing removed code; autocomplete tests must still pass).

- [ ] **Step 6: Format, stage, commit**

```bash
git commit -m "feat: remove AI diffs, AI commit generation, and Agents settings tab"
```

---

### Task 6: Verify the Rust IPC surface (net.rs and secrets.rs both stay)

**Files:**

- Keep: `src-tauri/src/modules/net.rs` (`ai_http_stream` is autocomplete's local/custom-provider transport via `src/lib/models/proxy-fetch`; `lm_ping` is the Models tab test-connection), `src-tauri/src/modules/secrets.rs` (keyring backend), `src-tauri/src/modules/agent.rs`, `src-tauri/src/modules/pty/agent_detect.rs`
- Possibly modify: `src-tauri/src/lib.rs` (only if Step 1 proves `ai_http_request` unused), `src-tauri/Cargo.toml` (only if a crate proves unused)

**Interfaces:**

- Consumes: Tasks 3–5 (chat frontend gone; autocomplete slice intact).
- Produces: IPC surface unchanged or minus one dead handler; cargo gates green.

- [ ] **Step 1: Audit proxy command usage**

Run: `grep -rn "ai_http_request" src/ src-tauri/src/`
Expected: only the definition/registration in Rust, zero frontend callers. If confirmed, remove the `ai_http_request` handler from `net.rs` and `generate_handler!` (dead even before this branch — chat streaming used `ai_http_stream`). If anything calls it, keep it and note why in your report.
Run: `grep -rn "ai_http_stream\|lm_ping\|secrets_" src/`
Expected: hits in `src/lib/models/proxy-fetch.ts`, `src/settings/sections/ModelsSection.tsx`, `src/lib/models/keyring.ts` — the kept surface.

- [ ] **Step 2: Crate audit (expect no changes)**

Run: `cd src-tauri && grep -rn "reqwest\|bytes::\|futures_util\|keyring\|tokio::" src/`
Expected: `reqwest`/`bytes`/`futures_util` used by `net.rs`, `keyring` by `secrets.rs`, `tokio` via `tauri::async_runtime` — all still needed. Remove a crate only if the grep proves it unused.

- [ ] **Step 3: Rust gates**

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cd src-tauri && cargo test && cargo clippy
```

Expected: PASS (tests in `net.rs`/`secrets.rs`/`agent.rs`/`agent_detect.rs` still run and pass).

- [ ] **Step 4: Commit (only if anything changed)**

```bash
git commit -m "chore: drop dead ai_http_request handler"
```

If Steps 1–2 changed nothing, skip the commit and report DONE with the audit evidence.

---

### Task 7: Dependencies, tests, docs, build config

**Files:**

- Delete: `docs/architecture/ai-subsystem.md`, `docs/ai-workflow.png`
- Modify: `package.json`, `pnpm-lock.yaml` (via install), `components.json`, `README.md`, `docs/readme/README.*.md`, `CONTRIBUTING.md`, `docs/README.md`, `docs/architecture/security-model.md`, `docs/architecture/two-process-model.md`, `docs/architecture/cli-control.md`, `docs/architecture/pty-shell-integration.md`, `docs/architecture/terminal-renderer-pool.md`
- Do NOT delete: `TERAX.md`. Do NOT modify: `vite.config.ts` `@ai-sdk/*` manual chunks (deps kept).

**Interfaces:**

- Consumes: Tasks 1–6 (all chat code gone; autocomplete slice kept).
- Produces: dependency lists with only autocomplete's surviving AI deps; docs describing the app **with editor autocomplete + Models settings tab**, without the built-in chat, with the coding-agent integration intact.

- [ ] **Step 1: Verify and drop chat-only npm deps**

Run: `grep -rn "from \"zod\"\|from 'zod'\|require(\"zod\")" src/ vite.config.ts`
Expected: no output (zod was chat tooling). Then remove `zod` from `package.json` if unused. **Keep** `@ai-sdk/*`, `ai`, `streamdown` — also verify the kept packages ARE still imported (autocomplete slice + ModelsSection use them): `grep -rn "@ai-sdk\|from \"ai\"" src/ | head`. Leave the pre-existing `allowScripts` entries untouched. Then:

```bash
pnpm install
```

Expected: lockfile updates cleanly.

- [ ] **Step 2: Clean build config**

In `components.json` remove the `@ai-elements` registry entry. Leave `vite.config.ts` unchanged (verify the `@ai-sdk/*` chunks are still accurate for the kept deps).

- [ ] **Step 3: Delete AI chat docs and scrub mentions**

```bash
git rm docs/architecture/ai-subsystem.md docs/ai-workflow.png
```

Then edit `README.md`, every `docs/readme/README.*.md`, `CONTRIBUTING.md`, `docs/README.md`, and the five architecture docs listed above: remove built-in-chat feature descriptions (chat panel, agent runner, subagents, STT, AI commit generation) while keeping docs for editor autocomplete, the Models settings tab, and the coding-agent integration (external CLI agents like pi/Claude Code, OSC detection, notification bell) accurate. Remember: public text, reviewer audience, no session jargon.

- [ ] **Step 4: Full verification gates**

```bash
pnpm check-types && pnpm lint && pnpm test && pnpm build
```

Expected: all PASS.

- [ ] **Step 5: Final residue greps**

Run: `grep -rn "@/modules/ai\|ai-elements" src/ vite.config.ts components.json package.json`
Expected: no output.
Run: `grep -rn "ai_http_request" src/`
Expected: no output (dead handler dropped in Task 6). `ai_http_stream`/`lm_ping`/`secrets_` hits in `src/lib/models/` + ModelsSection are expected and correct.

- [ ] **Step 6: Commit**

```bash
git commit -m "chore: drop chat-only dependencies, tests, docs, and build config"
```

---

### Task 8: Full-diff review

- [ ] **Step 1: Dispatch a fresh `reviewer` subagent over the complete diff**

Diff range: everything on `chore/remove-built-in-ai` after the spec/plan docs commits (i.e. implementation commits only) against merge-base with `fix/ime-shift-punct-commit`. Review checklist: (1) no non-AI functionality deleted or broken; (2) coding-agent integration intact — detection, bell, ⇧⌘A, launcher, `agent_enable_hooks`, `pty/agent_detect.rs`; (3) **autocomplete slice intact** — `editor.aiComplete`, `src/modules/editor/lib/autocomplete/`, `@/lib/models/*`, Models settings tab, `secrets.rs` + `secrets_*` handlers, `@ai-sdk/*` deps; (4) no dangling imports/settings keys/shortcut ids/IPC commands; (5) no removed dependency still needed, no kept dependency now dead; (6) spec scope fully covered — cross-check the spec's Delete list item by item.

- [ ] **Step 2: Fix review findings and re-review**

Address each finding (worker fixes, then reviewer re-checks). Max 3 rounds; escalate to the user if a finding conflicts with the spec.

- [ ] **Step 3: Final gate sweep**

Run all Global Constraints verification gates one last time on the final tree; report results to the user.

## Self-Review Notes

- Spec coverage: Keep list → Tasks 1, 2, 3 (agents decoupling + slice preservation), 4 (slice extraction + ModelsSection decoupling), 5 (Models tab/autocomplete kept), 6 (secrets.rs/keyring crate kept), 7 (deps kept); Delete list → Tasks 3, 4 (chat-only leftovers), 5, 6, 7; verification gates → every task + Task 8.
- `agentNotifications`/`agentLaunchCommands`: Task 5 Step 4 verifies usage before deciding (spec requirement).
- Settings-store field deletes are reference-driven (Task 5 Step 4): only fields with zero surviving references are deleted — protects autocomplete/Models-tab fields.
- `ModelsSection.tsx` chat-store coupling: single usage site (selectedModelId), resolved in Task 4 Step 4.
- Plan history: v1 (7 tasks) superseded 2026-08-21 after the user decided to keep editor autocomplete; Tasks 1–2 unchanged and already complete; old Tasks 4–7 renumbered to 5–8; new Task 4 = provider-slice extraction.

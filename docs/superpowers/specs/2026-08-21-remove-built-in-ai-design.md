# Remove built-in AI subsystem — design & execution plan

Date: 2026-08-21
Branch: `chore/remove-built-in-ai` (stacked on `fix/ime-shift-punct-commit` to keep the user's IME fix in their personal build)

## Goal

Remove Terax's built-in AI subsystem (chat, providers, tools, autocomplete, AI commit generation, settings UI) — logic and UI — while keeping the terminal coding-agent integration (detection/notifications for external agents like pi, Claude Code, Codex) and any generic infrastructure reused by non-AI features.

Rationale: the built-in AI duplicates what external coding agents (pi et al.) already do; the coding-agent integration complements them.

## Scope

### Keep (decouple where needed)

- `src/modules/agents/` — coding-agent detection, notification bell, ⇧⌘A jump (`agent.focusAttention`), new-tab launcher. **Cut** its dependency on the built-in chat (`lib/review.ts` pipes review messages into the AI chat — remove that path; `LocalAgentNotificationsBridge` goes with the chat).
- `src-tauri/src/modules/agent.rs` + `src-tauri/src/modules/pty/agent_detect.rs` — OSC hooks / agent-signal detection (used by the coding-agent integration, not the built-in AI).
- `src/modules/ai/lib/native.ts` — generic Tauri IPC facade used by git, explorer, source-control, spaces, editor. **Relocate to `src/lib/native.ts`** and rewrite all imports.
- `src/components/ai-elements/markdown-code.tsx` (+ `chat-code-lezer.ts` if it depends on it) — shared with `MarkdownPreviewPane`. **Relocate to a shared location** (e.g. `src/components/markdown/`). Keep `streamdown` dep.
- `TERAX.md` file itself (project docs); only the AI prompt-builder that loads it is deleted.

### Delete

- `src/modules/ai/` entirely (except relocated `native.ts`) — 78 files, ~12.8k LOC
- `src/components/ai-elements/` (except relocated markdown code renderer) — 11 files
- AI-adjacent features:
  - Editor: `lib/autocomplete/provider.ts` (AI inline completion), `AiDiffPane.tsx`/`AiDiffStack*.tsx`, `openAiDiffTab`/`closeAiDiffTab` in `useTabs`, `editor.aiComplete` shortcut
  - Source control: AI commit-message generation in `useSourceControlPanel.ts` (git logic stays)
  - Settings: `ModelsSection.tsx`, `AgentsSection.tsx`, `ProviderIcon.tsx`, `ProviderKeyCard.tsx`, `models`/`agents` tabs in `SettingsApp.tsx` + `SettingsTab`/`VALID_TABS` + legacy `?tab=ai`/`connections` redirects
  - AI preference fields/keys in `src/modules/settings/store.ts` (defaultModelId, autocomplete*, lmstudio*, mlx*, ollama*, openaiCompatible*, customEndpoints, openrouterModelId, stt*, favoriteModelIds, recentModelIds, customInstructions, agentLaunchCommands if unused by agents module — verify). Keep `agentNotifications` if the coding-agent UI uses it — verify before deleting.
- Entry points: `App.tsx` AI mounts (`AiComposerProvider`, `AgentRunBridge`, `AiMiniWindow`, `SelectionAskAi`, `AiStatusBarControls`), `ai.toggle`/`ai.toggleMini`/`ai.askSelection` shortcuts (keep `agent.focusAttention`), command-palette AI entries, status-bar AI controls, block-overlay "Attach to AI chat", explorer "Attach to Agent" context items
- Rust: `src-tauri/src/modules/net.rs`, `src-tauri/src/modules/secrets.rs` + their `generate_handler!` registrations in `lib.rs` (verify no non-AI frontend code invokes `ai_http_request`/`secrets_*` after frontend removal)
- Tests: 16 frontend AI test files; Rust tests inside `net.rs`/`secrets.rs`
- Docs: delete `docs/architecture/ai-subsystem.md` + `docs/ai-workflow.png`; update README*, `CONTRIBUTING.md`, and other architecture docs that describe the built-in AI
- Build config: `@ai-sdk/*` manual chunks in `vite.config.ts`; `@ai-elements` registry in `components.json`

### Dependencies to drop (after code removal, verify with a final grep)

- npm: `@ai-sdk/anthropic`, `@ai-sdk/cerebras`, `@ai-sdk/google`, `@ai-sdk/groq`, `@ai-sdk/openai`, `@ai-sdk/openai-compatible`, `@ai-sdk/react`, `@ai-sdk/xai`, `ai`, `zod` (verify zod unused elsewhere first)
- Keep: `streamdown` (markdown preview)
- Rust crates: `reqwest`, `keyring`, `bytes`, `futures-util` (verify each is only used by `net.rs`/`secrets.rs`; check `tokio` direct usage)

## Verification gates

- `pnpm check-types` (tsc --noEmit)
- `pnpm lint` (biome)
- `pnpm test` (vitest)
- `pnpm build`
- Rust: `cargo test` and `cargo clippy` in `src-tauri` (note: cargo via `/opt/homebrew/opt/rustup/bin` — export PATH explicitly in non-interactive shells)
- Final greps: no imports from `@/modules/ai`, no `ai_http_request`/`secrets_` invocations, no `@ai-sdk`/`from "ai"` imports

## Execution

Sequential tasks on this branch (heavy file overlap on `App.tsx`, settings, shortcuts — no parallel writers). One commit per task.

### Task A — Relocate shared infra (worker)

- **Goal:** Move `src/modules/ai/lib/native.ts` → `src/lib/native.ts`; move `src/components/ai-elements/markdown-code.tsx` (+ `chat-code-lezer.ts` if needed) → `src/components/markdown/`; rewrite every import repo-wide.
- **Files:** `src/modules/ai/lib/native.ts`, `src/components/ai-elements/markdown-code.tsx`, all importers (App.tsx, explorer, source-control, git-history, spaces, editor/lib/diffCache, MarkdownPreviewPane, …).
- **Done when:** `pnpm check-types` clean; no file outside `src/modules/ai` imports from `@/modules/ai/lib/native`; commit.

### Task B — Delete frontend AI core (worker)

- **Goal:** Delete `src/modules/ai/` and `src/components/ai-elements/`; remove all mount points/wiring in `App.tsx`; remove `ai.*` shortcuts, command-palette AI entries, status-bar AI controls, block-overlay/explorer "Attach to AI" items; decouple `src/modules/agents/` from the chat (remove `lib/review.ts` chat path + `LocalAgentNotificationsBridge`) while keeping detection/notification/launcher behavior.
- **Done when:** `pnpm check-types` + `pnpm lint` clean; app builds; no dangling imports; commit.

### Task C — Remove AI-adjacent features (worker)

- **Goal:** Editor AI autocomplete + AiDiff panes/tabs + `editor.aiComplete`; source-control AI commit generation; settings Models/Agents tabs + provider widgets + AI preference keys (verify `agentNotifications`/`agentLaunchCommands` usage first); `TERAX.md` loader references.
- **Done when:** `pnpm check-types` + `pnpm lint` + `pnpm test` clean; commit.

### Task D — Rust cleanup (worker)

- **Goal:** Delete `net.rs`, `secrets.rs`; remove their registrations in `lib.rs`; drop `reqwest`/`keyring`/`bytes`/`futures-util` from `Cargo.toml` if unused elsewhere; keep `agent.rs`/`pty/agent_detect.rs`.
- **Done when:** `cargo test` + `cargo clippy` clean; commit.

### Task E — Deps, tests, docs (worker)

- **Goal:** Delete AI test files; drop AI npm deps (verify `zod` first); clean `vite.config.ts` chunks + `components.json`; delete `docs/architecture/ai-subsystem.md` + `docs/ai-workflow.png`; scrub built-in-AI mentions from README* / CONTRIBUTING / docs (keep coding-agent integration docs accurate).
- **Done when:** full verification gates pass; commit.

### Review

- Fresh `reviewer` subagent pass over the full diff vs. merge-base, checking: nothing non-AI deleted, coding-agent integration intact, no dangling references, gates green. Max 3 review rounds.

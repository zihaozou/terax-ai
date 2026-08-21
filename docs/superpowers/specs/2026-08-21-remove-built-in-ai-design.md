# Remove built-in AI subsystem — design

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
- `src/components/ai-elements/markdown-code.tsx` (+ `chat-code-lezer.ts` if it depends on it) — shared with `MarkdownPreviewPane`. **Relocate to `src/components/markdown/`** and update its importers (`MarkdownPreviewPane`, `ChatMessageItem`). Keep `streamdown` dep.
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

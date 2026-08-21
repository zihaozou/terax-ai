# Remove built-in AI subsystem — design

Date: 2026-08-21
Branch: `chore/remove-built-in-ai` (stacked on `fix/ime-shift-punct-commit` to keep the user's IME fix in their personal build)

## Goal

Remove Terax's built-in AI subsystem (chat, agent runner, subagents, provider key management for chat, AI commit generation, AI settings UI) — logic and UI — while keeping the terminal coding-agent integration (detection/notifications for external agents like pi, Claude Code, Codex) and any generic infrastructure reused by non-AI features.

Rationale: the built-in AI chat duplicates what external coding agents (pi et al.) already do; the coding-agent integration complements them.

**Scope amendment (2026-08-21, user decision):** the editor inline autocomplete is **kept** — it is an editor feature, not a chat/coding-agent duplicate. Keeping it pulls a thin provider slice back into the Keep list (model/endpoint config, API-key keyring, provider adapters, Models settings tab, `secrets.rs`, `@ai-sdk/*` deps). Everything chat-related is still deleted.

## Scope

### Keep (decouple where needed)

- `src/modules/agents/` — coding-agent detection, notification bell, ⇧⌘A jump (`agent.focusAttention`), new-tab launcher. **Cut** its dependency on the built-in chat (`lib/review.ts` pipes review messages into the AI chat — remove that path; `LocalAgentNotificationsBridge` goes with the chat).
- `src-tauri/src/modules/agent.rs` + `src-tauri/src/modules/pty/agent_detect.rs` — OSC hooks / agent-signal detection (used by the coding-agent integration, not the built-in AI).
- `src/modules/ai/lib/native.ts` — generic Tauri IPC facade used by git, explorer, source-control, spaces, editor. **Relocate to `src/lib/native.ts`** and rewrite all imports. ✅ Done (Task 1).
- `src/components/ai-elements/markdown-code.tsx` renderer chain — shared with `MarkdownPreviewPane`. **Relocated to `src/components/markdown/`** (incl. `chat-code.tsx`/`chat-code-lezer.ts`/`shimmer.tsx`; `RunInTerminalButton` removed — its chat backend is deleted). ✅ Done (Task 2). Keep `streamdown` dep.
- **Editor inline autocomplete** (`src/modules/editor/lib/autocomplete/` — provider/prompt/inlineExtension/normalizeIndent/trimSuggestion + tests) and the `editor.aiComplete` shortcut.
- **Autocomplete provider slice** — the only survivors of `src/modules/ai/`:
  - `config.ts` (model/endpoint registry, default model IDs, base-URL defaults)
  - `lib/keyring.ts` (API-key storage via Tauri `secrets_*` commands)
  - `buildLanguageModel` + `buildConfiguredLanguageModel` from `lib/agent.ts` (provider adapters; the chat-only `runAgentStream`/`buildStableSystem`/`buildTools`/`compact`/`prompt`/`proxyFetch` machinery is deleted)
  - `lib/proxyFetch.ts` (Tauri `ai_http_stream` fetch adapter — required by the local/custom providers, which must proxy through Rust to reach LAN endpoints)
  - **Relocate** these to `src/lib/models/` and rewire importers (editor autocomplete, settings).
- **Settings Models tab**: `ModelsSection.tsx`, `ProviderIcon.tsx`, `ProviderKeyCard.tsx`, the `models` tab in `SettingsApp.tsx` (required to configure autocomplete providers/keys). Its `useChatStore` usage (chat model selection) is replaced with preferences-store state. Settings-store fields referenced by autocomplete/Models tab stay (`autocomplete*`, `lmstudio*`/`mlx*`/`ollama*`/`openaiCompatible*`, `customEndpoints`, `favoriteModelIds`/`recentModelIds`, and the matching defaults now sourced from `src/lib/models/config`).
- `src-tauri/src/modules/secrets.rs` + its `secrets_*` handlers (keyring backend for autocomplete API keys) and the `keyring` crate.
- `src-tauri/src/modules/net.rs` + its `ai_http_stream`/`lm_ping` handlers (autocomplete's local/custom providers proxy through it via `proxyFetch.ts`; the Models tab's test-connection buttons call `lm_ping`) and the `reqwest`/`bytes`/`futures-util` crates it needs. (`ai_http_request` may be dropped if a grep proves it unused.)
- npm `@ai-sdk/*` provider packages + `ai` (used by the autocomplete provider adapter).
- `TERAX.md` file itself (project docs); only the AI prompt-builder that loads it is deleted.

### Delete

- `src/modules/ai/` entirely **except the relocated provider slice above** — ~74 files of chat/agents/tools/prompt machinery
- `src/components/ai-elements/` (remaining files after the Task 2 relocation)
- AI-adjacent features:
  - Editor: `AiDiffPane.tsx`/`AiDiffStack*.tsx`, `openAiDiffTab`/`closeAiDiffTab` in `useTabs` (chat's apply-diff UI; the chat is gone so it has no entry point). **Autocomplete stays.**
  - Source control: AI commit-message generation in `useSourceControlPanel.ts` (git logic stays)
  - Settings: `AgentsSection.tsx` + the `agents` tab + legacy `?tab=connections` redirect. **The `models` tab stays.**
  - Chat-only preference fields in `src/modules/settings/store.ts` (`defaultModelId` if chat-only, `openrouterModelId` if chat-only, `stt*`, `groqSttModel`, `whispercppBaseURL`, `customInstructions`, and anything not referenced by surviving code — verify each field against the kept surfaces before deleting). Keep `agentNotifications`/`agentLaunchCommands` if the coding-agent UI uses them — verify before deleting.
- Entry points: `App.tsx` AI chat mounts (`AiComposerProvider`, `AgentRunBridge`, `AiMiniWindow`, `SelectionAskAi`, `AiStatusBarControls`), `ai.toggle`/`ai.toggleMini`/`ai.askSelection` shortcuts (keep `agent.focusAttention` **and** `editor.aiComplete`), command-palette AI chat entries, status-bar AI controls, block-overlay "Attach to AI chat", explorer "Attach to Agent" context items
- Rust: nothing deleted — `net.rs` and `secrets.rs` both stay (autocomplete proxy + keyring). Task 6 verifies the surface and may drop the unused `ai_http_request` handler.
- Tests: frontend AI chat test files. Autocomplete tests (`normalizeIndent`, `prompt`, `trimSuggestion`) stay; Rust `net.rs`/`secrets.rs` tests stay.
- Docs: delete `docs/architecture/ai-subsystem.md` + `docs/ai-workflow.png`; update README*/CONTRIBUTING/architecture docs — they must describe the app **with autocomplete and the Models settings tab** but without the built-in chat.
- Build config: `@ai-elements` registry in `components.json`. The `@ai-sdk/*` manual chunks in `vite.config.ts` **stay** (deps kept).

### Dependencies to drop (after code removal, verify with a final grep)

- npm: `zod` (verify unused by surviving code first). Keep `@ai-sdk/*`, `ai`, `streamdown`.
- Rust crates: none dropped unless Task 6 proves otherwise (net.rs/secrets.rs both stay).

## Verification gates

- `pnpm check-types` (tsc --noEmit)
- `pnpm lint` (biome)
- `pnpm test` (vitest)
- `pnpm build`
- Rust: `cargo test` and `cargo clippy` in `src-tauri` (note: cargo via `/opt/homebrew/opt/rustup/bin` — export PATH explicitly in non-interactive shells)
- Final greps: no imports from `@/modules/ai`, no `zod` imports if dropped (`secrets_*` and `ai_http_stream`/`lm_ping` invocations remain — keyring + autocomplete proxy)

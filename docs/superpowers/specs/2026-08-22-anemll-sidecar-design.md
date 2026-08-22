# Anemll Sidecar — Design

Local ANE model server as a native Swift binary, bundled into Terax.app and
managed by the app lifecycle. Replaces the external Python server + LaunchAgent.

## Goals

- Autocomplete's local model server starts when Terax starts and dies when
  Terax quits. No login item, no resident daemon, no Python/venv on the path.
- Single native binary (`anemll-serverd`) compiled from Swift, using Anemll's
  official `AnemllCore` inference library (ANE via CoreML stateful models).
- OpenAI-compatible surface identical to the Python server so the existing
  `openai-compatible` provider config keeps working unchanged:
  `GET /v1/models`, `POST /v1/chat/completions` (non-stream + SSE stream).
- FIM-first: when the request is a Terax autocomplete prompt
  (`PREFIX:\n<<<…>>>\n\nSUFFIX:\n<<<…>>>`) and the model has FIM tokens,
  serve it as raw `<|fim_prefix|>…<|fim_suffix|>…<|fim_middle|>` continuation.
  Otherwise fall back to the tokenizer chat template (plain concat when none).

## Non-goals (v1)

- No model files bundled in the app (model dir stays user-provided).
- No crash auto-restart (next app launch respawns; port probe prevents dupes).
- No multi-instance reference counting: the spawning instance owns the child.
  A second Terax instance reuses the running server; if the owner quits first
  the second instance loses completions until relaunch. Known limitation.
- No sampling beyond temperature=0/low (autocomplete use case).

## Components

### 1. `sidecar/anemll-serverd/` — Swift package (in-repo)

- Deps: `AnemllCore` (git: Anemll/Anemll `anemll-swift-cli` package, MIT),
  `FlyingFox` (HTTP), `swift-argument-parser`.
- Args: `--model-dir <path>` (required), `--port <int>` (default 8100),
  `--max-tokens <int>` (default 48), `--max-newlines <int>` (default 5).
- Startup: YAMLConfig from `<model-dir>/meta.yaml` → ModelLoader → Tokenizer →
  InferenceManager (wiring mirrors `anemllcli.swift`), then warmup generation,
  then bind HTTP on 127.0.0.1 only.
- Generation: serialized behind an actor (single in-flight request; later
  requests queue). Early stops: stop tokens, FIM pad/endoftext, newline budget,
  closing code fence (chat mode only), token cap.
- Logs: stdout; parent redirects to app log dir.

### 2. Terax integration

- Tauri `externalBin` sidecar: binary ships inside Terax.app/Contents/MacOS.
- New Rust `modules/sidecar.rs`:
  - `sidecar_start(modelDir, port, …)` command — TCP probe first: if the port
    answers, report `already-running` and do NOT own it; else spawn the
    bundled binary via tauri-plugin-shell sidecar API, keep the child handle
    in managed state, pipe output to `<app-log-dir>/sidecar.log`.
  - `RunEvent::ExitRequested`/`Exit` — kill the owned child only.
- Settings (Models tab, "Local model server" section):
  - `sidecarEnabled: bool` (default false)
  - `sidecarModelDir: string` (default "")
  - `sidecarPort: number` (default 8100)
- Frontend: after preference hydration in App, if enabled && modelDir set →
  invoke `sidecar_start`. Failures logged via plugin-log, non-fatal.

## Migration (this machine)

- Disable + remove LaunchAgent `com.zihaozou.anemll-server`.
- Settings: enable sidecar, model dir `~/anemll/qwen25-coder-05b-ctx2048-argmax`.
- Python server + venv remain on disk as a fallback bench tool; no longer used.

## Accepted tradeoffs

- First completion after app launch waits for model load (~10–20 s with a warm
  ANE cache; minutes on a cold cache after reboot/system purge).
- Building the sidecar requires Xcode (already a repo requirement direction);
  CI/packaging story deferred until the fork needs distribution.

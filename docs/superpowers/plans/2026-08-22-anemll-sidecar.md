# Anemll Sidecar — Plan

Spec: `docs/superpowers/specs/2026-08-22-anemll-sidecar-design.md`

## Phase 1 — Swift daemon standalone

1. Scaffold `sidecar/anemll-serverd` package (AnemllCore + FlyingFox +
   argument-parser); pin AnemllCore to a commit.
2. Model boot path: YAMLConfig → ModelLoader → Tokenizer → InferenceManager,
   wiring copied from `anemllcli.swift` (argmax, sliding window, dynamic
   slice, split lm_head, chunk sizes).
3. Generation actor: prompt → token ids → runPrefill → generateNextToken loop
   with shiftWindow, early stops, incremental detokenization.
4. HTTP: `GET /v1/models`, `POST /v1/chat/completions` (stream + non-stream),
   FIM detection/parse identical to `server_gemma.py`.
5. Verify: `swift run anemll-serverd --model-dir …argmax` then reuse the
   existing Python benchmark payloads (big/small); compare latency + output
   sanity against the Python server numbers.

Checkpoint: benchmark table posted; user go/no-go.

## Phase 2 — Terax integration

1. `swift build -c release` → copy binary to `src-tauri/binaries/` with target
   triple suffix; `tauri.conf.json` externalBin + shell plugin sidecar
   permission scoped to this binary.
2. `modules/sidecar.rs`: port probe, spawn via shell plugin sidecar API,
   owned-child state, kill on exit, `sidecar.log` in app log dir.
3. Settings store keys + Models section UI (toggle, model dir picker, port).
4. App.tsx boot effect invoking `sidecar_start` after hydration.

## Phase 3 — Verify & land

 1. Gates: check-types / lint / test / build + cargo test + clippy.
 2. Dev-instance end-to-end: sidecar spawns, completion works, quit kills it
    (verify no orphan via `pgrep`); relaunch reuses/respawns cleanly.
 3. Migration: bootout + remove LaunchAgent, configure settings, release
    build, user swaps /Applications/Terax.app.
 4. Commit(s) + push to fork; memory + ledger updates.

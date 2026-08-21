# Security model

This guide elaborates on `TERAX.md`. If anything here conflicts with `TERAX.md`, `TERAX.md` wins.

Terax runs shells, reads and writes files, and sends autocomplete requests to AI providers. The security model is defense-in-depth: no single guard is enough, so every boundary validates input before acting on it.

## Boundaries

The main trust boundaries are:

1. **IPC boundary** - commands registered in `src-tauri/src/lib.rs`, gated by `src-tauri/capabilities/default.json`.
2. **File-system boundary** - PTY spawn, git, and file mutations go through the workspace authorization registry.
3. **Network boundary** - HTTP proxy in `src-tauri/src/modules/net.rs` with SSRF and DNS-rebinding defenses (used by autocomplete provider calls and local-model pings).
4. **Secret-storage boundary** - keys live in the OS keychain, never on disk or in `localStorage`.
5. **Terminal escape-sequence boundary** - OSC sequences are parsed and acted on, but never blindly trusted to mutate state.

## Workspace authorization registry

`WorkspaceRegistry` (`src-tauri/src/modules/workspace.rs:20`) tracks directories that PTY spawn and git commands are allowed to operate in.

- `workspace_authorize` adds a directory.
- `authorize_spawn_cwd` rejects a spawn cwd outside an authorized root.
- `authorize_user_spawn_cwd` registers the user's chosen cwd as a new root instead of rejecting it.
- The registry is bootstrapped with the launch directory and the user's home directory (`workspace.rs:135`).

This is the allow side of the file-system boundary. Any new feature that spawns a shell or mutates files outside the current workspace must interact with this registry.

## SSRF and DNS rebinding defense

`src-tauri/src/modules/net.rs` proxies autocomplete provider requests and local-model pings. Before connecting:

1. Resolve the hostname once (`resolve_and_classify`).
2. Classify every resolved IP as public, private, loopback, or blocked metadata.
3. Block cloud metadata endpoints (`169.254.169.254`, `metadata.google.internal`, AWS IPv6 metadata, etc.).
4. Pin reqwest to the resolved IPs so a second DNS lookup cannot return a different address (DNS rebinding).

Local model endpoints are explicitly allowed because the user opted in by pointing Terax at them, but they are still classified and logged.

## Secret storage

API keys are stored via `secrets_*` commands (`src-tauri/src/modules/secrets.rs`):

- macOS: Keychain via `keyring`
- Windows: Credential Manager via `keyring`
- Linux: a JSON file in the app's local data dir with mode `0600` (atomic write to `.tmp` then rename)

Service constant: `terax-ai`. Keys never touch disk outside the keychain/Linux secrets file, never go in `localStorage`, and never appear in logs.

## OSC trust gating

The terminal parses OSC sequences from the PTY byte stream:

- **OSC 7** updates the tab cwd.
- **OSC 133 A/B/C/D** marks prompt/command boundaries.
- **OSC 777** is used by the agent detector to signal coding-agent state transitions.

The agent detector (`src-tauri/src/modules/pty/agent_detect.rs`) is armed by `OSC 133;C;<cmd>` or by a self-armed marker and emits `terax:agent-signal` events. It is driven **only by OSC sequences**, never by raw output, so a repainting TUI never flaps.

## Invariants

- New file-system-touching commands must respect the workspace authorization registry.
- New network-facing commands must go through the `net.rs` proxy or reimplement the same classification and DNS pinning.
- New plugin APIs must be added to `src-tauri/capabilities/default.json`.
- Keys, tokens, and credentials stay in the keychain / Linux secrets file.

## See also

- [`TERAX.md`](../../TERAX.md) - the architecture source of truth
- [`docs/README.md`](../README.md) - index of contributor guides
- [Two-process model](two-process-model.md) - IPC boundary and command catalog

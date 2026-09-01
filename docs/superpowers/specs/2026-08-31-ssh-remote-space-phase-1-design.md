# SSH Remote Space Phase 1 Design

## Summary

Terax will add SSH as a first-class Space environment. Phase 1 establishes the connection, trust, authentication, and terminal foundations for a later full remote workspace. A remote Space references a reusable SSH profile, connects lazily when activated, and multiplexes terminal PTY channels over one authenticated `russh` transport.

Phase 1 supports Terax clients on macOS, Linux, and Windows connecting to Linux or macOS SSH servers. It does not yet provide remote file access, remote Git, remote LSP, port forwarding, or transparent terminal recovery.

## Goals

- Make SSH a first-class `WorkspaceEnv`, alongside Local and WSL.
- Preserve the existing Space ownership rule: the Space owns its environment, root, tabs, and layout.
- Support SSH Agent, private key, password, and keyboard-interactive authentication.
- Resolve common OpenSSH config aliases and connection directives.
- Enforce OpenSSH known_hosts verification before authentication.
- Reuse the existing xterm renderer, renderer pool, DormantRing, and tab lifecycle.
- Keep SSH dormant at zero runtime cost until a remote Space is activated.
- Establish a connection abstraction that can later carry SFTP, remote exec, LSP, and forwarding channels.
- Keep the release binary increase near or below 1 MB, subject to a measured implementation gate.

## Non-goals

Phase 1 does not include:

- SFTP, remote Explorer, remote Editor, or remote file mutations
- remote Git, search, formatter, or language server execution
- local, remote, dynamic, X11, or Agent forwarding
- Windows SSH Server support
- persistent PTY processes across transport loss or app restart
- automatic tmux or zellij management
- remote shell integration installation or injection
- direct PKCS#11 or FIDO handling outside an available SSH Agent
- arbitrary remote command orchestration beyond connection setup and remote-home discovery

## Product model

### SSH profiles

An SSH profile is a reusable, non-secret connection definition. It can reference an OpenSSH host alias or define a host manually.

```ts
type SshProfile = {
  version: 1;
  id: string;
  name: string;
  source:
    | { kind: "openssh"; alias: string }
    | { kind: "manual"; host: string };
  overrides?: {
    host?: string;
    port?: number;
    user?: string;
    identityFiles?: string[];
    authOrder?: SshAuthMethod[];
    knownHostsFile?: string;
    strictHostKeyChecking?: boolean;
  };
};
```

Profiles are stored in a dedicated Tauri plugin store, separate from general preferences. The profile store contains aliases, hosts, ports, usernames, key paths, authentication preferences, and explicit overrides. It never contains passwords, private key contents, passphrases, or keyboard-interactive answers.

OpenSSH aliases are resolved again for every connection attempt. A stored profile therefore follows later changes to the user's SSH config instead of caching stale effective settings. Profile changes affect the next connection attempt and never mutate a live transport.

A profile referenced by any Space cannot be deleted. The UI lists the referencing Spaces and requires reassignment or deletion of those Spaces first. Deleting an unreferenced profile also deletes its remembered password entry. Versioned profile coercion rejects invalid or corrupt records instead of partially loading them.

### Workspace environment

`WorkspaceEnv` gains an SSH variant:

```ts
export type WorkspaceEnv =
  | { kind: "local" }
  | { kind: "wsl"; distro: string }
  | { kind: "ssh"; profileId: string };
```

`workspaceScopeKey()` uses the stable profile id. It must not use hostname, username, or other mutable profile fields.

A remote Space persists:

- its SSH `profileId`
- its canonical remote-home root when discovery succeeds
- its name and color
- its serialized terminal tabs and split-pane layout

A remote Space does not persist a live connection, authentication challenge, or PTY channel. Two Spaces that reference the same profile still own separate transports because connection lifecycle and terminal ownership remain Space-scoped.

### Remote root

After authentication, Rust opens a short exec channel to discover the remote POSIX home with a non-interactive command. A successful canonical path becomes the Space root. If discovery fails, the Space remains rootless but may still open a terminal.

Phase 1 always starts the remote shell in the SSH server's default home. Root browsing and arbitrary remote-root selection belong to the SFTP phase.

## User experience

### Profile management

Settings gains an SSH section with two profile sources:

1. OpenSSH config aliases
2. Terax-managed manual profiles

The primary view is a compact profile list. Selecting a profile shows an effective connection summary. Manual fields and profile overrides use progressive disclosure. Advanced fields stay collapsed until requested.

An OpenSSH alias preview shows resolved host, port, user, authentication order, identity files, proxy behavior, and known_hosts path. Unsupported directives that affect the selected alias appear as actionable errors or warnings.

### Creating a remote Space

The Space Switcher keeps its existing compact layout and `New space` action. The creation surface asks the user to choose Local, WSL, or SSH. The SSH path focuses on one task:

1. choose OpenSSH config, saved profile, or new connection
2. choose a profile
3. review a concise effective connection summary
4. create and connect

There is one primary action. Authentication details, manual fields, host trust, and advanced overrides do not compete with profile selection in the initial hierarchy.

### Activation

Restoring a remote Space does not resolve DNS, open sockets, access an Agent, read private keys, or prompt for credentials. The first terminal activation starts the connection flow.

A connected remote Space stays connected while it owns live PTY channels, including while another Space is visible. This preserves background terminal output. If a disconnected remote Space is inactive, it stays paused until the user activates it again.

### Phase 1 unavailable surfaces

Explorer, Source Control, remote search, and remote Editor operations are disabled for an SSH Space in Phase 1. Their empty states explain that remote file and exec capabilities are not available yet. They must not fall through to local filesystem or Git commands.

Dragging a local file into an SSH terminal must not paste a meaningless local path. The action is rejected with a concise message until remote upload exists.

### Terminal behavior

Each terminal leaf opens an independent SSH PTY channel on the Space connection. The channel requests:

- `xterm-256color`
- current terminal rows and columns
- the server's default interactive shell
- optional environment hints only when the server accepts them

The existing xterm instance, renderer pooling, scrollback, hidden-tab behavior, DormantRing, clipboard, search, and keyboard handling remain unchanged.

Remote shell integration is not installed or injected in Phase 1. SSH terminals use standard terminal mode. Block-mode features that require Terax OSC 133 integration remain disabled unless a trusted, already-configured remote environment provides compatible markers.

## Architecture

### Frontend modules

Create `src/modules/remote/` to own:

- profile types and profile persistence
- effective-profile preview types
- connection status and challenge reducers
- Remote Space creation UI
- Settings SSH section
- trust, authentication, connection-error, and Session-lost UI
- typed wrappers for SSH Tauri commands

Do not add SSH profile logic to the already large `src/modules/settings/store.ts`. `App.tsx` remains a coordinator.

Extend the existing modules only at their ownership boundaries:

- `src/modules/workspace/env.ts` for the SSH environment variant and scope key
- `src/modules/spaces/` for SSH Space creation, persistence, activation, root handling, and migrations
- `src/modules/terminal/` for transport selection and SSH-specific terminal bridge wiring
- `src/modules/statusbar/` for remote environment and connection status presentation

Introduce a small terminal transport interface used by `useTerminalSession`:

```ts
type TerminalTransport = {
  write(data: string): void;
  resize(cols: number, rows: number): void;
  close(): Promise<void>;
  onData(handler: (data: Uint8Array) => void): Unsubscribe;
  onExit(handler: (exit: TerminalExit) => void): Unsubscribe;
};
```

Local and WSL terminals keep using `pty-bridge.ts`. SSH terminals use an `ssh-bridge.ts` implementation. Transport selection depends on the owning Space environment, never the currently visible global environment.

### Rust modules

Create `src-tauri/src/modules/ssh/`:

- `config.rs`: profile validation and OpenSSH config resolution
- `known_hosts.rs`: trust-file loading, key matching, fingerprints, and atomic user-file updates
- `auth.rs`: Agent, key, password, and keyboard-interactive sequencing
- `manager.rs`: Space connection state, lazy connection, channel registry, keepalive, and cleanup
- `channel.rs`: PTY channel open, data, resize, exit, and close
- `errors.rs`: stable error codes and retry classification
- `mod.rs`: thin Tauri command shell

Register one `SshState` in `src-tauri/src/lib.rs`. The state owns an async map from Space id to connection entry. Each ready entry owns one authenticated `russh` handle and a bounded set of PTY channels.

Phase 1 adds `russh` and `russh-config`. It does not add `russh-sftp` until the SFTP phase needs it.

No new Tauri plugin is required. Custom commands follow the existing `invoke_handler` pattern. If implementation introduces any plugin API, its permissions must be added to `src-tauri/capabilities/default.json` before use.

### Connection state

A remote Space connection uses an explicit state machine:

```text
Dormant
  -> ResolvingProfile
  -> Connecting
  -> VerifyingHostKey
  -> AwaitingTrust | Authenticating
  -> AwaitingAuthResponse
  -> Ready
  -> Disconnecting
  -> Dormant
```

Terminal channels have a separate lifecycle:

```text
Opening -> Live -> Exited
                -> Lost
```

Connection and channel states are typed. UI code does not infer state by parsing error strings or terminal output.

### IPC surface

The exact names may be refined during planning, but the responsibilities remain separate:

- list or preview OpenSSH aliases
- connect or cancel a Space connection
- respond to a correlated trust or authentication challenge
- disconnect a Space connection
- open, write, resize, and close a PTY channel
- query connection status for restored frontend state

Connection events and PTY output use Tauri `Channel` values, following the existing PTY and LSP bridge patterns. Every challenge includes a connection id and challenge id. Responses with stale or mismatched ids are rejected.

PTY output is binary and bounded. Secret responses are never emitted back through event channels.

## OpenSSH config contract

Phase 1 must support the common fields represented by the selected `russh-config` stack:

- `Host` matching
- `HostName`
- `User`
- `Port`
- `IdentityFile`
- `ProxyCommand`
- `ProxyJump`
- `AddKeysToAgent`
- `UserKnownHostsFile`
- `StrictHostKeyChecking`

`Include` support is required for ordinary user configs. It must be covered by the implementation spike and tests. Include resolution is limited to 8 levels, 64 files, and 1 MiB per file. Symlink cycles and repeated canonical paths are rejected.

OpenSSH supports many more directives than Phase 1. The resolver classifies unmatched directives as:

- irrelevant to this connection
- unsupported but safe to ignore with a visible warning
- connection-critical or security-sensitive and therefore blocking

The selected alias preview shows all warnings before connection. The implementation must not silently claim full OpenSSH compatibility.

Terax does not treat `StrictHostKeyChecking=no` as permission to bypass its trust policy. Unknown hosts still require explicit trust, and changed keys remain blocked. The effective-profile preview explains this stricter behavior.

`ProxyCommand` executes a local command from user-owned SSH config. Terax requires first-use consent before executing it. Consent is bound to a hash of the effective command and target profile, so any command change requires new approval. Dormant Space restore never executes a proxy command.

## Host trust

Host-key verification completes before user authentication.

Terax reads configured user known_hosts files and platform-standard system host files. A match proceeds without prompting.

For an unknown host, the UI displays:

- resolved host and port
- key algorithm
- SHA256 fingerprint
- source profile

The user may choose:

- `Trust once`, which lasts only for the current connection
- `Trust and save`, which atomically updates the selected user known_hosts file

The writer serializes concurrent updates, preserves existing permissions and unrelated entries, and avoids partial writes.

A key mismatch is a hard stop. The UI shows old and new fingerprints plus the matching file and line. Phase 1 does not provide a bypass button or automatically rewrite the entry.

## Authentication

The default authentication order is:

1. SSH Agent
2. resolved identity files
3. password
4. keyboard-interactive

An explicit profile preference may change the order. Each method runs only when the server advertises it and the profile permits it.

Agent authentication does not export private key material. Private key files are read by Rust. Encrypted keys request a passphrase only when needed.

Password, passphrase, and keyboard-interactive answers use correlated frontend challenges. The frontend cannot guarantee immediate string zeroization because JavaScript strings are garbage collected, but it must avoid persistence, logging, analytics, copies, and error interpolation. Rust secret buffers are cleared as soon as practical.

Passwords are saved only when the user explicitly selects remember. Saved values use the existing `secrets` boundary and a profile-scoped account key. Private-key passphrases and keyboard-interactive answers are transient in Phase 1.

Cancellation invalidates the current challenge and aborts the entire connection attempt.

The broker permits one in-flight challenge per connection, at most 16 prompts per keyboard-interactive round, 8 KiB of server text per prompt, and 64 KiB per response. Exceeding a bound aborts authentication with a typed protocol-limit error.

## Disconnect and retry behavior

Limited automatic retries apply only while establishing a connection or while a ready connection has no live PTY channels. The retry schedule is bounded to 1, 2, and 5 seconds.

Terax never automatically retries:

- host-key mismatch
- rejected or cancelled authentication
- invalid or unsupported blocking config
- missing identity files
- normal remote shell exit

An SSH transport cannot transparently restore a PTY channel. If a transport drops while a terminal is live:

- the pane keeps its existing buffer
- the pane enters `Session lost`
- Terax does not create a replacement shell automatically
- the user may choose `Reconnect with new shell`
- the UI states that the original remote process may still exist but is no longer attached

Users who need durable remote processes can run tmux or zellij themselves. Automatic multiplexer management is outside Phase 1.

## Error handling

Rust returns stable error codes with structured details. Expected categories include:

- profile invalid or missing
- config unsupported
- DNS failure
- network timeout or refusal
- proxy command rejected or failed
- host unknown
- host key mismatch
- Agent unavailable
- key read or decrypt failure
- authentication rejected
- keyboard-interactive cancelled
- PTY request rejected
- transport lost
- channel limit reached

User-visible messages provide the failed stage, target, safe corrective action, and retry availability. They never include passwords, passphrases, private key bytes, keyboard-interactive answers, or raw config containing secrets.

## Resource and performance rules

- No SSH socket, task, config parse, Agent access, or credential prompt occurs when no remote Space is active.
- SSH frontend UI is lazy-loaded when the SSH path is opened.
- Each connected Space owns one transport and multiple channels.
- A Space may own at most 32 live SSH PTY channels.
- Connection events, PTY events, and Rust-side output queues are bounded and cannot grow with an unresponsive webview.
- Hidden SSH terminals retain the existing renderer-slot policy and 1 MiB DormantRing limit per leaf.
- A connection with no channels is closed when no active operation owns it.
- App exit cancels challenges, closes channels, and disconnects every SSH transport.

Before implementation continues beyond the transport spike, compare release builds with and without the SSH dependencies. The target release-binary increase is approximately 1 MB or less. If the increase exceeds that target, stop and review crypto features, dependency flags, and the transport choice before proceeding.

## Testing strategy

### Rust unit tests

Cover pure and dependency-light functions for:

- profile validation and override precedence
- OpenSSH alias matching, bounded Include behavior, and cycle rejection
- supported, warning, and blocking directive classification
- strict host-key behavior even when config requests weaker checking
- authentication method ordering
- fingerprint formatting
- known_hosts match, unknown, mismatch, and port-specific entries
- retry classification and bounded schedule
- connection and channel state transitions
- challenge id validation, cancellation, and protocol limits
- ProxyCommand consent invalidation when the effective command changes
- secret redaction
- bounded channel output and the 32-channel limit

### Rust integration tests

Use an in-process test SSH server where practical. Do not depend on a developer's system sshd.

Cover:

- known, unknown, and changed host keys
- Agent-compatible public-key authentication
- encrypted private keys
- password authentication
- multi-prompt keyboard-interactive authentication
- PTY open, output, input, resize, exit, and close
- multiple PTY channels on one transport
- connection cancellation at every blocking stage
- transport loss with a live PTY
- ProxyJump and approved ProxyCommand paths
- atomic known_hosts updates under concurrency

### Frontend tests

Cover:

- SSH `WorkspaceEnv` parsing, migration, and scope keys
- profile store coercion, reference-safe deletion, and secret exclusion
- profile edits applying only to the next connection
- Remote Space creation and serialization
- cold restore with zero connection calls
- activation-triggered lazy connection
- stale challenge rejection
- trust and authentication reducers
- Session-lost buffer preservation
- manual new-shell behavior
- local and WSL terminal transport regression
- disabled remote Explorer, Git, search, and file-drop actions

### Cross-platform verification

Verify Terax clients on:

- macOS with Unix-domain SSH Agent
- Linux with Unix-domain SSH Agent
- Windows with supported Windows Agent endpoints

Each client must connect to Linux and macOS SSH servers using OpenSSH aliases, Agent, encrypted key, password, and keyboard-interactive methods applicable to that environment.

### Standard repository checks

Run:

```text
pnpm lint
pnpm check-types
pnpm test
cd src-tauri && cargo clippy --all-targets --locked -- -D warnings
cd src-tauri && cargo nextest run --locked
```

A release build and binary-size comparison are required before claiming Phase 1 complete.

## Implementation gates

Implementation proceeds only after these gates pass:

1. `russh` transport spike proves PTY, Agent, key, password, keyboard-interactive, host verification, and cancellation on all client platforms.
2. OpenSSH config spike proves common aliases, Include, ProxyJump, and consent-gated ProxyCommand.
3. Release-size measurement is reviewed against the approximate 1 MB target.
4. The functional-core state machines and trust rules have tests before UI wiring.
5. Local and WSL terminal regressions remain green after transport abstraction.

Failure at a gate stops the implementation for design review. It does not permit silently dropping an approved authentication or security requirement.

## Acceptance criteria

Phase 1 is complete when:

- a user can create an SSH profile from OpenSSH config or manual fields
- a user can create, restore, activate, and delete an SSH Space
- restoring dormant SSH Spaces performs no network or authentication work
- first activation verifies host identity before authentication
- Agent, private key, password, and keyboard-interactive authentication work as specified
- a connected Space opens multiple remote terminal panes over one transport
- terminal data, resize, exit, cancellation, and close behave correctly
- a dropped live PTY preserves its buffer and requires explicit new-shell creation
- unsupported remote-only surfaces cannot access local resources by mistake
- known_hosts mismatch is never bypassed or overwritten automatically
- secrets are neither persisted outside the secret store nor included in logs or errors
- all unit, integration, lint, type, clippy, and release-size checks pass

## Future phases

Later designs may build on the same Space connection manager in this order:

1. SFTP-backed Explorer, Editor, remote root selection, and conflict-safe saves
2. remote exec for Git, search, formatting, and language servers
3. local, remote, and dynamic port forwarding
4. Windows SSH Server shell and path support
5. optional persistent-session integration designed separately from raw SSH reconnect

# SSH Remote Space Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SSH as a first-class, lazy Space environment that safely opens Linux and macOS remote terminal sessions from Terax clients on macOS, Linux, and Windows.

**Architecture:** Rust owns OpenSSH config resolution, host trust, authentication, one `russh` transport per connected Space, and multiplexed PTY channels. The frontend owns non-secret profile metadata, Remote Space UX, typed challenge state, and selection of a local PTY or SSH terminal transport while preserving the existing xterm renderer pool.

**Tech Stack:** Rust 2021, Tauri 2, `russh` 0.63.1 with `ring` and `rsa`, `russh-config` 0.58.0, Tokio, React 19, TypeScript 6, Zustand, Tauri plugin store, Vitest, Cargo nextest.

**Spec:** `docs/superpowers/specs/2026-08-31-ssh-remote-space-phase-1-design.md`

## Global Constraints

- Use `pnpm` only. Never use npm, npx, or yarn.
- Frontend imports across modules use `@/` aliases.
- Do not add em dash characters or emojis to code, comments, docs, or commits.
- Keep `App.tsx`, Tauri commands, and React components thin. Put decisions in pure, dependency-light functions.
- Rust owns network access, SSH config files, known_hosts, Agent access, key files, and secret retrieval.
- SSH stays dormant until a remote terminal is activated. Local and WSL startup behavior must not change.
- Terax clients support macOS, Linux, and Windows. Phase 1 servers support Linux and macOS only.
- Unknown hosts require explicit trust. Changed host keys are always blocked.
- A live PTY lost with its transport is never silently replaced. The buffer remains and a new shell requires user action.
- One Space owns one SSH transport and at most 32 live PTY channels.
- `russh-sftp`, remote files, remote Git, LSP, port forwarding, Windows SSH Server, and automatic tmux or zellij are out of scope.
- Do not begin Task 6 until the Task 5 gate is explicitly recorded as PASS by the parent agent.
- Each task is executed by a fresh implementation subagent and ends with parent review plus the listed focused checks.
- Pushes and PRs, if separately authorized later, target `fork` only. Never recreate or use `origin`.

---

## File Structure

### Rust files to create

- `src-tauri/src/modules/ssh/mod.rs`: `SshState` and thin Tauri command exports.
- `src-tauri/src/modules/ssh/types.rs`: serializable profiles, connection events, challenges, PTY events, phases, and ids.
- `src-tauri/src/modules/ssh/errors.rs`: stable error codes, redacted error details, and retry classification.
- `src-tauri/src/modules/ssh/limits.rs`: centralized config, challenge, channel, and queue limits.
- `src-tauri/src/modules/ssh/config.rs`: bounded OpenSSH config loading, profile resolution, directive classification, and ProxyCommand consent hashes.
- `src-tauri/src/modules/ssh/known_hosts.rs`: host-key decisions, SHA256 fingerprints, known_hosts reads, and atomic user-file updates.
- `src-tauri/src/modules/ssh/auth.rs`: authentication ordering, Agent and key handling, correlated challenges, cancellation, and remembered-password lookup.
- `src-tauri/src/modules/ssh/transport.rs`: `russh` client handler and authenticated transport wrapper.
- `src-tauri/src/modules/ssh/manager.rs`: Space-scoped connection state and lifecycle.
- `src-tauri/src/modules/ssh/channel.rs`: remote-home discovery and PTY channel open, write, resize, close, and loss handling.
- `src-tauri/src/modules/ssh/test_server.rs`: test-only in-process SSH server and fixtures.

### Frontend files to create

- `src/modules/remote/index.ts`: public barrel.
- `src/modules/remote/lib/types.ts`: frontend profile, status, challenge, and event types.
- `src/modules/remote/lib/profileStore.ts`: dedicated `terax-ssh-profiles.json` store and coercion.
- `src/modules/remote/lib/profileStore.test.ts`: profile coercion, reference-safe deletion, and secret exclusion tests.
- `src/modules/remote/lib/profileSecrets.ts`: profile-scoped password account names and existing secret-command wrappers.
- `src/modules/remote/lib/connectionState.ts`: pure connection and challenge reducer.
- `src/modules/remote/lib/connectionState.test.ts`: stale challenge, cancellation, loss, and retry tests.
- `src/modules/remote/lib/remoteSpaceViewModel.ts`: pure creation-step and challenge-action derivation.
- `src/modules/remote/lib/remoteSpaceViewModel.test.ts`: creation hierarchy and trust-action tests.
- `src/modules/remote/lib/remoteBoundaries.ts`: pure local-only surface gate for SSH environments.
- `src/modules/remote/lib/remoteBoundaries.test.ts`: Explorer, Git, search, and file-drop boundary tests.
- `src/modules/remote/lib/ipc.ts`: typed Tauri commands and binary channel wrappers.
- `src/modules/remote/lib/ipc.test.ts`: invoke payload and handler-release tests.
- `src/modules/remote/components/RemoteSpaceDialog.tsx`: focused SSH Space creation dialog matching the approved v2 hierarchy.
- `src/modules/remote/components/SshProfilePicker.tsx`: OpenSSH, saved, and manual connection choices.
- `src/modules/remote/components/SshChallengeDialog.tsx`: unknown-host, password, passphrase, and keyboard-interactive challenges.
- `src/modules/remote/components/SshSessionLost.tsx`: preserved-buffer overlay and explicit new-shell action.
- `src/modules/remote/components/RemoteUnavailablePanel.tsx`: Phase 1 Explorer and Source Control empty state.
- `src/settings/sections/SshSection.tsx`: profile management and effective-profile preview.
- `src/modules/terminal/lib/terminal-transport.ts`: shared terminal session interface and local or SSH transport selection.
- `src/modules/terminal/lib/terminal-transport.test.ts`: environment routing, local id behavior, and SSH block-mode tests.
- `src/modules/terminal/lib/ssh-bridge.ts`: SSH PTY IPC implementation.

### Existing files expected to change

- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- `src-tauri/src/modules/mod.rs`, `src-tauri/src/lib.rs`
- `src-tauri/src/modules/secrets.rs`, `src-tauri/src/modules/workspace.rs`
- `src/modules/workspace/env.ts`, `src/modules/workspace/index.ts`
- `src/modules/spaces/lib/store.ts`, `src/modules/spaces/lib/store.test.ts`
- `src/modules/spaces/lib/useSpaces.ts`, `src/modules/spaces/lib/useSpacesBoot.ts`, `src/modules/spaces/lib/useSpacesBoot.test.ts`
- `src/modules/spaces/lib/rootValidation.ts`, `src/modules/spaces/lib/rootValidation.test.ts`
- `src/modules/spaces/lib/spaceController.ts`, `src/modules/spaces/lib/spaceController.test.ts`
- `src/modules/spaces/SpaceSwitcher.tsx`, `src/modules/spaces/index.ts`
- `src/app/hooks/useWorkspaceSwitcher.ts`, `src/app/App.tsx`, `src/app/components/WorkspaceSurface.tsx`
- `src/modules/terminal/lib/pty-bridge.ts`, `src/modules/terminal/lib/useTerminalSession.ts`
- `src/modules/terminal/TerminalPane.tsx`, `src/modules/terminal/PaneTreeView.tsx`, `src/modules/terminal/TerminalStack.tsx`
- `src/modules/statusbar/StatusBar.tsx`, `src/modules/statusbar/WorkspaceEnvSelector.tsx`
- `src/modules/source-control/useSourceControlContext.ts`
- `src/modules/explorer/lib/useFileTree.ts`, `src/modules/explorer/lib/useTerminalFileDrop.ts`
- `src/modules/settings/openSettingsWindow.ts`, `src/settings/SettingsApp.tsx`
- `TERAX.md`, `ROADMAP.md`

---

### Task 1: Pin SSH Dependencies and Define the Shared Rust Contract

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/modules/mod.rs`
- Create: `src-tauri/src/modules/ssh/mod.rs`
- Create: `src-tauri/src/modules/ssh/types.rs`
- Create: `src-tauri/src/modules/ssh/errors.rs`
- Create: `src-tauri/src/modules/ssh/limits.rs`

**Interfaces:**

- Consumes: existing Tauri `Channel` serialization conventions and `serde`.
- Produces: `SshProfileInput`, `ResolvedSshConfig`, `ConnectionId`, `SshConnectionEvent`, `SshChallenge`, `SshError`, `SshErrorCode`, `retry_delays`, and all centralized hard limits.

- [ ] **Step 1: Record the pre-SSH release baseline**

Run from the repository root:

```bash
cd src-tauri
cargo build --release --locked
cd ..
node -e 'const fs=require("fs"); const p=process.platform==="win32"?"src-tauri/target/release/terax.exe":"src-tauri/target/release/terax"; console.log(JSON.stringify({path:p,bytes:fs.statSync(p).size}))'
```

Expected: release build exits 0 and prints one JSON object with the baseline byte count. Preserve the count in the task report before editing `Cargo.toml`.

- [ ] **Step 2: Add failing contract tests**

In `errors.rs` and `limits.rs`, add tests before implementation:

```rust
#[test]
fn auth_and_host_key_errors_do_not_retry() {
    assert!(!SshErrorCode::AuthenticationRejected.is_retryable());
    assert!(!SshErrorCode::HostKeyMismatch.is_retryable());
    assert_eq!(retry_delays(SshErrorCode::NetworkTimeout), [1, 2, 5]);
}

#[test]
fn protocol_limits_match_the_spec() {
    assert_eq!(MAX_CHANNELS_PER_SPACE, 32);
    assert_eq!(MAX_AUTH_PROMPTS, 16);
    assert_eq!(MAX_PROMPT_BYTES, 8 * 1024);
    assert_eq!(MAX_RESPONSE_BYTES, 64 * 1024);
    assert_eq!(MAX_INCLUDE_DEPTH, 8);
    assert_eq!(MAX_INCLUDE_FILES, 64);
    assert_eq!(MAX_CONFIG_FILE_BYTES, 1024 * 1024);
}
```

- [ ] **Step 3: Run the tests and verify the red state**

Run:

```bash
cd src-tauri
cargo test --locked modules::ssh
```

Expected: compilation fails because the module and constants do not exist.

- [ ] **Step 4: Add size-conscious dependencies**

Add to `[dependencies]`:

```toml
russh = { version = "0.63.1", default-features = false, features = ["ring", "rsa"] }
russh-config = "0.58.0"
thiserror = "2"
zeroize = "1"
sha2 = "0.10"
base64 = "0.22"
tokio-util = { version = "0.7", default-features = false, features = ["rt"] }
tokio = { version = "1", default-features = false, features = [
  "io-util",
  "net",
  "process",
  "rt",
  "sync",
  "time",
] }
```

Replace the existing Tokio entry instead of adding a second entry. Do not enable `russh` default `aws-lc-rs` or `flate2` features in Phase 1.

- [ ] **Step 5: Implement stable types and limits**

Use ids and enums that can cross Tauri without string parsing:

```rust
pub type ConnectionId = u64;
pub type ChannelId = u32;
pub type ChallengeId = u64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshProfileInput {
    pub id: String,
    pub name: String,
    pub source: SshProfileSource,
    pub overrides: Option<SshProfileOverrides>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionPhase {
    Dormant,
    ResolvingProfile,
    Connecting,
    VerifyingHostKey,
    AwaitingTrust,
    Authenticating,
    AwaitingAuthResponse,
    Ready,
    Disconnecting,
    Lost,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SshErrorCode {
    ProfileInvalid,
    ConfigUnsupported,
    DnsFailure,
    NetworkTimeout,
    ConnectionRefused,
    ProxyCommandDenied,
    ProxyCommandFailed,
    HostUnknown,
    HostKeyMismatch,
    AgentUnavailable,
    KeyReadFailed,
    KeyDecryptFailed,
    AuthenticationRejected,
    ChallengeCancelled,
    PtyRejected,
    TransportLost,
    ChannelLimitReached,
    ProtocolLimitExceeded,
}
```

`SshError` contains `code`, `stage`, `target`, `message`, and `retryable`. It never stores raw secret values or unredacted config.

- [ ] **Step 6: Run focused checks**

Run:

```bash
cd src-tauri
cargo fmt --check
cargo test --locked modules::ssh::errors::tests
cargo test --locked modules::ssh::limits::tests
cargo check --locked
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/modules/mod.rs src-tauri/src/modules/ssh
git commit -m "feat(ssh): define remote connection contract"
```

---

### Task 2: Resolve Profiles and Bounded OpenSSH Config

**Files:**

- Create: `src-tauri/src/modules/ssh/config.rs`
- Modify: `src-tauri/src/modules/ssh/types.rs`
- Modify: `src-tauri/src/modules/ssh/errors.rs`
- Modify: `src-tauri/src/modules/ssh/mod.rs`

**Interfaces:**

- Consumes: `SshProfileInput`, `SshError`, config limits.
- Produces: `resolve_profile(profile, context) -> Result<ResolvedSshConfig, SshError>`, `classify_directive`, `proxy_consent_hash`, and effective-profile warnings.

- [ ] **Step 1: Write failing resolver tests**

Use `tempfile::TempDir` and table-driven fixtures:

```rust
#[test]
fn manual_overrides_win_over_openssh_alias() {
    let home = fixture_home("Host prod\n  HostName old.example\n  User deploy\n  Port 22\n");
    let profile = openssh_profile("prod").with_host("new.example").with_port(2202);
    let out = resolve_profile(&profile, &ResolveContext::for_home(home.path())).unwrap();
    assert_eq!(out.host, "new.example");
    assert_eq!(out.user, "deploy");
    assert_eq!(out.port, 2202);
}

#[test]
fn include_cycles_and_limits_are_rejected() {
    let home = cyclic_include_fixture();
    let err = resolve_profile(&openssh_profile("prod"), &ResolveContext::for_home(home.path())).unwrap_err();
    assert_eq!(err.code, SshErrorCode::ConfigUnsupported);
}

#[test]
fn weaker_strict_host_checking_never_disables_terax_trust() {
    let out = resolve_text("Host prod\n StrictHostKeyChecking no\n", "prod").unwrap();
    assert!(out.warnings.iter().any(|w| w.code == "strict-host-key-overridden"));
    assert!(out.strict_host_key_checking);
}
```

- [ ] **Step 2: Verify the tests fail**

Run:

```bash
cd src-tauri
cargo test --locked modules::ssh::config::tests
```

Expected: compilation fails because `resolve_profile` and fixtures do not exist.

- [ ] **Step 3: Implement bounded config loading**

Implement an injectable loader so file traversal remains pure and testable:

```rust
pub trait ConfigFiles: Send + Sync {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_limited(&self, path: &Path, max_bytes: usize) -> io::Result<String>;
    fn expand_glob(&self, pattern: &Path) -> io::Result<Vec<PathBuf>>;
}

pub struct ResolveContext<'a> {
    pub home: &'a Path,
    pub system_config: Option<&'a Path>,
    pub files: &'a dyn ConfigFiles,
}
```

Track canonical paths, depth, file count, and size before calling `russh_config` parsing helpers. Reject cycles, traversal beyond approved config roots, and limit excess with `ConfigUnsupported`.

- [ ] **Step 4: Implement explicit directive classification**

Return one of:

```rust
pub enum DirectiveSupport {
    Supported,
    Warning(&'static str),
    Blocking(&'static str),
}
```

Support `Host`, `Include`, `HostName`, `User`, `Port`, `IdentityFile`, `ProxyCommand`, `ProxyJump`, `AddKeysToAgent`, `UserKnownHostsFile`, and `StrictHostKeyChecking`. Classify connection-critical unsupported directives as blocking. Keep unrelated directives visible as warnings.

- [ ] **Step 5: Bind ProxyCommand consent to effective content**

```rust
pub fn proxy_consent_hash(profile_id: &str, target: &str, command: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(profile_id.as_bytes());
    hasher.update([0]);
    hasher.update(target.as_bytes());
    hasher.update([0]);
    hasher.update(command.as_bytes());
    format!("sha256:{}", BASE64_STANDARD_NO_PAD.encode(hasher.finalize()))
}
```

A changed command produces a new hash and therefore new consent.

- [ ] **Step 6: Run focused checks**

```bash
cd src-tauri
cargo fmt --check
cargo test --locked modules::ssh::config::tests
cargo clippy --lib --locked -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/modules/ssh
git commit -m "feat(ssh): resolve bounded OpenSSH profiles"
```

---

### Task 3: Enforce Host Trust and Atomic known_hosts Updates

**Files:**

- Create: `src-tauri/src/modules/ssh/known_hosts.rs`
- Modify: `src-tauri/src/modules/ssh/types.rs`
- Modify: `src-tauri/src/modules/ssh/errors.rs`
- Modify: `src-tauri/src/modules/ssh/mod.rs`

**Interfaces:**

- Consumes: resolved target and known_hosts paths.
- Produces: `check_host_key`, `host_fingerprint`, `save_host_key`, `HostKeyDecision`, and host trust challenge payloads.

- [ ] **Step 1: Write failing host trust tests**

```rust
#[test]
fn known_unknown_and_changed_keys_are_distinct() {
    let store = fixture_known_hosts("prod.example ssh-ed25519 AAAA-known\n");
    assert_eq!(store.check("prod.example", 22, &key("AAAA-known")), HostKeyDecision::Match);
    assert_eq!(store.check("new.example", 22, &key("AAAA-new")), HostKeyDecision::Unknown);
    assert!(matches!(store.check("prod.example", 22, &key("AAAA-new")), HostKeyDecision::Mismatch { .. }));
}

#[test]
fn nonstandard_ports_use_bracketed_host_form() {
    assert_eq!(known_host_name("prod.example", 2202), "[prod.example]:2202");
}

#[test]
fn concurrent_save_preserves_unrelated_entries_and_mode() {
    let fixture = writable_known_hosts(0o600);
    save_host_key(&fixture, entry("one.example", 22, key_one())).unwrap();
    save_host_key(&fixture, entry("two.example", 22, key_two())).unwrap();
    assert_eq!(fixture.mode(), 0o600);
    assert!(fixture.text().contains("one.example"));
    assert!(fixture.text().contains("two.example"));
}
```

- [ ] **Step 2: Verify red state**

```bash
cd src-tauri
cargo test --locked modules::ssh::known_hosts::tests
```

Expected: compilation fails because trust functions do not exist.

- [ ] **Step 3: Implement fingerprints and decisions**

Represent a presented key without leaking parser internals:

```rust
pub struct PresentedHostKey {
    pub algorithm: String,
    pub blob: Vec<u8>,
}

pub enum HostKeyDecision {
    Match,
    Unknown,
    Mismatch {
        old_fingerprint: String,
        new_fingerprint: String,
        file: PathBuf,
        line: usize,
    },
}
```

Compute OpenSSH-style `SHA256:<base64-no-pad>` over the key blob. Read user and platform-standard system host files, including hashed-host entries supported by the chosen parser.

- [ ] **Step 4: Implement serialized atomic writes**

Use a process-wide async mutex per canonical target file. Write a sibling temporary file, copy existing permissions, flush, sync, and rename. Never rewrite a mismatch. Only an explicit `TrustAndSave` response reaches `save_host_key`.

- [ ] **Step 5: Run focused checks**

```bash
cd src-tauri
cargo fmt --check
cargo test --locked modules::ssh::known_hosts::tests
cargo clippy --lib --locked -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/modules/ssh
git commit -m "feat(ssh): enforce known host trust"
```

---

### Task 4: Implement Authentication Broker and Secret Boundary

**Files:**

- Create: `src-tauri/src/modules/ssh/auth.rs`
- Modify: `src-tauri/src/modules/ssh/types.rs`
- Modify: `src-tauri/src/modules/ssh/errors.rs`
- Modify: `src-tauri/src/modules/ssh/mod.rs`
- Modify: `src-tauri/src/modules/secrets.rs`

**Interfaces:**

- Consumes: `ResolvedSshConfig`, `russh::client::Handle`, existing `SecretsState`.
- Produces: `AuthBroker`, `AuthChallenge`, `AuthResponse`, `auth_order`, and internal `secrets::get_value`, `set_value`, `delete_value` helpers shared by commands and SSH.

- [ ] **Step 1: Write failing pure authentication tests**

```rust
#[test]
fn default_order_is_agent_key_password_keyboard_interactive() {
    assert_eq!(
        auth_order(None),
        [AuthMethod::Agent, AuthMethod::PrivateKey, AuthMethod::Password, AuthMethod::KeyboardInteractive]
    );
}

#[test]
fn stale_challenge_and_oversized_round_are_rejected() {
    let mut broker = AuthBroker::new(ConnectionId::from(7));
    let challenge = broker.begin_keyboard_interactive(vec![prompt("Password:", false)]).unwrap();
    assert_eq!(broker.respond(challenge.id + 1, vec![secret("x")]).unwrap_err().code, SshErrorCode::ChallengeCancelled);
    assert_eq!(broker.begin_keyboard_interactive(vec![prompt("x", false); 17]).unwrap_err().code, SshErrorCode::ProtocolLimitExceeded);
}

#[test]
fn cancellation_zeroizes_pending_responses() {
    let mut broker = AuthBroker::new(ConnectionId::from(9));
    let id = broker.begin_password().unwrap().id;
    broker.cancel(id).unwrap();
    assert!(broker.pending().is_none());
}
```

- [ ] **Step 2: Verify red state**

```bash
cd src-tauri
cargo test --locked modules::ssh::auth::tests
```

Expected: compilation fails because the broker does not exist.

- [ ] **Step 3: Refactor secret commands through internal helpers**

In `secrets.rs`, preserve command behavior while exposing crate-internal helpers:

```rust
pub(crate) async fn get_value(
    app: &AppHandle,
    state: &SecretsState,
    service: &str,
    account: &str,
) -> Result<Option<String>, String>;

pub(crate) async fn set_value(
    app: &AppHandle,
    state: &SecretsState,
    service: &str,
    account: &str,
    value: &str,
) -> Result<(), String>;

pub(crate) async fn delete_value(
    app: &AppHandle,
    state: &SecretsState,
    service: &str,
    account: &str,
) -> Result<(), String>;
```

Make existing `secrets_get`, `secrets_set`, and `secrets_delete` delegate to these helpers. Add regression tests that command storage semantics and Linux 0600 behavior remain unchanged.

- [ ] **Step 4: Implement authentication methods**

Implement methods in explicit order. Agent authentication enumerates identities and never exports key bytes. Private-key authentication reads each configured key, requests a passphrase only for encrypted keys, and drops the decrypted key after use. Password and keyboard-interactive methods use one correlated challenge at a time.

Profile-scoped remembered passwords use:

```rust
pub fn password_account(profile_id: &str) -> String {
    format!("ssh:{profile_id}:password")
}
```

Only a response with `remember: true` calls `set_value`.

- [ ] **Step 5: Add platform Agent adapters**

Unix uses `SSH_AUTH_SOCK`. Windows supports the available OpenSSH named pipe endpoint and returns `AgentUnavailable` when no supported endpoint exists. Put endpoint discovery behind an injected trait so tests use a fake Agent.

- [ ] **Step 6: Run focused checks**

```bash
cd src-tauri
cargo fmt --check
cargo test --locked modules::ssh::auth::tests
cargo test --locked modules::secrets::tests
cargo clippy --lib --locked -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/modules/ssh src-tauri/src/modules/secrets.rs
git commit -m "feat(ssh): broker remote authentication"
```

---

### Task 5: Build the Space Connection Manager, PTY Channels, and Mandatory Gate

**Files:**

- Create: `src-tauri/src/modules/ssh/transport.rs`
- Create: `src-tauri/src/modules/ssh/manager.rs`
- Create: `src-tauri/src/modules/ssh/channel.rs`
- Create: `src-tauri/src/modules/ssh/test_server.rs`
- Modify: `src-tauri/src/modules/ssh/mod.rs`
- Modify: `src-tauri/src/modules/ssh/types.rs`
- Modify: `src-tauri/src/modules/ssh/errors.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

- Consumes: resolver, trust, authentication, `russh` client API.
- Produces: `SshState`, `connect_space`, `disconnect_space`, `open_pty`, `write_pty`, `resize_pty`, `close_pty`, `discover_remote_home`, and bounded Space or channel state.

- [ ] **Step 1: Write failing manager state tests**

```rust
#[tokio::test]
async fn one_space_owns_one_transport_and_multiple_channels() {
    let server = TestSshServer::password("user", "secret").await;
    let state = SshState::default();
    let connection = state.connect(test_request(&server)).await.unwrap();
    let one = state.open_pty(connection, size(80, 24)).await.unwrap();
    let two = state.open_pty(connection, size(100, 40)).await.unwrap();
    assert_ne!(one, two);
    assert_eq!(state.connection_count().await, 1);
    assert_eq!(state.channel_count(connection).await, 2);
}

#[tokio::test]
async fn thirty_third_channel_is_rejected() {
    let fixture = ready_state().await;
    for _ in 0..32 { fixture.open_pty().await.unwrap(); }
    assert_eq!(fixture.open_pty().await.unwrap_err().code, SshErrorCode::ChannelLimitReached);
}

#[tokio::test]
async fn transport_loss_marks_live_channels_lost_without_reopening() {
    let fixture = ready_state().await;
    let channel = fixture.open_pty().await.unwrap();
    fixture.server.drop_transport().await;
    assert_eq!(fixture.channel_phase(channel).await, ChannelPhase::Lost);
    assert_eq!(fixture.server.shell_open_count(), 1);
}
```

- [ ] **Step 2: Write failing PTY behavior tests**

```rust
#[tokio::test]
async fn pty_round_trips_bytes_resize_and_exit() {
    let fixture = ready_state().await;
    let channel = fixture.open_pty().await.unwrap();
    fixture.write(channel, b"printf terax-test\r").await.unwrap();
    assert!(fixture.read_until(channel, b"terax-test").await.is_ok());
    fixture.resize(channel, 132, 50).await.unwrap();
    assert_eq!(fixture.server.last_window_size(), (132, 50));
    fixture.close(channel).await.unwrap();
}

#[tokio::test]
async fn remote_home_failure_keeps_connection_ready() {
    let fixture = ready_state_with_home_command_failure().await;
    assert_eq!(fixture.discover_home().await.unwrap(), None);
    assert_eq!(fixture.phase().await, ConnectionPhase::Ready);
}
```

- [ ] **Step 3: Verify red state**

```bash
cd src-tauri
cargo test --locked modules::ssh
```

Expected: compilation fails because manager, channel, and test server do not exist.

- [ ] **Step 4: Implement the `russh` handler and transport**

The client handler performs host-key verification by delegating to `known_hosts.rs`. It emits a trust challenge for unknown keys and returns an error for mismatches. It does not authenticate before trust succeeds.

`AuthenticatedTransport` owns the `russh::client::Handle` and exposes session and direct-tcpip channel operations needed by Phase 1. Keep external API types out of manager state by wrapping them.

- [ ] **Step 5: Implement `SshState`**

Use an async map keyed by Space id:

```rust
#[derive(Default)]
pub struct SshState {
    next_connection_id: AtomicU64,
    connections: tokio::sync::RwLock<HashMap<String, Arc<SpaceConnection>>>,
}

pub struct SpaceConnection {
    pub id: ConnectionId,
    pub profile_id: String,
    pub phase: tokio::sync::watch::Sender<ConnectionPhase>,
    pub transport: AuthenticatedTransport,
    pub channels: tokio::sync::RwLock<HashMap<ChannelId, RemotePty>>,
    pub cancel: tokio_util::sync::CancellationToken,
}
```

Use the Task 1 pinned `tokio-util` dependency. Its release-size effect is part of the mandatory gate.

- [ ] **Step 6: Implement PTY channels and bounded events**

Request `xterm-256color`, rows, columns, and the server default shell. Keep a bounded per-channel event queue. When the webview cannot consume events, close the channel with `ProtocolLimitExceeded` instead of accumulating memory.

Remote-home discovery uses a short exec channel and returns `Option<String>` after validating a canonical absolute POSIX path.

- [ ] **Step 7: Register state and exit cleanup**

Add `.manage(ssh::SshState::default())` in `lib.rs`. On `RunEvent::Exit`, close all connections alongside existing PTY and shell cleanup. Do not register frontend-facing commands until Task 6.

- [ ] **Step 8: Run the complete mandatory gate**

Run:

```bash
cd src-tauri
cargo fmt --check
cargo test --locked modules::ssh
cargo clippy --lib --all-targets --locked -- -D warnings
cargo build --release --locked
cd ..
node -e 'const fs=require("fs"); const p=process.platform==="win32"?"src-tauri/target/release/terax.exe":"src-tauri/target/release/terax"; console.log(JSON.stringify({path:p,bytes:fs.statSync(p).size}))'
```

Expected:

- all focused SSH tests pass
- clippy exits 0
- release build exits 0
- parent compares the byte count to Task 1 baseline
- release increase is approximately 1 MB or less

Also run available cross-target checks without installing targets implicitly:

```bash
cd src-tauri
rustup target list --installed
cargo check --locked --target x86_64-pc-windows-msvc
cargo check --locked --target x86_64-unknown-linux-gnu
```

Run only targets already installed. Missing targets are recorded as pending cross-platform evidence, not silently treated as passes.

- [ ] **Step 9: Parent records PASS or BLOCKED**

PASS requires basic PTY, host trust, Agent adapter compilation, private key, password, keyboard-interactive, cancellation, ProxyJump or ProxyCommand transport path, and size evidence. If any approved capability cannot be supported, stop. Do not begin Task 6 and do not delete the requirement.

- [ ] **Step 10: Commit after PASS**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/modules/ssh
git commit -m "feat(ssh): manage remote PTY transports"
```

---

### Task 6: Expose Typed Tauri IPC and Frontend Bridge

**Files:**

- Modify: `src-tauri/src/modules/ssh/mod.rs`
- Modify: `src-tauri/src/modules/ssh/types.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/modules/remote/index.ts`
- Create: `src/modules/remote/lib/types.ts`
- Create: `src/modules/remote/lib/ipc.ts`
- Create: `src/modules/remote/lib/ipc.test.ts`

**Interfaces:**

- Consumes: Task 5 manager and channels.
- Produces: commands `ssh_config_list_hosts`, `ssh_config_preview`, `ssh_connect`, `ssh_cancel_connect`, `ssh_respond_challenge`, `ssh_disconnect`, `ssh_open_pty`, `ssh_write`, `ssh_resize`, `ssh_close_pty`, and matching TypeScript wrappers.

- [ ] **Step 1: Write failing frontend IPC tests**

Mock `@tauri-apps/api/core`:

```ts
it("opens an SSH PTY with binary handlers and releases them on exit", async () => {
  invokeMock.mockResolvedValueOnce(41);
  const events: number[] = [];
  const session = await openSshPty("sp-1", 80, 24, {
    onData: () => {},
    onExit: (code) => events.push(code),
  });
  expect(invokeMock).toHaveBeenCalledWith(
    "ssh_open_pty",
    expect.objectContaining({ spaceId: "sp-1", cols: 80, rows: 24 }),
  );
  await session.close();
  expect(invokeMock).toHaveBeenCalledWith("ssh_close_pty", { channelId: 41 });
  expect(events).toEqual([]);
});

it("rejects events for stale connection ids", () => {
  const state = reduceConnection(initialConnection("sp-1"), readyEvent(99));
  expect(state.phase).toBe("dormant");
});
```

- [ ] **Step 2: Verify red state**

```bash
pnpm test src/modules/remote/lib/ipc.test.ts
```

Expected: test fails because remote IPC files do not exist.

- [ ] **Step 3: Implement thin Rust commands**

Each command validates ids, delegates once, and serializes typed errors. `ssh_write` follows the local PTY raw-body pattern using an `x-ssh-channel-id` header. Commands never hold Tauri state locks across channel sends or user challenges.

Connection events use `Channel<SshConnectionEvent>`. PTY bytes use `Channel<Vec<u8>>` or the existing `ArrayBuffer`-compatible serialization path. Exit events carry a typed reason, not only an integer exit code.

- [ ] **Step 4: Implement TypeScript wrappers**

Expose:

```ts
export type TerminalExit =
  | { kind: "exited"; code: number }
  | { kind: "lost"; error: SshClientError };

export type RemotePtySession = {
  kind: "ssh";
  id: number;
  write(data: string): Promise<void>;
  resize(cols: number, rows: number): Promise<void>;
  close(): Promise<void>;
};
```

Release every `Channel.onmessage` handler exactly once on exit, error, cancellation, or close.

- [ ] **Step 5: Run focused checks**

```bash
pnpm test src/modules/remote/lib/ipc.test.ts
pnpm check-types
cd src-tauri
cargo test --locked modules::ssh
cargo clippy --lib --locked -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/modules/ssh src/modules/remote
git commit -m "feat(ssh): expose remote connection IPC"
```

---

### Task 7: Persist Non-secret Profiles and Connection State

**Files:**

- Create: `src/modules/remote/lib/profileStore.ts`
- Create: `src/modules/remote/lib/profileStore.test.ts`
- Create: `src/modules/remote/lib/profileSecrets.ts`
- Create: `src/modules/remote/lib/connectionState.ts`
- Create: `src/modules/remote/lib/connectionState.test.ts`
- Modify: `src/modules/remote/lib/types.ts`
- Modify: `src/modules/remote/index.ts`

**Interfaces:**

- Consumes: Task 6 client types and existing `secrets_*` commands.
- Produces: `loadSshProfiles`, `saveSshProfile`, `deleteSshProfile`, `profileReferences`, `passwordAccount`, `reduceConnection`, and a Zustand-facing immutable state contract.

- [ ] **Step 1: Write failing profile tests**

```ts
it("coerces valid profiles and drops secret-shaped fields", () => {
  const [profile] = coerceProfiles([{ version: 1, id: "p1", name: "Prod", source: { kind: "manual", host: "prod.example" }, password: "leak" }]);
  expect(profile.id).toBe("p1");
  expect(profile).not.toHaveProperty("password");
});

it("blocks deletion while a Space references the profile", () => {
  expect(profileReferences("p1", [{ id: "s1", env: { kind: "ssh", profileId: "p1" } }])).toEqual(["s1"]);
  expect(canDeleteProfile("p1", [{ id: "s1", env: { kind: "ssh", profileId: "p1" } }])).toBe(false);
});

it("uses a stable profile-scoped password account", () => {
  expect(passwordAccount("p1")).toBe("ssh:p1:password");
});
```

- [ ] **Step 2: Write failing reducer tests**

```ts
it("keeps a lost session until the user requests a new shell", () => {
  const live = connectionState({ phase: "ready", connectionId: 7 });
  const lost = reduceConnection(live, { type: "transportLost", connectionId: 7, message: "network" });
  expect(lost.phase).toBe("lost");
  expect(lost.openNewShell).toBe(false);
});

it("ignores stale challenges", () => {
  const state = connectionState({ phase: "authenticating", connectionId: 8 });
  const next = reduceConnection(state, challengeEvent(7, 3));
  expect(next.challenge).toBeNull();
});
```

- [ ] **Step 3: Verify red state**

```bash
pnpm test src/modules/remote/lib/profileStore.test.ts src/modules/remote/lib/connectionState.test.ts
```

Expected: tests fail because modules do not exist.

- [ ] **Step 4: Implement the dedicated profile store**

Use `new LazyStore("terax-ssh-profiles.json", { defaults: {}, autoSave: 500 })`. Keep pure coercion separate from I/O. The persisted schema is:

```ts
type StoredProfiles = {
  schemaVersion: 1;
  profiles: SshProfile[];
  proxyConsent: Record<string, string>;
};
```

Deleting an unreferenced profile removes its remembered password through `secrets_delete`. Profile updates do not mutate live connection state.

- [ ] **Step 5: Implement the pure reducer**

Model dormant, resolving, connecting, trust, authenticating, ready, lost, and disconnecting phases. Require matching `connectionId` and `challengeId` before accepting an event or response.

- [ ] **Step 6: Run focused checks**

```bash
pnpm test src/modules/remote/lib/profileStore.test.ts src/modules/remote/lib/connectionState.test.ts
pnpm check-types
pnpm lint
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```bash
git add src/modules/remote
git commit -m "feat(ssh): persist remote profiles safely"
```

---

### Task 8: Add SSH to Workspace and Space Ownership

**Files:**

- Modify: `src/modules/workspace/env.ts`
- Modify: `src/modules/workspace/index.ts`
- Modify: `src-tauri/src/modules/workspace.rs`
- Modify: `src/modules/spaces/lib/store.ts`
- Modify: `src/modules/spaces/lib/store.test.ts`
- Modify: `src/modules/spaces/lib/useSpaces.ts`
- Modify: `src/modules/spaces/lib/useSpacesBoot.ts`
- Modify: `src/modules/spaces/lib/useSpacesBoot.test.ts`
- Modify: `src/modules/spaces/lib/rootValidation.ts`
- Modify: `src/modules/spaces/lib/rootValidation.test.ts`
- Modify: `src/modules/spaces/lib/spaceController.ts`
- Modify: `src/modules/spaces/lib/spaceController.test.ts`
- Modify: `src/app/hooks/useWorkspaceSwitcher.ts`

**Interfaces:**

- Consumes: profile ids and remote-home result.
- Produces: `{ kind: "ssh"; profileId: string }`, `ssh:<profileId>` scope keys, nullable Space creation roots, SSH-aware boot, and explicit rejection by local-only Rust commands.

- [ ] **Step 1: Write failing workspace and store tests**

```ts
it("round-trips an SSH workspace scope", () => {
  const env = { kind: "ssh", profileId: "prod" } as const;
  expect(workspaceScopeKey(env)).toBe("ssh:prod");
  expect(parseWorkspaceScopeKey("ssh:prod")).toEqual(env);
});

it("accepts persisted SSH environments and rejects empty profile ids", () => {
  const good = normalizeSpaceEnvs([rawSpace({ kind: "ssh", profileId: "prod" })], LOCAL_WORKSPACE);
  expect(good[0].env).toEqual({ kind: "ssh", profileId: "prod" });
  const bad = normalizeSpaceEnvs([rawSpace({ kind: "ssh", profileId: "" })], LOCAL_WORKSPACE);
  expect(bad[0].env).toEqual(LOCAL_WORKSPACE);
});

it("restores an SSH Space without validating its root through local fs", async () => {
  const deps = bootDepsWithSpace(sshSpace("prod", "/home/deploy"));
  await bootSpaces(deps);
  expect(deps.fsStat).not.toHaveBeenCalled();
  expect(deps.connectSsh).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Verify red state**

```bash
pnpm test src/modules/spaces/lib/store.test.ts src/modules/spaces/lib/useSpacesBoot.test.ts src/modules/spaces/lib/rootValidation.test.ts
```

Expected: tests fail because SSH env is not accepted.

- [ ] **Step 3: Add frontend environment support**

```ts
export type WorkspaceEnv =
  | { kind: "local" }
  | { kind: "wsl"; distro: string }
  | { kind: "ssh"; profileId: string };

export function workspaceScopeKey(env: WorkspaceEnv): string {
  if (env.kind === "wsl") return `wsl:${env.distro}`;
  if (env.kind === "ssh") return `ssh:${env.profileId}`;
  return "local";
}
```

Validate non-empty profile ids. `CreateInput.root` becomes `string | null`. SSH Space creation uses `root: null` until remote-home discovery succeeds.

- [ ] **Step 4: Prevent cold restore from connecting**

`useSpacesBoot` preserves SSH metadata and serialized terminal tabs but skips local root authorization and filesystem validation. `spaceController` treats SSH preparation as metadata-only. The terminal bridge owns first connection.

`useWorkspaceSwitcher` must not call `homeDir`, `wsl_home`, or `workspace_authorize` for SSH. Return a prepared SSH environment with its persisted nullable root.

- [ ] **Step 5: Add the Rust enum and reject SSH from local-only boundaries**

```rust
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WorkspaceEnv {
    #[default]
    Local,
    Wsl { distro: String },
    Ssh {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
}
```

Add `is_ssh()` and `require_local_or_wsl(operation)`. Audit every exhaustive match in workspace, fs, git, shell, LSP, and PTY modules. Local-only commands return a typed error for SSH instead of treating it as Local.

- [ ] **Step 6: Run focused and compile-guided checks**

```bash
pnpm test src/modules/spaces/lib/store.test.ts src/modules/spaces/lib/useSpacesBoot.test.ts src/modules/spaces/lib/rootValidation.test.ts src/modules/spaces/lib/spaceController.test.ts
pnpm check-types
cd src-tauri
cargo test --locked modules::workspace::tests
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
```

Expected: all commands exit 0 and Rust has no non-exhaustive environment matches.

- [ ] **Step 7: Commit**

```bash
git add src/modules/workspace src/modules/spaces src/app/hooks/useWorkspaceSwitcher.ts src-tauri/src/modules/workspace.rs src-tauri/src/modules/fs src-tauri/src/modules/git src-tauri/src/modules/lsp src-tauri/src/modules/pty src-tauri/src/modules/shell
git commit -m "feat(spaces): own SSH workspace environments"
```

---

### Task 9: Route Terminal Leaves Through Local or SSH Transports

**Files:**

- Create: `src/modules/terminal/lib/terminal-transport.ts`
- Create: `src/modules/terminal/lib/terminal-transport.test.ts`
- Create: `src/modules/terminal/lib/ssh-bridge.ts`
- Modify: `src/modules/terminal/lib/pty-bridge.ts`
- Modify: `src/modules/terminal/lib/useTerminalSession.ts`
- Modify: `src/modules/terminal/TerminalPane.tsx`
- Modify: `src/modules/terminal/PaneTreeView.tsx`
- Modify: `src/modules/terminal/TerminalStack.tsx`
- Modify: `src/app/components/WorkspaceSurface.tsx`
- Modify: `src/app/App.tsx`

**Interfaces:**

- Consumes: explicit owning `WorkspaceEnv`, local `openPty`, remote `openSshPty`.
- Produces: `openTerminalTransport(env, request, handlers)`, `TerminalSession`, and environment propagation from Space to leaf.

- [ ] **Step 1: Write failing transport selection tests**

```ts
it("passes an explicit local or WSL env to local PTY open", async () => {
  await openTerminalTransport({ kind: "wsl", distro: "Ubuntu" }, request(), handlers());
  expect(openPtyMock).toHaveBeenCalledWith(expect.objectContaining({ workspace: { kind: "wsl", distro: "Ubuntu" } }));
  expect(openSshPtyMock).not.toHaveBeenCalled();
});

it("routes SSH env by owning Space and disables blocks", async () => {
  await openTerminalTransport({ kind: "ssh", profileId: "prod" }, { ...request(), blocks: true }, handlers());
  expect(openSshPtyMock).toHaveBeenCalledWith(expect.objectContaining({ profileId: "prod", blocks: false }));
  expect(openPtyMock).not.toHaveBeenCalled();
});

it("does not expose an SSH channel id as a local PTY id", () => {
  expect(localPtyId({ kind: "ssh", id: 44, write, resize, close })).toBeNull();
});
```

- [ ] **Step 2: Verify red state**

```bash
pnpm test src/modules/terminal/lib/terminal-transport.test.ts
```

Expected: test fails because the abstraction does not exist.

- [ ] **Step 3: Make local PTY env explicit**

Remove `currentWorkspaceEnv()` from `pty-bridge.ts`. Accept `workspace: WorkspaceEnv` in the request. This prevents a terminal leaf from accidentally using whichever Space became globally active during an async spawn.

```ts
export type TerminalSession = {
  kind: "local" | "ssh";
  id: number;
  write(data: string): Promise<void>;
  resize(cols: number, rows: number): Promise<void>;
  close(): Promise<void>;
};
```

- [ ] **Step 4: Thread Space env to every terminal leaf**

`App` builds a stable `Map<string, WorkspaceEnv>` from Spaces. `WorkspaceSurface` passes it to `TerminalStack`; `TerminalStack` reads each tab's `spaceId`; `PaneTreeView` and `TerminalPane` pass the owning env to `useTerminalSession`.

Do not derive env from the active Space inside `useTerminalSession`.

- [ ] **Step 5: Select transport in `openPtyForSession`**

Replace direct `openPty` with `openTerminalTransport`. Preserve the same data delivery, pending input, resize-after-spawn guard, and handler cleanup.

For SSH:

- force `blocks: false`
- ignore the local shell preference
- retain existing xterm buffer and DormantRing behavior
- return `TerminalExit.kind = "lost"` without calling normal shell-exit tab cleanup
- show Session-lost UI through callback state added in Task 10

`ptyIdForLeaf` returns an id only when `session.kind === "local"`. Local agent detection and `pty_has_foreground_job` are never called with SSH channel ids.

- [ ] **Step 6: Preserve renderer safety**

Keep the existing `isLeafAltScreen` guard for hidden SSH leaves. Add tests proving an SSH leaf in the alternate buffer is not released. For non-alt-screen SSH leaves, use the existing retained-buffer and DormantRing path. Do not introduce timer-based remote command detection.

- [ ] **Step 7: Run focused regression checks**

```bash
pnpm test src/modules/terminal/lib/terminal-transport.test.ts src/modules/terminal/lib/dormantRing.test.ts src/modules/terminal/lib/liveTerminals.test.ts src/modules/terminal/lib/panes.test.ts
pnpm check-types
pnpm lint
```

Expected: all commands exit 0.

- [ ] **Step 8: Commit**

```bash
git add src/modules/terminal src/app/App.tsx src/app/components/WorkspaceSurface.tsx
git commit -m "feat(terminal): route SSH Space sessions"
```

---

### Task 10: Build Profile, Remote Space, Challenge, and Session-lost UI

**Files:**

- Create: `src/modules/remote/components/RemoteSpaceDialog.tsx`
- Create: `src/modules/remote/components/SshProfilePicker.tsx`
- Create: `src/modules/remote/components/SshChallengeDialog.tsx`
- Create: `src/modules/remote/components/SshSessionLost.tsx`
- Create: `src/modules/remote/lib/remoteSpaceViewModel.ts`
- Create: `src/modules/remote/lib/remoteSpaceViewModel.test.ts`
- Create: `src/settings/sections/SshSection.tsx`
- Modify: `src/modules/remote/index.ts`
- Modify: `src/modules/spaces/SpaceSwitcher.tsx`
- Modify: `src/modules/spaces/index.ts`
- Modify: `src/modules/settings/openSettingsWindow.ts`
- Modify: `src/settings/SettingsApp.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/modules/terminal/TerminalPane.tsx`

**Interfaces:**

- Consumes: profile store, effective preview IPC, connection reducer, Space `create`, SSH bridge callbacks.
- Produces: approved v2 creation flow, profile settings, correlated trust and auth dialogs, and explicit Session-lost new-shell action.

- [ ] **Step 1: Write failing pure UI-state tests**

Extract view-model functions rather than introducing a new testing library:

```ts
it("keeps profile selection as the primary creation step", () => {
  expect(remoteSpaceStep(initialRemoteSpaceDraft())).toBe("selectProfile");
  expect(canCreateRemoteSpace(initialRemoteSpaceDraft())).toBe(false);
  expect(canCreateRemoteSpace(draftWithProfile("prod"))).toBe(true);
});

it("never offers a bypass for a changed host key", () => {
  const actions = challengeActions(hostKeyMismatchChallenge());
  expect(actions.map((a) => a.id)).toEqual(["openDetails", "cancel"]);
});

it("unknown hosts offer trust once and trust and save", () => {
  const actions = challengeActions(unknownHostChallenge());
  expect(actions.map((a) => a.id)).toEqual(["trustOnce", "trustAndSave", "cancel"]);
});
```

Place these tests in `src/modules/remote/lib/remoteSpaceViewModel.test.ts` and implement the pure helpers in `remoteSpaceViewModel.ts`.

- [ ] **Step 2: Verify red state**

```bash
pnpm test src/modules/remote/lib/connectionState.test.ts
```

Expected: new view-model cases fail because helpers do not exist.

- [ ] **Step 3: Add the SSH Settings tab**

Extend `SettingsTab` with `"ssh"`, add it to `VALID_TABS`, and register `SshSection` with `ComputerTerminal02Icon`, already used by `NewTabMenu`. Follow the existing `SectionHeader` typography and `max-w-160` content width.

The section shows profiles first, effective summary second, and advanced overrides collapsed. It never renders passwords or passphrases.

- [ ] **Step 4: Add the focused Remote Space dialog**

Change `SpaceSwitcher.onNewSpace` to open `RemoteSpaceDialog` rather than immediately clone the active environment. Preserve a quick Local or WSL path in the dialog.

The SSH flow follows the approved visual hierarchy:

1. title and purpose
2. OpenSSH, Saved, or New source tabs
3. profile list as the visual focus
4. compact effective summary
5. one `Create and connect` primary action

Create the Space with `{ name, root: null, env: { kind: "ssh", profileId } }`, create one terminal tab, activate it, and let terminal mount trigger connection.

- [ ] **Step 5: Add challenge dialogs**

Unknown host dialog shows host, port, algorithm, and SHA256 fingerprint. Changed-key dialog shows old and new fingerprints plus file and line, with no continue action.

Password, passphrase, and keyboard-interactive forms submit only the current challenge id. Default remember state is false. Cancel aborts the whole connection attempt.

- [ ] **Step 6: Add Session-lost UI**

Overlay the preserved terminal buffer with a compact status surface. `Reconnect with new shell` explicitly closes the lost channel state and calls `respawnSession` with a new SSH PTY. It does not clear the old buffer until the user confirms the new shell action.

- [ ] **Step 7: Run focused UI checks**

```bash
pnpm test src/modules/remote/lib/connectionState.test.ts src/modules/remote/lib/profileStore.test.ts
pnpm check-types
pnpm lint
pnpm build
```

Expected: all commands exit 0.

- [ ] **Step 8: Commit**

```bash
git add src/modules/remote src/modules/spaces src/modules/settings/openSettingsWindow.ts src/settings src/app/App.tsx src/modules/terminal/TerminalPane.tsx
git commit -m "feat(ssh): add Remote Space workflows"
```

---

### Task 11: Gate Unsupported Surfaces and Complete Lifecycle UX

**Files:**

- Create: `src/modules/remote/components/RemoteUnavailablePanel.tsx`
- Create: `src/modules/remote/lib/remoteBoundaries.ts`
- Create: `src/modules/remote/lib/remoteBoundaries.test.ts`
- Modify: `src/modules/source-control/useSourceControlContext.ts`
- Modify: `src/modules/explorer/lib/useFileTree.ts`
- Modify: `src/modules/explorer/lib/useTerminalFileDrop.ts`
- Modify: `src/app/App.tsx`
- Modify: `src/app/components/WorkspaceSurface.tsx`
- Modify: `src/modules/statusbar/StatusBar.tsx`
- Modify: `src/modules/statusbar/WorkspaceEnvSelector.tsx`
- Modify: `src/modules/spaces/lib/spaceDeletion.ts`
- Modify: `src/modules/spaces/lib/spaceDeletion.test.ts`
- Modify: `src-tauri/src/modules/ssh/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

- Consumes: active Space env and connection status.
- Produces: zero local FS or Git fallthrough, remote connection status presentation, profile-safe deletion, and app-exit cleanup.

- [ ] **Step 1: Write failing boundary tests**

```ts
it("does not refresh local source control for an SSH Space", async () => {
  const deps = sourceControlDeps({ env: { kind: "ssh", profileId: "prod" } });
  await refreshSourceControlContext(deps);
  expect(deps.gitPanelSnapshot).not.toHaveBeenCalled();
});

it("does not read local files for an SSH Space", async () => {
  const deps = explorerDeps({ env: { kind: "ssh", profileId: "prod" } });
  await refreshExplorer(deps);
  expect(deps.fsReadDir).not.toHaveBeenCalled();
});

it("disconnects a remote Space before deleting its state", async () => {
  const calls: string[] = [];
  await deleteSpaceAfterActivation(remoteDeletionDeps(calls));
  expect(calls).toEqual(["activate:fallback", "disconnect:remote", "remove:remote"]);
});
```

- [ ] **Step 2: Verify red state**

```bash
pnpm test src/modules/spaces/lib/spaceDeletion.test.ts src/modules/source-control
```

Expected: at least one test fails because SSH surfaces are not gated.

- [ ] **Step 3: Gate Explorer, Source Control, search, and file drop**

At the highest common boundary, branch on `env.kind === "ssh"` before any native invocation. Render `RemoteUnavailablePanel` with concise Phase 1 copy. Do not pass a fallback local root.

Reject local file drop into SSH terminal with a toast explaining that remote upload is not available. Do not paste the local path.

- [ ] **Step 4: Add status and root behavior**

Status bar shows `SSH user@host` plus dormant, connecting, ready, or lost state. Remote home is read-only in Phase 1. Hide or disable root mutation controls and `onCreateInEnv` for SSH.

The Workspace environment selector may create Local or WSL Spaces as before. SSH selection opens the Remote Space dialog instead of attempting local environment adoption.

- [ ] **Step 5: Complete cleanup**

Space deletion disconnects before deleting tabs and persisted state. App exit calls `ssh_close_all`. Challenge cancellation and webview loss release waiters so shutdown cannot hang.

- [ ] **Step 6: Run focused and broad frontend checks**

```bash
pnpm test src/modules/spaces/lib/spaceDeletion.test.ts src/modules/source-control src/modules/explorer src/modules/remote
pnpm check-types
pnpm lint
pnpm build
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```bash
git add src/modules/remote src/modules/source-control src/modules/explorer src/modules/statusbar src/modules/spaces src/app src-tauri/src/modules/ssh src-tauri/src/lib.rs
git commit -m "feat(ssh): guard remote Space boundaries"
```

---

### Task 12: Cross-platform Verification, Documentation, and Release Gate

**Files:**

- Modify: `TERAX.md`
- Modify: `ROADMAP.md`
- Modify tests under: `src-tauri/src/modules/ssh/`
- Modify tests under: `src/modules/remote/`, `src/modules/spaces/`, `src/modules/terminal/`
- Verify the existing `.github/workflows/ci.yml` macOS, Windows, and Linux jobs without changing their scope

**Interfaces:**

- Consumes: complete Phase 1 implementation.
- Produces: cross-platform evidence, final size evidence, architecture documentation, and a release-ready branch.

- [ ] **Step 1: Add any missing acceptance tests**

Map every spec acceptance criterion to an automated test name. Add tests for any uncovered criterion. Required named coverage includes:

```text
restore_remote_spaces_without_connecting
verify_host_before_authentication
open_multiple_ptys_on_one_space_transport
preserve_buffer_when_live_transport_is_lost
reject_local_fs_for_ssh_environment
reject_local_git_for_ssh_environment
never_bypass_changed_host_key
do_not_persist_auth_responses
close_all_ssh_connections_on_exit
```

- [ ] **Step 2: Verify frontend checks**

Run:

```bash
pnpm lint
pnpm check-types
pnpm test
pnpm build
pnpm analyze:eager
pnpm size
```

Expected: every command exits 0, all tests pass, and SSH UI does not enter the eager main or settings startup bundles beyond the approved lightweight shell.

- [ ] **Step 3: Verify Rust checks**

Run:

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo nextest run --locked
cargo build --release --locked
```

Expected: every command exits 0.

- [ ] **Step 4: Measure final release size**

Run from repository root:

```bash
node -e 'const fs=require("fs"); const p=process.platform==="win32"?"src-tauri/target/release/terax.exe":"src-tauri/target/release/terax"; console.log(JSON.stringify({path:p,bytes:fs.statSync(p).size}))'
```

Compare with Task 1 baseline. If the increase is materially above approximately 1 MB, stop and review dependency features. Do not claim the gate passed solely because the app still builds.

- [ ] **Step 5: Verify client platform matrix**

Record evidence for:

```text
macOS client -> Linux server
macOS client -> macOS server
Linux client -> Linux server
Linux client -> macOS server
Windows client -> Linux server
Windows client -> macOS server
```

For each available client, test OpenSSH alias, Agent, encrypted private key, password, keyboard-interactive, unknown host, changed host key, PTY resize, split panes, cancellation, and transport loss. Unavailable machines remain explicit blockers for release completion, not implied passes.

- [ ] **Step 6: Update architecture and roadmap docs**

In `TERAX.md`, document:

- `WorkspaceEnv.kind = "ssh"`
- Rust SSH manager and security boundaries
- one transport per Space and 32-channel limit
- Phase 1 server platform scope
- no transparent PTY recovery
- disabled remote file and Git surfaces

In `ROADMAP.md`, mark only the delivered SSH terminal/auth/known_hosts slice as shipped. Keep SFTP and forwarding planned.

- [ ] **Step 7: Run final diff and diagnostics review**

```bash
git diff --check
git status --short
git diff --stat
git diff -- src-tauri/Cargo.toml src-tauri/Cargo.lock TERAX.md ROADMAP.md
```

Run `lens_diagnostics mode=all` for every edited source and documentation path. Resolve all blocking errors and review every warning before completion.

- [ ] **Step 8: Commit final tests and docs**

```bash
git add TERAX.md ROADMAP.md src src-tauri
git commit -m "test(ssh): verify Remote Space phase 1"
```

- [ ] **Step 9: Stop before push or PR**

Report the final branch, commit range, verification evidence, residual platform blockers, release-size delta, persistent files created, and retained `.superpowers/brainstorm/` working artifacts. Do not push or open a PR without a separate explicit user instruction. If later authorized, verify that the only remote is `fork` before any network Git action.

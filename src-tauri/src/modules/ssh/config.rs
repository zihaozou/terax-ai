use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD_NO_PAD as BASE64_STANDARD_NO_PAD, Engine};
use globset::Glob;
use sha2::{Digest, Sha256};

use super::{
    limits::{MAX_CONFIG_FILE_BYTES, MAX_INCLUDE_DEPTH, MAX_INCLUDE_FILES},
    AuthMethod, ResolvedSshConfig, SshConfigWarning, SshError, SshErrorCode, SshProfileInput,
    SshProfileSource,
};

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

impl<'a> ResolveContext<'a> {
    pub fn for_home(home: &'a Path) -> Self {
        Self {
            home,
            system_config: None,
            files: &OS_CONFIG_FILES,
        }
    }
}

struct OsConfigFiles;

static OS_CONFIG_FILES: OsConfigFiles = OsConfigFiles;

impl ConfigFiles for OsConfigFiles {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn read_limited(&self, path: &Path, max_bytes: usize) -> io::Result<String> {
        let bytes = fs::read(path)?;
        if bytes.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "config file exceeds limit",
            ));
        }
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn expand_glob(&self, pattern: &Path) -> io::Result<Vec<PathBuf>> {
        let parent = pattern.parent().unwrap_or_else(|| Path::new("."));
        let file_pattern = pattern
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "include pattern has no file name",
                )
            })?;
        let matcher = Glob::new(file_pattern)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
            .compile_matcher();
        let mut paths = fs::read_dir(parent)?.try_fold(Vec::new(), |mut paths, entry| {
            let path = entry?.path();
            if matcher.is_match(path.file_name().unwrap_or_default()) {
                paths.push(path);
            }
            Ok::<_, io::Error>(paths)
        })?;
        paths.sort();
        Ok(paths)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectiveSupport {
    Supported,
    Warning(&'static str),
    Blocking(&'static str),
}

pub fn classify_directive(directive: &str) -> DirectiveSupport {
    match directive.to_ascii_lowercase().as_str() {
        "host"
        | "include"
        | "hostname"
        | "user"
        | "port"
        | "identityfile"
        | "proxycommand"
        | "proxyjump"
        | "addkeystoagent"
        | "userknownhostsfile"
        | "stricthostkeychecking" => DirectiveSupport::Supported,
        "match"
        | "canonicalizehostname"
        | "proxyusefdpass"
        | "hostkeyalgorithms"
        | "pubkeyacceptedalgorithms"
        | "kexalgorithms"
        | "ciphers"
        | "macs"
        | "certificatefile"
        | "identitiesonly"
        | "preferredauthentications"
        | "securitykeyprovider" => DirectiveSupport::Blocking("connection-critical-directive"),
        _ => DirectiveSupport::Warning("unsupported-directive"),
    }
}

pub fn proxy_consent_hash(profile_id: &str, target: &str, command: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(profile_id.as_bytes());
    hasher.update([0]);
    hasher.update(target.as_bytes());
    hasher.update([0]);
    hasher.update(command.as_bytes());
    format!(
        "sha256:{}",
        BASE64_STANDARD_NO_PAD.encode(hasher.finalize())
    )
}

pub fn resolve_profile(
    profile: &SshProfileInput,
    context: &ResolveContext<'_>,
) -> Result<ResolvedSshConfig, SshError> {
    let (target, config) = match &profile.source {
        SshProfileSource::OpenSsh { alias } => {
            if alias.trim().is_empty() {
                return Err(profile_error(profile, "OpenSSH alias is required"));
            }
            (alias.as_str(), load_openssh_config(context, alias)?)
        }
        SshProfileSource::Manual { host } => {
            if host.trim().is_empty() {
                return Err(profile_error(profile, "manual host is required"));
            }
            (host.as_str(), String::new())
        }
    };
    resolve_text_for_profile(&config, target, profile, context.home)
}

pub fn resolve_text(text: &str, target: &str) -> Result<ResolvedSshConfig, SshError> {
    let profile = SshProfileInput {
        id: "preview".to_owned(),
        name: "Preview".to_owned(),
        source: SshProfileSource::OpenSsh {
            alias: target.to_owned(),
        },
        overrides: None,
    };
    resolve_text_for_profile(text, target, &profile, Path::new(""))
}

fn resolve_text_for_profile(
    text: &str,
    target: &str,
    profile: &SshProfileInput,
    home: &Path,
) -> Result<ResolvedSshConfig, SshError> {
    let parsed = russh_config::parse(
        &format!("Host *\n{}", normalize_for_russh(text, home)),
        target,
    )
    .map_err(|error| config_error(profile, error.to_string()))?;
    let overrides = profile.overrides.as_ref();
    let host = overrides
        .and_then(|value| value.host.clone())
        .unwrap_or_else(|| parsed.host().to_owned());
    let port = overrides
        .and_then(|value| value.port)
        .unwrap_or_else(|| parsed.port());
    if host.trim().is_empty() || port == 0 {
        return Err(profile_error(
            profile,
            "resolved host and port must be valid",
        ));
    }
    let identity_files = overrides
        .and_then(|value| value.identity_files.clone())
        .unwrap_or_else(|| {
            parsed
                .host_config
                .identity_file
                .as_ref()
                .into_iter()
                .flatten()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        });
    let known_hosts_files = overrides
        .and_then(|value| value.known_hosts_file.clone())
        .map(|path| vec![path])
        .or_else(|| {
            parsed
                .host_config
                .user_known_hosts_file
                .as_ref()
                .map(|path| vec![path.to_string_lossy().into_owned()])
        })
        .unwrap_or_default();
    let user = overrides
        .and_then(|value| value.user.clone())
        .unwrap_or_else(|| parsed.user());
    let proxy_command = parsed.host_config.proxy_command.clone();
    let proxy_jump = parsed.host_config.proxy_jump.clone();
    let proxy_consent_hash = proxy_command
        .as_deref()
        .map(|command| proxy_consent_hash(&profile.id, &host, command));
    let mut warnings = warnings_for(text, target);
    if strict_host_checking_is_weakened(text, target) {
        warnings.push(SshConfigWarning {
            code: "strict-host-key-overridden".to_owned(),
            message: "Terax always requires explicit trust for unknown hosts and blocks changed host keys."
                .to_owned(),
            blocking: false,
        });
    }
    let blocking_warning = warnings.iter().find(|warning| warning.blocking);
    if let Some(warning) = blocking_warning {
        return Err(config_error(profile, warning.message.clone()));
    }

    Ok(ResolvedSshConfig {
        profile_id: profile.id.clone(),
        host,
        port,
        user,
        identity_files,
        auth_order: overrides
            .and_then(|value| value.auth_order.clone())
            .unwrap_or_else(default_auth_order),
        proxy_command: proxy_command.clone(),
        proxy_jump,
        known_hosts_files,
        warnings,
        proxy_consent_hash,
    })
}

fn default_auth_order() -> Vec<AuthMethod> {
    vec![
        AuthMethod::Agent,
        AuthMethod::PrivateKey,
        AuthMethod::Password,
        AuthMethod::KeyboardInteractive,
    ]
}

fn load_openssh_config(
    context: &ResolveContext<'_>,
    profile_target: &str,
) -> Result<String, SshError> {
    let user_config = context.home.join(".ssh/config");
    let mut state = LoadState::default();
    let mut roots = vec![canonical_root(context, &context.home.join(".ssh"))];
    if let Some(system_config) = context.system_config {
        roots.push(canonical_root(
            context,
            system_config.parent().unwrap_or_else(|| Path::new("/")),
        ));
    }
    let mut text = String::new();
    match context.files.canonicalize(&user_config) {
        Ok(_) => text.push_str(&load_file(
            &user_config,
            context,
            &roots,
            &mut state,
            0,
            profile_target,
        )?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(config_error_for_target(profile_target, error.to_string())),
    }
    if let Some(system_config) = context.system_config {
        text.push_str(&load_file(
            system_config,
            context,
            &roots,
            &mut state,
            0,
            profile_target,
        )?);
    }
    Ok(text)
}

#[derive(Default)]
struct LoadState {
    seen: HashSet<PathBuf>,
    files: usize,
}

fn load_file(
    path: &Path,
    context: &ResolveContext<'_>,
    roots: &[PathBuf],
    state: &mut LoadState,
    depth: usize,
    target: &str,
) -> Result<String, SshError> {
    if depth >= MAX_INCLUDE_DEPTH {
        return Err(config_error_for_target(
            target,
            "OpenSSH Include depth exceeds the limit",
        ));
    }
    let canonical = context
        .files
        .canonicalize(path)
        .map_err(|error| config_error_for_target(target, error.to_string()))?;
    let approved = roots.iter().any(|root| canonical.starts_with(root));
    if !approved {
        return Err(config_error_for_target(
            target,
            "OpenSSH Include escapes approved config roots",
        ));
    }
    if !state.seen.insert(canonical.clone()) {
        return Err(config_error_for_target(
            target,
            "OpenSSH Include contains a cycle",
        ));
    }
    state.files += 1;
    if state.files > MAX_INCLUDE_FILES {
        return Err(config_error_for_target(
            target,
            "OpenSSH Include file count exceeds the limit",
        ));
    }
    let contents = context
        .files
        .read_limited(&canonical, MAX_CONFIG_FILE_BYTES)
        .map_err(|error| config_error_for_target(target, error.to_string()))?;
    let mut resolved = String::new();
    for line in contents.lines() {
        let Some((directive, value)) = split_directive(line) else {
            resolved.push_str(line);
            resolved.push('\n');
            continue;
        };
        if !directive.eq_ignore_ascii_case("Include") {
            resolved.push_str(line);
            resolved.push('\n');
            continue;
        }
        for include in include_patterns(value, canonical.parent(), context.home) {
            let paths = context
                .files
                .expand_glob(&include)
                .map_err(|error| config_error_for_target(target, error.to_string()))?;
            for path in paths {
                resolved.push_str(&load_file(&path, context, roots, state, depth + 1, target)?);
            }
        }
    }
    Ok(resolved)
}

fn canonical_root(context: &ResolveContext<'_>, root: &Path) -> PathBuf {
    context
        .files
        .canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
}

fn include_patterns(value: &str, parent: Option<&Path>, home: &Path) -> Vec<PathBuf> {
    value
        .split_ascii_whitespace()
        .map(|pattern| {
            let expanded = pattern
                .strip_prefix("~/")
                .map(|path| home.join(path))
                .unwrap_or_else(|| PathBuf::from(pattern));
            if expanded.is_absolute() {
                expanded
            } else {
                parent.unwrap_or(home).join(expanded)
            }
        })
        .collect()
}

fn normalize_for_russh(text: &str, home: &Path) -> String {
    text.lines()
        .filter_map(|line| {
            let (directive, value) = split_directive(line)?;
            if directive.eq_ignore_ascii_case("Include") {
                return None;
            }
            let value = if matches!(
                directive.to_ascii_lowercase().as_str(),
                "identityfile" | "userknownhostsfile"
            ) {
                value
                    .strip_prefix("~/")
                    .map(|path| home.join(path).to_string_lossy().into_owned())
                    .unwrap_or_else(|| value.to_owned())
            } else {
                value.to_owned()
            };
            Some(format!("{directive} {value}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn warnings_for(text: &str, target: &str) -> Vec<SshConfigWarning> {
    let mut warnings = Vec::new();
    let mut applies = true;
    for line in text.lines() {
        let Some((directive, value)) = split_directive(line) else {
            continue;
        };
        if directive.eq_ignore_ascii_case("Host") {
            applies = host_patterns_match(value, target);
            continue;
        }
        if !applies {
            continue;
        }
        match classify_directive(directive) {
            DirectiveSupport::Supported => {}
            DirectiveSupport::Warning(code) => warnings.push(SshConfigWarning {
                code: code.to_owned(),
                message: format!("OpenSSH directive {directive} is not supported by Terax."),
                blocking: false,
            }),
            DirectiveSupport::Blocking(code) => warnings.push(SshConfigWarning {
                code: code.to_owned(),
                message: format!("OpenSSH directive {directive} is required for this connection."),
                blocking: true,
            }),
        }
    }
    warnings
}

fn strict_host_checking_is_weakened(text: &str, target: &str) -> bool {
    let mut applies = true;
    text.lines().any(|line| {
        let Some((directive, value)) = split_directive(line) else {
            return false;
        };
        if directive.eq_ignore_ascii_case("Host") {
            applies = host_patterns_match(value, target);
            return false;
        }
        applies
            && directive.eq_ignore_ascii_case("StrictHostKeyChecking")
            && matches!(
                value.to_ascii_lowercase().as_str(),
                "no" | "off" | "accept-new"
            )
    })
}

fn host_patterns_match(patterns: &str, target: &str) -> bool {
    let mut matches = false;
    for pattern in patterns.split_ascii_whitespace() {
        let (pattern, negated) = pattern
            .strip_prefix('!')
            .map(|pattern| (pattern, true))
            .unwrap_or((pattern, false));
        let Ok(glob) = Glob::new(pattern) else {
            continue;
        };
        if glob.compile_matcher().is_match(target) {
            if negated {
                return false;
            }
            matches = true;
        }
    }
    matches
}

fn split_directive(line: &str) -> Option<(&str, &str)> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() {
        return None;
    }
    let split = line.find(char::is_whitespace).or_else(|| line.find('='))?;
    let directive = line[..split].trim();
    let value = line[split + 1..].trim_start_matches(['=', ' ']).trim();
    (!directive.is_empty() && !value.is_empty()).then_some((directive, value))
}

fn profile_error(profile: &SshProfileInput, message: impl Into<String>) -> SshError {
    SshError::new(
        SshErrorCode::ProfileInvalid,
        "resolve-profile",
        profile.id.clone(),
        message,
    )
}

fn config_error(profile: &SshProfileInput, message: impl Into<String>) -> SshError {
    config_error_for_target(&profile.id, message)
}

fn config_error_for_target(target: &str, message: impl Into<String>) -> SshError {
    SshError::new(
        SshErrorCode::ConfigUnsupported,
        "resolve-profile",
        target,
        message,
    )
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::modules::ssh::{SshProfileOverrides, SshProfileSource};

    fn fixture_home(config: &str) -> TempDir {
        let home = TempDir::new().expect("temporary home must be created");
        let ssh = home.path().join(".ssh");
        std::fs::create_dir(&ssh).expect("ssh directory must be created");
        std::fs::write(ssh.join("config"), config).expect("config must be written");
        home
    }

    fn cyclic_include_fixture() -> TempDir {
        let home = fixture_home("Include cycle-a\nHost prod\n  HostName prod.example\n");
        let ssh = home.path().join(".ssh");
        std::fs::write(ssh.join("cycle-a"), "Include cycle-b\n").expect("include must be written");
        std::fs::write(ssh.join("cycle-b"), "Include cycle-a\n").expect("include must be written");
        home
    }

    fn openssh_profile(alias: &str) -> SshProfileInput {
        SshProfileInput {
            id: "profile-1".to_owned(),
            name: "Production".to_owned(),
            source: SshProfileSource::OpenSsh {
                alias: alias.to_owned(),
            },
            overrides: None,
        }
    }

    trait ProfileExt {
        fn with_host(self, host: &str) -> Self;
        fn with_port(self, port: u16) -> Self;
    }

    impl ProfileExt for SshProfileInput {
        fn with_host(mut self, host: &str) -> Self {
            self.overrides = Some(SshProfileOverrides {
                host: Some(host.to_owned()),
                ..Default::default()
            });
            self
        }

        fn with_port(mut self, port: u16) -> Self {
            self.overrides
                .get_or_insert_with(SshProfileOverrides::default)
                .port = Some(port);
            self
        }
    }

    #[test]
    fn manual_overrides_win_over_openssh_alias() {
        let home = fixture_home("Host prod\n  HostName old.example\n  User deploy\n  Port 22\n");
        let profile = openssh_profile("prod")
            .with_host("new.example")
            .with_port(2202);
        let out = resolve_profile(&profile, &ResolveContext::for_home(home.path())).unwrap();
        assert_eq!(out.host, "new.example");
        assert_eq!(out.user, "deploy");
        assert_eq!(out.port, 2202);
    }

    #[test]
    fn include_cycles_and_limits_are_rejected() {
        let home = cyclic_include_fixture();
        let err = resolve_profile(
            &openssh_profile("prod"),
            &ResolveContext::for_home(home.path()),
        )
        .unwrap_err();
        assert_eq!(err.code, SshErrorCode::ConfigUnsupported);
    }

    #[test]
    fn weaker_strict_host_checking_never_disables_terax_trust() {
        let out = resolve_text("Host prod\n StrictHostKeyChecking no\n", "prod").unwrap();
        assert!(out
            .warnings
            .iter()
            .any(|warning| warning.code == "strict-host-key-overridden"));
    }

    #[test]
    fn proxy_consent_hash_changes_with_effective_command() {
        assert_eq!(
            proxy_consent_hash("profile", "host", "nc %h %p"),
            "sha256:yEOtNcu+WEB3JglgcPPUiwviFr1mYwI1YWQ9NgdAIg4"
        );
        assert_ne!(
            proxy_consent_hash("profile", "host", "nc %h %p"),
            proxy_consent_hash("profile", "host", "nc -x proxy %h %p")
        );
    }

    #[test]
    fn include_globs_resolve_within_the_ssh_root() {
        let home = fixture_home("Include hosts/*.conf\n");
        let hosts = home.path().join(".ssh/hosts");
        std::fs::create_dir(&hosts).expect("include directory must be created");
        std::fs::write(
            hosts.join("prod.conf"),
            "Host prod\n  HostName prod.example\n",
        )
        .expect("included config must be written");

        let out = resolve_profile(
            &openssh_profile("prod"),
            &ResolveContext::for_home(home.path()),
        )
        .expect("included alias must resolve");

        assert_eq!(out.host, "prod.example");
    }

    #[test]
    fn directive_classification_distinguishes_safe_and_blocking_settings() {
        assert_eq!(classify_directive("HostName"), DirectiveSupport::Supported);
        assert!(matches!(
            classify_directive("Compression"),
            DirectiveSupport::Warning(_)
        ));
        assert!(matches!(
            classify_directive("Ciphers"),
            DirectiveSupport::Blocking(_)
        ));
    }
}

//! Config file management for `~/.corelink/config.toml` (WI-S15-001).
//!
//! The config file uses TOML format with sections `[auth]`, `[defaults]`,
//! and `[telemetry]`. On first write chmod 600 is enforced (Unix). Writes
//! are atomic (temp file + rename).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Full config file schema.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CorelinkConfig {
    /// Auth section.
    #[serde(default)]
    pub auth: AuthConfig,

    /// Defaults section.
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Telemetry section (opt-in default-off; WI-S15-005).
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

/// `[auth]` section.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AuthConfig {
    /// Personal Access Token. Never echo raw in output — use [`AuthConfig::redacted_pat`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pat: Option<String>,

    /// BYOK enabled flag — triggers doctor check #5.
    #[serde(default)]
    pub byok_enabled: bool,
}

impl AuthConfig {
    /// Returns the PAT with secret portion replaced by `***` for display.
    ///
    /// Redacts everything after the first `.` to prevent exposure of the
    /// random secret or HMAC sig (CTRL-CRED-001).
    #[must_use]
    pub fn redacted_pat(&self) -> Option<String> {
        self.pat.as_ref().map(|p| {
            if let Some(idx) = p.find('.') {
                let prefix = &p[..idx.min(20)];
                format!("{prefix}***")
            } else if p.len() > 20 {
                format!("{}***", &p[..20])
            } else {
                "***".to_owned()
            }
        })
    }
}

/// `[defaults]` section.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DefaultsConfig {
    /// Default tenant ID used when `--tenant` is not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Default output format (`text` or `json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
}

/// `[telemetry]` section. Default off (privacy-first).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TelemetryConfig {
    /// Telemetry enabled. Default: `false` (opt-in).
    #[serde(default)]
    pub enabled: bool,
}

/// Returns the canonical config file path: `~/.corelink/config.toml`.
#[must_use]
pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home).join(".corelink").join("config.toml")
}

/// Load config from `~/.corelink/config.toml`.
///
/// Returns `Ok(CorelinkConfig::default())` if the file does not exist.
/// On Unix, warns if permissions are not 0o600.
pub fn load() -> Result<CorelinkConfig, ConfigError> {
    let path = config_path();
    if !path.exists() {
        return Ok(CorelinkConfig::default());
    }

    #[cfg(unix)]
    check_permissions(&path)?;

    let raw = std::fs::read_to_string(&path).map_err(ConfigError::Read)?;
    let cfg: CorelinkConfig = toml::from_str(&raw)?;
    Ok(cfg)
}

/// Write `cfg` atomically to `~/.corelink/config.toml`.
///
/// Creates the directory if needed. Enforces chmod 600 on Unix.
pub fn save(cfg: &CorelinkConfig) -> Result<(), ConfigError> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ConfigError::Write)?;
    }
    let content = toml::to_string_pretty(cfg)?;
    atomic_write(&path, content.as_bytes())?;
    #[cfg(unix)]
    set_permissions_600(&path)?;
    Ok(())
}

/// Set a config key. Supports dotted paths like `auth.pat`, `defaults.tenant_id`,
/// `telemetry.enabled`.
pub fn set_key(key: &str, value: &str) -> Result<(), ConfigError> {
    let mut cfg = load()?;
    apply_key(&mut cfg, key, value)?;
    save(&cfg)
}

/// Get a config key value as a string.
pub fn get_key(key: &str) -> Result<String, ConfigError> {
    let cfg = load()?;
    read_key(&cfg, key)
}

/// List config as a sanitised TOML string (PAT redacted).
pub fn list_sanitised() -> Result<String, ConfigError> {
    let cfg = load()?;
    Ok(list_sanitised_from_cfg(&cfg))
}

/// Return sanitised TOML string from a given [`CorelinkConfig`] (PAT redacted).
/// Exposed as a testable helper that doesn't depend on the filesystem.
pub fn list_sanitised_from_cfg(cfg: &CorelinkConfig) -> String {
    let mut sanitised = cfg.clone();
    if sanitised.auth.pat.is_some() {
        sanitised.auth.pat = sanitised.auth.redacted_pat();
    }
    toml::to_string_pretty(&sanitised).unwrap_or_else(|_| "(serialise error)".to_owned())
}

/// Load config from an explicit path (test helper; skips permission check).
#[allow(dead_code)]
pub fn load_from_path(path: &Path) -> Result<CorelinkConfig, ConfigError> {
    if !path.exists() {
        return Ok(CorelinkConfig::default());
    }
    let raw = std::fs::read_to_string(path).map_err(ConfigError::Read)?;
    let cfg: CorelinkConfig = toml::from_str(&raw)?;
    Ok(cfg)
}

/// Save config to an explicit path (test helper; skips chmod).
#[allow(dead_code)]
pub fn save_to_path(path: &Path, cfg: &CorelinkConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ConfigError::Write)?;
    }
    let content = toml::to_string_pretty(cfg)?;
    atomic_write(path, content.as_bytes())?;
    Ok(())
}

/// Public wrapper around `apply_key` for test access.
#[allow(dead_code)]
pub fn apply_key_to_cfg(cfg: &mut CorelinkConfig, key: &str, value: &str) -> Result<(), ConfigError> {
    apply_key(cfg, key, value)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn apply_key(cfg: &mut CorelinkConfig, key: &str, value: &str) -> Result<(), ConfigError> {
    match key {
        "auth.pat" => {
            cfg.auth.pat = Some(value.to_owned());
        }
        "auth.byok_enabled" => {
            cfg.auth.byok_enabled = parse_bool(value, key)?;
        }
        "defaults.tenant_id" => {
            cfg.defaults.tenant_id = Some(value.to_owned());
        }
        "defaults.output_format" => {
            if value != "text" && value != "json" {
                return Err(ConfigError::UnknownKey(format!(
                    "defaults.output_format must be 'text' or 'json', got '{value}'"
                )));
            }
            cfg.defaults.output_format = Some(value.to_owned());
        }
        "telemetry.enabled" => {
            cfg.telemetry.enabled = parse_bool(value, key)?;
        }
        _ => {
            return Err(ConfigError::UnknownKey(key.to_owned()));
        }
    }
    Ok(())
}

fn read_key(cfg: &CorelinkConfig, key: &str) -> Result<String, ConfigError> {
    match key {
        "auth.pat" => Ok(cfg
            .auth
            .redacted_pat()
            .unwrap_or_else(|| "(unset)".to_owned())),
        "auth.byok_enabled" => Ok(cfg.auth.byok_enabled.to_string()),
        "defaults.tenant_id" => Ok(cfg
            .defaults
            .tenant_id
            .clone()
            .unwrap_or_else(|| "(unset)".to_owned())),
        "defaults.output_format" => Ok(cfg
            .defaults
            .output_format
            .clone()
            .unwrap_or_else(|| "text".to_owned())),
        "telemetry.enabled" => Ok(cfg.telemetry.enabled.to_string()),
        _ => Err(ConfigError::UnknownKey(key.to_owned())),
    }
}

fn parse_bool(value: &str, key: &str) -> Result<bool, ConfigError> {
    match value.to_lowercase().as_str() {
        "true" | "on" | "1" | "yes" => Ok(true),
        "false" | "off" | "0" | "no" => Ok(false),
        _ => Err(ConfigError::UnknownKey(format!(
            "key '{key}' expects a boolean (true/false), got '{value}'"
        ))),
    }
}

/// Atomic write: write to a `.tmp` sibling then rename.
fn atomic_write(path: &Path, data: &[u8]) -> Result<(), ConfigError> {
    let tmp_path = path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, data).map_err(ConfigError::Write)?;
    std::fs::rename(&tmp_path, path).map_err(ConfigError::Write)?;
    Ok(())
}

#[cfg(unix)]
fn set_permissions_600(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::PermissionsExt as _;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms).map_err(ConfigError::Write)?;
    Ok(())
}

#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt as _;
    let meta = std::fs::metadata(path).map_err(ConfigError::Read)?;
    let mode = meta.mode() & 0o777;
    if mode != 0o600 {
        return Err(ConfigError::InsecurePermissions);
    }
    Ok(())
}

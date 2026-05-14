//! CoreLink CLI configuration — persistent `~/.corelink/config.toml`.
//!
//! # Telemetry (WI-S15-005)
//!
//! Telemetry is **opt-in, default-off** per R-S15-14 and GDPR Art. 25 (privacy by design).
//!
//! Enable:  `corelink config set telemetry on`
//! Disable: `corelink config set telemetry off`
//! Inspect: `corelink config list`
//!
//! There is **no `--telemetry=on` command-line flag** (Lote 10.15 codex P2 canonical).
//! Telemetry state is exclusively managed via the persistent config file.
//!
//! # Anonymized-ID rotation
//!
//! `corelink config rotate telemetry-id` — generates a new UUID v4 and writes it to config.
//! The old ID is discarded; no server-side mapping exists (privacy-first).

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur when loading or persisting config.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The config directory or file could not be determined.
    #[error("cannot determine config directory")]
    NoConfigDir,

    /// An I/O error occurred.
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A TOML parse error occurred.
    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),

    /// A TOML serialisation error occurred.
    #[error("config serialise error: {0}")]
    Serialise(#[from] toml::ser::Error),

    /// An unknown key was passed to `config set`.
    #[error("unknown config key: {0:?}; valid keys: telemetry")]
    UnknownKey(String),

    /// An invalid value was passed to `config set`.
    #[error("invalid value {value:?} for key {key:?}; {hint}")]
    InvalidValue {
        /// Config key.
        key: String,
        /// Supplied value.
        value: String,
        /// Human-readable hint.
        hint: String,
    },
}

/// Persistent configuration stored in `~/.corelink/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Telemetry opt-in flag. **Default: false** (privacy-first; GDPR Art. 25).
    ///
    /// Set via `corelink config set telemetry on`.
    /// Never set via command-line flag (Lote 10.15 codex P2 canonical).
    #[serde(default)]
    pub telemetry: bool,

    /// Stable anonymised UUID generated on first launch.
    ///
    /// Rotatable via `corelink config rotate telemetry-id`.
    /// Never equals `tenant_id`; not linked server-side.
    #[serde(default = "Uuid::new_v4")]
    pub anonymized_id: Uuid,

    /// PAT (Personal Access Token). Loaded from env `CORELINK_PAT` at runtime
    /// if not present here. Never stored in CLI args (CTRL-CRED-001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pat: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            telemetry: false, // MUST remain false — opt-in default-off (R-S15-14)
            anonymized_id: Uuid::new_v4(),
            pat: None,
        }
    }
}

impl Config {
    /// Return the canonical config file path: `~/.corelink/config.toml`.
    pub fn path() -> Result<PathBuf, ConfigError> {
        let home = dirs::home_dir().ok_or(ConfigError::NoConfigDir)?;
        Ok(home.join(".corelink").join("config.toml"))
    }

    /// Load config from disk, creating a default one if absent.
    pub fn load() -> Result<Self, ConfigError> {
        let p = Self::path()?;
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&p)?;
        let cfg: Self = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Persist config to disk (creates parent dir if needed).
    pub fn save(&self) -> Result<(), ConfigError> {
        let p = Self::path()?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&p, contents)?;
        Ok(())
    }

    /// Load config from an explicit path (used in tests).
    #[cfg(test)]
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Save config to an explicit path (used in tests).
    #[cfg(test)]
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Apply a `config set <key> <value>` mutation.
    ///
    /// Valid keys: `telemetry` (values: `on` / `off`).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownKey`] or [`ConfigError::InvalidValue`] on invalid input.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key {
            "telemetry" => match value {
                "on" | "true" => {
                    self.telemetry = true;
                    Ok(())
                }
                "off" | "false" => {
                    self.telemetry = false;
                    Ok(())
                }
                _ => Err(ConfigError::InvalidValue {
                    key: key.to_owned(),
                    value: value.to_owned(),
                    hint: "use 'on' or 'off'".to_owned(),
                }),
            },
            _ => Err(ConfigError::UnknownKey(key.to_owned())),
        }
    }

    /// Return the value for a single config key as a string.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownKey`] if `key` is not recognised.
    pub fn get(&self, key: &str) -> Result<String, ConfigError> {
        match key {
            "telemetry" => Ok(if self.telemetry { "on".to_owned() } else { "off".to_owned() }),
            "anonymized_id" => Ok(self.anonymized_id.to_string()),
            _ => Err(ConfigError::UnknownKey(key.to_owned())),
        }
    }

    /// Return a human-readable listing of all config keys and their current values,
    /// including a reference to the privacy policy.
    #[must_use]
    pub fn list(&self) -> String {
        format!(
            "telemetry: {telemetry}\n\
             anonymized_id: {id}\n\
             \n\
             Privacy policy: docs/cli/telemetry.md\n\
             Enable telemetry: corelink config set telemetry on\n\
             Rotate telemetry ID: corelink config rotate telemetry-id",
            telemetry = if self.telemetry { "on" } else { "off" },
            id = self.anonymized_id,
        )
    }

    /// Rotate the `anonymized_id` to a fresh UUID v4.
    ///
    /// The previous ID is permanently discarded — no server-side mapping exists.
    pub fn rotate_telemetry_id(&mut self) {
        self.anonymized_id = Uuid::new_v4();
    }

    /// Returns `true` if telemetry is enabled.
    #[must_use]
    #[inline]
    pub fn telemetry_enabled(&self) -> bool {
        self.telemetry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_path(dir: &std::path::Path) -> PathBuf {
        dir.join("config.toml")
    }

    /// Default config must have telemetry = false (invariant: opt-in default-off).
    #[test]
    fn default_telemetry_is_off() {
        let cfg = Config::default();
        assert!(!cfg.telemetry, "INVARIANT VIOLATED: default telemetry must be false");
    }

    /// config set telemetry on → telemetry = true.
    #[test]
    fn set_telemetry_on() {
        let mut cfg = Config::default();
        cfg.set("telemetry", "on").expect("set should succeed");
        assert!(cfg.telemetry);
    }

    /// config set telemetry off → telemetry = false.
    #[test]
    fn set_telemetry_off() {
        let mut cfg = Config::default();
        cfg.set("telemetry", "on").expect("set on");
        cfg.set("telemetry", "off").expect("set off");
        assert!(!cfg.telemetry);
    }

    /// config get telemetry returns "off" for fresh config.
    #[test]
    fn get_telemetry_default_off() {
        let cfg = Config::default();
        assert_eq!(cfg.get("telemetry").expect("get"), "off");
    }

    /// config list always mentions telemetry status.
    #[test]
    fn list_contains_telemetry() {
        let cfg = Config::default();
        let listing = cfg.list();
        assert!(listing.contains("telemetry: off"));
        assert!(listing.contains("Privacy policy"), "listing must reference privacy policy");
    }

    /// Roundtrip: save then load preserves telemetry = false.
    #[test]
    fn roundtrip_telemetry_false() {
        let dir = tempdir().expect("tempdir");
        let p = test_path(dir.path());
        let cfg = Config::default();
        cfg.save_to(&p).expect("save");
        let loaded = Config::load_from(&p).expect("load");
        assert!(!loaded.telemetry, "roundtrip must preserve telemetry = false");
    }

    /// Roundtrip: save then load preserves telemetry = true.
    #[test]
    fn roundtrip_telemetry_true() {
        let dir = tempdir().expect("tempdir");
        let p = test_path(dir.path());
        let mut cfg = Config::default();
        cfg.set("telemetry", "on").expect("set");
        cfg.save_to(&p).expect("save");
        let loaded = Config::load_from(&p).expect("load");
        assert!(loaded.telemetry);
    }

    /// anonymized_id rotation produces a different UUID.
    #[test]
    fn rotate_telemetry_id_changes_uuid() {
        let mut cfg = Config::default();
        let old = cfg.anonymized_id;
        cfg.rotate_telemetry_id();
        assert_ne!(cfg.anonymized_id, old, "rotation must produce a new UUID");
    }

    /// Unknown key returns ConfigError::UnknownKey (negative scenario).
    #[test]
    fn set_unknown_key_errors() {
        let mut cfg = Config::default();
        let err = cfg.set("tenant_id", "acme").expect_err("should error");
        assert!(matches!(err, ConfigError::UnknownKey(_)));
    }

    /// Invalid value for known key returns ConfigError::InvalidValue (negative scenario).
    #[test]
    fn set_invalid_value_errors() {
        let mut cfg = Config::default();
        let err = cfg.set("telemetry", "yes").expect_err("should error");
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
    }

    /// Load from non-existent path returns default (negative scenario — missing file).
    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("nonexistent.toml");
        let cfg = Config::load_from(&p).expect("should return default");
        assert!(!cfg.telemetry, "missing file → default telemetry = false");
    }
}

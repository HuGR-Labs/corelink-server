//! Config file management for `~/.corelink/config.toml` (WI-S15-001 + WI-S15-005).
//!
//! The config file uses TOML format with sections `[auth]`, `[defaults]`,
//! and `[telemetry]`. On first write chmod 600 is enforced (Unix). Writes
//! are atomic (temp file + rename).
//!
//! # Telemetry (WI-S15-005)
//!
//! Telemetry is **opt-in, default-off** per R-S15-14 and GDPR Art. 25.
//! `[telemetry] enabled = false` is the default; the anonymised UUID is
//! generated on first config materialisation and is rotatable via
//! [`rotate_telemetry_id`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    /// Observability export section (RED/USE metric forwarding to
    /// Datadog / an OTel Collector / Grafana Cloud).
    ///
    /// Held as a free-form TOML table so the full nested vendor schema
    /// documented in `how-to/observability/*` (e.g.
    /// `[observability.export.datadog]`, per-region blocks) round-trips
    /// verbatim through `corelink config apply --file` and
    /// `corelink config set observability.*`. `None` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observability: Option<toml::Value>,
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
    /// Populated automatically by `corelink login` and `corelink whoami`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Default output format (`text` or `json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,

    /// API endpoint base URL.
    ///
    /// Defaults to `https://corelink-api.humangr.com` when not set.
    /// Override via `CORELINK_BASE_URL` env var or `corelink config set
    /// defaults.endpoint <url>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

/// Canonical default API endpoint for the CoreLink production service.
pub const DEFAULT_ENDPOINT: &str = "https://corelink-api.humangr.com";

/// `[telemetry]` section. Default off (privacy-first).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TelemetryConfig {
    /// Telemetry enabled. Default: `false` (opt-in).
    #[serde(default)]
    pub enabled: bool,

    /// Stable anonymised UUID, opt-in telemetry payload (WI-S15-005).
    ///
    /// Generated on first config materialisation; never equals `tenant_id`;
    /// rotatable via `corelink config rotate telemetry-id`.
    #[serde(default = "Uuid::new_v4")]
    pub anonymized_id: Uuid,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false, // MUST remain false — opt-in default-off (R-S15-14).
            anonymized_id: Uuid::new_v4(),
        }
    }
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
pub fn apply_key_to_cfg(
    cfg: &mut CorelinkConfig,
    key: &str,
    value: &str,
) -> Result<(), ConfigError> {
    apply_key(cfg, key, value)
}

/// Rotate `[telemetry].anonymized_id` to a fresh UUID v4 and persist.
///
/// The previous ID is permanently discarded — no server-side mapping exists.
pub fn rotate_telemetry_id() -> Result<(), ConfigError> {
    let mut cfg = load()?;
    cfg.telemetry.anonymized_id = Uuid::new_v4();
    save(&cfg)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn apply_key(cfg: &mut CorelinkConfig, key: &str, value: &str) -> Result<(), ConfigError> {
    // Dynamic observability sub-tree — any `observability.*` dotted key is
    // accepted (the vendor schema is open-ended). `config set` supplies a
    // string; we infer bool / integer / string so `timeout_ms = 2000` and
    // `opt_out_low_value_metrics = true` land as their natural TOML types.
    if let Some(rest) = key.strip_prefix("observability.") {
        return set_observability_value(cfg, rest, infer_toml_value(value), key);
    }
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
        "defaults.endpoint" => {
            cfg.defaults.endpoint = Some(value.to_owned());
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
    if let Some(rest) = key.strip_prefix("observability.") {
        return Ok(read_observability_value(cfg, rest));
    }
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
        "defaults.endpoint" => Ok(cfg
            .defaults
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned())),
        "telemetry.enabled" => Ok(cfg.telemetry.enabled.to_string()),
        "telemetry.anonymized_id" => Ok(cfg.telemetry.anonymized_id.to_string()),
        _ => Err(ConfigError::UnknownKey(key.to_owned())),
    }
}

/// Canonical observability export variants accepted for
/// `observability.export.variant`.
const OBSERVABILITY_VARIANTS: [&str; 3] = ["datadog", "otel_collector", "grafana_cloud"];

/// Infer a natural TOML scalar type from a `config set` string value:
/// `true`/`false` → boolean, a bare integer → integer, else string.
fn infer_toml_value(value: &str) -> toml::Value {
    match value.to_lowercase().as_str() {
        "true" => return toml::Value::Boolean(true),
        "false" => return toml::Value::Boolean(false),
        _ => {}
    }
    if let Ok(i) = value.parse::<i64>() {
        return toml::Value::Integer(i);
    }
    toml::Value::String(value.to_owned())
}

/// Collapse a TOML scalar to the string form the fixed-key dispatch table
/// (`auth.*`, `defaults.*`, `telemetry.*`) expects. Rejects composite
/// (table / array) values for those fixed keys.
fn toml_scalar_to_string(key: &str, val: &toml::Value) -> Result<String, ConfigError> {
    match val {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(i) => Ok(i.to_string()),
        toml::Value::Boolean(b) => Ok(b.to_string()),
        toml::Value::Float(f) => Ok(f.to_string()),
        _ => Err(ConfigError::UnknownKey(format!(
            "config apply: key '{key}' expects a scalar value, got a table/array"
        ))),
    }
}

/// Set a value under the `observability` sub-tree at `dotted_rest`
/// (the portion after the `observability.` prefix), creating
/// intermediate tables as needed. Validates the known-constrained
/// `observability.export.variant` enum.
fn set_observability_value(
    cfg: &mut CorelinkConfig,
    dotted_rest: &str,
    val: toml::Value,
    full_key: &str,
) -> Result<(), ConfigError> {
    if full_key == "observability.export.variant" {
        let variant = val.as_str().ok_or_else(|| {
            ConfigError::UnknownKey("observability.export.variant must be a string".to_owned())
        })?;
        if !OBSERVABILITY_VARIANTS.contains(&variant) {
            return Err(ConfigError::UnknownKey(format!(
                "observability.export.variant must be one of {}, got '{variant}'",
                OBSERVABILITY_VARIANTS.join("|")
            )));
        }
    }

    let segments: Vec<&str> = dotted_rest.split('.').collect();
    let (last, parents) = segments.split_last().ok_or_else(|| {
        ConfigError::UnknownKey(format!("invalid observability key '{full_key}'"))
    })?;
    if last.is_empty() || parents.iter().any(|s| s.is_empty()) {
        return Err(ConfigError::UnknownKey(format!(
            "invalid observability key '{full_key}' (empty path segment)"
        )));
    }

    let root = cfg
        .observability
        .get_or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let mut cur = root.as_table_mut().ok_or_else(|| {
        ConfigError::UnknownKey("observability root is not a table".to_owned())
    })?;
    for seg in parents {
        let entry = cur
            .entry((*seg).to_owned())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        cur = entry.as_table_mut().ok_or_else(|| {
            ConfigError::UnknownKey(format!(
                "observability path conflict at '{seg}' in '{full_key}' (existing non-table value)"
            ))
        })?;
    }
    cur.insert((*last).to_owned(), val);
    Ok(())
}

/// Read a value under the `observability` sub-tree, returning `(unset)`
/// when the path is absent.
fn read_observability_value(cfg: &CorelinkConfig, dotted_rest: &str) -> String {
    let Some(root) = cfg.observability.as_ref() else {
        return "(unset)".to_owned();
    };
    let mut cur = root;
    for seg in dotted_rest.split('.') {
        match cur.as_table().and_then(|t| t.get(seg)) {
            Some(next) => cur = next,
            None => return "(unset)".to_owned(),
        }
    }
    match cur {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Apply a single TOML leaf (from `config apply --file`) to `cfg`,
/// preserving its native type for the `observability` sub-tree and
/// stringifying for the fixed-key dispatch table.
fn apply_toml_leaf(
    cfg: &mut CorelinkConfig,
    key: &str,
    val: toml::Value,
) -> Result<(), ConfigError> {
    if let Some(rest) = key.strip_prefix("observability.") {
        set_observability_value(cfg, rest, val, key)
    } else {
        let s = toml_scalar_to_string(key, &val)?;
        apply_key(cfg, key, &s)
    }
}

/// Flatten a nested TOML table into `(dotted_key, leaf_value)` pairs.
/// Tables are recursed; every non-table value (scalar or array) is a leaf.
fn flatten_toml(table: &toml::Table, prefix: &str, out: &mut Vec<(String, toml::Value)>) {
    for (k, v) in table {
        let full = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            toml::Value::Table(t) => flatten_toml(t, &full, out),
            other => out.push((full, other.clone())),
        }
    }
}

/// Apply an entire TOML config document (`corelink config apply --file
/// <path>`). Every leaf key is applied through the same validation the
/// `config set` path uses. Returns the number of keys applied.
pub fn apply_file(path: &Path) -> Result<usize, ConfigError> {
    let raw = std::fs::read_to_string(path).map_err(ConfigError::Read)?;
    let table: toml::Table = toml::from_str(&raw)?;
    let mut leaves: Vec<(String, toml::Value)> = Vec::new();
    flatten_toml(&table, "", &mut leaves);
    let count = leaves.len();
    let mut cfg = load()?;
    for (k, v) in leaves {
        apply_toml_leaf(&mut cfg, &k, v)?;
    }
    save(&cfg)?;
    Ok(count)
}

/// Apply a TOML document into an in-memory config (test/lib helper;
/// filesystem-free). Returns the number of leaf keys applied.
#[allow(dead_code)]
pub fn apply_document_to_cfg(cfg: &mut CorelinkConfig, doc: &str) -> Result<usize, ConfigError> {
    let table: toml::Table = toml::from_str(doc)?;
    let mut leaves: Vec<(String, toml::Value)> = Vec::new();
    flatten_toml(&table, "", &mut leaves);
    let count = leaves.len();
    for (k, v) in leaves {
        apply_toml_leaf(cfg, &k, v)?;
    }
    Ok(count)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod observability_tests {
    use super::*;

    #[test]
    fn infer_toml_value_types() {
        assert_eq!(infer_toml_value("true"), toml::Value::Boolean(true));
        assert_eq!(infer_toml_value("false"), toml::Value::Boolean(false));
        assert_eq!(infer_toml_value("2000"), toml::Value::Integer(2000));
        assert_eq!(
            infer_toml_value("us1"),
            toml::Value::String("us1".to_owned())
        );
    }

    #[test]
    fn set_and_read_nested_observability_key() {
        let mut cfg = CorelinkConfig::default();
        apply_key(&mut cfg, "observability.export.variant", "datadog").unwrap();
        apply_key(&mut cfg, "observability.export.datadog.timeout_ms", "2000").unwrap();
        apply_key(
            &mut cfg,
            "observability.export.datadog.opt_out_low_value_metrics",
            "true",
        )
        .unwrap();

        assert_eq!(
            read_key(&cfg, "observability.export.variant").unwrap(),
            "datadog"
        );
        assert_eq!(
            read_key(&cfg, "observability.export.datadog.timeout_ms").unwrap(),
            "2000"
        );
        assert_eq!(
            read_key(&cfg, "observability.export.datadog.opt_out_low_value_metrics").unwrap(),
            "true"
        );
        // Absent path reads as (unset), never an error.
        assert_eq!(
            read_key(&cfg, "observability.export.datadog.nope").unwrap(),
            "(unset)"
        );
    }

    #[test]
    fn invalid_variant_rejected() {
        let mut cfg = CorelinkConfig::default();
        let err = apply_key(&mut cfg, "observability.export.variant", "splunk");
        assert!(err.is_err(), "unknown variant must be rejected");
    }

    #[test]
    fn apply_document_populates_nested_tables_with_types() {
        // Mirrors the documented `corelink config apply --file` shape.
        let doc = r#"
[defaults]
tenant_id = "acme-prod"

[observability.export]
variant = "otel_collector"

[observability.export.otel_collector]
endpoint = "https://collector.example.com:4318"
protocol = "http"
timeout_ms = 2000
opt_out_low_value_metrics = false
"#;
        let mut cfg = CorelinkConfig::default();
        let n = apply_document_to_cfg(&mut cfg, doc).unwrap();
        assert_eq!(n, 6, "6 leaf keys expected");
        assert_eq!(cfg.defaults.tenant_id.as_deref(), Some("acme-prod"));
        assert_eq!(
            read_key(&cfg, "observability.export.variant").unwrap(),
            "otel_collector"
        );
        // Integer type preserved from the file (not stringified).
        let obs = cfg.observability.as_ref().unwrap();
        let timeout = obs
            .get("export")
            .and_then(|e| e.get("otel_collector"))
            .and_then(|c| c.get("timeout_ms"))
            .cloned();
        assert_eq!(timeout, Some(toml::Value::Integer(2000)));
    }

    #[test]
    fn observability_survives_toml_roundtrip() {
        let mut cfg = CorelinkConfig::default();
        apply_key(&mut cfg, "observability.export.variant", "grafana_cloud").unwrap();
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let reparsed: CorelinkConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(
            read_key(&reparsed, "observability.export.variant").unwrap(),
            "grafana_cloud"
        );
    }
}

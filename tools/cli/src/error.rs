//! CLI error types for `corelink-cli` (WI-S15-001).

use thiserror::Error;

/// Top-level CLI error.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CliError {
    /// PAT could not be resolved from env var or config file.
    #[error("No PAT found. Set env var CORELINK_PAT or run `corelink config set auth.pat <value>`.")]
    PatNotFound,

    /// PAT format is malformed (not `corelink_<env>_<token_id>.<secret>.<sig>`).
    #[error("PAT format invalid (expected `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`). Regenerate via admin UI.")]
    PatMalformed,

    /// Config file I/O error.
    #[error("Config file error: {0}")]
    Config(#[from] ConfigError),

    /// HTTP / network error.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON serialisation error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error (file read/write).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error with context.
    #[error("{0}")]
    Other(String),

    /// Wave-19 (WI-S09-008 §6) — `audit verify-ndjson --url`
    /// detected the wave-18 `x-corelink-audit-export-aborted` HTTP
    /// trailer mid-stream. The detailed canonical diagnostic is
    /// already printed to stderr by
    /// `commands::verify_ndjson_http::run_verify_ndjson_http`; this
    /// variant exists so `main` can translate to sysexits DATAERR
    /// (65) without re-printing.
    #[error("audit-export aborted mid-stream (see diagnostic on stderr)")]
    AuditExportAborted,
}

/// Config-file specific errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Config file could not be read.
    #[error("Cannot read config file: {0}")]
    Read(std::io::Error),

    /// Config file could not be written.
    #[error("Cannot write config file: {0}")]
    Write(std::io::Error),

    /// Config file TOML parse failure.
    #[error("Config TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),

    /// Config file TOML serialise failure.
    #[error("Config TOML serialise error: {0}")]
    Serialise(#[from] toml::ser::Error),

    /// Config key unknown or invalid.
    #[error("Unknown config key: {0}")]
    UnknownKey(String),

    /// Config file permissions are insecure (not 0o600 on Unix).
    #[error("Config file permissions are insecure (expected 0o600); run: chmod 600 ~/.corelink/config.toml")]
    InsecurePermissions,
}

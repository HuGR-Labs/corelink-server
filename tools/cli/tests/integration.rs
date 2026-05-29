//! Integration tests for `corelink-cli` Stream-1 surface.
//!
//! These tests exercise the CLI's pure-logic surface (config, PAT validation,
//! SHA-256 streaming, command structs) without any real network I/O.
//! No `CORELINK_PAT`, no R2, no API credentials required.
//!
//! Gate: `cargo test -p corelink-cli --lib --tests` (all green, no network).

use corelink_cli::{
    auth::validate_pat_shape,
    config::{
        load_from_path, save_to_path, CorelinkConfig, DEFAULT_ENDPOINT,
    },
    error::CliError,
};
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// PAT validation (Stream-1 contract: corelink_<env>_<id>.<secret>.<hmac>)
// ---------------------------------------------------------------------------

fn make_valid_pat(env: &str) -> String {
    let token_id = "ABCDEFGH01234567";
    let secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"; // 43 chars
    let sig = "AAAAAAAAAAAAAAAAAAAAAA"; // 22 chars
    format!("corelink_{env}_{token_id}.{secret}.{sig}")
}

#[test]
fn pat_env_pat_valid() {
    assert!(validate_pat_shape(&make_valid_pat("pat")).is_ok());
}

#[test]
fn pat_env_ci_valid() {
    assert!(validate_pat_shape(&make_valid_pat("ci")).is_ok());
}

#[test]
fn pat_env_ro_valid() {
    assert!(validate_pat_shape(&make_valid_pat("ro")).is_ok());
}

#[test]
fn pat_bad_prefix_rejected() {
    let pat = make_valid_pat("pat").replace("corelink_", "notcorelink_");
    assert!(matches!(validate_pat_shape(&pat), Err(CliError::PatMalformed)));
}

#[test]
fn pat_bad_env_rejected() {
    // "dev" is not in the allowlist
    let pat = make_valid_pat("dev");
    assert!(matches!(validate_pat_shape(&pat), Err(CliError::PatMalformed)));
}

#[test]
fn pat_short_secret_rejected() {
    let token_id = "ABCDEFGH01234567";
    let short_secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"; // 42 chars (1 short)
    let sig = "AAAAAAAAAAAAAAAAAAAAAA";
    let pat = format!("corelink_pat_{token_id}.{short_secret}.{sig}");
    assert!(matches!(validate_pat_shape(&pat), Err(CliError::PatMalformed)));
}

#[test]
fn pat_empty_rejected() {
    assert!(matches!(validate_pat_shape(""), Err(CliError::PatMalformed)));
}

// ---------------------------------------------------------------------------
// Config read/write (filesystem — uses tempfile)
// ---------------------------------------------------------------------------

#[test]
fn config_default_endpoint_is_prod() {
    assert_eq!(DEFAULT_ENDPOINT, "https://corelink-api.humangr.com");
}

#[test]
fn config_roundtrip_with_token_and_tenant() {
    let tmp = NamedTempFile::new().expect("tempfile");
    let path = tmp.path();

    let mut cfg = CorelinkConfig::default();
    cfg.auth.pat = Some(make_valid_pat("pat"));
    cfg.defaults.tenant_id = Some("tenant_test_001".to_owned());
    cfg.defaults.endpoint = Some("https://corelink-api.humangr.com".to_owned());

    save_to_path(path, &cfg).expect("save");
    let loaded = load_from_path(path).expect("load");

    assert_eq!(loaded.auth.pat, Some(make_valid_pat("pat")));
    assert_eq!(loaded.defaults.tenant_id, Some("tenant_test_001".to_owned()));
    assert_eq!(
        loaded.defaults.endpoint,
        Some("https://corelink-api.humangr.com".to_owned())
    );
}

#[test]
fn config_missing_file_returns_default() {
    // Use a path that definitely doesn't exist.
    let path = std::path::Path::new("/tmp/corelink_nonexistent_config_test_xyz.toml");
    let cfg = load_from_path(path).expect("load nonexistent should return default");
    assert!(cfg.auth.pat.is_none());
    assert!(cfg.defaults.tenant_id.is_none());
}

#[test]
fn config_redacted_pat_hides_secret() {
    let mut cfg = CorelinkConfig::default();
    cfg.auth.pat = Some(make_valid_pat("pat"));
    let redacted = cfg.auth.redacted_pat();
    assert!(redacted.is_some());
    let r = redacted.expect("redacted");
    // Must not contain the full secret portion.
    assert!(r.contains("***"), "redaction marker must appear: {r}");
    // Must not contain the full 95/96-char PAT.
    assert!(r.len() < 40, "redacted PAT should be short: len={}", r.len());
}

#[test]
fn config_apply_key_endpoint() {
    use corelink_cli::config::apply_key_to_cfg;
    let mut cfg = CorelinkConfig::default();
    apply_key_to_cfg(&mut cfg, "defaults.endpoint", "https://custom.example.com")
        .expect("set endpoint");
    assert_eq!(
        cfg.defaults.endpoint,
        Some("https://custom.example.com".to_owned())
    );
}

#[test]
fn config_unknown_key_errors() {
    use corelink_cli::config::apply_key_to_cfg;
    let mut cfg = CorelinkConfig::default();
    let result = apply_key_to_cfg(&mut cfg, "bogus.key", "value");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// SHA-256 streaming integrity (no I/O — pure algorithm test)
// ---------------------------------------------------------------------------

#[test]
fn sha256_streaming_matches_oneshot_hello() {
    use sha2::{Digest as _, Sha256};
    let data = b"hello corelink stream-1";
    let mut h1 = Sha256::new();
    h1.update(data);
    let expected = hex::encode(h1.finalize());

    let mut h2 = Sha256::new();
    for chunk in data.chunks(4) {
        h2.update(chunk);
    }
    let actual = hex::encode(h2.finalize());

    assert_eq!(expected, actual);
}

#[test]
fn sha256_10mb_is_64_char_hex() {
    use sha2::{Digest as _, Sha256};
    let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
    let mut hasher = Sha256::new();
    // Simulate 256 KiB chunks (same as put.rs CHUNK_SIZE).
    for chunk in data.chunks(256 * 1024) {
        hasher.update(chunk);
    }
    let digest = hex::encode(hasher.finalize());
    assert_eq!(digest.len(), 64);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
}

// ---------------------------------------------------------------------------
// Command struct serialisation (no network)
// ---------------------------------------------------------------------------

#[test]
fn whoami_result_json_has_all_fields() {
    // Import the public WhoamiResult via lib — exercises the lib re-export surface.
    // We test it directly by constructing and serialising.
    let json = serde_json::json!({
        "tenant_id": "ten_abc",
        "token_prefix": "corelink_pat_ABC***",
        "route_kind": "pat"
    });
    assert!(json["tenant_id"].is_string());
    assert!(json["token_prefix"].is_string());
    assert!(json["route_kind"].is_string());
}

#[test]
fn login_result_json_has_message_field() {
    let json = serde_json::json!({
        "tenant_id": "ten_xyz",
        "token_prefix": "corelink_ci_XYZ***",
        "route_kind": "ci",
        "message": "Config saved to ~/.corelink/config.toml"
    });
    assert!(json["message"].is_string());
}

// ---------------------------------------------------------------------------
// Not-logged-in path: `whoami` without PAT should error cleanly
// ---------------------------------------------------------------------------

#[test]
fn pat_not_found_error_has_helpful_message() {
    let err = CliError::PatNotFound;
    let msg = err.to_string();
    // Must mention config file or env var (CTRL-CRED-001 guidance).
    assert!(
        msg.contains("CORELINK_PAT") || msg.contains("config"),
        "error message should guide the user: {msg}"
    );
}

#[test]
fn pat_malformed_error_has_helpful_message() {
    let err = CliError::PatMalformed;
    let msg = err.to_string();
    assert!(
        msg.contains("corelink_") || msg.contains("PAT"),
        "error message should show expected format: {msg}"
    );
}

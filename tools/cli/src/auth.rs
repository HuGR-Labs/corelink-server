//! Auth resolution for `corelink-cli` (WI-S15-001).
//!
//! Order: env var `CORELINK_PAT` first → config file `[auth].pat` fallback →
//! error if neither set (CTRL-CRED-001 compliant).
//!
//! PAT format validation: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`
//! regex match per `auth_model.md §2.2` cycle 9 SEAL decision (a).

use crate::error::CliError;

/// PAT validation regex — checks structural shape without full crypto.
/// Full validation (HMAC-SHA256 sig) happens server-side.
///
/// Shape:
/// - `corelink_` literal (9 chars)
/// - env: `pat|ci|ro` (2-3 chars)
/// - `_` separator
/// - token_id: 16-char Crockford base32 `[0-9A-Za-z]`
/// - `.` separator
/// - random_secret: 43-char base64url no-pad `[A-Za-z0-9_-]`
/// - `.` separator
/// - hmac_sig: 22-char base64url no-pad `[A-Za-z0-9_-]`
const PAT_PREFIX: &str = "corelink_";
const PAT_TOKEN_ID_LEN: usize = 16;
const PAT_RANDOM_SECRET_LEN: usize = 43;
const PAT_HMAC_SIG_LEN: usize = 22;

/// Resolve PAT from `CORELINK_PAT` env var first, then config file.
///
/// Returns [`CliError::PatNotFound`] if neither source is set, or
/// [`CliError::PatMalformed`] if the resolved value fails shape validation.
pub fn resolve_pat() -> Result<String, CliError> {
    let raw = if let Ok(v) = std::env::var("CORELINK_PAT") {
        if !v.is_empty() {
            v
        } else {
            load_from_config()?
        }
    } else {
        load_from_config()?
    };
    validate_pat_shape(&raw)?;
    Ok(raw)
}

fn load_from_config() -> Result<String, CliError> {
    let cfg = crate::config::load()?;
    cfg.auth
        .pat
        .filter(|p| !p.is_empty())
        .ok_or(CliError::PatNotFound)
}

/// Validates the structural shape of a PAT plaintext.
///
/// Does NOT perform cryptographic verification (that is server-side).
/// Returns [`CliError::PatMalformed`] for any shape deviation.
pub fn validate_pat_shape(pat: &str) -> Result<(), CliError> {
    let bytes = pat.as_bytes();

    // Prefix check.
    if !pat.starts_with(PAT_PREFIX) {
        return Err(CliError::PatMalformed);
    }

    // env + token_id separator position depends on env length (2 or 3).
    let after_prefix = &pat[PAT_PREFIX.len()..];

    // Find the `_` that terminates the env segment.
    let env_sep = after_prefix.find('_').ok_or(CliError::PatMalformed)?;
    let env = &after_prefix[..env_sep];
    if env != "pat" && env != "ci" && env != "ro" {
        return Err(CliError::PatMalformed);
    }

    let after_env = &after_prefix[env_sep + 1..];

    // token_id: 16 Crockford base32 chars.
    if after_env.len() < PAT_TOKEN_ID_LEN + 1 {
        return Err(CliError::PatMalformed);
    }
    let token_id = &after_env[..PAT_TOKEN_ID_LEN];
    if !token_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(CliError::PatMalformed);
    }

    let after_token = &after_env[PAT_TOKEN_ID_LEN..];
    if !after_token.starts_with('.') {
        return Err(CliError::PatMalformed);
    }
    let after_dot1 = &after_token[1..];

    // random_secret: 43 base64url no-pad chars.
    if after_dot1.len() < PAT_RANDOM_SECRET_LEN + 1 {
        return Err(CliError::PatMalformed);
    }
    let secret = &after_dot1[..PAT_RANDOM_SECRET_LEN];
    if !secret
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CliError::PatMalformed);
    }

    let after_dot2_check = &after_dot1[PAT_RANDOM_SECRET_LEN..];
    if !after_dot2_check.starts_with('.') {
        return Err(CliError::PatMalformed);
    }
    let hmac_sig = &after_dot2_check[1..];

    // hmac_sig: exactly 22 base64url no-pad chars.
    if hmac_sig.len() != PAT_HMAC_SIG_LEN {
        return Err(CliError::PatMalformed);
    }
    if !hmac_sig
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CliError::PatMalformed);
    }

    // Total length guard (95 or 96 chars).
    let total = bytes.len();
    if total != 95 && total != 96 {
        return Err(CliError::PatMalformed);
    }

    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    fn make_pat(env: &str) -> String {
        // token_id: 16 alphanumeric chars
        let token_id = "ABCDEFGH01234567";
        // random_secret: 43 base64url no-pad chars
        let secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        // hmac_sig: 22 base64url no-pad chars
        let sig = "AAAAAAAAAAAAAAAAAAAAAA";
        format!("corelink_{env}_{token_id}.{secret}.{sig}")
    }

    #[test]
    fn valid_pat_env_variants() {
        assert!(validate_pat_shape(&make_pat("pat")).is_ok(), "env=pat");
        assert!(validate_pat_shape(&make_pat("ci")).is_ok(), "env=ci");
        assert!(validate_pat_shape(&make_pat("ro")).is_ok(), "env=ro");
    }

    #[test]
    fn invalid_prefix_rejected() {
        let pat = make_pat("pat").replace("corelink_", "badprefix_");
        assert!(matches!(
            validate_pat_shape(&pat),
            Err(CliError::PatMalformed)
        ));
    }

    #[test]
    fn invalid_env_rejected() {
        let pat = make_pat("dev"); // 'dev' not in allowlist
        assert!(matches!(
            validate_pat_shape(&pat),
            Err(CliError::PatMalformed)
        ));
    }

    #[test]
    fn short_secret_rejected() {
        let token_id = "ABCDEFGH01234567";
        let short_secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"[..42].to_owned();
        let sig = "AAAAAAAAAAAAAAAAAAAAAA";
        let pat = format!("corelink_pat_{token_id}.{short_secret}.{sig}");
        assert!(matches!(
            validate_pat_shape(&pat),
            Err(CliError::PatMalformed)
        ));
    }

    #[test]
    fn wrong_separator_rejected() {
        let pat = make_pat("pat").replace('.', "_");
        assert!(matches!(
            validate_pat_shape(&pat),
            Err(CliError::PatMalformed)
        ));
    }

    #[test]
    fn empty_rejected() {
        assert!(matches!(
            validate_pat_shape(""),
            Err(CliError::PatMalformed)
        ));
    }

    #[test]
    fn literal_rejected() {
        assert!(matches!(
            validate_pat_shape("--pat=secret"),
            Err(CliError::PatMalformed)
        ));
    }
}

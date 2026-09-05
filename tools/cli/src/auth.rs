//! Auth resolution for `corelink-cli` (WI-S15-001).
//!
//! Order: env var `CORELINK_PAT` first → config file `[auth].pat` fallback →
//! error if neither set (CTRL-CRED-001 compliant). PAT shape validation is
//! delegated to `corelink-pat`, the server's canonical parser (B-162).

use crate::error::CliError;

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

/// Validate a PAT using the exact byte-oriented parser used by the server.
///
/// This performs structural validation, including canonical environment
/// tags, Crockford token-id charset, exact byte lengths, and base64url
/// decoding. Cryptographic verification remains server-side.
pub fn validate_pat_shape(pat: &str) -> Result<(), CliError> {
    corelink_pat::parse_plaintext(pat)
        .map(|_| ())
        .map_err(|_| CliError::PatMalformed)
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
        let token_id = "ABCDEFGH01234567";
        let secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
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
    fn invalid_crockford_token_id_is_rejected() {
        for illegal in ['I', 'L', 'O', 'U', 'i', 'l', 'o', 'u', '!'] {
            let pat = make_pat("pat").replacen('A', &illegal.to_string(), 1);
            assert!(matches!(
                validate_pat_shape(&pat),
                Err(CliError::PatMalformed)
            ));
        }
    }

    #[test]
    fn invalid_prefix_and_env_are_rejected() {
        assert!(matches!(
            validate_pat_shape(&make_pat("pat").replace("corelink_", "badprefix_")),
            Err(CliError::PatMalformed)
        ));
        assert!(matches!(
            validate_pat_shape(&make_pat("dev")),
            Err(CliError::PatMalformed)
        ));
    }

    #[test]
    fn unicode_and_length_mutations_are_rejected_without_panic() {
        let valid = make_pat("pat");
        for candidate in [
            format!("{valid}é"),
            valid[..valid.len() - 1].to_owned(),
            format!("{valid}A"),
            valid.replace("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "!"),
        ] {
            assert!(matches!(
                validate_pat_shape(&candidate),
                Err(CliError::PatMalformed)
            ));
        }
    }
}

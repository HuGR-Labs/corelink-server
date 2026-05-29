//! `corelink login --token=<PAT>` — store token + fetch + cache tenant_id.
//!
//! Writes the PAT to `~/.corelink/config.toml` with 0600 permissions, then
//! calls `/v1/users/me` to resolve the `tenant_id` and cache it in config.
//!
//! Security controls:
//! - PAT is validated for shape before writing (CTRL-CRED-001).
//! - Config file is written with 0600 permissions (secrets at rest).
//! - World-readable configs are rejected on load (existing security gate).

use std::fmt;

use serde::Serialize;

use crate::auth::validate_pat_shape;
use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Output for `corelink login`.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct LoginResult {
    /// Tenant ID resolved from `/v1/users/me`.
    pub tenant_id: String,
    /// Token prefix (redacted).
    pub token_prefix: String,
    /// Route kind.
    pub route_kind: String,
    /// Confirmation message.
    pub message: String,
}

impl fmt::Display for LoginResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Logged in as tenant {} ({})\nConfig saved to ~/.corelink/config.toml",
            self.tenant_id, self.route_kind
        )
    }
}

/// Run `corelink login --token=<PAT>`.
///
/// Validates the PAT shape, writes it to config (0600 perms), then calls
/// `whoami` to resolve + cache the tenant_id.
pub async fn run(token: &str, format: OutputFormat) -> Result<(), CliError> {
    // Validate shape before persisting (CTRL-CRED-001).
    validate_pat_shape(token)?;

    // Write the PAT into config.
    let mut cfg = crate::config::load()?;
    cfg.auth.pat = Some(token.to_owned());
    crate::config::save(&cfg)?;

    // Resolve tenant_id via whoami.
    let client = CorelinkClient::new(token.to_owned())?;
    let resp = client.whoami().await.map_err(|e| {
        CliError::Other(format!(
            "login: token saved but whoami failed ({e}). \
             Run `corelink whoami` once connectivity is restored."
        ))
    })?;

    // Cache tenant_id in config.
    let mut cfg2 = crate::config::load()?;
    cfg2.defaults.tenant_id = Some(resp.tenant_id.clone());
    crate::config::save(&cfg2)?;

    let result = LoginResult {
        tenant_id: resp.tenant_id,
        token_prefix: resp.token_prefix,
        route_kind: resp.route_kind,
        message: "Config saved to ~/.corelink/config.toml".to_owned(),
    };

    let fmt = Formatter::new(format);
    fmt.emit(&result).map_err(CliError::Json)?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;

    #[test]
    fn login_result_display() {
        let r = LoginResult {
            tenant_id: "ten_abc".to_owned(),
            token_prefix: "corelink_pat_ABC***".to_owned(),
            route_kind: "pat".to_owned(),
            message: "saved".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("ten_abc"));
        assert!(s.contains("pat"));
    }

    #[test]
    fn login_result_serialises() {
        let r = LoginResult {
            tenant_id: "t".to_owned(),
            token_prefix: "p".to_owned(),
            route_kind: "ro".to_owned(),
            message: "ok".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"tenant_id\""));
        assert!(json.contains("\"message\""));
    }
}

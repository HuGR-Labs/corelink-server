//! `corelink whoami` — query `/v1/users/me` and display identity.
//!
//! Also caches the `tenant_id` in `~/.corelink/config.toml` so subsequent
//! `put`/`get`/`ac` commands can use the tenant-scoped API paths without
//! requiring an explicit `--tenant` flag.

use std::fmt;

use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Output for `corelink whoami`.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct WhoamiResult {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Token prefix (PAT redacted per CTRL-CRED-001).
    pub token_prefix: String,
    /// Route kind: `pat`, `ci`, or `ro`.
    pub route_kind: String,
}

impl fmt::Display for WhoamiResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tenant_id:    {}\ntoken_prefix: {}\nroute_kind:   {}",
            self.tenant_id, self.token_prefix, self.route_kind
        )
    }
}

/// Run `corelink whoami`.
pub async fn run(client: &CorelinkClient, format: OutputFormat) -> Result<(), CliError> {
    let resp = client.whoami().await?;

    // Cache tenant_id in config for subsequent commands.
    if let Ok(mut cfg) = crate::config::load() {
        if cfg.defaults.tenant_id.as_deref() != Some(&resp.tenant_id) {
            cfg.defaults.tenant_id = Some(resp.tenant_id.clone());
            // Best-effort save — don't fail whoami if config write fails.
            let _ = crate::config::save(&cfg);
        }
    }

    let result = WhoamiResult {
        tenant_id: resp.tenant_id,
        token_prefix: resp.token_prefix,
        route_kind: resp.route_kind,
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
    fn whoami_result_display() {
        let r = WhoamiResult {
            tenant_id: "tenant_abc".to_owned(),
            token_prefix: "corelink_pat_ABC***".to_owned(),
            route_kind: "pat".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("tenant_abc"));
        assert!(s.contains("pat"));
    }

    #[test]
    fn whoami_result_serialises() {
        let r = WhoamiResult {
            tenant_id: "t".to_owned(),
            token_prefix: "p".to_owned(),
            route_kind: "ro".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"tenant_id\""));
        assert!(json.contains("\"route_kind\""));
    }
}

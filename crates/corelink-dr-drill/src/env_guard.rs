//! Staging-only environment guard (Lote 10.17 codex P0 canonical fix).
//!
//! The DR drill MUST run in staging only at GA. Prior versions enforced
//! `env=staging` as a Gherkin assertion — bypassable. Canonical: explicit
//! env check is the FIRST line of any drill execution; throws immediately
//! if env != staging.

use serde::{Deserialize, Serialize};

use crate::error::DrillError;

/// Drill execution environment.
///
/// `#[non_exhaustive]` — callers MUST use a wildcard arm per CoreLink
/// codex §9.3.
///
/// # Examples
///
/// ```
/// use corelink_dr_drill::DrillEnv;
///
/// assert_eq!(DrillEnv::Staging.as_str(), "staging");
/// assert!(DrillEnv::Staging.is_safe_for_chaos());
/// assert!(!DrillEnv::Production.is_safe_for_chaos());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DrillEnv {
    /// Staging environment — DR drill is allowed.
    Staging,
    /// Local test environment — DR drill is allowed (in-memory only).
    Test,
    /// Production environment — DR drill is FORBIDDEN (hard-fail).
    Production,
}

impl DrillEnv {
    /// Canonical string label for metrics + audit events.
    pub fn as_str(self) -> &'static str {
        match self {
            DrillEnv::Staging => "staging",
            DrillEnv::Test => "test",
            DrillEnv::Production => "production",
        }
    }

    /// Returns `true` if chaos / DR drill execution is permitted in this env.
    ///
    /// Only `Staging` and `Test` are safe; `Production` returns `false`
    /// (anti-prod hit guarantee per WI §6.1 + spec contract §15 row 1).
    pub fn is_safe_for_chaos(self) -> bool {
        matches!(self, DrillEnv::Staging | DrillEnv::Test)
    }
}

impl std::fmt::Display for DrillEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Require that the current environment is safe for chaos execution.
///
/// This is the canonical staging-only enforcement boundary. Returns
/// [`DrillError::ProdEnvForbidden`] immediately if `env == Production`.
///
/// # Examples
///
/// ```
/// use corelink_dr_drill::{require_staging, DrillEnv, DrillError};
///
/// assert!(require_staging(DrillEnv::Staging).is_ok());
/// assert!(require_staging(DrillEnv::Test).is_ok());
///
/// let err = require_staging(DrillEnv::Production).unwrap_err();
/// assert!(matches!(err, DrillError::ProdEnvForbidden));
/// ```
pub fn require_staging(env: DrillEnv) -> Result<(), DrillError> {
    if env.is_safe_for_chaos() {
        Ok(())
    } else {
        Err(DrillError::ProdEnvForbidden)
    }
}

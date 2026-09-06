//! Shared container-side PAT verifier for cache adapters.
//!
//! B126-H2 keeps this stable module path as the public reanchor point while
//! separating lookup, admission, crypto memoisation, and verifier wiring.
//! Symbol map:
//! - lookup: PatRow, PatRowLookup, SingleFlightPatLookup, D1 SQL/decoding
//! - gate: PerTenantGate and bounded admission constants
//! - crypto: SecretMatchMemo, FlightGroup, fingerprints, outcomes
//! - verifier: PatVerifier, VerifyError, HMAC → D1 → Argon2id → scope
//!
//! The extraction is intentionally mechanical: public names below are the same
//! names and paths used by adapter route shells; no authorization state or
//! signed-chain semantics are changed.

mod adapter_pat_crypto;
mod adapter_pat_gate;
mod adapter_pat_lookup;
mod adapter_pat_verifier;

pub use adapter_pat_lookup::{PatRow, PatRowLookup, SingleFlightPatLookup};
pub use adapter_pat_verifier::{PatVerifier, VerifyError};

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_R,
        SCOPE_CACHE_RW,
    };
    use tokio::sync::Semaphore;
    use uuid::Uuid;

    use super::adapter_pat_crypto::{
        secret_match_fingerprint, FlightGroup, SecretMatchMemo, FLIGHT_GROUP_CAP,
    };
    use super::adapter_pat_gate::{
        ARGON2_PERMIT_WAIT, ARGON2_PER_TENANT_PERMITS, UNKNOWN_TOKEN_BUCKET,
    };
    use super::adapter_pat_lookup::{pat_row_from_columns, PAT_LOOKUP_SQL, PAT_URL_MAP_COREAD_SQL};
    use super::*;

    include!("adapter_pat_tests_1.rs");
    include!("adapter_pat_tests_2.rs");
    include!("adapter_pat_tests_3.rs");
}

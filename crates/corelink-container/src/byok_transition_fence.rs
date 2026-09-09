//! Tenant-wide D1 fencing for BYOK state transitions.
//!
//! Data operations acquire a short-lived intent before touching CAS or AC.
//! Control-plane transitions acquire an exclusive fence against the exact
//! `(config_version, config_state, tenant.byok_status)` snapshot they intend to
//! change. D1 serializes the conditional writes, so a tenant can have live data
//! intents or a live transition fence, never both.
//!
//! A transition owner must condition its state mutation on the live, exact
//! fence token, increment `tenant_byok_config.config_version`, and release the
//! fence in the same D1 transaction. Merely reading a fence before a separate
//! mutation is not sufficient.

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use thiserror::Error;

use crate::storage::d1_http::{D1HttpClient, D1Row};

/// Upper bound for both kinds of lease. Operations must finish or abort before
/// this deadline; an expired token never authorizes a mutation or release.
pub const MAX_BYOK_FENCE_LEASE: Duration = Duration::from_secs(15 * 60);

/// Highest persisted epoch/generation. Reserving `i64::MAX` prevents SQLite's
/// integer addition from silently promoting an exhausted counter to `REAL`.
pub const MAX_BYOK_COUNTER: i64 = i64::MAX - 1;

const LOAD_SNAPSHOT_SQL: &str = "SELECT g.gate_epoch, g.current_generation, c.config_version, \
        COALESCE(c.state, 'absent') AS config_state, t.byok_status \
        FROM byok_tenant_gate g JOIN tenant t ON t.tenant_id = g.tenant_id \
        LEFT JOIN tenant_byok_config c ON c.tenant_id = g.tenant_id \
        WHERE g.tenant_id = ?1 LIMIT 1";

const ACQUIRE_DATA_INTENT_SQL: &str = "INSERT INTO byok_data_intent \
        (token, tenant_id, operation, observed_gate_epoch, observed_generation, observed_config_version, \
         observed_config_state, observed_byok_status, acquired_at_ms, expires_at_ms) \
    SELECT ?1, g.tenant_id, ?2, g.gate_epoch, g.current_generation, c.config_version, \
           COALESCE(c.state, 'absent'), t.byok_status, \
           (CAST(strftime('%s', 'now') AS INTEGER) * 1000), \
           (CAST(strftime('%s', 'now') AS INTEGER) * 1000) + ?3 \
      FROM byok_tenant_gate g JOIN tenant t ON t.tenant_id = g.tenant_id \
      LEFT JOIN tenant_byok_config c ON c.tenant_id = g.tenant_id \
     WHERE g.tenant_id = ?4 AND t.byok_status = 'active' \
       AND COALESCE(c.state, 'absent') <> 'shredded' \
       AND NOT EXISTS (SELECT 1 FROM byok_transition_fence f \
                        WHERE f.tenant_id = g.tenant_id AND f.outcome = 'active' AND f.expires_at_ms > \
                        (CAST(strftime('%s', 'now') AS INTEGER) * 1000)) \
    RETURNING observed_gate_epoch, observed_generation, observed_config_version, observed_config_state, \
              observed_byok_status, expires_at_ms";

const ACQUIRE_TRANSITION_FENCE_SQL: &str = "INSERT INTO byok_transition_fence \
        (tenant_id, token, epoch, observed_gate_epoch, observed_generation, \
         observed_config_version, observed_config_state, \
         observed_byok_status, acquired_at_ms, expires_at_ms) \
    SELECT g.tenant_id, ?1, \
           COALESCE((SELECT MAX(prior.epoch) FROM byok_transition_fence prior \
                     WHERE prior.tenant_id = g.tenant_id), 0) + 1, \
           g.gate_epoch, g.current_generation, c.config_version, \
           COALESCE(c.state, 'absent'), t.byok_status, \
           (CAST(strftime('%s', 'now') AS INTEGER) * 1000), \
           (CAST(strftime('%s', 'now') AS INTEGER) * 1000) + ?2 \
      FROM byok_tenant_gate g JOIN tenant t ON t.tenant_id = g.tenant_id \
      LEFT JOIN tenant_byok_config c ON c.tenant_id = g.tenant_id \
     WHERE g.tenant_id = ?3 AND g.gate_epoch = ?4 AND g.current_generation = ?5 \
       AND COALESCE((SELECT MAX(prior.epoch) FROM byok_transition_fence prior \
                     WHERE prior.tenant_id = g.tenant_id), 0) < 9223372036854775806 \
       AND ((?6 IS NULL AND c.config_version IS NULL) OR c.config_version = ?6) \
       AND COALESCE(c.state, 'absent') = ?7 AND t.byok_status = ?8 \
       AND COALESCE(c.state, 'absent') <> 'shredded' \
       AND t.byok_status <> 'revoked' \
       AND NOT EXISTS (SELECT 1 FROM byok_data_intent i \
                        WHERE i.tenant_id = g.tenant_id AND i.outcome = 'active' AND i.expires_at_ms > \
                        (CAST(strftime('%s', 'now') AS INTEGER) * 1000)) \
       AND NOT EXISTS (SELECT 1 FROM byok_transition_fence live \
                        WHERE live.tenant_id = g.tenant_id AND live.outcome = 'active' AND live.expires_at_ms > \
                        (CAST(strftime('%s', 'now') AS INTEGER) * 1000)) \
    RETURNING epoch, observed_gate_epoch, observed_generation, observed_config_version, \
              observed_config_state, observed_byok_status, expires_at_ms";

const RELEASE_DATA_INTENT_SQL: &str = "UPDATE byok_data_intent SET \
    outcome = CASE WHEN expires_at_ms > (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
                   THEN 'completed' ELSE 'expired' END, \
    completed_at_ms = (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
    WHERE tenant_id = ?1 AND token = ?2 AND outcome = 'active' RETURNING expires_at_ms, \
    CASE WHEN expires_at_ms > (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
         THEN 1 ELSE 0 END AS was_live";
const ABORT_TRANSITION_FENCE_SQL: &str = "UPDATE byok_transition_fence SET \
    outcome = CASE WHEN expires_at_ms > (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
                   THEN 'aborted' ELSE 'expired' END, \
    completed_at_ms = (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
    WHERE tenant_id = ?1 AND token = ?2 AND outcome = 'active' RETURNING expires_at_ms, \
    CASE WHEN expires_at_ms > (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
         THEN 1 ELSE 0 END AS was_live";

const RENEW_DATA_INTENT_SQL: &str = "UPDATE byok_data_intent SET expires_at_ms = \
    (CAST(strftime('%s', 'now') AS INTEGER) * 1000) + ?3 \
    WHERE tenant_id = ?1 AND token = ?2 AND outcome = 'active' AND expires_at_ms > \
    (CAST(strftime('%s', 'now') AS INTEGER) * 1000) RETURNING expires_at_ms";

/// Exact predicate WP3 must embed in the same statement/transaction as a
/// control transition. Bind `tenant_id`, opaque `token`, and `epoch` as
/// `?1..=?3`. It rechecks expiry plus the complete gate/config/status identity
/// using SQLite's clock; a separate preflight read is never authoritative.
pub const LIVE_TRANSITION_GUARD_SQL: &str = "EXISTS (SELECT 1 \
    FROM byok_transition_fence f \
    JOIN byok_tenant_gate g ON g.tenant_id = f.tenant_id \
    JOIN tenant t ON t.tenant_id = f.tenant_id \
    LEFT JOIN tenant_byok_config c ON c.tenant_id = f.tenant_id \
    WHERE f.tenant_id = ?1 AND f.token = ?2 AND f.epoch = ?3 \
      AND f.outcome = 'active' \
      AND f.expires_at_ms > (CAST(strftime('%s', 'now') AS INTEGER) * 1000) \
      AND f.observed_gate_epoch = g.gate_epoch \
      AND f.observed_generation = g.current_generation \
      AND ((f.observed_config_version IS NULL AND c.config_version IS NULL) \
           OR f.observed_config_version = c.config_version) \
      AND f.observed_config_state = COALESCE(c.state, 'absent') \
      AND f.observed_byok_status = t.byok_status)";

/// Small query seam implemented by the native D1-over-HTTP client.
#[async_trait]
pub trait ByokFenceD1Client: Send + Sync + fmt::Debug {
    /// Execute one parameterized statement.
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String>;
}

#[async_trait]
impl ByokFenceD1Client for D1HttpClient {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
        D1HttpClient::query(self, sql, params).await
    }
}

/// BYOK configuration state observed atomically when a lease is acquired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigState {
    /// The tenant has no BYOK config yet; plaintext work is still fenced so an
    /// activation cannot race it.
    Absent,
    /// No BYOK material is in use.
    Inactive,
    /// Onboarding is not complete.
    Pending,
    /// BYOK is active for all data.
    Active,
    /// BYOK is active for new data during a backfill.
    Partial,
    /// Key material was destroyed. This state is terminal and always denied.
    Shredded,
}

impl ConfigState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Inactive => "inactive",
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Partial => "partial",
            Self::Shredded => "shredded",
        }
    }

    fn parse(value: &str) -> Result<Self, FenceError> {
        match value {
            "absent" => Ok(Self::Absent),
            "inactive" => Ok(Self::Inactive),
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "partial" => Ok(Self::Partial),
            "shredded" => Ok(Self::Shredded),
            other => Err(FenceError::MalformedRow(format!(
                "unknown tenant_byok_config.state={other:?}"
            ))),
        }
    }
}

/// Tenant kill-switch state observed with the BYOK config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByokStatus {
    /// Data operations are permitted.
    Active,
    /// Reads and writes are blocked while KMS health is degraded.
    DegradedReadOnly,
    /// Permanently revoked; no future acquisition is permitted.
    Revoked,
}

impl ByokStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::DegradedReadOnly => "degraded_read_only",
            Self::Revoked => "revoked",
        }
    }

    fn parse(value: &str) -> Result<Self, FenceError> {
        match value {
            "active" => Ok(Self::Active),
            "degraded_read_only" => Ok(Self::DegradedReadOnly),
            "revoked" => Ok(Self::Revoked),
            other => Err(FenceError::MalformedRow(format!(
                "unknown tenant.byok_status={other:?}"
            ))),
        }
    }
}

/// Exact optimistic snapshot pinned into a transition fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSnapshot {
    /// Persistent per-tenant gate generation.
    pub gate_epoch: i64,
    /// Published encryption generation visible to readers.
    pub current_generation: i64,
    /// Monotonic value incremented by every successful transition.
    pub config_version: Option<i64>,
    /// BYOK state-machine state.
    pub config_state: ConfigState,
    /// Tenant kill-switch state.
    pub byok_status: ByokStatus,
}

/// Kind of CAS/AC work protected by a data intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataOperation {
    /// CAS or AC read.
    Read,
    /// CAS or AC write.
    Write,
    /// CAS or AC delete.
    Delete,
}

impl DataOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
        }
    }
}

/// Cryptographically random capability. Its value is intentionally opaque and
/// redacted from `Debug`; releases compare it exactly in D1.
#[derive(Clone, PartialEq, Eq)]
struct LeaseToken(String);

impl LeaseToken {
    fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(hex::encode(bytes))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LeaseToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Live capability held by one CAS/AC operation.
#[derive(Debug)]
pub struct DataIntent {
    tenant_id: String,
    token: LeaseToken,
    /// Operation protected by this intent.
    pub operation: DataOperation,
    /// Snapshot observed by the atomic acquisition.
    pub snapshot: ConfigSnapshot,
    /// D1-authoritative expiry in Unix epoch milliseconds.
    pub expires_at_ms: i64,
}

impl DataIntent {
    /// Tenant this capability is scoped to.
    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Exact opaque token for an atomic, token-guarded integration statement.
    /// Never log or persist this value outside the D1 lease row.
    #[must_use]
    pub fn token(&self) -> &str {
        self.token.as_str()
    }
}

/// Exclusive capability held by one control-plane transition.
#[derive(Debug)]
pub struct TransitionFence {
    tenant_id: String,
    token: LeaseToken,
    epoch: i64,
    /// Snapshot which the transition is authorized to change.
    pub snapshot: ConfigSnapshot,
    /// D1-authoritative expiry in Unix epoch milliseconds.
    pub expires_at_ms: i64,
}

impl TransitionFence {
    /// Tenant this capability is scoped to.
    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Exact opaque token which must guard the transition UPDATE.
    /// Never log or persist this value outside the D1 lease row.
    #[must_use]
    pub fn token(&self) -> &str {
        self.token.as_str()
    }

    /// Monotonic per-tenant transition epoch.
    #[must_use]
    pub fn epoch(&self) -> i64 {
        self.epoch
    }

    /// Bind values matching [`LIVE_TRANSITION_GUARD_SQL`].
    pub fn guard_values(&self) -> [Value; 3] {
        [
            json!(self.tenant_id.as_str()),
            json!(self.token.as_str()),
            json!(self.epoch),
        ]
    }
}

impl ConfigSnapshot {
    /// Next gate epoch for a guarded control-plane commit.
    pub fn next_gate_epoch(&self) -> Result<i64, FenceError> {
        checked_next_counter("gate_epoch", self.gate_epoch)
    }

    /// Next published encryption generation for activation/rotation.
    pub fn next_generation(&self) -> Result<i64, FenceError> {
        checked_next_counter("current_generation", self.current_generation)
    }

    /// Next config version. An absent config starts at version one.
    pub fn next_config_version(&self) -> Result<i64, FenceError> {
        match self.config_version {
            Some(version) => checked_next_counter("config_version", version),
            None => Ok(1),
        }
    }
}

/// Fencing failures are fail-closed; callers must not proceed on any variant.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FenceError {
    /// Tenant/config missing, terminal, unhealthy, changed, or currently fenced.
    #[error("BYOK fence acquisition denied")]
    AcquireDenied,
    /// The caller attempted an unbounded, zero, or overflowing lease.
    #[error("invalid BYOK lease: {0}")]
    InvalidLease(String),
    /// The release capability no longer names a row.
    #[error("stale BYOK lease token")]
    StaleToken,
    /// The exact capability existed but had already expired.
    #[error("expired BYOK lease token")]
    Expired,
    /// D1 returned a row that cannot be trusted.
    #[error("malformed BYOK fence row: {0}")]
    MalformedRow(String),
    /// D1 transport or execution failed.
    #[error("BYOK fence D1 failure: {0}")]
    Backend(String),
    /// A monotonic counter reached its last safe SQLite integer value.
    #[error("BYOK counter exhausted: {0}")]
    CounterExhausted(&'static str),
}

/// D1 implementation of the tenant-wide BYOK fencing protocol.
#[derive(Debug)]
pub struct D1ByokFence<C = D1HttpClient> {
    client: Arc<C>,
}

impl<C: ByokFenceD1Client> D1ByokFence<C> {
    /// Construct over the process-wide D1 client.
    #[must_use]
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }

    /// Read the exact snapshot a transition will later compare atomically.
    pub async fn load_snapshot(&self, tenant_id: &str) -> Result<ConfigSnapshot, FenceError> {
        let rows = self
            .client
            .query(LOAD_SNAPSHOT_SQL, &[json!(tenant_id)])
            .await
            .map_err(FenceError::Backend)?;
        let row = rows.first().ok_or(FenceError::AcquireDenied)?;
        snapshot_from_row(row)
    }

    /// Acquire a data intent. Missing/malformed rows, non-active tenant status,
    /// and a live transition fence all return [`FenceError::AcquireDenied`].
    /// Config is deliberately optional: plaintext work before activation must
    /// participate in the same gate as encrypted work.
    pub async fn acquire_data_intent(
        &self,
        tenant_id: &str,
        operation: DataOperation,
        lease: Duration,
    ) -> Result<DataIntent, FenceError> {
        let lease_ms = checked_lease_ms(lease)?;
        let token = LeaseToken::generate();
        let rows = self
            .client
            .query(
                ACQUIRE_DATA_INTENT_SQL,
                &[
                    json!(token.as_str()),
                    json!(operation.as_str()),
                    json!(lease_ms),
                    json!(tenant_id),
                ],
            )
            .await
            .map_err(FenceError::Backend)?;
        let row = rows.first().ok_or(FenceError::AcquireDenied)?;
        Ok(DataIntent {
            tenant_id: tenant_id.to_owned(),
            token,
            operation,
            snapshot: snapshot_from_row(row)?,
            expires_at_ms: required_i64(row, "expires_at_ms")?,
        })
    }

    /// Release exactly the supplied data capability. Its persistent outcome is
    /// set to `completed` (or `expired`) and replay can never count as success.
    pub async fn release_data_intent(&self, intent: &DataIntent) -> Result<(), FenceError> {
        self.release(RELEASE_DATA_INTENT_SQL, &intent.tenant_id, &intent.token)
            .await
    }

    /// Extend a still-live data intent using D1's clock. A watchdog may call
    /// this while external I/O is pending; failure requires immediate
    /// cancellation of that I/O. Expired intents cannot be resurrected.
    pub async fn renew_data_intent(
        &self,
        intent: &mut DataIntent,
        lease: Duration,
    ) -> Result<(), FenceError> {
        let lease_ms = checked_lease_ms(lease)?;
        let rows = self
            .client
            .query(
                RENEW_DATA_INTENT_SQL,
                &[
                    json!(intent.tenant_id.as_str()),
                    json!(intent.token.as_str()),
                    json!(lease_ms),
                ],
            )
            .await
            .map_err(FenceError::Backend)?;
        let row = rows.first().ok_or(FenceError::Expired)?;
        intent.expires_at_ms = required_i64(row, "expires_at_ms")?;
        Ok(())
    }

    /// Acquire exclusive transition ownership against an exact prior snapshot.
    /// An expired fence may be atomically replaced; a live fence or live intent
    /// denies acquisition. `shredded` and `revoked` are terminal.
    pub async fn acquire_transition_fence(
        &self,
        tenant_id: &str,
        expected: &ConfigSnapshot,
        lease: Duration,
    ) -> Result<TransitionFence, FenceError> {
        let lease_ms = checked_lease_ms(lease)?;
        let token = LeaseToken::generate();
        let rows = self
            .client
            .query(
                ACQUIRE_TRANSITION_FENCE_SQL,
                &[
                    json!(token.as_str()),
                    json!(lease_ms),
                    json!(tenant_id),
                    json!(expected.gate_epoch),
                    json!(expected.current_generation),
                    json!(expected.config_version),
                    json!(expected.config_state.as_str()),
                    json!(expected.byok_status.as_str()),
                ],
            )
            .await
            .map_err(FenceError::Backend)?;
        let row = rows.first().ok_or(FenceError::AcquireDenied)?;
        Ok(TransitionFence {
            tenant_id: tenant_id.to_owned(),
            token,
            epoch: required_i64(row, "epoch")?,
            snapshot: snapshot_from_row(row)?,
            expires_at_ms: required_i64(row, "expires_at_ms")?,
        })
    }

    /// Explicitly abort a transition without changing config/gate state.
    /// Successful commits are reserved for WP3's atomic guarded batch and must
    /// persist `outcome='committed'` in that same transaction.
    pub async fn abort_transition_fence(&self, fence: &TransitionFence) -> Result<(), FenceError> {
        self.release(ABORT_TRANSITION_FENCE_SQL, &fence.tenant_id, &fence.token)
            .await
    }

    async fn release(
        &self,
        sql: &str,
        tenant_id: &str,
        token: &LeaseToken,
    ) -> Result<(), FenceError> {
        let rows = self
            .client
            .query(sql, &[json!(tenant_id), json!(token.as_str())])
            .await
            .map_err(FenceError::Backend)?;
        let row = rows.first().ok_or(FenceError::StaleToken)?;
        if required_i64(row, "was_live")? != 1 {
            return Err(FenceError::Expired);
        }
        Ok(())
    }
}

/// Increment a persisted SQLite counter without allowing integer-to-REAL
/// promotion. Exhaustion is a permanent fail-closed condition.
pub fn checked_next_counter(name: &'static str, current: i64) -> Result<i64, FenceError> {
    if !(0..MAX_BYOK_COUNTER).contains(&current) {
        return Err(FenceError::CounterExhausted(name));
    }
    Ok(current + 1)
}

fn checked_lease_ms(lease: Duration) -> Result<i64, FenceError> {
    if lease < Duration::from_secs(1) || lease > MAX_BYOK_FENCE_LEASE {
        return Err(FenceError::InvalidLease(format!(
            "duration must be in 1000ms..={}ms",
            MAX_BYOK_FENCE_LEASE.as_millis()
        )));
    }
    i64::try_from(lease.as_millis())
        .map_err(|_| FenceError::InvalidLease("duration overflow".to_owned()))
}

fn snapshot_from_row(row: &D1Row) -> Result<ConfigSnapshot, FenceError> {
    let gate_epoch =
        required_i64(row, "gate_epoch").or_else(|_| required_i64(row, "observed_gate_epoch"))?;
    if !(1..=MAX_BYOK_COUNTER).contains(&gate_epoch) {
        return Err(FenceError::MalformedRow(
            "gate_epoch must be positive".to_owned(),
        ));
    }
    let current_generation = required_i64(row, "current_generation")
        .or_else(|_| required_i64(row, "observed_generation"))?;
    if !(0..=MAX_BYOK_COUNTER).contains(&current_generation) {
        return Err(FenceError::MalformedRow(
            "current_generation must not be negative".to_owned(),
        ));
    }
    let config_version = optional_i64(row, "config_version")
        .or_else(|| optional_i64(row, "observed_config_version"));
    if matches!(config_version, Some(version) if !(1..=MAX_BYOK_COUNTER).contains(&version)) {
        return Err(FenceError::MalformedRow(
            "config_version must be positive when present".to_owned(),
        ));
    }
    let config_state = required_str(row, "config_state")
        .or_else(|_| required_str(row, "observed_config_state"))?;
    let byok_status =
        required_str(row, "byok_status").or_else(|_| required_str(row, "observed_byok_status"))?;
    Ok(ConfigSnapshot {
        gate_epoch,
        current_generation,
        config_version,
        config_state: ConfigState::parse(config_state)?,
        byok_status: ByokStatus::parse(byok_status)?,
    })
}

fn optional_i64(row: &D1Row, column: &str) -> Option<i64> {
    row.get(column).and_then(Value::as_i64)
}

fn required_i64(row: &D1Row, column: &str) -> Result<i64, FenceError> {
    row.get(column)
        .and_then(Value::as_i64)
        .ok_or_else(|| FenceError::MalformedRow(format!("missing integer {column}")))
}

fn required_str<'a>(row: &'a D1Row, column: &str) -> Result<&'a str, FenceError> {
    row.get(column)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| FenceError::MalformedRow(format!("missing text {column}")))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
#[path = "byok_transition_fence_tests.rs"]
mod tests;

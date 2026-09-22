// In-memory [`ByteStore`] for tests + route-integration fixtures.
use std::collections::HashMap;
use std::sync::Mutex;

use super::*;

/// One in-memory storage row: the running counter + an optional cap
/// (`quota == 0` ⇒ uncapped, mirroring the D1 `bytes_quota = 0` default).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Row {
    /// Running `bytes_used` counter.
    pub used: i64,
    /// Cap (`0` ⇒ uncapped).
    pub quota: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Liability {
    pub bytes_reserved: i64,
    pub state: MutationLiabilityState,
    pub intent_id: Option<Uuid>,
}

/// Hermetic in-memory [`ByteStore`] — models the D1 `(tenant, region)` row
/// with an in-process `Mutex` so the read-modify-write is atomic exactly as
/// the D1 `bytes_used = bytes_used + ?` increment is.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct InMemoryByteStore {
    rows: Mutex<HashMap<(String, String), Row>>,
    liabilities: Mutex<HashMap<(String, String, String), Liability>>,
}

impl InMemoryByteStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: Mutex::new(HashMap::new()),
            liabilities: Mutex::new(HashMap::new()),
        }
    }

    /// Seed a `(tenant, region)` row (test helper).
    pub fn seed(&self, tenant: &str, region: &str, row: Row) {
        if let Ok(mut rows) = self.rows.lock() {
            let _ = rows.insert((tenant.to_owned(), region.to_owned()), row);
        }
    }

    /// Read a row's `bytes_used` (test assertion helper); `0` when absent.
    #[must_use]
    pub fn used(&self, tenant: &str, region: &str) -> i64 {
        self.rows
            .lock()
            .ok()
            .and_then(|r| {
                r.get(&(tenant.to_owned(), region.to_owned()))
                    .map(|row| row.used)
            })
            .unwrap_or(0)
    }

    /// Read a row's `bytes_quota` (test assertion helper); `0` when absent.
    #[must_use]
    pub fn quota(&self, tenant: &str, region: &str) -> i64 {
        self.rows
            .lock()
            .ok()
            .and_then(|r| {
                r.get(&(tenant.to_owned(), region.to_owned()))
                    .map(|row| row.quota)
            })
            .unwrap_or(0)
    }

    pub fn liability(&self, tenant: &str, region: &str, logical_key: &str) -> Option<Liability> {
        self.liabilities
            .lock()
            .ok()
            .and_then(|rows| {
                rows.get(&(
                    tenant.to_owned(),
                    region.to_owned(),
                    logical_key.to_owned(),
                ))
                .cloned()
            })
    }
}

#[async_trait]
impl ByteStore for InMemoryByteStore {
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        quota_seed: Option<i64>,
        _now_ms: i64,
    ) -> Result<AccrueOutcome, String> {
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "InMemoryByteStore: poisoned lock".to_owned())?;
        let key = (tenant_id.to_owned(), region.to_owned());
        match rows.get_mut(&key) {
            // ── Existing row: accrue against the AUTHORITATIVE cap ────────
            // (mirrors the D1 UPSERT-conflict branch incl. rt-nuclear #16).
            // A finite incoming `quota_seed` (`Some(n)`, `n > 0`) RECONCILES
            // the stored cap (tier downgrade takes effect) and gates this
            // write by the new cap; an unlimited / absent incoming cap does
            // NOT clobber the stored cap and gates by the stored value.
            Some(row) => {
                // Reseed the stored cap only when the incoming cap is finite.
                // brutal-audit H3: this reconcile is DELIBERATELY decoupled from
                // the accrual outcome below — it runs BEFORE the over-cap check,
                // so a downgrade carrier that is itself OverCap STILL lowers the
                // stored cap (mirroring the D1 path's separate refused-path
                // reconcile UPDATE). Do NOT move it after the OverCap return, or
                // the deadlock (adapter writes gating on a stale higher cap)
                // re-opens and this fake stops faithfully modelling D1.
                if let Some(seed) = quota_seed {
                    if seed != 0 {
                        row.quota = seed;
                    }
                }
                // Over-cap predicate, faithful to the D1 UPSERT DO UPDATE
                // `WHERE ?5 = 0 OR bytes_used + ?3 <= ?5` and the None-path
                // `WHERE bytes_quota = 0 OR bytes_used + ?3 <= bytes_quota`:
                //   - `Some(0)` — genuine unlimited INCOMING cap ⇒ ALWAYS
                //     passes, bypassing any finite stored cap;
                //   - `Some(n>0)` — gate by the incoming finite cap `n`;
                //   - `None` — gate by the (possibly just-reconciled) stored
                //     cap (`0` stored = genuine unlimited).
                let over_cap = match quota_seed {
                    Some(0) => false,
                    Some(seed) => row.used.saturating_add(bytes) > seed,
                    None => row.quota != 0 && row.used.saturating_add(bytes) > row.quota,
                };
                if over_cap {
                    return Ok(AccrueOutcome::OverCap);
                }
                row.used = row.used.saturating_add(bytes);
                Ok(AccrueOutcome::Accrued)
            }
            // ── Fresh row: seed from `quota_seed` (mirrors the D1 INSERT) ─
            None => match quota_seed {
                // No resolved cap → never seed uncapped; fail CLOSED.
                None => Ok(AccrueOutcome::Indeterminate),
                // Finite cap whose very first write already exceeds it →
                // refuse, no row created (mirrors the D1 INSERT guard).
                Some(seed) if seed != 0 && bytes > seed => Ok(AccrueOutcome::OverCap),
                // Seed the fresh row with the real cap (`0` = genuine
                // unlimited) and apply the first write.
                Some(seed) => {
                    rows.insert(
                        key,
                        Row {
                            used: bytes,
                            quota: seed,
                        },
                    );
                    Ok(AccrueOutcome::Accrued)
                }
            },
        }
    }

    async fn release(
        &self,
        tenant_id: &str,
        region: &str,
        bytes: i64,
        _now_ms: i64,
    ) -> Result<(), String> {
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "InMemoryByteStore: poisoned lock".to_owned())?;
        if let Some(row) = rows.get_mut(&(tenant_id.to_owned(), region.to_owned())) {
            row.used = (row.used - bytes).max(0);
        }
        Ok(())
    }

    async fn record_mutation_liability(
        &self,
        liability: MutationLiability<'_>,
    ) -> Result<(), String> {
        let mut rows = self.liabilities.lock().map_err(|_| "liability lock poisoned".to_owned())?;
        let key = (
            liability.tenant_id.to_owned(),
            liability.region.to_owned(),
            liability.logical_key.to_owned(),
        );
        if let Some(row) = rows.get_mut(&key) {
            row.bytes_reserved = liability.bytes_reserved;
            row.state = liability.state;
            row.intent_id = liability.intent_id;
        } else {
            rows.insert(key, Liability {
                bytes_reserved: liability.bytes_reserved,
                state: liability.state,
                intent_id: liability.intent_id,
            });
        }
        Ok(())
    }

    async fn settle_mutation_liability(
        &self,
        tenant_id: &str,
        region: &str,
        logical_key: &str,
        resolution: MutationLiabilityResolution,
        _now_ms: i64,
    ) -> Result<MutationLiabilitySettlement, String> {
        // Hold ownership while applying the counter transition, so a concurrent
        // repeat cannot observe the liability active and refund it twice.
        let mut liabilities = self
            .liabilities
            .lock()
            .map_err(|_| "liability lock poisoned".to_owned())?;
        let key = (tenant_id.to_owned(), region.to_owned(), logical_key.to_owned());
        let Some(liability) = liabilities.get_mut(&key) else {
            return Ok(MutationLiabilitySettlement::AlreadySettled);
        };
        if matches!(liability.state, MutationLiabilityState::Released | MutationLiabilityState::Committed) {
            return Ok(MutationLiabilitySettlement::AlreadySettled);
        }
        match resolution {
            MutationLiabilityResolution::Committed => {
                liability.state = MutationLiabilityState::Committed;
                liability.intent_id = None;
            }
            MutationLiabilityResolution::NotWritten => {
                let mut rows = self
                    .rows
                    .lock()
                    .map_err(|_| "InMemoryByteStore: poisoned lock".to_owned())?;
                if let Some(row) = rows.get_mut(&(tenant_id.to_owned(), region.to_owned())) {
                    row.used = (row.used - liability.bytes_reserved).max(0);
                }
                liability.state = MutationLiabilityState::Released;
                liability.intent_id = None;
            }
        }
        Ok(MutationLiabilitySettlement::Applied)
    }
}

/// A [`ByteStore`] that always errors — drives the fail-CLOSED 503 path.
#[derive(Debug)]
#[non_exhaustive]
pub struct ErroringByteStore;

#[async_trait]
impl ByteStore for ErroringByteStore {
    async fn check_and_accrue(
        &self,
        _tenant_id: &str,
        _region: &str,
        _bytes: i64,
        _quota_seed: Option<i64>,
        _now_ms: i64,
    ) -> Result<AccrueOutcome, String> {
        Err("simulated D1 transport error".to_owned())
    }
    async fn release(
        &self,
        _tenant_id: &str,
        _region: &str,
        _bytes: i64,
        _now_ms: i64,
    ) -> Result<(), String> {
        Err("simulated D1 transport error".to_owned())
    }
}

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();

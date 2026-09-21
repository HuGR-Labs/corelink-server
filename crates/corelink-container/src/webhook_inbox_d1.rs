//! Durable Stripe webhook inbox adapter over the D1 HTTP client.
//!
//! This module is intentionally not wired into the dispatcher yet. It freezes
//! the persistence operations needed by the later dispatcher/materializer wave.

use std::sync::Arc;

use corelink_billing::stripe::real::webhook_dispatch::{
    DurableWebhookEvent, DurableWebhookInbox, InboxClaim as DispatcherInboxClaim,
    InboxReceiveOutcome, InboxTerminalState as DispatcherInboxTerminalState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::storage::d1_http::{D1BatchStatement, D1HttpClient, D1Row};

/// Insert an authenticated event without replacing an existing body.
pub const SQL_RECEIVE: &str = "INSERT INTO stripe_webhook_event_inbox (event_id, event_type, raw_body_hex, payload_sha256, stripe_created_at_ms, state, received_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, 'received', ?6, ?6) ON CONFLICT(event_id) DO NOTHING RETURNING state";
/// Read a stored event's identity and ownership state.
pub const SQL_READ: &str =
    "SELECT event_type, payload_sha256, state FROM stripe_webhook_event_inbox WHERE event_id = ?1";
/// Claim a received event or reclaim an expired claim, advancing its fence.
pub const SQL_CLAIM: &str = "UPDATE stripe_webhook_event_inbox SET state = 'claimed', fence = fence + 1, claim_owner = ?1, claim_expires_at_ms = ?2, updated_at_ms = ?3 WHERE event_id = ?4 AND (state = 'received' OR (state = 'claimed' AND claim_expires_at_ms <= ?3)) RETURNING event_id, event_type, raw_body_hex, payload_sha256, fence";
/// Complete only the claim which still owns the current fence.
pub const SQL_COMPLETE: &str = "UPDATE stripe_webhook_event_inbox SET state = 'completed', claim_owner = NULL, claim_expires_at_ms = NULL, updated_at_ms = ?1, terminal_at_ms = ?1, last_error = NULL WHERE event_id = ?2 AND state = 'claimed' AND fence = ?3 AND claim_owner = ?4 AND claim_expires_at_ms > ?5 RETURNING event_id";
/// Quarantine only the claim which still owns the current fence.
pub const SQL_QUARANTINE: &str = "UPDATE stripe_webhook_event_inbox SET state = 'quarantined', claim_owner = NULL, claim_expires_at_ms = NULL, updated_at_ms = ?1, terminal_at_ms = ?1, last_error = ?2 WHERE event_id = ?3 AND state = 'claimed' AND fence = ?4 AND claim_owner = ?5 AND claim_expires_at_ms > ?6 RETURNING event_id";
/// Insert the idempotent effect witness only while this exact lease is live.
pub const SQL_EFFECT_INSERT: &str = "INSERT INTO stripe_webhook_event_effects (event_id, effect_key, payload_sha256, fence, effect_kind, applied_at_ms) SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE EXISTS (SELECT 1 FROM stripe_webhook_event_inbox WHERE event_id = ?1 AND state = 'claimed' AND fence = ?4 AND claim_owner = ?7 AND claim_expires_at_ms > ?6) ON CONFLICT(event_id) DO NOTHING RETURNING event_id";
/// Terminalize only when the witness just inserted by the same fenced claim exists.
pub const SQL_EFFECT_COMPLETE: &str = "UPDATE stripe_webhook_event_inbox SET state = 'completed', claim_owner = NULL, claim_expires_at_ms = NULL, updated_at_ms = ?1, terminal_at_ms = ?1, last_error = NULL WHERE event_id = ?2 AND state = 'claimed' AND fence = ?3 AND claim_owner = ?4 AND claim_expires_at_ms > ?1 AND EXISTS (SELECT 1 FROM stripe_webhook_event_effects WHERE event_id = ?2 AND effect_key = ?5 AND payload_sha256 = ?6 AND fence = ?3 AND effect_kind = ?7) RETURNING event_id";

/// Authenticated event bytes and identity persisted before processing.
#[derive(Clone, Debug)]
pub struct AuthenticatedWebhookEvent {
    /// Stripe event id.
    pub event_id: String,
    /// Canonical Stripe event type.
    pub event_type: String,
    /// Exact signed body, hex encoded for D1 text storage.
    pub raw_body_hex: String,
    /// SHA-256 digest of the exact raw body, lower hexadecimal.
    pub payload_sha256: String,
    /// Stripe's event creation timestamp, if the envelope supplied one.
    pub stripe_created_at_ms: Option<u64>,
}

/// Result of making an authenticated delivery durable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxReceipt {
    /// A fresh, recoverable event awaits a claim.
    Received,
    /// A completed or quarantined event may be acknowledged as terminal.
    Terminal,
    /// A historical marker requires manual reconciliation.
    LegacyAmbiguous,
}

/// Lease-backed ownership of a durable inbox event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxClaim {
    /// Stripe event id.
    pub event_id: String,
    /// Monotonically increasing ownership fence.
    pub fence: u64,
}

/// Terminal state selected after a durable outcome is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxTerminalState {
    /// The materialized effect completed durably.
    Completed,
    /// A durable DLQ record exists for the event.
    Quarantined,
}

/// D1-backed inbox persistence. Later wiring must make effect plus completion
/// one D1 transaction, and must call quarantine only after its DLQ write.
#[derive(Clone)]
pub struct D1WebhookInbox {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1WebhookInbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1WebhookInbox").finish_non_exhaustive()
    }
}

impl D1WebhookInbox {
    /// Construct the durable inbox over a shared D1 client.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    fn run(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        let d1 = Arc::clone(&self.d1);
        let sql = sql.to_owned();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.query(&sql, &binds).await })
        })
    }

    fn run_batch(&self, statements: Vec<D1BatchStatement>) -> Result<Vec<Vec<D1Row>>, String> {
        let d1 = Arc::clone(&self.d1);
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.batch(statements).await })
        })
        .map_err(|error| format!("webhook inbox effect batch: {}", error.message))
    }

    /// Persist an authenticated event. A changed body or type for an existing
    /// event id is rejected so it can never be acknowledged as a duplicate.
    pub fn receive(
        &self,
        event: &AuthenticatedWebhookEvent,
        now_ms: u64,
    ) -> Result<InboxReceipt, String> {
        validate_authenticated_event(event)?;
        let rows = self.run(
            SQL_RECEIVE,
            vec![
                json!(event.event_id),
                json!(event.event_type),
                json!(event.raw_body_hex),
                json!(event.payload_sha256),
                json!(event
                    .stripe_created_at_ms
                    .map(|v| i64::try_from(v).unwrap_or(i64::MAX))),
                json!(i64::try_from(now_ms).unwrap_or(i64::MAX)),
            ],
        )?;
        if !rows.is_empty() {
            return Ok(InboxReceipt::Received);
        }
        let row = self
            .run(SQL_READ, vec![json!(event.event_id)])?
            .into_iter()
            .next()
            .ok_or_else(|| "webhook inbox: existing event disappeared".to_owned())?;
        let state = text(&row, "state")?;
        if state == "legacy_ambiguous" {
            return Ok(InboxReceipt::LegacyAmbiguous);
        }
        if text(&row, "event_type")? != event.event_type
            || text(&row, "payload_sha256")? != event.payload_sha256
        {
            return Err("webhook inbox: event id conflicts with authenticated body".to_owned());
        }
        match state.as_str() {
            "completed" | "quarantined" => Ok(InboxReceipt::Terminal),
            "received" | "claimed" => Ok(InboxReceipt::Received),
            _ => Err("webhook inbox: invalid stored state".to_owned()),
        }
    }

    /// Atomically claim or reclaim an expired event. `None` means terminal,
    /// legacy, or another owner's unexpired claim.
    pub fn claim(
        &self,
        event_id: &str,
        owner: &str,
        now_ms: u64,
        lease_ms: u64,
    ) -> Result<Option<InboxClaim>, String> {
        let expires = now_ms
            .checked_add(lease_ms)
            .ok_or_else(|| "webhook inbox: lease overflow".to_owned())?;
        let rows = self.run(
            SQL_CLAIM,
            vec![
                json!(owner),
                json!(i64::try_from(expires).unwrap_or(i64::MAX)),
                json!(i64::try_from(now_ms).unwrap_or(i64::MAX)),
                json!(event_id),
            ],
        )?;
        match rows.into_iter().next() {
            None => Ok(None),
            Some(row) => Ok(Some(InboxClaim {
                event_id: text(&row, "event_id")?,
                fence: number(&row, "fence")?,
            })),
        }
    }

    /// Fenced terminal transition. `false` means ownership was lost or expired.
    pub fn finish(
        &self,
        claim: &InboxClaim,
        owner: &str,
        state: InboxTerminalState,
        error: Option<&str>,
        now_ms: u64,
    ) -> Result<bool, String> {
        let now = i64::try_from(now_ms).unwrap_or(i64::MAX);
        let rows = match state {
            InboxTerminalState::Completed => self.run(
                SQL_COMPLETE,
                vec![
                    json!(now),
                    json!(claim.event_id),
                    json!(i64::try_from(claim.fence).unwrap_or(i64::MAX)),
                    json!(owner),
                    json!(now),
                ],
            )?,
            InboxTerminalState::Quarantined => self.run(
                SQL_QUARANTINE,
                vec![
                    json!(now),
                    json!(error.unwrap_or("quarantined")),
                    json!(claim.event_id),
                    json!(i64::try_from(claim.fence).unwrap_or(i64::MAX)),
                    json!(owner),
                    json!(now),
                ],
            )?,
        };
        Ok(!rows.is_empty())
    }

    /// Persist the effect witness and terminal inbox state in one D1 batch.
    /// A response lost after this call is safe: the next delivery observes the
    /// terminal state and is the only duplicate path that returns 200.
    pub fn commit_effect(
        &self,
        claim: &InboxClaim,
        owner: &str,
        event: &DurableWebhookEvent,
        effect_key: &str,
        effect_kind: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        validate_durable_event(event)?;
        if claim.event_id != event.event_id {
            return Err("webhook inbox: claim and authenticated event differ".to_owned());
        }
        if effect_key.is_empty() || effect_kind.is_empty() || owner.is_empty() {
            return Err("webhook inbox: effect identity is empty".to_owned());
        }
        let now = i64::try_from(now_ms).unwrap_or(i64::MAX);
        let fence = i64::try_from(claim.fence).unwrap_or(i64::MAX);
        let result = self.run_batch(vec![
            D1BatchStatement::new(
                SQL_EFFECT_INSERT,
                vec![
                    json!(claim.event_id),
                    json!(effect_key),
                    json!(event.payload_sha256),
                    json!(fence),
                    json!(effect_kind),
                    json!(now),
                    json!(owner),
                ],
            ),
            D1BatchStatement::new(
                SQL_EFFECT_COMPLETE,
                vec![
                    json!(now),
                    json!(claim.event_id),
                    json!(fence),
                    json!(owner),
                    json!(effect_key),
                    json!(event.payload_sha256),
                    json!(effect_kind),
                ],
            ),
        ])?;
        let inserted = result.first().is_some_and(|rows| !rows.is_empty());
        let completed = result.get(1).is_some_and(|rows| !rows.is_empty());
        Ok(inserted && completed)
    }
}

impl DurableWebhookInbox for D1WebhookInbox {
    fn receive(
        &self,
        event: &DurableWebhookEvent,
        now_ms: u64,
    ) -> Result<InboxReceiveOutcome, String> {
        match self.receive(
            &AuthenticatedWebhookEvent {
                event_id: event.event_id.clone(),
                event_type: event.event_type.clone(),
                raw_body_hex: event.raw_body_hex.clone(),
                payload_sha256: event.payload_sha256.clone(),
                stripe_created_at_ms: Some(event.stripe_created_at_ms),
            },
            now_ms,
        )? {
            InboxReceipt::Received => Ok(InboxReceiveOutcome::Received),
            InboxReceipt::Terminal => Ok(InboxReceiveOutcome::Terminal),
            InboxReceipt::LegacyAmbiguous => Ok(InboxReceiveOutcome::LegacyAmbiguous),
        }
    }

    fn claim(
        &self,
        event_id: &str,
        owner: &str,
        now_ms: u64,
        lease_ms: u64,
    ) -> Result<Option<DispatcherInboxClaim>, String> {
        self.claim(event_id, owner, now_ms, lease_ms)
            .map(|claim| claim.map(|claim| DispatcherInboxClaim::new(claim.event_id, claim.fence)))
    }

    fn finish(
        &self,
        claim: &DispatcherInboxClaim,
        owner: &str,
        state: DispatcherInboxTerminalState,
        error: Option<&str>,
        now_ms: u64,
    ) -> Result<bool, String> {
        let state = match state {
            DispatcherInboxTerminalState::Completed => InboxTerminalState::Completed,
            DispatcherInboxTerminalState::Quarantined => InboxTerminalState::Quarantined,
        };
        self.finish(
            &InboxClaim {
                event_id: claim.event_id.clone(),
                fence: claim.fence,
            },
            owner,
            state,
            error,
            now_ms,
        )
    }

    fn commit_effect(
        &self,
        claim: &DispatcherInboxClaim,
        owner: &str,
        event: &DurableWebhookEvent,
        effect_key: &str,
        effect_kind: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        self.commit_effect(
            &InboxClaim {
                event_id: claim.event_id.clone(),
                fence: claim.fence,
            },
            owner,
            event,
            effect_key,
            effect_kind,
            now_ms,
        )
    }
}

fn text(row: &D1Row, name: &str) -> Result<String, String> {
    row.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("webhook inbox: row missing `{name}`"))
}

fn number(row: &D1Row, name: &str) -> Result<u64, String> {
    row.get(name)
        .and_then(Value::as_i64)
        .and_then(|v| u64::try_from(v).ok())
        .ok_or_else(|| format!("webhook inbox: row missing `{name}`"))
}

fn validate_authenticated_event(event: &AuthenticatedWebhookEvent) -> Result<(), String> {
    if event.event_id.is_empty()
        || event.event_type.is_empty()
        || event.payload_sha256.len() != 64
        || !event
            .payload_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || event.raw_body_hex.len() > 200_000
    {
        return Err("webhook inbox: invalid authenticated event identity".to_owned());
    }
    let raw_body = hex::decode(&event.raw_body_hex)
        .map_err(|_| "webhook inbox: raw body is not hex".to_owned())?;
    if hex::encode(Sha256::digest(raw_body)) != event.payload_sha256 {
        return Err("webhook inbox: raw body digest mismatch".to_owned());
    }
    Ok(())
}

fn validate_durable_event(event: &DurableWebhookEvent) -> Result<(), String> {
    validate_authenticated_event(&AuthenticatedWebhookEvent {
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        raw_body_hex: event.raw_body_hex.clone(),
        payload_sha256: event.payload_sha256.clone(),
        stripe_created_at_ms: Some(event.stripe_created_at_ms),
    })
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests use direct SQLite assertions"
)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn migration_preserves_legacy_marker_as_manual_work() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0044_stripe_webhook_events_processed.sql"
        ))
        .unwrap();
        db.execute("INSERT INTO stripe_webhook_events_processed VALUES ('evt_legacy','invoice.paid',1,'dispatched','evt_legacy')", []).unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0131_stripe_webhook_inbox.sql"
        ))
        .unwrap();
        let state: String = db
            .query_row(
                "SELECT state FROM stripe_webhook_event_inbox WHERE event_id='evt_legacy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "legacy_ambiguous");
    }

    #[test]
    fn fenced_sql_reclaims_only_expired_claims() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0044_stripe_webhook_events_processed.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0131_stripe_webhook_inbox.sql"
        ))
        .unwrap();
        let _: String = db
            .query_row(
                SQL_RECEIVE,
                rusqlite::params!["evt_1", "invoice.paid", "aa", "a".repeat(64), 1_i64, 1_i64],
                |r| r.get(0),
            )
            .unwrap();
        let first_fence: i64 = db
            .query_row(
                SQL_CLAIM,
                rusqlite::params!["one", 10_i64, 1_i64, "evt_1"],
                |r| r.get(4),
            )
            .unwrap();
        assert_eq!(first_fence, 1);
        let expired_finish = db.query_row(
            SQL_COMPLETE,
            rusqlite::params![10_i64, "evt_1", 1_i64, "one", 10_i64],
            |r| r.get::<_, String>(0),
        );
        assert!(matches!(
            expired_finish,
            Err(rusqlite::Error::QueryReturnedNoRows)
        ));
        let blocked = db.query_row(
            SQL_CLAIM,
            rusqlite::params!["two", 20_i64, 9_i64, "evt_1"],
            |r| r.get::<_, String>(0),
        );
        assert!(matches!(blocked, Err(rusqlite::Error::QueryReturnedNoRows)));
        let reclaimed_fence: i64 = db
            .query_row(
                SQL_CLAIM,
                rusqlite::params!["two", 21_i64, 10_i64, "evt_1"],
                |r| r.get(4),
            )
            .unwrap();
        assert_eq!(reclaimed_fence, 2);
    }

    #[test]
    fn effect_witness_and_completion_share_one_fenced_transaction() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0044_stripe_webhook_events_processed.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0131_stripe_webhook_inbox.sql"
        ))
        .unwrap();
        db.execute_batch(include_str!(
            "../../../migrations/d1/0132_stripe_webhook_effect_ledger.sql"
        ))
        .unwrap();
        let raw = "00";
        let digest = hex::encode(Sha256::digest(hex::decode(raw).unwrap()));
        db.query_row(
            SQL_RECEIVE,
            rusqlite::params!["evt_effect", "invoice.paid", raw, digest, 1_i64, 1_i64],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
        let fence: i64 = db
            .query_row(
                SQL_CLAIM,
                rusqlite::params!["owner-a", 100_i64, 1_i64, "evt_effect"],
                |row| row.get(4),
            )
            .unwrap();

        let tx = db.transaction().unwrap();
        tx.query_row(
            SQL_EFFECT_INSERT,
            rusqlite::params![
                "evt_effect",
                "stripe-webhook-effect:key",
                digest,
                fence,
                "invoice.paid",
                2_i64,
                "owner-a",
            ],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
        tx.query_row(
            SQL_EFFECT_COMPLETE,
            rusqlite::params![
                2_i64,
                "evt_effect",
                fence,
                "owner-a",
                "stripe-webhook-effect:key",
                digest,
                "invoice.paid"
            ],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
        tx.commit().unwrap();

        let effects: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM stripe_webhook_event_effects WHERE event_id='evt_effect'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let state: String = db
            .query_row(
                "SELECT state FROM stripe_webhook_event_inbox WHERE event_id='evt_effect'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(effects, 1, "lost ACK retry cannot create another effect");
        assert_eq!(state, "completed", "only a committed effect can ack 200");

        let stale = db.query_row(
            SQL_EFFECT_INSERT,
            rusqlite::params![
                "evt_effect",
                "stripe-webhook-effect:key",
                digest,
                fence,
                "invoice.paid",
                3_i64,
                "owner-a",
            ],
            |row| row.get::<_, String>(0),
        );
        assert!(matches!(stale, Err(rusqlite::Error::QueryReturnedNoRows)));
    }

    #[test]
    fn receive_rejects_raw_body_digest_mismatch() {
        let event = AuthenticatedWebhookEvent {
            event_id: "evt_1".to_owned(),
            event_type: "invoice.paid".to_owned(),
            raw_body_hex: "00".to_owned(),
            payload_sha256: "0".repeat(64),
            stripe_created_at_ms: None,
        };
        assert!(validate_authenticated_event(&event).is_err());
    }
}

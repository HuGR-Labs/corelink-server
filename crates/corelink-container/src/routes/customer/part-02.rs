/// Deterministic, name-based (v5-shaped) UUID from a subject key — byte-for-byte
/// the `deterministicDsrId` algorithm in `clerk.ts` (`SHA-256("corelink-dsr-v1:"
/// + key)`, first 16 bytes, version 5 + RFC-4122 variant). Keying on the Clerk
/// user id gives the SAME `dsr_id` as the webhook path, so a dashboard-initiated
/// delete and a Clerk `user.deleted` for the same account are idempotency-compatible.
#[must_use]
pub(crate) fn deterministic_dsr_id(subject_key: &str) -> String {
    let digest = Sha256::digest(format!("corelink-dsr-v1:{subject_key}").as_bytes());
    let mut b = [0u8; 16];
    // First 16 bytes of the SHA-256 digest (the digest is 32 bytes — never short).
    b.copy_from_slice(digest.get(..16).unwrap_or(&[0u8; 16]));
    b[6] = (b[6] & 0x0f) | 0x50; // version 5 (name-based)
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
                                 // Render via the uuid crate (lowercase, hyphenated 8-4-4-4-12) — no manual
                                 // slicing; the version/variant bits set above survive verbatim.
    uuid::Uuid::from_bytes(b).to_string()
}

/// Production [`AccountDeletionRequester`] over the [`crate::customer_d1::CustomerD1`]
/// row-source seam + an injected [`DsrErasureSink`]. Mirrors the Clerk
/// `user.deleted` path: resolve the account's Clerk id, derive the deterministic
/// `dsr_id`, honor an operator legal hold, build the canonical `dsr.queued.v1`
/// message, `INSERT OR IGNORE` the `dsr_requested` anchor (G4), then enqueue.
#[non_exhaustive]
pub struct D1AccountDeletionRequester {
    /// D1 row source (production: `customer_d1::D1HttpCustomerDb`).
    db: Arc<dyn crate::customer_d1::CustomerD1>,
    /// Erasure-message transport (wired in `routes.rs`).
    sink: Arc<dyn DsrErasureSink>,
}

impl core::fmt::Debug for D1AccountDeletionRequester {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AccountDeletionRequester")
            .field("db", &"[CustomerD1]")
            .field("sink", &self.sink)
            .finish()
    }
}

impl D1AccountDeletionRequester {
    /// Wire the requester over a D1 row source + an erasure sink.
    #[must_use]
    pub fn new(db: Arc<dyn crate::customer_d1::CustomerD1>, sink: Arc<dyn DsrErasureSink>) -> Self {
        Self { db, sink }
    }

    /// Real wall-clock unix-ms (the SLA anchor — the route's logical clock is 0).
    fn now_ms() -> u64 {
        u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
        )
        .unwrap_or(0)
    }

    /// `HMAC-SHA256(ERASURE_SALT_KEY, dsr_id)` hex — the per-DSR erasure salt
    /// (GDPR Art. 4(5) unlinkable pseudonymization), mirroring `clerk.ts`
    /// `deriveErasureSalt`. Fail-CLOSED: the key is REQUIRED here (the requester
    /// is only wired in configured/prod envs); an absent/empty key is an
    /// operator fault that must surface as 500, never a predictable salt.
    fn derive_salt_hex(dsr_id: &str) -> Result<String, AccountDeletionError> {
        let key = std::env::var("ERASURE_SALT_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                AccountDeletionError::Internal(
                    "ERASURE_SALT_KEY unset — refusing to derive a predictable erasure salt \
                     (fail-CLOSED)"
                        .to_owned(),
                )
            })?;
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes()).map_err(|e| {
            AccountDeletionError::Internal(format!("erasure salt key invalid: {e}"))
        })?;
        mac.update(dsr_id.as_bytes());
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    /// Is `tenant_id` under an operator legal hold? Mirrors the webhook's
    /// `tenantUnderLegalHold` posture against the FROZEN C-LEGALHOLD schema
    /// (migration 0076: a row's presence == held). A query error (table not yet
    /// provisioned) → `false`: returning `true` on error would make EVERY
    /// deletion a no-op preservation and silently break the live erasure
    /// obligation (a far larger harm than the not-yet-built hold feature).
    fn under_legal_hold(&self, tenant_id: &str) -> bool {
        match self.db.query(
            "SELECT 1 AS held FROM tenant_legal_hold WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        ) {
            Ok(rows) => !rows.is_empty(),
            Err(_) => false,
        }
    }
}

impl AccountDeletionRequester for D1AccountDeletionRequester {
    fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError> {
        // Resolve the account row + its Clerk id (subject key for the deterministic
        // dsr_id). No row → nothing to erase (already deleted) → NotFound.
        let rows = self
            .db
            .query(
                "SELECT clerk_user_id FROM tenant WHERE tenant_id = ?1 LIMIT 1",
                vec![json!(tenant_id)],
            )
            .map_err(|e| AccountDeletionError::Internal(format!("tenant lookup failed: {e}")))?;
        let Some(row) = rows.into_iter().next() else {
            return Err(AccountDeletionError::NotFound);
        };
        // Prefer the Clerk user id (dsr_id parity with the webhook path); fall back
        // to the tenant id when absent (still deterministic + idempotent).
        let subject_key = row
            .get("clerk_user_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(tenant_id);
        let dsr_id = deterministic_dsr_id(subject_key);
        let clerk_user_id = row
            .get("clerk_user_id")
            .and_then(Value::as_str)
            .map(str::to_owned);

        let salt_hex = Self::derive_salt_hex(&dsr_id)?;
        let legal_hold = self.under_legal_hold(tenant_id);
        let now_ms = Self::now_ms();

        // Canonical dsr.queued.v1 envelope (mirrors buildErasureQueueMessage):
        // subject_id == tenant_id (1 Clerk user : 1 tenant — the tenant is the
        // deletion unit); source distinguishes the self-serve trigger.
        let message = json!({
            "schema": DSR_QUEUED_SCHEMA,
            "dsr_id": dsr_id,
            "tenant_id": tenant_id,
            "subject_id": tenant_id,
            "erasure_salt_hex": salt_hex,
            "queued_at_ms": now_ms,
            "legal_hold": legal_hold,
            "source": "customer.account.delete",
            "clerk_user_id": clerk_user_id,
        });

        // G4 (WI-S11-008): write the durable "DSR requested" anchor BEFORE enqueue
        // so the 24h verify sweep can detect an SLA breach even if the erasure
        // fails before any backend tombstone lands. Idempotent: dsr_id is
        // deterministic, so a repeat request is an INSERT-OR-IGNORE no-op. This
        // row is ALSO the legitimacy gate the /_internal/dsr/erase consumer checks
        // (dsr_requested must exist for (dsr_id, tenant)) — writing it first makes
        // the subsequent enqueue authorized by construction.
        self.db
            .query(
                "INSERT OR IGNORE INTO dsr_requested (dsr_id, tenant_id, requested_at, status) \
                 VALUES (?1, ?2, ?3, 'requested')",
                vec![
                    json!(dsr_id),
                    json!(tenant_id),
                    json!(i64::try_from(now_ms).unwrap_or(i64::MAX)),
                ],
            )
            .map_err(|e| {
                AccountDeletionError::Internal(format!("dsr_requested anchor write failed: {e}"))
            })?;

        // Enqueue the erasure message (transport injected by routes.rs).
        self.sink
            .enqueue(&message)
            .map_err(|e| AccountDeletionError::Internal(format!("erasure enqueue failed: {e}")))?;
        Ok(())
    }
}

// ─── Error mapping ────────────────────────────────────────────────────────────

/// Map a [`CustomerHandlerError`] to the canonical HTTP response.
fn map_err(e: CustomerHandlerError) -> axum::response::Response {
    tracing::warn!(error = ?e, "customer handler error");
    match e {
        CustomerHandlerError::InvalidRequest(_) => {
            (StatusCode::BAD_REQUEST, "invalid request").into_response()
        }
        CustomerHandlerError::Unauthorized(_) => {
            (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
        }
        CustomerHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        CustomerHandlerError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
        CustomerHandlerError::AuditFailed(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        // HONEST v1 (dashboard revival WP-3): an endpoint the concrete
        // handler does not implement yet is an explicit 501 — never
        // fabricated data, never a misleading 404/500. Team invites are
        // now fully implemented (D1 create/list/remove + signup-worker
        // accept, ADR-S33-001 / migration 0074), so this generic arm is a
        // defensive fallback for any future NotImplemented surface, NOT a
        // team-invite stub. The message stays generic + honest accordingly.
        CustomerHandlerError::NotImplemented(_) => (
            StatusCode::NOT_IMPLEMENTED,
            "this endpoint is not yet implemented",
        )
            .into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

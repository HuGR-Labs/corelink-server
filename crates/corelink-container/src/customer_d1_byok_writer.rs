// ─── BYOK config WRITER (activation seam — control-plane authority) ───────────
//
// The activation WRITE path over migration 0081, closing the H5 gap: the read
// model above + the r2_s3 CAS engagement gate are inert until SOMETHING flips a
// tenant's `tenant_byok_config.state` to `active` AND persists its CMK-wrapped
// Tcs in `tenant_byok_secret`. This writer is that authority. It is operator-
// gated at the route boundary (`routes::byok_admin`, mirrors `/v1/admin/*`);
// nothing tenant-reachable calls it.
//
// SECURITY / fail-CLOSED discipline:
//   * The plaintext Tcs is NEVER handled here — only the CMK-WRAPPED ciphertext
//     (`tcs_wrapped`), mirroring 0081's INV-BYOK-CRYPTO-SOVEREIGNTY note. No
//     key material is ever logged (the audit event records tenant + provider +
//     CMK *identity* + state, never secret bytes).
//   * Parameterised SQL only; every statement is tenant-scoped by PK
//     (INV-TENANT-ISOLATION).
//   * Write ORDER is Tcs-secret FIRST, then flip config→active — so the read
//     path never observes `state=='active'` pointing at a missing Tcs (the
//     r2_s3 resolve fails CLOSED on that combination; the ordering keeps the
//     activation atomic-enough that a crash between the two writes leaves the
//     tenant NON-active, i.e. plaintext, never a half-active fail-closed brick).
//   * Invalid activation parameters are rejected BEFORE any D1 write; the
//     monotonic state machine refuses to re-activate a crypto-shredded tenant.

/// CMK providers accepted for a BYOK activation. A subset of migration 0081's
/// `cmk_provider` CHECK — `corelink_managed` is excluded because activation
/// implies a customer-held CMK (the whole point of BYOK).
const BYOK_ACTIVATION_PROVIDERS: [&str; 4] = ["aws", "gcp", "azure", "vault"];

/// A failure writing the BYOK activation / deactivation state. Fail-CLOSED:
/// the caller MUST surface any variant as an error — a partial or unvalidated
/// custody record is never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ByokWriteError {
    /// Caller-supplied activation parameters are invalid (rejected BEFORE any
    /// D1 write — never persist a half-valid custody record).
    Invalid(String),
    /// The requested transition is not permitted by the monotonic state
    /// machine (e.g. re-activating a crypto-shredded tenant, or deactivating a
    /// tenant that was never active).
    IllegalTransition {
        /// Current persisted state.
        from: ByokState,
        /// Requested target state.
        to: ByokState,
    },
    /// D1 transport / write failure.
    Transport(String),
}

impl core::fmt::Display for ByokWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "byok activation invalid: {e}"),
            Self::IllegalTransition { from, to } => write!(
                f,
                "byok illegal state transition: {} → {}",
                from.as_str(),
                to.as_str()
            ),
            Self::Transport(e) => write!(f, "byok write transport error: {e}"),
        }
    }
}

impl std::error::Error for ByokWriteError {}

/// The activation parameters that flip a tenant to BYOK `active` with its CMK
/// identity + CMK-wrapped Tenant Convergence Secret.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ByokActivation {
    /// Tenant id (PK of both `tenant_byok_config` and `tenant_byok_secret`).
    pub tenant_id: String,
    /// Key-custody rung — MUST be `byok` or `hyok` (not `managed`).
    pub mode: ByokMode,
    /// Convergent (Mode A) vs random (Mode B).
    pub crypto_mode: ByokCryptoMode,
    /// CMK provider — one of [`BYOK_ACTIVATION_PROVIDERS`].
    pub cmk_provider: String,
    /// CMK identity: ARN / GCP resource name / Azure URI / Vault path. Public
    /// key IDENTITY, not secret material.
    pub cmk_key_id: String,
    /// CMK region (bound for the latency SLO; optional).
    pub cmk_region: Option<String>,
    /// CMK-WRAPPED Tenant Convergence Secret ciphertext. The plaintext Tcs is
    /// NEVER carried here — only the provider-opaque wrapped bytes.
    pub tcs_wrapped: Vec<u8>,
}

impl ByokActivation {
    /// Validate fail-CLOSED. Rejects `managed` custody, unknown providers,
    /// empty CMK identity, and an empty wrapped-Tcs BEFORE any D1 write.
    pub(crate) fn validate_for_control(&self) -> Result<(), ByokWriteError> {
        if self.tenant_id.trim().is_empty() {
            return Err(ByokWriteError::Invalid("tenant_id is empty".to_owned()));
        }
        if !matches!(self.mode, ByokMode::Byok | ByokMode::Hyok) {
            return Err(ByokWriteError::Invalid(format!(
                "mode must be byok or hyok for an activation, got {}",
                self.mode.as_str()
            )));
        }
        if !BYOK_ACTIVATION_PROVIDERS.contains(&self.cmk_provider.as_str()) {
            return Err(ByokWriteError::Invalid(format!(
                "cmk_provider must be one of {BYOK_ACTIVATION_PROVIDERS:?}, got {:?}",
                self.cmk_provider
            )));
        }
        if self.cmk_key_id.trim().is_empty() {
            return Err(ByokWriteError::Invalid("cmk_key_id is empty".to_owned()));
        }
        if self.tcs_wrapped.is_empty() {
            return Err(ByokWriteError::Invalid(
                "tcs_wrapped is empty (refusing to activate without a wrapped Tcs)".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Control-plane WRITER for the per-tenant BYOK configuration (migration 0081).
///
/// The counterpart of [`D1ByokConfigReader`] — the sole authority that flips a
/// tenant to BYOK `active` (engaging the r2_s3 encryption gate) and the
/// crypto-shred kill switch that flips it back off. Generic over the same
/// [`ByokConfigRows`] async seam so unit tests drive it hermetically.
#[derive(Debug)]
#[non_exhaustive]
pub struct D1ByokConfigWriter<R = D1HttpClient> {
    /// Async row source (production: [`D1HttpClient`]). `query_rows` carries
    /// both reads and writes (a write returns an empty result set).
    #[cfg_attr(not(test), allow(dead_code, reason = "legacy writer is test-only; production uses D1ByokControl"))]
    rows: Arc<R>,
}

impl<R: ByokConfigRows> D1ByokConfigWriter<R> {
    /// Wire the writer over an async row source.
    #[must_use]
    pub fn new(rows: Arc<R>) -> Self {
        Self { rows }
    }

    /// Read the tenant's current `state` (fail-CLOSED on an unparseable value).
    /// `Ok(None)` ⇒ no config row yet.
    #[cfg(test)]
    async fn current_state(&self, tenant_id: &str) -> Result<Option<ByokState>, ByokWriteError> {
        let rows = self
            .rows
            .query_rows(
                "SELECT state FROM tenant_byok_config WHERE tenant_id = ?1 LIMIT 1",
                vec![json!(tenant_id)],
            )
            .await
            .map_err(ByokWriteError::Transport)?;
        match rows.first() {
            None => Ok(None),
            Some(row) => {
                let s = col_opt_str(row, "state")
                    .ok_or_else(|| {
                        ByokWriteError::Transport("tenant_byok_config.state missing".to_owned())
                    })?
                    .parse::<ByokState>()
                    .map_err(|e| ByokWriteError::Transport(e.to_string()))?;
                Ok(Some(s))
            }
        }
    }

    /// Flip a tenant to BYOK `active` with its CMK identity + wrapped Tcs.
    ///
    /// Idempotent + safe to call on an already-active tenant (a re-activation
    /// re-wraps the Tcs and bumps `tcs_version`). Refuses to re-activate a
    /// crypto-shredded tenant (the monotonic terminal state).
    ///
    /// # Errors
    ///
    /// - [`ByokWriteError::Invalid`] when the parameters fail validation.
    /// - [`ByokWriteError::IllegalTransition`] when the tenant is `shredded`.
    /// - [`ByokWriteError::Transport`] on any D1 write failure.
    #[cfg(test)]
    pub async fn activate(&self, act: &ByokActivation, now_ms: i64) -> Result<(), ByokWriteError> {
        act.validate_for_control()?;
        // Monotonic guard: crypto-shred is terminal — a shredded tenant's
        // ciphertext is unrecoverable, so re-activation would be a lie.
        if let Some(ByokState::Shredded) = self.current_state(&act.tenant_id).await? {
            return Err(ByokWriteError::IllegalTransition {
                from: ByokState::Shredded,
                to: ByokState::Active,
            });
        }

        // BLOB over D1-HTTP: the wrapped Tcs is written as a base64 STRING; the
        // read seam (`storage::byok_cas::decode_blob`) accepts both a base64
        // string and a byte-array, so this round-trips to the exact bytes.
        let tcs_b64 = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(&act.tcs_wrapped)
        };

        // 1) Persist the wrapped Tcs FIRST (so an active config never dangles
        //    over a missing secret). UPSERT: a re-activation bumps tcs_version.
        self.rows
            .query_rows(
                "INSERT INTO tenant_byok_secret \
                   (tenant_id, tcs_wrapped, cmk_key_id, tcs_version, wrapped_at_ms) \
                 VALUES (?1, ?2, ?3, 1, ?4) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                   tcs_wrapped   = excluded.tcs_wrapped, \
                   cmk_key_id    = excluded.cmk_key_id, \
                   tcs_version   = tenant_byok_secret.tcs_version + 1, \
                   wrapped_at_ms = excluded.wrapped_at_ms",
                vec![
                    json!(act.tenant_id),
                    json!(tcs_b64),
                    json!(act.cmk_key_id),
                    json!(now_ms),
                ],
            )
            .await
            .map_err(ByokWriteError::Transport)?;

        // 2) Flip the config row to `active`. UPSERT preserves created_at_ms on
        //    conflict (only set on first insert).
        self.rows
            .query_rows(
                "INSERT INTO tenant_byok_config \
                   (tenant_id, mode, crypto_mode, cmk_provider, cmk_key_id, \
                    cmk_region, state, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                   mode          = excluded.mode, \
                   crypto_mode   = excluded.crypto_mode, \
                   cmk_provider  = excluded.cmk_provider, \
                   cmk_key_id    = excluded.cmk_key_id, \
                   cmk_region    = excluded.cmk_region, \
                   state         = 'active', \
                   updated_at_ms = excluded.updated_at_ms",
                vec![
                    json!(act.tenant_id),
                    json!(act.mode.as_str()),
                    json!(act.crypto_mode.as_str()),
                    json!(act.cmk_provider),
                    json!(act.cmk_key_id),
                    json!(act.cmk_region),
                    json!(now_ms),
                ],
            )
            .await
            .map_err(ByokWriteError::Transport)?;

        // Audit: identity + provider + state only — NEVER key/secret material.
        tracing::info!(
            target: "corelink.byok.activation.audit",
            audit = true,
            op = "activate",
            tenant = %act.tenant_id,
            provider = %act.cmk_provider,
            cmk_key_id = %act.cmk_key_id,
            crypto_mode = act.crypto_mode.as_str(),
            state = "active",
            "BYOK activation: tenant flipped to active"
        );
        Ok(())
    }

    /// Crypto-shred KILL SWITCH — flip an active/partial tenant to `shredded`.
    ///
    /// The deliberate control-plane complement of the always-on CMK-revocation
    /// detector (`corelink_byok::revocation`): an operator (or a downstream
    /// erasure flow) can hard-stop BYOK for a tenant. Monotonic + idempotent:
    /// `shredded` is terminal (a second call is a no-op `Ok`); a tenant that
    /// was never active has nothing to shred and is rejected fail-CLOSED.
    ///
    /// # Errors
    ///
    /// - [`ByokWriteError::IllegalTransition`] when the tenant is not
    ///   active/partial/shredded (i.e. no active BYOK config to kill).
    /// - [`ByokWriteError::Transport`] on any D1 read/write failure.
    #[cfg(test)]
    pub async fn deactivate(&self, tenant_id: &str, now_ms: i64) -> Result<(), ByokWriteError> {
        match self.current_state(tenant_id).await? {
            // Nothing active to kill — refuse rather than write a spurious
            // shredded record over a fresh/managed tenant.
            None | Some(ByokState::Inactive) | Some(ByokState::Pending) => {
                return Err(ByokWriteError::IllegalTransition {
                    from: ByokState::Inactive,
                    to: ByokState::Shredded,
                })
            }
            // Already shredded — idempotent success.
            Some(ByokState::Shredded) => return Ok(()),
            Some(ByokState::Active | ByokState::Partial) => {}
        }

        self.rows
            .query_rows(
                "UPDATE tenant_byok_config \
                 SET state = 'shredded', updated_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND state IN ('active', 'partial')",
                vec![json!(now_ms), json!(tenant_id)],
            )
            .await
            .map_err(ByokWriteError::Transport)?;

        tracing::warn!(
            target: "corelink.byok.activation.audit",
            audit = true,
            op = "deactivate",
            tenant = %tenant_id,
            state = "shredded",
            "BYOK kill switch: tenant crypto-shredded (state → shredded)"
        );
        Ok(())
    }
}

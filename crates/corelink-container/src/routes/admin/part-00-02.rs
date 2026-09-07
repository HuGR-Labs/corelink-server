/// Durable, D1-backed dual-approval ledger (migration 0091, finding H5).
///
/// Implements both halves of the ledger over the `admin_approvals` table:
/// [`ApprovalLedgerWriter::record_approval`] (the approve endpoint's "create"
/// step) and [`ApprovalLedger::verify_and_consume`] (the mutate handler's
/// verify + single-use consume). The consume is an atomic conditional
/// `UPDATE ... WHERE consumed = 0 RETURNING`, so a concurrent replay of the
/// same approval cannot double-spend.
///
/// Both trait methods are sync (the ledger is consulted from the sync admin
/// handler chain); each bridges to the async D1 client with the same
/// `block_in_place` pattern used by [`D1AdminHandler`].
pub struct D1ApprovalLedger {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl core::fmt::Debug for D1ApprovalLedger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1ApprovalLedger").finish_non_exhaustive()
    }
}

impl D1ApprovalLedger {
    /// Construct from the shared D1 client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }

    /// Bridge sync → async D1 query (see [`D1AdminHandler::block_on`]).
    fn block_on<F, T>(fut: F) -> T
    where
        F: core::future::Future<Output = T>,
    {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(fut))
    }
}

impl ApprovalLedger for D1ApprovalLedger {
    fn verify_and_consume(
        &self,
        approval_id: &str,
        initiator: &str,
        resource: &str,
    ) -> Result<VerifiedApproval, ApprovalRejection> {
        let now_ms = SystemWallClock.now_ms();
        let outcome = Self::block_on(self.client.admin_approval_verify_consume(
            approval_id,
            initiator,
            resource,
            i64::try_from(now_ms).unwrap_or(i64::MAX),
        ))
        .map_err(ApprovalRejection::Backend)?;
        match outcome {
            AdminApprovalConsume::Consumed { approver } => Ok(VerifiedApproval::new(approver)),
            AdminApprovalConsume::Unknown => Err(ApprovalRejection::Unknown),
            AdminApprovalConsume::ScopeMismatch => Err(ApprovalRejection::ScopeMismatch),
            AdminApprovalConsume::SelfApproval { approver } => {
                Err(ApprovalRejection::SelfApproval { approver })
            }
            AdminApprovalConsume::AlreadyConsumed => Err(ApprovalRejection::Consumed),
        }
    }
}

impl ApprovalLedgerWriter for D1ApprovalLedger {
    fn record_approval(
        &self,
        approval_id: &str,
        approver: &str,
        resource: &str,
    ) -> Result<(), String> {
        let now_ms = SystemWallClock.now_ms();
        Self::block_on(self.client.admin_approval_create(
            approval_id,
            approver,
            resource,
            i64::try_from(now_ms).unwrap_or(i64::MAX),
        ))
    }
}

/// Minimum accepted length (chars) for any internal-auth shared secret.
///
/// Shared 32-char floor used by EVERY internal-auth reader in the
/// container (admin, mint, erase, DSR, introspect) so all gates stay
/// consistent (F29 fix). The secrets-checklist instructs
/// `openssl rand -hex 32` (64 chars); anything shorter is rejected.
pub(crate) const INTERNAL_AUTH_KEY_MIN_LEN: usize = 32;

/// Per-consumer internal-auth key split (red-team #3).
///
/// A single shared `CORELINK_INTERNAL_AUTH_KEY` previously gated FIVE
/// high-privilege internal surfaces (any-tenant PAT mint, GDPR erase,
/// CAS erase, admin, pilots) — one leak granted ALL of them. This helper
/// reads a **consumer-specific** key first and only falls back to the
/// shared key when the specific one is truly absent, so each
/// surface can be rotated to its own credential without a flag day
/// (mirrors the #8 OCI dual-name pattern — additive, deployable BEFORE
/// the new prod secrets exist).
///
/// Resolution order (fail-CLOSED at each step):
/// 1. `specific_env` — used iff set AND ≥ [`INTERNAL_AUTH_KEY_MIN_LEN`];
/// 2. else `CORELINK_INTERNAL_AUTH_KEY` — used iff set AND ≥ floor;
/// 3. else `None` — the handler fails CLOSED (403), exactly as today.
///
/// A present-but-invalid `specific_env` (including empty, whitespace-only,
/// non-Unicode, or sub-floor values) hard-fails the surface. Treating a
/// malformed declaration as absent would silently widen authorization back to
/// the shared authority. Only a truly absent dedicated variable may use the
/// shared migration fallback.
#[must_use]
pub(crate) fn resolve_internal_auth_key(specific_env: &str) -> Option<Arc<str>> {
    // 1. A present dedicated key owns the decision, including rejection. Use
    // var_os so a non-Unicode value cannot masquerade as an absent variable.
    match std::env::var_os(specific_env) {
        None => {}
        Some(raw) => {
            let Ok(key) = raw.into_string() else {
                tracing::warn!(
                    env = specific_env,
                    "consumer-specific internal-auth key is not valid UTF-8; \
                     this surface will fail CLOSED (403)"
                );
                return None;
            };
            if key.len() < INTERNAL_AUTH_KEY_MIN_LEN || key.trim().is_empty() {
                tracing::warn!(
                    env = specific_env,
                    "consumer-specific internal-auth key is blank or < 32 chars; \
                     this surface will fail CLOSED (403) (use `openssl rand -hex 32`)"
                );
                return None;
            }
            return Some(Arc::from(key));
        }
    }
    // 2. Shared fallback key.
    let shared = std::env::var_os("CORELINK_INTERNAL_AUTH_KEY")?
        .into_string()
        .ok()?;
    if shared.len() < INTERNAL_AUTH_KEY_MIN_LEN || shared.trim().is_empty() {
        tracing::warn!(
            env = specific_env,
            "neither the consumer-specific key nor CORELINK_INTERNAL_AUTH_KEY \
             is a valid key ≥ 32 chars; this surface will fail CLOSED (403) \
             (use `openssl rand -hex 32`)"
        );
        return None;
    }
    Some(Arc::from(shared))
}

/// Read the operator-only **admin/pilots** shared secret from the
/// environment (red-team #3).
///
/// Reads `CORELINK_ADMIN_AUTH_KEY` first, falling back to the shared
/// `CORELINK_INTERNAL_AUTH_KEY` when unset/blank/too-short (see
/// [`resolve_internal_auth_key`]). When `None`, the admin handlers fail
/// CLOSED (403) — privileged logic never runs without a properly sized
/// gate.
#[must_use]
pub fn internal_auth_key_from_env() -> Option<Arc<str>> {
    resolve_internal_auth_key("CORELINK_ADMIN_AUTH_KEY")
}

/// Read the operator **dual-approver** dedicated secret from the environment
/// (finding H5; hardened by the 2026-08-19 red-team, finding "two-person
/// control collapses to one shared secret").
///
/// This gates `POST /v1/admin/approve` — the step that RECORDS a second
/// approver into the durable approval ledger. It is deliberately a **separate
/// credential** from [`internal_auth_key_from_env`] (the mutate/admin key): a
/// real two-person control requires the approver and the initiator to hold
/// DIFFERENT keys, so a single admin-key holder cannot both create the
/// approval and spend it.
///
/// Reads `CORELINK_ADMIN_APPROVER_AUTH_KEY` ONLY and does **NOT** fall back to
/// the shared `CORELINK_INTERNAL_AUTH_KEY` — same DEDICATED-only treatment as
/// the erase authority ([`erase_auth_key_from_env`], finding H4) and the DSR
/// anchor. The old shared-key fallback silently COLLAPSED two-person control:
/// with `CORELINK_ADMIN_APPROVER_AUTH_KEY` unset both this key and the mutate
/// key resolved to the SAME `CORELINK_INTERNAL_AUTH_KEY`, so a single
/// shared-secret holder could call `/v1/admin/approve` (records
/// `approver@internal`) then `/v1/admin/mutate` (initiator `operator@internal`)
/// and defeat dual approval — the only discriminator being two hardcoded,
/// cosmetic principal strings. Dedicated-only closes that: when unset or
/// < 32 chars the approve route fails CLOSED (403) and dual approval cannot be
/// recorded until a distinct approver key is provisioned.
///
/// # Owner action (REQUIRED in prod)
///
/// `CORELINK_ADMIN_APPROVER_AUTH_KEY` MUST be bound in prod (≥ 32 chars,
/// `openssl rand -hex 32`) and MUST be DISTINCT from `CORELINK_INTERNAL_AUTH_KEY`
/// / `CORELINK_ADMIN_AUTH_KEY`; [`crate::routes::approver_key_distinct_or_none`]
/// enforces the distinctness at boot (a byte-equal approver key is treated as
/// unset → approve fails CLOSED).
#[must_use]
pub fn approver_auth_key_from_env() -> Option<Arc<str>> {
    resolve_dedicated_auth_key("CORELINK_ADMIN_APPROVER_AUTH_KEY")
}

/// Read the **CAS-erase / DSR** dedicated secret from the environment
/// (finding H4 — was red-team #3).
///
/// Reads `CORELINK_ERASE_AUTH_KEY` ONLY and does **NOT** fall back to the
/// shared `CORELINK_INTERNAL_AUTH_KEY`: the erase authority drives
/// irreversible tombstones, so a leak of the broad shared secret must never,
/// by itself, exercise it. This keeps the anti-forge "eraser ≠ requester"
/// split against the anchor authority (`CORELINK_DSR_ANCHOR_AUTH_KEY`) that
/// the old shared fallback silently collapsed (finding H4). `None` ⇒ every
/// erase surface (CAS-erase, DSR, audit-drain) fails CLOSED (403/unmounted).
///
/// # Owner action (Track-2)
///
/// The dedicated `CORELINK_ERASE_AUTH_KEY` is now REQUIRED in prod (≥ 32
/// chars, `openssl rand -hex 32`); until it is bound the erase surfaces stay
/// fail-CLOSED. Mirrors the dedicated-key-only PAT-mint gate (`internal_pat`).
#[must_use]
pub fn erase_auth_key_from_env() -> Option<Arc<str>> {
    resolve_dedicated_auth_key("CORELINK_ERASE_AUTH_KEY")
}

/// Dual-key rotation variant of [`erase_auth_key_from_env`]: the accepted ERASE
/// keys = the current `CORELINK_ERASE_AUTH_KEY` PLUS, when set and ≥
/// [`INTERNAL_AUTH_KEY_MIN_LEN`], the outgoing `CORELINK_ERASE_AUTH_KEY_PREVIOUS`.
///
/// Both are DEDICATED-ONLY (no shared `CORELINK_INTERNAL_AUTH_KEY` fallback — H4)
/// and hold the ≥32-char floor (F28/F15). Accepting the previous value for the
/// duration of a rotation bridges the window where the apex already forwards the
/// NEW key but a not-yet-recycled DO container still booted with the OLD one (the
/// env-read-at-start footgun that took the whole erase path down). The operator
/// clears `CORELINK_ERASE_AUTH_KEY_PREVIOUS` once the fleet has recycled. Returns
/// an empty vec (⇒ erase surfaces fail CLOSED / stay unmounted) when neither is
/// usable. The current key is always FIRST; a duplicate previous is dropped.
#[must_use]
pub fn erase_auth_keys_from_env() -> Vec<String> {
    let mut keys: Vec<String> = Vec::with_capacity(2);
    if let Some(current) = erase_auth_key_from_env() {
        keys.push(current.to_string());
    }
    if let Ok(previous) = std::env::var("CORELINK_ERASE_AUTH_KEY_PREVIOUS") {
        if previous.len() >= INTERNAL_AUTH_KEY_MIN_LEN && !keys.contains(&previous) {
            keys.push(previous);
        }
    }
    keys
}

/// Build the axum `Router` exposing the admin read + mutate + approve routes.
pub fn router(state: AdminRouteState) -> Router {
    Router::new()
        .route(ADMIN_READ_ROUTE, get(handle_read))
        .route(ADMIN_MUTATE_ROUTE, post(handle_mutate))
        .route(ADMIN_APPROVE_ROUTE, post(handle_approve))
        .with_state(state)
}

/// JSON shape for `POST /v1/admin/mutate`.
///
/// The route is intentionally hand-coded against `serde_json` rather
/// than a `prost`-generated message because the admin plane is the
/// control plane: cardinality is small, evolution is cheap, and the
/// surface is human-curated.
///
/// # Auth authority
///
/// Since the PR #152 internal-auth gate, the authority for the admin
/// assertion comes entirely from the `x-corelink-internal-auth` gate,
/// NOT from the JSON body. The gated path (`into_request_gated`)
/// hardcodes `operator@internal` as the initiator principal and forces
/// `is_admin = true` regardless of the body fields. Operators are no
/// longer required to send `initiator` / `initiator_is_admin` — they
/// default to empty/false and are ignored for the auth decision.
#[derive(Clone, Debug, Deserialize)]
pub struct AdminMutateBody {
    /// Operation kind (`set_tenant_tier` / `rotate_admin_token`).
    pub op_kind: String,
    /// Tenant ID (required for `set_tenant_tier`).
    pub tenant: Option<String>,
    /// New tier (required for `set_tenant_tier`).
    pub tier: Option<String>,
    /// Token ID (required for `rotate_admin_token`).
    pub token_id: Option<String>,
    /// Initiator principal.
    ///
    /// **Ignored by the gated route path** (`into_request_gated`): since
    /// the PR #152 internal-auth gate the authority comes from clearing
    /// the `x-corelink-internal-auth` gate, which hardcodes the operator
    /// principal. This field is optional on the wire (`#[serde(default)]`)
    /// so callers are not required to send a dummy value; any supplied
    /// value is discarded.
    #[serde(default)]
    pub initiator: String,
    /// True if initiator carries the admin role.
    ///
    /// **Ignored by the gated route path** (`into_request_gated`): the
    /// internal-auth gate forces `is_admin = true` regardless of this
    /// field. Optional on the wire (`#[serde(default)]`); any supplied
    /// value is discarded.
    #[serde(default)]
    pub initiator_is_admin: bool,
    /// Approval id from the dual-approval ledger (None = rejected).
    pub approval_id: Option<String>,
    /// Second approver principal (None = rejected).
    pub approver: Option<String>,
}

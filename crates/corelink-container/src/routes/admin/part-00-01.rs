/// Real D1-backed admin handler (WP-S1 Phase 2).
///
/// Implements both [`AdminReadHandler`] and [`AdminMutateHandler`] by
/// translating each operation to a D1 HTTP query against the
/// `tier_selections` table (per `migrations/d1/0039_tier_selection.sql`).
///
/// # Delegation pattern
///
/// To keep the audit + SLI emit ordering contract identical to the
/// [`InMemoryAdminHandler`] (`MutateAttempted` BEFORE checks,
/// `MutateDualApprovalRejected` / `MutateDenied` BEFORE rejection,
/// `MutateCommitted` AFTER state change, `Sli::AvailControlPlane` on
/// every return path), this handler **delegates** to an inner
/// `InMemoryAdminHandler` for the validation + audit + SLI plumbing
/// and **mirrors** the resulting state to D1:
///
/// - On read: D1 lookup runs FIRST. Found rows are seeded into the
///   inner handler so the delegated `read` emits the canonical
///   `ReadAttempted` + `ReadServed` rows and returns the JSON body.
/// - On mutate: the delegated `mutate` enforces RBAC + dual-approval
///   and emits the canonical audit chain. On `Ok` it has applied the
///   state in-memory; we then mirror to D1 (`tenant_set_tier`). If D1
///   mirroring fails, the inner audit row stays `MutateCommitted` and
///   the caller receives `Internal(...)` — a tracked durability gap
///   addressed by the outbox flow (WP-S1 Phase 2 follow-up §3).
///
/// # Operation coverage
///
/// - Read `tenant:<id>` — implemented (production path).
/// - Mutate [`MutateOp::SetTenantTier`] — implemented (production path).
/// - Mutate [`MutateOp::RotateAdminToken`] — returns `Internal(...)`
///   indicating the multi-step PAT rotation flow is not yet wired
///   through the admin trait. Tracked as Phase 2 follow-up.
pub struct D1AdminHandler {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
    inner: Arc<InMemoryAdminHandler>,
}

impl core::fmt::Debug for D1AdminHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AdminHandler").finish_non_exhaustive()
    }
}

impl D1AdminHandler {
    /// Construct from collaborators.
    #[must_use]
    pub fn new(
        client: Arc<crate::storage::d1_http::D1HttpClient>,
        inner: Arc<InMemoryAdminHandler>,
    ) -> Self {
        Self { client, inner }
    }

    /// Bridge sync trait method → async D1 query via `block_in_place`.
    ///
    /// Mirrors the `cas.rs::build_handler` rationale: routes are
    /// driven from `#[tokio::main]`, so a bare
    /// `Handle::current().block_on(...)` panics with "Cannot start a
    /// runtime from within a runtime". `block_in_place` releases the
    /// current worker so the inner `block_on` is legal.
    fn block_on<F, T>(fut: F) -> T
    where
        F: core::future::Future<Output = T>,
    {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(fut))
    }
}

impl AdminReadHandler for D1AdminHandler {
    fn read(&self, req: AdminReadRequest) -> Result<AdminReadResponse, AdminHandlerError> {
        // Pre-flight: if this is a tenant resource and the caller is
        // admin, fetch from D1 and seed into the inner handler so the
        // delegated read emits the canonical ReadAttempted + ReadServed
        // chain and returns a real D1-sourced body.
        //
        // For non-admin callers we let the inner handler emit ReadDenied
        // BEFORE returning Forbidden (fail-CLOSED ordering pin).
        if req.is_admin {
            if let Some(tenant_id) = req.resource.strip_prefix("tenant:") {
                match Self::block_on(self.client.tenant_admin_lookup(tenant_id)) {
                    Ok(Some(record)) => {
                        let body = serde_json::to_vec(&record).map_err(|e| {
                            AdminHandlerError::Internal(format!("admin read json encode: {e}"))
                        })?;
                        self.inner.seed_read(req.resource.clone(), body)?;
                    }
                    Ok(None) => {
                        // Leave body unseeded — inner returns NotFound
                        // (after emitting ReadAttempted).
                    }
                    Err(e) => {
                        return Err(AdminHandlerError::Internal(format!("D1 admin read: {e}")));
                    }
                }
            }
        }
        self.inner.read(req)
    }
}

impl AdminMutateHandler for D1AdminHandler {
    fn mutate(&self, req: AdminMutateRequest) -> Result<AdminMutateResponse, AdminHandlerError> {
        // Pre-validate the D1-backed op surface BEFORE delegating, so
        // unsupported ops are rejected without polluting the audit log
        // with a MutateCommitted row.
        if let MutateOp::RotateAdminToken { .. } = req.op {
            return Err(AdminHandlerError::Internal(
                "rotate_admin_token not implemented in D1 backend \
                 (WP-S1 Phase 2 follow-up — multi-step PAT rotation)"
                    .to_owned(),
            ));
        }

        // Capture the op shape for the post-commit D1 mirror (the
        // inner handler consumes `req` by value).
        let mirror = match &req.op {
            MutateOp::SetTenantTier { tenant, tier } => Some((tenant.clone(), tier.clone())),
            _ => None,
        };

        // Delegate to InMemoryAdminHandler — it enforces RBAC + dual-
        // approval + emits MutateAttempted / MutateDenied /
        // MutateDualApprovalRejected / MutateCommitted in canonical
        // order and emits Sli::AvailControlPlane on every return path.
        let resp = self.inner.mutate(req)?;

        // Mirror the mutation to D1. The inner handler has already
        // emitted MutateCommitted; a D1 failure surfaces as
        // Internal(...) and is tracked by the outbox flow.
        if let Some((tenant, tier)) = mirror {
            match Self::block_on(self.client.tenant_set_tier(&tenant, &tier)) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(AdminHandlerError::NotFound {
                        what: format!("tenant:{tenant}"),
                    });
                }
                Err(e) => {
                    return Err(AdminHandlerError::Internal(format!(
                        "D1 admin mutate mirror: {e}"
                    )));
                }
            }
        }
        Ok(resp)
    }
}

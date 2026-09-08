impl CustomerBillingHandler for D1CustomerHandler {
    fn billing(&self, req: BillingRequest) -> Result<BillingResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::BillingDenied,
            &req.principal,
            "",
        )?;
        self.emit_audit(
            AuditEventKind::BillingAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        let tenant = self
            .tenant_row(&req.caller_tenant)?
            .ok_or_else(|| self.tenant_not_found(&req.caller_tenant))?;
        let billing = self.billing_row(&req.caller_tenant)?;
        // Only an ACTIVE subscription's tier is the customer's real plan; a
        // `pending_checkout` row (paid tier written at click time, before
        // payment) must NOT surface as the active plan. Mirrors the
        // enforcement filter in `worker/src/lib/quota.ts::getTierForTenant`.
        let tier_rows = self.run(
            "SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = 'active' LIMIT 1",
            vec![json!(req.caller_tenant)],
        )?;

        // Plan slug: tier_selections (canonical FSM) first, then the
        // tenant.tier fallback (0057's documented read order).
        let plan = tier_rows
            .first()
            .and_then(|r| col_opt_str(r, "tier"))
            .or_else(|| col_opt_str(&tenant, "tier"))
            .unwrap_or_else(|| "free".to_owned());
        let status = map_billing_status(
            billing
                .as_ref()
                .and_then(|b| col_opt_str(b, "status"))
                .as_deref(),
        );
        let current_period_end = billing
            .as_ref()
            .and_then(|b| col_opt_i64(b, "current_period_end_ms"))
            .map(ms_to_iso8601)
            .unwrap_or_default();

        let resp = BillingResponse::new(
            status,
            plan,
            // Period START is not materialized in tenant_billing: honest
            // empty string, never a guessed date.
            String::new(),
            current_period_end,
            // amount_due_cents is not materialized in D1: honest 0.
            0,
            "usd",
            // No invoice-history read surface yet: honest empty.
            Vec::<InvoiceRow>::new(),
        );

        self.emit_audit(
            AuditEventKind::BillingServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }

    fn portal_url(&self, req: PortalRequest) -> Result<PortalResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::BillingDenied,
            &req.principal,
            "portal",
        )?;
        // Portal is a billing sub-action (audit symmetry with InMemory).
        self.emit_audit(
            AuditEventKind::BillingAttempted,
            &req.caller_tenant,
            &req.principal,
            "portal",
        )?;

        let billing = self.billing_row(&req.caller_tenant)?;
        let Some(customer_id) = billing
            .as_ref()
            .and_then(|b| col_opt_str(b, "stripe_customer_id"))
            .filter(|c| !c.is_empty())
        else {
            // FROZEN: no Stripe customer id → 404 "no billing account".
            self.emit_sli(true);
            return Err(CustomerHandlerError::NotFound {
                what: format!("no billing account for tenant={}", req.caller_tenant),
            });
        };

        let Some(portal) = self.portal.as_ref() else {
            // Fail-CLOSED: Stripe unconfigured is an operator fault, not
            // a customer 404.
            self.emit_sli(true);
            return Err(CustomerHandlerError::Internal(
                "customer_d1: Stripe client not configured; portal unavailable".to_owned(),
            ));
        };
        // Idempotency key varies per request on purpose: portal sessions
        // are short-lived and a fresh one per click is the Stripe-
        // documented shape.
        let idem = format!("portal:{}:{}", req.caller_tenant, self.clock.now_ms());
        let url = portal
            .create(&customer_id, &self.portal_return_url, &idem)
            .map_err(|e| {
                self.emit_sli(true);
                CustomerHandlerError::Internal(format!("customer_d1: {e}"))
            })?;

        self.emit_audit(
            AuditEventKind::BillingServed,
            &req.caller_tenant,
            &req.principal,
            "portal",
        )?;
        self.emit_sli(false);
        Ok(PortalResponse::new(url))
    }
}

impl CustomerKeysHandler for D1CustomerHandler {
    fn list(&self, req: KeysListRequest) -> Result<KeysListResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::KeysDenied,
            &req.principal,
            "",
        )?;
        // Read-only, always self-tenant-scoped (same as InMemory: no
        // Attempted audit kind exists for list).
        let rows = self.run(
            // `name` + `revoked_at_ms` are the WP-2 columns (migration
            // 0063, landing in a parallel PR — this PR depends on it).
            "SELECT pat_id, name, scope, created_ms, revoked_at_ms, find_only \
             FROM pat WHERE tenant_id = ?1 ORDER BY created_ms DESC",
            vec![json!(req.caller_tenant)],
        )?;
        let pats: Vec<PatRow> = rows
            .iter()
            .map(|row| {
                // `find_only` (0093): NULL/0 = normal PAT; 1 = find-missing only.
                let find_only = col_opt_i64(row, "find_only").unwrap_or(0) == 1;
                PatRow::new(
                    col_opt_str(row, "pat_id").unwrap_or_default(),
                    // Pre-0063 rows have no name: honest empty string.
                    col_opt_str(row, "name").unwrap_or_default(),
                    scope_to_list(&col_opt_str(row, "scope").unwrap_or_default(), find_only),
                    col_opt_i64(row, "created_ms")
                        .map(ms_to_iso8601)
                        .unwrap_or_default(),
                    // last-used is not tracked yet: honest None.
                    None,
                    col_opt_i64(row, "revoked_at_ms").map(ms_to_iso8601),
                )
            })
            .collect();

        let tenant = self.tenant_row(&req.caller_tenant)?;
        let byok = self.byok_status(&req.caller_tenant, tenant.as_ref())?;

        self.emit_sli(false);
        Ok(KeysListResponse::new(pats, byok))
    }

    fn create(&self, req: KeyCreateRequest) -> Result<KeyCreateResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::KeysDenied,
            &req.principal,
            &req.name,
        )?;
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::KeyCreateAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.name,
        )?;

        let Some((signing_key, signing_key_id)) = self.signing.as_ref() else {
            self.emit_sli(true);
            return Err(CustomerHandlerError::Internal(
                "customer_d1: PAT signing key not configured; key creation unavailable".to_owned(),
            ));
        };

        // FROZEN scope map ('admin' NEVER grantable). `scope` is the CHECK-safe
        // D1 value ('read-only'/'read-write'); a FIND-ONLY request maps to
        // 'read-only' + the `find_only` marker (ADR-0071, migration 0093).
        let scope = map_requested_scopes(&req.scopes).inspect_err(|_| self.emit_sli(true))?;
        let find_only = mint_is_find_only(&req.scopes);
        // Bitset mirrors the effective grant (ADR-0071). Read is a superset of
        // find-missing, so read/read-write also carry the FIND bit; a find-only
        // token carries ONLY `SCOPE_CACHE_FIND` (no read/write). (The authoritative
        // enforcement input is the scope the Worker forwards as `x-corelink-scope`
        // — `find-missing` for a find-only PAT; these bits mirror it.)
        let scope_bits = if find_only {
            PatScopes::from_u64(SCOPE_CACHE_FIND)
        } else if scope == "read-write" {
            PatScopes::from_u64(SCOPE_CACHE_RW | SCOPE_CACHE_FIND)
        } else {
            PatScopes::from_u64(SCOPE_CACHE_R | SCOPE_CACHE_FIND)
        };
        // Human-readable scope for the audit summary (the stored `scope` is the
        // CHECK-safe base, so a find-only PAT would otherwise read "read-only").
        let scope_label = if find_only { "find-missing" } else { scope };

        let tenant_uuid = Uuid::parse_str(&req.caller_tenant).map_err(|_| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(format!(
                "customer_d1: tenant id is not a UUID: {}",
                req.caller_tenant
            ))
        })?;

        let (plaintext, pat) = mint(
            PatEnv::Pat,
            TenantId(tenant_uuid),
            // The dashboard principal is the token-prefix string (not a
            // UUID); the PAT's embedded principal is the owning tenant —
            // the same identity the first-PAT signup mint binds.
            PrincipalId(tenant_uuid),
            scope_bits,
            Some(SELF_SERVE_PAT_TTL),
            signing_key,
            *signing_key_id,
        )
        .map_err(|e| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(format!("customer_d1: PAT mint failed: {e}"))
        })?;

        let now_ms = self.clock.now_ms();
        let expires_ms: u64 = pat
            .expires_at
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

        // Customer-facing audit row (migration 0077, write half). UNSKIPPABLE /
        // fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER): emitted BEFORE the pat
        // INSERT so a self-serve key mint can NEVER commit without its customer-
        // visible audit row. `target` is the PAT id (no PII, minted above); the
        // summary names the key + granted scope. A failed insert → 503 and the
        // key is never created (the non-idempotent mint is not left half-applied).
        self.insert_audit_event(
            &req.caller_tenant,
            "pat.created",
            &req.principal,
            &pat.id.to_string(),
            &format!("Created API key {:?} ({scope_label})", req.name),
        )?;

        // Durable INSERT. `shown_once_token` (NOT NULL UNIQUE, 0037) is
        // a fresh UUID immediately marked consumed: the dashboard
        // returns the plaintext in THIS response (shown once) and the
        // reveal-endpoint path is never used for self-serve keys.
        self.run(
            "INSERT INTO pat \
             (pat_id, tenant_id, pat_hash, scope, expires_ms, \
              shown_once_token, shown_once_consumed, created_ms, token_id, name, find_only) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10)",
            vec![
                json!(pat.id.to_string()),
                json!(req.caller_tenant),
                json!(pat.hash.as_str()),
                json!(scope),
                json!(i64::try_from(expires_ms).unwrap_or(i64::MAX)),
                json!(Uuid::now_v7().to_string()),
                json!(i64::try_from(now_ms).unwrap_or(i64::MAX)),
                json!(pat.token_id.as_str()),
                json!(req.name),
                json!(i32::from(find_only)),
            ],
        )?;

        let row = PatRow::new(
            pat.id.to_string(),
            req.name.clone(),
            scope_to_list(scope, find_only),
            ms_to_iso8601(i64::try_from(now_ms).unwrap_or(i64::MAX)),
            None,
            None,
        );

        self.emit_audit(
            AuditEventKind::KeyCreateCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.name,
        )?;
        self.emit_sli(false);
        // The plaintext is returned ONCE here and is NEVER logged.
        Ok(KeyCreateResponse::new(row, plaintext.into_string()))
    }

    fn revoke(&self, req: KeyRevokeRequest) -> Result<KeyRevokeResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::KeysDenied,
            &req.principal,
            &req.pat_id,
        )?;
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::KeyRevokeAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.pat_id,
        )?;

        // Tenant-scoped SELECT: a PAT owned by another tenant is simply
        // not visible → NotFound (cross-tenant safe by construction). The
        // principal marker distinguishes owner/legacy rows (NULL) from a
        // member-issued PAT, allowing the admin-vs-owner safeguard below.
        let rows = self.run(
            "SELECT pat_id, token_id, name, scope, created_ms, revoked_at_ms, find_only, principal_id \
             FROM pat WHERE pat_id = ?1 AND tenant_id = ?2 LIMIT 1",
            vec![json!(req.pat_id), json!(req.caller_tenant)],
        )?;
        let Some(row) = rows.into_iter().next() else {
            self.emit_sli(true);
            return Err(CustomerHandlerError::NotFound {
                what: format!("pat_id={} for tenant={}", req.pat_id, req.caller_tenant),
            });
        };
        // The edge KV eviction handle must be known before the mutation. A
        // malformed legacy row therefore fails closed without revoking first.
        let cache_token_id = col_opt_str(&row, "token_id")
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                CustomerHandlerError::Internal(
                    "PAT row missing token_id; refusing ambiguous revocation".to_owned(),
                )
            })?;

        // Owner/legacy PATs have a NULL principal_id by contract (migration
        // 0075). An admin may manage member credentials, but must not revoke
        // the tenant owner's credential; only the owner can perform that
        // irreversible lockout. Keep the attempted audit, add the explicit
        // denial audit, and never issue an UPDATE on this path.
        if req.caller_role.trim().eq_ignore_ascii_case("admin")
            && col_opt_str(&row, "principal_id").is_none()
        {
            self.emit_audit(
                AuditEventKind::KeysDenied,
                &req.caller_tenant,
                &req.principal,
                &req.pat_id,
            )
            .inspect_err(|_| self.emit_sli(true))?;
            self.emit_sli(true);
            return Err(CustomerHandlerError::Unauthorized(
                "an admin cannot revoke the tenant owner's PAT".to_owned(),
            ));
        }

        let existing_revoked = col_opt_i64(&row, "revoked_at_ms");
        let revoked_at_ms = if let Some(already) = existing_revoked {
            // Idempotent: keep the original revocation timestamp.
            already
        } else {
            let now = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
            self.run(
                "UPDATE pat SET revoked_at_ms = ?1 \
                 WHERE pat_id = ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL",
                vec![json!(now), json!(req.pat_id), json!(req.caller_tenant)],
            )?;
            now
        };

        let pat = PatRow::new(
            col_opt_str(&row, "pat_id").unwrap_or_else(|| req.pat_id.clone()),
            col_opt_str(&row, "name").unwrap_or_default(),
            scope_to_list(
                &col_opt_str(&row, "scope").unwrap_or_default(),
                col_opt_i64(&row, "find_only").unwrap_or(0) == 1,
            ),
            col_opt_i64(&row, "created_ms")
                .map(ms_to_iso8601)
                .unwrap_or_default(),
            None,
            Some(ms_to_iso8601(revoked_at_ms)),
        );

        self.emit_audit(
            AuditEventKind::KeyRevokeCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.pat_id,
        )?;
        self.emit_sli(false);
        Ok(KeyRevokeResponse::new(pat).with_cache_token_id(cache_token_id))
    }
}

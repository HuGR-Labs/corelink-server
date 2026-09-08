impl CustomerTeamHandler for D1CustomerHandler {
    fn list(&self, req: TeamListRequest) -> Result<TeamListResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::TeamDenied,
            &req.principal,
            "",
        )?;
        // The tenant's Clerk binding is the canonical OWNER (synthesized — the
        // email is NOT stored in D1, only `email_hash` per CTRL-PRIV-001, so it
        // is shown as "—"). Additional seats are real `team_member` rows
        // (ADR-S33-001, migration 0074), appended below.
        let tenant = self
            .tenant_row(&req.caller_tenant)?
            .ok_or_else(|| self.tenant_not_found(&req.caller_tenant))?;

        let owner_id =
            col_opt_str(&tenant, "clerk_user_id").unwrap_or_else(|| req.caller_tenant.clone());
        let joined_at = col_opt_i64(&tenant, "created_at_ms")
            .map(ms_to_iso8601)
            .unwrap_or_default();
        let mut members = vec![TeamMemberRow::new(
            owner_id.clone(),
            "—",
            "owner",
            joined_at,
            "active",
        )];

        // Seats from `team_member` (active + invited; `removed` rows are audit
        // tombstones and not listed). Skip any duplicate of the synthesized owner.
        let rows = self.run(
            "SELECT user_id, role, status, joined_at_ms, invited_at_ms \
             FROM team_member \
             WHERE tenant_id = ?1 AND status IN ('active','invited') \
             ORDER BY invited_at_ms ASC",
            vec![json!(req.caller_tenant)],
        )?;
        for row in &rows {
            let user_id = col_opt_str(row, "user_id").unwrap_or_default();
            if user_id.is_empty() || user_id == owner_id {
                continue;
            }
            let role = col_opt_str(row, "role").unwrap_or_else(|| "member".to_owned());
            let role = persisted_team_role(&role).ok_or_else(|| {
                CustomerHandlerError::Internal("invalid persisted team role".to_owned())
            })?;
            let status = col_opt_str(row, "status").unwrap_or_else(|| "invited".to_owned());
            let joined = col_opt_i64(row, "joined_at_ms")
                .or_else(|| col_opt_i64(row, "invited_at_ms"))
                .map(ms_to_iso8601)
                .unwrap_or_default();
            members.push(TeamMemberRow::new(user_id, "—", role, joined, status));
        }

        self.emit_sli(false);
        Ok(TeamListResponse::new(members))
    }

    fn invite(&self, req: TeamInviteRequest) -> Result<TeamInviteResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::TeamDenied,
            &req.principal,
            &req.email,
        )?;
        // Validate the requested role before emitting ANY audit event or
        // touching the persistence layer. Unknown/non-grantable values must
        // be a clean 400, not a TeamInviteAttempted record for an operation
        // that can never be performed.
        let role = match normalize_invite_role(&req.role) {
            Ok(role) => role,
            Err(err) => {
                self.emit_sli(true);
                return Err(err);
            }
        };

        // Attempted audit BEFORE the mutation (fail-CLOSED ordering) — mirrors
        // `remove()` / `create()`.
        self.emit_audit(
            AuditEventKind::TeamInviteAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.email,
        )?;

        // B-073: generate a high-entropy, opaque, one-time capability. Only its
        // SHA-256 digest is persisted; the plaintext is returned exactly once to
        // the authenticated inviter. The digest is bound to this row's tenant and
        // invitation id by the UPDATE predicate used by the acceptance endpoint.
        //
        // CTRL-PRIV-001: only the pseudonymized SHA-256 email hash is stored, never
        // the raw invitee email (mirrors the rest of the D1 schema + 0074's header).
        // NORMALIZE (trim + lowercase) BEFORE hashing — this is the join key the
        // Worker redemption (`redeemTeamInvitation`, `emailHashCandidates`) matches on,
        // and it normalizes identically; without it an invite to `Alice@Example.com`
        // would never flip to `active` when Clerk delivers `alice@example.com`.
        // ONE canonical scheme (CTRL-PRIV-001): HMAC-SHA256 under `EMAIL_HASH_SALT`
        // when set, else unsalted SHA-256 (pre-salt parity). The DSR rectification
        // and the signup-worker accept-match MUST use the SAME helper, or the join
        // key diverges. Normalization (trim+lowercase) lives inside the helper.
        let email_hash = crate::email_hash::hash_email(&req.email);
        // No real Clerk user_id exists yet (OB-1) — `team_member.user_id` is NOT
        // NULL (PK), so a fresh UUID is the invitation-id placeholder 0074 expects
        // ("carries the Clerk invitation id until acceptance binds the real user").
        let invitation_id = Uuid::now_v7().to_string();
        let invited_at_ms = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);
        let mut token_bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut token_bytes);
        let invitation_token = hex::encode(token_bytes);
        let invitation_token_hash = hex::encode(Sha256::digest(invitation_token.as_bytes()));

        // Audit and invited-seat persistence are one typed D1 transaction. D1
        // rolls both rows back if either fixed statement fails.
        self.db
            .invite_team_member_with_audit(CustomerTeamInviteOperation {
                tenant_id: req.caller_tenant.clone(),
                invitation_id: invitation_id.clone(),
                email_hash,
                invitation_token_hash,
                role: role.to_owned(),
                invited_by: req.principal.clone(),
                invited_at_ms,
                audit_actor: req.principal.clone(),
                audit_ts_ms: invited_at_ms,
                audit_detail: format!("Invited a team member with role {role}"),
            })
            .map_err(|e| self.atomic_error(e))?;

        self.emit_audit(
            AuditEventKind::TeamInviteCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.email,
        )?;

        // Response mirrors the `InMemoryCustomerHandler::invite` shape (echoes the
        // caller-supplied email + requested role; status `invited`). The raw email
        // is reflected back to the caller that supplied it — it is NOT persisted
        // (only `email_hash` is).
        let member = TeamMemberRow::new(
            invitation_id,
            req.email.clone(),
            role,
            String::new(),
            "invited",
        );
        self.emit_sli(false);
        Ok(TeamInviteResponse::with_token(member, invitation_token))
    }

    fn remove(&self, req: TeamRemoveRequest) -> Result<TeamRemoveResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::TeamDenied,
            &req.principal,
            &req.target_user_id,
        )?;
        // Attempted audit BEFORE the mutation (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::TeamRemoveAttempted,
            &req.caller_tenant,
            &req.principal,
            &req.target_user_id,
        )?;

        // The member must exist under THIS tenant and not be the owner.
        let rows = self.run(
            "SELECT role, status FROM team_member WHERE tenant_id = ?1 AND user_id = ?2 LIMIT 1",
            vec![json!(req.caller_tenant), json!(req.target_user_id)],
        )?;
        let Some(row) = rows.into_iter().next() else {
            self.emit_sli(true);
            return Err(CustomerHandlerError::NotFound {
                what: format!("team member={}", req.target_user_id),
            });
        };
        let role = col_opt_str(&row, "role").unwrap_or_default();
        if role.eq_ignore_ascii_case("owner") {
            self.emit_sli(true);
            return Err(CustomerHandlerError::Unauthorized(
                "cannot remove the tenant owner".to_owned(),
            ));
        }
        // Count the member's live PATs BEFORE revoking (the D1 query bridge
        // returns rows, not an UPDATE changes-count) — this is the revoked total.
        let live = self.run(
            "SELECT pat_id FROM pat \
             WHERE tenant_id = ?1 AND principal_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(req.caller_tenant), json!(req.target_user_id)],
        )?;
        let revoked_pats = u32::try_from(live.len()).unwrap_or(u32::MAX);

        let now = i64::try_from(self.clock.now_ms()).unwrap_or(i64::MAX);

        // Revoke every live PAT the member holds for this tenant — the
        // load-bearing security effect of seat removal.
        self.run(
            "UPDATE pat SET revoked_at_ms = ?3 \
             WHERE tenant_id = ?1 AND principal_id = ?2 AND revoked_at_ms IS NULL",
            vec![
                json!(req.caller_tenant),
                json!(req.target_user_id),
                json!(now),
            ],
        )?;

        // Flip the seat to `removed` (retain as an audit tombstone).
        self.run(
            "UPDATE team_member SET status = 'removed', joined_at_ms = joined_at_ms \
             WHERE tenant_id = ?1 AND user_id = ?2",
            vec![json!(req.caller_tenant), json!(req.target_user_id)],
        )?;

        self.emit_audit(
            AuditEventKind::TeamRemoveCommitted,
            &req.caller_tenant,
            &req.principal,
            &req.target_user_id,
        )?;

        let member = TeamMemberRow::new(
            req.target_user_id.clone(),
            "—",
            if role.is_empty() {
                "member".to_owned()
            } else {
                role
            },
            String::new(),
            "removed",
        );
        self.emit_sli(false);
        Ok(TeamRemoveResponse::new(member, revoked_pats))
    }
}

impl CustomerAuditHandler for D1CustomerHandler {
    fn query(&self, req: AuditQueryRequest) -> Result<AuditQueryResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::AuditQueryDenied,
            &req.principal,
            "",
        )?;
        self.emit_audit(
            AuditEventKind::AuditQueryAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        // Real customer-facing activity log (migration 0077): newest-first,
        // tenant-scoped, bounded. Written UNSKIPPABLE / fail-CLOSED by the
        // control-plane mutations (`create` / `invite`). Fail-CLOSED on transport error
        // (`self.run`), never degraded to fabricated empty data.
        //
        // Contract: honor the `?from=`/`?kind=` filters (`req.since` /
        // `req.event_types`). The WHERE clause is BUILT with generated
        // positional placeholders (`?N`) and every value is BOUND through
        // `self.run` — no value is ever string-interpolated, so the dynamic
        // shape carries NO injection surface. An absent filter is omitted
        // (behaves as before — no narrowing). Tenant scope stays fail-CLOSED
        // (always `WHERE tenant_id = ?1`).
        let mut sql = String::from(
            "SELECT id, event_type, actor, target, ts_ms, detail \
             FROM customer_audit_events WHERE tenant_id = ?1",
        );
        let mut binds: Vec<Value> = vec![json!(req.caller_tenant)];

        // `?from=` → `AND ts_ms >= ?` (ISO-8601 parsed to the integer ts_ms
        // domain). A present-but-unparseable `since` applies NO filter
        // (lenient — never silently drops the customer's rows on a bad param).
        if let Some(since_ms) = req.since.as_deref().and_then(iso8601_to_ms) {
            binds.push(json!(since_ms));
            sql.push_str(&format!(" AND ts_ms >= ?{}", binds.len()));
        }

        // `?kind=` → `AND event_type IN (?, ?, …)`. Placeholders are GENERATED
        // (positional `?N`); each event-type value is BOUND, never interpolated.
        if !req.event_types.is_empty() {
            let first = binds.len() + 1;
            let placeholders: Vec<String> = (first..first + req.event_types.len())
                .map(|n| format!("?{n}"))
                .collect();
            for et in &req.event_types {
                binds.push(json!(et));
            }
            sql.push_str(&format!(" AND event_type IN ({})", placeholders.join(", ")));
        }

        // Newest-first, bounded (preserved).
        binds.push(json!(AUDIT_QUERY_LIMIT));
        sql.push_str(&format!(" ORDER BY ts_ms DESC LIMIT ?{}", binds.len()));

        let rows = self.run(&sql, binds)?;
        let event_rows: Vec<CustomerAuditEventRow> = rows
            .iter()
            .map(|row| {
                CustomerAuditEventRow::new(
                    col_opt_i64(row, "id")
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    col_opt_i64(row, "ts_ms")
                        .map(ms_to_iso8601)
                        .unwrap_or_default(),
                    col_opt_str(row, "event_type").unwrap_or_default(),
                    // No per-event severity column; the customer-facing surface
                    // carries only informational activity rows.
                    "info",
                    col_opt_str(row, "actor").unwrap_or_default(),
                    col_opt_str(row, "detail").unwrap_or_default(),
                )
            })
            .collect();
        let resp = AuditQueryResponse::new(event_rows);

        self.emit_audit(
            AuditEventKind::AuditQueryServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }
}

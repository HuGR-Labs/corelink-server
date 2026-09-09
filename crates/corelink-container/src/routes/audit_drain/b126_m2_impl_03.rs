/// `POST /_internal/audit/drain` — seal every pending partition. Returns a
/// summary `{ ok, partitions_drained, rows_sealed, partitions_drifted,
/// partitions_failed, partitions_leased, heads_resigned, incomplete }`
/// (`partitions_leased` is B-038 lease backpressure, always 0 while
/// `AUDIT_DRAIN_LEASE_ENABLED` is OFF). Internal-auth gated; no request body.
async fn handle_drain(State(state): State<AuditDrainState>, headers: HeaderMap) -> Response {
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let partitions = match read_pending_partitions(&state.d1).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "audit/drain: partition scan failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "drain scan failed").into_response();
        }
    };
    let now = now_ms();
    let mut partitions_drained: u64 = 0;
    let mut rows_sealed: u64 = 0;
    let mut partitions_drifted: u64 = 0;
    let mut partitions_failed: u64 = 0;
    // B-038: partitions skipped because another drain holds their lease
    // (`AUDIT_DRAIN_LEASE_ENABLED` ON) — normal backpressure, always 0 when OFF.
    let mut partitions_leased: u64 = 0;
    // Global per-call row budget (see `AuditDrainState::batch_limit`). Bounds the
    // total sealing work so a cold backlog can never make one call exceed the edge
    // subrequest timeout. When the budget is spent (or a partition's tail was
    // truncated by it) the call returns `incomplete: true` — a caller (hourly cron
    // or a manual loop) re-drains until it returns `incomplete: false`.
    let mut remaining = state.batch_limit;
    let mut incomplete = false;
    // CF-6: borrow the keyed-head signing seed once for the whole sweep.
    let signing_seed: Option<&[u8; 32]> = state.signing_seed.as_deref().map(|z| &**z);
    for (tenant_id, region) in partitions {
        if remaining <= 0 {
            // Budget spent before every partition was visited — pending rows
            // remain in the unvisited partitions; a re-drain is required.
            incomplete = true;
            break;
        }
        let limit = remaining;
        match drain_partition(
            &state.d1,
            &tenant_id,
            &region,
            now,
            signing_seed,
            state.signing_key_id,
            state.trust_unsigned_resume,
            limit,
            state.lease_enabled,
            state.witness.as_deref(),
            state.link_keyring.as_deref(),
        )
        .await
        {
            Ok(PartitionOutcome::Sealed(n)) => {
                partitions_drained = partitions_drained.saturating_add(1);
                rows_sealed = rows_sealed.saturating_add(n);
                let sealed_i = i64::try_from(n).unwrap_or(i64::MAX);
                remaining = remaining.saturating_sub(sealed_i);
                // A full batch means this partition's unsealed tail was truncated
                // by the limit — it may hold more rows a later call must seal.
                if sealed_i >= limit {
                    incomplete = true;
                }
            }
            Ok(PartitionOutcome::Fenced(n)) => {
                // B-038 fence: the pre-expiry PREFIX (`n` rows) was sealed but the
                // head was NOT advanced. Treated like a truncated batch — count the
                // rows and force a re-drain (which resumes from the sealed tail).
                rows_sealed = rows_sealed.saturating_add(n);
                let sealed_i = i64::try_from(n).unwrap_or(i64::MAX);
                remaining = remaining.saturating_sub(sealed_i);
                incomplete = true;
                tracing::warn!(
                    region = %region,
                    prefix_sealed = n,
                    "audit/drain: partition fenced at lease expiry — prefix sealed, re-drain required"
                );
            }
            Ok(PartitionOutcome::Leased) => {
                partitions_leased = partitions_leased.saturating_add(1);
                tracing::debug!(
                    region = %region,
                    "audit/drain: partition lease held by another drain — skipped"
                );
            }
            Ok(PartitionOutcome::Drift) => {
                partitions_drifted = partitions_drifted.saturating_add(1);
                tracing::warn!(
                    region = %region,
                    "audit/drain: head drift (concurrent drain) — partition aborted, no fork"
                );
            }
            Ok(PartitionOutcome::Empty) => {}
            Err(e) => {
                partitions_failed = partitions_failed.saturating_add(1);
                tracing::error!(error = %e, region = %region, "audit/drain: partition failed");
            }
        }
    }

    // CF-6 convergence sweep (self-gated, D1-only — see `converge_unsigned_heads`).
    let (heads_resigned, heads_resign_incomplete) = converge_unsigned_heads(
        &state.d1,
        signing_seed,
        state.trust_unsigned_resume,
        state.signing_key_id,
        state.batch_limit,
        now,
    )
    .await;
    incomplete = incomplete || heads_resign_incomplete;
    let ok = audit_drain_ok(partitions_failed);
    (
        StatusCode::OK,
        Json(build_drain_response_body(&DrainOutcome {
            ok,
            partitions_drained,
            rows_sealed,
            partitions_drifted,
            partitions_failed,
            partitions_leased,
            heads_resigned,
            incomplete,
        })),
    )
        .into_response()
}

/// WP-L MED-21: the canonical "sweep as a whole succeeded" signal.
/// `partitions_failed > 0` means at least one partition could not be
/// drained (transport / consistency / lock failure on that key);
/// returning `ok:true` while the chain is silently NOT advancing is the
/// exact trap that hid the original bug.
///
/// Pure fn (extracted so the invariant has a unit-test anchor that does
/// not require a live D1 client or an HTTP harness).
pub(crate) const fn audit_drain_ok(partitions_failed: u64) -> bool {
    partitions_failed == 0
}

/// Build the JSON body returned by `POST /_internal/audit/drain`. Extracted
/// so the `ok: partitions_failed == 0` invariant + the
/// `incomplete`-independence-from-`ok` invariant are unit-testable without
/// a live handler / D1.
/// The counters one drain sweep produced. A struct rather than eight
/// positional parameters: six of them are `u64` and two are `bool`, so any
/// transposition at a call site type-checks and silently reports the wrong
/// number. `clippy::too_many_arguments` was flagging exactly that risk.
pub(crate) struct DrainOutcome {
    pub(crate) ok: bool,
    pub(crate) partitions_drained: u64,
    pub(crate) rows_sealed: u64,
    pub(crate) partitions_drifted: u64,
    pub(crate) partitions_failed: u64,
    pub(crate) partitions_leased: u64,
    pub(crate) heads_resigned: u64,
    pub(crate) incomplete: bool,
}

pub(crate) fn build_drain_response_body(outcome: &DrainOutcome) -> serde_json::Value {
    let DrainOutcome {
        ok,
        partitions_drained,
        rows_sealed,
        partitions_drifted,
        partitions_failed,
        partitions_leased,
        heads_resigned,
        incomplete,
    } = *outcome;
    serde_json::json!({
        "ok": ok,
        "partitions_drained": partitions_drained,
        "rows_sealed": rows_sealed,
        "partitions_drifted": partitions_drifted,
        "partitions_failed": partitions_failed,
        // B-038: partitions another drain's live lease made us skip (always 0
        // unless AUDIT_DRAIN_LEASE_ENABLED is ON). Normal backpressure.
        "partitions_leased": partitions_leased,
        // CF-6 convergence: legacy UNSIGNED/foreign-key heads re-signed in place
        // this sweep (only non-zero while AUDIT_CHAIN_TRUST_UNSIGNED_RESUME is ON).
        "heads_resigned": heads_resigned,
        // `true` => the per-call row budget bounded this sweep and pending rows
        // remain; a caller (hourly cron or a manual loop) must re-drain until
        // this is `false`. Independent of `partitions_failed` (a distinct retry
        // signal).
        "incomplete": incomplete,
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_1_1.rs");
    include!("b126_m2_test_1_1_part2.rs");
    include!("b126_m2_test_1_2.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_1_1_REANCHOR, B126_M2_TEST_1_2_REANCHOR];
    }

    #[test]
    fn b054_epoch_admin_requires_two_distinct_named_headers() {
        let executor = b"executor-secret-that-is-at-least-32-bytes";
        let approver = b"approver-secret-that-is-at-least-32-bytes";
        let mut headers = HeaderMap::new();
        headers.insert(
            INTERNAL_AUTH_HEADER,
            axum::http::HeaderValue::from_static("executor-secret-that-is-at-least-32-bytes"),
        );
        assert!(internal_auth_ok(executor, &headers));
        assert!(!named_auth_ok(
            approver,
            &headers,
            B054_SECURITY_APPROVAL_HEADER
        ));

        headers.insert(
            B054_SECURITY_APPROVAL_HEADER,
            axum::http::HeaderValue::from_static("approver-secret-that-is-at-least-32-bytes"),
        );
        assert!(internal_auth_ok(executor, &headers));
        assert!(named_auth_ok(
            approver,
            &headers,
            B054_SECURITY_APPROVAL_HEADER
        ));

        headers.insert(
            B054_SECURITY_APPROVAL_HEADER,
            axum::http::HeaderValue::from_static("executor-secret-that-is-at-least-32-bytes"),
        );
        assert!(!named_auth_ok(
            approver,
            &headers,
            B054_SECURITY_APPROVAL_HEADER
        ));
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_3_REANCHOR: () = ();

// ─── Frozen maps ──────────────────────────────────────────────────────────────

/// FROZEN billing status map (`tenant_billing.status` → dashboard
/// status): `paid`→`active`, `past_due`→`past_due`,
/// `canceled`→`canceled`, `incomplete`→`past_due`,
/// `inactive`/no-row/unknown→`inactive`.
///
/// AUDIT REV-S5 (low, KNOWN-LIMITATION, deferred — contract-level fix):
/// `incomplete` (Stripe's "subscription created, first payment not yet
/// settled") is collapsed onto `past_due`, so a pending FIRST payment is
/// surfaced to the dashboard as a renewal FAILURE. The audit recommends a
/// distinct `pending`/`awaiting_payment` status. We do NOT introduce one
/// here: the dashboard status set is FROZEN and shared cross-team —
/// `apps/admin-ui/src/lib/customer-types.ts` types it as the closed union
/// `"trialing" | "active" | "past_due" | "canceled" | "inactive"`, and
/// `specs/03_architecture/data_model.md` pins the `tenant_billing.status`
/// CHECK to `('active','past_due','canceled')`. Emitting a new `pending`
/// string from this map alone would drift the backend off that frozen
/// contract (the frontend renders `data.status` verbatim and could not
/// classify it). A real fix must land as a coordinated change across the
/// TS union + the spec CHECK + (optionally) a new `BillingResponse` field
/// — out of scope for a `customer_d1.rs`-only patch. The `incomplete` arm
/// is split out below (still → `past_due`, byte-for-byte identical output)
/// purely to make this decision explicit and give that future fix an
/// anchor; it changes NO emitted value.
#[must_use]
pub fn map_billing_status(d1_status: Option<&str>) -> &'static str {
    match d1_status {
        Some("paid") => "active",
        Some("past_due") => "past_due",
        // KNOWN-LIMITATION (REV-S5): would ideally map to a distinct
        // `pending`, but the frozen cross-team status union has no such
        // value — keep `past_due` until that contract is widened.
        Some("incomplete") => "past_due",
        Some("canceled") => "canceled",
        _ => "inactive",
    }
}

/// FROZEN requested-scopes → D1 `pat.scope` map: `admin` is NEVER
/// grantable self-serve (`Err`); anything carrying a write capability →
/// `read-write`; otherwise (incl. the canonical `["cache:read"]` and an
/// empty request — least privilege) → `read-only`.
fn map_requested_scopes(requested: &[String]) -> Result<&'static str, CustomerHandlerError> {
    // SINGLE source of truth with the mint escalation gate
    // (`routes::customer::mint_requests_write`), via `scope::classify_requested_scopes`.
    // rt-nuclear #15: this used to substring-match (`s.contains("write")`) while
    // the gate exact-matched, so `"writes"` skipped the gate yet persisted
    // `read-write` (read-only PAT self-escalation). Now both share one exact-token
    // classifier, and unrecognized tokens are REJECTED (fail-CLOSED) rather than
    // silently mapped to a privilege.
    match crate::scope::classify_requested_scopes(requested) {
        // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only`
        // (`pat.scope` CHECK forbids a 4th value, migration 0037) and is narrowed
        // to find-missing via the additive `find_only` marker (migration 0093) —
        // NOT a new `pat.scope` string. See `mint_is_find_only`.
        Ok(crate::scope::RequestedScopeClass::FindMissing)
        | Ok(crate::scope::RequestedScopeClass::ReadOnly) => Ok("read-only"),
        Ok(crate::scope::RequestedScopeClass::ReadWrite) => Ok("read-write"),
        Ok(crate::scope::RequestedScopeClass::Admin) => Err(CustomerHandlerError::Unauthorized(
            "the admin scope is not grantable via self-serve key creation".to_owned(),
        )),
        Err(token) => Err(CustomerHandlerError::Unauthorized(format!(
            "unrecognized scope token {token:?}; valid self-serve scopes: cache:read, cache:write, cache:find-missing"
        ))),
    }
}

/// True when the requested scopes classify as FIND-ONLY (the true least-privilege
/// scope, ADR-0071) — a `find-missing` grant with NO read/write. The mint stores
/// `pat.scope = 'read-only'` (CHECK-safe) PLUS the `find_only = 1` marker; the
/// Worker then narrows the forwarded `x-corelink-scope` to `find-missing`. Any
/// unrecognized/admin token classifies elsewhere and is rejected by
/// [`map_requested_scopes`], so this is only ever consulted after that succeeds.
fn mint_is_find_only(requested: &[String]) -> bool {
    matches!(
        crate::scope::classify_requested_scopes(requested),
        Ok(crate::scope::RequestedScopeClass::FindMissing)
    )
}

/// D1 `pat.scope` string → dashboard scopes list (inverse of
/// [`map_requested_scopes`] for the canonical values; unknown legacy
/// values are surfaced verbatim rather than guessed at).
fn scope_to_list(scope: &str, find_only: bool) -> Vec<String> {
    // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only` + the
    // `find_only` marker, so surface it as the find-missing capability it
    // actually grants (NOT `cache:read`, which it cannot do).
    if find_only {
        return vec!["cache:find-missing".to_owned()];
    }
    match scope {
        "read-only" => vec!["cache:read".to_owned()],
        "read-write" => vec!["cache:read".to_owned(), "cache:write".to_owned()],
        "" => vec![],
        other => vec![other.to_owned()],
    }
}

/// Accept only roles that can be inserted by a self-serve invite. Unknown
/// values are rejected rather than silently persisted as `member`.
fn normalize_invite_role(role: &str) -> Result<&'static str, CustomerHandlerError> {
    canonical_invite_role(role).ok_or_else(|| {
        CustomerHandlerError::InvalidRequest("unsupported team invite role".to_owned())
    })
}

/// Decode persisted role values before returning them on the API wire.
fn persisted_team_role(role: &str) -> Option<&'static str> {
    match role {
        "owner" => Some("owner"),
        "admin" => Some("admin"),
        "member" => Some("member"),
        "viewer" => Some("viewer"),
        _ => None,
    }
}

// ─── Calendar helpers (no chrono/time dependency in this crate) ──────────────

/// Days-since-epoch → civil (y, m, d). Howard Hinnant's `civil_from_days`
/// algorithm (public domain), exact for the full i64 day range we use.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// Civil (y, m, d) → days-since-epoch. Howard Hinnant's `days_from_civil`
/// algorithm (public domain), the exact inverse of [`civil_from_days`]. Used to
/// turn an ISO-8601 `since` filter back into the integer `ts_ms` domain.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let m = i64::from(m);
    let d = i64::from(d);
    let y = if m <= 2 { y - 1 } else { y };
    // `div_euclid` floors (matching `civil_from_days`), so no truncating-
    // division `y - 399` adjustment is needed — that would double-correct.
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Unix-ms → `"YYYY-MM-DDTHH:MM:SSZ"` (UTC, second precision).
#[must_use]
pub fn ms_to_iso8601(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400); // [0, 86399]
    let (y, m, d) = civil_from_days(days);
    let hh = sod / 3600;
    let mi = (sod % 3600) / 60;
    let ss = sod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mi:02}:{ss:02}Z")
}

/// ISO-8601 / RFC3339 UTC timestamp → unix-ms. The inverse of
/// [`ms_to_iso8601`], used to honor the `GET /v1/customer/audit?from=` filter
/// (`AuditQueryRequest::since`), whose contract type is an ISO-8601 string while
/// the `customer_audit_events.ts_ms` column is integer millis.
///
/// Accepts the canonical `ms_to_iso8601` output (`YYYY-MM-DDTHH:MM:SSZ`) plus
/// common variants: a bare date (`YYYY-MM-DD`), a space date/time separator, an
/// optional fractional-second part, and an optional trailing `Z`. Returns
/// `None` for anything it cannot parse — the caller then applies NO `since`
/// filter (lenient: a malformed param never silently drops the customer's rows,
/// nor errors their whole activity read). UTC-only, mirroring `ms_to_iso8601`.
#[must_use]
fn iso8601_to_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    // Split date from the optional time component on 'T' or ' '.
    let (date, time) = s
        .split_once(['T', ' '])
        .map_or((s, None), |(d, t)| (d, Some(t)));
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: u32 = dp.next()?.parse().ok()?;
    let d: u32 = dp.next()?.parse().ok()?;
    if dp.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (mut hh, mut mi, mut ss): (i64, i64, i64) = (0, 0, 0);
    if let Some(t) = time {
        // Strip a trailing 'Z' and any fractional-second suffix.
        let t = t.trim_end_matches('Z');
        let t = t.split_once('.').map_or(t, |(whole, _)| whole);
        let mut tp = t.split(':');
        hh = tp.next()?.parse().ok()?;
        mi = tp.next().map_or(Ok(0), str::parse).ok()?;
        ss = tp.next().map_or(Ok(0), str::parse).ok()?;
        if !(0..=23).contains(&hh) || !(0..=59).contains(&mi) || !(0..=60).contains(&ss) {
            return None;
        }
    }
    Some((days_from_civil(y, m, d) * 86_400 + hh * 3_600 + mi * 60 + ss) * 1_000)
}

/// Unix-ms → billing period `"YYYY-MM"` (UTC).
#[must_use]
pub fn period_from_ms(unix_ms: u64) -> String {
    let secs = i64::try_from(unix_ms / 1000).unwrap_or(i64::MAX);
    let (y, m, _) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}")
}

// ─── Row-extraction helpers ───────────────────────────────────────────────────

/// Extract an optional string column (`None` for SQL NULL / absent).
fn col_opt_str(row: &D1Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Extract an optional integer column (`None` for SQL NULL / absent).
fn col_opt_i64(row: &D1Row, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

/// Extract a monotonic counter column as `u64` (`0` for NULL / absent /
/// negative — the `usage_daily` CHECK constraints keep these non-negative).
fn col_u64(row: &D1Row, key: &str) -> u64 {
    col_opt_i64(row, key)
        .and_then(|c| u64::try_from(c).ok())
        .unwrap_or(0)
}

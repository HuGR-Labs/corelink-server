//! Shared D1 helpers for the WI-S11-008 Wave 1 erasure transports
//! (ledger, audit sink, D1 erase adapter).
//!
//! The canonical async→sync bridge mirrors [`crate::customer_d1`]: the
//! erasure trait surfaces (`ErasureIdempotencyLedger`,
//! `ErasureAuditSink`, `BackendErasureAdapter`) are SYNCHRONOUS, but
//! [`D1HttpClient::query`] is async. We are always called from inside an
//! axum handler running on the multi-thread tokio runtime, so
//! `tokio::task::block_in_place` hands the worker thread back to the
//! scheduler while `Handle::current().block_on` drives the D1 round-trip
//! (identical rationale + safety envelope as `D1HttpCustomerDb::query`).

use std::sync::Arc;

use serde_json::Value;
use zeroize::Zeroizing;

use corelink_tenant_path::TenantDerivationKey;

use crate::storage::d1_http::{D1HttpClient, D1Row};

/// Run a single parameterised D1 statement to completion from a sync
/// context. MUST be called on a multi-thread tokio runtime worker thread
/// (the axum handler path) — `block_in_place` panics otherwise, exactly
/// as documented for [`crate::customer_d1::D1HttpCustomerDb::query`].
pub(super) fn d1_query_blocking(
    client: &Arc<D1HttpClient>,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<D1Row>, String> {
    let client = Arc::clone(client);
    let sql = sql.to_owned();
    tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move { client.query(&sql, &params).await })
    })
}

/// Extract a string column (`None` for SQL NULL / absent / non-string).
pub(super) fn col_str(row: &D1Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Extract an `i64` column (`None` for SQL NULL / absent / non-integer).
pub(super) fn col_i64(row: &D1Row, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

/// Load the tenant derivation key from `R2_TDK_HEX` (64 hex chars = 32 bytes),
/// mirroring `storage::r2_s3::load_tdk_from_env`.
///
/// The R2 erase adapters key every object by `derive_prefix(tdk, tenant)` — the
/// SAME 16-char `URL_SAFE_NO_PAD(HMAC-SHA256(tdk, tenant))[..16]` the CAS/AC
/// write path used — so deriving with this key matches the stored objects **by
/// construction** (no reliance on a materialised-prefix column / BLOB wire
/// form). Returns `None` when unset/malformed; the adapters then fail CLOSED
/// (they cannot address the tenant's R2 objects, so must not report success).
///
/// NOTE: assumes a single live TDK version (`path_key_id = 1`, true at launch).
/// Under TDK rotation an entry written with an older key would need a versioned
/// lookup — flagged in ADR-S11-013; out of scope for the launch erase-set.
pub(super) fn load_tdk() -> Option<TenantDerivationKey> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(hex_str, bytes.as_mut()).ok()?;
    Some(TenantDerivationKey::from_bytes(bytes))
}

/// `u64` epoch-ms → `i64` for [`crate::customer_d1::ms_to_iso8601`],
/// saturating on overflow (epoch ms never realistically exceeds `i64`).
pub(super) fn clamp_ms(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

/// Read a single-column `SELECT COUNT(*)` style scalar from the first row.
/// Returns 0 when the result set is empty or the column is absent.
pub(super) fn scalar_count(rows: &[D1Row], col: &str) -> u64 {
    rows.first()
        .and_then(|r| col_i64(r, col))
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(0)
}

/// Inverse of [`crate::customer_d1::ms_to_iso8601`] for the exact
/// fixed-width `"YYYY-MM-DDTHH:MM:SSZ"` shape it emits. Second precision
/// (the ISO column itself is second-precision, so sub-second ms cannot be
/// round-tripped — this is a property of the canonical
/// `dsr_erasure_log.started_at`/`completed_at` TEXT columns, not a defect
/// here). Returns `None` on any malformed input.
pub(super) fn iso8601_to_ms(s: &str) -> Option<u64> {
    // "YYYY-MM-DDTHH:MM:SSZ" — 20 bytes, ASCII. Use `.get()` (not `[]`) to
    // stay clear of the workspace `deny(clippy::indexing_slicing)`.
    let b = s.as_bytes();
    if b.len() != 20
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || b.get(10) != Some(&b'T')
        || b.get(19) != Some(&b'Z')
    {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> { s.get(from..to)?.parse::<i64>().ok() };
    let y = num(0, 4)?;
    let m = u32::try_from(num(5, 7)?).ok()?;
    let d = u32::try_from(num(8, 10)?).ok()?;
    let hh = num(11, 13)?;
    let mi = num(14, 16)?;
    let ss = num(17, 19)?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let secs = days_from_civil(y, m, d) * 86_400 + hh * 3600 + mi * 60 + ss;
    u64::try_from(secs.checked_mul(1000)?).ok()
}

/// Howard Hinnant's `days_from_civil` (public domain) — civil (y,m,d) →
/// days since the Unix epoch. Inverse of the `civil_from_days` used by
/// [`crate::customer_d1::ms_to_iso8601`].
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = i64::from(m);
    let d = i64::from(d);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;
    use crate::customer_d1::ms_to_iso8601;

    #[test]
    fn iso_roundtrip_second_precision() {
        // ms truncated to the second, then round-tripped, is stable.
        for ms in [0i64, 1_000, 1_700_000_000_000, 1_699_999_999_000] {
            let sec_ms = (ms / 1000) * 1000;
            let iso = ms_to_iso8601(sec_ms);
            assert_eq!(
                iso8601_to_ms(&iso),
                Some(u64::try_from(sec_ms).unwrap()),
                "iso={iso}"
            );
        }
    }

    #[test]
    fn iso_rejects_malformed() {
        assert_eq!(iso8601_to_ms("not-a-timestamp"), None);
        assert_eq!(iso8601_to_ms("2026-13-01T00:00:00Z"), None); // month 13
        assert_eq!(iso8601_to_ms(""), None);
    }

    #[test]
    fn tdk_absent_is_none() {
        // With no R2_TDK_HEX set the loader yields None (adapters fail closed).
        // (Cannot assert the Some path without mutating process env in a
        // shared test binary.)
        std::env::remove_var("R2_TDK_HEX");
        assert!(load_tdk().is_none());
    }
}

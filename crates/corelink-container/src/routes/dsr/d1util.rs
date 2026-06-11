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
        tokio::runtime::Handle::current()
            .block_on(async move { client.query(&sql, &params).await })
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

/// Read a D1 BLOB column as a lowercase hex string.
///
/// The Cloudflare D1 HTTP query API represents a BLOB as a JSON **array of
/// byte integers** (the canonical form); we also accept an already-hex string
/// defensively. Any other / unrecognised encoding returns `None` — the callers
/// (R2 erase adapters) then **fail CLOSED** (Transport error → retry/surface)
/// rather than build a wrong key and silently skip a PII object. Returns `None`
/// for SQL NULL / absent too.
pub(super) fn col_blob_hex(row: &D1Row, key: &str) -> Option<String> {
    match row.get(key)? {
        // Canonical D1 form: array of 0..=255 byte values.
        Value::Array(arr) => {
            let bytes: Option<Vec<u8>> = arr
                .iter()
                .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect();
            bytes.map(hex::encode)
        }
        // Defensive: an already-hex string (even length, all hex digits).
        Value::String(s) => {
            let t = s.trim();
            if !t.is_empty() && t.len() % 2 == 0 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
                Some(t.to_ascii_lowercase())
            } else {
                None
            }
        }
        _ => None,
    }
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
            assert_eq!(iso8601_to_ms(&iso), Some(u64::try_from(sec_ms).unwrap()), "iso={iso}");
        }
    }

    #[test]
    fn iso_rejects_malformed() {
        assert_eq!(iso8601_to_ms("not-a-timestamp"), None);
        assert_eq!(iso8601_to_ms("2026-13-01T00:00:00Z"), None); // month 13
        assert_eq!(iso8601_to_ms(""), None);
    }

    #[test]
    fn blob_hex_decodes_canonical_and_hex_forms() {
        let raw: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
        let want = "deadbeef";
        // (a) array-of-ints (canonical D1 wire form).
        let mut row = D1Row::new();
        row.insert(
            "p".into(),
            Value::Array(raw.iter().map(|b| Value::from(u64::from(*b))).collect()),
        );
        assert_eq!(col_blob_hex(&row, "p").as_deref(), Some(want));
        // (b) already-hex string (case-insensitive).
        let mut row = D1Row::new();
        row.insert("p".into(), Value::String("DEADBEEF".into()));
        assert_eq!(col_blob_hex(&row, "p").as_deref(), Some(want));
        // (c) unrecognised form / absent → None (callers fail CLOSED).
        let mut row = D1Row::new();
        row.insert("p".into(), Value::String("not hex!".into()));
        assert_eq!(col_blob_hex(&row, "p"), None);
        assert_eq!(col_blob_hex(&D1Row::new(), "p"), None);
    }
}

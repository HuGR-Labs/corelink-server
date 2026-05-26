//! Helper parsers + utilities for the `/v1/audit/export` route:
//! timestamp parsing, constant-time UUID compare, NDJSON serialization,
//! and the legacy window-derived `now_ms` fallback.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Verbatim move of `parse_timestamp`, `parse_rfc3339_utc_ms`,
//! `parse_u32_digits`, `uuid_eq_ct`, `serialize_ndjson_lines`, and
//! `now_ms_from_window`.

#![forbid(unsafe_code)]

use corelink_audit_chain::{ExportWindow, ExportedAuditEvent};
use subtle::ConstantTimeEq;
use uuid::Uuid;

/// Constant-time compare on two UUIDs (defense in depth — the
/// tenant comparator is auth-sensitive).
#[must_use]
pub(super) fn uuid_eq_ct(a: &Uuid, b: &Uuid) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Parse a timestamp shape: raw Unix epoch ms OR a minimal RFC 3339
/// subset (`YYYY-MM-DDTHH:MM:SSZ`). Returns `None` on malformed input.
pub(super) fn parse_timestamp(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<u64>() {
        return Some(v);
    }
    // Minimal RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`); 20 ASCII bytes.
    parse_rfc3339_utc_ms(s)
}

/// Pure-logic parse of `YYYY-MM-DDTHH:MM:SSZ` to Unix epoch ms.
/// Returns `None` on malformed shape. Covers years 1970..=9999.
///
/// This is a deliberate minimal-RFC3339 subset (per WI-S09-008 §4):
/// - **No** fractional-second support (`.NNN` rejected).
/// - **No** timezone offset support beyond literal `Z` (UTC only).
/// - **No** leap-second handling (`23:59:60` rejected).
///
/// The minimal shape is load-bearing: the audit-export window parameter is a
/// security-sensitive boundary input, and a smaller grammar means a smaller
/// adversarial surface. The full RFC3339 surface (offsets, fractional seconds,
/// `+00:00` vs `Z`) is intentionally out of scope here — a customer with a
/// non-UTC timestamp converts at the call site, NOT inside the audit-emit
/// hot path. Reviewed wave-20 (A-P2-03 closure).
fn parse_rfc3339_utc_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20 {
        return None;
    }
    // Shape: YYYY-MM-DDTHH:MM:SSZ
    //        0123456789012345678901
    let y = parse_u32_digits(b.get(0..4)?)?;
    if *b.get(4)? != b'-' { return None; }
    let mo = parse_u32_digits(b.get(5..7)?)?;
    if *b.get(7)? != b'-' { return None; }
    let d = parse_u32_digits(b.get(8..10)?)?;
    if *b.get(10)? != b'T' { return None; }
    let h = parse_u32_digits(b.get(11..13)?)?;
    if *b.get(13)? != b':' { return None; }
    let mi = parse_u32_digits(b.get(14..16)?)?;
    if *b.get(16)? != b':' { return None; }
    let se = parse_u32_digits(b.get(17..19)?)?;
    if *b.get(19)? != b'Z' { return None; }
    if !(1970..=9999).contains(&y) { return None; }
    if !(1..=12).contains(&mo) { return None; }
    if !(1..=31).contains(&d) { return None; }
    if h >= 24 || mi >= 60 || se >= 60 { return None; }

    // Days from Unix epoch (1970-01-01) using the canonical
    // proleptic Gregorian formula. Reference: Howard Hinnant's
    // `days_from_civil` (date_civil_from_days_inverse).
    let y_i: i64 = i64::from(y);
    let mo_i: i64 = i64::from(mo);
    let d_i: i64 = i64::from(d);
    let yy = if mo_i <= 2 { y_i - 1 } else { y_i };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400; // 0..=399
    let doy = (153_i64
        .checked_mul(if mo_i > 2 { mo_i - 3 } else { mo_i + 9 })?
        .checked_add(2)?)
        / 5
        + d_i
        - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch: i64 = era.checked_mul(146_097)?.checked_add(doe)?.checked_sub(719_468)?;
    if days_since_epoch < 0 { return None; }
    let secs: i64 = days_since_epoch
        .checked_mul(86_400)?
        .checked_add(i64::from(h) * 3_600 + i64::from(mi) * 60 + i64::from(se))?;
    if secs < 0 { return None; }
    let ms = u64::try_from(secs).ok()?.checked_mul(1_000)?;
    Some(ms)
}

fn parse_u32_digits(b: &[u8]) -> Option<u32> {
    let mut out: u32 = 0;
    for &c in b {
        if !c.is_ascii_digit() { return None; }
        out = out.checked_mul(10)?.checked_add(u32::from(c - b'0'))?;
    }
    Some(out)
}

/// Window-derived `now_ms` fallback for the rate-limit gate.
///
/// **Wave-21 closure (`A-P2-05`):** the route now consumes
/// [`super::state::AuditExportRouteState::wall_clock`]
/// (`Arc<dyn WallClock>`) and uses `wall_clock.now_ms()` as the
/// canonical bucket clock. This helper was retained at wave-21 as a
/// last-resort fallback for the degenerate case where the wall clock
/// saturates to `0`.
///
/// **Wave-23 closure (`W21-R-P2-01`):** the saturating-fallback path
/// is now fail-CLOSED (503 + `clock_unavailable` audit row at the
/// callsite). This helper is therefore unused on the production hot
/// path and is retained for archival reference only — no caller
/// remains. A future cleanup may delete it once external callers (none
/// exist today) confirm. Marked `#[allow(dead_code)]` so the rest of
/// the crate keeps clippy-clean; deletion is a separate cosmetic step.
#[allow(dead_code, reason = "wave-23: superseded by fail-CLOSED branch at callsite; retained for archival reference until next hygiene sweep")]
#[must_use]
pub(super) fn now_ms_from_window(window: ExportWindow) -> u64 {
    window.until_ms
}

/// Serialize each row to its canonical NDJSON envelope line (no
/// trailing newline; the streaming layer appends `\n` between rows).
/// Envelope shape: `{"event": <ev>, "proof": <p>}` per WI-S09-008 §4.
pub(super) fn serialize_ndjson_lines(
    rows: &[ExportedAuditEvent],
) -> Result<Vec<String>, serde_json::Error> {
    let mut out: Vec<String> = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(serde_json::to_string(row)?);
    }
    Ok(out)
}

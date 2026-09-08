//! Shared helper for the `origin_timing` test modules.
//!
//! `parse` turns a `Server-Timing` value into a canonical name-to-ms map. It lives here
//! rather than inside one of the test files because three test modules need it,
//! and importing it from a sibling would make that sibling look load-bearing
//! for the others when it is only a neighbour.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::collections::HashMap;

/// Parse a `Server-Timing` value into `{ canonical_name: dur_ms }`.
///
/// During the mixed rollout, the container emits `ohandler` plus an identical
/// legacy `oother` alias. Keep only the canonical value so partition assertions
/// cannot count that compatibility alias twice.
pub(super) fn parse(value: &str) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for part in value.split(',') {
        let mut it = part.trim().split(";dur=");
        if let (Some(name), Some(dur)) = (it.next(), it.next()) {
            let duration = dur.split(';').next().unwrap_or("").trim();
            if let Ok(v) = duration.parse::<i64>() {
                let canonical = match name.trim() {
                    "oother" => "ohandler",
                    other => other,
                };
                out.insert(canonical.to_owned(), v);
            }
        }
    }
    out
}

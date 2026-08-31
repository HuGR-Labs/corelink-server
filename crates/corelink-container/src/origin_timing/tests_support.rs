//! Shared helper for the `origin_timing` test modules.
//!
//! `parse` turns a `Server-Timing` value into a name-to-ms map. It lives here
//! rather than inside one of the test files because three of the four need it,
//! and importing it from a sibling would make that sibling look load-bearing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::collections::HashMap;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

use super::{current_ledger, origin_timing_layer, timed, Phase, PhaseLedger, PhaseScope, LEDGER};

/// Parse a `Server-Timing` value into `{ name: dur_ms }`.
pub(super) fn parse(value: &str) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for part in value.split(',') {
        let mut it = part.trim().split(";dur=");
        if let (Some(name), Some(dur)) = (it.next(), it.next()) {
            if let Ok(v) = dur.trim().parse::<i64>() {
                out.insert(name.trim().to_owned(), v);
            }
        }
    }
    out
}

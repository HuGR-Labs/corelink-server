//! Action-Cache (AC) journeys — `/v1/ac/{tenant}/{action_digest}`.
//!
//! STUB: the fleet fills this. Suggested journeys: AC update→lookup round-trip,
//! AC list enumeration (D-8), cross-tenant AC isolation, read-only PAT cannot
//! update an AC entry. Use only [`crate::harness`] URL builders + personas.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the AC journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub("AC: round-trip + isolation", "Action-Cache")]
}

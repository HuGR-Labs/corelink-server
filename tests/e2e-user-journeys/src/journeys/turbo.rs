//! Turborepo remote-cache journeys — `/v8/artifacts/*`.
//!
//! STUB: the fleet fills this. Suggested journeys: artifact PUT→GET round-trip
//! (tenant via `?teamId=`/`?slug=`), `status` handshake, `events` ingest,
//! cross-tenant artifact isolation. Use only [`crate::harness`] URL builders.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the Turborepo journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "Turbo: artifact round-trip + status",
        "Turborepo",
    )]
}

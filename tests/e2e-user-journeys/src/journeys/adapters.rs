//! Package-ecosystem adapter journeys — cargo (sccache) / npm / pip / brew / OCI.
//!
//! STUB: the fleet fills this. Suggested journeys: cargo sccache key
//! PUT→GET→HEAD; npm tarball cache-fill on miss; pip simple-index; brew bottle
//! proxy; OCI two-leg auth (`/token` then `/v2/...`) + blob push/pull.
//! Use only [`crate::harness`] URL builders (`url_cargo`/`url_npm`/`url_pip`/
//! `url_brew`/`url_oci_token`/`url_oci_v2`).

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the adapter journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "Adapters: cargo/npm/pip/brew/OCI",
        "package-ecosystem adapter",
    )]
}

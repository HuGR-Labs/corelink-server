//! Customer-dashboard journeys — `/v1/customer/{overview,usage,billing,...}`.
//!
//! STUB: the fleet fills this. Suggested journeys: overview/usage/billing read
//! shape; cross-tenant cannot read another tenant's overview; read-only PAT
//! access. Use only [`crate::harness::url_customer`] + personas.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the dashboard journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "Dashboard: overview/usage/billing",
        "customer-dashboard",
    )]
}

//! PAT-lifecycle journeys — `/v1/customer/keys` create / list / revoke.
//!
//! STUB: the fleet fills this. Suggested journeys: create scoped PAT → use it →
//! revoke it → confirm the revoked PAT is denied (read-only-PAT-revoke
//! priv-esc regression, rt-nuclear #7); list keys shape. Use only
//! [`crate::harness::url_customer`] + personas.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the PAT-lifecycle journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "PAT lifecycle: create/use/revoke",
        "PAT-lifecycle",
    )]
}

//! Billing journeys — tier-select checkout + billing portal + past-due gating.
//!
//! STUB: the fleet fills this. Suggested journeys: `/v1/onboarding/tier-select`
//! checkout (Clerk session, not PAT); `/v1/customer/billing/portal`; past-due
//! subscription is denied paid-tier capacity (billing-state integrity — paid
//! tier must NOT be served without `subscription_state='active'`). Use only
//! [`crate::harness::url_tier_select`] / `url_customer` + personas.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the billing journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "Billing: checkout/portal/past-due gate",
        "billing",
    )]
}

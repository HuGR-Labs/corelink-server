//! DSR / GDPR erasure journeys (data-subject-request, account deletion).
//!
//! STUB: the fleet fills this. The erasure transports are container-internal
//! (`/_internal/dsr/*`, `/_internal/cas/{tenant}/{hash}/erase`) and NOT
//! reachable with a plain customer PAT from the public edge — a black-box DSR
//! journey verifies the CUSTOMER-OBSERVABLE contract (e.g. after deletion, the
//! erased content address returns a deny; the customer-facing request path
//! behaves per the published DSR flow). Gate where no public surface exists.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the DSR journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "DSR: erasure + account deletion",
        "DSR/GDPR-erasure",
    )]
}

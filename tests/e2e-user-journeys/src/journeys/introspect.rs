//! PAT-introspection journeys — `POST /internal/v1/auth/introspect`.
//!
//! STUB: the fleet fills this. NOTE (flagged in the CARD): this route is a
//! CONTAINER-INTERNAL endpoint gated by `X-Corelink-Internal-Auth`; it is NOT
//! reachable with a plain customer PAT from the public edge. A black-box
//! journey here must therefore EITHER be supplied the internal-auth key via env
//! (and the route exposed to the test plane) OR it must assert that an
//! UNAUTHENTICATED introspect attempt from the edge is denied — gate otherwise.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the introspection journeys.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::stub(
        "Introspect: PAT → tenant+plan",
        "auth-introspection",
    )]
}

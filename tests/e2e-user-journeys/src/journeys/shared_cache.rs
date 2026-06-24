//! # Shared / `_public` cross-team cache journeys (gap-map M2 — THE MOAT).
//!
//! STUB (F-B scaffold). Owner WP: **W1**. This module must POSITIVELY prove the
//! product's network-effect thesis: a deterministic PUBLIC artifact written /
//! warmed by tenant A is served to tenant B as a **cache HIT** (same bytes, no
//! re-upload), while PRIVATE bytes stay isolated. Today the suite only proves the
//! NEGATIVE (B can't read A's private; `_public` can't be poisoned) — the
//! positive HIT that the COGS/margin story rests on is untested.
//!
//! Contract available to fill against (do NOT add deps; black-box only):
//!   - `cfg.token(TokenKind::ReadWrite)` = tenant A, `TokenKind::TenantB` = tenant B.
//!   - `cfg.public_hash` (`CORELINK_E2E_PUBLIC_HASH`) = a known public artifact.
//!   - URL builders in `harness` (`url_cas`, the adapter public-fetch builders).
//!   - Assert real byte-equality (A's bytes == B's bytes) and distinguish a true
//!     HIT from a cache-fill — a status code alone is NOT sufficient (see M2).

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the shared-cache cross-team journeys. STUB → one gated row until W1 fills it.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::gated(
        "Shared cache: cross-team `_public` dedup HIT (A→B) — M2",
        "TODO(W1): prove tenant B HITs tenant A's public bytes (byte-equal, true HIT)",
    )]
}

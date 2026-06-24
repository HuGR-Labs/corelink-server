//! # Team-invite / multi-seat lifecycle journeys (gap-map M13).
//!
//! STUB (F-B scaffold). Owner WP: **W8**. SMB teams are the target buyer;
//! "invite a teammate" is table-stakes onboarding and is completely untested.
//! Must drive `POST /v1/customer/team/invite`, a second-seat-under-one-tenant
//! flow, and seat removal — asserting the invited seat actually gains/loses
//! scoped access.
//!
//! Contract: `cfg.token(TokenKind::Admin)` (the owner/admin PAT that can invite)
//! + `cfg.team_invite_email` (`CORELINK_E2E_TEAM_INVITE_EMAIL`).

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the team-invite / multi-seat journeys. STUB → one gated row until W8 fills it.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::gated(
        "Team: invite teammate → second seat gains scoped access → removal revokes — M13",
        "TODO(W8): drive POST /v1/customer/team/invite + multi-seat access + seat removal",
    )]
}

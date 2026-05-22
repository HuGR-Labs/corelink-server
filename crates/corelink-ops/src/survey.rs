//! NPS / CSAT / free-text / multi-choice survey infra — wave-33
//! canonical surface.
//!
//! Re-exports the entire public API of `corelink-survey` (HMAC-SHA256
//! signed one-shot invite tokens + TTL + audit-fail-CLOSED record
//! path + in-memory fake; companion to RB-SURVEY-ABUSE +
//! `migrations/d1/0046_survey_responses.sql`). The actual
//! implementation lives in `crates/corelink-survey/` (Stage 1 Stream C
//! sub-step C.2 Option-A aggregator pattern).

pub use corelink_survey::*;

//! Survey-kind enum + response payload variants + validation helpers.

use serde::{Deserialize, Serialize};

use super::error::SurveyError;

/// Maximum free-text length in **bytes** (UTF-8). Chosen at 2 KiB —
/// large enough for a meaningful paragraph, small enough that an abuse
/// flood can't bloat the response table. Property test asserts the cap
/// is enforced across UTF-8 multibyte boundaries without panicking.
pub const FREE_TEXT_MAX_LEN: usize = 2 * 1024;

/// Maximum number of selected options in a [`SurveyResponse::MultiChoice`]
/// response. 16 covers every survey shape the customer-health team uses
/// (typical: 3-6 options); the bound keeps the persisted JSON blob small.
pub const MULTI_CHOICE_MAX_OPTIONS: usize = 16;

/// Maximum per-option index value. Surveys with > 256 options are
/// architectural anti-pattern; reject at the type boundary.
pub const MULTI_CHOICE_MAX_OPTION_INDEX: u8 = u8::MAX;

/// Survey kind enum — must match the variant carried by the token.
///
/// Stable string serialization is used in the token payload + the
/// `response_kind` column of the `survey_responses` D1 table so the
/// audit + analytics pipelines can pivot without re-encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SurveyKind {
    /// 0..=10 promoter / passive / detractor score.
    Nps,
    /// 1..=5 customer satisfaction score.
    Csat,
    /// Sanitized + length-capped UTF-8 free text.
    FreeText,
    /// Bounded option set (≤ 16 option indices, each ≤ 255).
    MultiChoice,
}

impl SurveyKind {
    /// Stable lowercase snake_case name for metric labels + audit logs.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Nps => "nps",
            Self::Csat => "csat",
            Self::FreeText => "free_text",
            Self::MultiChoice => "multi_choice",
        }
    }

    /// Whether this `kind` matches the variant carried by `response`.
    #[must_use]
    pub const fn matches(&self, response: &SurveyResponse) -> bool {
        matches!(
            (self, response),
            (Self::Nps, SurveyResponse::Nps { .. })
                | (Self::Csat, SurveyResponse::Csat { .. })
                | (Self::FreeText, SurveyResponse::FreeText { .. })
                | (Self::MultiChoice, SurveyResponse::MultiChoice { .. })
        )
    }
}

/// Survey response payload — one variant per [`SurveyKind`].
///
/// All variants are immutable + safe-to-clone; the wire format uses
/// `tag = "kind"` discriminator that matches [`SurveyKind::as_str`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SurveyResponse {
    /// 0..=10 NPS score.
    Nps {
        /// Score, validated to 0..=10 at construction time.
        score: u8,
    },
    /// 1..=5 CSAT score.
    Csat {
        /// Score, validated to 1..=5 at construction time.
        score: u8,
    },
    /// Sanitized + length-capped UTF-8 text.
    FreeText {
        /// Text payload, validated to be:
        /// - UTF-8 (Rust string guarantees this);
        /// - ≤ [`FREE_TEXT_MAX_LEN`] bytes;
        /// - no control chars except `\n` / `\t`;
        /// - leading + trailing whitespace trimmed.
        text: String,
    },
    /// Multi-choice selection.
    MultiChoice {
        /// Selected option indices. ≤ [`MULTI_CHOICE_MAX_OPTIONS`] entries.
        /// Duplicates rejected; order preserved as caller submitted.
        selected: Vec<u8>,
    },
}

impl SurveyResponse {
    /// Construct + validate an NPS response.
    ///
    /// # Errors
    ///
    /// Returns [`SurveyError::InvalidResponse`] if `score` is not in 0..=10.
    pub fn nps(score: u8) -> Result<Self, SurveyError> {
        validate_nps(score)?;
        Ok(Self::Nps { score })
    }

    /// Construct + validate a CSAT response.
    ///
    /// # Errors
    ///
    /// Returns [`SurveyError::InvalidResponse`] if `score` is not in 1..=5.
    pub fn csat(score: u8) -> Result<Self, SurveyError> {
        validate_csat(score)?;
        Ok(Self::Csat { score })
    }

    /// Construct + sanitize + validate a free-text response.
    ///
    /// # Errors
    ///
    /// Returns [`SurveyError::InvalidResponse`] on length-cap overflow
    /// AFTER sanitization, or on an empty post-trim payload.
    pub fn free_text(raw: &str) -> Result<Self, SurveyError> {
        let sanitized = sanitize_free_text(raw)?;
        Ok(Self::FreeText { text: sanitized })
    }

    /// Construct + validate a multi-choice response.
    ///
    /// # Errors
    ///
    /// Returns [`SurveyError::InvalidResponse`] on empty selection,
    /// > 16 options, or duplicate option indices.
    pub fn multi_choice(selected: Vec<u8>) -> Result<Self, SurveyError> {
        validate_multi_choice(&selected)?;
        Ok(Self::MultiChoice { selected })
    }

    /// Return the [`SurveyKind`] corresponding to this response variant.
    #[must_use]
    pub const fn kind(&self) -> SurveyKind {
        match self {
            Self::Nps { .. } => SurveyKind::Nps,
            Self::Csat { .. } => SurveyKind::Csat,
            Self::FreeText { .. } => SurveyKind::FreeText,
            Self::MultiChoice { .. } => SurveyKind::MultiChoice,
        }
    }
}

/// Validate an NPS score (0..=10).
///
/// # Errors
///
/// Returns [`SurveyError::InvalidResponse`] if `score > 10`.
pub fn validate_nps(score: u8) -> Result<(), SurveyError> {
    if score > 10 {
        Err(SurveyError::InvalidResponse("nps score out of range 0..=10"))
    } else {
        Ok(())
    }
}

/// Validate a CSAT score (1..=5).
///
/// # Errors
///
/// Returns [`SurveyError::InvalidResponse`] if `score` is outside 1..=5.
pub fn validate_csat(score: u8) -> Result<(), SurveyError> {
    if !(1..=5).contains(&score) {
        Err(SurveyError::InvalidResponse(
            "csat score out of range 1..=5",
        ))
    } else {
        Ok(())
    }
}

/// Validate a multi-choice selection set.
///
/// # Errors
///
/// Returns [`SurveyError::InvalidResponse`] on:
/// - empty selection,
/// - more than [`MULTI_CHOICE_MAX_OPTIONS`] entries,
/// - duplicate option indices.
pub fn validate_multi_choice(selected: &[u8]) -> Result<(), SurveyError> {
    if selected.is_empty() {
        return Err(SurveyError::InvalidResponse(
            "multi_choice selection empty",
        ));
    }
    if selected.len() > MULTI_CHOICE_MAX_OPTIONS {
        return Err(SurveyError::InvalidResponse(
            "multi_choice selection exceeds 16 options",
        ));
    }
    // O(n²) dup check is fine; `n ≤ 16` so the bound is tiny + we avoid
    // pulling a BTreeSet just for this.
    for (i, lhs) in selected.iter().enumerate() {
        for rhs in selected.iter().skip(i + 1) {
            if lhs == rhs {
                return Err(SurveyError::InvalidResponse(
                    "multi_choice selection has duplicates",
                ));
            }
        }
    }
    Ok(())
}

/// Sanitize a free-text response: trim, reject control chars (except
/// `\n` / `\t`), enforce the byte-length cap.
///
/// # Errors
///
/// Returns [`SurveyError::InvalidResponse`] if the trimmed text is
/// empty, exceeds [`FREE_TEXT_MAX_LEN`] bytes, or contains a disallowed
/// control character.
pub fn sanitize_free_text(raw: &str) -> Result<String, SurveyError> {
    // Trim leading + trailing whitespace per the same `str::trim`
    // semantics applied by all CoreLink form-input layers.
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SurveyError::InvalidResponse(
            "free_text empty after trim",
        ));
    }
    if trimmed.len() > FREE_TEXT_MAX_LEN {
        return Err(SurveyError::InvalidResponse(
            "free_text exceeds 2048-byte cap",
        ));
    }
    for ch in trimmed.chars() {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            return Err(SurveyError::InvalidResponse(
                "free_text contains disallowed control char",
            ));
        }
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test module — assertions panic by design"
)]
mod tests {
    use super::*;

    #[test]
    fn nps_score_range_enforced() {
        assert!(SurveyResponse::nps(0).is_ok());
        assert!(SurveyResponse::nps(10).is_ok());
        assert!(SurveyResponse::nps(11).is_err());
        assert!(SurveyResponse::nps(255).is_err());
    }

    #[test]
    fn csat_score_range_enforced() {
        assert!(SurveyResponse::csat(1).is_ok());
        assert!(SurveyResponse::csat(5).is_ok());
        assert!(SurveyResponse::csat(0).is_err());
        assert!(SurveyResponse::csat(6).is_err());
    }

    #[test]
    fn free_text_sanitized() {
        let r = SurveyResponse::free_text("  hello world  ").expect("ok");
        match r {
            SurveyResponse::FreeText { text } => assert_eq!(text, "hello world"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn free_text_rejects_control_chars() {
        assert!(SurveyResponse::free_text("hi\x07there").is_err()); // BEL
        assert!(SurveyResponse::free_text("hi\nthere").is_ok()); // newline allowed
        assert!(SurveyResponse::free_text("hi\tthere").is_ok()); // tab allowed
    }

    #[test]
    fn free_text_length_cap() {
        let oversized = "a".repeat(FREE_TEXT_MAX_LEN + 1);
        assert!(SurveyResponse::free_text(&oversized).is_err());
        let just_right = "a".repeat(FREE_TEXT_MAX_LEN);
        assert!(SurveyResponse::free_text(&just_right).is_ok());
    }

    #[test]
    fn free_text_empty_rejected() {
        assert!(SurveyResponse::free_text("").is_err());
        assert!(SurveyResponse::free_text("   ").is_err());
    }

    #[test]
    fn multi_choice_dedup() {
        assert!(SurveyResponse::multi_choice(vec![1, 2, 1]).is_err());
        assert!(SurveyResponse::multi_choice(vec![1, 2, 3]).is_ok());
    }

    #[test]
    fn multi_choice_bounds() {
        assert!(SurveyResponse::multi_choice(vec![]).is_err());
        let too_many = (0u8..17).collect();
        assert!(SurveyResponse::multi_choice(too_many).is_err());
    }

    #[test]
    fn kind_matches_variant() {
        assert!(SurveyKind::Nps.matches(&SurveyResponse::Nps { score: 0 }));
        assert!(!SurveyKind::Nps.matches(&SurveyResponse::Csat { score: 1 }));
    }
}

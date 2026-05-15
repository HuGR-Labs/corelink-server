//! Example: mint an NPS invite token + record a response against it.
//!
//! Run with: `cargo run -p corelink-survey --example sign_and_record`

use corelink_survey::{
    InMemoryFake, RecipientHash, SigningKey, SurveyError, SurveyId, SurveyKind, SurveyLinkSigner,
    SurveyResponse, SurveyResponseRecorder, TenantId,
};
use uuid::Uuid;

fn main() -> Result<(), SurveyError> {
    let key = SigningKey::from_bytes([0x42; 32]);
    let fake = InMemoryFake::new(key);

    let tenant = TenantId::from_uuid(Uuid::nil());
    let recipient = RecipientHash::derive_with_salt(
        "user@example.com",
        b"survey-recipient-salt-v1",
    );
    let survey_id = SurveyId::new("nps-w1-2026q2");

    let token = fake.sign_invite(
        tenant,
        recipient,
        survey_id,
        SurveyKind::Nps,
        1_700_000_000_000,
        60 * 60 * 24 * 1_000, // 24-hour TTL
    )?;
    let _ = token.as_str(); // would be embedded in the survey email URL

    fake.record(
        token.as_str(),
        SurveyResponse::nps(9)?,
        1_700_000_010_000,
        [0u8; 32],
        [0u8; 32],
    )?;

    let rows = fake.snapshot();
    let audit = fake.audit_snapshot();
    assert_eq!(rows.len(), 1);
    assert_eq!(audit.len(), 1);
    Ok(())
}

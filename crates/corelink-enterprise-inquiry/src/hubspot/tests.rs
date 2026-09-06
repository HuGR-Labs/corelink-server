use super::*;
use crate::encryption::{seal_inquiry, InMemoryInquiryPayloadEncryptor};
use crate::form::{BYOKRequirementsKind, EnterpriseInquiryForm, ResidencyKind, Role};

fn token() -> HubSpotToken {
    HubSpotToken::new("pat-na1-AAAAAAAAAAAAAAAA-deadbeef").unwrap()
}

fn eu_form() -> EnterpriseInquiryForm {
    EnterpriseInquiryForm {
        company: "Acme GmbH".to_string(),
        role: Role::Ciso,
        email: "ciso@acme.example".to_string(),
        phone_optional: None,
        expected_gb_per_month: 5_000,
        byok_requirements: BYOKRequirementsKind::AwsKms,
        residency_requirements: ResidencyKind::Eu,
        additional_notes: None,
        locale: "en-DE".to_string(),
        use_case: "EU cache".to_string(),
    }
}

fn us_form() -> EnterpriseInquiryForm {
    let mut f = eu_form();
    f.residency_requirements = ResidencyKind::Us;
    f.company = "Acme Inc".to_string();
    f
}

fn client_us(http: RecordingHubSpotHttp) -> HubSpotCrmClient<RecordingHubSpotHttp, NoopSleeper> {
    HubSpotCrmClient::new(http, NoopSleeper::new(), token(), HubSpotRegion::Us1).unwrap()
}

fn client_eu(http: RecordingHubSpotHttp) -> HubSpotCrmClient<RecordingHubSpotHttp, NoopSleeper> {
    HubSpotCrmClient::new(http, NoopSleeper::new(), token(), HubSpotRegion::Eu1).unwrap()
}

// -------- T1 token validation --------
#[test]
fn token_shape_validation_rejects_malformed_token() {
    assert!(matches!(
        HubSpotToken::new("not-a-token"),
        Err(HubSpotConfigError::InvalidTokenShape)
    ));
    assert!(matches!(
        HubSpotToken::new("pat-na1-short"),
        Err(HubSpotConfigError::InvalidTokenShape)
    ));
    let t = HubSpotToken::new("pat-na1-AAAAAAAAAAAAAAAA-deadbeef").unwrap();
    assert_eq!(t.bearer(), "Bearer pat-na1-AAAAAAAAAAAAAAAA-deadbeef");
    // Debug must redact.
    assert_eq!(format!("{:?}", t), "HubSpotToken(<redacted>)");
}

// -------- T2 region routing --------
#[test]
fn region_base_url_routes_us_vs_eu() {
    assert_eq!(HubSpotRegion::Us1.base_url(), "https://api.hubapi.com");
    assert_eq!(HubSpotRegion::Eu1.base_url(), "https://api.hubapi.eu");
}

// -------- T3 residency mismatch rejection --------
#[test]
fn eu_inquiry_against_us_region_rejected_pre_flight() {
    let http = RecordingHubSpotHttp::new();
    let c = client_us(http.clone());
    let enc = InMemoryInquiryPayloadEncryptor::new();
    let sealed = seal_inquiry(&eu_form(), "tenant-1", b"k", &enc).unwrap();
    let err = c
        .create_entry(
            &InquiryId::new("inq-eu-on-us"),
            &sealed,
            "tenant-1",
            &enc,
            100,
        )
        .unwrap_err();
    assert!(matches!(err, CrmError::Rejected(ref m) if m.contains("residency-mismatch")));
    // No wire request was issued.
    assert_eq!(http.request_count(), 0);
}

// -------- T4 us inquiry against eu rejected --------
#[test]
fn us_inquiry_against_eu_region_rejected_pre_flight() {
    let http = RecordingHubSpotHttp::new();
    let c = client_eu(http.clone());
    let enc = InMemoryInquiryPayloadEncryptor::new();
    let sealed = seal_inquiry(&us_form(), "tenant-1", b"k", &enc).unwrap();
    let err = c
        .create_entry(
            &InquiryId::new("inq-us-on-eu"),
            &sealed,
            "tenant-1",
            &enc,
            50,
        )
        .unwrap_err();
    assert!(matches!(err, CrmError::Rejected(ref m) if m.contains("residency-mismatch")));
    assert_eq!(http.request_count(), 0);
}

// -------- T5 contact create happy path --------
#[test]
fn create_contact_happy_path() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(
        201,
        "{\"id\":\"100001\",\"properties\":{\"email\":\"a@b.c\"}}",
    );
    let c = client_us(http.clone());
    let id = c
        .create_contact(
            "a@b.c",
            "Alice",
            "Acme",
            "Acme Inc",
            &InquiryId::new("inq-1"),
        )
        .unwrap();
    assert_eq!(id, "100001");
    let req = &http.requests()[0];
    assert_eq!(req.method, HubSpotMethod::Post);
    assert_eq!(req.url, "https://api.hubapi.com/crm/v3/objects/contacts");
    assert!(req.body.contains("\"email\":\"a@b.c\""));
    assert!(req
        .body
        .contains("\"hs_unique_creation_key\":\"contact:inq-1\""));
}

// -------- T6 company search-then-create idempotent (hit) --------
#[test]
fn find_or_create_company_returns_existing_when_search_hits() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(
        200,
        "{\"total\":1,\"results\":[{\"id\":\"900\",\"properties\":{\"name\":\"Acme Inc\"}}]}",
    );
    let c = client_us(http.clone());
    let id = c
        .find_or_create_company("Acme Inc", Some("US-12-345"), &InquiryId::new("inq-2"))
        .unwrap();
    assert_eq!(id, "900");
    // ONLY the search request fired — no create.
    assert_eq!(http.request_count(), 1);
    assert!(http.requests()[0]
        .url
        .ends_with("/crm/v3/objects/companies/search"));
}

// -------- T7 company search-then-create idempotent (miss → create) --------
#[test]
fn find_or_create_company_creates_when_search_misses() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(200, "{\"total\":0,\"results\":[]}");
    http.push_response(
        201,
        "{\"id\":\"901\",\"properties\":{\"name\":\"Acme Inc\"}}",
    );
    let c = client_us(http.clone());
    let id = c
        .find_or_create_company("Acme Inc", None, &InquiryId::new("inq-3"))
        .unwrap();
    assert_eq!(id, "901");
    let reqs = http.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].url.ends_with("/crm/v3/objects/companies/search"));
    assert!(reqs[1].url.ends_with("/crm/v3/objects/companies"));
    assert!(reqs[1]
        .body
        .contains("\"hs_unique_creation_key\":\"company:inq-3\""));
}

// -------- T8 deal create linked to contact + company --------
#[test]
fn create_deal_carries_associations_and_stage() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(201, "{\"id\":\"7777\"}");
    let c = client_us(http.clone());
    let id = c
        .create_deal(
            "C-1",
            "CO-2",
            DEAL_STAGE_ENTERPRISE_INQUIRY,
            250_000,
            &InquiryId::new("inq-4"),
        )
        .unwrap();
    assert_eq!(id, "7777");
    let body = &http.requests()[0].body;
    assert!(body.contains("\"dealstage\":\"enterprise_inquiry\""));
    assert!(body.contains("\"amount\":\"250000\""));
    assert!(body.contains("\"to\":{\"id\":\"C-1\"}"));
    assert!(body.contains("\"to\":{\"id\":\"CO-2\"}"));
    assert!(body.contains("\"hs_unique_creation_key\":\"deal:inq-4\""));
}

// -------- T9 create_entry end-to-end (3 calls) --------
#[test]
fn create_entry_orchestrates_contact_company_deal() {
    let http = RecordingHubSpotHttp::new();
    // contact
    http.push_response(201, "{\"id\":\"1001\"}");
    // company search miss
    http.push_response(200, "{\"total\":0,\"results\":[]}");
    // company create
    http.push_response(201, "{\"id\":\"2001\"}");
    // deal
    http.push_response(201, "{\"id\":\"3001\"}");
    let c = client_eu(http.clone());
    let enc = InMemoryInquiryPayloadEncryptor::new();
    let sealed = seal_inquiry(&eu_form(), "tenant-1", b"k", &enc).unwrap();
    let entry = c
        .create_entry(&InquiryId::new("inq-e2e"), &sealed, "tenant-1", &enc, 100)
        .unwrap();
    assert_eq!(entry.as_str(), "3001");
    assert_eq!(http.request_count(), 4);
    for r in http.requests() {
        assert!(r.url.starts_with("https://api.hubapi.eu"));
    }
    // Plaintext PII reached the HubSpot wire (this IS the
    // permitted decryption boundary).
    let bodies: Vec<String> = http.requests().iter().map(|r| r.body.clone()).collect();
    let joined = bodies.join("\n");
    assert!(joined.contains("ciso@acme.example"));
    assert!(joined.contains("Acme GmbH"));
}

// -------- T-r2-11 a: AAD tenant swap rejected at HubSpot boundary --------
#[test]
fn r2_11_hubspot_rejects_cross_tenant_push() {
    let http = RecordingHubSpotHttp::new();
    let c = client_eu(http.clone());
    let enc = InMemoryInquiryPayloadEncryptor::new();
    let sealed = seal_inquiry(&eu_form(), "tenant-A", b"k", &enc).unwrap();
    let err = c
        .create_entry(&InquiryId::new("inq-x"), &sealed, "tenant-B", &enc, 100)
        .unwrap_err();
    assert!(matches!(err, CrmError::Encryption(_)));
    // Zero wire requests issued — fail closed BEFORE wire egress.
    assert_eq!(http.request_count(), 0);
}

// -------- T10 retry on 5xx with backoff --------
#[test]
fn retry_on_5xx_succeeds_after_two_failures() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(503, "{\"status\":\"error\"}");
    http.push_response(502, "{\"status\":\"error\"}");
    http.push_response(201, "{\"id\":\"42\"}");
    let sleeper = NoopSleeper::new();
    let c =
        HubSpotCrmClient::new(http.clone(), sleeper.clone(), token(), HubSpotRegion::Us1).unwrap();
    let id = c
        .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-retry"))
        .unwrap();
    assert_eq!(id, "42");
    assert_eq!(http.request_count(), 3);
    // Sleeps recorded: 250ms then 500ms (exponential).
    let sleeps = sleeper.sleeps();
    assert_eq!(sleeps, vec![250, 500]);
}

// -------- T11 retry budget exhaustion --------
#[test]
fn retry_budget_exhaustion_after_five_5xx() {
    let http = RecordingHubSpotHttp::new();
    for _ in 0..6 {
        http.push_response(503, "x");
    }
    let c = client_us(http.clone());
    let err = c
        .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-x"))
        .unwrap_err();
    assert!(matches!(err, CrmError::Transport(ref m) if m.contains("retry budget exhausted")));
    // 5 retries + 1 initial = 6 attempts.
    assert_eq!(http.request_count(), 6);
}

// -------- T12 hard-fail on 401 (no retry) --------
#[test]
fn hard_fail_on_401_no_retry() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(401, "{\"category\":\"INVALID_AUTHENTICATION\"}");
    let c = client_us(http.clone());
    let err = c
        .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-401"))
        .unwrap_err();
    assert!(matches!(err, CrmError::Rejected(ref m) if m.contains("auth hard-fail")));
    // Token redaction MUST hold in the error.
    let CrmError::Rejected(m) = err else {
        panic!("expected Rejected variant");
    };
    assert!(m.contains("<redacted>"));
    // Single attempt.
    assert_eq!(http.request_count(), 1);
}

// -------- T13 retry-after honoured --------
#[test]
fn retry_after_header_honoured_and_capped() {
    let http = RecordingHubSpotHttp::new();
    http.push_response_with_retry_after(429, "{}", Some(2));
    http.push_response_with_retry_after(429, "{}", Some(60)); // capped to 30
    http.push_response(201, "{\"id\":\"99\"}");
    let sleeper = NoopSleeper::new();
    let c =
        HubSpotCrmClient::new(http.clone(), sleeper.clone(), token(), HubSpotRegion::Us1).unwrap();
    c.create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-429"))
        .unwrap();
    // Sleeps: 2_000 (server hint), 30_000 (capped from 60s).
    assert_eq!(sleeper.sleeps(), vec![2_000, 30_000]);
}

// -------- T14 4xx non-retryable hard-fail --------
#[test]
fn validation_4xx_is_not_retried() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(
        400,
        "{\"category\":\"VALIDATION_ERROR\",\"message\":\"bad email\"}",
    );
    let c = client_us(http.clone());
    let err = c
        .create_contact("not-an-email", "A", "B", "Co", &InquiryId::new("inq-400"))
        .unwrap_err();
    assert!(matches!(err, CrmError::Rejected(_)));
    assert_eq!(http.request_count(), 1);
}

// -------- T15 ticket create --------
#[test]
fn create_ticket_carries_subject_content_owner() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(201, "{\"id\":\"T-1\"}");
    let c = client_us(http.clone());
    let id = c
        .create_ticket(
            "Enterprise inquiry",
            "Please follow up",
            Some("OWNER-1"),
            &InquiryId::new("inq-tkt"),
        )
        .unwrap();
    assert_eq!(id, "T-1");
    let body = &http.requests()[0].body;
    assert!(body.contains("\"subject\":\"Enterprise inquiry\""));
    assert!(body.contains("\"hubspot_owner_id\":\"OWNER-1\""));
}

// -------- T16 compensate PATCHes the deal --------
#[test]
fn compensate_patches_deal_to_closedlost() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(
        200,
        "{\"id\":\"3001\",\"properties\":{\"dealstage\":\"closedlost\"}}",
    );
    let c = client_us(http.clone());
    c.compensate(&InquiryId::new("inq-cmp"), &CrmEntryId::new("3001"))
        .unwrap();
    let req = &http.requests()[0];
    assert_eq!(req.method, HubSpotMethod::Patch);
    assert!(req.url.ends_with("/crm/v3/objects/deals/3001"));
    assert!(req.body.contains("\"dealstage\":\"closedlost\""));
    assert!(req.body.contains("\"saga_rollback\":\"true\""));
}

// -------- T17 transport failure retried then succeeds --------
#[test]
fn transport_failure_retried_then_succeeds() {
    let http = RecordingHubSpotHttp::new();
    http.push_transport_failure("connection reset");
    http.push_response(201, "{\"id\":\"55\"}");
    let c = client_us(http.clone());
    let id = c
        .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-net"))
        .unwrap();
    assert_eq!(id, "55");
}

// -------- T18 from_env happy path --------
#[test]
fn from_env_reads_token_and_region() {
    let env = |k: &str| match k {
        "HUBSPOT_PRIVATE_APP_TOKEN" => Some("pat-na1-AAAAAAAAAAAAAAAA-deadbeef".to_string()),
        "HUBSPOT_REGION" => Some("eu1".to_string()),
        _ => None,
    };
    let c =
        HubSpotCrmClient::from_env(RecordingHubSpotHttp::new(), NoopSleeper::new(), env).unwrap();
    assert_eq!(c.region(), HubSpotRegion::Eu1);
}

// -------- T19 from_env missing var --------
#[test]
fn from_env_missing_var_errors() {
    let env = |_: &str| -> Option<String> { None };
    let err = HubSpotCrmClient::from_env(RecordingHubSpotHttp::new(), NoopSleeper::new(), env)
        .unwrap_err();
    assert!(matches!(err, HubSpotConfigError::EnvMissing(_)));
}

// -------- T20 classify_retry pure helper --------
#[test]
fn classify_retry_pure_helper() {
    let ok = HubSpotResponse {
        status: 200,
        body: String::new(),
        retry_after_s: None,
    };
    assert_eq!(classify_retry(&ok, 0), RetryDecision::Success);
    let auth = HubSpotResponse {
        status: 401,
        body: String::new(),
        retry_after_s: None,
    };
    assert_eq!(classify_retry(&auth, 0), RetryDecision::AuthHardFail);
    let nr = HubSpotResponse {
        status: 422,
        body: String::new(),
        retry_after_s: None,
    };
    assert_eq!(classify_retry(&nr, 0), RetryDecision::NonRetryable);
    let r5 = HubSpotResponse {
        status: 503,
        body: String::new(),
        retry_after_s: None,
    };
    assert_eq!(
        classify_retry(&r5, 0),
        RetryDecision::Retry { delay_ms: 250 }
    );
    assert_eq!(classify_retry(&r5, 5), RetryDecision::GiveUp);
}

// -------- T-extra: Authorization header injected --------
#[test]
fn authorization_header_injected_into_request() {
    let http = RecordingHubSpotHttp::new();
    http.push_response(201, "{\"id\":\"1\"}");
    let c = client_us(http.clone());
    c.create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-auth"))
        .unwrap();
    let req = &http.requests()[0];
    assert_eq!(
        req.authorization,
        "Bearer pat-na1-AAAAAAAAAAAAAAAA-deadbeef"
    );
    // Debug must redact the authorization field.
    let dbg = format!("{:?}", req);
    assert!(dbg.contains("authorization: \"<redacted>\""));
    assert!(!dbg.contains("pat-na1"));
}

// -------- T21 residency_routable matrix --------
#[test]
fn residency_routable_matrix() {
    assert!(is_residency_routable(ResidencyKind::Eu, HubSpotRegion::Eu1));
    assert!(!is_residency_routable(
        ResidencyKind::Eu,
        HubSpotRegion::Us1
    ));
    assert!(is_residency_routable(ResidencyKind::Us, HubSpotRegion::Us1));
    assert!(!is_residency_routable(
        ResidencyKind::Us,
        HubSpotRegion::Eu1
    ));
    // None / Sam / Apac / Specific currently permitted on either
    // (legal review out-of-band).
    assert!(is_residency_routable(
        ResidencyKind::None,
        HubSpotRegion::Us1
    ));
    assert!(is_residency_routable(
        ResidencyKind::Apac,
        HubSpotRegion::Eu1
    ));
}

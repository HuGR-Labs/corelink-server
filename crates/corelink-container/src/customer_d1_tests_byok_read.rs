// ── BYOK config read model (Wave 2) ────────────────────────────────────────

/// Hermetic async mock for the [`ByokConfigRows`] seam.
#[derive(Debug, Default)]
struct MockByokRows {
    rows: Vec<D1Row>,
    fail: Option<String>,
}

impl ByokConfigRows for MockByokRows {
    async fn query_rows(&self, _sql: &str, _binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        match &self.fail {
            Some(e) => Err(e.clone()),
            None => Ok(self.rows.clone()),
        }
    }
}

fn byok_config_row_fixture() -> D1Row {
    row(&[
        ("tenant_id", json!(TENANT)),
        ("mode", json!("byok")),
        ("crypto_mode", json!("convergent")),
        ("cmk_provider", json!("aws")),
        ("cmk_key_id", json!("arn:aws:kms:us-east-1:1:key/abc")),
        ("cmk_region", json!("us-east-1")),
        ("state", json!("active")),
    ])
}

#[test]
fn byok_mode_parse_round_trip_every_variant() {
    for m in [ByokMode::Managed, ByokMode::Byok, ByokMode::Hyok] {
        assert_eq!(m.as_str().parse::<ByokMode>().unwrap(), m);
    }
}

#[test]
fn byok_crypto_mode_parse_round_trip_every_variant() {
    for m in [ByokCryptoMode::Convergent, ByokCryptoMode::Random] {
        assert_eq!(m.as_str().parse::<ByokCryptoMode>().unwrap(), m);
    }
}

#[test]
fn byok_state_parse_round_trip_every_variant() {
    for s in [
        ByokState::Inactive,
        ByokState::Pending,
        ByokState::Active,
        ByokState::Partial,
        ByokState::Shredded,
    ] {
        assert_eq!(s.as_str().parse::<ByokState>().unwrap(), s);
    }
}

#[test]
fn byok_enums_fail_closed_on_unknown_string() {
    // Fail-CLOSED: an unknown value is an Err, NEVER a permissive default
    // (an unrecognised custody rung must not silently mean "encryption off").
    assert!(matches!(
        "rot13".parse::<ByokMode>(),
        Err(ByokConfigError::Parse(_))
    ));
    assert!(matches!(
        "".parse::<ByokMode>(),
        Err(ByokConfigError::Parse(_))
    ));
    assert!(matches!(
        "homomorphic".parse::<ByokCryptoMode>(),
        Err(ByokConfigError::Parse(_))
    ));
    assert!(matches!(
        "rotated".parse::<ByokState>(),
        Err(ByokConfigError::Parse(_))
    ));
    // Case sensitivity is enforced (the D1 CHECK is lowercase-only).
    assert!("BYOK".parse::<ByokMode>().is_err());
    assert!("Active".parse::<ByokState>().is_err());
}

#[test]
fn is_encryption_active_truth_table() {
    let cfg = |state| TenantByokConfig {
        tenant_id: TENANT.to_owned(),
        mode: ByokMode::Byok,
        crypto_mode: ByokCryptoMode::Convergent,
        cmk_provider: None,
        cmk_key_id: None,
        cmk_region: None,
        state,
    };
    // Active + Partial encrypt new writes → active.
    assert!(is_encryption_active(&cfg(ByokState::Active)));
    assert!(is_encryption_active(&cfg(ByokState::Partial)));
    // The rest do not.
    assert!(!is_encryption_active(&cfg(ByokState::Inactive)));
    assert!(!is_encryption_active(&cfg(ByokState::Pending)));
    assert!(!is_encryption_active(&cfg(ByokState::Shredded)));
}

#[tokio::test]
async fn get_byok_config_none_when_absent() {
    let reader = D1ByokConfigReader::new(Arc::new(MockByokRows::default()));
    let got = reader.get_byok_config(TENANT).await.unwrap();
    assert!(
        got.is_none(),
        "no row = BYOK not configured = today's behaviour"
    );
}

#[tokio::test]
async fn get_byok_config_parses_present_row() {
    let reader = D1ByokConfigReader::new(Arc::new(MockByokRows {
        rows: vec![byok_config_row_fixture()],
        fail: None,
    }));
    let cfg = reader.get_byok_config(TENANT).await.unwrap().unwrap();
    assert_eq!(cfg.tenant_id, TENANT);
    assert_eq!(cfg.mode, ByokMode::Byok);
    assert_eq!(cfg.crypto_mode, ByokCryptoMode::Convergent);
    assert_eq!(cfg.cmk_provider.as_deref(), Some("aws"));
    assert_eq!(cfg.state, ByokState::Active);
    assert!(is_encryption_active(&cfg));
}

#[tokio::test]
async fn get_byok_config_fail_closed_on_unparseable_enum() {
    // A corrupt/unknown `state` must be an Err, not a silent default.
    let mut bad = byok_config_row_fixture();
    bad.insert("state".to_owned(), json!("frobnicated"));
    let reader = D1ByokConfigReader::new(Arc::new(MockByokRows {
        rows: vec![bad],
        fail: None,
    }));
    assert!(matches!(
        reader.get_byok_config(TENANT).await,
        Err(ByokConfigError::Parse(_))
    ));
}

#[tokio::test]
async fn get_byok_config_fail_closed_on_transport_error() {
    let reader = D1ByokConfigReader::new(Arc::new(MockByokRows {
        rows: vec![],
        fail: Some("D1 HTTP 500: transport down".to_owned()),
    }));
    assert!(matches!(
        reader.get_byok_config(TENANT).await,
        Err(ByokConfigError::Transport(_))
    ));
}

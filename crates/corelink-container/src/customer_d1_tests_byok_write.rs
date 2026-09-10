// ── BYOK config WRITER (activation seam) ───────────────────────────────────

/// A STATEFUL hermetic mock of the two 0081 tables over the async
/// [`ByokConfigRows`] seam. It interprets the exact SQL the writer + readers
/// emit so a WRITE is observable by a later READ (proving read-after-write).
#[derive(Debug, Default)]
struct StatefulByokDb {
    config: Mutex<std::collections::HashMap<String, D1Row>>,
    secret: Mutex<std::collections::HashMap<String, D1Row>>,
    fail: Option<String>,
}

impl StatefulByokDb {
    fn tenant_of(binds: &[Value], idx: usize) -> String {
        binds
            .get(idx)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }
}

impl ByokConfigRows for StatefulByokDb {
    async fn query_rows(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        if let Some(e) = &self.fail {
            return Err(e.clone());
        }
        if sql.contains("INSERT INTO tenant_byok_secret") {
            let tenant = Self::tenant_of(&binds, 0);
            self.secret.lock().unwrap().insert(
                tenant.clone(),
                row(&[
                    ("tenant_id", json!(tenant)),
                    ("tcs_wrapped", binds[1].clone()),
                    ("cmk_key_id", binds[2].clone()),
                    ("tcs_version", json!(1)),
                ]),
            );
            return Ok(Vec::new());
        }
        if sql.contains("INSERT INTO tenant_byok_config") {
            let tenant = Self::tenant_of(&binds, 0);
            self.config.lock().unwrap().insert(
                tenant.clone(),
                row(&[
                    ("tenant_id", json!(tenant)),
                    ("mode", binds[1].clone()),
                    ("crypto_mode", binds[2].clone()),
                    ("cmk_provider", binds[3].clone()),
                    ("cmk_key_id", binds[4].clone()),
                    ("cmk_region", binds[5].clone()),
                    ("state", json!("active")),
                ]),
            );
            return Ok(Vec::new());
        }
        if sql.contains("UPDATE tenant_byok_config") {
            // deactivate: binds = [now_ms, tenant]
            let tenant = Self::tenant_of(&binds, 1);
            if let Some(r) = self.config.lock().unwrap().get_mut(&tenant) {
                r.insert("state".to_owned(), json!("shredded"));
            }
            return Ok(Vec::new());
        }
        if sql.contains("FROM tenant_byok_secret") {
            let tenant = Self::tenant_of(&binds, 0);
            return Ok(self
                .secret
                .lock()
                .unwrap()
                .get(&tenant)
                .cloned()
                .into_iter()
                .collect());
        }
        if sql.contains("FROM tenant_byok_config") {
            let tenant = Self::tenant_of(&binds, 0);
            return Ok(self
                .config
                .lock()
                .unwrap()
                .get(&tenant)
                .cloned()
                .into_iter()
                .collect());
        }
        Ok(Vec::new())
    }
}

const NOW_ACT_MS: i64 = 1_700_000_000_000;

fn activation_fixture() -> ByokActivation {
    ByokActivation {
        tenant_id: TENANT.to_owned(),
        mode: ByokMode::Byok,
        crypto_mode: ByokCryptoMode::Convergent,
        cmk_provider: "aws".to_owned(),
        cmk_key_id: "arn:aws:kms:us-east-1:1:key/abc".to_owned(),
        cmk_region: Some("us-east-1".to_owned()),
        tcs_wrapped: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03],
    }
}

/// Read-after-write: activate() flips the tenant to `active`, and a
/// subsequent read observes encryption ENGAGED (engagement_for → Encrypt)
/// with the wrapped Tcs round-tripping to the exact bytes.
#[tokio::test]
async fn activate_then_read_engages_encryption() {
    use crate::storage::byok_cas::{
        engagement_for, ByokEngagement, ByokSecretSource, D1ByokSecretReader,
    };

    let db = Arc::new(StatefulByokDb::default());
    let writer = D1ByokConfigWriter::new(db.clone());
    let reader = D1ByokConfigReader::new(db.clone());

    let act = activation_fixture();
    writer.activate(&act, NOW_ACT_MS).await.expect("activate");

    // Config read observes the active state → encryption engaged.
    let cfg = reader
        .get_byok_config(TENANT)
        .await
        .expect("read")
        .expect("row present after activation");
    assert_eq!(cfg.state, ByokState::Active);
    assert_eq!(cfg.mode, ByokMode::Byok);
    assert_eq!(cfg.crypto_mode, ByokCryptoMode::Convergent);
    assert_eq!(cfg.cmk_provider.as_deref(), Some("aws"));
    assert!(is_encryption_active(&cfg));
    assert!(
        matches!(
            engagement_for(&cfg),
            ByokEngagement::Encrypt(ByokCryptoMode::Convergent)
        ),
        "an active tenant must ENGAGE convergent encryption on the CAS path"
    );

    // The wrapped Tcs round-trips through the base64 BLOB encoding to the
    // EXACT bytes the caller supplied — so the Tcs is resolvable (a missing
    // Tcs under an active state would fail CLOSED on the read path).
    let secret_reader = D1ByokSecretReader::new(db.clone());
    let wrapped = secret_reader
        .get_wrapped_tcs(TENANT)
        .await
        .expect("secret read")
        .expect("wrapped Tcs present after activation");
    assert_eq!(wrapped.tcs_wrapped, act.tcs_wrapped);
    assert_eq!(
        wrapped.cmk_key_id.as_deref(),
        Some("arn:aws:kms:us-east-1:1:key/abc")
    );
}

/// Kill switch: deactivate() flips active → shredded, and a subsequent read
/// observes encryption DISENGAGED while storage access remains fail-CLOSED
/// (never downgraded to plaintext).
#[tokio::test]
async fn deactivate_kill_switch_disengages_encryption() {
    use crate::storage::byok_cas::{engagement_for, ByokEngagement};

    let db = Arc::new(StatefulByokDb::default());
    let writer = D1ByokConfigWriter::new(db.clone());
    let reader = D1ByokConfigReader::new(db.clone());

    writer
        .activate(&activation_fixture(), NOW_ACT_MS)
        .await
        .expect("activate");
    writer
        .deactivate(TENANT, NOW_ACT_MS + 1)
        .await
        .expect("deactivate");

    let cfg = reader
        .get_byok_config(TENANT)
        .await
        .expect("read")
        .expect("row still present");
    assert_eq!(cfg.state, ByokState::Shredded);
    assert!(!is_encryption_active(&cfg));
    assert!(matches!(
        engagement_for(&cfg),
        ByokEngagement::FailClosed(
            "BYOK tenant is crypto-shredded; storage access is permanently disabled"
        )
    ));

    // Idempotent: a second kill switch on an already-shredded tenant is Ok.
    writer
        .deactivate(TENANT, NOW_ACT_MS + 2)
        .await
        .expect("idempotent shred");
}

/// Re-activating a crypto-shredded tenant is refused (monotonic terminal).
#[tokio::test]
async fn activate_refuses_reactivation_of_shredded() {
    let db = Arc::new(StatefulByokDb::default());
    let writer = D1ByokConfigWriter::new(db.clone());
    writer
        .activate(&activation_fixture(), NOW_ACT_MS)
        .await
        .expect("activate");
    writer
        .deactivate(TENANT, NOW_ACT_MS + 1)
        .await
        .expect("shred");
    let err = writer
        .activate(&activation_fixture(), NOW_ACT_MS + 2)
        .await
        .expect_err("re-activation of a shredded tenant must fail");
    assert!(matches!(
        err,
        ByokWriteError::IllegalTransition {
            from: ByokState::Shredded,
            to: ByokState::Active
        }
    ));
}

/// Deactivating a tenant that was never active is refused fail-CLOSED.
#[tokio::test]
async fn deactivate_fresh_tenant_is_illegal() {
    let db = Arc::new(StatefulByokDb::default());
    let writer = D1ByokConfigWriter::new(db);
    let err = writer
        .deactivate(TENANT, NOW_ACT_MS)
        .await
        .expect_err("nothing to shred");
    assert!(matches!(err, ByokWriteError::IllegalTransition { .. }));
}

/// Validation fail-CLOSED: managed custody, unknown provider, empty CMK
/// identity, and an empty wrapped-Tcs are all rejected BEFORE any D1 write.
#[tokio::test]
async fn activate_validation_rejects_bad_params() {
    let db = Arc::new(StatefulByokDb::default());
    let writer = D1ByokConfigWriter::new(db.clone());

    let mut managed = activation_fixture();
    managed.mode = ByokMode::Managed;
    assert!(matches!(
        writer.activate(&managed, NOW_ACT_MS).await,
        Err(ByokWriteError::Invalid(_))
    ));

    let mut bad_provider = activation_fixture();
    bad_provider.cmk_provider = "corelink_managed".to_owned();
    assert!(matches!(
        writer.activate(&bad_provider, NOW_ACT_MS).await,
        Err(ByokWriteError::Invalid(_))
    ));

    let mut empty_key = activation_fixture();
    empty_key.cmk_key_id = "  ".to_owned();
    assert!(matches!(
        writer.activate(&empty_key, NOW_ACT_MS).await,
        Err(ByokWriteError::Invalid(_))
    ));

    let mut empty_tcs = activation_fixture();
    empty_tcs.tcs_wrapped = Vec::new();
    assert!(matches!(
        writer.activate(&empty_tcs, NOW_ACT_MS).await,
        Err(ByokWriteError::Invalid(_))
    ));

    // Nothing was persisted — a rejected activation is invisible to a read.
    assert!(db.config.lock().unwrap().is_empty());
    assert!(db.secret.lock().unwrap().is_empty());
}

/// A D1 transport failure surfaces as `Transport`, never a silent success.
#[tokio::test]
async fn activate_fails_closed_on_transport_error() {
    let db = Arc::new(StatefulByokDb {
        fail: Some("D1 HTTP 500: transport down".to_owned()),
        ..StatefulByokDb::default()
    });
    let writer = D1ByokConfigWriter::new(db);
    assert!(matches!(
        writer.activate(&activation_fixture(), NOW_ACT_MS).await,
        Err(ByokWriteError::Transport(_))
    ));
}

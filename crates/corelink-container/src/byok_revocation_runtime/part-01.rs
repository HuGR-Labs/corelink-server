#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingD1 {
        responses: Mutex<Vec<Vec<D1Row>>>,
        sql: Mutex<Vec<String>>,
    }

    impl RecordingD1 {
        fn with_responses(responses: Vec<Vec<D1Row>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                sql: Mutex::new(Vec::new()),
            }
        }

        fn row(values: &[(&str, &str)]) -> D1Row {
            values
                .iter()
                .map(|(key, value)| ((*key).to_owned(), Value::String((*value).to_owned())))
                .collect()
        }

        fn statements(&self) -> Vec<String> {
            self.sql.lock().expect("sql lock").clone()
        }
    }

    #[async_trait]
    impl RevocationD1Client for RecordingD1 {
        async fn query(&self, sql: &str, _params: &[Value]) -> Result<Vec<D1Row>, String> {
            self.sql.lock().expect("sql lock").push(sql.to_owned());
            Ok(self
                .responses
                .lock()
                .expect("responses lock")
                .pop()
                .unwrap_or_default())
        }
    }

    fn key_id() -> KmsKeyId {
        KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: "arn:aws:kms:test:key/b083".to_owned(),
            region: "us-east-1".to_owned(),
        }
    }

    #[tokio::test]
    async fn d1_population_preserves_provider_key_and_region() {
        let db = Arc::new(RecordingD1::with_responses(vec![vec![RecordingD1::row(
            &[
                ("cmk_provider", "aws"),
                ("cmk_key_id", "arn:aws:kms:test:key/b083"),
                ("cmk_region", "us-east-1"),
            ],
        )]]));
        let keys = D1ActiveByokKeySource::new(db.clone())
            .list_active_byok_keys(KmsProviderKind::AwsKms)
            .await
            .expect("valid active key row");
        assert_eq!(keys, vec![key_id()]);
        assert!(db.statements()[0].contains("state IN ('active', 'partial')"));
    }

    #[tokio::test]
    async fn d1_population_rejects_missing_key_or_region_and_unknown_provider() {
        for missing in ["cmk_key_id", "cmk_region"] {
            let row = if missing == "cmk_key_id" {
                RecordingD1::row(&[("cmk_provider", "aws"), ("cmk_region", "us-east-1")])
            } else {
                RecordingD1::row(&[("cmk_provider", "aws"), ("cmk_key_id", "k1")])
            };
            let db = Arc::new(RecordingD1::with_responses(vec![vec![row]]));
            assert!(D1ActiveByokKeySource::new(db)
                .list_active_byok_keys(KmsProviderKind::AwsKms)
                .await
                .is_err());
        }

        let db = Arc::new(RecordingD1::with_responses(vec![vec![RecordingD1::row(
            &[
                ("cmk_provider", "not-a-provider"),
                ("cmk_key_id", "k1"),
                ("cmk_region", "us-east-1"),
            ],
        )]]));
        assert!(D1ActiveByokKeySource::new(db)
            .list_active_byok_keys(KmsProviderKind::AwsKms)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn d1_population_rejects_returned_provider_mismatch() {
        let db = Arc::new(RecordingD1::with_responses(vec![vec![RecordingD1::row(
            &[
                // The request asked for AWS, but a buggy/malicious D1 adapter
                // returned a GCP row.  The adapter must not relabel it as AWS.
                ("cmk_provider", "gcp"),
                (
                    "cmk_key_id",
                    "projects/p/locations/l/keyRings/r/cryptoKeys/k",
                ),
                ("cmk_region", "us-central1"),
            ],
        )]]));
        let result = D1ActiveByokKeySource::new(db)
            .list_active_byok_keys(KmsProviderKind::AwsKms)
            .await;
        assert!(
            result.is_err(),
            "returned provider mismatch must fail closed"
        );
    }

    #[tokio::test]
    async fn d1_zero_update_and_missing_alert_recipient_fail_closed() {
        let db = Arc::new(RecordingD1::with_responses(vec![Vec::new()]));
        let store = D1TenantStatusStore::new(db);
        assert!(store.mark_degraded(&key_id(), "aws", 1).await.is_err());

        let db = Arc::new(RecordingD1::with_responses(vec![Vec::new()]));
        let alerter = D1RevocationAlerter::new(db);
        let payload = RevocationAlertPayload {
            provider: "aws".to_owned(),
            kms_key_id: key_id(),
            tenant_id_hashed: "hashed".to_owned(),
            detected_at_ms: 1,
            kill_switch_duration_ms: 1,
            recovery_instructions: "restore".to_owned(),
        };
        assert!(alerter.alert(payload).await.is_err());
    }

    #[test]
    fn status_and_customer_audit_share_one_database_transaction() {
        let sql =
            include_str!("../../../../migrations/d1/0114_byok_revocation_customer_audit_atomic.sql");
        let mut connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE tenant (
                    tenant_id TEXT PRIMARY KEY, byok_status TEXT NOT NULL,
                    byok_revoked_at_ms INTEGER, byok_revoked_provider TEXT,
                    byok_revoked_kms_key_id TEXT
                 );
                 CREATE TABLE tenant_byok_config (
                    tenant_id TEXT PRIMARY KEY, cmk_provider TEXT NOT NULL,
                    cmk_key_id TEXT NOT NULL
                 );
                 CREATE TABLE customer_audit_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, tenant_id TEXT NOT NULL,
                    event_type TEXT NOT NULL, actor TEXT, target TEXT,
                    ts_ms INTEGER NOT NULL, detail TEXT
                 );",
            )
            .expect("minimal D1 schema");
        connection
            .execute_batch(sql)
            .expect("0114 trigger installs");
        connection
            .execute_batch(
                "INSERT INTO tenant VALUES ('t-b083', 'active', NULL, NULL, NULL);
                 INSERT INTO tenant_byok_config VALUES ('t-b083', 'aws', 'arn:aws:kms:test:key/b083');",
            )
            .expect("fixture");

        // The exact SQL issued by D1TenantStatusStore is executed in SQLite.
        // Commit proves the trigger is in the same transaction as the update.
        {
            let transaction = connection.transaction().expect("begin degrade");
            let returned: Vec<String> = transaction
                .prepare(MARK_DEGRADED_SQL)
                .expect("prepare degrade")
                .query_map(
                    rusqlite::params![1_i64, "aws", "arn:aws:kms:test:key/b083"],
                    |row| row.get(0),
                )
                .expect("degrade RETURNING")
                .collect::<Result<_, _>>()
                .expect("degrade row");
            assert_eq!(returned, ["t-b083"]);
            transaction.commit().expect("commit degrade");
        }
        let status: String = connection
            .query_row("SELECT byok_status FROM tenant", [], |row| row.get(0))
            .expect("degraded status");
        assert_eq!(status, "degraded_read_only");
        let revoked_events: i64 = connection
            .query_row(
                "SELECT count(*) FROM customer_audit_events WHERE event_type = 'byok.cmk_revoked'",
                [],
                |row| row.get(0),
            )
            .expect("revoked audit");
        assert_eq!(revoked_events, 1);

        // A rollback must remove both the status transition and its trigger
        // row, never leaving a customer-visible half-transition.
        {
            let transaction = connection.transaction().expect("begin restore");
            let returned: Vec<String> = transaction
                .prepare(RESTORE_ACTIVE_SQL)
                .expect("prepare restore")
                .query_map(
                    rusqlite::params!["aws", "arn:aws:kms:test:key/b083"],
                    |row| row.get(0),
                )
                .expect("restore RETURNING")
                .collect::<Result<_, _>>()
                .expect("restore row");
            assert_eq!(returned, ["t-b083"]);
            transaction.rollback().expect("rollback restore");
        }
        let status_after_rollback: String = connection
            .query_row("SELECT byok_status FROM tenant", [], |row| row.get(0))
            .expect("status after rollback");
        assert_eq!(status_after_rollback, "degraded_read_only");
        let events_after_rollback: i64 = connection
            .query_row("SELECT count(*) FROM customer_audit_events", [], |row| {
                row.get(0)
            })
            .expect("events after rollback");
        assert_eq!(events_after_rollback, 1);

        // Finally commit the restore and observe its distinct trigger event.
        let returned: Vec<String> = connection
            .prepare(RESTORE_ACTIVE_SQL)
            .expect("prepare committed restore")
            .query_map(
                rusqlite::params!["aws", "arn:aws:kms:test:key/b083"],
                |row| row.get(0),
            )
            .expect("committed restore RETURNING")
            .collect::<Result<_, _>>()
            .expect("committed restore row");
        assert_eq!(returned, ["t-b083"]);
        let restored_events: i64 = connection
            .query_row(
                "SELECT count(*) FROM customer_audit_events WHERE event_type = 'byok.cmk_restored'",
                [],
                |row| row.get(0),
            )
            .expect("restored audit");
        assert_eq!(restored_events, 1);
    }
}

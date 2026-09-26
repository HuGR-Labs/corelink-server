#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "loopback D1 fixture setup and assertions"
)]

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use hmac::{Hmac, KeyInit, Mac};
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde_json::{json, Value};
use sha2::Sha256;

use super::*;
use crate::{
    customer_d1::{ByokCryptoMode, ByokMode},
    storage::{
        d1_http::D1HttpClient,
        staging_load_test_admission::{
            admit_staging_load_test_request, StagingLoadTestAdmissionGate,
            StagingLoadTestAdmissionStore, StagingLoadTestAdmissionVerifier,
            STAGING_LOAD_TEST_ADMISSION_HEADER,
        },
        staging_load_test_ownership::StagingLoadTestScenario,
        StorageEnv,
    },
};

type HmacSha256 = Hmac<Sha256>;

const TENANT: &str = "0192f0c1-2345-7890-abcd-ef0123456789";
const DEPLOYMENT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY: &[u8] = b"01234567890123456789012345678901";
const NONCE_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const NONCE_B: &str = "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Fixture {
    db: Arc<Mutex<Connection>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    endpoint: String,
}

impl Fixture {
    fn new() -> Self {
        let db = Arc::new(Mutex::new(Connection::open_in_memory().expect("sqlite")));
        {
            let db = db.lock().expect("sqlite lock");
            db.execute_batch(
                "CREATE TABLE tenant (
                    tenant_id TEXT PRIMARY KEY,
                    primary_region TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );",
            )
            .expect("base tenant schema");
            for migration in [
                "0031_byok_tenant_status.sql",
                "0081_byok_tenant_config.sql",
                "0118_byok_transition_fence.sql",
                "0121_byok_activation_pipeline.sql",
                "0147_staging_load_test_run_ownership.sql",
                "0148_staging_load_test_admission_nonce.sql",
                "0149_staging_load_test_r2_intents.sql",
            ] {
                let path = format!(
                    "{}/../../migrations/d1/{migration}",
                    env!("CARGO_MANIFEST_DIR")
                );
                db.execute_batch(&std::fs::read_to_string(path).expect("migration read"))
                    .expect("migration apply");
            }
            db.execute(
                "INSERT INTO tenant (tenant_id,primary_region,created_at_ms,updated_at_ms,byok_status) \
                 VALUES (?1,'wnam',1,1,'active')",
                [TENANT],
            )
            .expect("seed tenant");
        }

        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let endpoint = format!(
            "http://{}",
            listener.local_addr().expect("loopback address")
        );
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_db, thread_stop) = (Arc::clone(&db), Arc::clone(&stop));
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => respond(&mut stream, &thread_db),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            db,
            stop,
            worker: Some(worker),
            endpoint,
        }
    }

    fn d1(&self) -> D1HttpClient {
        let env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        D1HttpClient::new_for_loopback_test(&env, &self.endpoint).expect("loopback D1 client")
    }

    fn gate(&self) -> StagingLoadTestAdmissionGate {
        let verifier = StagingLoadTestAdmissionVerifier::new("staging", KEY).expect("test key");
        StagingLoadTestAdmissionGate::from_parts_for_test(
            verifier,
            StagingLoadTestAdmissionStore::from_d1_client_for_test(self.d1()),
        )
    }

    fn seed_ownership_failure(&self, run_id: &str) {
        self.db
            .lock()
            .expect("sqlite lock")
            .execute_batch(&format!(
                "CREATE TRIGGER reject_byok_ownership BEFORE INSERT ON staging_load_test_resources \
                 WHEN NEW.run_id='{run_id}' BEGIN SELECT RAISE(ABORT,'ownership registration rejected'); END;"
            ))
            .expect("ownership failure trigger");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .expect("worker")
            .join()
            .expect("join worker");
    }
}

fn activation() -> ByokActivation {
    ByokActivation {
        tenant_id: TENANT.to_owned(),
        mode: ByokMode::Byok,
        crypto_mode: ByokCryptoMode::Convergent,
        cmk_provider: "aws".to_owned(),
        cmk_key_id: "arn:aws:kms:us-east-1:123456789012:key/example".to_owned(),
        cmk_region: Some("us-east-1".to_owned()),
        tcs_wrapped: b"synthetic-wrapped-secret".to_vec(),
    }
}

fn issue_credential(run_id: &str, nonce: &str) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64;
    let issued = now_ms - 1_000;
    let expires = now_ms + 60_000;
    let payload = format!("v1.{run_id}.byok.staging.{DEPLOYMENT_SHA}.{issued}.{expires}.{nonce}");
    let mut mac = HmacSha256::new_from_slice(KEY).expect("HMAC key");
    mac.update(b"corelink/staging-load-admission-auth/v1\0");
    mac.update(payload.as_bytes());
    format!("{payload}.{}", hex::encode(mac.finalize().into_bytes()))
}

async fn admitted_context(
    gate: &StagingLoadTestAdmissionGate,
    run_id: &str,
    nonce: &str,
) -> Arc<crate::storage::staging_load_test_admission::StagingLoadTestAdmissionContext> {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        STAGING_LOAD_TEST_ADMISSION_HEADER,
        issue_credential(run_id, nonce)
            .parse()
            .expect("credential header"),
    );
    admit_staging_load_test_request(Some(gate), &headers, StagingLoadTestScenario::Byok)
        .await
        .expect("verified durable admission")
        .expect("synthetic admission context")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn synthetic_activation_is_registered_atomically_and_replay_fails_closed() {
    let fixture = Fixture::new();
    let gate = fixture.gate();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        STAGING_LOAD_TEST_ADMISSION_HEADER,
        issue_credential("123", NONCE_A)
            .parse()
            .expect("credential header"),
    );
    let context =
        admit_staging_load_test_request(Some(&gate), &headers, StagingLoadTestScenario::Byok)
            .await
            .expect("verified durable admission")
            .expect("synthetic context");
    let control = D1ByokControl::new(Arc::new(fixture.d1()));

    control
        .prepare_activation_with_context(&activation(), 10_000, Some(&context))
        .await
        .expect("activation plus ownership row");

    {
        let db = fixture.db.lock().expect("sqlite lock");
        let (run_id, scenario, resource_class, handle, disposition, receipt): (
            String,
            String,
            String,
            String,
            String,
            String,
        ) = db
            .query_row(
                "SELECT run_id,scenario,resource_class,opaque_handle,disposition,receipt_ref \
                 FROM staging_load_test_resources",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("one ownership row");
        assert_eq!((run_id.as_str(), scenario.as_str()), ("123", "byok"));
        assert_eq!(resource_class, "byok_artifact");
        assert_eq!(disposition, "disposable");
        assert!(handle.starts_with("byok-activation:"));
        assert!(!handle.contains(TENANT));
        assert_eq!(receipt.len(), 64);
        assert!(!handle.contains("synthetic-wrapped-secret"));
        let registrations: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM staging_load_test_resources",
                [],
                |row| row.get(0),
            )
            .expect("registration count");
        let intents: i64 = db
            .query_row("SELECT COUNT(*) FROM byok_activation_intent", [], |row| {
                row.get(0)
            })
            .expect("intent count");
        assert_eq!((registrations, intents), (1, 1));
    }

    let replay =
        admit_staging_load_test_request(Some(&gate), &headers, StagingLoadTestScenario::Byok).await;
    assert!(replay.is_err(), "replayed credential must fail closed");
    let mut forged_headers = axum::http::HeaderMap::new();
    forged_headers.insert(
        STAGING_LOAD_TEST_ADMISSION_HEADER,
        "forged".parse().expect("forged credential header"),
    );
    assert!(admit_staging_load_test_request(
        Some(&gate),
        &forged_headers,
        StagingLoadTestScenario::Byok,
    )
    .await
    .is_err());
    let resources: i64 = fixture
        .db
        .lock()
        .expect("sqlite lock")
        .query_row(
            "SELECT COUNT(*) FROM staging_load_test_resources",
            [],
            |row| row.get(0),
        )
        .expect("registration count");
    assert_eq!(resources, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_scenario_and_registration_failure_cannot_commit_activation() {
    let fixture = Fixture::new();
    let control = D1ByokControl::new(Arc::new(fixture.d1()));
    let gate = fixture.gate();

    let mut wrong_headers = axum::http::HeaderMap::new();
    let cas_credential = issue_credential("124", NONCE_A).replace(".byok.", ".cas.");
    wrong_headers.insert(
        STAGING_LOAD_TEST_ADMISSION_HEADER,
        cas_credential.parse().expect("credential header"),
    );
    let verifier = StagingLoadTestAdmissionVerifier::new("staging", KEY).expect("test key");
    let wrong_gate = StagingLoadTestAdmissionGate::from_parts_for_test(
        verifier,
        StagingLoadTestAdmissionStore::from_d1_client_for_test(fixture.d1()),
    );
    let wrong_context = admit_staging_load_test_request(
        Some(&wrong_gate),
        &wrong_headers,
        StagingLoadTestScenario::Cas,
    )
    .await
    .expect("valid CAS admission")
    .expect("context");
    assert!(control
        .prepare_activation_with_context(&activation(), 10_000, Some(&wrong_context))
        .await
        .is_err());
    let config_rows: i64 = fixture
        .db
        .lock()
        .expect("sqlite lock")
        .query_row("SELECT COUNT(*) FROM tenant_byok_config", [], |row| {
            row.get(0)
        })
        .expect("config count");
    assert_eq!(config_rows, 0, "scenario mismatch precedes BYOK writes");

    let good_context = admitted_context(&gate, "125", NONCE_B).await;
    fixture.seed_ownership_failure("125");
    assert!(control
        .prepare_activation_with_context(&activation(), 10_001, Some(&good_context))
        .await
        .is_err());
    let db = fixture.db.lock().expect("sqlite lock");
    let configs: i64 = db
        .query_row("SELECT COUNT(*) FROM tenant_byok_config", [], |row| {
            row.get(0)
        })
        .expect("config count");
    let intents: i64 = db
        .query_row("SELECT COUNT(*) FROM byok_activation_intent", [], |row| {
            row.get(0)
        })
        .expect("intent count");
    let resources: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM staging_load_test_resources",
            [],
            |row| row.get(0),
        )
        .expect("registration count");
    assert_eq!((configs, intents, resources), (0, 0, 0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_activation_preserves_none_compatibility_without_registration() {
    let fixture = Fixture::new();
    let control = D1ByokControl::new(Arc::new(fixture.d1()));

    control
        .prepare_activation(&activation(), 10_000)
        .await
        .expect("ordinary activation without a staging context");

    let db = fixture.db.lock().expect("sqlite lock");
    let intents: i64 = db
        .query_row("SELECT COUNT(*) FROM byok_activation_intent", [], |row| {
            row.get(0)
        })
        .expect("intent count");
    let resources: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM staging_load_test_resources",
            [],
            |row| row.get(0),
        )
        .expect("registration count");
    assert_eq!((intents, resources), (1, 0));
}

fn respond(stream: &mut TcpStream, db: &Arc<Mutex<Connection>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0_u8; 4096];
        let read = match stream.read(&mut buffer) {
            Ok(read) => read,
            Err(_) => return,
        };
        bytes.extend_from_slice(&buffer[..read]);
        let Some(headers_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..headers_end]);
        let Some(length) = headers.lines().find_map(|line| {
            line.strip_prefix("content-length: ")
                .or_else(|| line.strip_prefix("Content-Length: "))
        }) else {
            continue;
        };
        let Ok(length) = length.parse::<usize>() else {
            return;
        };
        if bytes.len() >= headers_end + 4 + length {
            break;
        }
    }
    let body_start = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .expect("headers")
        + 4;
    let headers_end = body_start - 4;
    let headers = String::from_utf8_lossy(&bytes[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length: ")
                .or_else(|| line.strip_prefix("Content-Length: "))
        })
        .expect("content length")
        .parse::<usize>()
        .expect("content length number");
    let request: Value = serde_json::from_slice(&bytes[body_start..body_start + content_length])
        .expect("request JSON");

    let response = if let Some(batch) = request.get("batch").and_then(Value::as_array) {
        let mut db = db.lock().expect("sqlite lock");
        let transaction = db.transaction().expect("batch transaction");
        let mut results = Vec::with_capacity(batch.len());
        let mut failure = None;
        for (index, statement) in batch.iter().enumerate() {
            let sql = statement["sql"].as_str().expect("batch SQL");
            let params = sql_params(&statement["params"]);
            match run_statement(&transaction, sql, params) {
                Ok(rows) => results.push(json!({"results": rows, "success": true})),
                Err(error) => {
                    results.push(json!({"results": [], "success": false}));
                    failure = Some((index, error));
                    break;
                }
            }
        }
        match failure {
            Some((_, error)) => {
                json!({"result": results, "success": false, "errors": [{"message": error}]})
            }
            None => {
                transaction.commit().expect("batch commit");
                json!({"result": results, "success": true, "errors": []})
            }
        }
    } else {
        let sql = request["sql"].as_str().expect("query SQL");
        let params = sql_params(&request["params"]);
        match run_statement(&db.lock().expect("sqlite lock"), sql, params) {
            Ok(rows) => {
                json!({"result": [{"results": rows, "success": true}], "success": true, "errors": []})
            }
            Err(error) => {
                json!({"result": [{"results": [], "success": false}], "success": false, "errors": [{"message": error}]})
            }
        }
    };
    let body = response.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("response write");
}

fn sql_params(value: &Value) -> Vec<SqlValue> {
    value
        .as_array()
        .expect("parameter array")
        .iter()
        .map(|value| match value {
            Value::Null => SqlValue::Null,
            Value::Bool(value) => SqlValue::Integer(if *value { 1 } else { 0 }),
            Value::Number(value) => value
                .as_i64()
                .map(SqlValue::Integer)
                .or_else(|| value.as_f64().map(SqlValue::Real))
                .expect("numeric parameter"),
            Value::String(value) => SqlValue::Text(value.clone()),
            _ => panic!("unexpected D1 parameter"),
        })
        .collect()
}

fn run_statement(
    connection: &Connection,
    sql: &str,
    params: Vec<SqlValue>,
) -> Result<Vec<Value>, String> {
    if sql.trim_start().starts_with("SELECT") || sql.contains("RETURNING") {
        let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
        let columns = statement
            .column_names()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let mut rows = statement
            .query(params_from_iter(params))
            .map_err(|error| error.to_string())?;
        let mut output = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let mut object = serde_json::Map::new();
            for (index, column) in columns.iter().enumerate() {
                let value = match row.get_ref(index).map_err(|error| error.to_string())? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(value) => json!(value),
                    rusqlite::types::ValueRef::Real(value) => json!(value),
                    rusqlite::types::ValueRef::Text(value) => {
                        Value::String(String::from_utf8_lossy(value).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(value) => json!(value),
                };
                object.insert(column.clone(), value);
            }
            output.push(Value::Object(object));
        }
        Ok(output)
    } else {
        connection
            .execute(sql, params_from_iter(params))
            .map_err(|error| error.to_string())?;
        Ok(Vec::new())
    }
}

//! Loopback D1/SQLite regressions for durable usage-coordinate classification.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test fixture setup and assertions"
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

use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde_json::Value;

use super::tests_support::{hex64, record_json, tenant_a};
use super::*;

struct Fixture {
    db: Arc<Mutex<Connection>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    endpoint: String,
}

#[rustfmt::skip]
impl Fixture {
    fn new() -> Self {
        let db = Arc::new(Mutex::new(Connection::open_in_memory().expect("sqlite")));
        let migrations = ["0017_usage_event_idem.sql", "0095_usage_event_staging_aggregatable.sql", "0133_usage_event_staging_conflicts.sql"];
        for migration in migrations {
            let path = format!("{}/../../migrations/d1/{migration}", env!("CARGO_MANIFEST_DIR"));
            db.lock().expect("sqlite lock").execute_batch(&std::fs::read_to_string(path).expect("migration read")).expect("migration apply");
        }
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        listener.set_nonblocking(true).expect("nonblocking listener");
        let endpoint = format!("http://{}", listener.local_addr().expect("address"));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_db, thread_stop) = (Arc::clone(&db), Arc::clone(&stop));
        let worker = thread::spawn(move || while !thread_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => respond(&mut stream, &thread_db),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(2)),
                Err(_) => break,
            }
        });
        Self { db, stop, worker: Some(worker), endpoint }
    }

    fn store(&self) -> D1UsageStagingStore {
        let env = crate::storage::StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(), r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(), cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(), d1_database_id: "test".to_owned(),
        };
        D1UsageStagingStore::new(Arc::new(D1HttpClient::new_for_loopback_test(&env, &self.endpoint).expect("loopback D1")))
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

#[rustfmt::skip]
fn respond(stream: &mut TcpStream, db: &Arc<Mutex<Connection>>) {
    stream.set_read_timeout(Some(Duration::from_secs(1))).expect("read timeout");
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0_u8; 4096]; let read = stream.read(&mut buffer).expect("request read");
        bytes.extend_from_slice(&buffer[..read]);
        let Some(headers) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else { continue };
        let text = std::str::from_utf8(&bytes[..headers]).expect("headers utf8");
        let length = text.lines().find_map(|line| line.strip_prefix("content-length: ").or_else(|| line.strip_prefix("Content-Length: "))).expect("content length").parse::<usize>().expect("length number");
        if bytes.len() >= headers + 4 + length { break; }
    }
    let headers = bytes.windows(4).position(|part| part == b"\r\n\r\n").expect("headers") + 4;
    let request: Value = serde_json::from_slice(&bytes[headers..]).expect("request JSON");
    let sql = request["sql"].as_str().expect("SQL");
    let params = request["params"].as_array().expect("params").iter().map(|value| match value {
        Value::String(value) => SqlValue::Text(value.clone()),
        Value::Number(value) => SqlValue::Integer(value.as_i64().expect("integer parameter")),
        _ => panic!("unexpected D1 parameter"),
    }).collect::<Vec<_>>();
    let db = db.lock().expect("sqlite lock");
    let rows = if sql.contains("RETURNING") || sql.trim_start().starts_with("SELECT") {
        let mut statement = db.prepare(sql).expect("sqlite statement");
        let mut rows = statement.query(params_from_iter(params)).expect("sqlite query");
        rows.next().expect("sqlite row").map(|row| vec![serde_json::json!({ "event_payload_hash": row.get::<_, String>(0).expect("fingerprint") }).as_object().expect("object").clone()]).unwrap_or_default()
    } else {
        db.execute(sql, params_from_iter(params)).expect("sqlite mutation");
        Vec::new()
    };
    let body = serde_json::json!({ "result": [{ "results": rows, "success": true }], "success": true, "errors": [] }).to_string();
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("response write");
}

fn record(qty: u64, key: &str) -> StagedUsageRecord {
    let mut record = validate_record(
        serde_json::from_value(record_json(&tenant_a(), key)).expect("wire record"),
    )
    .expect("valid record");
    record.qty = qty;
    record
}

#[rustfmt::skip]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn durable_classification_matrix() {
    let replay = Fixture::new();
    let store = replay.store();
    let same = record(7200, &hex64(0x31));
    assert_eq!(store.stage(&same).await, Ok(StageOutcome::Inserted));
    assert_eq!(store.stage(&same).await, Ok(StageOutcome::Deduped));

    let mismatch = Fixture::new();
    let store = mismatch.store();
    let key = hex64(0x32);
    let accepted = record(7200, &key);
    assert_eq!(store.stage(&accepted).await, Ok(StageOutcome::Inserted));
    mismatch
        .db
        .lock()
        .expect("sqlite lock")
        .execute(
            "UPDATE usage_event_staging SET drained_to_r2_at = emitted_at + 1",
            [],
        )
        .expect("watermark");
    let before: (String, i64, Option<i64>) = mismatch
        .db
        .lock()
        .expect("sqlite lock")
        .query_row(
            "SELECT event_payload_hash, qty, drained_to_r2_at FROM usage_event_staging",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("accepted row");
    assert_eq!(
        store.stage(&record(7201, &key)).await,
        Ok(StageOutcome::Conflict(StageConflictReason::PayloadMismatch))
    );
    let after: (String, i64, Option<i64>) = mismatch
        .db
        .lock()
        .expect("sqlite lock")
        .query_row(
            "SELECT event_payload_hash, qty, drained_to_r2_at FROM usage_event_staging",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("accepted row");
    assert_eq!(after, before);
    let conflict: (String, String, String, i64) = mismatch.db.lock().expect("sqlite lock").query_row("SELECT observed_fingerprint, incoming_fingerprint, reason, observation_count FROM usage_event_staging_conflicts", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).expect("conflict row");
    assert_eq!(
        (conflict.0, conflict.2, conflict.3),
        (before.0, "payload_mismatch".to_owned(), 1)
    );
    assert_eq!(
        conflict.1,
        D1UsageStagingStore::payload_hash(&record(7201, &key))
    );

    let concurrent = Fixture::new();
    let store = Arc::new(concurrent.store());
    let key = hex64(0x33);
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let left = {
        let (store, barrier, record) =
            (Arc::clone(&store), Arc::clone(&barrier), record(7200, &key));
        tokio::spawn(async move {
            barrier.wait().await;
            store.stage(&record).await
        })
    };
    let right = {
        let (store, barrier, record) =
            (Arc::clone(&store), Arc::clone(&barrier), record(7201, &key));
        tokio::spawn(async move {
            barrier.wait().await;
            store.stage(&record).await
        })
    };
    barrier.wait().await;
    let outcomes = [
        left.await.expect("left task"),
        right.await.expect("right task"),
    ];
    assert!(outcomes.contains(&Ok(StageOutcome::Inserted)));
    assert!(outcomes.contains(&Ok(StageOutcome::Conflict(
        StageConflictReason::PayloadMismatch
    ))));

    let legacy = Fixture::new();
    let store = legacy.store();
    let key = hex64(0x34);
    let record = record(7200, &key);
    assert_eq!(store.stage(&record).await, Ok(StageOutcome::Inserted));
    legacy
        .db
        .lock()
        .expect("sqlite lock")
        .execute(
            "UPDATE usage_event_staging SET event_payload_hash = ?1",
            ["z".repeat(64)],
        )
        .expect("legacy fingerprint");
    assert_eq!(
        store.stage(&record).await,
        Ok(StageOutcome::Conflict(
            StageConflictReason::ExistingFingerprintUnverifiable
        ))
    );
}

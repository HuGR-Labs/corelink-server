use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rusqlite::{params, params_from_iter, Connection};
use serde_json::Value;

use super::*;

#[derive(Debug)]
struct SqliteD1 {
    db: Mutex<Connection>,
}

impl SqliteD1 {
    fn fixture(state: &str, status: &str) -> Arc<Self> {
        let db = Connection::open_in_memory().expect("sqlite");
        db.execute_batch(
            "CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, byok_status TEXT);\
             CREATE TABLE tenant_byok_config (\
               tenant_id TEXT PRIMARY KEY, state TEXT NOT NULL);",
        )
        .expect("base schema");
        db.execute_batch(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../migrations/d1/0118_byok_transition_fence.sql"
        )))
        .expect("0118 migration");
        db.execute(
            "INSERT INTO tenant VALUES ('tenant-a', ?1)",
            params![status],
        )
        .expect("tenant");
        db.execute(
            "INSERT INTO tenant_byok_config (tenant_id, state, config_version) \
             VALUES ('tenant-a', ?1, 7)",
            params![state],
        )
        .expect("config");
        Arc::new(Self { db: Mutex::new(db) })
    }

    fn count(&self, table: &str) -> i64 {
        self.db
            .lock()
            .expect("lock")
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count")
    }

    fn expire_all(&self, table: &str) {
        self.db
            .lock()
            .expect("lock")
            .execute(
                &format!("UPDATE {table} SET acquired_at_ms = 0, expires_at_ms = 1"),
                [],
            )
            .expect("expire");
    }
}

#[async_trait]
impl ByokFenceD1Client for SqliteD1 {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
        let values = params
            .iter()
            .map(json_to_sql)
            .collect::<Result<Vec<_>, _>>()?;
        let db = self
            .db
            .lock()
            .map_err(|_| "sqlite lock poisoned".to_owned())?;
        let mut statement = db.prepare(sql).map_err(|e| e.to_string())?;
        let names = statement
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut rows = statement
            .query(params_from_iter(values))
            .map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut mapped = D1Row::new();
            for (index, name) in names.iter().enumerate() {
                let value = row.get_ref(index).map_err(|e| e.to_string())?.to_owned();
                mapped.insert(name.clone(), sql_to_json(value.into()));
            }
            result.push(mapped);
        }
        Ok(result)
    }
}

fn json_to_sql(value: &Value) -> Result<rusqlite::types::Value, String> {
    match value {
        Value::Null => Ok(rusqlite::types::Value::Null),
        Value::Bool(value) => Ok(rusqlite::types::Value::Integer(i64::from(*value))),
        Value::Number(value) => value
            .as_i64()
            .map(rusqlite::types::Value::Integer)
            .ok_or_else(|| "non-i64 number".to_owned()),
        Value::String(value) => Ok(rusqlite::types::Value::Text(value.clone())),
        _ => Err("unsupported test parameter".to_owned()),
    }
}

fn sql_to_json(value: rusqlite::types::Value) -> Value {
    match value {
        rusqlite::types::Value::Null => Value::Null,
        rusqlite::types::Value::Integer(value) => Value::from(value),
        rusqlite::types::Value::Real(value) => Value::from(value),
        rusqlite::types::Value::Text(value) => Value::from(value),
        rusqlite::types::Value::Blob(value) => {
            Value::Array(value.into_iter().map(Value::from).collect())
        }
    }
}

fn active_snapshot() -> ConfigSnapshot {
    ConfigSnapshot {
        gate_epoch: 1,
        current_generation: 0,
        config_version: Some(7),
        config_state: ConfigState::Active,
        byok_status: ByokStatus::Active,
    }
}

#[test]
fn migration_rejects_preexisting_active_configuration() {
    let db = Connection::open_in_memory().expect("sqlite");
    db.execute_batch(
        "CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, byok_status TEXT);\
         CREATE TABLE tenant_byok_config (tenant_id TEXT PRIMARY KEY, state TEXT NOT NULL);\
         INSERT INTO tenant VALUES ('tenant-a','active');\
         INSERT INTO tenant_byok_config VALUES ('tenant-a','active');",
    )
    .expect("base schema");
    let result = db.execute_batch(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations/d1/0118_byok_transition_fence.sql"
    )));
    assert!(
        result.is_err(),
        "0118 must refuse an un-cataloged active tenant"
    );
}

#[tokio::test]
async fn catalog_allows_concurrent_allocations_and_lists_only_publication() {
    let client = SqliteD1::fixture("active", "active");
    client
        .db
        .lock()
        .expect("lock")
        .execute(
            "UPDATE byok_tenant_gate SET current_generation=3 WHERE tenant_id='tenant-a'",
            [],
        )
        .expect("generation");
    let fence = D1ByokFence::new(Arc::clone(&client));
    let intent = fence
        .acquire_data_intent("tenant-a", DataOperation::Write, Duration::from_secs(30))
        .await
        .expect("intent");
    let db = client.db.lock().expect("lock");
    for (allocation, physical) in [("write-a", "r2/a"), ("write-b", "r2/b")] {
        db.execute(
            "INSERT INTO byok_logical_object_generation (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,intent_token,gate_epoch,size_bytes,allocated_at_ms) VALUES ('tenant-a','ac','action',3,?1,?2,?3,1,9,1)",
            params![allocation, physical, intent.token()],
        ).expect("distinct allocation in same generation");
    }
    db.execute(
        "INSERT INTO byok_logical_object_publication (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms) VALUES ('tenant-a','ac','action',3,'write-b','r2/b',1,9,2)", [],
    ).expect("publication");
    let listed: Vec<(String, String)> = db.prepare(
        "SELECT logical_key,physical_key FROM byok_logical_object_publication WHERE tenant_id='tenant-a' AND object_kind='ac' ORDER BY logical_key",
    ).expect("prepare").query_map([], |row| Ok((row.get(0)?, row.get(1)?))).expect("list").collect::<Result<_, _>>().expect("rows");
    assert_eq!(listed, vec![("action".to_owned(), "r2/b".to_owned())]);
}

#[tokio::test]
async fn data_intent_blocks_transition_until_exact_release() {
    let db = SqliteD1::fixture("active", "active");
    let fence = D1ByokFence::new(Arc::clone(&db));
    let intent = fence
        .acquire_data_intent("tenant-a", DataOperation::Write, Duration::from_secs(5))
        .await
        .expect("intent");
    assert_eq!(intent.snapshot, active_snapshot());
    assert!(matches!(
        fence
            .acquire_transition_fence("tenant-a", &active_snapshot(), Duration::from_secs(5))
            .await,
        Err(FenceError::AcquireDenied)
    ));
    fence
        .release_data_intent(&intent)
        .await
        .expect("exact release");
    fence
        .acquire_transition_fence("tenant-a", &active_snapshot(), Duration::from_secs(5))
        .await
        .expect("transition after release");
}

#[tokio::test]
async fn transition_blocks_all_data_operations() {
    let db = SqliteD1::fixture("active", "active");
    let fence = D1ByokFence::new(db);
    let transition = fence
        .acquire_transition_fence("tenant-a", &active_snapshot(), Duration::from_secs(10))
        .await
        .expect("transition");
    for operation in [
        DataOperation::Read,
        DataOperation::Write,
        DataOperation::Delete,
    ] {
        assert!(matches!(
            fence
                .acquire_data_intent("tenant-a", operation, Duration::from_secs(1))
                .await,
            Err(FenceError::AcquireDenied)
        ));
    }
    fence
        .abort_transition_fence(&transition)
        .await
        .expect("release");
}

#[tokio::test]
async fn stale_snapshot_terminal_states_and_unhealthy_status_fail_closed() {
    let active = D1ByokFence::new(SqliteD1::fixture("active", "active"));
    let mut stale = active_snapshot();
    stale.config_version = Some(6);
    assert!(matches!(
        active
            .acquire_transition_fence("tenant-a", &stale, Duration::from_secs(1))
            .await,
        Err(FenceError::AcquireDenied)
    ));

    for (state, status) in [
        ("shredded", "active"),
        ("active", "degraded_read_only"),
        ("active", "revoked"),
    ] {
        let adapter = D1ByokFence::new(SqliteD1::fixture(state, status));
        assert!(matches!(
            adapter
                .acquire_data_intent("tenant-a", DataOperation::Read, Duration::from_secs(1))
                .await,
            Err(FenceError::AcquireDenied)
        ));
    }

    let shredded = D1ByokFence::new(SqliteD1::fixture("shredded", "active"));
    let snapshot = shredded.load_snapshot("tenant-a").await.expect("snapshot");
    assert_eq!(snapshot.config_state, ConfigState::Shredded);
    assert!(matches!(
        shredded
            .acquire_transition_fence("tenant-a", &snapshot, Duration::from_secs(1))
            .await,
        Err(FenceError::AcquireDenied)
    ));
}

#[tokio::test]
async fn expired_and_replayed_tokens_never_succeed() {
    let db = SqliteD1::fixture("active", "active");
    let adapter = D1ByokFence::new(Arc::clone(&db));
    let intent = adapter
        .acquire_data_intent("tenant-a", DataOperation::Read, Duration::from_secs(1))
        .await
        .expect("intent");
    db.expire_all("byok_data_intent");
    assert_eq!(
        adapter.release_data_intent(&intent).await,
        Err(FenceError::Expired)
    );
    assert_eq!(
        adapter.release_data_intent(&intent).await,
        Err(FenceError::StaleToken)
    );

    let first = adapter
        .acquire_transition_fence("tenant-a", &active_snapshot(), Duration::from_secs(1))
        .await
        .expect("first fence");
    db.expire_all("byok_transition_fence");
    let second = adapter
        .acquire_transition_fence("tenant-a", &active_snapshot(), Duration::from_secs(1))
        .await
        .expect("replace expired fence");
    assert_eq!(
        adapter.abort_transition_fence(&first).await,
        Err(FenceError::StaleToken)
    );
    adapter
        .abort_transition_fence(&second)
        .await
        .expect("new token");
}

#[tokio::test]
async fn exhausted_epoch_fails_closed_without_real_coercion() {
    let db = SqliteD1::fixture("active", "active");
    db.db
        .lock()
        .expect("lock")
        .execute(
            "INSERT INTO byok_transition_fence (token, tenant_id, epoch, outcome, \
             observed_gate_epoch, observed_generation, observed_config_version, \
             observed_config_state, observed_byok_status, acquired_at_ms, expires_at_ms, \
             completed_at_ms) VALUES ('old', 'tenant-a', 9223372036854775806, 'expired', \
             1, 0, 7, 'active', 'active', 0, 1, 1)",
            [],
        )
        .expect("max epoch fixture");
    let adapter = D1ByokFence::new(db);
    assert!(matches!(
        adapter
            .acquire_transition_fence("tenant-a", &active_snapshot(), Duration::from_secs(1))
            .await,
        Err(FenceError::AcquireDenied)
    ));
    assert_eq!(
        checked_next_counter("gate_epoch", MAX_BYOK_COUNTER),
        Err(FenceError::CounterExhausted("gate_epoch"))
    );
}

#[tokio::test]
async fn lease_bounds_are_enforced_before_d1() {
    let db = SqliteD1::fixture("active", "active");
    let adapter = D1ByokFence::new(Arc::clone(&db));
    for lease in [
        Duration::ZERO,
        MAX_BYOK_FENCE_LEASE + Duration::from_millis(1),
    ] {
        assert!(matches!(
            adapter
                .acquire_data_intent("tenant-a", DataOperation::Read, lease)
                .await,
            Err(FenceError::InvalidLease(_))
        ));
    }
    assert_eq!(db.count("byok_data_intent"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn barrier_race_has_exactly_one_side_win() {
    for _round in 0..32 {
        let db = SqliteD1::fixture("active", "active");
        let adapter = Arc::new(D1ByokFence::new(Arc::clone(&db)));
        let barrier = Arc::new(Barrier::new(2));

        let data_adapter = Arc::clone(&adapter);
        let data_barrier = Arc::clone(&barrier);
        let data = tokio::task::spawn_blocking(move || {
            data_barrier.wait();
            tokio::runtime::Handle::current().block_on(data_adapter.acquire_data_intent(
                "tenant-a",
                DataOperation::Write,
                Duration::from_secs(1),
            ))
        });
        let transition_adapter = Arc::clone(&adapter);
        let transition_barrier = Arc::clone(&barrier);
        let transition = tokio::task::spawn_blocking(move || {
            transition_barrier.wait();
            tokio::runtime::Handle::current().block_on(transition_adapter.acquire_transition_fence(
                "tenant-a",
                &active_snapshot(),
                Duration::from_secs(1),
            ))
        });

        let data_won = data.await.expect("data join").is_ok();
        let transition_won = transition.await.expect("transition join").is_ok();
        assert_ne!(data_won, transition_won);
        assert_eq!(
            db.count("byok_data_intent") + db.count("byok_transition_fence"),
            1
        );
    }
}

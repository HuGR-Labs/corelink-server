use super::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct FakeStore(Arc<Mutex<FakeState>>);

#[derive(Default)]
struct FakeState {
    run: Option<BackfillRun>,
    current_generation: i64,
    state_active: bool,
    source: BTreeMap<(BackfillSurface, String), (String, Vec<u8>)>,
    physical: HashMap<String, Vec<u8>>,
    catalog: BTreeMap<(BackfillSurface, i64, String), String>,
    stage_calls: usize,
    fail_checkpoint_once: bool,
    transition_expired: bool,
}

impl FakeStore {
    fn add(&self, surface: BackfillSurface, logical: &str, physical: &str, bytes: &[u8]) {
        let mut state = self.0.lock().unwrap();
        state.source.insert(
            (surface, logical.to_owned()),
            (physical.to_owned(), bytes.to_vec()),
        );
        state.physical.insert(physical.to_owned(), bytes.to_vec());
    }

    fn set_fail_checkpoint_once(&self) {
        self.0.lock().unwrap().fail_checkpoint_once = true;
    }

    fn expire_transition(&self) {
        self.0.lock().unwrap().transition_expired = true;
    }

    fn visible(&self, surface: BackfillSurface, logical: &str) -> Option<Vec<u8>> {
        let state = self.0.lock().unwrap();
        if !state.state_active {
            return None;
        }
        let physical =
            state
                .catalog
                .get(&(surface, state.current_generation, logical.to_owned()))?;
        state.physical.get(physical).cloned()
    }
}

#[async_trait]
impl ByokBackfillStore for FakeStore {
    async fn begin(&self, request: &BeginBackfill) -> Result<BackfillRun, BackfillError> {
        let mut state = self.0.lock().unwrap();
        if state.run.is_some() {
            return Err(BackfillError::Conflict("transition already held".into()));
        }
        state.state_active = false;
        let run = BackfillRun {
            tenant_id: request.tenant_id.clone(),
            run_id: "fake-stable-run-id".into(),
            run_token: "fake-generated-256-bit-token".into(),
            transition_epoch: 3,
            gate_epoch: request.expected_gate_epoch,
            source_generation: request.expected_source_generation,
            target_generation: request.expected_source_generation + 1,
            phase: BackfillPhase::Copying,
            cas_cursor: None,
            ac_cursor: None,
            cas_complete: false,
            ac_complete: false,
        };
        state.run = Some(run.clone());
        Ok(run)
    }

    async fn resume(
        &self,
        tenant_id: &str,
        run_id: &str,
        previous_token: &str,
        _lease: Duration,
    ) -> Result<BackfillRun, BackfillError> {
        let mut state = self.0.lock().unwrap();
        let transition_expired = state.transition_expired;
        if transition_expired {
            state.transition_expired = false;
        }
        let run = state
            .run
            .as_mut()
            .ok_or_else(|| BackfillError::Conflict("no durable run".into()))?;
        if run.tenant_id != tenant_id || run.run_id != run_id || run.run_token != previous_token {
            return Err(BackfillError::Conflict("token mismatch".into()));
        }
        if transition_expired {
            run.run_token = "fake-takeover-token".into();
            run.transition_epoch += 1;
        }
        Ok(run.clone())
    }

    async fn renew(&self, run: &BackfillRun, _lease: Duration) -> Result<(), BackfillError> {
        exact_run(&self.0.lock().unwrap(), run)
    }

    async fn enumerate(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        after: Option<&str>,
        limit: usize,
    ) -> Result<BackfillPage, BackfillError> {
        let state = self.0.lock().unwrap();
        exact_run(&state, run)?;
        let objects: Vec<_> = state
            .source
            .iter()
            .filter(|((candidate_surface, logical), _)| {
                *candidate_surface == surface
                    && after.is_none_or(|cursor| logical.as_str() > cursor)
            })
            .take(limit)
            .map(
                |((candidate_surface, logical), (physical, _))| BackfillSourceObject {
                    surface: *candidate_surface,
                    logical_key: logical.clone(),
                    source_physical_key: physical.clone(),
                    source_crypto: Some(SourceCryptoIdentity::Plaintext),
                    source_blake3: None,
                    plaintext_size: None,
                },
            )
            .collect();
        let has_more = objects.last().is_some_and(|last| {
            state.source.keys().any(|(candidate_surface, logical)| {
                *candidate_surface == surface && logical > &last.logical_key
            })
        });
        Ok(BackfillPage {
            next_cursor: has_more.then(|| {
                objects
                    .last()
                    .expect("non-empty when has_more")
                    .logical_key
                    .clone()
            }),
            objects,
        })
    }

    async fn read_source(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
    ) -> Result<Vec<u8>, BackfillError> {
        let state = self.0.lock().unwrap();
        exact_run(&state, run)?;
        state
            .physical
            .get(&source.source_physical_key)
            .cloned()
            .ok_or_else(|| BackfillError::SourceChanged(source.source_physical_key.clone()))
    }

    async fn target_physical_key(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        logical_key: &str,
    ) -> Result<String, BackfillError> {
        Ok(format!(
            "iad/secret-prefix/{}/{}",
            run.tenant_id,
            generation_qualified_suffix(surface, run.target_generation, logical_key)
        ))
    }

    async fn stage(
        &self,
        run: &BackfillRun,
        staged: &StagedBackfillObject,
    ) -> Result<BackfillAllocation, BackfillError> {
        let mut state = self.0.lock().unwrap();
        exact_run(&state, run)?;
        state.stage_calls += 1;
        if let Some(existing) = state.physical.get(&staged.target_physical_key) {
            if existing != &staged.ciphertext {
                return Err(BackfillError::Conflict("unequal idempotent PUT".into()));
            }
            return Ok(allocation_for(staged));
        }
        state.physical.insert(
            staged.target_physical_key.clone(),
            staged.ciphertext.clone(),
        );
        Ok(allocation_for(staged))
    }

    async fn checkpoint(
        &self,
        run: &BackfillRun,
        checkpoint: &BackfillCheckpoint,
    ) -> Result<(), BackfillError> {
        let mut state = self.0.lock().unwrap();
        exact_run(&state, run)?;
        if state.fail_checkpoint_once {
            state.fail_checkpoint_once = false;
            return Err(BackfillError::Store("injected checkpoint crash".into()));
        }
        if let Some(allocation) = &checkpoint.allocation {
            let bytes = state.physical.get(&allocation.target_physical_key);
            if bytes.is_none_or(|value| {
                hex::encode(blake3::hash(value).as_bytes()) != allocation.ciphertext_blake3
            }) {
                return Err(BackfillError::Store("staged bytes absent".into()));
            }
            let key = (
                allocation.surface,
                allocation.target_generation,
                allocation.logical_key.clone(),
            );
            if let Some(existing) = state.catalog.get(&key) {
                if existing != &allocation.target_physical_key {
                    return Err(BackfillError::Conflict("catalog publication drift".into()));
                }
            } else {
                state
                    .catalog
                    .insert(key, allocation.target_physical_key.clone());
            }
        }
        let durable = state.run.as_mut().unwrap();
        durable.apply_checkpoint(checkpoint);
        Ok(())
    }

    async fn commit_generation(&self, run: &BackfillRun) -> Result<(), BackfillError> {
        let mut state = self.0.lock().unwrap();
        exact_run(&state, run)?;
        let durable = state.run.as_ref().unwrap();
        if durable.phase != BackfillPhase::ReadyToCommit
            || !durable.cas_complete
            || !durable.ac_complete
        {
            return Err(BackfillError::Conflict(
                "durable completeness missing".into(),
            ));
        }
        state.current_generation = run.target_generation;
        state.state_active = true;
        state.run.as_mut().unwrap().phase = BackfillPhase::Committed;
        Ok(())
    }

    async fn abort(&self, run: &BackfillRun, _reason: &str) -> Result<(), BackfillError> {
        let mut state = self.0.lock().unwrap();
        exact_run(&state, run)?;
        state.run.as_mut().unwrap().phase = BackfillPhase::Aborted;
        state.state_active = false;
        Ok(())
    }
}

fn exact_run(state: &FakeState, run: &BackfillRun) -> Result<(), BackfillError> {
    let durable = state
        .run
        .as_ref()
        .ok_or_else(|| BackfillError::Conflict("transition absent".into()))?;
    if durable.tenant_id != run.tenant_id
        || durable.run_id != run.run_id
        || durable.run_token != run.run_token
        || durable.transition_epoch != run.transition_epoch
        || durable.gate_epoch != run.gate_epoch
        || durable.target_generation != run.target_generation
        || matches!(
            durable.phase,
            BackfillPhase::Committed | BackfillPhase::Aborted
        )
    {
        return Err(BackfillError::Conflict(
            "exact transition token lost".into(),
        ));
    }
    Ok(())
}

fn allocation_for(staged: &StagedBackfillObject) -> BackfillAllocation {
    BackfillAllocation {
        allocation_id: staged.allocation_id.clone(),
        surface: staged.surface,
        logical_key: staged.logical_key.clone(),
        target_generation: staged.target_generation,
        target_physical_key: staged.target_physical_key.clone(),
        plaintext_len: staged.plaintext_len,
        ciphertext_len: staged.ciphertext.len() as u64,
        ciphertext_blake3: hex::encode(blake3::hash(&staged.ciphertext).as_bytes()),
    }
}

#[derive(Clone, Copy)]
struct FakeEncryptor;

#[async_trait]
impl ByokBackfillEncryptor for FakeEncryptor {
    async fn target_hardened_digest(
        &self,
        _run: &BackfillRun,
        _surface: BackfillSurface,
        logical_key: &str,
        _config_version: i64,
        _tcs_version: i64,
    ) -> Result<String, BackfillError> {
        Ok(logical_key
            .rsplit_once(':')
            .map_or(logical_key, |(_, digest)| digest)
            .to_owned())
    }

    async fn source_hardened_digest(
        &self,
        _run: &BackfillRun,
        source: &BackfillSourceObject,
    ) -> Result<String, BackfillError> {
        Ok(source
            .logical_key
            .rsplit_once(':')
            .map_or(source.logical_key.as_str(), |(_, digest)| digest)
            .to_owned())
    }

    async fn encrypt(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
        _allocation_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, BackfillError> {
        let mut out = format!(
            "ENC:{}:{}:{}:",
            run.target_generation,
            source.surface.as_str(),
            source.logical_key
        )
        .into_bytes();
        out.extend_from_slice(plaintext);
        Ok(out)
    }
}

fn request() -> BeginBackfill {
    BeginBackfill {
        tenant_id: "tenant-a".into(),
        expected_gate_epoch: 7,
        expected_source_generation: 0,
        lease: Duration::from_secs(60),
    }
}

async fn drain(engine: &ByokBackfillEngine<FakeStore, FakeEncryptor>, run: &mut BackfillRun) {
    for surface in [BackfillSurface::Cas, BackfillSurface::Ac] {
        while !run.complete(surface) {
            engine
                .copy_page(run, surface, 1, Duration::from_secs(60))
                .await
                .unwrap();
        }
    }
}

#[tokio::test]
async fn cas_and_ac_stay_invisible_until_one_complete_generation_commit() {
    let store = FakeStore::default();
    store.add(BackfillSurface::Cas, "c1", "raw/c1", b"cas-one");
    store.add(BackfillSurface::Cas, "c2", "g0/c2", b"cas-two");
    store.add(BackfillSurface::Ac, "a1", "raw/a1", b"ac-one");
    let engine = ByokBackfillEngine::new(store.clone(), FakeEncryptor);
    let mut run = engine.begin(&request()).await.unwrap();

    engine
        .copy_page(&mut run, BackfillSurface::Cas, 1, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(store.visible(BackfillSurface::Cas, "c1"), None);
    assert!(engine.commit(&mut run).await.is_err());

    drain(&engine, &mut run).await;
    assert_eq!(run.phase, BackfillPhase::ReadyToCommit);
    assert_eq!(store.visible(BackfillSurface::Ac, "a1"), None);
    engine.commit(&mut run).await.unwrap();

    assert!(store
        .visible(BackfillSurface::Cas, "c1")
        .unwrap()
        .ends_with(b"cas-one"));
    assert!(store
        .visible(BackfillSurface::Ac, "a1")
        .unwrap()
        .ends_with(b"ac-one"));
}

#[tokio::test]
async fn crash_after_r2_stage_resumes_same_key_without_duplicate_publication() {
    let store = FakeStore::default();
    store.add(BackfillSurface::Cas, "c1", "raw/c1", b"payload");
    store.set_fail_checkpoint_once();
    let engine = ByokBackfillEngine::new(store.clone(), FakeEncryptor);
    let mut lost = engine.begin(&request()).await.unwrap();

    assert!(engine
        .copy_page(&mut lost, BackfillSurface::Cas, 1, Duration::from_secs(60),)
        .await
        .is_err());
    assert_eq!(store.visible(BackfillSurface::Cas, "c1"), None);
    store.expire_transition();

    let mut resumed = engine
        .resume(
            "tenant-a",
            "fake-stable-run-id",
            "fake-generated-256-bit-token",
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert_eq!(resumed.run_id, lost.run_id);
    assert_eq!(resumed.run_token, "fake-takeover-token");
    assert!(resumed.transition_epoch > lost.transition_epoch);
    drain(&engine, &mut resumed).await;
    engine.commit(&mut resumed).await.unwrap();
    let state = store.0.lock().unwrap();
    assert_eq!(state.stage_calls, 2, "same deterministic PUT was retried");
    assert_eq!(
        state
            .catalog
            .keys()
            .filter(|(surface, _, logical)| { *surface == BackfillSurface::Cas && logical == "c1" })
            .count(),
        1
    );
}

#[tokio::test]
async fn barrier_delayed_old_generation_put_is_unaddressable_after_switch() {
    let store = FakeStore::default();
    store.add(BackfillSurface::Cas, "same", "raw/same", b"snapshot");
    let engine = ByokBackfillEngine::new(store.clone(), FakeEncryptor);
    let mut run = engine.begin(&request()).await.unwrap();
    let ready = Arc::new(tokio::sync::Barrier::new(2));
    let release = Arc::new(tokio::sync::Barrier::new(2));
    let stale_store = store.clone();
    let stale_ready = ready.clone();
    let stale_release = release.clone();

    let delayed = tokio::spawn(async move {
        let captured_old_key = "raw/same".to_owned();
        stale_ready.wait().await;
        stale_release.wait().await;
        stale_store
            .0
            .lock()
            .unwrap()
            .physical
            .insert(captured_old_key, b"late-stale-put".to_vec());
    });
    ready.wait().await;
    drain(&engine, &mut run).await;
    engine.commit(&mut run).await.unwrap();
    release.wait().await;
    delayed.await.unwrap();

    let visible = store.visible(BackfillSurface::Cas, "same").unwrap();
    assert!(visible.ends_with(b"snapshot"));
    assert!(!visible.ends_with(b"late-stale-put"));
}

#[tokio::test]
async fn abort_never_publishes_staged_generation_and_wrong_token_cannot_resume() {
    let store = FakeStore::default();
    store.add(BackfillSurface::Cas, "c1", "raw/c1", b"payload");
    let engine = ByokBackfillEngine::new(store.clone(), FakeEncryptor);
    let mut run = engine.begin(&request()).await.unwrap();
    engine
        .copy_page(&mut run, BackfillSurface::Cas, 1, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(engine
        .resume(
            "tenant-a",
            "fake-stable-run-id",
            "thief",
            Duration::from_secs(60),
        )
        .await
        .is_err());
    engine.abort(&mut run, "operator cancelled").await.unwrap();
    assert_eq!(run.phase, BackfillPhase::Aborted);
    assert_eq!(store.visible(BackfillSurface::Cas, "c1"), None);
}

#[test]
fn generation_key_is_surface_separated_deterministic_and_path_safe() {
    let cas = generation_qualified_suffix(BackfillSurface::Cas, 9, "aa/bb");
    assert_eq!(
        cas,
        generation_qualified_suffix(BackfillSurface::Cas, 9, "aa/bb")
    );
    assert_ne!(
        cas,
        generation_qualified_suffix(BackfillSurface::Ac, 9, "aa/bb")
    );
    assert!(!cas.ends_with("aa/bb"));
    assert!(cas.contains("g00000000000000000009"));
}

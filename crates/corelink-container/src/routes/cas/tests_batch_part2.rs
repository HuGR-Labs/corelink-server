/// Companion: a `/batch-read` BELOW the limit succeeds and RELEASES its read
/// slot (read pool back to empty), pinning the RAII release on the read axis.
#[tokio::test]
async fn batch_read_below_limit_releases_slot() {
    let state = fixture();
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(format!(
            "{}\n",
            serde_json::json!({ "hash": fake_hash(b"absent-blob") })
        )))
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    // The slot is deliberately still held while the response body is
    // outstanding; consuming the stream is the release boundary.
    {
        let g = state.read_inflight.lock().expect("lock");
        assert_eq!(g.get(TEST_TENANT), Some(&1));
    }
    let _ = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let g = state.read_inflight.lock().expect("lock");
    assert_eq!(
        g.get(TEST_TENANT),
        None,
        "the batch-read's read-concurrency slot must be released after its stream is consumed"
    );
}

/// Batch admission is one-at-a-time because one envelope plus one full object
/// peak is the safe 220 MiB bound. The admission permit must remain held while
/// the response stream is outstanding, then become available again after the
/// stream is consumed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_read_admission_is_released_after_response_consumed() {
    let state = fixture();
    let bytes = b"concurrent-batch-read".to_vec();
    let hash = fake_hash(&bytes);
    state
        .write
        .write(CasWriteRequest::new(
            TEST_TENANT,
            hash.clone(),
            bytes,
            format!("anon@{TEST_TENANT}"),
            TEST_TENANT,
            0,
        ))
        .expect("seed CAS object");

    let request = || {
        Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
            .header("x-corelink-tenant-id", TEST_TENANT)
            .header(crate::scope::SCOPE_HEADER, "cas:r")
            .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
            .body(Body::from(format!(
                "{}\n",
                serde_json::json!({ "hash": hash })
            )))
            .expect("request")
    };
    let first = router(state.clone())
        .oneshot(request())
        .await
        .expect("first response");
    assert_eq!(first.status(), StatusCode::OK);

    // The first response has not been consumed, so its batch envelope and
    // admission permit are still live. A second batch is bounded backpressure,
    // not a weighted-budget deadlock or an unbounded queue.
    let blocked = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        router(state.clone()).oneshot(request()),
    )
    .await
    .expect("admission timeout must be bounded")
    .expect("second response");
    assert_eq!(blocked.status(), StatusCode::SERVICE_UNAVAILABLE);

    let _ = axum::body::to_bytes(first.into_body(), usize::MAX)
        .await
        .expect("first body");

    let recovered = router(state)
        .oneshot(request())
        .await
        .expect("recovered response");
    assert_eq!(recovered.status(), StatusCode::OK);
    let _ = axum::body::to_bytes(recovered.into_body(), usize::MAX)
        .await
        .expect("recovered body");
}

#[derive(Debug)]
struct BatchReadTrackingHandler {
    active: Arc<std::sync::atomic::AtomicUsize>,
    max_active: Arc<std::sync::atomic::AtomicUsize>,
    started: Option<Arc<std::sync::atomic::AtomicUsize>>,
    first_started: Option<Arc<tokio::sync::Notify>>,
    saw_max_bytes: Arc<std::sync::atomic::AtomicBool>,
    response_len: usize,
    fail_hash: Option<String>,
    read_delay: std::time::Duration,
}

impl CasReadHandler for BatchReadTrackingHandler {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        use std::sync::atomic::Ordering;

        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(started) = self.started.as_ref() {
            started.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(first_started) = self.first_started.as_ref() {
            first_started.notify_one();
        }
        let mut observed = self.max_active.load(Ordering::SeqCst);
        while active > observed {
            match self.max_active.compare_exchange(
                observed,
                active,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(next) => observed = next,
            }
        }
        if req.max_bytes == Some(BATCH_MAX_BYTES as u64) {
            self.saw_max_bytes.store(true, Ordering::SeqCst);
        }
        // Give the other window tasks time to enter the storage seam. This
        // makes the >1 concurrency assertion deterministic on multi-thread
        // test runtimes while the production window remains budget-derived.
        std::thread::sleep(self.read_delay);
        let result = if self.fail_hash.as_deref() == Some(req.hash.as_str()) {
            Err(CasHandlerError::Internal(
                "tracked batch read failure".into(),
            ))
        } else {
            Ok(CasReadResponse::new(vec![0; self.response_len], req.hash))
        };
        self.active.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

fn tracking_state(read: Arc<dyn CasReadHandler>) -> CasRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared = Arc::new(InMemoryCasHandler::new(audit, sli));
    let write: Arc<dyn CasWriteHandler> = shared.clone();
    let delete: Arc<dyn CasDeleteHandler> = shared.clone();
    let list: Arc<dyn CasListHandler> = shared;
    CasRouteState {
        read,
        write,
        delete,
        list,
        tombstones: None,
        quota: None,
        pat_gate: None,
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        usage_meter: std::sync::Arc::new(crate::usage_meter::UsageMeter::new(None, || 0)),
    }
}

fn batch_read_request(hashes: &[String]) -> Request<Body> {
    let body = hashes
        .iter()
        .map(|hash| format!("{}\n", serde_json::json!({"hash": hash})))
        .collect::<String>();
    Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cas/{TEST_TENANT}/batch-read"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .header(axum::http::header::CONTENT_TYPE, NDJSON_CONTENT_TYPE)
        .body(Body::from(body))
        .expect("request")
}

/// A post-header R2 body stall must surface as a terminal read error, and the
/// route must release both sides of [`BatchReadLease`] after that timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_read_post_header_timeout_releases_admission_and_tenant_lease() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct PostHeaderStall {
        started: Arc<tokio::sync::Notify>,
        active: Arc<AtomicUsize>,
    }

    impl CasReadHandler for PostHeaderStall {
        fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            self.active.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            // Model an R2 GetObject whose headers arrived but whose body made
            // no progress until the body-level idle timeout fails it closed.
            std::thread::sleep(std::time::Duration::from_millis(25));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Err(CasHandlerError::Internal(
                "R2 body idle timeout".into(),
            ))
        }
    }

    let started = Arc::new(tokio::sync::Notify::new());
    let active = Arc::new(AtomicUsize::new(0));
    let state = tracking_state(Arc::new(PostHeaderStall {
        started: started.clone(),
        active: active.clone(),
    }));
    let admission = global_cas_batch_read_admission();
    let admission_before = admission.available_permits();
    assert!(admission_before > 0, "batch admission must be available");
    let request = router(state.clone()).oneshot(batch_read_request(&[
        fake_hash(b"post-header-stall"),
    ]));
    tokio::pin!(request);
    tokio::select! {
        _ = started.notified() => {}
        response = &mut request => panic!("batch read completed before body timeout: {response:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
            panic!("batch read did not enter the post-header body stall")
        }
    }
    assert_eq!(state.read_inflight.lock().expect("read tracker").get(TEST_TENANT), Some(&1));
    assert_eq!(admission.available_permits(), admission_before - 1);

    let response = tokio::time::timeout(std::time::Duration::from_secs(1), request)
        .await
        .expect("body timeout must terminate the batch")
        .expect("batch route response");
    assert!(response.status().is_server_error());
    assert_eq!(active.load(Ordering::Acquire), 0);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if state
                .read_inflight
                .lock()
                .expect("read tracker")
                .get(TEST_TENANT)
                .is_none()
                && admission.available_permits() == admission_before
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("tenant and global leases must release after body timeout");
}

/// A full structured window reaches concurrency >1 but never exceeds the
/// budget-derived eight object tasks. The request ceiling is observed by the
/// read handler, not merely checked after the response is materialised.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn batch_read_window_is_bounded_and_passes_object_ceiling() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let saw_max_bytes = Arc::new(AtomicBool::new(false));
    let read = Arc::new(BatchReadTrackingHandler {
        active: active.clone(),
        max_active: max_active.clone(),
        started: None,
        first_started: None,
        saw_max_bytes: saw_max_bytes.clone(),
        response_len: 1,
        fail_hash: None,
        read_delay: std::time::Duration::from_millis(10),
    });
    let hashes: Vec<String> = (0..BATCH_READ_FANOUT)
        .map(|i| fake_hash(format!("window-{i}").as_bytes()))
        .collect();
    let response = router(tracking_state(read))
        .oneshot(batch_read_request(&hashes))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let _ = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert!(saw_max_bytes.load(Ordering::SeqCst));
    let observed = max_active.load(Ordering::SeqCst);
    assert!(observed > 1, "structured batch reads must overlap");
    assert!(
        observed <= BATCH_READ_FANOUT,
        "fanout exceeded derived bound"
    );
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

/// A read failure aborts and drains every later task before the route returns;
/// no storage task remains active after the terminal response is selected.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn batch_read_failure_drains_all_active_tasks_before_return() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let saw_max_bytes = Arc::new(AtomicBool::new(false));
    let hashes: Vec<String> = (0..(BATCH_READ_FANOUT + 1))
        .map(|i| fake_hash(format!("failure-{i}").as_bytes()))
        .collect();
    let read = Arc::new(BatchReadTrackingHandler {
        active: active.clone(),
        max_active: max_active.clone(),
        started: None,
        first_started: None,
        saw_max_bytes: saw_max_bytes.clone(),
        response_len: 1,
        fail_hash: Some(hashes[0].clone()),
        read_delay: std::time::Duration::from_millis(10),
    });
    let response = router(tracking_state(read))
        .oneshot(batch_read_request(&hashes))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(saw_max_bytes.load(Ordering::SeqCst));
    assert!(max_active.load(Ordering::SeqCst) > 1);
    assert!(max_active.load(Ordering::SeqCst) <= BATCH_READ_FANOUT);
    assert_eq!(
        active.load(Ordering::SeqCst),
        0,
        "all spawned storage tasks must be drained before failure returns"
    );
}

/// Aggregate payload overflow is terminal, but all already-spawned reads are
/// still drained before the 413 reaches the caller.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn batch_read_overflow_drains_all_active_tasks_before_return() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let saw_max_bytes = Arc::new(AtomicBool::new(false));
    let read = Arc::new(BatchReadTrackingHandler {
        active: active.clone(),
        max_active: max_active.clone(),
        started: None,
        first_started: None,
        saw_max_bytes: saw_max_bytes.clone(),
        response_len: BATCH_MAX_BYTES / 2 + 1,
        fail_hash: None,
        read_delay: std::time::Duration::from_millis(10),
    });
    let hashes: Vec<String> = (0..(BATCH_READ_FANOUT + 1))
        .map(|i| fake_hash(format!("overflow-{i}").as_bytes()))
        .collect();
    let response = router(tracking_state(read))
        .oneshot(batch_read_request(&hashes))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(saw_max_bytes.load(Ordering::SeqCst));
    assert!(max_active.load(Ordering::SeqCst) > 1);
    assert!(max_active.load(Ordering::SeqCst) <= BATCH_READ_FANOUT);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

/// Dropping a handler future must abort its live batch tasks instead of
/// detaching the JoinHandles. Already-running synchronous reads are allowed to
/// unwind, but no queued read may start after cancellation and all active
/// storage calls must finish within the bounded test deadline.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn batch_read_cancellation_aborts_tasks_and_waits_for_unwind() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let active_tasks = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let saw_max_bytes = Arc::new(AtomicBool::new(false));
    let first_started = Arc::new(tokio::sync::Notify::new());
    let read = Arc::new(BatchReadTrackingHandler {
        active: active_tasks.clone(),
        max_active: max_active.clone(),
        started: Some(started.clone()),
        first_started: Some(first_started.clone()),
        saw_max_bytes,
        response_len: 1,
        fail_hash: None,
        read_delay: std::time::Duration::from_millis(100),
    });
    let hashes: Vec<String> = (0..(BATCH_READ_FANOUT * 2))
        .map(|i| fake_hash(format!("cancel-{i}").as_bytes()))
        .collect();
    let mut response = Box::pin(router(tracking_state(read)).oneshot(batch_read_request(&hashes)));

    tokio::select! {
        _ = first_started.notified() => {}
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
            panic!("batch handler did not start a storage read before cancellation");
        }
        _ = &mut response => panic!("batch handler completed before cancellation"),
    }
    // One worker leaves the other window handles queued behind the first
    // synchronous read. They are the cancellation-sensitive work owned by the
    // guard; removing `Drop::abort` makes their `read()` calls start later.
    let started_at_drop = started.load(Ordering::SeqCst);
    drop(response);

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            assert_eq!(
                started.load(Ordering::SeqCst),
                started_at_drop,
                "no new storage read may start after the handler future is dropped"
            );
            if active_tasks.load(Ordering::SeqCst) == 0 {
                // Give queued detached tasks a bounded scheduling opportunity;
                // a missing Drop abort must be observable in `started`.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                assert_eq!(
                    started.load(Ordering::SeqCst),
                    started_at_drop,
                    "queued storage tasks must be aborted with the handler future"
                );
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled batch storage calls must unwind within the bounded deadline");
    assert_eq!(active_tasks.load(Ordering::SeqCst), 0);
    assert!(max_active.load(Ordering::SeqCst) <= BATCH_READ_FANOUT);
}

/// Cancellation must not release the request-level envelope while a child is
/// still inside the synchronous read bridge. This pins both fairness axes: the
/// tenant slot and the one-batch global admission permit remain held until the
/// read is explicitly allowed to unwind, then both are released.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_read_cancellation_retains_leases_until_sync_read_unwinds() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Debug)]
    struct BlockingRead {
        started: Arc<tokio::sync::Notify>,
        active: Arc<AtomicUsize>,
        release: Arc<AtomicBool>,
    }

    impl CasReadHandler for BlockingRead {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            self.active.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            while !self.release.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(CasReadResponse::new(vec![0], req.hash))
        }
    }

    let started = Arc::new(tokio::sync::Notify::new());
    let active = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(AtomicBool::new(false));
    let read = Arc::new(BlockingRead {
        started: started.clone(),
        active: active.clone(),
        release: release.clone(),
    });
    let state = tracking_state(read);
    let admission = global_cas_batch_read_admission();
    let admission_before = admission.available_permits();
    let hashes: Vec<String> = (0..BATCH_READ_FANOUT)
        .map(|i| fake_hash(format!("lease-cancel-{i}").as_bytes()))
        .collect();
    let mut response = Box::pin(router(state.clone()).oneshot(batch_read_request(&hashes)));

    tokio::select! {
        _ = started.notified() => {}
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
            panic!("batch handler did not enter a synchronous read")
        }
        _ = &mut response => panic!("batch handler completed before cancellation"),
    }
    assert!(active.load(Ordering::Acquire) > 0);
    assert_eq!(admission.available_permits(), admission_before - 1);

    drop(response);
    tokio::task::yield_now().await;
    assert!(active.load(Ordering::Acquire) > 0);
    assert_eq!(
        state.read_inflight.lock().expect("read tracker").get(TEST_TENANT),
        Some(&1),
        "tenant slot must remain leased while a synchronous read is active"
    );
    assert_eq!(
        admission.available_permits(),
        admission_before - 1,
        "global batch envelope must remain leased while a synchronous read is active"
    );

    release.store(true, Ordering::Release);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if active.load(Ordering::Acquire) == 0
                && state
                    .read_inflight
                    .lock()
                    .expect("read tracker")
                    .get(TEST_TENANT)
                    .is_none()
                && admission.available_permits() == admission_before
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("leases must release after synchronous reads unwind");
}

/// B-077 HOLD: the single GET keeps its tenant slot until its response
/// stream is consumed, not merely until the handler returns.
#[tokio::test]
async fn single_read_keeps_slot_until_response_consumed() {
    let state = fixture();
    let bytes = b"stream-lifetime".to_vec();
    let hash = fake_hash(&bytes);
    state
        .write
        .write(CasWriteRequest::new(
            TEST_TENANT,
            hash.clone(),
            bytes,
            format!("anon@{TEST_TENANT}"),
            TEST_TENANT,
            0,
        ))
        .expect("seed CAS object");
    let app = router(state.clone());
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/cas/{TEST_TENANT}/{hash}"))
        .header("x-corelink-tenant-id", TEST_TENANT)
        .header(crate::scope::SCOPE_HEADER, "cas:r")
        .body(Body::empty())
        .expect("request");
    let resp = app.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    {
        let g = state.read_inflight.lock().expect("lock");
        assert_eq!(g.get(TEST_TENANT), Some(&1));
    }
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&body[..], b"stream-lifetime");
    let g = state.read_inflight.lock().expect("lock");
    assert_eq!(g.get(TEST_TENANT), None);
}

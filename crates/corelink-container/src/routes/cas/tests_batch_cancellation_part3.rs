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

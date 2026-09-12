//! BYOK background supervisors owned by the container entry point.
//!
//! Keeping these long-running workers in their own module leaves `main.rs`
//! focused on boot policy, route composition, and listener lifecycle. The
//! workers and their retry/lease budgets are unchanged; this is a structural
//! split only.

use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};

const BYOK_BACKGROUND_ENABLED_ENV: &str = "CORELINK_BYOK_BACKGROUND_WORKERS_ENABLED";

fn byok_background_enabled() -> bool {
    // Durable purge is mandatory once its D1 capability exists, so production
    // is capability-driven ON. An operator can explicitly stop background I/O
    // during incident containment with only these fail-closed values.
    std::env::var(BYOK_BACKGROUND_ENABLED_ENV).map_or(true, |value| {
        let value = value.trim();
        value != "0" && !value.eq_ignore_ascii_case("false") && !value.eq_ignore_ascii_case("off")
    })
}

fn next_byok_retry(current: Duration) -> Duration {
    current.saturating_mul(2).min(Duration::from_secs(30))
}

async fn supervise_common_byok_purge(d1: Arc<corelink_server::storage::d1_http::D1HttpClient>) {
    let mut retry = Duration::from_secs(1);
    loop {
        let config = corelink_server::storage::byok_purge_io::ByokPurgeIoConfig {
            // One claim keeps the complete DELETE + mandatory HEAD sequence
            // strictly inside the durable 120-second claim lease.
            page_limit: 1,
            operation_timeout: Duration::from_secs(15),
        };
        let driver = match tokio::time::timeout(
            Duration::from_secs(30),
            corelink_server::storage::byok_purge_io::D1R2ByokPurgeDriver::from_env(
                Arc::clone(&d1),
                config,
            ),
        )
        .await
        {
            Ok(Ok(driver)) => driver,
            Ok(Err(_)) | Err(_) => {
                warn!("BYOK common purge factory failed; retrying with bounded backoff");
                tokio::time::sleep(retry).await;
                retry = next_byok_retry(retry);
                continue;
            }
        };

        loop {
            match tokio::time::timeout(Duration::from_secs(105), driver.run_page()).await {
                Ok(Ok(report)) => {
                    if report.claimed > 0 && report.retryable == report.claimed {
                        // A page-wide retryable result commonly means rotated
                        // R2 credentials. Rebuild the clients instead of
                        // retaining a permanently failed driver.
                        warn!(
                            "BYOK common purge page was wholly retryable; rebuilding with bounded backoff"
                        );
                        tokio::time::sleep(retry).await;
                        retry = next_byok_retry(retry);
                        break;
                    }
                    retry = Duration::from_secs(1);
                    if report.claimed == 0 {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    } else {
                        tokio::task::yield_now().await;
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    // Reconstruct clients after an I/O failure so rotated runtime
                    // credentials can recover without restarting the container.
                    warn!("BYOK common purge page failed; retrying with bounded backoff");
                    tokio::time::sleep(retry).await;
                    retry = next_byok_retry(retry);
                    break;
                }
            }
        }
    }
}

#[cfg(any(
    feature = "byok-aws-real",
    feature = "byok-gcp-real",
    feature = "byok-azure-real",
    feature = "byok-vault-real"
))]
async fn supervise_production_byok_activation(
    d1: Arc<corelink_server::storage::d1_http::D1HttpClient>,
) {
    use corelink_server::storage::byok_activation_runtime::{
        supervise_activation_runtime, ActivationJobBudget, ActivationSupervisorPolicy,
    };

    let mut retry = Duration::from_secs(1);
    loop {
        let built = tokio::time::timeout(Duration::from_secs(45), async {
            let storage = corelink_server::storage::StorageEnv::from_env().ok_or(())?;
            let kms = corelink_server::byok_orchestrator::make_provider()
                .await
                .map_err(|_| ())?;
            let encryptor = Arc::new(
                corelink_server::storage::byok_backfill_crypto::build_production_backfill_encryptor(
                    Arc::clone(&d1),
                    kms,
                )
                .map_err(|_| ())?,
            );
            let store = corelink_server::storage::byok_activation_worker::build_production_activation_store_from_env(
                &storage,
                Arc::clone(&d1),
                encryptor,
                corelink_server::storage::byok_activation_worker::ActivationWorkerLimits {
                    max_object_bytes: corelink_server::routes::cas::CAS_READ_MAX_OBJECT_BYTES,
                    purge_lease: Duration::from_secs(120),
                    quarantine_after: 5,
                },
            )
            .await
            .map_err(|_| ())?;
            store.require_capability().await.map_err(|_| ())?;
            Ok::<_, ()>(store)
        })
        .await;

        let store = match built {
            Ok(Ok(store)) => store,
            Ok(Err(())) | Err(_) => {
                warn!("BYOK activation factory failed; retrying with bounded backoff");
                tokio::time::sleep(retry).await;
                retry = next_byok_retry(retry);
                continue;
            }
        };
        retry = Duration::from_secs(1);
        let factory_store = store.clone();
        let factory = Arc::new(move || {
            let store = factory_store.clone();
            async move { Ok::<_, std::convert::Infallible>(store) }
        });
        let policy = ActivationSupervisorPolicy {
            job: ActivationJobBudget {
                max_ticks: 1,
                max_items_per_tick: 1,
                claim_lease: Duration::from_secs(120),
                // A one-item copy performs at most four bounded 20-second
                // phases after reserving its target. Keep the call and the
                // complete job below both that 90-second target-write lease
                // and the 120-second durable activation claim.
                call_timeout: Duration::from_secs(85),
                wall_budget: Duration::from_secs(89),
            },
            factory_timeout: Duration::from_secs(2),
            idle_poll_interval: Duration::from_secs(5),
            initial_failure_backoff: Duration::from_secs(1),
            max_failure_backoff: Duration::from_secs(30),
        };
        let worker_id = uuid::Uuid::new_v4().to_string();
        if supervise_activation_runtime(factory, worker_id, policy)
            .await
            .is_err()
        {
            warn!("BYOK activation supervisor stopped; rebuilding with bounded backoff");
            tokio::time::sleep(retry).await;
            retry = next_byok_retry(retry);
        }
    }
}

pub(super) fn start_byok_background_tasks(
    d1: Option<&Arc<corelink_server::storage::d1_http::D1HttpClient>>,
) -> Vec<tokio::task::JoinHandle<()>> {
    if !byok_background_enabled() {
        info!(
            env = BYOK_BACKGROUND_ENABLED_ENV,
            "BYOK activation and purge supervisors are disabled"
        );
        return Vec::new();
    }
    let Some(d1) = d1 else {
        warn!("BYOK background supervisors requested without durable D1; not started");
        return Vec::new();
    };

    let purge_task = tokio::spawn(supervise_common_byok_purge(Arc::clone(d1)));
    #[cfg(any(
        feature = "byok-aws-real",
        feature = "byok-gcp-real",
        feature = "byok-azure-real",
        feature = "byok-vault-real"
    ))]
    {
        let activation_task = tokio::spawn(supervise_production_byok_activation(Arc::clone(d1)));
        vec![purge_task, activation_task]
    }
    #[cfg(not(any(
        feature = "byok-aws-real",
        feature = "byok-gcp-real",
        feature = "byok-azure-real",
        feature = "byok-vault-real"
    )))]
    {
        warn!("BYOK activation supervisor requested but no real KMS provider is compiled in");
        vec![purge_task]
    }
}

//! Bounded native supervisor for the typed BYOK activation engine.
//!
//! This module is deliberately independent from an HTTP route.  A process
//! startup path supplies the production store factory to
//! [`spawn_activation_supervisor`], while tests and one-shot administration
//! use [`run_activation_job`].  Reports contain counters and stop reasons
//! only: neither worker claims nor store errors cross this runtime boundary.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

use super::byok_activation::{ActivationTick, ByokActivationEngine, ByokActivationStore};

/// Limits applied to one finite activation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationJobBudget {
    /// Maximum engine calls made by one job.
    pub max_ticks: usize,
    /// Maximum objects copied or purged by one engine call.
    pub max_items_per_tick: usize,
    /// Durable claim lifetime requested from the store.
    pub claim_lease: Duration,
    /// Maximum wall time allowed for each engine call.
    pub call_timeout: Duration,
    /// Maximum monotonic wall time allowed for the complete job.
    pub wall_budget: Duration,
}

impl ActivationJobBudget {
    fn is_valid(self) -> bool {
        self.max_ticks > 0
            && self.max_items_per_tick > 0
            && !self.claim_lease.is_zero()
            && !self.call_timeout.is_zero()
            && !self.wall_budget.is_zero()
    }
}

/// Restart and polling policy for the long-lived supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationSupervisorPolicy {
    /// Limits inherited by every finite child job.
    pub job: ActivationJobBudget,
    /// Maximum time allowed to construct a fresh production store.
    pub factory_timeout: Duration,
    /// Delay after a healthy job reports that no work is claimable.
    pub idle_poll_interval: Duration,
    /// Delay after the first factory, job, or panic failure.
    pub initial_failure_backoff: Duration,
    /// Inclusive ceiling for exponential failure backoff.
    pub max_failure_backoff: Duration,
}

impl ActivationSupervisorPolicy {
    fn is_valid(self) -> bool {
        self.job.is_valid()
            && !self.factory_timeout.is_zero()
            && !self.idle_poll_interval.is_zero()
            && !self.initial_failure_backoff.is_zero()
            && self.max_failure_backoff >= self.initial_failure_backoff
    }
}

/// Public-safe reason why a finite activation job stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationJobStop {
    /// No durable intent was claimable.
    Idle,
    /// The claimed intent reached a terminal state.
    Terminal,
    /// The configured engine-call ceiling was reached.
    TickLimit,
    /// The monotonic wall-time budget was exhausted.
    WallBudget,
}

/// Public-safe result of a finite activation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationJobReport {
    /// Why the job yielded control to its supervisor or caller.
    pub stop: ActivationJobStop,
    /// Number of completed engine calls.
    pub ticks: usize,
    /// Total objects reported copied or purge-verified.
    pub processed: usize,
}

/// Sanitized failure returned by the job seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationJobError {
    /// A zero or internally inconsistent runtime limit was supplied.
    InvalidPolicy,
    /// One engine call exceeded its timeout.
    CallTimedOut,
    /// The durable engine rejected or failed an operation.
    EngineFailed,
}

impl core::fmt::Display for ActivationJobError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::InvalidPolicy => "invalid activation runtime policy",
            Self::CallTimedOut => "activation engine call timed out",
            Self::EngineFailed => "activation engine call failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ActivationJobError {}

/// Run a finite unit of production activation work.
///
/// The monotonic budget is checked before every call.  A call receives the
/// smaller of the remaining job budget and its configured per-call timeout,
/// so a slow store cannot extend the overall deadline.
pub async fn run_activation_job<S: ByokActivationStore>(
    engine: &ByokActivationEngine<S>,
    worker_id: &str,
    budget: ActivationJobBudget,
) -> Result<ActivationJobReport, ActivationJobError> {
    if worker_id.trim().is_empty() || !budget.is_valid() {
        return Err(ActivationJobError::InvalidPolicy);
    }

    let started = Instant::now();
    let mut ticks = 0usize;
    let mut processed = 0usize;
    while ticks < budget.max_ticks {
        let Some(remaining) = budget.wall_budget.checked_sub(started.elapsed()) else {
            return Ok(report(ActivationJobStop::WallBudget, ticks, processed));
        };
        if remaining.is_zero() {
            return Ok(report(ActivationJobStop::WallBudget, ticks, processed));
        }

        let call_budget = remaining.min(budget.call_timeout);
        let tick = match tokio::time::timeout(
            call_budget,
            engine.tick(worker_id, budget.claim_lease, budget.max_items_per_tick),
        )
        .await
        {
            Ok(result) => result.map_err(|_| ActivationJobError::EngineFailed)?,
            Err(_) if remaining <= budget.call_timeout => {
                return Ok(report(ActivationJobStop::WallBudget, ticks, processed));
            }
            Err(_) => return Err(ActivationJobError::CallTimedOut),
        };
        ticks = ticks.saturating_add(1);

        match tick {
            ActivationTick::Advanced(step) => {
                processed = processed.saturating_add(step.processed);
            }
            ActivationTick::Terminal(_) => {
                return Ok(report(ActivationJobStop::Terminal, ticks, processed));
            }
            ActivationTick::Idle => {
                return Ok(report(ActivationJobStop::Idle, ticks, processed));
            }
        }
    }

    Ok(report(ActivationJobStop::TickLimit, ticks, processed))
}

fn report(stop: ActivationJobStop, ticks: usize, processed: usize) -> ActivationJobReport {
    ActivationJobReport {
        stop,
        ticks,
        processed,
    }
}

enum FactoryAttempt<S> {
    Ready(S),
    Failed,
}

async fn bounded_factory<F, Fut, S, E>(factory: Arc<F>, timeout: Duration) -> FactoryAttempt<S>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<S, E>> + Send + 'static,
    S: Send + 'static,
    E: Send + 'static,
{
    // Construct and poll the factory inside the child task: a synchronous
    // panic while invoking the closure is therefore supervised as well.
    let mut task = tokio::spawn(async move { factory().await });
    match tokio::time::timeout(timeout, &mut task).await {
        Ok(Ok(Ok(store))) => FactoryAttempt::Ready(store),
        Ok(Ok(Err(_))) | Ok(Err(_)) => FactoryAttempt::Failed,
        Err(_) => {
            task.abort();
            let _ = task.await;
            FactoryAttempt::Failed
        }
    }
}

enum SupervisedJob {
    Complete(ActivationJobReport),
    Failed,
}

async fn bounded_child_job<S>(
    store: S,
    worker_id: String,
    budget: ActivationJobBudget,
) -> SupervisedJob
where
    S: ByokActivationStore + 'static,
{
    let outer_timeout = budget.wall_budget.saturating_add(budget.call_timeout);
    let mut task = tokio::spawn(async move {
        let engine = ByokActivationEngine::new(store);
        run_activation_job(&engine, &worker_id, budget).await
    });
    match tokio::time::timeout(outer_timeout, &mut task).await {
        Ok(Ok(Ok(report))) => SupervisedJob::Complete(report),
        Ok(Ok(Err(_))) | Ok(Err(_)) => SupervisedJob::Failed,
        Err(_) => {
            task.abort();
            let _ = task.await;
            SupervisedJob::Failed
        }
    }
}

/// Supervise production activation jobs until the owning task is cancelled.
///
/// A new store is built for every finite job.  Factory errors, engine errors,
/// timeouts, and child-task panics are reduced to a non-sensitive failure
/// class, delayed with capped exponential backoff, and retried.  Successful
/// bounded jobs reset the backoff; idle jobs use the polling interval.
pub async fn supervise_activation_runtime<F, Fut, S, E>(
    factory: Arc<F>,
    worker_id: String,
    policy: ActivationSupervisorPolicy,
) -> Result<(), ActivationJobError>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<S, E>> + Send + 'static,
    S: ByokActivationStore + 'static,
    E: Send + 'static,
{
    if worker_id.trim().is_empty() || !policy.is_valid() {
        return Err(ActivationJobError::InvalidPolicy);
    }

    let mut failure_backoff = policy.initial_failure_backoff;
    loop {
        let store = match bounded_factory(Arc::clone(&factory), policy.factory_timeout).await {
            FactoryAttempt::Ready(store) => store,
            FactoryAttempt::Failed => {
                // Deliberately omit the factory error: provider diagnostics can
                // include opaque credentials or worker capabilities.
                tracing::warn!("BYOK activation store factory failed; supervisor will retry");
                tokio::time::sleep(failure_backoff).await;
                failure_backoff = next_backoff(failure_backoff, policy.max_failure_backoff);
                continue;
            }
        };

        match bounded_child_job(store, worker_id.clone(), policy.job).await {
            SupervisedJob::Complete(report) => {
                failure_backoff = policy.initial_failure_backoff;
                if report.stop == ActivationJobStop::Idle {
                    tokio::time::sleep(policy.idle_poll_interval).await;
                } else {
                    tokio::task::yield_now().await;
                }
            }
            SupervisedJob::Failed => {
                // Includes JoinError/panic without formatting its payload.
                tracing::warn!("BYOK activation child failed; supervisor will restart it");
                tokio::time::sleep(failure_backoff).await;
                failure_backoff = next_backoff(failure_backoff, policy.max_failure_backoff);
            }
        }
    }
}

fn next_backoff(current: Duration, maximum: Duration) -> Duration {
    current.saturating_mul(2).min(maximum)
}

/// Start the production activation supervisor on the current Tokio runtime.
///
/// Dropping the returned handle detaches the supervisor; graceful process
/// shutdown should call [`JoinHandle::abort`] and await the handle.
pub fn spawn_activation_supervisor<F, Fut, S, E>(
    factory: F,
    worker_id: String,
    policy: ActivationSupervisorPolicy,
) -> Result<JoinHandle<()>, ActivationJobError>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<S, E>> + Send + 'static,
    S: ByokActivationStore + 'static,
    E: Send + 'static,
{
    if worker_id.trim().is_empty() || !policy.is_valid() {
        return Err(ActivationJobError::InvalidPolicy);
    }
    Ok(tokio::spawn(async move {
        // Inputs were validated synchronously above, so the supervisor can
        // only return if its contract changes; never surface internal state.
        if supervise_activation_runtime(Arc::new(factory), worker_id, policy)
            .await
            .is_err()
        {
            tracing::error!("BYOK activation supervisor stopped after invalid configuration");
        }
    }))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::storage::byok_activation::{
        ActivationIntent, ActivationPhase, ActivationStep, ActivationSuspension,
        PinnedActivationPolicy, WorkerClaim,
    };
    use crate::storage::byok_backfill::BackfillError;

    #[derive(Debug, Clone, Copy)]
    enum StoreBehavior {
        Idle,
        Terminal,
        Suspended,
        Pending,
    }

    #[derive(Debug)]
    struct ScriptedStore {
        behavior: StoreBehavior,
    }

    impl ScriptedStore {
        const fn new(behavior: StoreBehavior) -> Self {
            Self { behavior }
        }
    }

    #[async_trait]
    impl ByokActivationStore for ScriptedStore {
        async fn claim_next(
            &self,
            _worker_id: &str,
            _lease: Duration,
        ) -> Result<Option<ActivationIntent>, BackfillError> {
            match self.behavior {
                StoreBehavior::Idle => Ok(None),
                StoreBehavior::Terminal => Ok(Some(intent(ActivationPhase::Committed, false))),
                StoreBehavior::Suspended => Ok(Some(intent(ActivationPhase::Copy, true))),
                StoreBehavior::Pending => std::future::pending().await,
            }
        }

        async fn copy_page(
            &self,
            _intent: &ActivationIntent,
            _limit: usize,
        ) -> Result<ActivationStep, BackfillError> {
            unreachable!("suspended test intent cannot copy")
        }

        async fn publish_partial(
            &self,
            _intent: &ActivationIntent,
        ) -> Result<ActivationIntent, BackfillError> {
            unreachable!("test store cannot publish")
        }

        async fn begin_purge(
            &self,
            _intent: &ActivationIntent,
        ) -> Result<ActivationIntent, BackfillError> {
            unreachable!("test store cannot begin purge")
        }

        async fn purge_page(
            &self,
            _intent: &ActivationIntent,
            _limit: usize,
        ) -> Result<ActivationStep, BackfillError> {
            unreachable!("test store cannot purge")
        }

        async fn ready_finalize(
            &self,
            _intent: &ActivationIntent,
        ) -> Result<ActivationIntent, BackfillError> {
            unreachable!("test store cannot ready finalization")
        }

        async fn finalize(
            &self,
            _intent: &ActivationIntent,
        ) -> Result<ActivationIntent, BackfillError> {
            unreachable!("test store cannot finalize")
        }

        async fn abort_or_preempt(
            &self,
            _intent: &ActivationIntent,
            _reason: &str,
            _control_token: Option<&str>,
        ) -> Result<ActivationIntent, BackfillError> {
            unreachable!("test store cannot abort")
        }

        async fn set_suspension(
            &self,
            _intent: &ActivationIntent,
            _suspension: ActivationSuspension,
            _control_token: &str,
        ) -> Result<ActivationIntent, BackfillError> {
            unreachable!("test store cannot change suspension")
        }
    }

    fn intent(phase: ActivationPhase, suspended: bool) -> ActivationIntent {
        let terminal = phase.is_terminal();
        let published = matches!(
            phase,
            ActivationPhase::PublishedPartial
                | ActivationPhase::Purging
                | ActivationPhase::ReadyFinalize
                | ActivationPhase::Committed
        );
        ActivationIntent {
            intent_id: "intent-test".to_owned(),
            tenant_id: "tenant-test".to_owned(),
            guard_id: "guard-test".to_owned(),
            request_blake3: "a".repeat(64),
            state_version: 1,
            claim: (!terminal).then(|| {
                WorkerClaim::new(
                    "worker-test".to_owned(),
                    "opaque-test-claim".to_owned(),
                    1,
                    1,
                )
            }),
            observed_gate_epoch: 1,
            publication_gate_epoch: published.then_some(2),
            published_config_version: published.then_some(2),
            source_generation: 0,
            source_identity: None,
            target_generation: 1,
            policy: PinnedActivationPolicy {
                config_version: 1,
                mode: "byok".to_owned(),
                crypto_mode: "convergent".to_owned(),
                cmk_provider: Some("aws".to_owned()),
                cmk_key_id: Some("key-test".to_owned()),
                cmk_region: Some("us-east-1".to_owned()),
                tcs_version: 1,
                wrapped_tcs_blake3: Some("b".repeat(64)),
                policy_blake3: "c".repeat(64),
            },
            phase,
            suspension: if suspended {
                ActivationSuspension::DegradedReadOnly
            } else {
                ActivationSuspension::Active
            },
            cas_complete: published,
            ac_complete: published,
            deadline_at_ms: 10,
            deadline_expired: false,
        }
    }

    fn budget() -> ActivationJobBudget {
        ActivationJobBudget {
            max_ticks: 3,
            max_items_per_tick: 7,
            claim_lease: Duration::from_secs(1),
            call_timeout: Duration::from_millis(50),
            wall_budget: Duration::from_secs(1),
        }
    }

    fn supervisor_policy() -> ActivationSupervisorPolicy {
        ActivationSupervisorPolicy {
            job: budget(),
            factory_timeout: Duration::from_millis(50),
            idle_poll_interval: Duration::from_millis(1),
            initial_failure_backoff: Duration::from_millis(1),
            max_failure_backoff: Duration::from_millis(2),
        }
    }

    #[tokio::test]
    async fn idle_and_terminal_are_finite_public_safe_reports() {
        let idle = run_activation_job(
            &ByokActivationEngine::new(ScriptedStore::new(StoreBehavior::Idle)),
            "worker-test",
            budget(),
        )
        .await
        .expect("idle job");
        assert_eq!(idle.stop, ActivationJobStop::Idle);
        assert_eq!(idle.ticks, 1);

        let terminal = run_activation_job(
            &ByokActivationEngine::new(ScriptedStore::new(StoreBehavior::Terminal)),
            "worker-test",
            budget(),
        )
        .await
        .expect("terminal job");
        assert_eq!(terminal.stop, ActivationJobStop::Terminal);
        assert_eq!(terminal.ticks, 1);
        assert_eq!(terminal.processed, 0);
    }

    #[tokio::test]
    async fn max_ticks_bounds_a_nonterminal_suspended_intent() {
        let report = run_activation_job(
            &ByokActivationEngine::new(ScriptedStore::new(StoreBehavior::Suspended)),
            "worker-test",
            budget(),
        )
        .await
        .expect("bounded suspended job");
        assert_eq!(report.stop, ActivationJobStop::TickLimit);
        assert_eq!(report.ticks, 3);
        assert_eq!(report.processed, 0);
    }

    #[tokio::test]
    async fn monotonic_wall_budget_cancels_a_pending_store_call() {
        let mut short = budget();
        short.call_timeout = Duration::from_secs(1);
        short.wall_budget = Duration::from_millis(1);
        let report = run_activation_job(
            &ByokActivationEngine::new(ScriptedStore::new(StoreBehavior::Pending)),
            "worker-test",
            short,
        )
        .await
        .expect("wall budget is a normal yield");
        assert_eq!(report.stop, ActivationJobStop::WallBudget);
        assert_eq!(report.ticks, 0);
    }

    #[tokio::test]
    async fn invalid_policy_is_rejected_before_work_or_spawn() {
        let mut invalid = budget();
        invalid.max_ticks = 0;
        let error = run_activation_job(
            &ByokActivationEngine::new(ScriptedStore::new(StoreBehavior::Idle)),
            "worker-test",
            invalid,
        )
        .await
        .expect_err("zero ticks must fail");
        assert_eq!(error, ActivationJobError::InvalidPolicy);

        let spawn = spawn_activation_supervisor(
            || async { Ok::<_, ()>(ScriptedStore::new(StoreBehavior::Idle)) },
            " ".to_owned(),
            supervisor_policy(),
        );
        assert!(matches!(spawn, Err(ActivationJobError::InvalidPolicy)));
    }

    #[tokio::test]
    async fn supervisor_restarts_after_a_factory_panic() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&attempts);
        let handle = spawn_activation_supervisor(
            move || {
                let observed = Arc::clone(&observed);
                async move {
                    if observed.fetch_add(1, Ordering::SeqCst) == 0 {
                        panic!("synthetic factory panic");
                    }
                    Ok::<_, ()>(ScriptedStore::new(StoreBehavior::Idle))
                }
            },
            "worker-test".to_owned(),
            supervisor_policy(),
        )
        .expect("valid supervisor");

        tokio::time::timeout(Duration::from_secs(1), async {
            while attempts.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("supervisor did not restart factory");
        handle.abort();
        let _ = handle.await;
    }

    #[test]
    fn exponential_backoff_is_capped() {
        assert_eq!(
            next_backoff(Duration::from_millis(1), Duration::from_millis(4)),
            Duration::from_millis(2)
        );
        assert_eq!(
            next_backoff(Duration::from_millis(4), Duration::from_millis(4)),
            Duration::from_millis(4)
        );
    }
}

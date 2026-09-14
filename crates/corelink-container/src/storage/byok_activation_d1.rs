//! D1 lifecycle adapter for the 0121 purge-before-active protocol.

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    byok_activation::{
        ActivationIntent, ActivationPhase, ActivationStep, ActivationSuspension,
        ByokActivationStore, PinnedActivationPolicy, SourceActivationIdentity, WorkerClaim,
    },
    byok_backfill::BackfillError,
    d1_http::{D1BatchStatement, D1HttpClient, D1Row},
};

const NOW_MS: &str = "(CAST(strftime('%s','now') AS INTEGER) * 1000)";

// Historical source custody is retired only after the exact finalize capability
// has crossed the target-generation fence. The source-object predicate is
// deliberately written as a NOT EXISTS over invalid rows: an empty source
// population is therefore complete, while a source row with a mismatched
// generation/allocation/physical key or an unverified purge remains a hard
// blocker. This is the only safe shape for an empty rotation: there is no
// source row from which an ordinary EXISTS can discover the old TCS.
const RETIRE_SOURCE_HISTORY_SQL: &str = r#"
UPDATE tenant_byok_secret_history AS h
   SET tcs_wrapped=NULL,retired_at_ms={NOW_MS}
 WHERE h.tenant_id=?1
   AND h.tcs_wrapped IS NOT NULL
   AND h.retired_at_ms IS NULL
   -- tenant_byok_secret stores only cmk_key_id; the config join is
   -- intentional and completes the current-key identity with provider/region.
   AND NOT EXISTS (
       SELECT 1
         FROM tenant_byok_secret current_secret
         JOIN tenant_byok_config current_config
           ON current_config.tenant_id=current_secret.tenant_id
        WHERE current_secret.tenant_id=h.tenant_id
          AND current_secret.tcs_version=h.tcs_version
          AND current_secret.cmk_key_id=h.cmk_key_id
          AND current_secret.tcs_wrapped IS NOT NULL
          AND current_config.cmk_provider=h.cmk_provider
          AND current_config.cmk_key_id=h.cmk_key_id
          AND current_config.cmk_region=h.cmk_region)
   AND EXISTS (
       SELECT 1
         FROM byok_activation_intent a
         JOIN byok_activation_guard f
           ON f.guard_id=a.guard_id AND f.tenant_id=a.tenant_id
         JOIN byok_transition_fence tf
           ON tf.token=f.transition_token
          AND tf.tenant_id=a.tenant_id
          AND tf.epoch=f.transition_epoch
         JOIN byok_activation_operation_guard og
           ON og.intent_id=a.intent_id
         JOIN byok_tenant_gate g
           ON g.tenant_id=a.tenant_id
        WHERE a.intent_id=?2
          AND a.tenant_id=h.tenant_id
          AND a.phase='ready_finalize'
          AND a.source_generation>0
          AND h.tcs_version=a.source_tcs_version
          AND h.cmk_provider=a.source_cmk_provider
          AND h.cmk_key_id=a.source_cmk_key_id
          AND h.cmk_region=a.source_cmk_region
          AND og.operation_token=?3
          AND og.action='finalize'
          AND og.expected_state_version=?4
          AND f.outcome='active'
          AND tf.outcome='active'
          AND tf.epoch=f.transition_epoch
          AND tf.observed_generation=a.target_generation
          AND tf.observed_gate_epoch=a.publication_gate_epoch
          AND g.current_generation=a.target_generation
          AND g.gate_epoch=a.publication_gate_epoch+1
          AND NOT EXISTS (
              SELECT 1
                FROM byok_activation_source_object s
               WHERE s.intent_id=a.intent_id
                 AND NOT (
                     s.tenant_id=a.tenant_id
                 AND s.source_generation=a.source_generation
                 AND s.source_config_version IS a.source_config_version
                 AND s.source_cmk_provider IS a.source_cmk_provider
                 AND s.source_cmk_key_id IS a.source_cmk_key_id
                 AND s.source_cmk_region IS a.source_cmk_region
                 AND s.source_tcs_version IS a.source_tcs_version
                 AND s.copy_state='purged'
                 AND EXISTS (
                     SELECT 1
                       FROM byok_object_purge_item p
                       JOIN byok_object_purge_cause pc
                         ON pc.purge_id=p.purge_id
                        AND pc.cause_kind='activation_source'
                        AND pc.cause_id=s.intent_id
                      WHERE p.intent_id=s.intent_id
                        AND p.tenant_id=s.tenant_id
                        AND p.object_kind=s.object_kind
                        AND p.logical_key=s.logical_key
                        AND p.generation=s.source_generation
                        AND p.allocation_id IS s.source_allocation_id
                        AND p.physical_key=s.source_physical_key
                        AND p.reason='activation_source'
                        AND p.state='verified')))
          AND NOT EXISTS (
              SELECT 1
                FROM byok_activation_source_object pending
               WHERE pending.tenant_id=a.tenant_id
                 AND pending.intent_id<>a.intent_id
                 AND pending.source_generation=a.source_generation
                 AND pending.source_config_version IS a.source_config_version
                 AND pending.source_cmk_provider IS a.source_cmk_provider
                 AND pending.source_cmk_key_id IS a.source_cmk_key_id
                 AND pending.source_cmk_region IS a.source_cmk_region
                 AND pending.source_tcs_version IS a.source_tcs_version
                 AND NOT (
                     pending.copy_state='purged'
                 AND EXISTS (
                     SELECT 1
                       FROM byok_activation_intent prior
                       JOIN byok_activation_guard prior_guard
                         ON prior_guard.guard_id=prior.guard_id
                        AND prior_guard.tenant_id=prior.tenant_id
                       JOIN byok_transition_fence prior_fence
                         ON prior_fence.token=prior_guard.transition_token
                        AND prior_fence.tenant_id=prior.tenant_id
                        AND prior_fence.epoch=prior_guard.transition_epoch
                       JOIN byok_object_purge_item p
                         ON p.intent_id=prior.intent_id
                        AND p.tenant_id=prior.tenant_id
                        AND p.object_kind=pending.object_kind
                        AND p.logical_key=pending.logical_key
                        AND p.generation=pending.source_generation
                        AND p.allocation_id IS pending.source_allocation_id
                        AND p.physical_key=pending.source_physical_key
                       JOIN byok_object_purge_cause pc
                         ON pc.purge_id=p.purge_id
                        AND pc.cause_kind='activation_source'
                        AND pc.cause_id=prior.intent_id
                      WHERE prior.intent_id=pending.intent_id
                        AND prior.tenant_id=pending.tenant_id
                        AND prior.phase='committed'
                        AND prior.source_generation=pending.source_generation
                        AND prior.source_config_version IS pending.source_config_version
                        AND prior.source_cmk_provider IS pending.source_cmk_provider
                        AND prior.source_cmk_key_id IS pending.source_cmk_key_id
                        AND prior.source_cmk_region IS pending.source_cmk_region
                        AND prior.source_tcs_version IS pending.source_tcs_version
                        AND prior.publication_gate_epoch IS NOT NULL
                        AND prior_guard.outcome='committed'
                        AND prior_fence.outcome='committed'
                        AND prior_fence.observed_generation=prior.target_generation
                        AND prior_fence.observed_gate_epoch=prior.publication_gate_epoch
                        AND p.reason='activation_source'
                        AND p.state='verified')))
              )
"#;

fn retire_source_history_statement(
    intent: &ActivationIntent,
    operation_token: &str,
) -> ActivationD1Statement {
    ActivationD1Statement::new(
        RETIRE_SOURCE_HISTORY_SQL.replace("{NOW_MS}", NOW_MS),
        vec![
            json!(intent.tenant_id),
            json!(intent.intent_id),
            json!(operation_token),
            json!(intent.state_version),
        ],
    )
}

/// One internal statement in an atomic D1 lifecycle batch.
#[derive(Debug, Clone)]
pub struct ActivationD1Statement {
    sql: String,
    params: Vec<Value>,
}

impl ActivationD1Statement {
    fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }
}

/// Narrow D1 seam retained for hermetic lifecycle tests.
#[async_trait]
pub trait ActivationD1Client: Send + Sync + fmt::Debug {
    /// Execute one parameterized query.
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String>;
    /// Execute one rollback-on-error transactional batch.
    async fn batch(
        &self,
        statements: Vec<ActivationD1Statement>,
    ) -> Result<Vec<Vec<D1Row>>, String>;
}

#[async_trait]
impl ActivationD1Client for D1HttpClient {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<D1Row>, String> {
        D1HttpClient::query(self, sql, params).await
    }

    async fn batch(
        &self,
        statements: Vec<ActivationD1Statement>,
    ) -> Result<Vec<Vec<D1Row>>, String> {
        D1HttpClient::batch(
            self,
            statements
                .into_iter()
                .map(|statement| D1BatchStatement::new(statement.sql, statement.params))
                .collect(),
        )
        .await
        .map_err(|error| match error.statement {
            Some(index) => format!("D1 activation batch statement {index}: {}", error.message),
            None => format!("D1 activation batch: {}", error.message),
        })
    }
}

/// Bounded R2/crypto work performed between guarded lifecycle transitions.
#[async_trait]
pub trait ActivationObjectWorker: Send + Sync + fmt::Debug {
    /// Copy and checkpoint at most `limit` exact source objects.
    async fn copy_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError>;

    /// Delete, HEAD-verify and checkpoint at most `limit` purge items.
    async fn purge_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError>;
}

/// Production D1 lifecycle adapter. Object I/O is supplied by the concrete
/// bounded worker so D1 capability transactions remain independently testable.
#[derive(Debug)]
pub struct D1ByokActivationStore<D, W> {
    d1: Arc<D>,
    worker: Arc<W>,
}

impl<D, W> Clone for D1ByokActivationStore<D, W> {
    fn clone(&self) -> Self {
        Self {
            d1: Arc::clone(&self.d1),
            worker: Arc::clone(&self.worker),
        }
    }
}

impl<D, W> D1ByokActivationStore<D, W> {
    /// Construct after the caller has verified the 0121 capability marker.
    #[must_use]
    pub fn new(d1: Arc<D>, worker: Arc<W>) -> Self {
        Self { d1, worker }
    }
}

impl<D: ActivationD1Client, W> D1ByokActivationStore<D, W> {
    /// Fail closed unless the migration completion marker is exact.
    pub async fn require_capability(&self) -> Result<(), BackfillError> {
        let rows = self
            .d1
            .query(
                "SELECT singleton,schema_version FROM byok_0121_capability \
                 WHERE singleton=1 AND schema_version=2 LIMIT 1",
                &[],
            )
            .await
            .map_err(store_error)?;
        if rows.first().is_none_or(|row| {
            integer(row, "singleton") != Ok(1) || integer(row, "schema_version") != Ok(2)
        }) {
            return Err(BackfillError::Store(
                "0121 activation capability is absent or malformed".to_owned(),
            ));
        }
        Ok(())
    }

    async fn guarded_transition(
        &self,
        intent: &ActivationIntent,
        action: &str,
        statements: Vec<ActivationD1Statement>,
        expected_phase: ActivationPhase,
    ) -> Result<ActivationIntent, BackfillError> {
        let claim = intent.claim.as_ref().ok_or_else(|| {
            BackfillError::Conflict("terminal activation has no worker capability".to_owned())
        })?;
        let operation_token = Uuid::new_v4().to_string();
        let mut batch = Vec::with_capacity(statements.len() + 2);
        batch.push(operation_guard(
            &operation_token,
            intent,
            claim.token(),
            action,
            None,
        ));
        batch.extend(statements);
        batch.push(postcondition(&operation_token, intent, expected_phase));
        let results = self.d1.batch(batch).await.map_err(store_error)?;
        if results.first().is_none_or(Vec::is_empty) || results.last().is_none_or(Vec::is_empty)
        {
            return Err(BackfillError::Conflict(
                "guarded activation transition lost its exact capability".to_owned(),
            ));
        }
        self.load(&intent.intent_id).await
    }

    async fn load(&self, intent_id: &str) -> Result<ActivationIntent, BackfillError> {
        let rows = self
            .d1
            .query(
                &format!("{} WHERE a.intent_id=?1 LIMIT 1", intent_select()),
                &[json!(intent_id)],
            )
            .await
            .map_err(store_error)?;
        rows.first()
            .ok_or_else(|| BackfillError::Conflict("activation intent disappeared".to_owned()))
            .and_then(parse_intent)
    }

    async fn prepare_claimable_transition(
        &self,
        worker_id: &str,
        lease_ms: i64,
    ) -> Result<Option<String>, BackfillError> {
        let candidates = self
            .d1
            .query(
                &format!(
                    "SELECT a.guard_id FROM byok_activation_intent a \
                 WHERE a.phase IN ('copy','published_partial','purging','ready_finalize') \
                  AND a.suspension_state='active' AND (a.claim_token IS NULL \
                   OR a.claim_owner=?1 OR a.claim_expires_at_ms<={NOW_MS}) \
                 ORDER BY a.checkpointed_at_ms,a.tenant_id,a.intent_id LIMIT 1"
                ),
                &[json!(worker_id)],
            )
            .await
            .map_err(store_error)?;
        let Some(guard_id) = candidates
            .first()
            .and_then(|row| optional_string(row, "guard_id"))
        else {
            return Ok(None);
        };
        let transition_token = Uuid::new_v4().to_string();
        let capability_token = Uuid::new_v4().to_string();
        let assertion_token = Uuid::new_v4().to_string();
        let classify = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_guard SET outcome='expired',completed_at_ms={NOW_MS} \
             WHERE guard_id=?1 AND outcome='active' AND (expires_at_ms<={NOW_MS} OR EXISTS \
              (SELECT 1 FROM byok_transition_fence tf WHERE tf.token=transition_token \
               AND (tf.outcome<>'active' OR tf.expires_at_ms<={NOW_MS})))"
            ),
            vec![json!(guard_id)],
        );
        let fence = ActivationD1Statement::new(
            format!(
                "INSERT INTO byok_transition_fence \
             (token,tenant_id,epoch,observed_gate_epoch,observed_generation, \
              observed_config_version,observed_config_state,observed_byok_status, \
              acquired_at_ms,expires_at_ms) \
             SELECT ?1,a.tenant_id,(SELECT COALESCE(MAX(epoch),0)+1 FROM byok_transition_fence \
              WHERE tenant_id=a.tenant_id),g.gate_epoch,g.current_generation,c.config_version, \
              c.state,t.byok_status,{NOW_MS},{NOW_MS}+?3 \
             FROM byok_activation_intent a JOIN byok_activation_guard f ON f.guard_id=a.guard_id \
              JOIN byok_tenant_gate g ON g.tenant_id=a.tenant_id \
              JOIN tenant_byok_config c ON c.tenant_id=a.tenant_id \
              JOIN tenant t ON t.tenant_id=a.tenant_id \
             WHERE a.guard_id=?2 AND f.outcome='expired' \
              AND NOT EXISTS (SELECT 1 FROM byok_data_intent d WHERE d.tenant_id=a.tenant_id \
               AND d.outcome='active' AND d.expires_at_ms>{NOW_MS}) \
              AND NOT EXISTS (SELECT 1 FROM byok_transition_fence other \
               WHERE other.tenant_id=a.tenant_id AND other.token<>f.transition_token \
                AND other.outcome='active' AND other.expires_at_ms>{NOW_MS}) RETURNING epoch"
            ),
            vec![json!(transition_token), json!(guard_id), json!(lease_ms)],
        );
        let rebind = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_guard SET transition_token=?1, \
             transition_epoch=(SELECT epoch FROM byok_transition_fence WHERE token=?1), \
             capability_token=?2,epoch=epoch+1,outcome='active',completed_at_ms=NULL, \
             acquired_at_ms={NOW_MS},expires_at_ms={NOW_MS}+?4 WHERE guard_id=?3 \
              AND outcome='expired' AND EXISTS (SELECT 1 FROM byok_transition_fence tf \
               WHERE tf.token=?1 AND tf.outcome='active') RETURNING guard_id"
            ),
            vec![
                json!(transition_token),
                json!(capability_token),
                json!(guard_id),
                json!(lease_ms),
            ],
        );
        let renew_fence = ActivationD1Statement::new(
            format!(
                "UPDATE byok_transition_fence SET expires_at_ms=MAX(expires_at_ms+1,{NOW_MS}+?2) \
             WHERE token=(SELECT transition_token FROM byok_activation_guard WHERE guard_id=?1) \
              AND outcome='active' AND expires_at_ms>{NOW_MS}"
            ),
            vec![json!(guard_id), json!(lease_ms)],
        );
        let renew_guard = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_guard SET expires_at_ms=MAX(expires_at_ms+1,{NOW_MS}+?2) \
             WHERE guard_id=?1 AND outcome='active' AND expires_at_ms>{NOW_MS}"
            ),
            vec![json!(guard_id), json!(lease_ms)],
        );
        let assertion = ActivationD1Statement::new(
            "INSERT INTO byok_activation_transition_assertion \
             (assertion_token,guard_id,checked_at_ms) VALUES (?1,?2,0) RETURNING assertion_token",
            vec![json!(assertion_token), json!(guard_id)],
        );
        self.d1
            .batch(vec![
                classify,
                fence,
                rebind,
                renew_fence,
                renew_guard,
                assertion,
            ])
            .await
            .map_err(store_error)?;
        Ok(Some(guard_id))
    }
}

#[async_trait]
impl<D, W> ByokActivationStore for D1ByokActivationStore<D, W>
where
    D: ActivationD1Client,
    W: ActivationObjectWorker,
{
    async fn claim_next(
        &self,
        worker_id: &str,
        lease: Duration,
    ) -> Result<Option<ActivationIntent>, BackfillError> {
        self.require_capability().await?;
        let lease_ms = i64::try_from(lease.as_millis()).map_err(|_| {
            BackfillError::InvalidRequest("activation claim lease overflow".to_owned())
        })?;
        if worker_id.is_empty() || lease_ms <= 0 {
            return Err(BackfillError::InvalidRequest(
                "worker and positive claim lease are required".to_owned(),
            ));
        }
        let Some(guard_id) = self
            .prepare_claimable_transition(worker_id, lease_ms)
            .await?
        else {
            return Ok(None);
        };
        let token = Uuid::new_v4().to_string();
        let sql = format!(
            "UPDATE byok_activation_intent SET claim_owner=?1,claim_token=?2, \
             claim_epoch=claim_epoch+1,claim_expires_at_ms={NOW_MS}+?3, \
             state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
             WHERE intent_id=(SELECT a.intent_id FROM byok_activation_intent a \
              JOIN byok_activation_guard f ON f.guard_id=a.guard_id \
              JOIN byok_transition_fence tf ON tf.token=f.transition_token \
              WHERE a.phase IN ('copy','published_partial','purging','ready_finalize') \
               AND a.suspension_state='active' AND (a.claim_token IS NULL \
                    OR a.claim_owner=?1 OR a.claim_expires_at_ms<={NOW_MS}) \
               AND f.outcome='active' AND f.expires_at_ms>{NOW_MS} \
               AND tf.outcome='active' AND tf.expires_at_ms>{NOW_MS} AND a.guard_id=?4 \
              ORDER BY a.checkpointed_at_ms,a.tenant_id,a.intent_id LIMIT 1) \
             RETURNING {}",
            returning_columns()
        );
        let rows = self
            .d1
            .query(
                &sql,
                &[
                    json!(worker_id),
                    json!(token),
                    json!(lease_ms),
                    json!(guard_id),
                ],
            )
            .await
            .map_err(store_error)?;
        rows.first().map(parse_intent).transpose()
    }

    async fn copy_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError> {
        self.worker.copy_page(intent, limit).await
    }

    async fn publish_partial(
        &self,
        intent: &ActivationIntent,
    ) -> Result<ActivationIntent, BackfillError> {
        let identity = identity(intent);
        let publish_generation = ActivationD1Statement::new(
            format!(
                "UPDATE byok_logical_object_generation SET outcome='published', \
             gate_epoch=?5+1,completed_at_ms={NOW_MS} WHERE tenant_id=?2 \
             AND backfill_run_id=?1 AND generation=?4 AND gate_epoch=?5 \
             AND outcome='allocated'"
            ),
            identity.clone(),
        );
        let publication = ActivationD1Statement::new(
            format!("INSERT INTO byok_logical_object_publication \
             (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms) \
             SELECT tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,{NOW_MS} \
             FROM byok_logical_object_generation WHERE tenant_id=?2 AND backfill_run_id=?1 \
              AND generation=?4 AND gate_epoch=?5+1 AND outcome='published' \
             ON CONFLICT(tenant_id,object_kind,logical_key) DO UPDATE SET \
              generation=excluded.generation,allocation_id=excluded.allocation_id, \
              physical_key=excluded.physical_key,gate_epoch=excluded.gate_epoch, \
              size_bytes=excluded.size_bytes,published_at_ms=excluded.published_at_ms \
             WHERE byok_logical_object_publication.generation=?3"),
            identity.clone(),
        );
        let gate = ActivationD1Statement::new(
            "UPDATE byok_tenant_gate SET current_generation=?4,gate_epoch=?5+1 \
             WHERE tenant_id=?2 AND current_generation=?3 AND gate_epoch=?5 \
              AND NOT EXISTS (SELECT 1 FROM byok_logical_object_generation x \
               LEFT JOIN byok_logical_object_publication p ON p.tenant_id=x.tenant_id \
                AND p.object_kind=x.object_kind AND p.logical_key=x.logical_key \
               WHERE x.tenant_id=?2 AND x.backfill_run_id=?1 AND x.generation=?4 \
                AND (x.outcome<>'published' OR p.tenant_id IS NULL \
                 OR p.generation<>x.generation OR p.allocation_id<>x.allocation_id \
                 OR p.physical_key<>x.physical_key OR p.size_bytes<>x.size_bytes \
                 OR p.gate_epoch<>?5+1)) \
              AND NOT EXISTS (SELECT 1 FROM byok_logical_object_generation x \
               LEFT JOIN byok_activation_source_object s ON s.intent_id=x.backfill_run_id \
                AND s.object_kind=x.object_kind AND s.logical_key=x.logical_key \
               WHERE x.tenant_id=?2 AND x.backfill_run_id=?1 AND x.generation=?4 \
                AND (s.intent_id IS NULL OR s.target_allocation_id<>x.allocation_id \
                 OR s.target_physical_key<>x.physical_key OR s.plaintext_size<>x.size_bytes)) \
              AND NOT EXISTS (SELECT 1 FROM byok_activation_source_object s \
               LEFT JOIN byok_logical_object_generation x ON x.tenant_id=s.tenant_id \
                AND x.object_kind=s.object_kind AND x.logical_key=s.logical_key \
                AND x.generation=?4 AND x.allocation_id=s.target_allocation_id \
                AND x.physical_key=s.target_physical_key \
               WHERE s.intent_id=?1 AND (x.tenant_id IS NULL OR x.outcome<>'published' \
                OR x.size_bytes<>s.plaintext_size OR x.backfill_run_id<>?1)) \
              RETURNING gate_epoch",
            identity.clone(),
        );
        let config = ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_config SET state='partial',config_version=config_version+1, \
             updated_at_ms={NOW_MS} WHERE tenant_id=?2 AND state='pending' AND config_version=?6 \
              AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?2 \
               AND g.current_generation=?4 AND g.gate_epoch=?5+1) RETURNING config_version"
            ),
            identity.clone(),
        );
        let sources = ActivationD1Statement::new(
            "UPDATE byok_activation_source_object SET copy_state='published' \
             WHERE intent_id=?1 AND copy_state='staged'",
            identity.clone(),
        );
        let finish = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_intent SET phase='published_partial', \
             publication_gate_epoch=?5+1,published_config_version=?6+1, \
             state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
             WHERE intent_id=?1 AND tenant_id=?2 AND phase='copy' AND source_generation=?3 \
              AND target_generation=?4 AND observed_gate_epoch=?5 AND config_version=?6 \
              AND cas_complete=1 AND ac_complete=1 \
              AND NOT EXISTS (SELECT 1 FROM byok_activation_source_object s \
               WHERE s.intent_id=?1 AND s.copy_state<>'published') \
              AND EXISTS (SELECT 1 FROM tenant_byok_config c WHERE c.tenant_id=?2 \
               AND c.state='partial' AND c.config_version=?6+1) RETURNING intent_id"
            ),
            identity,
        );
        let transition_token = Uuid::new_v4().to_string();
        let capability_token = Uuid::new_v4().to_string();
        let new_fence = ActivationD1Statement::new(
            format!(
                "INSERT INTO byok_transition_fence \
             (token,tenant_id,epoch,observed_gate_epoch,observed_generation, \
              observed_config_version,observed_config_state,observed_byok_status, \
              acquired_at_ms,expires_at_ms) \
             SELECT ?1,a.tenant_id,(SELECT COALESCE(MAX(epoch),0)+1 FROM byok_transition_fence \
              WHERE tenant_id=a.tenant_id),a.publication_gate_epoch,a.target_generation, \
              a.published_config_version,'partial',t.byok_status,{NOW_MS},{NOW_MS}+120000 \
             FROM byok_activation_intent a JOIN tenant t ON t.tenant_id=a.tenant_id \
              JOIN byok_activation_guard f ON f.guard_id=a.guard_id \
             WHERE a.intent_id=?2 AND a.phase='published_partial' \
              AND NOT EXISTS (SELECT 1 FROM byok_data_intent d WHERE d.tenant_id=a.tenant_id \
               AND d.outcome='active' AND d.expires_at_ms>{NOW_MS}) \
              AND NOT EXISTS (SELECT 1 FROM byok_transition_fence other \
               WHERE other.tenant_id=a.tenant_id AND other.token<>f.transition_token \
                AND other.outcome='active' AND other.expires_at_ms>{NOW_MS}) RETURNING epoch"
            ),
            vec![json!(transition_token), json!(intent.intent_id)],
        );
        let rebind = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_guard SET transition_token=?1, \
             transition_epoch=(SELECT epoch FROM byok_transition_fence WHERE token=?1), \
             capability_token=?2,epoch=epoch+1,acquired_at_ms={NOW_MS}, \
             expires_at_ms={NOW_MS}+120000 WHERE guard_id=?3 AND outcome='active' \
             RETURNING guard_id"
            ),
            vec![
                json!(transition_token),
                json!(capability_token),
                json!(intent.guard_id),
            ],
        );
        self.guarded_transition(
            intent,
            "publish",
            vec![
                publish_generation,
                publication,
                gate,
                config,
                sources,
                finish,
                new_fence,
                rebind,
            ],
            ActivationPhase::PublishedPartial,
        )
        .await
    }

    async fn begin_purge(
        &self,
        intent: &ActivationIntent,
    ) -> Result<ActivationIntent, BackfillError> {
        let params = vec![json!(intent.intent_id), json!(intent.tenant_id)];
        let enqueue = ActivationD1Statement::new(
            format!("INSERT OR IGNORE INTO byok_object_purge_item \
             (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id, \
              physical_key,object_size,object_blake3,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) \
             SELECT lower(hex(randomblob(16))),s.tenant_id,s.intent_id,s.object_kind,s.logical_key, \
              s.source_generation,s.source_allocation_id,s.source_physical_key,s.source_stored_size, \
              s.source_blake3,s.source_crypto_mode, \
              'activation_source','pending',{NOW_MS},{NOW_MS} \
             FROM byok_activation_source_object s WHERE s.intent_id=?1 AND s.tenant_id=?2 \
              AND s.copy_state='published'"),
            params.clone(),
        );
        let causes = ActivationD1Statement::new(
            format!(
                "INSERT OR IGNORE INTO byok_object_purge_cause \
             (purge_id,cause_kind,cause_id,created_at_ms) \
             SELECT p.purge_id,'activation_source',s.intent_id,{NOW_MS} \
             FROM byok_activation_source_object s JOIN byok_object_purge_item p \
              ON p.tenant_id=s.tenant_id AND p.physical_key=s.source_physical_key \
              AND p.generation=s.source_generation \
              AND p.allocation_id IS s.source_allocation_id \
             WHERE s.intent_id=?1 AND s.tenant_id=?2"
            ),
            params.clone(),
        );
        let sources = ActivationD1Statement::new(
            "UPDATE byok_activation_source_object SET copy_state='purge_queued' \
             WHERE intent_id=?1 AND tenant_id=?2 AND copy_state='published'",
            params.clone(),
        );
        let finish = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_intent SET phase='purging', \
             state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
             WHERE intent_id=?1 AND tenant_id=?2 AND phase='published_partial' \
              AND NOT EXISTS (SELECT 1 FROM byok_activation_source_object s \
               LEFT JOIN byok_object_purge_item p ON p.tenant_id=s.tenant_id \
                AND p.physical_key=s.source_physical_key \
               LEFT JOIN byok_object_purge_cause c ON c.purge_id=p.purge_id \
                AND c.cause_kind='activation_source' AND c.cause_id=s.intent_id \
               WHERE s.intent_id=?1 AND (s.copy_state<>'purge_queued' \
                OR p.purge_id IS NULL OR c.purge_id IS NULL)) RETURNING intent_id"
            ),
            params,
        );
        self.guarded_transition(
            intent,
            "begin_purge",
            vec![enqueue, causes, sources, finish],
            ActivationPhase::Purging,
        )
        .await
    }

    async fn purge_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError> {
        self.worker.purge_page(intent, limit).await
    }

    async fn ready_finalize(
        &self,
        intent: &ActivationIntent,
    ) -> Result<ActivationIntent, BackfillError> {
        let finish = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_intent SET phase='ready_finalize', \
             state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
             WHERE intent_id=?1 AND phase='purging' AND NOT EXISTS \
               (SELECT 1 FROM byok_object_purge_cause pc \
                JOIN byok_object_purge_item p ON p.purge_id=pc.purge_id \
                WHERE pc.cause_kind='activation_source' AND pc.cause_id=?1 \
                 AND p.state<>'verified') RETURNING intent_id"
            ),
            vec![json!(intent.intent_id)],
        );
        self.guarded_transition(
            intent,
            "purge",
            vec![finish],
            ActivationPhase::ReadyFinalize,
        )
        .await
    }

    async fn finalize(&self, intent: &ActivationIntent) -> Result<ActivationIntent, BackfillError> {
        let claim = claim(intent)?;
        let operation_token = Uuid::new_v4().to_string();
        let config = ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_config SET state='active',config_version=config_version+1, \
             updated_at_ms={NOW_MS} WHERE tenant_id=?1 AND state='partial' \
              AND config_version=?2 AND NOT EXISTS (SELECT 1 FROM byok_object_purge_cause pc \
               JOIN byok_object_purge_item p ON p.purge_id=pc.purge_id \
               WHERE pc.cause_kind='activation_source' AND pc.cause_id=?3 \
                AND p.state<>'verified') RETURNING config_version"
            ),
            vec![
                json!(intent.tenant_id),
                json!(intent.published_config_version),
                json!(intent.intent_id),
            ],
        );
        let retire_source_history = retire_source_history_statement(intent, &operation_token);
        let finish = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_intent SET phase='committed',completed_at_ms={NOW_MS}, \
             failure_reason=NULL,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL, \
             state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
             WHERE intent_id=?1 AND phase='ready_finalize' AND state_version=?2 \
              AND EXISTS (SELECT 1 FROM tenant_byok_config c WHERE c.tenant_id=?3 \
               AND c.state='active' AND c.config_version=?4+1) RETURNING intent_id"
            ),
            vec![
                json!(intent.intent_id),
                json!(intent.state_version),
                json!(intent.tenant_id),
                json!(intent.published_config_version),
            ],
        );
        let results = self
            .d1
            .batch(vec![
                operation_guard(&operation_token, intent, claim.token(), "finalize", None),
                ActivationD1Statement::new(
                    "UPDATE byok_tenant_gate SET gate_epoch=gate_epoch+1 WHERE tenant_id=?1 \
                     AND current_generation=?2 AND gate_epoch=?3 RETURNING gate_epoch",
                    vec![
                        json!(intent.tenant_id),
                        json!(intent.target_generation),
                        json!(intent.publication_gate_epoch),
                    ],
                ),
                retire_source_history,
                config,
                finish,
                postcondition(&operation_token, intent, ActivationPhase::Committed),
            ])
            .await
            .map_err(store_error)?;
        ensure_edges(&results, "finalize")?;
        self.load(&intent.intent_id).await
    }

    async fn abort_or_preempt(
        &self,
        intent: &ActivationIntent,
        reason: &str,
        control_token: Option<&str>,
    ) -> Result<ActivationIntent, BackfillError> {
        if reason.is_empty() {
            return Err(BackfillError::InvalidRequest(
                "activation termination reason is required".to_owned(),
            ));
        }
        let preempt = control_token.is_some();
        let action = if preempt { "preempt" } else { "abort" };
        let operation_token = Uuid::new_v4().to_string();
        let claim_token = intent.claim.as_ref().map(WorkerClaim::token).unwrap_or("");
        // An expired copy has not published a target generation yet.  A
        // generation-zero activation therefore returns to the empty/inactive
        // state, while a rotated activation must restore the exact active
        // source custody it displaced.  Only an authorized preemption is
        // allowed to shred the tenant.  Keeping this distinction here is
        // essential: 0121's abort postcondition rejects an inactive config
        // for a rotated source, and a failed postcondition rolls the whole
        // D1 batch back (leaving the intent claimable forever).
        let config_state = abort_config_state(intent.source_generation, preempt);
        let expire_data = ActivationD1Statement::new(
            format!(
                "UPDATE byok_data_intent SET outcome='expired',completed_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND outcome='active' AND ?2=1"
            ),
            vec![json!(intent.tenant_id), json!(preempt)],
        );
        let enqueue_sources = ActivationD1Statement::new(
            format!(
                "INSERT OR IGNORE INTO byok_object_purge_item \
             (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id, \
              physical_key,object_size,object_blake3,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) \
             SELECT lower(hex(randomblob(16))),s.tenant_id,s.intent_id,s.object_kind,s.logical_key, \
              s.source_generation,s.source_allocation_id,s.source_physical_key,s.source_stored_size, \
              s.source_blake3,s.source_crypto_mode,'activation_source','pending',{NOW_MS},{NOW_MS} \
             FROM byok_activation_source_object s JOIN byok_activation_intent a \
              ON a.intent_id=s.intent_id WHERE s.intent_id=?1 AND ?2=1 \
              AND a.phase IN ('published_partial','purging','ready_finalize')"
            ),
            vec![json!(intent.intent_id), json!(preempt)],
        );
        let source_causes = ActivationD1Statement::new(
            format!(
                "INSERT OR IGNORE INTO byok_object_purge_cause \
             (purge_id,cause_kind,cause_id,created_at_ms) \
             SELECT p.purge_id,'activation_source',s.intent_id,{NOW_MS} \
             FROM byok_activation_source_object s JOIN byok_object_purge_item p \
              ON p.tenant_id=s.tenant_id AND p.physical_key=s.source_physical_key \
             JOIN byok_activation_intent a ON a.intent_id=s.intent_id \
             WHERE s.intent_id=?1 AND ?2=1 \
              AND a.phase IN ('published_partial','purging','ready_finalize')"
            ),
            vec![json!(intent.intent_id), json!(preempt)],
        );
        let mark_sources = ActivationD1Statement::new(
            "UPDATE byok_activation_source_object SET copy_state='purge_queued' \
             WHERE intent_id=?1 AND ?2=1 AND copy_state IN ('published','purge_queued') \
              AND EXISTS (SELECT 1 FROM byok_activation_intent a WHERE a.intent_id=?1 \
               AND a.phase IN ('published_partial','purging','ready_finalize'))",
            vec![json!(intent.intent_id), json!(preempt)],
        );
        let enqueue_targets = ActivationD1Statement::new(
            format!("INSERT OR IGNORE INTO byok_object_purge_item \
             (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id, \
              physical_key,object_size,object_blake3,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) \
             SELECT lower(hex(randomblob(16))),s.tenant_id,s.intent_id,s.object_kind,s.logical_key, \
              a.target_generation,s.target_allocation_id,s.target_physical_key, \
              x.ciphertext_size,x.ciphertext_blake3,a.crypto_mode, \
              'publish_loser','pending',{NOW_MS},{NOW_MS} \
             FROM byok_activation_source_object s JOIN byok_activation_intent a ON a.intent_id=s.intent_id \
             LEFT JOIN byok_logical_object_generation x ON x.tenant_id=s.tenant_id \
              AND x.backfill_run_id=s.intent_id AND x.object_kind=s.object_kind \
              AND x.logical_key=s.logical_key AND x.allocation_id=s.target_allocation_id \
             WHERE s.intent_id=?1 AND s.tenant_id=?2 AND s.target_physical_key IS NOT NULL"),
            vec![json!(intent.intent_id), json!(intent.tenant_id)],
        );
        let enqueue_generation_fallback = ActivationD1Statement::new(
            format!("INSERT OR IGNORE INTO byok_object_purge_item \
             (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id, \
              physical_key,object_size,object_blake3,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) \
             SELECT lower(hex(randomblob(16))),x.tenant_id,?1,x.object_kind,x.logical_key,x.generation, \
              x.allocation_id,x.physical_key,x.ciphertext_size,x.ciphertext_blake3,a.crypto_mode, \
              'publish_loser','pending',{NOW_MS},{NOW_MS} \
             FROM byok_logical_object_generation x JOIN byok_activation_intent a ON a.intent_id=?1 \
             WHERE x.tenant_id=?2 AND x.backfill_run_id=?1"),
            vec![json!(intent.intent_id), json!(intent.tenant_id)],
        );
        let target_causes = ActivationD1Statement::new(
            format!(
                "INSERT OR IGNORE INTO byok_object_purge_cause \
             (purge_id,cause_kind,cause_id,created_at_ms) \
             SELECT p.purge_id,'publish_loser',p.allocation_id,{NOW_MS} FROM byok_object_purge_item p \
             WHERE p.tenant_id=?2 AND p.allocation_id IS NOT NULL AND (EXISTS (SELECT 1 FROM byok_activation_source_object s \
               WHERE s.intent_id=?1 AND s.tenant_id=p.tenant_id \
                AND s.target_physical_key=p.physical_key \
                AND s.target_allocation_id=p.allocation_id) \
              OR EXISTS (SELECT 1 FROM byok_logical_object_generation x \
               WHERE x.backfill_run_id=?1 AND x.tenant_id=p.tenant_id \
                AND x.physical_key=p.physical_key AND x.allocation_id=p.allocation_id \
                AND x.generation=p.generation))"
            ),
            vec![json!(intent.intent_id), json!(intent.tenant_id)],
        );
        let unpublish = ActivationD1Statement::new(
            "DELETE FROM byok_logical_object_publication WHERE tenant_id=?1 AND ?3=1 \
             AND physical_key IN (SELECT physical_key FROM byok_logical_object_generation \
              WHERE tenant_id=?1 AND backfill_run_id=?2)",
            vec![
                json!(intent.tenant_id),
                json!(intent.intent_id),
                json!(preempt),
            ],
        );
        let abandon = ActivationD1Statement::new(
            format!(
                "UPDATE byok_logical_object_generation SET outcome='abandoned', \
             completed_at_ms={NOW_MS} WHERE tenant_id=?1 AND backfill_run_id=?2 \
              AND outcome IN ('allocated','published')"
            ),
            vec![json!(intent.tenant_id), json!(intent.intent_id)],
        );
        let (config, secret) = abort_custody_statements(intent, preempt)?;
        // Restoring the source through the guarded secret UPDATE intentionally
        // fires 0121's archive trigger for the unpublished target TCS.  Retire
        // only that exact target identity; the source history remains the
        // decoder authority for a later retry or purge verification.
        let target_secret_history = abort_target_secret_history_statement(intent, preempt);
        let secret_history = ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_secret_history SET tcs_wrapped=NULL,retired_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND ?2=1 AND tcs_wrapped IS NOT NULL"
            ),
            vec![json!(intent.tenant_id), json!(preempt)],
        );
        let tenant = ActivationD1Statement::new(
            "UPDATE tenant SET byok_revoked_provider=CASE WHEN ?2=1 THEN \
              (SELECT cmk_provider FROM tenant_byok_config WHERE tenant_id=?1) ELSE byok_revoked_provider END, \
             byok_revoked_kms_key_id=CASE WHEN ?2=1 THEN \
              (SELECT cmk_key_id FROM tenant_byok_config WHERE tenant_id=?1) ELSE byok_revoked_kms_key_id END, \
             byok_status=CASE WHEN ?2=1 THEN 'revoked' ELSE byok_status END \
             WHERE tenant_id=?1 RETURNING tenant_id",
            vec![json!(intent.tenant_id), json!(preempt)],
        );
        let phase = if preempt { "preempted" } else { "aborted" };
        let finish = ActivationD1Statement::new(
            format!(
                "UPDATE byok_activation_intent SET phase='{phase}',failure_reason=?4, \
             cmk_provider=CASE WHEN ?3=1 THEN NULL ELSE cmk_provider END, \
             cmk_key_id=CASE WHEN ?3=1 THEN NULL ELSE cmk_key_id END, \
             cmk_region=CASE WHEN ?3=1 THEN NULL ELSE cmk_region END, \
             completed_at_ms={NOW_MS},claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL, \
             state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
             WHERE intent_id=?1 AND state_version=?2 AND (?3=1 OR phase='copy') \
              AND EXISTS (SELECT 1 FROM tenant_byok_config c WHERE c.tenant_id=tenant_id \
               AND c.state='{config_state}') RETURNING intent_id"
            ),
            vec![
                json!(intent.intent_id),
                json!(intent.state_version),
                json!(preempt),
                json!(reason),
            ],
        );
        let gate = ActivationD1Statement::new(
            "UPDATE byok_tenant_gate SET gate_epoch=gate_epoch+CASE WHEN ?2=1 THEN 1 ELSE 0 END \
             WHERE tenant_id=?1 RETURNING gate_epoch",
            vec![json!(intent.tenant_id), json!(preempt)],
        );
        let results = self
            .d1
            .batch(vec![
                operation_guard(&operation_token, intent, claim_token, action, control_token),
                expire_data,
                enqueue_sources,
                source_causes,
                mark_sources,
                enqueue_targets,
                enqueue_generation_fallback,
                target_causes,
                unpublish,
                abandon,
                secret,
                target_secret_history,
                secret_history,
                tenant,
                gate,
                config,
                finish,
                postcondition(
                    &operation_token,
                    intent,
                    if preempt {
                        ActivationPhase::Preempted
                    } else {
                        ActivationPhase::Aborted
                    },
                ),
            ])
            .await
            .map_err(store_error)?;
        ensure_edges(&results, action)?;
        self.load(&intent.intent_id).await
    }

    async fn set_suspension(
        &self,
        intent: &ActivationIntent,
        suspension: ActivationSuspension,
        control_token: &str,
    ) -> Result<ActivationIntent, BackfillError> {
        if control_token.is_empty() {
            return Err(BackfillError::InvalidRequest(
                "0119 control capability is required".to_owned(),
            ));
        }
        let (state, timestamp, control_action) = match suspension {
            ActivationSuspension::Active => ("active", "NULL", "restore"),
            ActivationSuspension::DegradedReadOnly => ("degraded_read_only", NOW_MS, "degrade"),
        };
        let rows = self
            .d1
            .query(
                &format!("UPDATE byok_activation_intent SET suspension_state='{state}', \
                 suspended_at_ms={timestamp},state_version=state_version+1,checkpointed_at_ms={NOW_MS} \
                 WHERE intent_id=?1 AND state_version=?2 AND EXISTS \
                  (SELECT 1 FROM byok_control_outcome o \
                   JOIN tenant t ON t.tenant_id=o.tenant_id \
                   JOIN tenant_byok_config cfg ON cfg.tenant_id=o.tenant_id \
                   WHERE o.token=?3 AND o.tenant_id=tenant_id AND o.action=?4 \
                    AND o.outcome='completed' \
                    AND cfg.cmk_provider=byok_activation_intent.cmk_provider \
                    AND cfg.cmk_key_id=byok_activation_intent.cmk_key_id \
                    AND cfg.cmk_region=byok_activation_intent.cmk_region \
                    AND t.byok_status=CASE ?4 WHEN 'degrade' THEN 'degraded_read_only' \
                     ELSE 'active' END) \
                 RETURNING {}", returning_columns()),
                &[
                    json!(intent.intent_id),
                    json!(intent.state_version),
                    json!(control_token),
                    json!(control_action),
                ],
            )
            .await
            .map_err(store_error)?;
        rows.first()
            .ok_or_else(|| BackfillError::Conflict("suspension capability is stale".to_owned()))
            .and_then(parse_intent)
    }
}

fn claim(intent: &ActivationIntent) -> Result<&WorkerClaim, BackfillError> {
    intent
        .claim
        .as_ref()
        .ok_or_else(|| BackfillError::Conflict("activation worker capability is absent".to_owned()))
}

fn identity(intent: &ActivationIntent) -> Vec<Value> {
    vec![
        json!(intent.intent_id),
        json!(intent.tenant_id),
        json!(intent.source_generation),
        json!(intent.target_generation),
        json!(intent.observed_gate_epoch),
        json!(intent.policy.config_version),
    ]
}

/// Select the only safe configuration state for an activation termination.
///
/// An unpublished generation-zero activation has no predecessor to restore,
/// so it returns to the empty inactive state.  An unpublished rotated
/// activation still has a live source generation and must restore that exact
/// source custody.  A control-plane preemption is the sole destructive path
/// and intentionally ends in `shredded`.
fn abort_config_state(source_generation: i64, preempt: bool) -> &'static str {
    if preempt {
        "shredded"
    } else if source_generation == 0 {
        "inactive"
    } else {
        "active"
    }
}

/// Build the config/TCS part of an abort batch.
///
/// Every non-destructive branch carries the activation's target config/TCS
/// identity as a compare-and-swap predicate.  The rotated branch additionally
/// requires the exact source history, source generation gate, and absence of
/// a publication from another generation.  A missing or changed predecessor
/// therefore returns zero rows and the terminal postcondition rolls the batch
/// back instead of guessing at custody.
fn abort_custody_statements(
    intent: &ActivationIntent,
    preempt: bool,
) -> Result<(ActivationD1Statement, ActivationD1Statement), BackfillError> {
    let config = if preempt {
        ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_config SET state='shredded',config_version=config_version+1, \
             cmk_provider=NULL,cmk_key_id=NULL,cmk_region=NULL,updated_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND state IN ('pending','partial') RETURNING config_version"
            ),
            vec![json!(intent.tenant_id)],
        )
    } else if intent.source_generation == 0 {
        ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_config SET state='inactive',config_version=config_version+1, \
             cmk_provider=NULL,cmk_key_id=NULL,cmk_region=NULL,updated_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND state='pending' AND config_version=?2 \
              AND mode=?3 AND crypto_mode=?4 AND cmk_provider=?5 \
              AND cmk_key_id=?6 AND cmk_region=?7 RETURNING config_version"
            ),
            vec![
                json!(intent.tenant_id),
                json!(intent.policy.config_version),
                json!(intent.policy.mode),
                json!(intent.policy.crypto_mode),
                json!(intent.policy.cmk_provider),
                json!(intent.policy.cmk_key_id),
                json!(intent.policy.cmk_region),
            ],
        )
    } else {
        let source = intent.source_identity.as_ref().ok_or_else(|| {
            BackfillError::Store(
                "rotated activation is missing its immutable source custody".to_owned(),
            )
        })?;
        ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_config SET \
             mode=(SELECT h.mode FROM tenant_byok_config_history h WHERE h.tenant_id=?1 \
                AND h.config_version=?2 AND h.state='active' AND h.cmk_provider=?3 \
                AND h.cmk_key_id=?4 AND h.cmk_region=?5), \
             crypto_mode=(SELECT h.crypto_mode FROM tenant_byok_config_history h WHERE h.tenant_id=?1 \
                AND h.config_version=?2 AND h.state='active' AND h.cmk_provider=?3 \
                AND h.cmk_key_id=?4 AND h.cmk_region=?5), \
             cmk_provider=?3,cmk_key_id=?4,cmk_region=?5,state='active', \
             config_version=config_version+1,updated_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND state='pending' AND config_version=?6 \
              AND mode=?7 AND crypto_mode=?8 AND cmk_provider=?9 \
              AND cmk_key_id=?10 AND cmk_region=?11 \
              AND EXISTS (SELECT 1 FROM tenant_byok_config_history h WHERE h.tenant_id=?1 \
                AND h.config_version=?2 AND h.state='active' \
                AND h.cmk_provider=?3 AND h.cmk_key_id=?4 AND h.cmk_region=?5) \
              AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 \
                AND g.current_generation=?12) \
              AND NOT EXISTS (SELECT 1 FROM byok_logical_object_publication p \
                WHERE p.tenant_id=?1 AND p.generation<>?12) RETURNING config_version"
            ),
            vec![
                json!(intent.tenant_id),
                json!(source.config_version),
                json!(source.cmk_provider),
                json!(source.cmk_key_id),
                json!(source.cmk_region),
                json!(intent.policy.config_version),
                json!(intent.policy.mode),
                json!(intent.policy.crypto_mode),
                json!(intent.policy.cmk_provider),
                json!(intent.policy.cmk_key_id),
                json!(intent.policy.cmk_region),
                json!(intent.source_generation),
            ],
        )
    };

    let secret = if preempt {
        ActivationD1Statement::new(
            "UPDATE tenant_byok_secret SET tcs_wrapped=NULL,cmk_key_id=NULL,wrapped_at_ms=NULL, \
             tcs_version=tcs_version+1 WHERE tenant_id=?1",
            vec![json!(intent.tenant_id)],
        )
    } else if intent.source_generation == 0 {
        ActivationD1Statement::new(
            "UPDATE tenant_byok_secret SET tcs_wrapped=NULL,cmk_key_id=NULL,wrapped_at_ms=NULL, \
             tcs_version=tcs_version+1 WHERE tenant_id=?1 AND tcs_version=?2 \
              AND cmk_key_id=?3 AND tcs_wrapped IS NOT NULL",
            vec![
                json!(intent.tenant_id),
                json!(intent.policy.tcs_version),
                json!(intent.policy.cmk_key_id),
            ],
        )
    } else {
        let source = intent.source_identity.as_ref().ok_or_else(|| {
            BackfillError::Store(
                "rotated activation is missing its immutable source custody".to_owned(),
            )
        })?;
        ActivationD1Statement::new(
            format!(
                "UPDATE tenant_byok_secret SET \
             tcs_wrapped=(SELECT h.tcs_wrapped FROM tenant_byok_secret_history h \
                WHERE h.tenant_id=?1 AND h.tcs_version=?2 AND h.cmk_provider=?3 \
                 AND h.cmk_key_id=?4 AND h.cmk_region=?5 AND h.tcs_wrapped IS NOT NULL), \
             cmk_key_id=?4,tcs_version=tcs_version+1,wrapped_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND tcs_version=?6 AND cmk_key_id=?7 \
              AND tcs_wrapped IS NOT NULL AND EXISTS (SELECT 1 FROM tenant_byok_secret_history h \
                WHERE h.tenant_id=?1 AND h.tcs_version=?2 AND h.cmk_provider=?3 \
                 AND h.cmk_key_id=?4 AND h.cmk_region=?5 AND h.tcs_wrapped IS NOT NULL) \
              AND EXISTS (SELECT 1 FROM tenant_byok_config c WHERE c.tenant_id=?1 \
                AND c.state='pending' AND c.config_version=?8 \
                AND c.cmk_key_id=?7)"
            ),
            vec![
                json!(intent.tenant_id),
                json!(source.tcs_version),
                json!(source.cmk_provider),
                json!(source.cmk_key_id),
                json!(source.cmk_region),
                json!(intent.policy.tcs_version),
                json!(intent.policy.cmk_key_id),
                json!(intent.policy.config_version),
            ],
        )
    };
    Ok((config, secret))
}

/// Retire the unpublished target TCS archived by the secret-rotation trigger.
///
/// The target version/key tuple comes from the activation's immutable policy
/// snapshot, and the intent identity is also checked before retiring.  This
/// keeps a normal abort from shredding the source history or an unrelated
/// tenant version.  Preemption uses the broader historical shred below.
fn abort_target_secret_history_statement(
    intent: &ActivationIntent,
    preempt: bool,
) -> ActivationD1Statement {
    ActivationD1Statement::new(
        format!(
            "UPDATE tenant_byok_secret_history SET tcs_wrapped=NULL,retired_at_ms={NOW_MS} \
             WHERE tenant_id=?1 AND tcs_version=?2 AND cmk_provider=?3 \
              AND cmk_key_id=?4 AND cmk_region=?5 AND tcs_wrapped IS NOT NULL \
              AND ?6=0 AND EXISTS (SELECT 1 FROM byok_activation_intent a \
               WHERE a.intent_id=?7 AND a.tenant_id=tenant_byok_secret_history.tenant_id \
                AND a.tcs_version=?2 AND a.cmk_provider=?3 AND a.cmk_key_id=?4 \
                AND a.cmk_region=?5 AND a.phase='copy')"
        ),
        vec![
            json!(intent.tenant_id),
            json!(intent.policy.tcs_version),
            json!(intent.policy.cmk_provider),
            json!(intent.policy.cmk_key_id),
            json!(intent.policy.cmk_region),
            json!(preempt),
            json!(intent.intent_id),
        ],
    )
}

fn operation_guard(
    token: &str,
    intent: &ActivationIntent,
    claim_token: &str,
    action: &str,
    control_token: Option<&str>,
) -> ActivationD1Statement {
    ActivationD1Statement::new(
        "INSERT INTO byok_activation_operation_guard \
         (operation_token,intent_id,claim_token,control_token,expected_state_version,action,checked_at_ms) \
         VALUES (?1,?2,?3,?4,?5,?6,0) RETURNING operation_token",
        vec![
            json!(token),
            json!(intent.intent_id),
            if action == "preempt" {
                Value::Null
            } else {
                json!(claim_token)
            },
            json!(control_token),
            json!(intent.state_version),
            json!(action),
        ],
    )
}

fn postcondition(
    operation_token: &str,
    intent: &ActivationIntent,
    phase: ActivationPhase,
) -> ActivationD1Statement {
    ActivationD1Statement::new(
        "INSERT INTO byok_activation_postcondition \
         (operation_token,intent_id,expected_state_version,expected_phase,checked_at_ms) \
         VALUES (?1,?2,?3,?4,0) RETURNING operation_token",
        vec![
            json!(format!("{operation_token}:post")),
            json!(intent.intent_id),
            json!(intent.state_version + 1),
            json!(phase_name(phase)),
        ],
    )
}

fn ensure_edges(results: &[Vec<D1Row>], action: &str) -> Result<(), BackfillError> {
    if results.first().map_or(true, Vec::is_empty) || results.last().map_or(true, Vec::is_empty) {
        return Err(BackfillError::Conflict(format!(
            "guarded activation {action} did not complete"
        )));
    }
    Ok(())
}

fn intent_select() -> &'static str {
    "SELECT a.intent_id,a.tenant_id,a.guard_id,a.request_blake3,a.config_version,a.mode, \
     a.crypto_mode,a.cmk_provider,a.cmk_key_id,a.cmk_region,a.tcs_version,a.wrapped_tcs_blake3, \
     a.policy_blake3,a.source_generation,a.source_config_version,a.source_cmk_provider, \
     a.source_cmk_key_id,a.source_cmk_region,a.source_tcs_version,a.target_generation,a.observed_gate_epoch, \
     a.publication_gate_epoch,a.published_config_version,a.phase,a.suspension_state, \
     a.cas_complete,a.ac_complete,a.claim_owner,a.claim_token,a.claim_epoch,a.claim_expires_at_ms, \
     a.deadline_at_ms,a.state_version,(a.deadline_at_ms <= \
     (CAST(strftime('%s','now') AS INTEGER)*1000)) AS deadline_expired \
     FROM byok_activation_intent a"
}

fn returning_columns() -> &'static str {
    "intent_id,tenant_id,guard_id,request_blake3,config_version,mode,crypto_mode, \
     cmk_provider,cmk_key_id,cmk_region,tcs_version,wrapped_tcs_blake3,policy_blake3, \
     source_generation,source_config_version,source_cmk_provider,source_cmk_key_id, \
     source_cmk_region,source_tcs_version,target_generation,observed_gate_epoch,publication_gate_epoch, \
     published_config_version,phase,suspension_state,cas_complete,ac_complete,claim_owner, \
     claim_token,claim_epoch,claim_expires_at_ms,deadline_at_ms,state_version, \
     (deadline_at_ms <= (CAST(strftime('%s','now') AS INTEGER)*1000)) AS deadline_expired"
}

fn parse_intent(row: &D1Row) -> Result<ActivationIntent, BackfillError> {
    let phase = match string(row, "phase")?.as_str() {
        "copy" => ActivationPhase::Copy,
        "published_partial" => ActivationPhase::PublishedPartial,
        "purging" => ActivationPhase::Purging,
        "ready_finalize" => ActivationPhase::ReadyFinalize,
        "committed" => ActivationPhase::Committed,
        "aborted" => ActivationPhase::Aborted,
        "preempted" => ActivationPhase::Preempted,
        _ => return Err(parse_error("phase")),
    };
    let suspension = match string(row, "suspension_state")?.as_str() {
        "active" => ActivationSuspension::Active,
        "degraded_read_only" => ActivationSuspension::DegradedReadOnly,
        _ => return Err(parse_error("suspension_state")),
    };
    let claim = match optional_string(row, "claim_token") {
        Some(token) => Some(WorkerClaim::new(
            string(row, "claim_owner")?,
            token,
            integer(row, "claim_epoch")?,
            integer(row, "claim_expires_at_ms")?,
        )),
        None => None,
    };
    let source_identity = match optional_integer(row, "source_config_version") {
        Some(config_version) => Some(SourceActivationIdentity {
            config_version,
            cmk_provider: string(row, "source_cmk_provider")?,
            cmk_key_id: string(row, "source_cmk_key_id")?,
            cmk_region: string(row, "source_cmk_region")?,
            tcs_version: integer(row, "source_tcs_version")?,
        }),
        None => None,
    };
    Ok(ActivationIntent {
        intent_id: string(row, "intent_id")?,
        tenant_id: string(row, "tenant_id")?,
        guard_id: string(row, "guard_id")?,
        request_blake3: string(row, "request_blake3")?,
        claim,
        observed_gate_epoch: integer(row, "observed_gate_epoch")?,
        publication_gate_epoch: optional_integer(row, "publication_gate_epoch"),
        published_config_version: optional_integer(row, "published_config_version"),
        source_generation: integer(row, "source_generation")?,
        source_identity,
        target_generation: integer(row, "target_generation")?,
        policy: PinnedActivationPolicy {
            config_version: integer(row, "config_version")?,
            mode: string(row, "mode")?,
            crypto_mode: string(row, "crypto_mode")?,
            cmk_provider: optional_string(row, "cmk_provider"),
            cmk_key_id: optional_string(row, "cmk_key_id"),
            cmk_region: optional_string(row, "cmk_region"),
            tcs_version: integer(row, "tcs_version")?,
            wrapped_tcs_blake3: optional_string(row, "wrapped_tcs_blake3"),
            policy_blake3: string(row, "policy_blake3")?,
        },
        phase,
        suspension,
        cas_complete: boolean(row, "cas_complete")?,
        ac_complete: boolean(row, "ac_complete")?,
        deadline_at_ms: integer(row, "deadline_at_ms")?,
        deadline_expired: boolean(row, "deadline_expired")?,
        state_version: integer(row, "state_version")?,
    })
}

fn phase_name(phase: ActivationPhase) -> &'static str {
    match phase {
        ActivationPhase::Copy => "copy",
        ActivationPhase::PublishedPartial => "published_partial",
        ActivationPhase::Purging => "purging",
        ActivationPhase::ReadyFinalize => "ready_finalize",
        ActivationPhase::Committed => "committed",
        ActivationPhase::Aborted => "aborted",
        ActivationPhase::Preempted => "preempted",
    }
}

fn string(row: &D1Row, name: &str) -> Result<String, BackfillError> {
    optional_string(row, name).ok_or_else(|| parse_error(name))
}

fn optional_string(row: &D1Row, name: &str) -> Option<String> {
    row.get(name).and_then(Value::as_str).map(str::to_owned)
}

fn integer(row: &D1Row, name: &str) -> Result<i64, BackfillError> {
    row.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| parse_error(name))
}

fn optional_integer(row: &D1Row, name: &str) -> Option<i64> {
    row.get(name).and_then(Value::as_i64)
}

fn boolean(row: &D1Row, name: &str) -> Result<bool, BackfillError> {
    match integer(row, name)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(parse_error(name)),
    }
}

fn parse_error(field: &str) -> BackfillError {
    BackfillError::Store(format!("invalid activation D1 field: {field}"))
}

fn store_error(error: String) -> BackfillError {
    BackfillError::Store(error)
}

#[cfg(test)]
mod abort_tests {
    use super::*;

    fn intent(source_generation: i64) -> ActivationIntent {
        ActivationIntent {
            intent_id: "intent-rotated".to_owned(),
            tenant_id: "tenant-rotated".to_owned(),
            guard_id: "guard-rotated".to_owned(),
            request_blake3: "a".repeat(64),
            claim: Some(WorkerClaim::new(
                "worker".to_owned(),
                "claim".to_owned(),
                1,
                9_999,
            )),
            observed_gate_epoch: 7,
            publication_gate_epoch: None,
            published_config_version: None,
            source_generation,
            source_identity: (source_generation > 0).then(|| SourceActivationIdentity {
                config_version: 11,
                cmk_provider: "aws".to_owned(),
                cmk_key_id: "old-key".to_owned(),
                cmk_region: "us-east-1".to_owned(),
                tcs_version: 4,
            }),
            target_generation: source_generation + 1,
            policy: PinnedActivationPolicy {
                config_version: 12,
                mode: "byok".to_owned(),
                crypto_mode: "random".to_owned(),
                cmk_provider: Some("aws".to_owned()),
                cmk_key_id: Some("new-key".to_owned()),
                cmk_region: Some("us-east-1".to_owned()),
                tcs_version: 5,
                wrapped_tcs_blake3: Some("b".repeat(64)),
                policy_blake3: "c".repeat(64),
            },
            phase: ActivationPhase::Copy,
            suspension: ActivationSuspension::Active,
            cas_complete: false,
            ac_complete: false,
            deadline_at_ms: 1,
            deadline_expired: true,
            state_version: 3,
        }
    }

    #[test]
    fn expired_rotated_activation_restores_source_instead_of_inactive() {
        let rotated = intent(4);
        assert_eq!(
            abort_config_state(rotated.source_generation, false),
            "active"
        );
        let result = abort_custody_statements(&rotated, false);
        assert!(result.is_ok());
        let Ok((config, secret)) = result else { return };
        assert!(config.sql.contains("state='active'"));
        assert!(config.sql.contains("current_generation=?12"));
        assert!(config.sql.contains("tenant_byok_config_history"));
        assert!(secret.sql.contains("tenant_byok_secret_history"));
        assert!(secret.sql.contains("tcs_version=tcs_version+1"));
        assert!(!config.sql.contains("state='inactive'"));
        let target_history = abort_target_secret_history_statement(&rotated, false);
        assert!(target_history.sql.contains("tcs_version=?2"));
        assert!(target_history.sql.contains("cmk_key_id=?4"));
        assert!(target_history.sql.contains("AND ?6=0"));
        assert!(target_history.sql.contains("a.intent_id=?7"));
    }

    #[test]
    fn expired_generation_zero_activation_returns_to_inactive_without_tcs() {
        let generation_zero = intent(0);
        assert_eq!(
            abort_config_state(generation_zero.source_generation, false),
            "inactive"
        );
        let result = abort_custody_statements(&generation_zero, false);
        assert!(result.is_ok());
        let Ok((config, secret)) = result else { return };
        assert!(config.sql.contains("state='inactive'"));
        assert!(config.sql.contains("cmk_provider=NULL"));
        assert!(secret.sql.contains("tcs_wrapped=NULL"));
        assert!(!secret.sql.contains("tenant_byok_secret_history"));
        let target_history = abort_target_secret_history_statement(&generation_zero, false);
        assert!(target_history.sql.contains("tcs_version=?2"));
        assert!(target_history.sql.contains("cmk_provider=?3"));
    }

    #[test]
    fn only_preemption_shreds_rotated_custody() {
        let rotated = intent(4);
        assert_eq!(
            abort_config_state(rotated.source_generation, true),
            "shredded"
        );
        let result = abort_custody_statements(&rotated, true);
        assert!(result.is_ok());
        let Ok((config, secret)) = result else { return };
        assert!(config.sql.contains("state='shredded'"));
        assert!(secret.sql.contains("tcs_wrapped=NULL"));
        let target_history = abort_target_secret_history_statement(&rotated, true);
        assert!(target_history.sql.contains("AND ?6=0"));
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod zero_source_tests {
    use rusqlite::{params_from_iter, Connection};

    use super::*;

    fn fixture() -> (Connection, ActivationIntent) {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE tenant_byok_secret_history (
                    tenant_id TEXT, tcs_version INTEGER, cmk_provider TEXT,
                    cmk_key_id TEXT, cmk_region TEXT, tcs_wrapped BLOB,
                    retired_at_ms INTEGER
                 );
                 CREATE TABLE tenant_byok_secret (
                    tenant_id TEXT, tcs_version INTEGER, cmk_key_id TEXT,
                    tcs_wrapped BLOB
                 );
                 CREATE TABLE tenant_byok_config (
                    tenant_id TEXT, cmk_provider TEXT, cmk_key_id TEXT,
                    cmk_region TEXT
                 );
                 CREATE TABLE byok_activation_intent (
                    intent_id TEXT, tenant_id TEXT, guard_id TEXT, phase TEXT,
                    source_generation INTEGER, source_config_version INTEGER,
                    source_cmk_provider TEXT, source_cmk_key_id TEXT,
                    source_cmk_region TEXT, source_tcs_version INTEGER,
                    target_generation INTEGER, publication_gate_epoch INTEGER
                 );
                 CREATE TABLE byok_activation_guard (
                    guard_id TEXT, tenant_id TEXT, transition_token TEXT,
                    transition_epoch INTEGER, outcome TEXT
                 );
                 CREATE TABLE byok_transition_fence (
                    token TEXT, tenant_id TEXT, epoch INTEGER,
                    observed_generation INTEGER, observed_gate_epoch INTEGER,
                    outcome TEXT
                 );
                 CREATE TABLE byok_activation_operation_guard (
                    operation_token TEXT, intent_id TEXT, action TEXT,
                    expected_state_version INTEGER
                 );
                 CREATE TABLE byok_tenant_gate (
                    tenant_id TEXT, current_generation INTEGER, gate_epoch INTEGER
                 );
                 CREATE TABLE byok_activation_source_object (
                    intent_id TEXT, tenant_id TEXT, object_kind TEXT,
                    logical_key TEXT, source_generation INTEGER,
                    source_allocation_id TEXT, source_physical_key TEXT,
                    source_config_version INTEGER, source_cmk_provider TEXT,
                    source_cmk_key_id TEXT, source_cmk_region TEXT,
                    source_tcs_version INTEGER, copy_state TEXT
                 );
                 CREATE TABLE byok_object_purge_item (
                    purge_id TEXT, intent_id TEXT, tenant_id TEXT,
                    object_kind TEXT, logical_key TEXT, generation INTEGER,
                    allocation_id TEXT, physical_key TEXT, reason TEXT, state TEXT
                 );
                 CREATE TABLE byok_object_purge_cause (
                    purge_id TEXT, cause_kind TEXT, cause_id TEXT
                 );",
            )
            .expect("schema");
        connection
            .execute(
                "INSERT INTO tenant_byok_secret_history
                 VALUES ('tenant-a',1,'aws','old-key','us-east-1',X'01',NULL)",
                [],
            )
            .expect("history");
        connection
            .execute(
                "INSERT INTO tenant_byok_secret VALUES ('tenant-a',2,'new-key',X'02')",
                [],
            )
            .expect("current secret");
        connection
            .execute(
                "INSERT INTO tenant_byok_config
                 VALUES ('tenant-a','aws','new-key','us-east-1')",
                [],
            )
            .expect("current config");
        connection
            .execute(
                "INSERT INTO byok_activation_intent VALUES
                 ('intent-a','tenant-a','guard-a','ready_finalize',4,1,
                  'aws','old-key','us-east-1',1,5,7)",
                [],
            )
            .expect("intent");
        connection
            .execute(
                "INSERT INTO byok_activation_guard VALUES
                 ('guard-a','tenant-a','fence-a',3,'active')",
                [],
            )
            .expect("guard");
        connection
            .execute(
                "INSERT INTO byok_transition_fence VALUES
                 ('fence-a','tenant-a',3,5,7,'active')",
                [],
            )
            .expect("fence");
        connection
            .execute(
                "INSERT INTO byok_activation_operation_guard VALUES
                 ('op-a','intent-a','finalize',9)",
                [],
            )
            .expect("operation guard");
        connection
            .execute("INSERT INTO byok_tenant_gate VALUES ('tenant-a',5,8)", [])
            .expect("gate");

        let intent = ActivationIntent {
            intent_id: "intent-a".to_owned(),
            tenant_id: "tenant-a".to_owned(),
            guard_id: "guard-a".to_owned(),
            request_blake3: "a".repeat(64),
            claim: Some(WorkerClaim::new(
                "worker-a".to_owned(),
                "claim-a".to_owned(),
                1,
                10_000,
            )),
            observed_gate_epoch: 7,
            publication_gate_epoch: Some(7),
            published_config_version: Some(2),
            source_generation: 4,
            source_identity: Some(SourceActivationIdentity {
                config_version: 1,
                cmk_provider: "aws".to_owned(),
                cmk_key_id: "old-key".to_owned(),
                cmk_region: "us-east-1".to_owned(),
                tcs_version: 1,
            }),
            target_generation: 5,
            policy: PinnedActivationPolicy {
                config_version: 2,
                mode: "byok".to_owned(),
                crypto_mode: "convergent".to_owned(),
                cmk_provider: Some("aws".to_owned()),
                cmk_key_id: Some("new-key".to_owned()),
                cmk_region: Some("us-east-1".to_owned()),
                tcs_version: 2,
                wrapped_tcs_blake3: Some("b".repeat(64)),
                policy_blake3: "c".repeat(64),
            },
            phase: ActivationPhase::ReadyFinalize,
            suspension: ActivationSuspension::Active,
            cas_complete: true,
            ac_complete: true,
            deadline_at_ms: 20_000,
            deadline_expired: false,
            state_version: 9,
        };
        (connection, intent)
    }

    fn retire(connection: &Connection, intent: &ActivationIntent) -> usize {
        let statement = retire_source_history_statement(intent, "op-a");
        let values = statement
            .params
            .iter()
            .map(|value| match value {
                Value::String(value) => rusqlite::types::Value::Text(value.clone()),
                Value::Number(value) => {
                    rusqlite::types::Value::Integer(value.as_i64().expect("integer test parameter"))
                }
                _ => panic!("unexpected test parameter"),
            })
            .collect::<Vec<_>>();
        connection
            .execute(&statement.sql, params_from_iter(values))
            .expect("retirement statement");
        connection.changes() as usize
    }

    fn wrapped(connection: &Connection) -> Option<Vec<u8>> {
        connection
            .query_row(
                "SELECT tcs_wrapped FROM tenant_byok_secret_history
                 WHERE tenant_id='tenant-a' AND tcs_version=1",
                [],
                |row| row.get(0),
            )
            .expect("history row")
    }

    #[test]
    fn zero_source_rotation_retires_exact_historical_tcs() {
        let (connection, intent) = fixture();

        assert_eq!(retire(&connection, &intent), 1);
        assert_eq!(wrapped(&connection), None);
    }

    #[test]
    fn cross_intent_purged_source_requires_verified_fenced_proof() {
        let (connection, intent) = fixture();
        connection
            .execute(
                "INSERT INTO byok_activation_intent VALUES
                 ('prior-intent','tenant-a','prior-guard','committed',4,1,
                  'aws','old-key','us-east-1',1,5,7)",
                [],
            )
            .expect("prior intent");
        connection
            .execute(
                "INSERT INTO byok_activation_guard VALUES
                 ('prior-guard','tenant-a','prior-fence',4,'committed')",
                [],
            )
            .expect("prior guard");
        connection
            .execute(
                "INSERT INTO byok_transition_fence VALUES
                 ('prior-fence','tenant-a',4,5,7,'committed')",
                [],
            )
            .expect("prior fence");
        connection
            .execute(
                "INSERT INTO byok_activation_source_object VALUES
                 ('prior-intent','tenant-a','cas','blake3:object',4,'allocation-prior',
                  'r2/prior','1','aws','old-key','us-east-1',1,'purged')",
                [],
            )
            .expect("prior source");
        connection
            .execute(
                "INSERT INTO byok_object_purge_item VALUES
                 ('prior-purge','prior-intent','tenant-a','cas','blake3:object',4,
                  'allocation-prior','r2/prior','activation_source','verified')",
                [],
            )
            .expect("prior purge");
        connection
            .execute(
                "INSERT INTO byok_object_purge_cause VALUES
                 ('prior-purge','activation_source','prior-intent')",
                [],
            )
            .expect("prior cause");

        assert_eq!(retire(&connection, &intent), 1);
        assert_eq!(wrapped(&connection), None);
    }

    #[test]
    fn retirement_rejects_bad_allocation_pending_source_and_fence() {
        let (connection, intent) = fixture();
        connection
            .execute(
                "INSERT INTO byok_activation_source_object VALUES
                 ('intent-a','tenant-a','cas','blake3:object',4,'allocation-good',
                  'r2/old','1','aws','old-key','us-east-1',1,'purged')",
                [],
            )
            .expect("source");
        connection
            .execute(
                "INSERT INTO byok_object_purge_item VALUES
                 ('purge-a','intent-a','tenant-a','cas','blake3:object',4,
                  'allocation-wrong','r2/old','activation_source','verified')",
                [],
            )
            .expect("purge");
        connection
            .execute(
                "INSERT INTO byok_object_purge_cause VALUES
                 ('purge-a','activation_source','intent-a')",
                [],
            )
            .expect("cause");
        assert_eq!(retire(&connection, &intent), 0);
        assert_eq!(wrapped(&connection), Some(vec![1]));

        connection
            .execute("DELETE FROM byok_activation_source_object", [])
            .expect("source cleanup");
        connection
            .execute("DELETE FROM byok_object_purge_item", [])
            .expect("purge cleanup");
        connection
            .execute("DELETE FROM byok_object_purge_cause", [])
            .expect("cause cleanup");
        connection
            .execute(
                "INSERT INTO byok_activation_source_object VALUES
                 ('other-intent','tenant-a','cas','blake3:object',4,'allocation-other',
                  'r2/other','1','aws','old-key','us-east-1',1,'discovered')",
                [],
            )
            .expect("pending source");
        assert_eq!(retire(&connection, &intent), 0);
        assert_eq!(wrapped(&connection), Some(vec![1]));

        connection
            .execute("DELETE FROM byok_activation_source_object", [])
            .expect("pending source cleanup");
        connection
            .execute(
                "INSERT INTO byok_activation_source_object VALUES
                 ('other-intent','tenant-a','cas','blake3:object',4,'allocation-other',
                  'r2/other','1','aws','old-key','us-east-1',1,'purged')",
                [],
            )
            .expect("unproven purged source");
        assert_eq!(retire(&connection, &intent), 0);
        assert_eq!(wrapped(&connection), Some(vec![1]));

        connection
            .execute("DELETE FROM byok_activation_source_object", [])
            .expect("unproven source cleanup");
        connection
            .execute(
                "UPDATE byok_transition_fence SET observed_generation=4
                 WHERE token='fence-a'",
                [],
            )
            .expect("fence mutation");
        assert_eq!(retire(&connection, &intent), 0);
        assert_eq!(wrapped(&connection), Some(vec![1]));

        connection
            .execute(
                "INSERT INTO tenant_byok_secret VALUES ('tenant-a',1,'old-key',X'03')",
                [],
            )
            .expect("current source identity");
        connection
            .execute(
                "INSERT INTO tenant_byok_config
                 VALUES ('tenant-a','aws','old-key','us-east-1')",
                [],
            )
            .expect("current source config");
        connection
            .execute(
                "UPDATE byok_transition_fence SET observed_generation=5
                 WHERE token='fence-a'",
                [],
            )
            .expect("fence restore");
        assert_eq!(retire(&connection, &intent), 0);
        assert_eq!(wrapped(&connection), Some(vec![1]));
    }
}

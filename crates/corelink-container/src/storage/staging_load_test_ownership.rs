//! Append-only resource registration for the #2161 staging teardown ledger.
//!
//! This is a persistence seam only. It does not authenticate a caller, prove
//! that a resource was created by the named scenario, scan inventory, or
//! delete resources. Callers must obtain the run identity from a trusted
//! admission path before invoking it.

use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    d1_http::{D1BatchStatement, D1HttpClient},
    staging_load_test_admission::StagingLoadTestAdmissionContext,
};

const SQL_REGISTER_RESOURCE: &str = "INSERT INTO staging_load_test_resources \
     (run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms) \
     SELECT ?1, ?2, ?4, ?5, ?6, ?7, 'registered', ?8 \
     WHERE EXISTS (SELECT 1 FROM staging_load_test_runs \
       WHERE run_id = ?1 AND scenario = ?2 AND target_environment = 'staging' \
         AND target_deployment_sha = ?3 AND state = 'open') \
     RETURNING receipt_ref";

// Keep ?3 unused here to preserve the same positional parameter array as the
// insert query. D1's HTTP API accepts positional values, including values for
// skipped numbered placeholders.
const SQL_FIND_REGISTERED_RESOURCE: &str = "SELECT receipt_ref FROM staging_load_test_resources \
     WHERE run_id = ?1 AND scenario = ?2 AND resource_class = ?4 \
       AND receipt_ref = ?5 AND opaque_handle = ?6 AND disposition = ?7 \
     LIMIT 1";

const SQL_REGISTER_RESOURCE_IN_BATCH: &str = "INSERT INTO staging_load_test_resources \
     (run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'registered', ?7) \
     ON CONFLICT (run_id, scenario, resource_class, receipt_ref) DO NOTHING";

const SQL_PREPARE_R2_INTENT: &str = "INSERT INTO staging_load_test_r2_intents \
     (operation_id, run_id, scenario, target_deployment_sha, resource_class, receipt_ref, opaque_handle, disposition, state, prepared_at_ms) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'prepared', ?9)";

const SQL_COMMIT_R2_INTENT: &str = "UPDATE staging_load_test_r2_intents \
     SET state = 'committed', committed_at_ms = ?2 \
     WHERE operation_id = ?1 AND state IN ('prepared', 'committed')";

/// Optional request-scoped provenance. `None` is ordinary non-synthetic traffic.
pub type StagingLoadTestWriteContext<'a> = Option<&'a StagingLoadTestAdmissionContext>;

/// Fixed, redacted failures shared by every writer family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestOwnershipError {
    /// The context is valid but belongs to another scenario.
    ScenarioMismatch,
    /// A bounded public identifier is malformed.
    InvalidIdentifier,
    /// The resource class cannot use the requested disposition.
    InvalidDisposition,
}

impl core::fmt::Display for StagingLoadTestOwnershipError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::ScenarioMismatch => "staging ownership scenario mismatch",
            Self::InvalidIdentifier => "staging ownership identifier is invalid",
            Self::InvalidDisposition => "staging ownership disposition is invalid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for StagingLoadTestOwnershipError {}

/// One of the scenarios admitted by migration 0147.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestScenario {
    /// Signup flow scenario.
    Signup,
    /// Stripe webhook scenario.
    Webhook,
    /// Data-subject request scenario.
    Dsr,
    /// Content-addressable storage scenario.
    Cas,
    /// Bring-your-own-key scenario.
    Byok,
    /// Two-hour endurance scenario.
    Endurance2h,
    /// Cargo write-path scenario B-103.
    B103CargoWrite,
}

impl StagingLoadTestScenario {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Signup => "signup",
            Self::Webhook => "webhook",
            Self::Dsr => "dsr",
            Self::Cas => "cas",
            Self::Byok => "byok",
            Self::Endurance2h => "endurance-2h",
            Self::B103CargoWrite => "b103-cargo-write",
        }
    }
}

/// Resource classes admitted by migration 0147.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestResourceClass {
    /// Run-owned reference to a potentially shared CAS object.
    CasReference,
    /// Run-unique webhook inbox row.
    WebhookInbox,
    /// Effect row attributed to a run-unique webhook event.
    WebhookEffect,
    /// Synthetic DSR artifact whose disposition requires owner classification.
    DsrArtifact,
    /// Durable DSR obligation retained for compliance.
    DsrObligation,
    /// Audit evidence retained for chain integrity.
    AuditEvidence,
    /// Billing audit evidence retained for financial accountability.
    BillingAudit,
    /// Signup artifact whose disposition requires owner classification.
    SignupArtifact,
    /// BYOK artifact whose disposition requires owner classification.
    ByokArtifact,
}

impl StagingLoadTestResourceClass {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CasReference => "cas_reference",
            Self::WebhookInbox => "webhook_inbox",
            Self::WebhookEffect => "webhook_effect",
            Self::DsrArtifact => "dsr_artifact",
            Self::DsrObligation => "dsr_obligation",
            Self::AuditEvidence => "audit_evidence",
            Self::BillingAudit => "billing_audit",
            Self::SignupArtifact => "signup_artifact",
            Self::ByokArtifact => "byok_artifact",
        }
    }

    const fn requires_retention(self) -> bool {
        matches!(
            self,
            Self::CasReference | Self::DsrObligation | Self::AuditEvidence | Self::BillingAudit
        )
    }
}

/// Explicit retention decision made by the resource-owning adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagingLoadTestDisposition {
    /// Eligible for the later exact-run teardown flow only after its own
    /// resource-specific safety checks pass.
    Disposable,
    /// Receipt-only state that teardown must preserve.
    Retained,
}

impl StagingLoadTestDisposition {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Disposable => "disposable",
            Self::Retained => "retained",
        }
    }
}

/// Exact staging run identity and opaque resource handle to register.
///
/// `opaque_handle` is intentionally omitted from `Debug` and is never emitted
/// by this adapter. It must be a non-empty, bounded identifier without
/// control characters; callers remain responsible for ensuring it contains
/// no credentials or personal data.
pub struct StagingLoadTestResourceRegistration<'a> {
    /// Canonical positive decimal GitHub Actions run ID.
    run_id: &'a str,
    /// Allowlisted scenario associated with this run.
    scenario: StagingLoadTestScenario,
    /// Lowercase 40-hex deployment commit identity stored on the staging run.
    target_deployment_sha: &'a str,
    /// Resource class defined by migration 0147.
    resource_class: StagingLoadTestResourceClass,
    /// Caller classification is required even for classes with flexible
    /// treatment. DSR obligations, audit/billing evidence, and CAS references
    /// are retained here; a CAS reference is never a deletion claim about its
    /// potentially shared physical object.
    disposition: StagingLoadTestDisposition,
    /// Nonsecret opaque handle for the persistent resource.
    opaque_handle: &'a str,
}

impl core::fmt::Debug for StagingLoadTestResourceRegistration<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StagingLoadTestResourceRegistration")
            .field("run_id", &self.run_id)
            .field("scenario", &self.scenario)
            .field("target_deployment_sha", &self.target_deployment_sha)
            .field("resource_class", &self.resource_class)
            .field("disposition", &self.disposition)
            .field("opaque_handle", &"[REDACTED]")
            .finish()
    }
}

impl StagingLoadTestAdmissionContext {
    /// Require a writer family's fixed scenario before it can mutate state.
    pub(crate) fn require_ownership_scenario(
        &self,
        expected: StagingLoadTestScenario,
    ) -> Result<(), StagingLoadTestOwnershipError> {
        if self.scenario() == expected {
            Ok(())
        } else {
            Err(StagingLoadTestOwnershipError::ScenarioMismatch)
        }
    }

    /// Derive a registration whose run identity cannot be supplied by a caller.
    pub(crate) fn ownership_registration<'a>(
        &'a self,
        resource_class: StagingLoadTestResourceClass,
        disposition: StagingLoadTestDisposition,
        opaque_handle: &'a str,
    ) -> Result<StagingLoadTestResourceRegistration<'a>, StagingLoadTestOwnershipError> {
        let registration = StagingLoadTestResourceRegistration {
            run_id: self.run_id(),
            scenario: self.scenario(),
            target_deployment_sha: self.target_deployment_sha(),
            resource_class,
            disposition,
            opaque_handle,
        };
        validate_registration(&registration).map_err(|message| {
            if message.contains("disposition") {
                StagingLoadTestOwnershipError::InvalidDisposition
            } else {
                StagingLoadTestOwnershipError::InvalidIdentifier
            }
        })?;
        Ok(registration)
    }
}

impl StagingLoadTestResourceRegistration<'_> {
    /// Build the ledger statement that a writer appends to its domain batch.
    pub(crate) fn d1_statement(
        &self,
        registered_at_ms: i64,
    ) -> Result<D1BatchStatement, StagingLoadTestOwnershipError> {
        if registered_at_ms < 0 {
            return Err(StagingLoadTestOwnershipError::InvalidIdentifier);
        }
        validate_registration(self).map_err(|message| {
            if message.contains("disposition") {
                StagingLoadTestOwnershipError::InvalidDisposition
            } else {
                StagingLoadTestOwnershipError::InvalidIdentifier
            }
        })?;
        Ok(D1BatchStatement::new(
            SQL_REGISTER_RESOURCE_IN_BATCH,
            vec![
                json!(self.run_id),
                json!(self.scenario.as_str()),
                json!(self.resource_class.as_str()),
                json!(receipt_ref(self)),
                json!(self.opaque_handle),
                json!(self.disposition.as_str()),
                json!(registered_at_ms),
            ],
        ))
    }

    /// Bind an R2 operation to the same immutable admitted identity.
    pub(crate) fn r2_intent(
        &self,
        operation_id: &str,
    ) -> Result<StagingLoadTestR2Intent, StagingLoadTestOwnershipError> {
        if !is_lower_hex(operation_id, 64) {
            return Err(StagingLoadTestOwnershipError::InvalidIdentifier);
        }
        Ok(StagingLoadTestR2Intent {
            operation_id: operation_id.to_owned(),
            run_id: self.run_id.to_owned(),
            scenario: self.scenario,
            target_deployment_sha: self.target_deployment_sha.to_owned(),
            resource_class: self.resource_class,
            receipt_ref: receipt_ref(self),
            opaque_handle: self.opaque_handle.to_owned(),
            disposition: self.disposition,
        })
    }
}

/// Durable prepare/commit identity for mutations outside D1's transaction.
pub(crate) struct StagingLoadTestR2Intent {
    operation_id: String,
    run_id: String,
    scenario: StagingLoadTestScenario,
    target_deployment_sha: String,
    resource_class: StagingLoadTestResourceClass,
    receipt_ref: String,
    opaque_handle: String,
    disposition: StagingLoadTestDisposition,
}

impl core::fmt::Debug for StagingLoadTestR2Intent {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("StagingLoadTestR2Intent")
            .field("operation_id", &self.operation_id)
            .field("run_id", &self.run_id)
            .field("scenario", &self.scenario)
            .field("target_deployment_sha", &self.target_deployment_sha)
            .field("resource_class", &self.resource_class)
            .field("receipt_ref", &self.receipt_ref)
            .field("opaque_handle", &"[REDACTED]")
            .field("disposition", &self.disposition)
            .finish()
    }
}

impl StagingLoadTestR2Intent {
    /// Persist before the external R2 mutation is attempted.
    pub(crate) fn prepare_statement(&self, prepared_at_ms: i64) -> D1BatchStatement {
        D1BatchStatement::new(
            SQL_PREPARE_R2_INTENT,
            vec![
                json!(self.operation_id),
                json!(self.run_id),
                json!(self.scenario.as_str()),
                json!(self.target_deployment_sha),
                json!(self.resource_class.as_str()),
                json!(self.receipt_ref),
                json!(self.opaque_handle),
                json!(self.disposition.as_str()),
                json!(prepared_at_ms),
            ],
        )
    }

    /// Atomically register the resource and close its durable intent.
    pub(crate) fn commit_statements(&self, committed_at_ms: i64) -> [D1BatchStatement; 2] {
        let registration = D1BatchStatement::new(
            SQL_REGISTER_RESOURCE_IN_BATCH,
            vec![
                json!(self.run_id),
                json!(self.scenario.as_str()),
                json!(self.resource_class.as_str()),
                json!(self.receipt_ref),
                json!(self.opaque_handle),
                json!(self.disposition.as_str()),
                json!(committed_at_ms),
            ],
        );
        let close_intent = D1BatchStatement::new(
            SQL_COMMIT_R2_INTENT,
            vec![json!(self.operation_id), json!(committed_at_ms)],
        );
        [registration, close_intent]
    }
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Outcome of an attempted exact-run registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StagingLoadTestRegistrationOutcome {
    /// D1 inserted the row and returned this redacted receipt reference.
    Registered {
        /// Domain-separated SHA-256 receipt reference.
        receipt_ref: String,
    },
    /// No open staging run matched the supplied exact identity.
    NoMatchingOpenRun,
}

/// Restricted writer for the append-only staging load-test ownership ledger.
///
/// Construct with [`StagingLoadTestOwnershipWriter::from_d1_env`] so this path
/// loads only D1 scope and credentials, never R2 object-delete credentials.
pub struct StagingLoadTestOwnershipWriter {
    d1: D1HttpClient,
}

impl core::fmt::Debug for StagingLoadTestOwnershipWriter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StagingLoadTestOwnershipWriter")
            .field("d1", &"[REDACTED]")
            .finish()
    }
}

impl StagingLoadTestOwnershipWriter {
    /// Load the D1-only, writable capability needed by this adapter.
    ///
    /// The wrapped client is private, so this API exposes registration only;
    /// it does not expose arbitrary SQL or any delete operation.
    pub fn from_d1_env() -> Result<Self, String> {
        Ok(Self {
            d1: D1HttpClient::for_staging_load_test_ownership_writes()?,
        })
    }

    /// Append one run-owned resource reference to the migration 0147 ledger.
    ///
    /// The single `INSERT ... SELECT` binds the supplied run, scenario and
    /// deployment SHA to an exact open staging run. The caller supplies an
    /// explicit per-resource disposition. The adapter rejects disposable
    /// treatment for classes whose contract requires preservation or shared
    /// reference safety; it cannot delete or update ledger rows.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, a D1 failure/uniqueness conflict,
    /// or an unexpected D1 response. Error text does not include the opaque
    /// handle or any bound parameter.
    pub async fn register_staging_load_test_resource(
        &self,
        registration: StagingLoadTestResourceRegistration<'_>,
    ) -> Result<StagingLoadTestRegistrationOutcome, String> {
        validate_registration(&registration)?;

        let receipt_ref = receipt_ref(&registration);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system clock is before the Unix epoch".to_owned())?
            .as_millis();
        let now_ms = i64::try_from(now_ms)
            .map_err(|_| "system clock timestamp is out of range".to_owned())?;
        let params = [
            json!(registration.run_id),
            json!(registration.scenario.as_str()),
            json!(registration.target_deployment_sha),
            json!(registration.resource_class.as_str()),
            json!(receipt_ref),
            json!(registration.opaque_handle),
            json!(registration.disposition.as_str()),
            json!(now_ms),
        ];
        let rows = match self.d1.query(SQL_REGISTER_RESOURCE, &params).await {
            Ok(rows) => rows,
            Err(_) => {
                // A uniqueness error is the normal signal for an exact
                // idempotent retry. It can also indicate a conflicting
                // resource_class/opaque_handle owned by another run, so only
                // an exact identity lookup is allowed to recover success.
                let existing = self
                    .d1
                    .query(SQL_FIND_REGISTERED_RESOURCE, lookup_params(&params))
                    .await
                    .map_err(|_| "staging load-test resource registration failed".to_owned())?;
                return recover_exact_replay(&existing, &receipt_ref);
            }
        };

        let Some(row) = rows.first() else {
            return Ok(StagingLoadTestRegistrationOutcome::NoMatchingOpenRun);
        };
        let returned_receipt = row
            .get("receipt_ref")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "D1 returned a malformed registration result".to_owned())?;
        if returned_receipt != receipt_ref {
            return Err("D1 returned an unexpected registration receipt".to_owned());
        }
        Ok(StagingLoadTestRegistrationOutcome::Registered { receipt_ref })
    }
}

fn lookup_params(params: &[serde_json::Value; 8]) -> &[serde_json::Value] {
    // SQL_FIND_REGISTERED_RESOURCE uses ?1, ?2, and ?4 through ?7. Retain the
    // unused deployment SHA at ?3 so the positional binding indices stay
    // aligned with SQL_REGISTER_RESOURCE.
    &params[..7]
}

fn replay_receipt(
    rows: &[serde_json::Map<String, serde_json::Value>],
    expected_receipt: &str,
) -> Result<Option<String>, String> {
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let receipt = row
        .get("receipt_ref")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "D1 returned a malformed registration result".to_owned())?;
    if receipt != expected_receipt {
        return Err("D1 returned an unexpected registration receipt".to_owned());
    }
    Ok(Some(receipt.to_owned()))
}

fn recover_exact_replay(
    rows: &[serde_json::Map<String, serde_json::Value>],
    expected_receipt: &str,
) -> Result<StagingLoadTestRegistrationOutcome, String> {
    let Some(receipt_ref) = replay_receipt(rows, expected_receipt)? else {
        return Err("staging load-test resource registration failed".to_owned());
    };
    Ok(StagingLoadTestRegistrationOutcome::Registered { receipt_ref })
}

fn validate_registration(
    registration: &StagingLoadTestResourceRegistration<'_>,
) -> Result<(), String> {
    let run_id = registration.run_id;
    if run_id.is_empty()
        || run_id.len() > 20
        || run_id.starts_with('0')
        || !run_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("run_id must be canonical positive decimal text".to_owned());
    }
    let sha = registration.target_deployment_sha;
    if sha.len() != 40
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("target deployment SHA must be 40 lowercase hexadecimal characters".to_owned());
    }
    let handle = registration.opaque_handle;
    if handle.is_empty() || handle.len() > 512 || handle.chars().any(char::is_control) {
        return Err(
            "opaque resource handle must be 1-512 bytes without control characters".to_owned(),
        );
    }
    if registration.resource_class.requires_retention()
        && registration.disposition != StagingLoadTestDisposition::Retained
    {
        return Err("resource class requires retained disposition".to_owned());
    }
    Ok(())
}

fn receipt_ref(registration: &StagingLoadTestResourceRegistration<'_>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"corelink-staging-load-test-resource-receipt-v1\0");
    for part in [
        registration.run_id.as_bytes(),
        registration.scenario.as_str().as_bytes(),
        registration.target_deployment_sha.as_bytes(),
        registration.resource_class.as_str().as_bytes(),
        registration.opaque_handle.as_bytes(),
        registration.disposition.as_str().as_bytes(),
    ] {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration<'a>(
        run_id: &'a str,
        deployment_sha: &'a str,
        opaque_handle: &'a str,
        resource_class: StagingLoadTestResourceClass,
        disposition: StagingLoadTestDisposition,
    ) -> StagingLoadTestResourceRegistration<'a> {
        StagingLoadTestResourceRegistration {
            run_id,
            scenario: StagingLoadTestScenario::Cas,
            target_deployment_sha: deployment_sha,
            resource_class,
            disposition,
            opaque_handle,
        }
    }

    #[test]
    fn rejects_noncanonical_or_out_of_scope_identity() {
        let sha = "a".repeat(40);
        assert!(validate_registration(&registration(
            "01",
            &sha,
            "handle",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        ))
        .is_err());
        assert!(validate_registration(&registration(
            "1",
            &"A".repeat(40),
            "handle",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        ))
        .is_err());
        assert!(validate_registration(&registration(
            "1",
            &sha,
            "bad\nhandle",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        ))
        .is_err());
    }

    #[test]
    fn receipt_is_deterministic_but_run_scoped() {
        let sha = "a".repeat(40);
        let first = registration(
            "123",
            &sha,
            "opaque-id",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        );
        let same = registration(
            "123",
            &sha,
            "opaque-id",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        );
        let other_run = registration(
            "124",
            &sha,
            "opaque-id",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        );
        assert_eq!(receipt_ref(&first), receipt_ref(&same));
        assert_ne!(receipt_ref(&first), receipt_ref(&other_run));
    }

    #[test]
    fn retained_and_shared_reference_classes_reject_disposable_treatment() {
        let sha = "a".repeat(40);
        for class in [
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestResourceClass::DsrObligation,
            StagingLoadTestResourceClass::AuditEvidence,
            StagingLoadTestResourceClass::BillingAudit,
        ] {
            let registration = registration(
                "123",
                &sha,
                "opaque-id",
                class,
                StagingLoadTestDisposition::Disposable,
            );
            assert!(validate_registration(&registration).is_err());
        }
    }

    #[test]
    fn every_known_class_fails_closed_for_disposition_conflicts() {
        let sha = "a".repeat(40);
        let classes = [
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestResourceClass::WebhookInbox,
            StagingLoadTestResourceClass::WebhookEffect,
            StagingLoadTestResourceClass::DsrArtifact,
            StagingLoadTestResourceClass::DsrObligation,
            StagingLoadTestResourceClass::AuditEvidence,
            StagingLoadTestResourceClass::BillingAudit,
            StagingLoadTestResourceClass::SignupArtifact,
            StagingLoadTestResourceClass::ByokArtifact,
        ];
        for class in classes {
            for disposition in [
                StagingLoadTestDisposition::Disposable,
                StagingLoadTestDisposition::Retained,
            ] {
                let registration = registration("123", &sha, "opaque-id", class, disposition);
                let expected_valid = !class.requires_retention()
                    || disposition == StagingLoadTestDisposition::Retained;
                assert_eq!(validate_registration(&registration).is_ok(), expected_valid);
            }
        }
    }

    #[test]
    fn debug_redacts_opaque_handle() {
        let sha = "a".repeat(40);
        let value = registration(
            "123",
            &sha,
            "must-not-appear",
            StagingLoadTestResourceClass::WebhookInbox,
            StagingLoadTestDisposition::Disposable,
        );
        let rendered = format!("{value:?}");
        assert!(!rendered.contains("must-not-appear"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn d1_and_r2_contracts_accept_only_bounded_canonical_inputs() {
        let sha = "a".repeat(40);
        let value = registration(
            "123",
            &sha,
            "must-not-appear",
            StagingLoadTestResourceClass::CasReference,
            StagingLoadTestDisposition::Retained,
        );
        assert!(value.d1_statement(1).is_ok());
        assert!(value.d1_statement(-1).is_err());
        assert!(value.r2_intent(&"b".repeat(64)).is_ok());
        assert!(value.r2_intent(&"B".repeat(64)).is_err());
        assert!(value.r2_intent("short").is_err());
        let intent = value.r2_intent(&"c".repeat(64)).expect("valid intent");
        let _prepare = intent.prepare_statement(1);
        let _commit = intent.commit_statements(2);
        let rendered = format!("{intent:?}");
        assert!(!rendered.contains("must-not-appear"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn exact_replay_returns_original_receipt_and_other_run_lookup_fails_closed() {
        let expected_receipt = "b".repeat(64);
        let row = serde_json::Map::from_iter([(
            "receipt_ref".to_owned(),
            serde_json::Value::String(expected_receipt.clone()),
        )]);
        assert_eq!(
            recover_exact_replay(&[row], &expected_receipt),
            Ok(StagingLoadTestRegistrationOutcome::Registered {
                receipt_ref: expected_receipt.clone()
            })
        );
        // The SQL lookup is bound to the exact run and resource identity. A
        // collision on the global (resource_class, opaque_handle) key from a
        // different run therefore returns no row and remains an error.
        assert!(recover_exact_replay(&[], &expected_receipt).is_err());
    }

    #[test]
    fn replay_lookup_uses_first_seven_positional_values_with_skipped_three() {
        let params = std::array::from_fn(|index| json!(index + 1));
        assert_eq!(
            lookup_params(&params),
            &[
                json!(1),
                json!(2),
                json!(3),
                json!(4),
                json!(5),
                json!(6),
                json!(7)
            ]
        );
        assert!(SQL_FIND_REGISTERED_RESOURCE.contains("?1"));
        assert!(SQL_FIND_REGISTERED_RESOURCE.contains("?2"));
        assert!(!SQL_FIND_REGISTERED_RESOURCE.contains("?3"));
        for position in ["?4", "?5", "?6", "?7"] {
            assert!(SQL_FIND_REGISTERED_RESOURCE.contains(position));
        }
    }
}

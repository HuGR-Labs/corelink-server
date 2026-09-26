#!/usr/bin/env python3
"""Fail-closed source and census guard for issue #2581's DSR writer family."""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]

def read(path: str) -> str:
    return (ROOT / path).read_text()

DSR = read("crates/corelink-container/src/routes/dsr.rs")
AUDIT = read("crates/corelink-container/src/routes/dsr/audit.rs")
LEDGER = read("crates/corelink-container/src/routes/dsr/ledger.rs")
ACCESS = read("crates/corelink-container/src/routes/dsr/access.rs")
ATTESTATION = read("crates/corelink-container/src/routes/dsr/attestation.rs")
PORTAL = read("crates/corelink-container/src/routes/dsr/portal/part-00-01.rs")
CENSUS = read("docs/issue-2581-dsr-audit-writer-census.md")

required = {
    "DSR request admission is scenario-bound": "StagingLoadTestScenario::Dsr" in DSR and "admit_request(&state, &headers).await" in DSR,
    "invalid carrier fails closed": "downcast_ref::<StagingDsrRequestContext>()" in DSR and "DSR staging ownership context is invalid" in DSR,
    "audit trait context is verified": "emit_with_context" in AUDIT and "staging_admission_context(context)" in AUDIT,
    "audit insert and retained registration share D1 batch": "StagingLoadTestResourceClass::AuditEvidence" in AUDIT and "D1BatchStatement::new(sql, params), ownership" in AUDIT,
    "ledger uses retained DSR obligation rows": "StagingLoadTestResourceClass::DsrObligation" in LEDGER and "D1BatchStatement::new(ins, params), registration" in LEDGER,
    "ledger outcome update and registration are batched": "set_outcome_snapshot_with_context" in LEDGER and "d1_batch_blocking(&self.d1, statements)" in LEDGER,
    "right-event audit uses retained audit evidence": "StagingLoadTestResourceClass::AuditEvidence" in ACCESS and "D1BatchStatement::new(sql, params), registration" in ACCESS,
    "rectification update shares D1 batch with disposable artifact": "StagingLoadTestResourceClass::DsrArtifact" in ACCESS and 'D1BatchStatement::new(&plan.sql, params)' in ACCESS,
    "R2 object writes prepare and commit a durable intent": "intent.prepare_statement(prepared_at_ms)" in ACCESS and "intent.commit_statements(clamp_ms(now_ms))" in ACCESS,
    "R2 retry validates exact immutable identity before PUT": "staging R2 ownership intent conflicts with this write" in ACCESS and "target_deployment_sha" in ACCESS,
    "both export objects are attributed": "persist_owned_r2(" in ACCESS and "&sig_key" in ACCESS,
    "attestation R2 evidence is retained": "StagingLoadTestResourceClass::AuditEvidence" in ATTESTATION and "persist_owned_r2(" in ATTESTATION,
    "attestation D1 mutations share registration batches": "d1_batch_blocking(" in ATTESTATION and "erasure-attestation:" in ATTESTATION,
    "ordinary callers keep None-compatible path": "self.emit_attributed(record, None)" in AUDIT and "return self.set_outcome_snapshot(dsr_id, outcome_json)" in LEDGER,
    "ordinary portal callers explicitly pass None": "run_access(&self.d1, dsr_id, tenant_id, now_ms, None)" in PORTAL and "now_ms,\n            None," in PORTAL,
    "accepted-context D1 writer success and rollback tests exist": "admitted_access_audit_batches_registration_from_accepted_context" in ACCESS and "admitted_access_audit_batch_failure_rolls_back_domain_write" in ACCESS,
    "accepted-context R2 exact recovery and replay test exists": "admitted_r2_writer_recovers_exact_prepare_and_replays_once" in ACCESS and "failed external PUT never reports a registered artifact" in ACCESS,
    "accepted context is consumed through the shared verified-admission store": "consume_verified_admission(expectation, verified)" in ACCESS,
    "census records the closed family and exclusions": all(token in CENSUS for token in ("dsr_artifact", "dsr_obligation", "audit_evidence", "dsr_consumer.ts", "dsr_verify_cron.ts", "portal/part-00.rs", "adapter_r2_{cas,ac}.rs", "cas_retention")),
    "census rejects secret and PII claims": all(token not in CENSUS.lower() for token in ("raw nonce", "credential value", "personal email")),
}

missing = [name for name, present in required.items() if not present]
if missing:
    print("issue-2581 DSR/audit verification failed: " + ", ".join(missing), file=sys.stderr)
    raise SystemExit(1)
print("issue-2581 DSR/audit census and source guard: PASS")

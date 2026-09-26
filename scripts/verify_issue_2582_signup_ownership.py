#!/usr/bin/env python3
"""Fail-closed ownership census for signup-family writer changes (#2582)."""
from pathlib import Path
import os
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {
    ".github/workflows/issue-2582-signup-ownership.yml",
    "apps/signup-worker/src/signup_writer_ownership.ts",
    "apps/signup-worker/src/webhooks/clerk.ts",
    "apps/signup-worker/src/webhooks/github_install_callback.ts",
    "apps/signup-worker/src/webhooks/github_provision.ts",
    "apps/signup-worker/tests/signup_writer_ownership.test.ts",
    "crates/corelink-container/src/routes/signup.rs",
    "crates/corelink-container/src/routes/signup_support.rs",
    "crates/corelink-container/src/routes/signup_tests.rs",
    "crates/corelink-container/src/signup_d1_http.rs",
    "scripts/verify_issue_2582_signup_ownership.py",
}


def source(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def verify_diff_scope() -> None:
    base = os.environ.get("EXPECTED_BASE")
    head = os.environ.get("EXPECTED_HEAD")
    if not base or not head:
        raise RuntimeError("EXPECTED_BASE and EXPECTED_HEAD are required")
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", f"{base}...{head}"],
        cwd=ROOT,
        text=True,
    ).splitlines()
    outside = sorted(set(changed) - ALLOWED)
    if outside:
        raise RuntimeError("out-of-scope changed paths: " + ", ".join(outside))


def verify_census() -> None:
    rust_route = source("crates/corelink-container/src/routes/signup.rs")
    rust_d1 = source("crates/corelink-container/src/signup_d1_http.rs")
    clerk = source("apps/signup-worker/src/webhooks/clerk.ts")
    github = source("apps/signup-worker/src/webhooks/github_provision.ts")
    adapter = source("apps/signup-worker/src/signup_writer_ownership.ts")
    checks = {
        "Rust request consumes authenticated signup context":
            "admit_staging_load_test_request(" in rust_route
            and "StagingLoadTestScenario::Signup" in rust_route,
        "Rust signup record and ownership share D1 batch":
            "insert_or_existing_with_context" in rust_d1
            and "StagingLoadTestResourceClass::SignupArtifact" in rust_d1
            and "StagingLoadTestDisposition::Disposable" in rust_d1
            and ".batch(vec![" in rust_d1,
        "Clerk tenant and PAT writers use the signup adapter":
            "writeSignupArtifactBatch(" in clerk
            and "insertTenantStatement(" in clerk
            and "insertPatStatement(" in clerk,
        "Clerk org-map and entitlement writers use the signup adapter":
            "insertTenantOrgMapStatement(" in clerk
            and "seedTenantEntitlementStatements(" in clerk,
        "GitHub provisioning maps and batches signup_artifact":
            "writeSignupArtifactBatch(" in github
            and ":github-installation`" in github,
        "Worker verifies request-bound signup envelopes":
            'verifyStagingOwnershipEnvelope(envelope, "signup", requestId' in adapter
            and 'environment !== "staging"' in adapter,
        "Worker rejects registered handle before another write":
            "SELECT 1 AS present FROM staging_load_test_resources" in adapter
            and "if (prior !== null) throw" in adapter,
        "Worker registration is appended to one atomic batch":
            "runAtomicD1Batch(" in adapter
            and "ownership as unknown as SignupPreparedStatement" in adapter
            and "requireFreshOwnershipInsert(db, opaqueHandle)" in adapter
            and "AND changes() = 0 LIMIT 1" in adapter,
        "Clerk replay handles are stable and contain only the request identifier":
            "`${ownershipContext.requestId}:tenant`" in clerk
            and "`${ownershipContext.requestId}:pat`" in clerk
            and "`${ownershipContext.requestId}:org-map`" in clerk
            and "`${ownershipContext.requestId}:entitlements`" in clerk,
        "GitHub replay handle is stable and contains only the request identifier":
            "`${opts.ownershipContext.requestId}:github-installation`" in github,
        "Signup classification is the only registered resource class":
            adapter.count('"signup_artifact"') == 1
            and '"disposable"' in adapter,
        "No admission, migration, shared D1, or teardown ownership edits":
            all(path not in ALLOWED for path in (
                "apps/signup-worker/src/staging_load_test_ownership.ts",
                "apps/signup-worker/src/lib/d1.ts",
                "migrations/d1/0147_staging_load_test_run_ownership.sql",
                "crates/corelink-container/src/storage/staging_load_test_admission.rs",
            )),
    }
    missing = [label for label, passed in checks.items() if not passed]
    if missing:
        raise RuntimeError("census/adversarial verification failed: " + "; ".join(missing))


try:
    verify_diff_scope()
    verify_census()
except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
    print(f"issue-2582 signup ownership verification failed: {error}", file=sys.stderr)
    raise SystemExit(1) from error

print("issue-2582 signup ownership census: PASS")
print("class map: signup pilot / Clerk tenant, PAT, org-map, entitlement / GitHub installation bundle -> signup_artifact (disposable)")
print("excluded: Clerk provider-state mutation/publishUserMetadata (provider state is explicitly out of scope), provision locks/analytics, GitHub deprovision and audit outbox; sibling writer families remain untouched")

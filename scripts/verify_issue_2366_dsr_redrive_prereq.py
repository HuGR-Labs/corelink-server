#!/usr/bin/env python3
"""Static, credentialless contract gate for issue #2366."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


MIGRATION = Path("migrations/d1/0145_dsr_dlq_redrive_authority.sql")
SCHEMA_FENCE = Path("scripts/verify-signup-worker-dsr-redrive-schema.sh")
SECRET_FENCE = Path("scripts/verify-signup-worker-secrets.sh")
SIGNUP_DEPLOY = Path(".github/workflows/signup-worker-deploy.yml")
STAGING_BOOTSTRAP = Path(".github/workflows/staging-bootstrap.yml")
HOSTED_GATE = Path(".github/workflows/d1-migration-validate.yml")
STAGING_VERIFIER = Path("scripts/verify_staging_topology_contract.py")
STAGING_TOPOLOGY = Path("infra/staging/topology.json")
SECRETS_INVENTORY = Path("docs/internal/secrets-checklist.md")

ENVELOPE_COLUMNS = (
    "event_id", "dsr_id", "tenant_id", "queued_at_ms", "legal_hold", "requeue_count",
    "state", "actor_ref", "approval_ref", "expires_at_ms", "claim_expires_at_ms", "updated_at_ms",
)
AUDIT_COLUMNS = ("event_id", "transition")
ENVELOPE_STATES = "'ready', 'claimed', 'submitted', 'ambiguous'"
AUDIT_TRANSITIONS = "'claimed', 'submitted', 'ambiguous'"
PRODUCTION_SECRET = "DSR_DLQ_REDRIVE_AUTH_KEY"
STAGING_SECRET = "STAGING_DSR_DLQ_REDRIVE_AUTH_KEY"


def read(root: Path, path: Path) -> str:
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as error:
        raise RuntimeError(f"cannot read {path}: {error}") from error


def require(text: str, needle: str, label: str, gaps: list[str]) -> None:
    if needle not in text:
        gaps.append(label)


def table_body(sql: str, table: str, gaps: list[str]) -> str:
    match = re.search(
        rf"CREATE TABLE IF NOT EXISTS {re.escape(table)} \((?P<body>.*?)\);",
        sql,
        flags=re.S,
    )
    if not match:
        gaps.append(f"migration:{table}:definition")
        return ""
    return match.group("body")


def verify(root: Path) -> list[str]:
    gaps: list[str] = []
    migration = read(root, MIGRATION)
    require(migration, "CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_envelopes", "migration:envelopes-table", gaps)
    require(migration, "CREATE TABLE IF NOT EXISTS dsr_dlq_redrive_audit", "migration:audit-table", gaps)
    envelopes = table_body(migration, "dsr_dlq_redrive_envelopes", gaps)
    audit = table_body(migration, "dsr_dlq_redrive_audit", gaps)
    require(envelopes, "event_id TEXT PRIMARY KEY NOT NULL", "migration:opaque-event-primary-key", gaps)
    for column in ENVELOPE_COLUMNS:
        require(envelopes, f"  {column} ", f"migration:envelope-column:{column}", gaps)
    for column in AUDIT_COLUMNS:
        require(audit, f"  {column} ", f"migration:audit-column:{column}", gaps)
    require(envelopes, f"state IN ({ENVELOPE_STATES})", "migration:envelope-states", gaps)
    require(audit, f"transition IN ({AUDIT_TRANSITIONS})", "migration:audit-transitions", gaps)
    require(audit, "PRIMARY KEY (event_id, transition)", "migration:audit-primary-key", gaps)

    schema_fence = read(root, SCHEMA_FENCE)
    require(schema_fence, "set -euo pipefail", "schema-fence:strict-shell", gaps)
    require(schema_fence, "d1_migrations", "schema-fence:migration-ledger", gaps)
    require(schema_fence, "0145_dsr_dlq_redrive_authority.sql", "schema-fence:migration-name", gaps)
    require(schema_fence, "sqlite_master", "schema-fence:table-readback", gaps)
    require(schema_fence, "pragma_table_info('dsr_dlq_redrive_envelopes')", "schema-fence:envelope-column-readback", gaps)
    require(schema_fence, "pragma_table_info('dsr_dlq_redrive_audit')", "schema-fence:audit-column-readback", gaps)
    require(schema_fence, "--remote", "schema-fence:bound-d1", gaps)
    if re.search(r"\bmigrations\s+apply\b", schema_fence, flags=re.I):
        gaps.append("schema-fence:auto-apply")

    secret_fence = read(root, SECRET_FENCE)
    required = re.search(r"(?ms)^REQUIRED=\(\s*\n(?P<body>.*?)^\)\s*$", secret_fence)
    if not required or not re.search(rf"(?m)^\s*{PRODUCTION_SECRET}\s*$", required.group("body")):
        gaps.append("production-secret-fence")

    signup_deploy = read(root, SIGNUP_DEPLOY)
    schema_step = signup_deploy.find("- name: Verify DSR redrive schema before deploy")
    deploy_step = signup_deploy.find("- name: Deploy Worker")
    if schema_step < 0 or deploy_step < 0 or schema_step >= deploy_step:
        gaps.append("signup-deploy:schema-before-deploy")
    require(signup_deploy, "bash ../../scripts/verify-signup-worker-dsr-redrive-schema.sh", "signup-deploy:schema-command", gaps)
    require(signup_deploy, "migrations/d1/0145_dsr_dlq_redrive_authority.sql", "signup-deploy:migration-trigger", gaps)

    hosted_gate = read(root, HOSTED_GATE)
    require(
        hosted_gate,
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
        "hosted-gate:exact-head-checkout",
        gaps,
    )
    require(
        hosted_gate,
        "EXPECTED_HEAD: ${{ github.event.pull_request.head.sha || github.sha }}",
        "hosted-gate:expected-head",
        gaps,
    )
    require(
        hosted_gate,
        'test "$(git rev-parse HEAD)" = "$EXPECTED_HEAD"',
        "hosted-gate:exact-head-assertion",
        gaps,
    )
    require(hosted_gate, "python3 -I scripts/verify_issue_2366_dsr_redrive_prereq.py", "hosted-gate:contract", gaps)
    require(hosted_gate, "python3 -I tests/test_issue_2366_dsr_redrive_prereq.py", "hosted-gate:mutations", gaps)

    staging = read(root, STAGING_BOOTSTRAP)
    require(staging, f"DSR_DLQ_REDRIVE_AUTH_KEY: ${{{{ secrets.{STAGING_SECRET} }}}}", "staging:secret-inventory", gaps)
    require(staging, "STAGING_DSR_DLQ_REDRIVE_AUTH_KEY", "staging:required-name", gaps)
    require(staging, '"$signup:DSR_DLQ_REDRIVE_AUTH_KEY:DSR_DLQ_REDRIVE_AUTH_KEY"', "staging:secret-binding", gaps)
    staging_fence = staging.find("bash scripts/verify-signup-worker-dsr-redrive-schema.sh")
    staging_deploy = staging.find("pnpm exec wrangler deploy --config")
    if staging_fence < 0 or staging_deploy < 0 or staging_fence >= staging_deploy:
        gaps.append("staging:schema-before-deploy")

    staging_verifier = read(root, STAGING_VERIFIER)
    require(staging_verifier, f'"{PRODUCTION_SECRET}"', "staging-verifier:worker-secret", gaps)
    require(staging_verifier, f'"{STAGING_SECRET}"', "staging-verifier:environment-secret", gaps)
    try:
        topology = json.loads(read(root, STAGING_TOPOLOGY))
        names = topology["required_secret_names"]
        if PRODUCTION_SECRET not in names["corelink-signup-staging"]:
            gaps.append("staging-topology:worker-secret")
        if STAGING_SECRET not in names["github_environment_staging"]:
            gaps.append("staging-topology:environment-secret")
    except (json.JSONDecodeError, KeyError, TypeError):
        gaps.append("staging-topology:secret-contract")

    inventory = read(root, SECRETS_INVENTORY)
    require(inventory, f"| 297 | DSR DLQ redrive authority key (dedicated, >=32 chars) | `{PRODUCTION_SECRET}`", "secrets-inventory:production", gaps)
    require(inventory, f"| 298 | Staging DSR DLQ redrive authority key (dedicated, >=32 chars) | `{STAGING_SECRET}`", "secrets-inventory:staging", gaps)
    return gaps


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    try:
        gaps = verify(args.root.resolve())
    except RuntimeError as error:
        print(f"issue-2366 contract ERROR: {error}", file=sys.stderr)
        return 2
    if gaps:
        print("issue-2366 contract FAIL")
        for gap in gaps:
            print(f"- {gap}")
        return 1
    print("issue-2366 contract PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

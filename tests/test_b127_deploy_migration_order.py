"""Fail-closed guards for the B-127 writer-first production cutover."""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/cf-deploy-prod.yml"
DEPLOY_SCRIPT = ROOT / "scripts/deploy-container-prod.sh"


def _workflow(text: str) -> dict:
    parsed = yaml.safe_load(text)
    assert isinstance(parsed, dict)
    jobs = parsed.get("jobs")
    assert isinstance(jobs, dict)
    return jobs


def _assert_writer_first(jobs: dict) -> None:
    deploy = jobs.get("deploy")
    migrate = jobs.get("migrate-d1")
    assert isinstance(deploy, dict)
    assert isinstance(migrate, dict)

    # The matrix aggregate is the proof boundary: a migration may not depend
    # on a gate that can finish while one writer leg is still stale or failed.
    assert deploy["needs"] == [
        "gate-secrets-checklist",
        "gate-cf-secrets-populated",
    ]
    assert migrate["needs"] == "deploy"
    condition = str(migrate.get("if", ""))
    assert "needs.deploy.result == 'success'" in condition
    # A single-env dispatch proves only one of the five shared-D1 writers.
    assert "github.event.inputs.env == ''" in condition
    # Recycle/rollback is allowed to deploy a pinned image, but never to
    # authorize a schema mutation.
    assert "github.event.inputs.skip_pin_freshness != 'true'" in condition


def test_migration_is_downstream_of_the_healthy_writer_matrix() -> None:
    _assert_writer_first(_workflow(WORKFLOW.read_text(encoding="utf-8")))


def test_dependency_mutation_reopens_writer_first_boundary() -> None:
    original = WORKFLOW.read_text(encoding="utf-8")
    mutated = original.replace("    needs: deploy\n", "    needs: gate-secrets-checklist\n", 1)
    assert mutated != original
    with pytest.raises(AssertionError):
        _assert_writer_first(_workflow(mutated))


def test_rollback_and_single_env_mutations_reopen_schema_guard() -> None:
    original = WORKFLOW.read_text(encoding="utf-8")
    for marker in (
        "github.event.inputs.env == ''",
        "github.event.inputs.skip_pin_freshness != 'true'",
    ):
        mutated = original.replace(marker, "true", 1)
        assert mutated != original
        with pytest.raises(AssertionError):
            _assert_writer_first(_workflow(mutated))


def test_deploy_helper_is_the_non_bypassable_image_and_health_proof() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    script = DEPLOY_SCRIPT.read_text(encoding="utf-8")

    assert 'bash scripts/deploy-container-prod.sh --apply --env "${WRANGLER_ENV}"' in workflow
    _assert_health_proof(script)


def _assert_health_proof(script: str) -> None:
    assert 'configuration.image' in script
    assert 'if [ "$failed" -ne 0 ]; then' in script
    assert 'if [ "$desired" -gt 0 ] && [ "$healthy" -lt 1 ]; then' in script
    assert 'CONFIRM_POLLS=2' in script
    assert 'check-container-pin-fresh.sh' in script


def test_health_proof_mutation_reopens_contract() -> None:
    script = DEPLOY_SCRIPT.read_text(encoding="utf-8")
    mutated = script.replace('if [ "$failed" -ne 0 ]; then', 'if false; then', 1)
    assert mutated != script
    with pytest.raises(AssertionError):
        _assert_health_proof(mutated)

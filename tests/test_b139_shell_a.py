"""Regression coverage for B139 shell-boundary fixes."""

from pathlib import Path
import subprocess


ROOT = Path(__file__).parents[1]
WORKFLOWS = {
    "audit-chain-daily-verify.yml": "WINDOW_DAYS_INPUT",
    "backup-daily-verify.yml": "DRY_INPUT",
    "backup-daily.yml": "DRY_INPUT",
    "billing-aggregate-runner.yml": "BILLING_PERIOD_INPUT",
    "billing-reconcile-daily.yml": "BILLING_PERIOD_INPUT",
    "byok_kill_switch_drill_weekly.yml": "PROVIDER_INPUT",
    "container-build-push-prod.yml": "CONFIRM_INPUT",
}


def _run_blocks(text: str) -> list[str]:
    blocks: list[str] = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip() != "run: |":
            continue
        indent = len(line) - len(line.lstrip())
        body: list[str] = []
        for candidate in lines[i + 1 :]:
            candidate_indent = len(candidate) - len(candidate.lstrip())
            if candidate.strip() and candidate_indent <= indent:
                break
            body.append(candidate)
        blocks.append("\n".join(body))
    return blocks


def test_dispatch_values_are_env_bound_in_shell_blocks() -> None:
    forbidden = ("${{ inputs.", "${{ github.event.inputs.")
    for filename, env_name in WORKFLOWS.items():
        text = (ROOT / ".github/workflows" / filename).read_text()
        assert any(f"{env_name}: ${{{{" in line for line in text.splitlines())
        assert all(not any(token in block for token in forbidden) for block in _run_blocks(text))


def _bash(script: str, **env: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["bash", "-euo", "pipefail", "-c", script], env={**env}, text=True, capture_output=True)


def test_numeric_and_date_guards_reject_shell_payloads() -> None:
    numeric = '[[ "$VALUE" =~ ^[0-9]+$ ]] && (( VALUE >= 1 && VALUE <= 30 ))'
    date = '[[ "$VALUE" =~ ^[0-9]{4}-[0-9]{2}$ ]]'
    for script in (numeric, date):
        result = _bash(script, VALUE='7"; touch /tmp/b139-pwned; #')
        assert result.returncode != 0


def test_provider_and_confirm_guards_reject_invalid_input() -> None:
    provider = '[[ "$VALUE" =~ ^(aws|gcp|azure|vault)$ ]]'
    confirm = '[ "$VALUE" = build ]'
    dry_run = '[[ "$VALUE" = true || "$VALUE" = false ]]'
    assert _bash(provider, VALUE='aws"; id; #').returncode != 0
    assert _bash(provider, VALUE="evil").returncode != 0
    assert _bash(confirm, VALUE='build"; id; #').returncode != 0
    assert _bash(dry_run, VALUE='true"; id; #').returncode != 0


def test_actual_workflow_guards_reject_malicious_env_and_invalid_month() -> None:
    for filename in ("backup-daily-verify.yml", "backup-daily.yml"):
        text = (ROOT / ".github/workflows" / filename).read_text()
        assert "CORELINK_ENV_INPUT: ${{ github.event.inputs.env || 'production' }}" in text
        assert '[[ "$CORELINK_ENV_INPUT" != production && "$CORELINK_ENV_INPUT" != staging ]]' in text
        assert "options: [production, staging]" in text
        assert "environment: ${{ github.event.inputs.env || 'production' }}" in text
        assert '[[ "$CORELINK_ENV_INPUT" == staging && "$DRY" != true ]]' in text
    for filename in ("billing-aggregate-runner.yml", "billing-reconcile-daily.yml"):
        text = (ROOT / ".github/workflows" / filename).read_text()
        assert "^[0-9]{4}-(0[1-9]|1[0-2])$" in text
        guard = '[[ "$VALUE" =~ ^[0-9]{4}-(0[1-9]|1[0-2])$ ]]'
        assert _bash(guard, VALUE="2026-00").returncode != 0
        assert _bash(guard, VALUE="2026-13").returncode != 0
    assert _bash('[[ "$VALUE" = production || "$VALUE" = staging ]]', VALUE='production"; id; #').returncode != 0


def test_staging_live_is_rejected_but_staging_dry_run_is_allowed() -> None:
    guard = (
        'if [[ "$ENV" == staging && "$DRY" != true ]]; then exit 1; fi'
    )
    assert _bash(guard, ENV="staging", DRY="false").returncode != 0
    assert _bash(guard, ENV="staging", DRY="true").returncode == 0

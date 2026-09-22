"""Static mutation checks for the B-125 provider diagnostic receipt."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/b125-audit-throughput-read-only.yml"


def test_failed_wrangler_query_retains_bounded_redacted_provider_context() -> None:
    source = WORKFLOW.read_text(encoding="utf-8")
    assert '2>"$stderr"' in source
    assert '"provider_failures"' in source
    assert '"exit_code"' in source
    assert '"stderr_sha256"' in source
    assert 'redacted = re.sub' in source
    assert 'api[_ -]?token' in source
    assert '"stderr": redacted' in source
    assert '"stderr": raw' not in source
    assert 'redacted = redacted[-2000:]' in source
    assert 'fail_closed "wrangler_query_failed_${id}_${status}"' in source


def test_raw_stderr_is_runner_temp_only_and_never_uploaded() -> None:
    source = WORKFLOW.read_text(encoding="utf-8")
    assert 'raw_dir="$RUNNER_TEMP/b125-d1"' in source
    assert 'path: artifacts/b125-audit-throughput-receipt.json' in source
    assert 'path: "$raw_dir"' not in source
    assert 'path: "$stderr"' not in source


def test_diagnostic_does_not_weaken_read_only_or_remote_guards() -> None:
    source = WORKFLOW.read_text(encoding="utf-8")
    assert '[[ "$sql" =~ ^[[:space:]]*SELECT[[:space:]] ]]' in source
    assert '[[ "$sql" != *\';\'* ]]' in source
    assert '--remote --command "$sql" --json' in source
    assert "--local" not in source

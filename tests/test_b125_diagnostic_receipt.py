"""Static mutation checks for the B-125 provider diagnostic receipt."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

from b125_redact_provider_error import redact_provider_stderr, record_provider_failure


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/b125-audit-throughput-read-only.yml"
SCRUBBER = ROOT / "scripts/b125_redact_provider_error.py"


def test_failed_wrangler_query_retains_bounded_redacted_provider_context() -> None:
    source = WORKFLOW.read_text(encoding="utf-8")
    scrubber = SCRUBBER.read_text(encoding="utf-8")
    assert '2>"$stderr"' in source
    assert '"provider_failures"' in scrubber
    assert '"exit_code"' in scrubber
    assert '"stderr_sha256"' in scrubber
    assert 'b125_redact_provider_error.py "$receipt" "$id" "$status" "$stderr"' in source
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


def test_scrubber_executes_against_bearer_prefixed_env_and_generic_secrets(tmp_path: Path) -> None:
    raw = "Authorization: Bearer TOPSECRET CF_API_TOKEN=TOPSECRET2 secret: TOPSECRET3"
    redacted = redact_provider_stderr(raw)
    assert "TOPSECRET" not in redacted
    assert "[REDACTED]" in redacted


def test_recorded_excerpt_is_bounded_and_hashes_raw_input(tmp_path: Path) -> None:
    receipt = tmp_path / "receipt.json"
    stderr = tmp_path / "stderr"
    receipt.write_text("{}\n", encoding="utf-8")
    raw = "CF_API_TOKEN=TOPSECRET " + ("x" * 2100)
    stderr.write_text(raw, encoding="utf-8")
    record_provider_failure(receipt, "hourly", 1, stderr)
    entry = json.loads(receipt.read_text(encoding="utf-8"))["provider_failures"][0]
    assert len(entry["stderr"]) <= 2000
    assert "TOPSECRET" not in entry["stderr"]
    assert entry["stderr_sha256"]

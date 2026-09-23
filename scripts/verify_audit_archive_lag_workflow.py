#!/usr/bin/env python3
"""Check the hosted audit-lag monitor and its manual no-page boundary."""

from __future__ import annotations

import re
from pathlib import Path


WORKFLOW = Path(".github/workflows/audit-archive-lag.yml")
PAGERDUTY_GATE = "if: steps.measure.outputs.page == '1' && steps.measure.outputs.notify == 'true'"
ARCHIVE_JOB_GATE = (
    "github.repository == 'HuGR-dev/corelink-server' && "
    "github.ref == 'refs/heads/main' && github.ref_protected == true && "
    "(github.event_name == 'schedule' || github.event_name == 'workflow_dispatch')"
)
VERDICT = "page = (clause1 and clause2) or (partitions_failed > 0)"
PAGERDUTY_URL = "https://events.pagerduty.com/v2/enqueue"


def _normalized(source: str) -> str:
    return re.sub(r"\s+", " ", source)


def verify(source: str) -> None:
    normalized = _normalized(source)
    archive_match = re.search(
        r"(?ms)^  archive-lag:\n(?P<body>.*?)(?=^  [a-z0-9_-]+:\n|\Z)",
        source,
    )
    if archive_match is None:
        raise AssertionError("missing archive-lag job")
    archive_job = archive_match.group("body")

    required = (
        "runs-on: ubuntu-24.04",
        "workflow_dispatch:",
        'default: "read-only"',
        'options: ["read-only", "page"]',
        "from urllib.request import Request, urlopen",
        VERDICT,
        "os.environ.get(\"GITHUB_EVENT_NAME\") == \"schedule\"",
        'os.environ.get("NOTIFICATION_MODE") == "page"',
        "fh.write(f\"notify={'true' if notify else 'false'}\\n\")",
        PAGERDUTY_GATE,
        ARCHIVE_JOB_GATE,
    )
    for marker in required:
        if marker not in normalized:
            raise AssertionError(f"missing hosted monitor invariant: {marker}")

    if "runs-on: ubuntu-24.04" not in archive_job:
        raise AssertionError("archive-lag must use GitHub-hosted ubuntu-24.04")
    if _normalized(ARCHIVE_JOB_GATE) not in normalized:
        raise AssertionError("manual live reads must be limited to protected canonical main")
    if "NOTIFICATION_MODE: ${{ github.event.inputs.notification_mode || 'read-only' }}" not in normalized:
        raise AssertionError("manual dispatch must default to read-only/no-page mode")
    final_failure = re.search(
        r"(?ms)^      - name: Fail the job when the archive is absent[^\n]*\n(?P<body>.*?)(?=^      - name:|\Z)",
        source,
    )
    if final_failure is None or "if: steps.measure.outputs.page == '1'" not in final_failure.group("body"):
        raise AssertionError("a red full-detector verdict must still fail the read-only run")
    if "exit 1" not in final_failure.group("body"):
        raise AssertionError("a red full-detector verdict must exit non-zero")
    pagerduty_start = source.find("      - name: PagerDuty SEV-0 page")
    pagerduty_end = source.find("\n      - name:", pagerduty_start + 1)
    pagerduty_step = source[pagerduty_start:pagerduty_end if pagerduty_end >= 0 else None]
    if pagerduty_start < 0 or PAGERDUTY_GATE not in _normalized(pagerduty_step):
        raise AssertionError("the sole PagerDuty request must require a failing verdict and notify=true")
    if source.count(PAGERDUTY_URL) != 1:
        raise AssertionError("PagerDuty enqueue must exist exactly once inside the guarded step")
    if re.search(r"(?im)^\s*\"(?:UPDATE|INSERT|DELETE|REPLACE|ALTER|DROP|CREATE)\b", source):
        raise AssertionError("D1 query literals must remain SELECT-only")
    for action in re.findall(r"uses:\s*([^\s#]+)", source):
        if not re.search(r"@[0-9a-f]{40}$", action):
            raise AssertionError(f"unpinned action reference: {action}")


def verify_adversarial_mutations(source: str) -> None:
    mutations = (
        (
            source.replace("    runs-on: ubuntu-24.04\n    timeout-minutes: 10", "    runs-on: corelink\n    timeout-minutes: 10", 1),
            "self-hosted archive runner",
        ),
        (
            source.replace(PAGERDUTY_GATE, "if: steps.measure.outputs.page == '1'", 1),
            "paging without notification-mode gate",
        ),
        (
            source.replace('default: "read-only"', 'default: "page"', 1),
            "unsafe manual default",
        ),
        (
            source.replace("github.ref_protected == true", "github.ref_protected == false", 1),
            "unprotected manual live read",
        ),
        (
            re.sub(
                r"(?ms)^      - name: Fail the job when the archive is absent[^\n]*\n.*?(?=^      - name:|\Z)",
                "",
                source,
                count=1,
            ),
            "read-only PAGE verdict made green",
        ),
        (
            source.replace('"SELECT COUNT(*) AS pending_old, MIN(enqueued_at)', '"DELETE FROM audit_outbox', 1),
            "mutating D1 query",
        ),
        (
            source + f'\n# adversarial unguarded call: {PAGERDUTY_URL}\n',
            "extra unguarded PagerDuty request",
        ),
    )
    for mutated, label in mutations:
        if mutated == source:
            raise AssertionError(f"adversarial mutation fixture did not apply: {label}")
        try:
            verify(mutated)
        except AssertionError:
            continue
        raise AssertionError(f"verifier accepted adversarial mutation: {label}")


def main() -> int:
    source = WORKFLOW.read_text(encoding="utf-8")
    verify(source)
    verify_adversarial_mutations(source)
    print("audit-archive-lag hosted/read-only contract and adversarial checks: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

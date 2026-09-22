#!/usr/bin/env python3
"""Credentialless static contract for the B-122 production evidence lane."""

from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-1667-b122-production-evidence.yml")


def main() -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    required = (
        "workflow_dispatch:",
        "approval:",
        "production_base:",
        "dogfood_tenant:",
        "serving_sha:",
        "region:",
        "environment: production",
        "permissions:\n  contents: read",
        "persist-credentials: false",
        "secrets.CORELINK_DOGFOOD_PAT",
        "timeout 25s curl",
        "--connect-timeout 10",
        "--max-time 20",
        "--data-binary @<(head -c 1024 /dev/zero)",
        "-X PUT",
        "b102-warm",
        "b107-separated",
        "X-Server-Timing-Wdb-Detail: on",
        "verify_d03_timing_artifact.py --writes artifacts/b122/b102-warm.txt",
        "verify_d03_timing_artifact.py --writes artifacts/b122/b107-separated.txt",
        "sample-started-utc.txt",
        "sample_started_utc",
        "shape-check-b102.txt",
        "shape-check-b107.txt",
        "shape_checks",
        "upload-artifact",
        "retention-days: 30",
    )
    missing = [needle for needle in required if needle not in text]
    forbidden = [needle for needle in ("wrangler deploy", "kubectl apply", "terraform apply", "gh issue close") if needle in text]
    if missing:
        raise SystemExit("missing lane contract: " + ", ".join(missing))
    if forbidden:
        raise SystemExit("forbidden mutation command: " + ", ".join(forbidden))
    print("B-122 production evidence lane contract: PASS")


if __name__ == "__main__":
    main()

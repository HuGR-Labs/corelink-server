#!/usr/bin/env python3
"""Reject unsafe mutations and fleet-claim drift in the #1670 workflow."""
from pathlib import Path

source = (Path(__file__).parents[1] / ".github/workflows/issue-1670-fleet-classification.yml").read_text()
required = (
    "runs-on: ubuntu-24.04",
    "I1670_FLEET_LABEL: github-hosted",
    "Issue #1670 GitHub-hosted classification evidence",
    "github.ref_protected",
    "CORELINK_I1670_FIXTURE_MOUNT: /mnt/corelink-i1670/${{ github.run_id }}",
    "CORELINK_I1670_FIXTURE_MAX_BYTES: '134217728'",
    'expected_fixture_mount="/mnt/corelink-i1670/${GITHUB_RUN_ID}"',
    'test "$fixture_mount_target" = "$fixture_mount"',
    'test "$fixture_fs_type" = tmpfs',
    'test "$fixture_bytes" -le "$CORELINK_I1670_FIXTURE_MAX_BYTES"',
    "runner_identity_sha256",
    "hashlib.sha256",
    "cleanup_readback",
    "issue-1670-enospc-sanitized.log",
    "issue-1670-linker-sanitized.log",
    "GITHUB_STEP_SUMMARY=/dev/null",
)
for item in required:
    if item not in source:
        raise SystemExit(f"missing fleet safety contract: {item}")
for item in ("/dev/shm", "issue-1670-enospc.log", "issue-1670-linker.log",
             'echo "- runner: label=', '"name": os.environ["I1670_RUNNER_NAME"]',
             "I1670_FLEET_LABEL: corelink", 'test "$I1670_FLEET_LABEL" = corelink',
             "CoreLink fleet evidence"):
    if item in source:
        raise SystemExit(f"unsafe fleet workflow fragment: {item}")

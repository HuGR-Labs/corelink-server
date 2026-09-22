#!/usr/bin/env python3
"""Reject unsafe mutations of the #1670 fleet evidence workflow."""
from pathlib import Path

source = (Path(__file__).parents[1] / ".github/workflows/issue-1670-fleet-classification.yml").read_text()
required = (
    "runs-on: corelink",
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
             'echo "- runner: label=', '"name": os.environ["I1670_RUNNER_NAME"]'):
    if item in source:
        raise SystemExit(f"unsafe fleet workflow fragment: {item}")

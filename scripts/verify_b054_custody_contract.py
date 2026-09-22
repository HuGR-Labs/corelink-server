#!/usr/bin/env python3
"""Static contract for the protected, nonproduction B-054 custody drill."""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_PATH = ".github/workflows/b054-key-custody-drill.yml"
TEST_PATH = "crates/corelink-audit-chain/tests/b054_custody_rotation.rs"
DOC_PATH = "docs/operator/b054-key-custody-rotation.md"
SCANNER_PATH = "scripts/verify_b054_custody_redaction.py"
SECRETS = (
    "B054_E1_SIGNING_SEED_HEX",
    "B054_E1_LINK_KEY_HEX",
    "B054_E2_SIGNING_SEED_HEX",
    "B054_E2_LINK_KEY_HEX",
)


class ContractError(RuntimeError):
    pass


def section_after(text: str, marker: str, next_marker: str) -> str:
    try:
        start = text.index(marker)
        end = text.index(next_marker, start + len(marker))
    except ValueError as error:
        raise ContractError(f"workflow section boundary is missing: {marker}") from error
    return text[start:end]


def assess(files: dict[str, str]) -> None:
    workflow = files["workflow"]
    rust_test = files["rust_test"]
    doc = files["doc"]
    scanner = files["scanner"]

    for token in ("pull_request:", "workflow_dispatch:", "permissions:\n  contents: read"):
        if token not in workflow:
            raise ContractError(f"workflow is missing {token!r}")
    # Extract the peer job irrespective of ordering in the workflow document.
    start = workflow.find("  protected-rotation:\n")
    if start < 0:
        raise ContractError("protected rotation job is missing")
    tail = workflow[start:]
    boundary = re.search(r"\n  [a-z][a-z0-9-]*:\n", tail[1:])
    protected_job = tail if boundary is None else tail[: boundary.start() + 1]

    gate = "".join(
        (
            "github.repository == 'HuGR-dev/corelink-server'",
            "github.event_name == 'workflow_dispatch'",
            "github.ref == 'refs/heads/main'",
            "github.ref_protected",
        )
    )
    if any(token not in protected_job for token in gate.split("github.")[1:]):
        raise ContractError("protected job must require canonical repo, manual main dispatch, and protected ref")
    if "environment: b054-key-custody-drill" not in protected_job:
        raise ContractError("protected job must use the pre-provisioned nonproduction environment")
    if "persist-credentials: false" not in protected_job:
        raise ContractError("checkout credentials must be disabled")

    used_secrets = re.findall(r"secrets\.([A-Z0-9_]+)", workflow)
    if sorted(used_secrets) != sorted(SECRETS):
        raise ContractError("workflow must consume each of the four approved secret names exactly once")
    for secret in SECRETS:
        if f"{secret}: ${{{{ secrets.{secret} }}}}" not in protected_job:
            raise ContractError(f"{secret} must be scoped to the protected runtime step")
    if "secrets." in workflow[: workflow.index("  protected-rotation:")]:
        raise ContractError("pull request and compile jobs must not receive protected values")

    test_run = protected_job.find("Run synthetic rotation and scan before publishing output")
    scan_run = protected_job.find("verify_b054_custody_redaction.py")
    receipt_publish = protected_job.find('cat "$B054_RECEIPT_PATH"')
    upload = protected_job.find("actions/upload-artifact")
    if min(test_run, scan_run, receipt_publish, upload) < 0 or not test_run < scan_run < receipt_publish < upload:
        raise ContractError("runtime output must be scanned before summary and artifact publication")
    if "path: ${{ runner.temp }}/b054-custody-receipt.json" not in protected_job:
        raise ContractError("artifact must contain only the runner-temporary redacted receipt")
    if "retention-days: 90" not in protected_job:
        raise ContractError("receipt artifact retention must be bounded")

    normalized_doc = " ".join(doc.split())
    for token in (
        "#[ignore",
        "ChainEpoch::keyed_successor",
        "LinkKeyring::parse_json",
        "split_verifying_prefix_for_epoch",
        '"historic_key_revocation": "not_performed_before_retention_expiry"',
        '"d1_touched": false',
        '"r2_touched": false',
        '"worker_touched": false',
        "Zeroizing",
    ):
        if token not in rust_test:
            raise ContractError(f"rotation test is missing {token!r}")
    if "std::net::" in rust_test or "reqwest::" in rust_test or "wrangler" in rust_test:
        raise ContractError("protected exercise must stay in-memory and offline")

    for token in (
        '["git", "ls-files", "-z"]',
        "bytes.fromhex(value)",
        "value.upper().encode(\"ascii\")",
        "decoded))",
        "TRANSCRIPT RECEIPT",
        "tracked text, transcript, and receipt",
    ):
        if token not in scanner:
            raise ContractError(f"redaction scanner is missing {token!r}")
    if "output withheld" not in scanner:
        raise ContractError("redaction failures must not publish test output")

    for token in (
        "b054-key-custody-drill",
        "self-approval is disabled",
        "administrator bypass is disabled",
        "only `main` is an allowed branch",
        "Keep each historical name through the linked audit-data retention horizon",
        "Never put a key value",
    ):
        if token not in normalized_doc:
            raise ContractError(f"operator procedure is missing {token!r}")
    for secret in SECRETS:
        if re.search(rf"{secret}\s*=\s*[\"'][0-9a-fA-F]{{64}}[\"']", doc):
            raise ContractError("operator docs must contain approved names only, never key values")


def read_files() -> dict[str, str]:
    return {
        "workflow": (ROOT / WORKFLOW_PATH).read_text(encoding="utf-8"),
        "rust_test": (ROOT / TEST_PATH).read_text(encoding="utf-8"),
        "doc": (ROOT / DOC_PATH).read_text(encoding="utf-8"),
        "scanner": (ROOT / SCANNER_PATH).read_text(encoding="utf-8"),
    }


def main() -> int:
    try:
        assess(read_files())
    except (OSError, ContractError) as error:
        print(f"B-054 custody contract: FAIL ({error})", file=sys.stderr)
        return 1
    print("B-054 custody contract: PASS (protected execution and redaction boundaries held)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

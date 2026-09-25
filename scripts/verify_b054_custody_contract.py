#!/usr/bin/env python3
"""Static safety contract for the protected B-054 custody drill."""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ".github/workflows/b054-key-custody-drill.yml"
TEST = "crates/corelink-audit-chain/tests/b054_custody_rotation.rs"
DOC = "docs/operator/b054-key-custody-rotation.md"
APPROVAL = "scripts/verify_b054_custody_approval.py"
SCANNER = "scripts/verify_b054_custody_redaction.py"
SECRETS = (
    "B054_E1_SIGNING_SEED_HEX",
    "B054_E1_LINK_KEY_HEX",
    "B054_E2_SIGNING_SEED_HEX",
    "B054_E2_LINK_KEY_HEX",
)


class ContractError(RuntimeError):
    pass


def section(text: str, start: str, end: str | None = None) -> str:
    try:
        offset = text.index(start)
        limit = len(text) if end is None else text.index(end, offset + len(start))
    except ValueError as error:
        raise ContractError(f"required workflow section is missing: {start}") from error
    return text[offset:limit]


def assess(files: dict[str, str]) -> None:
    workflow = files["workflow"]
    test = files["test"]
    doc = files["doc"]
    approval = files["approval"]
    scanner = files["scanner"]
    protected = section(workflow, "  protected-rotation:\n")

    for token in (
        "pull_request:",
        "workflow_dispatch:",
        "permissions:\n  contents: read",
    ):
        if token not in workflow:
            raise ContractError(f"workflow is missing {token!r}")

    for token in (
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.event_name == 'workflow_dispatch'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "github.run_attempt == 1",
        "needs: static-contract",
        "environment: b054-key-custody-drill",
        "persist-credentials: false",
    ):
        if token not in protected:
            raise ContractError(f"protected job is missing {token!r}")

    static_job = section(workflow, "  static-contract:\n", "  protected-rotation:\n")
    if "secrets." in static_job or "environment:" in static_job:
        raise ContractError("pull request compile job must not receive environment secrets")
    for token in (
        "actions: read",
        "Record the separate dispatcher and environment approver",
        "verify_b054_custody_approval.py",
        "Build the test binary before providing protected values",
        "verify_b054_custody_redaction.py",
        'cat "$B054_RECEIPT_PATH"',
        "actions/upload-artifact",
        "retention-days: 90",
        "path: ${{ runner.temp }}/b054-custody-receipt.json",
    ):
        if token not in protected:
            raise ContractError(f"protected execution is missing {token!r}")

    secret_uses = re.findall(r"secrets\.([A-Z0-9_]+)", workflow)
    if sorted(secret_uses) != sorted(SECRETS):
        raise ContractError("workflow must scope each approved secret name exactly once")
    for name in SECRETS:
        if f"{name}: ${{{{ secrets.{name} }}}}" not in protected:
            raise ContractError(f"{name} must be supplied only to the protected drill step")

    approval_step = protected.find("Record the separate dispatcher and environment approver")
    secrets_step = protected.find("Run synthetic rotation and scan before publishing output")
    if min(approval_step, secrets_step) < 0 or approval_step >= secrets_step:
        raise ContractError("independent approval must be recorded before secret use")
    for token in (
        "actions/runs/{run_id}/approvals",
        'approval.get("state") != "approved"',
        'environment.get("name") == ENVIRONMENT',
        "if actor in reviewers:",
        '"distinct_people": True',
    ):
        if token not in approval:
            raise ContractError(f"approval verifier is missing {token!r}")

    for token in (
        "#[ignore =",
        "ChainEpoch::keyed_successor",
        "LinkKeyring::parse_json",
        "split_verifying_prefix_for_epoch",
        "missing_or_malformed_history",
        "rollback_to_retained_e1_checkpoint",
        "full_retention_horizon_elapsed",
        "Zeroizing",
        "custody_approval",
        "B054_APPROVAL_RECEIPT_PATH",
    ):
        if token not in test:
            raise ContractError(f"hosted drill is missing {token!r}")
    for forbidden in ("std::net::", "reqwest::", "wrangler", "AUDIT_CHAIN_SIGNING_SEED_HEX"):
        if forbidden in test:
            raise ContractError(f"hosted drill must stay synthetic and in-memory ({forbidden})")

    for token in (
        '["git", "ls-files", "-z"]',
        "bytes.fromhex(value)",
        "base64.b64encode(raw)",
        "base64.urlsafe_b64encode(raw)",
        "value.upper().encode(\"ascii\")",
        "approval receipt",
        "redacted receipt",
        "output withheld",
    ):
        if token not in scanner:
            raise ContractError(f"redaction scanner is missing {token!r}")

    normalized_doc = " ".join(doc.split())
    for token in (
        "self-review prevention",
        "dispatcher and at least one different approver",
        "approved cryptographically secure random generator",
        "Never overwrite E1",
        "does not provision or rotate production keys",
        "retention horizon has not elapsed",
    ):
        if token not in normalized_doc:
            raise ContractError(f"operator procedure is missing {token!r}")
    if "rollback_to_retained_e1_checkpoint" not in test:
        raise ContractError("hosted drill is missing rollback evidence")
    for name in SECRETS:
        if re.search(rf"{name}\s*=\s*[\"'][0-9a-fA-F]{{64}}[\"']", doc):
            raise ContractError("operator docs must not contain key values")


def read_files() -> dict[str, str]:
    paths = {
        "workflow": WORKFLOW,
        "test": TEST,
        "doc": DOC,
        "approval": APPROVAL,
        "scanner": SCANNER,
    }
    return {name: (ROOT / path).read_text(encoding="utf-8") for name, path in paths.items()}


def main() -> int:
    try:
        assess(read_files())
    except (OSError, ContractError) as error:
        print(f"B-054 custody contract: FAIL ({error})", file=sys.stderr)
        return 1
    print("B-054 custody contract: PASS (protected execution and two-person evidence path held)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Fail-closed contract for the B-314 GDPR transfer-table decision.

The current notice still has one combined ``PagerDuty / GitHub / Sigstore``
recipient row in each published locale, while the Trust Center says that
Sigstore is not a customer-data sub-processor.  This verifier intentionally
has *parked* polarity: it exits zero while that exact, four-locale inconsistency
and the pending Legal/DPO decision remain.  Once either the row population or
the pending decision changes, it exits non-zero and forces a backlog transition.

Only the GDPR transfer-table truth is in scope.  This does not delete or judge
``cosign-sign.yml`` and does not decide the B-005/B-112/B-118 keep-versus-retire
branch.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]

LOCALES = (
    "apps/docs/docs/explanation/privacy/gdpr.mdx",
    "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/explanation/privacy/gdpr.mdx",
    "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/explanation/privacy/gdpr.mdx",
    "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/explanation/privacy/gdpr.mdx",
)
TRUST = "apps/docs/docs/trust/subprocessors.mdx"
GENERATOR = "scripts/gen-public-subprocessors.py"
PACKET = "docs/handoff/2026-09-06-b314-gdpr-sigstore-transfer.json"
WORKFLOW = ".github/workflows/backlog-verify.yml"
TEST = "tests/test_verify_b314_gdpr_sigstore.py"

TABLE_HEADING = "| Recipient | Country | Mechanism | What's transferred |"
SIGSTORE_ROW = re.compile(
    r"^\|\s*PagerDuty\s*/\s*GitHub\s*/\s*Sigstore\s*\|\s*US\s*\|"
    r"\s*DPF\s*\+\s*SCC\s*\+\s*sub-processor-specific posture\s*\|"
    r"\s*Operational metadata;\s*no end-user PII\s*\|\s*$"
)
REQUIRED_PACKET_FIELDS = {
    "schema_version",
    "finding",
    "status",
    "owner",
    "non_claim",
    "scope",
    "baseline",
    "decision",
    "owner_action",
    "evidence",
    "retry_and_rollback",
    "references",
}
EXPECTED_LOCALES = list(LOCALES)
EXPECTED_WIRING = (
    "apps/docs/docs/explanation/privacy/gdpr.mdx",
    *LOCALES[1:],
    TRUST,
    GENERATOR,
    PACKET,
    "scripts/verify_b314_gdpr_sigstore.py",
    TEST,
)


class VerificationError(RuntimeError):
    """Raised when a target is absent, ambiguous, or no longer parked."""


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        value = overrides[path]
        if not isinstance(value, str):
            raise VerificationError(f"override for {path} is not text")
        return value
    target = root / path
    try:
        if target.is_symlink() or not target.is_file():
            raise VerificationError(f"missing/non-regular target: {path}")
        return target.read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"cannot read target: {path}") from exc


def _load_packet(text: str) -> dict[str, Any]:
    def no_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise VerificationError(f"packet contains duplicate JSON key: {key}")
            result[key] = value
        return result

    try:
        value = json.loads(text, object_pairs_hook=no_duplicates)
    except (json.JSONDecodeError, VerificationError) as exc:
        raise VerificationError(f"invalid B-314 packet JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise VerificationError("B-314 packet root must be an object")
    return value


def _require_text(mapping: dict[str, Any], key: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        raise VerificationError(f"B-314 packet field {key!r} is missing or empty")
    return value


def _check_packet(packet: dict[str, Any]) -> None:
    if set(packet) != REQUIRED_PACKET_FIELDS:
        missing = sorted(REQUIRED_PACKET_FIELDS - set(packet))
        extra = sorted(set(packet) - REQUIRED_PACKET_FIELDS)
        raise VerificationError(f"B-314 packet fields differ: missing={missing}, extra={extra}")
    if packet.get("schema_version") != 1:
        raise VerificationError("B-314 packet schema_version must be 1")
    for key in ("finding", "status", "owner", "non_claim", "retry_and_rollback"):
        _require_text(packet, key)
    if packet["finding"] != "B-314" or packet["owner"] != "tl":
        raise VerificationError("B-314 packet identity/owner drifted")
    if packet["status"] != "parked-owner-action-pending":
        raise VerificationError("B-314 packet no longer records the pending external decision")
    scope = packet["scope"]
    if not isinstance(scope, dict) or set(scope) != {"question", "population", "out_of_scope"}:
        raise VerificationError("B-314 scope must name question, exact population, and out-of-scope work")
    if scope["population"] != EXPECTED_LOCALES:
        raise VerificationError("B-314 scope population is not exactly the four published locales")
    if not isinstance(scope["question"], str) or "Sigstore" not in scope["question"]:
        raise VerificationError("B-314 scope question lost the Sigstore decision")
    if not isinstance(scope["out_of_scope"], list) or not all(isinstance(v, str) for v in scope["out_of_scope"]):
        raise VerificationError("B-314 out_of_scope must be a list of strings")
    out = " ".join(scope["out_of_scope"])
    for marker in ("B-005", "B-112", "B-118", "cosign-sign.yml", "retire", "keep"):
        if marker not in out:
            raise VerificationError(f"B-314 scope does not fence out {marker}")

    baseline = packet["baseline"]
    if not isinstance(baseline, dict) or set(baseline) != {"row_count_per_locale", "row_identity", "posture_sources"}:
        raise VerificationError("B-314 baseline schema drifted")
    if baseline["row_count_per_locale"] != 1 or baseline["row_identity"] != "PagerDuty / GitHub / Sigstore":
        raise VerificationError("B-314 baseline does not pin the measured row population")
    if baseline["posture_sources"] != [TRUST, GENERATOR]:
        raise VerificationError("B-314 baseline posture sources drifted")

    decision = packet["decision"]
    if not isinstance(decision, dict) or set(decision) != {"state", "allowed_outcomes", "required_reviewers"}:
        raise VerificationError("B-314 decision schema drifted")
    if decision["state"] != "pending" or decision["allowed_outcomes"] != ["remove_sigstore_row", "retain_and_document_transfer"]:
        raise VerificationError("B-314 decision is no longer explicitly pending or has unapproved outcomes")
    if decision["required_reviewers"] != ["Legal Counsel", "DPO"]:
        raise VerificationError("B-314 requires both Legal Counsel and DPO review")

    action = packet["owner_action"]
    if not isinstance(action, list) or len(action) != 4 or not all(isinstance(v, str) for v in action):
        raise VerificationError("B-314 owner_action must contain four executable steps")
    if not action[0].startswith("UI:") or not action[1].startswith("RUN:") or not action[2].startswith("UI:") or not action[3].startswith("RUN:"):
        raise VerificationError("B-314 owner_action UI/RUN boundaries drifted")
    for marker in ("Legal", "DPO", "no customer data", "four locales", "signed disposition"):
        if marker not in " ".join(action):
            raise VerificationError(f"B-314 owner action lost {marker}")

    evidence = packet["evidence"]
    if not isinstance(evidence, dict) or set(evidence) != {"path", "format", "required_fields", "completion_rule"}:
        raise VerificationError("B-314 evidence schema drifted")
    if evidence["path"] != "evidence/owner-actions/B-314/gdpr-sigstore-transfer-decision.json" or evidence["format"] != "json":
        raise VerificationError("B-314 evidence path/format drifted")
    required = evidence["required_fields"]
    if not isinstance(required, list) or required != ["schema_version", "captured_at", "decision", "legal_reviewer", "dpo_reviewer", "scope", "row_disposition", "effective_at"]:
        raise VerificationError("B-314 evidence required fields drifted")
    if not isinstance(evidence["completion_rule"], str) or "signed" not in evidence["completion_rule"] or "four" not in evidence["completion_rule"]:
        raise VerificationError("B-314 evidence completion rule is not review-bound")
    refs = packet["references"]
    if not isinstance(refs, list) or not all(isinstance(v, str) for v in refs):
        raise VerificationError("B-314 references must be a list of strings")
    if "BACKLOG.md#B-314" not in refs or TRUST not in refs or GENERATOR not in refs:
        raise VerificationError("B-314 references omit canonical backlog or posture source")


def _check_locale(path: str, text: str) -> None:
    lines = text.splitlines()
    heading_indexes = [i for i, line in enumerate(lines) if line.strip() == TABLE_HEADING]
    if len(heading_indexes) != 1:
        raise VerificationError(f"{path}: expected one transfer-table heading, found {len(heading_indexes)}")
    start = heading_indexes[0]
    rows = [line for line in lines[start + 1 :] if line.startswith("|") and SIGSTORE_ROW.fullmatch(line)]
    if len(rows) != 1:
        raise VerificationError(f"{path}: expected exactly one baseline Sigstore recipient row, found {len(rows)}")
    # A second Sigstore mention inside the table must not hide a duplicate row
    # behind a changed mechanism or a translated spelling.
    table_tail = lines[start + 1 :]
    table_end = next((i for i, line in enumerate(table_tail) if line.startswith("## ")), len(table_tail))
    sigstore_mentions = sum("Sigstore" in line for line in table_tail[:table_end])
    if sigstore_mentions != 1:
        raise VerificationError(f"{path}: transfer table has {sigstore_mentions} Sigstore mentions, expected one")


def _check_posture(trust: str, generator: str) -> None:
    for label, text in ((TRUST, trust), (GENERATOR, generator)):
        for marker in ("Sigstore (Linux Foundation)", "never receives customer data", "not a customer-data sub-processor"):
            if marker not in text:
                raise VerificationError(f"{label}: non-processor posture marker missing: {marker}")
    if "publishes no records" not in trust:
        raise VerificationError(f"{TRUST}: proposed/non-live no-records marker missing")
    if "own build artifacts" not in generator:
        raise VerificationError(f"{GENERATOR}: source scope no-customer-data marker missing")


def _check_runtime(workflow: str) -> None:
    # The path population must occur in both pull_request and push trigger
    # blocks.  A path in only one block creates a silent merge-time hole.
    for path in EXPECTED_WIRING:
        if workflow.count(path) < 2:
            raise VerificationError(f"{WORKFLOW}: runtime trigger population is missing {path} in one event")
    command = "python3 -S scripts/verify_b314_gdpr_sigstore.py --self-test"
    if workflow.count(command) != 1:
        raise VerificationError(f"{WORKFLOW}: expected one B-314 self-test step")
    if f"python3 -m pytest -q {TEST}" not in workflow:
        raise VerificationError(f"{WORKFLOW}: B-314 focused pytest is not wired")


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = {} if overrides is None else dict(overrides)
    known = set(LOCALES) | {TRUST, GENERATOR, PACKET, WORKFLOW, "BACKLOG.md"}
    unknown = set(overrides) - known
    if unknown:
        raise VerificationError(f"unknown override target(s): {sorted(unknown)}")
    for path in LOCALES:
        _check_locale(path, _read(root, path, overrides))
    _check_posture(_read(root, TRUST, overrides), _read(root, GENERATOR, overrides))
    _check_packet(_load_packet(_read(root, PACKET, overrides)))
    _check_runtime(_read(root, WORKFLOW, overrides))


def _must_reject(label: str, root: Path, overrides: dict[str, str]) -> None:
    try:
        verify(root, overrides=overrides)
    except VerificationError:
        return
    raise VerificationError(f"self-test mutation unexpectedly passed: {label}")


def mutation_checks(root: Path = ROOT) -> int:
    """Exercise row, decision, posture, and runtime restoration mutations."""
    originals = {path: _read(root, path, {}) for path in (*LOCALES, TRUST, GENERATOR, PACKET, WORKFLOW)}
    count = 0
    for path in LOCALES:
        _must_reject("row removal", root, {path: originals[path].replace(next(line for line in originals[path].splitlines() if SIGSTORE_ROW.fullmatch(line)), "", 1)})
        count += 1
        row = next(line for line in originals[path].splitlines() if SIGSTORE_ROW.fullmatch(line))
        _must_reject("row duplicate/restoration", root, {path: originals[path].replace(row, row + "\n" + row, 1)})
        count += 1
        _must_reject("row mechanism mutation", root, {path: originals[path].replace("DPF + SCC + sub-processor-specific posture", "SCC only", 1)})
        count += 1
    _must_reject("pending decision mutation", root, {PACKET: originals[PACKET].replace('"state": "pending"', '"state": "remove_sigstore_row"', 1)})
    count += 1
    _must_reject("trust posture removal", root, {TRUST: originals[TRUST].replace("never receives customer data", "receives customer data", 1)})
    count += 1
    _must_reject("generator posture restoration", root, {GENERATOR: originals[GENERATOR].replace("not a customer-data sub-processor", "a customer-data sub-processor", 1)})
    count += 1
    _must_reject("runtime trigger removal", root, {WORKFLOW: originals[WORKFLOW].replace('      - "' + PACKET + '"', "", 1)})
    count += 1
    return count


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run fail-closed mutation checks")
    args = parser.parse_args(argv)
    try:
        verify()
        mutations = mutation_checks() if args.self_test else 0
    except (OSError, VerificationError) as exc:
        print(f"B-314 parked gate FAIL: {exc}", file=sys.stderr)
        return 1
    suffix = f"; mutations={mutations}" if args.self_test else ""
    print(f"B-314 parked gate PASS: four locale rows + pending Legal/DPO packet{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

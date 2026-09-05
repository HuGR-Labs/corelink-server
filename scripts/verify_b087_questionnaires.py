#!/usr/bin/env python3
"""Fail-closed semantic guard for the bounded B-087 procurement population.

This is deliberately narrower than the B-156 published-claims census.  B-156
owns the repository-wide inventory of terms; this guard owns only the rows in
the two pre-filled procurement documents whose answers depend on the shipped
BYOK, WORM, supply-chain, SAST, fuzz, and synthetic-paging posture.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CAIQ = Path("marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md")
SIG = Path("marketing/sales/legal-questionnaires/SIG-LITE-2026-pre-filled.md")

# This is a bounded, named population.  Adding or removing a row requires a
# deliberate update here and in the mutation suite; a missing row is never a
# successful "clean" result.
CAIQ_ROWS: dict[str, tuple[str, tuple[str, ...], tuple[str, ...]]] = {
    "AIS-04.1": ("P", ("CodeQL", "workflow_dispatch", "cargo-fuzz", "not daily"), ("CodeQL + Semgrep (custom rules) on every PR", "cargo-fuzz daily")),
    "BCR-04.1": ("Y", ("provider-managed keys", "BYOK is NOT shipped"), ("BYOK if enabled", "optional BYOK")),
    "CEK-02.1": ("Y", ("provider-managed keys", "BYOK is NOT shipped"), ("Optional BYOK envelope.",)),
    "CEK-04.1": ("P", ("501 byok_not_available", "InMemoryFake", "No FIPS-validated-module claim is made"), ()),
    "CEK-05.1": ("P", ("Documented, not served", "501 byok_not_available"), ()),
    "CEK-06.1": ("P", ("Customer-key rotation: no", "501 byok_not_available"), ("DEK rotation on customer trigger",)),
    "CEK-07.1": ("P", ("CoreLink genuinely never holds CMK material", "501 byok_not_available"), ()),
    "CEK-09.1": ("N", ("No — BYOK is not shipped", "501 Not Implemented", "InMemoryFake"), ("BYOK available",)),
    "CEK-10.1": ("N", ("BYOK is NOT shipped", "byok_not_available", "shell simulation"), ("| Y |", "drilled weekly")),
    "CEK-11.1": ("N", ("BYOK is NOT shipped", "InMemoryFake"), ("| Y |", "HSM-backed key material")),
    "CEK-16.1": ("P", ("EVT-KMS-*` events are BYOK events", "501 byok_not_available"), ()),
    "CEK-17.1": ("P", ("Customer-controlled crypto-shredding is not available", "501 byok_not_available"), ()),
    "CEK-18.1": ("P", ("documents a **design**, not a served boundary", "501 Not Implemented"), ("apps/docs/docs/security/byok",)),
    "CEK-19.1": ("P", ("there is no CMK", "501 byok_not_available"), ()),
    "CCC-07.1": ("Y", ("signed commits", "DCO", "CODEOWNERS"), ()),
    "DSP-09.1": ("P", ("region pinning is a real supplementary measure", "BYOK is NOT shipped", "501 Not Implemented` / `byok_not_available"), ("supplementary measures (BYOK",)),
    "LOG-03.1": ("P", ("Tamper-EVIDENT, not immutable", "does not implement Object Lock", "NotImplemented"), ("immutable R2 with Object Lock",)),
    "SEF-03.1": ("P", ("does not fire in production", "not verifiable from this repository", "PIN_AT_RELEASE"), ()),
    "STA-08.1": ("P", ("No Cosign signatures", "no Rekor entries", "no SLSA attestation"), ("SLSA Level 3",)),
    "STA-11.1": ("N", ("no transparency-log entry", "SHA-256 checksums"), ("Rekor public transparency log entries",)),
    "TVM-02.1": ("P", ("cargo-audit", "cargo-deny", "no PR trigger", "workflow-dispatch-only"), ("CodeQL/Semgrep on every PR",)),
    "TVM-09.1": ("Y", ("Cloudflare WAF", "not represented here as a web-scanning cadence"), ("CodeQL + Semgrep custom rules + Cloudflare WAF",)),
}

SIG_ROWS: dict[str, tuple[str, tuple[str, ...], tuple[str, ...]]] = {
    "A.4": ("P", ("No external pentest has been contracted or completed", "only scoped as future"), ("Pentest scheduled pre-GA",)),
    "D.7": ("P", ("**not** signed", "no Cosign signatures", "no Rekor entries", "no SLSA attestation"), ()),
    "G.2": ("Y", ("branch protection", "paths-scoped clippy/test", "neither is claimed as an every-PR gate"), ("static-analysis gate (CodeQL + Semgrep custom rules)",)),
    "G.9": ("Y", ("daily `cargo-deny`", "not operating as a live daily scanner"), ("Dependency-Track + cargo-deny daily",)),
    "G.10": ("P", ("Partial cadence", "no PR trigger", "workflow-dispatch-only", "not a daily all-tool claim"), ("CodeQL + Semgrep custom rules on every PR",)),
    "I.5": ("P", ("Partial cadence", "no PR trigger", "workflow-dispatch-only"), ("CodeQL + Semgrep (custom rules) on every PR",)),
    "I.6": ("P", ("not daily", "workflow-dispatch-only", "Property tests do run"), ("cargo-fuzz daily",)),
    "I.7": ("Y", ("cargo-deny", "license allowlist", "daily scheduled lanes", "not operating as a live daily scanner"), ("Dependency-Track + cargo-deny + license allowlist (ADR-0024). DAILY cadence",)),
    "I.8": ("P", ("no Cosign signatures / Rekor entries today",), ("production deployments signed / attested? | Y",)),
    "I.9": ("N", ("No SLSA attestation today", "SHA-256"), ("| Y |",)),
    "J.4": ("P", ("does **not** fire in production", "lives outside this repository", "PIN_AT_RELEASE"), ()),
    "K.10": ("Y", ("**one** supplementary measure", "BYOK is not shipped", "501 byok_not_available"), ("supplementary measures (BYOK envelope encryption",)),
    "N.2": ("Y", ("BYOK is not shipped", "no CoreLink tenant reaches any of them"), ("customer-side BYOK KMS providers",)),
    "N.4": ("Y", ("provider-managed keys", "not shipped", "501 byok_not_available"), ("The \"optional BYOK envelope encryption per blob\"",)),
    "N.6": ("N", ("No — BYOK is not shipped", "501 Not Implemented", "InMemoryFake", "shell simulation"), ("| Y |", "across 4 providers … drilled weekly")),
}

SOURCE_CHECKS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("crates/corelink-container/src/routes/byok_admin.rs", ("REAL_KMS_PROVIDER_WIRED", "byok_not_available", "NOT_IMPLEMENTED")),
    ("crates/corelink-container/src/byok_orchestrator.rs", ("ActiveProvider::InMemoryFake", "Ok(Arc::new(InMemoryFake::new()))")),
    ("crates/corelink-container/Cargo.toml", ("default = []",)),
    (".github/workflows/codeql.yml", ("schedule:", "workflow_dispatch:",)),
    (".github/workflows/semgrep.yml", ("workflow_dispatch:", "schedule:",)),
    (".github/workflows/fuzz-nightly.yml", ("workflow_dispatch:", "schedule:",)),
    (".github/workflows/cargo-audit.yml", ("pull_request:", "schedule:",)),
    (".github/workflows/cargo-deny.yml", ("pull_request:", "schedule:",)),
    ("wrangler.toml", ("production env intentionally omits [triggers]", "<PIN_AT_RELEASE>")),
    (".github/CODEOWNERS", ("*",)),
)


class VerificationError(RuntimeError):
    """A missing or structurally changed verification object."""


@dataclass(frozen=True)
class Row:
    key: str
    answer: str
    text: str


def _rows(path: Path) -> dict[str, Row]:
    if not path.is_file():
        raise VerificationError(f"missing questionnaire: {path}")
    found: dict[str, Row] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 5:
            continue
        key = cells[0]
        if key not in set(CAIQ_ROWS) | set(SIG_ROWS):
            continue
        answer_index = 2
        if key in found:
            raise VerificationError(f"duplicate questionnaire row: {path}:{key}")
        found[key] = Row(key, cells[answer_index], " | ".join(cells))
    return found


def _check_rows(path: Path, specs: dict[str, tuple[str, tuple[str, ...], tuple[str, ...]]]) -> list[str]:
    rows = _rows(path)
    failures: list[str] = []
    for key, (answer, required, forbidden) in specs.items():
        row = rows.get(key)
        if row is None:
            failures.append(f"missing named row: {path}:{key}")
            continue
        if row.answer != answer:
            failures.append(f"unsupported answer at {path}:{key}: expected {answer}, got {row.answer}")
        for needle in required:
            if needle not in row.text:
                failures.append(f"missing required reality marker at {path}:{key}: {needle}")
        for needle in forbidden:
            if needle in row.text:
                failures.append(f"unsupported positive claim at {path}:{key}: {needle}")
    return failures


def verify(root: Path = ROOT) -> dict[str, object]:
    failures: list[str] = []
    for relative, needles in SOURCE_CHECKS:
        path = root / relative
        if not path.is_file():
            failures.append(f"missing source control: {relative}")
            continue
        text = path.read_text(encoding="utf-8")
        for needle in needles:
            if needle not in text:
                failures.append(f"source control changed or missing: {relative}: {needle}")
    failures.extend(_check_rows(root / CAIQ, CAIQ_ROWS))
    failures.extend(_check_rows(root / SIG, SIG_ROWS))
    # The legal instruments are intentionally observed, not mutated.  Their
    # live promises are owner/legal residue and must remain visible in output.
    owner_actions = [
        "owner/legal: review executed DPA Object Lock/WORM language; no amendment made by this guard",
        "owner/legal: review executed SLA and residency-amendment BYOK five-minute promises; no amendment made by this guard",
        "owner/ops: obtain PagerDuty 24/7 rotation export; it is not repository-verifiable",
        "owner/sales: determine whether recipients of superseded questionnaire copies require notice",
    ]
    return {
        "ok": not failures,
        "failures": failures,
        "owner_actions": owner_actions,
        "population": {"caiq": len(CAIQ_ROWS), "sig_lite": len(SIG_ROWS)},
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = verify(Path(args.root).resolve())
    except (OSError, UnicodeDecodeError, VerificationError) as exc:
        print(f"HALT: B-087 questionnaire population unavailable: {exc}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(result, indent=2))
    elif result["ok"]:
        print("B-087 questionnaire reality guard: PASS")
        for action in result["owner_actions"]:
            print(f"OWNER ACTION: {action}")
    else:
        print("B-087 questionnaire reality guard: FAIL", file=sys.stderr)
        print("\n".join(f"FAIL: {failure}" for failure in result["failures"]), file=sys.stderr)
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

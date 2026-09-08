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
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CAIQ = Path("marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md")
SIG = Path("marketing/sales/legal-questionnaires/SIG-LITE-2026-pre-filled.md")
OWNER_ACTIONS = Path("docs/internal/b087-questionnaire-owner-actions.md")
FALSE_BYOK_PROVIDER_CLAIM = "InMemoryFake"

# This is a bounded, named population.  Adding or removing a row requires a
# deliberate update here and in the mutation suite; a missing row is never a
# successful "clean" result.
CAIQ_ROWS: dict[str, tuple[str, tuple[str, ...], tuple[str, ...]]] = {
    "AIS-04.1": ("P", ("CodeQL", "workflow_dispatch", "cargo-fuzz", "not daily"), ("CodeQL + Semgrep (custom rules) on every PR", "cargo-fuzz daily")),
    "BCR-04.1": ("Y", ("provider-managed keys", "BYOK is NOT shipped"), ("BYOK if enabled", "optional BYOK")),
    "CEK-02.1": ("Y", ("provider-managed keys", "BYOK is NOT shipped"), ("Optional BYOK envelope.",)),
    "CEK-04.1": ("P", ("501 byok_not_available", "ActiveProvider::Unavailable", "No FIPS-validated-module claim is made"), ()),
    "CEK-05.1": ("P", ("Documented, not served", "501 byok_not_available"), ()),
    "CEK-06.1": ("P", ("Customer-key rotation: no", "501 byok_not_available"), ("DEK rotation on customer trigger",)),
    "CEK-07.1": ("P", ("CoreLink genuinely never holds CMK material", "501 byok_not_available"), ()),
    "CEK-09.1": ("N", ("No — BYOK is not shipped", "501 Not Implemented", "ActiveProvider::Unavailable"), ("BYOK available",)),
    "CEK-10.1": ("N", ("BYOK is NOT shipped", "byok_not_available", "shell simulation"), ("| Y |", "drilled weekly")),
    "CEK-11.1": ("N", ("BYOK is NOT shipped", "ActiveProvider::Unavailable"), ("| Y |", "HSM-backed key material")),
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
    "N.6": ("N", ("No — BYOK is not shipped", "501 Not Implemented", "ActiveProvider::Unavailable", "shell simulation"), ("| Y |", "across 4 providers … drilled weekly")),
}

SOURCE_CHECKS: tuple[tuple[str, tuple[str, ...]], ...] = (
    # These are code-level witnesses, not historical prose markers.  The
    # production binary now has no fake provider at all: the no-feature branch
    # returns an error and the admin route maps it to 501 before D1.
    (
        "crates/corelink-container/src/routes/byok_admin.rs",
        (
            "crate::byok_orchestrator::make_provider().await",
            "StatusCode::NOT_IMPLEMENTED",
        ),
    ),
    (
        "crates/corelink-container/src/byok_orchestrator.rs",
        (
            "ActiveProvider::Unavailable",
            "Err(BYOKError::Provider(",
            "#[cfg(not(any(",
        ),
    ),
    ("crates/corelink-container/Cargo.toml", ("default = []",)),
    (".github/workflows/codeql.yml", ("schedule:", "workflow_dispatch:",)),
    # Both lanes are intentionally parked: their schedules are comments, so
    # only the explicit manual trigger is a live witness.
    (".github/workflows/semgrep.yml", ("workflow_dispatch:",)),
    (".github/workflows/fuzz-nightly.yml", ("workflow_dispatch:",)),
    (".github/workflows/cargo-audit.yml", ("pull_request:", "schedule:",)),
    (".github/workflows/cargo-deny.yml", ("pull_request:", "schedule:",)),
    # `wrangler.toml` is checked structurally below.  Keep the path in this
    # closed-world fixture list, but do not accept a comment that merely says
    # production has no cron.
    ("wrangler.toml", ()),
    (".github/CODEOWNERS", ("*",)),
)


class VerificationError(RuntimeError):
    """A missing or structurally changed verification object."""


@dataclass(frozen=True)
class Row:
    key: str
    answer: str
    text: str


def _mask_comments(
    text: str, *, slash_comments: bool = False, hash_comments: bool = False
) -> str:
    """Remove comments while preserving strings and line numbers.

    Source witnesses must not be satisfiable by stale prose in a Rust `//!`
    block or a TOML `#` comment.  Strings remain because the admin response
    marker is itself a deliberate wire contract; the caller checks that marker
    together with the surrounding executable status branch.
    """

    out: list[str] = []
    i = 0
    block_depth = 0
    quote: str | None = None
    escaped = False
    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""
        if block_depth:
            if ch == "/" and nxt == "*":
                block_depth += 1
                out.extend((" ", " "))
                i += 2
            elif ch == "*" and nxt == "/":
                block_depth -= 1
                out.extend((" ", " "))
                i += 2
            else:
                out.append("\n" if ch == "\n" else " ")
                i += 1
            continue
        if quote:
            out.append(ch)
            # TOML/YAML single-quoted strings escape a quote by doubling it.
            if quote == "'" and ch == "'" and nxt == "'":
                out.append(nxt)
                i += 2
                continue
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == quote:
                quote = None
            i += 1
            continue
        if ch == '"' or (ch == "'" and (hash_comments or not slash_comments)):
            quote = ch
            out.append(ch)
            i += 1
        elif slash_comments and ch == "/" and nxt == "/":
            out.extend((" ", " "))
            i += 2
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
        elif slash_comments and ch == "/" and nxt == "*":
            block_depth = 1
            out.extend((" ", " "))
            i += 2
        elif hash_comments and ch == "#":
            out.append(" ")
            i += 1
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
        else:
            out.append(ch)
            i += 1
    return "".join(out)


def _mask_literals(text: str, *, single_quotes: bool = False) -> str:
    """Blank literals so code witnesses cannot be string decoys."""

    out: list[str] = []
    i = 0
    quote: str | None = None
    escaped = False
    while i < len(text):
        ch = text[i]
        if quote:
            out.append("\n" if ch == "\n" else " ")
            if quote == "'" and ch == "'" and i + 1 < len(text) and text[i + 1] == "'":
                out.append(" ")
                i += 2
                continue
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                quote = None
            elif ch == "'" and quote == "'":
                quote = None
            i += 1
            continue
        if ch == '"' or (single_quotes and ch == "'"):
            quote = ch
            out.append(" ")
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def _source_failures(root: Path, relative: str, needles: tuple[str, ...]) -> list[str]:
    path = root / relative
    if not path.is_file():
        return [f"missing source control: {relative}"]
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        return [f"source control unreadable: {relative}: {exc}"]

    if relative == "wrangler.toml":
        try:
            config = tomllib.loads(text)
        except tomllib.TOMLDecodeError as exc:
            return [f"source control changed or malformed: {relative}: {exc}"]
        prod = config.get("env", {}).get("prod")
        triggers = prod.get("triggers") if isinstance(prod, dict) else None
        if not isinstance(triggers, dict) or triggers.get("crons") != []:
            return [
                "source control changed or missing: wrangler.toml: "
                "env.prod.triggers.crons must be an explicit empty list"
            ]
        return []

    code = _mask_comments(
        text,
        slash_comments=relative.endswith(".rs"),
        hash_comments=relative.endswith((".toml", ".yml", ".yaml")),
    )
    code_tokens = _mask_literals(
        code, single_quotes=relative.endswith((".toml", ".yml", ".yaml"))
    )
    failures = [
        f"source control changed or missing: {relative}: {needle}"
        for needle in needles
        if needle not in code_tokens
    ]
    if relative in {
        ".github/workflows/semgrep.yml",
        ".github/workflows/fuzz-nightly.yml",
    }:
        # The parked lanes must remain dispatch-only.  A commented schedule is
        # historical context; an active schedule silently changes the answer
        # in the procurement population and must turn this guard red.
        if re.search(r"(?m)^\s*schedule\s*:", code):
            failures.append(
                f"source control changed or missing: {relative}: "
                "parked lane has an active schedule"
            )
        if re.search(r"(?m)^\s*pull_request\s*:", code):
            failures.append(
                f"source control changed or missing: {relative}: "
                "parked lane has an active pull_request trigger"
            )
    if relative == "crates/corelink-container/src/routes/byok_admin.rs":
        # Couple the response literal to the executable provider-error arm.
        # The prefix is matched after masking literals, while the response is
        # then required in the same short source window.  A copy of the whole
        # witness in a comment or string cannot satisfy this.
        prefix = re.compile(
            r"crate::byok_orchestrator::make_provider\(\)\.await"
            r"(?s:.*?)StatusCode::NOT_IMPLEMENTED\s*,\s*"
        )
        response = '"{\\"error\\":\\"byok_not_available\\"}"'
        match = prefix.search(code_tokens)
        if not match or response not in code[match.start() : match.end() + 256]:
            failures.append(
                "source control changed or missing: "
                "crates/corelink-container/src/routes/byok_admin.rs: "
                "provider error maps to executable 501 byok_not_available"
            )
    return failures


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


def _check_no_false_byok_provider_claims(root: Path) -> list[str]:
    """Keep procurement documents aligned with the shipped no-provider build.

    ``InMemoryFake`` is a test-only implementation elsewhere in the tree.  It
    must not appear in the bounded customer-facing documents or owner packet,
    where it would falsely describe the default runtime.  The positive marker
    checks above prove the affected rows carry the executable ``Unavailable``
    and 501 reality; this closed-world scan prevents a stale claim in any
    unlisted row (or owner handoff) from silently returning the guard to green.
    """

    failures: list[str] = []
    for relative in (CAIQ, SIG, OWNER_ACTIONS):
        path = root / relative
        if not path.is_file():
            failures.append(f"missing questionnaire reality packet: {relative}")
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            failures.append(f"questionnaire reality packet unreadable: {relative}: {exc}")
            continue
        if FALSE_BYOK_PROVIDER_CLAIM in text:
            failures.append(
                f"false default BYOK provider claim in {relative}: "
                f"{FALSE_BYOK_PROVIDER_CLAIM}"
            )
        if relative == OWNER_ACTIONS:
            for marker in (
                "ActiveProvider::Unavailable",
                "no real provider is compiled in",
                "501 byok_not_available",
            ):
                if marker not in text:
                    failures.append(f"owner packet missing BYOK reality marker: {marker}")
    return failures


def verify(root: Path = ROOT) -> dict[str, object]:
    failures: list[str] = []
    for relative, needles in SOURCE_CHECKS:
        failures.extend(_source_failures(root, relative, needles))
    failures.extend(_check_rows(root / CAIQ, CAIQ_ROWS))
    failures.extend(_check_rows(root / SIG, SIG_ROWS))
    failures.extend(_check_no_false_byok_provider_claims(root))
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

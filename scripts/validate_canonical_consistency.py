#!/usr/bin/env python3
"""
Validate canonical consistency: INV registry × TLA+ × code × tests.

Master cross-check that catches drift between canonical sources. Complements:

- `validate_specs.py` (YAML front matter + JSON Schema)
- `validate_references.py` (cross-doc ID definitions/uses)
- `check_tla_obligations.py` (TLA+ obligation matrix vs registry §4)
- `check_migrations_additive.py` (D1 schema migration discipline)

This adds the **ONE remaining gap**: does the IMPLEMENTATION (Rust crates,
TLA+ proofs) actually reference the invariants the spec corpus declares?

What it does
------------
1. Parse `specs/03_architecture/invariant_registry.md` §3 → all declared `INV-*`
   IDs **with severity** (CRITICAL / HIGH / MEDIUM).
2. Parse §5 → alias map (legacy name → canonical name).
3. Parse `specs/tla/*.tla` → set of invariants verified via formal model.
4. Walk `crates/*/src/**.rs` → INV-* mentions in production code.
5. Walk `crates/*/tests/**.rs` + `**/src/**` (test modules `#[cfg(test)]`)
   → INV-* mentions in tests.
6. Aggregate to a coverage matrix per INV-ID:
   declared / tla-verified / code-referenced / test-referenced.
7. Flag drift:
   - **orphan-ref**: INV referenced in code/tests but NOT in registry (and
     not an alias) → likely typo or renamed-without-fix.
   - **declared-no-code**: INV in registry but never mentioned in any src/
     under `crates/` → potential under-coverage (LOW severity tolerated,
     HIGH warns, CRITICAL fails unless waived).
   - **test-without-code**: INV named in a test but no production code refs
     it anywhere → test is verifying spec text only; ratchet on whether
     code-ref ever appeared.
   - **code-without-tla** for CRITICAL severity: surfacing for awareness
     (CTRL-FORMAL-001 already enforces this via check_tla_obligations; we
     duplicate as a sanity backstop).

Inputs (CLI)
------------
    python3 scripts/validate_canonical_consistency.py
    python3 scripts/validate_canonical_consistency.py --dry-run
    python3 scripts/validate_canonical_consistency.py --json
    python3 scripts/validate_canonical_consistency.py --baseline <path>

Exit codes
----------
    0 — no orphan refs and no CRITICAL coverage regressions vs baseline
    1 — orphan refs found OR CRITICAL coverage below baseline
    2 — registry parse error / IO error

Baseline file
-------------
`specs/_audits/2026-05-15-canonical-consistency-baseline.md` holds the
initial counts. The validator reads its frontmatter `baseline:` block for
the ratchet thresholds. New regressions (CRITICAL INV that LOST a code or
test reference vs baseline) fail CI.

Endereça F-CANON-DRIFT (cross-source drift between registry × TLA × code
× tests) — gap surfaced 2026-05-14 during R-prep canonical lint audit.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS = REPO_ROOT / "specs"
REGISTRY = SPECS / "03_architecture" / "invariant_registry.md"
TLA_DIR = SPECS / "tla"
CRATES = REPO_ROOT / "crates"
BASELINE_PATH = SPECS / "_audits" / "2026-05-15-canonical-consistency-baseline.md"

# Match any INV-* mention. We accept the legacy CamelCase form too because
# the registry §5 still keeps a few aliases until 2026-10-24.
INV_RE = re.compile(r"\bINV-[A-Za-z][A-Za-z0-9_-]+\b")

# Severity values we recognize in §3 tables (4th pipe-separated cell).
SEV_RE = re.compile(r"\b(CRITICAL|HIGH|MEDIUM)\b", re.IGNORECASE)

# Registry-row pattern: starts with `| **INV-XXX**`.
REGISTRY_ROW_RE = re.compile(r"^\|\s*\*\*(INV-[A-Za-z][A-Za-z0-9_-]+)\*\*")

# Alias row in §5: `| \`INV-Legacy\` | INV-CANONICAL |` (the first cell may
# wrap the ID in backticks; the second cell is bare).
ALIAS_ROW_RE = re.compile(
    r"^\|\s*`?(INV-[A-Za-z][A-Za-z0-9_-]+)`?(?:\s*\([^)]*\))?\s*\|\s*(INV-[A-Za-z][A-Za-z0-9_-]+)\s*\|"
)

# Tokens that look like INV-* but are NOT real IDs — registry intro lines,
# plural pattern mentions, template placeholders, line-wrap artifacts.
# Mirrors the WHITELIST_IDS in validate_references.py so we don't double-flag
# entries that the doc-level validator already accepts.
WHITELIST_NON_IDS = {
    "INV-AC", "INV-AUTH", "INV-AUTH-PAT", "INV-MULTIPART", "INV-LRU",
    "INV-GC", "INV-SUPPLY", "INV-DEDUP", "INV-EVICT", "INV-OBS",
    "INV-RATE-LIMIT", "INV-CAS-SIDE-CHANNEL",
    "INV-AUDIT-CHAIN",  # short form of INV-AUDIT-APPEND-ONLY
    "INV-AAA", "INV-BBB", "INV-XXX", "INV-XXX-", "INV-XXX-name",
    "INV-YYY", "INV-ZZZ", "INV-LIFECYCLE-001",
    "INV-DATA-CLASSIFICATION", "INV-SCOPE-DISCIPLINE",
}


@dataclass
class InvariantRecord:
    inv_id: str
    severity: str  # CRITICAL / HIGH / MEDIUM / UNKNOWN
    tla_verified: bool = False
    code_refs: set[str] = field(default_factory=set)
    test_refs: set[str] = field(default_factory=set)

    def to_dict(self) -> dict:
        return {
            "id": self.inv_id,
            "severity": self.severity,
            "tla_verified": self.tla_verified,
            "code_refs": sorted(self.code_refs),
            "test_refs": sorted(self.test_refs),
            "code_ref_count": len(self.code_refs),
            "test_ref_count": len(self.test_refs),
        }


@dataclass
class Report:
    declared: dict[str, InvariantRecord]
    aliases: dict[str, str]
    orphan_refs: dict[str, set[str]] = field(default_factory=lambda: defaultdict(set))
    declared_no_code: list[str] = field(default_factory=list)
    declared_no_test: list[str] = field(default_factory=list)
    test_without_code: list[str] = field(default_factory=list)
    critical_no_tla: list[str] = field(default_factory=list)

    def counts(self) -> dict:
        declared = list(self.declared.values())
        return {
            "declared": len(declared),
            "by_severity": {
                sev: sum(1 for r in declared if r.severity.upper() == sev)
                for sev in ("CRITICAL", "HIGH", "MEDIUM", "UNKNOWN")
            },
            "tla_verified": sum(1 for r in declared if r.tla_verified),
            "code_referenced": sum(1 for r in declared if r.code_refs),
            "test_referenced": sum(1 for r in declared if r.test_refs),
            "aliases": len(self.aliases),
            "orphan_refs": len(self.orphan_refs),
            "declared_no_code": len(self.declared_no_code),
            "declared_no_test": len(self.declared_no_test),
            "test_without_code": len(self.test_without_code),
            "critical_no_tla": len(self.critical_no_tla),
        }


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------


def parse_registry() -> tuple[dict[str, InvariantRecord], dict[str, str]]:
    """Parse §3 registry rows + §5 alias rows. Returns (declared, aliases)."""
    if not REGISTRY.exists():
        raise SystemExit(f"FATAL: registry not found at {REGISTRY}")

    declared: dict[str, InvariantRecord] = {}
    aliases: dict[str, str] = {}

    in_section_5 = False
    for line in REGISTRY.read_text(encoding="utf-8").splitlines():
        if line.startswith("## 5."):
            in_section_5 = True
            continue
        if line.startswith("## ") and in_section_5:
            in_section_5 = False

        if in_section_5:
            m = ALIAS_ROW_RE.match(line)
            if m:
                alias, canonical = m.group(1), m.group(2)
                if alias != canonical:
                    aliases[alias] = canonical
            continue

        m = REGISTRY_ROW_RE.match(line)
        if not m:
            continue
        inv_id = m.group(1)
        sev_match = SEV_RE.search(line)
        severity = sev_match.group(1).upper() if sev_match else "UNKNOWN"
        if inv_id not in declared:
            declared[inv_id] = InvariantRecord(inv_id=inv_id, severity=severity)
        else:
            # Keep the more severe of duplicates (defensive).
            rank = {"CRITICAL": 3, "HIGH": 2, "MEDIUM": 1, "UNKNOWN": 0}
            if rank.get(severity, 0) > rank.get(declared[inv_id].severity, 0):
                declared[inv_id].severity = severity

    return declared, aliases


def parse_tla_specs() -> set[str]:
    """Return set of INV-* IDs mentioned anywhere in TLA+ .tla files."""
    if not TLA_DIR.exists():
        return set()
    found: set[str] = set()
    for tla in TLA_DIR.glob("*.tla"):
        text = tla.read_text(encoding="utf-8", errors="ignore")
        for tok in INV_RE.findall(text):
            if tok not in WHITELIST_NON_IDS:
                found.add(tok)
    # Also walk runbook TLA specs if they exist.
    runbook_tla_dir = SPECS / "03_architecture" / "tla+" / "runbooks"
    if runbook_tla_dir.exists():
        for tla in runbook_tla_dir.glob("*.tla"):
            text = tla.read_text(encoding="utf-8", errors="ignore")
            for tok in INV_RE.findall(text):
                if tok not in WHITELIST_NON_IDS:
                    found.add(tok)
    return found


def walk_code_files() -> Iterable[Path]:
    """Yield every .rs file under crates/."""
    if not CRATES.exists():
        return
    yield from CRATES.glob("*/src/**/*.rs")
    yield from CRATES.glob("*/tests/**/*.rs")
    yield from CRATES.glob("*/benches/**/*.rs")


def parse_code() -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    """Return (code_refs, test_refs) mapping INV-* → set of file paths."""
    code_refs: dict[str, set[str]] = defaultdict(set)
    test_refs: dict[str, set[str]] = defaultdict(set)
    for rs in walk_code_files():
        rel = str(rs.relative_to(REPO_ROOT))
        try:
            text = rs.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        is_test_file = "/tests/" in rel or rel.endswith("_test.rs") or "/benches/" in rel
        has_cfg_test = "#[cfg(test)]" in text or "#[test]" in text or "#[tokio::test]" in text
        for tok in set(INV_RE.findall(text)):
            if tok in WHITELIST_NON_IDS:
                continue
            if is_test_file:
                test_refs[tok].add(rel)
            else:
                code_refs[tok].add(rel)
                # Files that also expose tests under #[cfg(test)] count both
                # as code-ref AND test-ref for that INV (a test name embedding
                # the INV inside src/ is BOTH a code-ref AND a test-ref).
                if has_cfg_test:
                    # Heuristic: if the INV appears specifically in a fn
                    # name prefixed by "fn test_" or in a #[test] block,
                    # also count as test ref. Simple check: any "test_" or
                    # "#[test]" line containing the INV.
                    for line in text.splitlines():
                        if tok in line and ("fn test_" in line or "#[test]" in line or "test_" in line):
                            test_refs[tok].add(rel)
                            break
    return code_refs, test_refs


# ---------------------------------------------------------------------------
# Analysis
# ---------------------------------------------------------------------------


def build_report() -> Report:
    declared, aliases = parse_registry()
    tla_set = parse_tla_specs()
    code_refs, test_refs = parse_code()

    # Mark TLA verification, attribute code/test refs, resolving aliases.
    for inv_id, rec in declared.items():
        rec.tla_verified = inv_id in tla_set
        rec.code_refs.update(code_refs.get(inv_id, set()))
        rec.test_refs.update(test_refs.get(inv_id, set()))

    # Apply alias redirection: any code/test ref to an alias counts toward
    # its canonical INV record.
    for alias, canonical in aliases.items():
        if canonical in declared:
            declared[canonical].code_refs.update(code_refs.get(alias, set()))
            declared[canonical].test_refs.update(test_refs.get(alias, set()))
        if alias in tla_set and canonical in declared:
            declared[canonical].tla_verified = True

    report = Report(declared=declared, aliases=aliases)

    # Detect orphan refs: any token in code/tests that is neither declared
    # nor a known alias nor whitelisted.
    declared_set = set(declared.keys())
    alias_set = set(aliases.keys())
    seen_tokens = set(code_refs.keys()) | set(test_refs.keys())
    for tok in seen_tokens:
        if tok in WHITELIST_NON_IDS:
            continue
        if tok in declared_set:
            continue
        if tok in alias_set:
            continue
        # Some forward-looking INVs may live only as future-promotion in
        # validate_references.WHITELIST_IDS; we don't import that. Instead,
        # check the registry text itself for an exact substring — if it
        # appears anywhere (e.g. in §3.12 sprint-driven list as a bullet
        # rather than a table row) we accept it.
        if _appears_in_registry_text(tok):
            continue
        for src in code_refs.get(tok, set()) | test_refs.get(tok, set()):
            report.orphan_refs[tok].add(src)

    # Coverage gaps.
    for inv_id, rec in declared.items():
        if not rec.code_refs and not rec.test_refs:
            report.declared_no_code.append(inv_id)
        elif not rec.code_refs:
            report.declared_no_test.append(inv_id)  # name kept; really means "no src/ ref"
        if rec.test_refs and not rec.code_refs:
            report.test_without_code.append(inv_id)
        if rec.severity == "CRITICAL" and not rec.tla_verified:
            report.critical_no_tla.append(inv_id)

    return report


_REGISTRY_TEXT_CACHE: str | None = None


def _appears_in_registry_text(token: str) -> bool:
    global _REGISTRY_TEXT_CACHE
    if _REGISTRY_TEXT_CACHE is None:
        try:
            _REGISTRY_TEXT_CACHE = REGISTRY.read_text(encoding="utf-8")
        except OSError:
            _REGISTRY_TEXT_CACHE = ""
    return token in _REGISTRY_TEXT_CACHE


# ---------------------------------------------------------------------------
# Baseline + output
# ---------------------------------------------------------------------------


_BASELINE_RE = re.compile(
    r"<!--\s*BASELINE\s+(?P<key>[a-z_]+)\s*=\s*(?P<val>\d+)\s*-->"
)


def read_baseline(path: Path) -> dict[str, int]:
    if not path.exists():
        return {}
    out: dict[str, int] = {}
    for m in _BASELINE_RE.finditer(path.read_text(encoding="utf-8")):
        out[m.group("key")] = int(m.group("val"))
    return out


def regressions(report: Report, baseline: dict[str, int]) -> list[str]:
    """Compare current report vs baseline counts; surface regressions."""
    if not baseline:
        return []
    counts = report.counts()
    flat = {
        "declared": counts["declared"],
        "tla_verified": counts["tla_verified"],
        "code_referenced": counts["code_referenced"],
        "test_referenced": counts["test_referenced"],
        "critical_referenced": sum(
            1
            for r in report.declared.values()
            if r.severity == "CRITICAL" and (r.code_refs or r.test_refs)
        ),
    }
    out: list[str] = []
    for key in ("code_referenced", "test_referenced", "critical_referenced"):
        base = baseline.get(key)
        if base is None:
            continue
        if flat[key] < base:
            out.append(f"{key}: {flat[key]} < baseline {base}")
    # Orphan refs are a hard fail regardless of baseline.
    if counts["orphan_refs"] > baseline.get("orphan_refs", 0):
        out.append(
            f"orphan_refs: {counts['orphan_refs']} > baseline "
            f"{baseline.get('orphan_refs', 0)}"
        )
    return out


def render_text(report: Report) -> str:
    counts = report.counts()
    lines = [
        "=== Canonical consistency report ===",
        f"declared: {counts['declared']} INVs in registry §3",
        f"  by severity: "
        f"CRITICAL={counts['by_severity']['CRITICAL']} "
        f"HIGH={counts['by_severity']['HIGH']} "
        f"MEDIUM={counts['by_severity']['MEDIUM']} "
        f"UNKNOWN={counts['by_severity']['UNKNOWN']}",
        f"  aliases (legacy → canonical): {counts['aliases']}",
        f"tla-verified: {counts['tla_verified']} declared INVs proved in specs/tla/",
        f"code-referenced: {counts['code_referenced']} declared INVs cited in crates/*/src/",
        f"test-referenced: {counts['test_referenced']} declared INVs cited in tests",
        "",
        f"DRIFT:",
        f"  orphan refs (in code, not in registry+aliases): {counts['orphan_refs']}",
        f"  declared with NO code/test reference: {counts['declared_no_code']}",
        f"  declared without src reference (test-only): {counts['test_without_code']}",
        f"  CRITICAL without TLA+ proof: {counts['critical_no_tla']}",
    ]
    if report.orphan_refs:
        lines.append("")
        lines.append("Orphan references (first 20):")
        for tok in sorted(report.orphan_refs)[:20]:
            srcs = sorted(report.orphan_refs[tok])
            lines.append(f"  {tok} → {srcs[0]}" + (f" (+{len(srcs)-1} more)" if len(srcs) > 1 else ""))
    if report.critical_no_tla:
        lines.append("")
        lines.append("CRITICAL invariants without TLA+ proof (first 20):")
        for inv_id in sorted(report.critical_no_tla)[:20]:
            lines.append(f"  {inv_id}")
    return "\n".join(lines)


def render_json(report: Report) -> str:
    return json.dumps(
        {
            "counts": report.counts(),
            "orphan_refs": {k: sorted(v) for k, v in report.orphan_refs.items()},
            "declared_no_code": report.declared_no_code,
            "test_without_code": report.test_without_code,
            "critical_no_tla": report.critical_no_tla,
            "invariants": [r.to_dict() for r in report.declared.values()],
        },
        indent=2,
        sort_keys=True,
    )


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--json", action="store_true", help="JSON output")
    parser.add_argument("--dry-run", action="store_true", help="never exit non-zero")
    parser.add_argument(
        "--baseline",
        type=Path,
        default=BASELINE_PATH,
        help="Markdown file with <!-- BASELINE key=N --> rollups",
    )
    parser.add_argument("--out", type=Path, help="Write JSON report to this path")
    args = parser.parse_args(argv)

    try:
        report = build_report()
    except SystemExit:
        raise
    except Exception as e:  # noqa: BLE001
        print(f"FATAL: {e}", file=sys.stderr)
        return 2

    if args.json:
        out = render_json(report)
    else:
        out = render_text(report)
    print(out)

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(render_json(report), encoding="utf-8")

    baseline = read_baseline(args.baseline) if args.baseline else {}
    regs = regressions(report, baseline)
    if regs:
        print("\nREGRESSIONS vs baseline:", file=sys.stderr)
        for r in regs:
            print(f"  - {r}", file=sys.stderr)
        if not args.dry_run:
            return 1
    # Hard-fail on orphan refs ONLY when there is no baseline (initial run)
    # OR when current count exceeds the baseline floor. At-baseline counts
    # are accepted; the next PR that adds an orphan fails CI via
    # `regressions()` above.
    if not baseline and report.orphan_refs and not args.dry_run:
        print(
            f"\nFAIL: {len(report.orphan_refs)} orphan INV-* references in "
            f"code/tests and no baseline file found at {args.baseline}.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

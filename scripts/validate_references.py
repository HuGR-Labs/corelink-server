#!/usr/bin/env python3
"""
Validate cross-references nos specs.

Para cada tipo de ID (EVT, CTRL, PAT, FM, INV, FF-HR, SLO, RB, ADR), extrai:

- **Definitions**: onde o ID é canonicamente DEFINIDO (cabeçalho ou tabela canônica do canonical source).
- **Uses**: onde o ID é referenciado em qualquer doc.

Reporta:

- **Dangling**: USE sem DEFINITION correspondente (referencia ID que não existe).
- **Orphan**: DEFINITION sem USE (catalogado mas nunca referenciado — pode ser dead code).

Uso:
    python3 scripts/validate_references.py                    # report normal
    python3 scripts/validate_references.py --warn-orphans     # também emite warnings de orphans
    python3 scripts/validate_references.py --json             # output em JSON

Exit code:
    0 se nenhum dangling.
    1 se há dangling references.

Endereça findings S-05/F-04 (PAT dangling), S-10 (CTRL dangling), S-16 (PAT-AUTHZ-002),
S-15 (FF-HR-011), F-04 (CTRL/PAT) do audit Lote 3+4.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = REPO_ROOT / "specs"

# ID patterns: (pattern, is_definition_anchor)
# Definitions são detectadas dentro de canonical sources nos paths abaixo.
ID_PATTERNS = {
    "EVT": re.compile(r"\bEVT-\d{3}\b"),
    "CTRL": re.compile(r"\bCTRL-[A-Z]+(?:-[A-Z]+)*-\d{3}\b"),  # v3 Lote 7.2: aceita multi-segmento (CTRL-PRIV-CONSENT-001, H-01)
    "PAT": re.compile(r"\bPAT-[A-Z][A-Z0-9-]+-\d{3}\b"),
    "FM": re.compile(r"\bFM-\d{3}\b"),
    "INV": re.compile(r"\bINV-[A-Za-z][A-Za-z0-9_-]+\b"),  # v2 Lote 6.3: aceita CamelCase legados (G-04)
    "FF-HR": re.compile(r"\bFF-HR-\d{3}\b"),
    "SLO": re.compile(r"\bSLO-[A-Z][A-Z0-9-]+\b"),
    "RB": re.compile(r"\bRB-[A-Z][A-Z0-9-]+\b"),
    "ADR": re.compile(r"\bADR-\d{4}\b"),
    "WAIVER": re.compile(r"\bWAIVER-\d{8}-\d{3}\b"),
    "FF-LR": re.compile(r"\bFF-LR-\d{3}\b"),
}

# Canonical source files for each ID type (where definitions live).
DEFINITION_SOURCES = {
    "EVT": ["00_framework.md"],
    "CTRL": [
        "03_architecture/security_model.md",
        "03_architecture/privacy_model.md",
        "03_architecture/key_management.md",
    ],
    "PAT": ["03_architecture/resilience_patterns.md"],
    "FM": ["03_architecture/failure_modes.md"],
    "INV": ["03_architecture/invariant_registry.md"],
    "FF-HR": ["00_framework.md"],
    "SLO": ["03_architecture/slo_catalog.md"],
    "RB": ["05_quality/runbooks/"],  # directory: any .md inside
    "ADR": ["03_architecture/adrs/"],
    "WAIVER": ["_waivers/"],
    "FF-LR": ["00_framework.md"],
}

# Skip these dirs entirely (audits, archives, csv, etc).
# _templates also skipped because they contain example placeholders, not real references.
SKIP_DIRS = {"_audits", "_archive", "_schemas", "_templates"}

# Detect definitions via "anchored" patterns in canonical sources.
# A definition is recognized when an ID appears at start of:
# - markdown header (### / #### / ##### with the ID)
# - table cell that starts with `**ID**` or `| ID |`
# - bullet `- **ID**` or `- ID`
# - bold `**SLO-XXX**:` or `**FM-001**:` (often used for SLO/PRR/runbook headings)
DEFINITION_ANCHORS = [
    re.compile(r"^#{1,5}\s+(?:[A-Z]+-?\d*[:\s]+)?\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?"),  # ### EVT-001 or # RB-FM-051 — title
    re.compile(r"^\|\s*\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?\s*[\(\|]"),  # | EVT-001 | or | **EVT-001** ( ...
    re.compile(r"^-\s+\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?"),  # - **EVT-001** or - EVT-001
    re.compile(r"^\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?\s*:"),  # **SLO-AVAIL-CP**:
    re.compile(r'^id:\s*"([A-Z]+(?:-[A-Z0-9_]+)+)"'),  # YAML front matter id field
    re.compile(r"^\*\*([A-Z]+(?:-[A-Z0-9_]+)+)\*\*\s*$"),  # **SLO-AVAIL-CP** standalone (no colon)
]

# IDs whitelisted (canonical IDs of canonical-source docs themselves; not "uses").
WHITELIST_IDS = {
    "FRAMEWORK-00",
    "REMOTE-CACHE-PRODUCT-PROFILE",
    "STORAGE-SEMANTICS-MATRIX",
    "AUTH-MODEL",
    "SECURITY-MODEL",
    "PRIVACY-MODEL",
    "OBSERVABILITY-MODEL",
    "FAILURE-MODES",
    "RESILIENCE-PATTERNS",
    "SLO-CATALOG",
    "COMPLIANCE-MATRIX",
    "DATA-MODEL",
    "INVARIANT-REGISTRY",
    "KEY-MANAGEMENT",
    "LIA-TEMPLATE",
    # Cross-doc invariants defined in key_management.md only
    "INV-KEY-NO-SKIP",
    "INV-KEY-OVERLAP",
    "INV-KEY-AUDIT",
    # Aliases históricos canonical em invariant_registry.md §5
    "INV-DATA-AC-REFS-EXIST",
    "INV-DATA-AUDIT-CHAIN",
    "INV-DATA-BLOB-HASH",
    "INV-DATA-BLOB-NO-ZOMBIE",
    "INV-DATA-REFCOUNT",
    "INV-DATA-TENANT-ISOLATION",
    "INV-DATA-MONOTONIC-TS",
    "INV-DATA-BILLING-RECONCILE",
    "INV-DATA-ERASURE-COMPLETE",
    "INV-TenantIsolation",
    "INV-AuditLogImmutability",
    "INV-CASIdempotency",
    "INV-QuotaEnforcement",
    "INV-DigestVerification",
    "INV-DataResidency",
    # Template placeholders
    "INV-AAA",
    "INV-BBB",
    "INV-XXX-",
    "INV-YYY",
    "INV-ZZZ",
    "INV-XXX",
    "INV-XXX-name",   # template placeholder em framework examples
    "INV-LIFECYCLE-001",  # framework-internal example
    "INV-GC",  # plural-form mention
    "FM-XXX",
    "ADR-XXXX",
    "ADR-YYYY",
    "ADR-ZZZZ",
    "ADR-YYYY-REPLACE",
    "ADR-XXXX-REPLACE",
    "ADR-XXXX-RELATED",
    # SLO usage context names (derived from SLO names)
    "SLO-CAS-GET",  # short form of SLO-AVAIL-CAS-GET
    "SLO-CORRECT-ISO",
    # CTRL placeholder em template
    "CTRL-XXX",
    # CTRL-KEY-030..032: placeholders Fase 2 BYOE (key_management §5.4)
    "CTRL-KEY-030",
    "CTRL-KEY-031",
    "CTRL-KEY-032",
    # Runbook placeholder (template exemplo)
    "RB-XXX",
    # SLOs em formato sem header standalone (definidos em corpo do §4.X mas não como anchor)
    "SLO-DEPLOY-SAFE",
    "SLO-INCIDENTS",
    "SLO-LAT-CAS-PUT-MULTIPART",
    "SLO-TENANT",
    "SLO-XXX",
    # ADRs exemplo no framework (não são deployments reais)
    "ADR-0002",
    "ADR-0007",
    "ADR-0015",
}

# IDs com prefixo wildcard (qualquer ID que comece com este prefixo é whitelist).
# Útil para padrões legados ou exemplos nos templates.
WHITELIST_PREFIXES = (
    "ADR-XXXX",
    "ADR-YYYY",
    "WI-S",  # Work Items são exemplos comuns
    "ST-",   # Sub-tasks
    "PRR-",  # PRRs são exemplos
)


def iter_spec_files() -> list[Path]:
    files = []
    for path in sorted(SPECS_DIR.rglob("*.md")):
        rel = path.relative_to(SPECS_DIR)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        files.append(path)
    return files


def is_definition_source(rel_path: str, id_type: str) -> bool:
    sources = DEFINITION_SOURCES.get(id_type, [])
    for src in sources:
        if src.endswith("/"):
            if rel_path.startswith(src):
                return True
        elif rel_path == src or rel_path.endswith("/" + src):
            return True
    return False


def extract_definitions(text: str, id_type: str) -> set[str]:
    """Find IDs of given type that appear in 'definition position'."""
    pattern = ID_PATTERNS[id_type]
    defs = set()
    for line in text.splitlines():
        line_stripped = line.strip()
        for anchor in DEFINITION_ANCHORS:
            m = anchor.match(line_stripped)
            if m:
                candidate = m.group(1)
                if pattern.fullmatch(candidate):
                    defs.add(candidate)
        # Also: H1/H2 with ID anywhere (looser)
        if line_stripped.startswith("# "):
            for m in pattern.finditer(line_stripped):
                defs.add(m.group(0))
    return defs


def extract_uses(text: str, id_type: str) -> set[str]:
    pattern = ID_PATTERNS[id_type]
    return set(pattern.findall(text))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--warn-orphans", action="store_true",
                        help="Also report orphan IDs (defined but never used).")
    parser.add_argument("--json", action="store_true",
                        help="Output report as JSON.")
    parser.add_argument("--id-types", nargs="*",
                        choices=list(ID_PATTERNS.keys()),
                        help="Limit check to these ID types.")
    args = parser.parse_args()

    id_types = args.id_types or list(ID_PATTERNS.keys())

    # definitions[id_type] = {ID: [files where defined]}
    definitions: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))
    # uses[id_type] = {ID: [files where used]}
    uses: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))

    files = iter_spec_files()
    for path in files:
        rel = str(path.relative_to(SPECS_DIR))
        text = path.read_text(encoding="utf-8")
        for id_type in id_types:
            ids_in_file = extract_uses(text, id_type)
            for id_ in ids_in_file:
                uses[id_type][id_].append(rel)
            if is_definition_source(rel, id_type):
                defs = extract_definitions(text, id_type)
                for id_ in defs:
                    definitions[id_type][id_].append(rel)

    # Compute dangling and orphans
    dangling: dict[str, dict[str, list[str]]] = defaultdict(dict)
    orphans: dict[str, dict[str, list[str]]] = defaultdict(dict)

    for id_type in id_types:
        all_uses = set(uses[id_type].keys())
        all_defs = set(definitions[id_type].keys())

        # Dangling: used but not defined
        for id_ in all_uses - all_defs - WHITELIST_IDS:
            dangling[id_type][id_] = uses[id_type][id_]

        # Orphans: defined but not used outside the canonical source itself
        for id_ in all_defs - WHITELIST_IDS:
            non_def_uses = [
                f for f in uses[id_type][id_]
                if not is_definition_source(f, id_type)
            ]
            if not non_def_uses:
                orphans[id_type][id_] = definitions[id_type][id_]

    if args.json:
        report = {
            "dangling": {k: dict(v) for k, v in dangling.items() if v},
            "orphans": {k: dict(v) for k, v in orphans.items() if v},
            "definitions_count": {k: len(v) for k, v in definitions.items()},
            "uses_count": {k: len(v) for k, v in uses.items()},
        }
        print(json.dumps(report, indent=2, ensure_ascii=False))
    else:
        print(f"=== Cross-reference report ({len(files)} docs analisados) ===\n")

        for id_type in id_types:
            n_defs = len(definitions[id_type])
            n_uses = len(uses[id_type])
            print(f"[{id_type}] definitions={n_defs}  uses={n_uses}")

        print()
        any_dangling = False
        for id_type in id_types:
            if dangling[id_type]:
                any_dangling = True
                print(f"❌ DANGLING [{id_type}] — usado mas não definido:")
                for id_, files_ in sorted(dangling[id_type].items()):
                    print(f"  {id_}")
                    for f in files_[:3]:
                        print(f"    in {f}")
                    if len(files_) > 3:
                        print(f"    ... +{len(files_)-3} arquivos")
                print()

        if not any_dangling:
            print("✅ Nenhuma dangling reference detectada.\n")

        if args.warn_orphans:
            for id_type in id_types:
                if orphans[id_type]:
                    print(f"⚠️  ORPHANS [{id_type}] — definido mas nunca referenciado fora do canonical source:")
                    for id_, files_ in sorted(orphans[id_type].items()):
                        print(f"  {id_} (defined in {files_[0]})")
                    print()

    return 1 if any(dangling[k] for k in id_types) else 0


if __name__ == "__main__":
    sys.exit(main())

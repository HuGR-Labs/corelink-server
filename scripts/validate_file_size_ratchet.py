#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
validate_file_size_ratchet.py — the god-file ratchet (B-126).

Owner decision, 2026-08-31: every source file above 1000 lines is refactored
into smaller files. This gate makes that decision irreversible without making
it unmergeable on day one.

WHY A RATCHET AND NOT A CEILING
-------------------------------
A hard ceiling at 1000 would fail on its very first run: 81 tracked files are
already above it, holding 143 667 lines. A gate that is red the day it lands
does not get obeyed, it gets loosened — and a loosened gate is worse than no
gate, because the loosening reads as "reviewed and accepted". So:

  * a file NOT in the baseline may never be created above LIMIT            (hard)
  * a file IN the baseline may not grow past its recorded size             (ratchet)
  * a file IN the baseline that drops to <= LIMIT is graduated: it leaves
    the baseline and is thereafter held to the hard rule                   (one-way)

The baseline is `reports/refactor/god-files-2026-08-31.tsv`, versioned in the
repo, so every relaxation of it is a reviewable diff rather than a flag.

WHAT THIS GATE DELIBERATELY DOES NOT DECIDE
-------------------------------------------
Whether a split IMPROVED anything. Line count is a surface metric: a 4 000-line
file cut into five 800-line files with the same responsibilities shuffled
between them passes this gate and leaves the code worse. That judgement lives in
PR review, and B-126's `verify-means` says so explicitly. This script only
guarantees the number cannot silently go the wrong way.

It also does not see files git does not track, and it excludes vendored and
generated trees (`node_modules`, `target`) — a gate that scans build output
measures the build, not the repo.

EXIT
----
0 — no new oversized file, nothing in the baseline grew.
1 — a violation, printed with the exact numbers.
2 — the gate could not measure (missing baseline, degenerate file list). This is
    NOT reported as success: "I found no violations" and "I could not look" must
    never share an exit code.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
BASELINE = REPO_ROOT / "reports" / "refactor" / "god-files-2026-08-31.tsv"
LIMIT = 1000

# Extensions the owner's decision covers: hand-written source.
SUFFIXES = (".rs", ".ts", ".tsx", ".py")

# Vendored or generated. Scanning these would measure the build, not the repo —
# and `target/` alone can dwarf the entire tracked tree.
EXCLUDE_PARTS = ("node_modules", "target", ".open-next", ".wrangler")


def tracked_sources() -> list[Path]:
    """Every tracked, hand-written source file, as git sees it."""
    out = subprocess.run(
        ["git", "-C", str(REPO_ROOT), "ls-files", *(f"*{s}" for s in SUFFIXES)],
        capture_output=True,
        text=True,
        check=False,
    )
    if out.returncode != 0:
        return []
    files = []
    for line in out.stdout.splitlines():
        if not line:
            continue
        parts = Path(line).parts
        if any(p in EXCLUDE_PARTS for p in parts):
            continue
        files.append(Path(line))
    return files


def line_count(rel: Path) -> int | None:
    """Physical lines, or None if the path is unreadable (deleted, binary, …)."""
    p = REPO_ROOT / rel
    try:
        with p.open("rb") as fh:
            return sum(1 for _ in fh)
    except OSError:
        return None


def read_baseline() -> dict[str, int] | None:
    if not BASELINE.exists():
        return None
    base: dict[str, int] = {}
    for raw in BASELINE.read_text(encoding="utf-8").splitlines():
        if not raw.strip():
            continue
        n, _, path = raw.partition("\t")
        try:
            base[path.strip()] = int(n)
        except ValueError:
            # A malformed row must not be read as "no baseline entry" — that
            # would silently downgrade a ratcheted file to the hard rule and
            # fail the build for the wrong reason.
            print(f"⛔ linha malformada na baseline: {raw!r}", file=sys.stderr)
            return None
    return base


def main() -> int:
    base = read_baseline()
    if base is None:
        print(
            "⛔ INDETERMINADO: baseline ausente ou malformada em "
            f"{BASELINE.relative_to(REPO_ROOT)} — a catraca nao pode medir. "
            "Isto NAO e 'sem violacoes'.",
            file=sys.stderr,
        )
        return 2

    files = tracked_sources()
    # A degenerate file list is the failure mode this whole campaign keeps
    # hitting: an empty result looks exactly like "nothing is oversized".
    if len(files) < 100:
        print(
            f"⛔ INDETERMINADO: git ls-files devolveu {len(files)} arquivos de codigo, "
            "o que e implausivel neste repo — o instrumento falhou, nao a arvore.",
            file=sys.stderr,
        )
        return 2

    new_offenders: list[tuple[str, int]] = []
    grew: list[tuple[str, int, int]] = []
    graduated: list[str] = []

    for rel in files:
        key = rel.as_posix()
        n = line_count(rel)
        if n is None:
            continue
        recorded = base.get(key)
        if recorded is None:
            if n > LIMIT:
                new_offenders.append((key, n))
        else:
            if n <= LIMIT:
                graduated.append(key)
            elif n > recorded:
                grew.append((key, recorded, n))

    still = sum(1 for rel in files
                if (c := line_count(rel)) is not None and c > LIMIT)
    print(f"catraca de tamanho — limite {LIMIT} linhas")
    print(f"  arquivos de codigo rastreados : {len(files)}")
    print(f"  acima do limite hoje          : {still}")
    print(f"  na baseline                   : {len(base)}")
    if graduated:
        print(f"  ✅ graduados nesta arvore     : {len(graduated)}")
        for g in sorted(graduated)[:10]:
            print(f"       {g}")
        if len(graduated) > 10:
            print(f"       … e mais {len(graduated) - 10}")
        print("     (removem-se da baseline no mesmo PR que os encolheu — a partir")
        print("      dai valem a regra dura, e voltar a crescer reprova)")

    if not new_offenders and not grew:
        print("✅ catraca OK: nenhum arquivo novo acima do limite, nenhum da baseline cresceu.")
        return 0

    print()
    for key, n in sorted(new_offenders, key=lambda t: -t[1]):
        print(f"⛔ ARQUIVO NOVO acima de {LIMIT}: {key} tem {n} linhas.")
        print("   Parta antes de mergear. Estilo da casa (zero `mod.rs` no repo):")
        print("   `foo.rs` PERMANECE como raiz do modulo e ganha um diretorio irmao `foo/`")
        print("   com os submodulos — assim o caminho citado pela wiki OKF sobrevive e o")
        print("   modulo pai nao e editado. Ver `routes/audit_export.rs` como precedente.")
    for key, was, now in sorted(grew, key=lambda t: -(t[2] - t[1])):
        print(f"⛔ CRESCEU: {key} passou de {was} para {now} linhas (+{now - was}).")
        print("   Este arquivo ja esta na campanha B-126; ele so pode encolher.")
    print()
    print("A catraca so anda para um lado. Se um destes crescimentos for inevitavel,")
    print("isso e uma decisao a defender no PR, nao um numero a editar na baseline.")
    return 1


if __name__ == "__main__":
    sys.exit(main())

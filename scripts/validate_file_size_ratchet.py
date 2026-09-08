#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Fail-closed god-file ratchet (B-126).

CI invokes this validator from the trusted base commit and gives it a
candidate commit to inspect. Candidate files are read as Git blobs rather
than through the working tree: a PR cannot replace the validator, baseline, or
an inspected source with a symlink and make the check observe something else.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(os.environ.get("GODFILE_REPO_ROOT", Path(__file__).resolve().parent.parent))
BASELINE_PATH = "reports/refactor/god-files-2026-08-31.tsv"
LIMIT = 1000

# Source census. The workflow runs on every path, so changing this declaration
# cannot evade review through a path filter.
SOURCE_SUFFIXES = (
    ".rs", ".ts", ".tsx", ".py", ".js", ".jsx", ".mjs",
    ".c", ".h", ".cc", ".cpp", ".cxx", ".go", ".java", ".kt",
    ".kts", ".rb", ".php", ".swift", ".scala", ".cs",
)
EXCLUDE_PARTS = frozenset(("node_modules", "target", ".open-next", ".wrangler"))


class MeasurementError(RuntimeError):
    """The repository could not be measured with trustworthy semantics."""


@dataclass(frozen=True)
class TreeEntry:
    mode: str
    object_type: str
    object_id: str
    path: str


def _git(*args: str) -> bytes:
    try:
        out = subprocess.run(
            ["git", "-C", str(REPO_ROOT), *args],
            capture_output=True,
            check=False,
        )
    except OSError as exc:
        raise MeasurementError(f"git indisponivel: {exc}") from exc
    if out.returncode != 0:
        detail = out.stderr.decode("utf-8", "replace").strip()
        raise MeasurementError(f"git falhou ({out.returncode}): {detail}")
    return out.stdout


def _is_source(path: str) -> bool:
    return path.endswith(SOURCE_SUFFIXES)


def _is_excluded(path: str) -> bool:
    return bool(EXCLUDE_PARTS.intersection(Path(path).parts))


def parse_tree(raw: bytes) -> list[TreeEntry]:
    """Parse `git ls-tree -rz` without lossy quoting or path splitting."""
    entries: list[TreeEntry] = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        try:
            header, raw_path = record.split(b"\t", 1)
            mode, object_type, object_id = header.split()
            path = raw_path.decode("utf-8")
        except (UnicodeDecodeError, ValueError) as exc:
            raise MeasurementError(f"entrada Git invalida: {record!r}") from exc
        entries.append(TreeEntry(mode.decode(), object_type.decode(), object_id.decode(), path))
    return entries


def tree_entries(ref: str) -> list[TreeEntry]:
    return parse_tree(_git("ls-tree", "-r", "-z", "--full-tree", ref))


def source_entries(ref: str) -> dict[str, TreeEntry]:
    selected: dict[str, TreeEntry] = {}
    for entry in tree_entries(ref):
        if _is_source(entry.path) and not _is_excluded(entry.path):
            if entry.path in selected:
                raise MeasurementError(f"path Git duplicado: {entry.path}")
            selected[entry.path] = entry
    if len(selected) < 100:
        raise MeasurementError(f"censo de fontes degenerado: {len(selected)} (<100)")
    return selected


def blob(object_id: str) -> bytes:
    return _git("cat-file", "blob", object_id)


def blob_line_count(object_id: str) -> int:
    data = blob(object_id)
    return data.count(b"\n") + (1 if data and not data.endswith(b"\n") else 0)


def blob_line_counts(object_ids: list[str]) -> dict[str, int]:
    """Count all source blobs in one Git process, preserving object framing."""
    if not object_ids:
        return {}
    try:
        out = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "cat-file", "--batch"],
            input=("".join(f"{object_id}\n" for object_id in object_ids)).encode(),
            capture_output=True,
            check=False,
        )
    except OSError as exc:
        raise MeasurementError(f"git indisponivel: {exc}") from exc
    if out.returncode != 0:
        raise MeasurementError(f"git cat-file falhou ({out.returncode})")
    data = out.stdout
    counts: dict[str, int] = {}
    offset = 0
    for object_id in object_ids:
        end = data.find(b"\n", offset)
        if end < 0:
            raise MeasurementError("cat-file truncado antes do cabecalho")
        header = data[offset:end].split()
        offset = end + 1
        if len(header) != 3 or header[0].decode("ascii", "replace") != object_id:
            raise MeasurementError("cat-file devolveu objeto inesperado")
        if header[1] == b"missing":
            raise MeasurementError(f"blob ausente: {object_id}")
        try:
            size = int(header[2])
        except ValueError as exc:
            raise MeasurementError("cat-file devolveu tamanho invalido") from exc
        body = data[offset : offset + size]
        if len(body) != size or offset + size >= len(data) or data[offset + size : offset + size + 1] != b"\n":
            raise MeasurementError("cat-file truncado no blob")
        offset += size + 1
        counts[object_id] = body.count(b"\n") + (1 if body and not body.endswith(b"\n") else 0)
    return counts


def source_counts(entries: dict[str, TreeEntry]) -> dict[str, int]:
    counts: dict[str, int] = {}
    valid: list[tuple[str, TreeEntry]] = []
    for path, entry in entries.items():
        if entry.mode not in {"100644", "100755"} or entry.object_type != "blob":
            raise MeasurementError(f"fonte nao-regular ou nao-blob: {path} (mode {entry.mode})")
        valid.append((path, entry))
    by_object = blob_line_counts([entry.object_id for _, entry in valid])
    for path, entry in valid:
        counts[path] = by_object[entry.object_id]
    return counts


def parse_baseline(text: str) -> dict[str, int]:
    base: dict[str, int] = {}
    for number, raw in enumerate(text.splitlines(), start=1):
        if not raw.strip():
            continue
        fields = raw.split("\t")
        if len(fields) != 2:
            raise MeasurementError(f"baseline linha {number} malformada: {raw!r}")
        value, path = fields[0].strip(), fields[1]
        if not value.isdigit() or int(value) <= LIMIT:
            raise MeasurementError(f"baseline linha {number} invalida: {raw!r}")
        if not path or path != path.strip() or "\\" in path or "\x00" in path:
            raise MeasurementError(f"baseline path invalido na linha {number}: {path!r}")
        if path in base:
            raise MeasurementError(f"baseline path duplicado na linha {number}: {path}")
        raw_parts = path.split("/")
        if path.startswith("/") or any(part in ("", ".", "..") for part in raw_parts):
            raise MeasurementError(f"baseline path fora do repo na linha {number}: {path}")
        if not _is_source(path) or _is_excluded(path):
            raise MeasurementError(f"baseline path nao pertence ao censo de fontes: {path}")
        base[path] = int(value)
    return base


def baseline_text(ref: str | None) -> str:
    if ref is None:
        path = REPO_ROOT / BASELINE_PATH
        try:
            return path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as exc:
            raise MeasurementError(f"baseline indisponivel: {path}") from exc
    try:
        return _git("show", f"{ref}:{BASELINE_PATH}").decode("utf-8")
    except UnicodeError as exc:
        raise MeasurementError("baseline Git nao esta em UTF-8") from exc


def validate_baseline_transition(previous: dict[str, int], candidate: dict[str, int], counts: dict[str, int]) -> list[str]:
    violations: list[str] = []
    for path, old in previous.items():
        if path not in candidate:
            if path in counts and counts[path] > LIMIT:
                violations.append(f"baseline removida antes da graduacao: {path}")
            continue
        if candidate[path] > old:
            violations.append(f"baseline aumentou: {path} {old} -> {candidate[path]}")
    for path in candidate:
        if path not in previous:
            violations.append(f"entrada nova na baseline: {path}")
    return violations


def evaluate(
    previous: dict[str, int],
    candidate: dict[str, int],
    counts: dict[str, int],
    *,
    trusted_base_counts: dict[str, int] | None = None,
) -> tuple[list[str], list[str]]:
    """Evaluate a candidate against the historical and trusted-base floors.

    The inventory is intentionally immutable evidence.  A repository may have
    accumulated growth before this gate was installed or while an older gate
    was bypassed; rejecting every future PR because of that inherited drift
    would incentivise a baseline increase.  When CI supplies a trusted base,
    the effective floor for an existing path is therefore ``max(historical,
    trusted-base)``.  A candidate still cannot grow one line beyond the tree it
    started from, and a new oversized path remains a hard failure.
    """
    baseline_violations = validate_baseline_transition(previous, candidate, counts)
    policy: list[str] = []
    for path, count in counts.items():
        recorded = previous.get(path)
        floor = recorded
        if trusted_base_counts is not None and path in trusted_base_counts:
            floor = max(floor or 0, trusted_base_counts[path])
        if recorded is None and count > LIMIT:
            policy.append(f"ARQUIVO NOVO acima de {LIMIT}: {path} tem {count} linhas")
        elif recorded is not None and floor is not None and count > floor:
            policy.append(f"CRESCEU: {path} passou de {floor} para {count} linhas (+{count - floor})")
        if path in candidate and count <= LIMIT:
            policy.append(f"baseline nao graduada: {path} caiu para {count} linhas")
    return baseline_violations, policy


def run(base_ref: str | None, head_ref: str) -> int:
    previous = parse_baseline(baseline_text(base_ref or head_ref))
    candidate = parse_baseline(baseline_text(head_ref))
    entries = source_entries(head_ref)
    counts = source_counts(entries)
    trusted_base_counts = None
    if base_ref is not None:
        trusted_base_counts = source_counts(source_entries(base_ref))
    baseline_violations, policy = evaluate(
        previous,
        candidate,
        counts,
        trusted_base_counts=trusted_base_counts,
    )
    print(f"catraca de tamanho — limite {LIMIT} linhas")
    print(f"  arquivos de codigo rastreados : {len(counts)}")
    print(f"  acima do limite hoje          : {sum(n > LIMIT for n in counts.values())}")
    print(f"  na baseline confiavel         : {len(previous)}")
    if trusted_base_counts is not None:
        print(f"  floor da arvore confiavel     : {len(trusted_base_counts)} arquivos")
    for item in sorted(baseline_violations + policy):
        print(f"⛔ {item}")
    if baseline_violations or policy:
        return 1
    print("✅ catraca OK: baseline monotona e nenhum arquivo viola o limite.")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-ref", help="commit confiavel que fornece a baseline")
    parser.add_argument("--head-ref", default="HEAD", help="commit candidato a medir")
    args = parser.parse_args(argv)
    try:
        return run(args.base_ref, args.head_ref)
    except MeasurementError as exc:
        print(f"⛔ INDETERMINADO: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Prove that every open backlog ID is assigned to exactly one WP."""

from __future__ import annotations

import re
import sys
import hashlib
import subprocess
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import backlog_verify  # noqa: E402

CATALOGS = {
    REPO_ROOT / "docs/campaigns/remediation/work-packages/B001-B045.md": (1, 45),
    REPO_ROOT / "docs/campaigns/remediation/work-packages/B046-B090.md": (46, 90),
    REPO_ROOT / "docs/campaigns/remediation/work-packages/B091-B130.md": (91, 130),
    REPO_ROOT / "docs/campaigns/remediation/work-packages/B131-B167.md": (131, 363),
}
# The D03 branch is squash-merged and is not a durable ancestry anchor. Keep
# its head as documentary provenance in the ledger, but anchor verification to
# delivered main, which remains an ancestor of both the D03 branch and its
# eventual squash commit.
LEDGER_BASE_REF = "main"
LEDGER_BASE_SHA = "ba51b02dc823cae9dbcb6ec3b5d4cc339bfa7266"
LEDGER_PATH = REPO_ROOT / "docs/campaigns/remediation/BACKLOG-WP-LEDGER.md"
ENTRY_RE = re.compile(r"^(B-\d+)\s+(WP-[A-Z0-9][A-Z0-9./_-]*)$")
WP_HEADING_RE = re.compile(r"^#{2,6}\s+(WP-[A-Z0-9][A-Z0-9./_-]*)(?:\s|—|$)", re.MULTILINE)
FIELD_PATTERNS = {
    "read-first": re.compile(
        r"\bread first\b|\bler primeiro\b|\bread-first\b", re.IGNORECASE
    ),
    "decided-change": re.compile(
        r"\bdecided change\b|\bdecisão\b|\bmudança decidida\b|\bdecision\b",
        re.IGNORECASE,
    ),
    "invariants": re.compile(r"\binvariants?\b|\binvariante(?:s)?\b", re.IGNORECASE),
    "non-goals": re.compile(
        r"\bnon-goal(?:s)?\b|\bn[ãa]o[- ]objetivo(?:s)?\b", re.IGNORECASE
    ),
    "completeness": re.compile(r"\bcompleteness\b|\bcompletude\b", re.IGNORECASE),
    "definition-of-done": re.compile(r"\bdefinition of done\b|\bDoD\b", re.IGNORECASE),
    "predecessor/integration": re.compile(
        r"\bpredecessor(?:s)?\b|\bintegration\b|\bintegração\b", re.IGNORECASE
    ),
    "return-card": re.compile(
        r"\breturn card\b|\breturn-card\b|\bretorno comum\b", re.IGNORECASE
    ),
}
WORKFLOW_MANIFEST_COUNT = 135
WORKFLOW_MANIFEST_SHA256 = "b2d70d640061bea79d6f002281d9656033da3d6096754b7e97a1d7a7cb9b2468"
PREDECESSOR_TOKEN_RE = re.compile(r"\bB-\d{3}\b|\bWP-[A-Z0-9][A-Z0-9./_-]*\b|#\d+\b")
WORKFLOW_OWNERSHIP_FENCE = "wp-workflow-ownership"
LEDGER_STATE_FENCE = "ledger-state"
WP_DEPENDENCY_ORDER_FENCE = "wp-dependency-order"

# Markdown's fenced-code grammar is stateful: a delimiter-looking line inside an
# already-open fence is content, not a new fence.  Keep this parser deliberately
# small and strict because the ledger's special fences are machine-readable
# contracts, not presentation markup.
SPECIAL_FENCES = frozenset(
    {
        "wp-coverage",
        "wp-editable-allowlist",
        WORKFLOW_OWNERSHIP_FENCE,
        LEDGER_STATE_FENCE,
        WP_DEPENDENCY_ORDER_FENCE,
    }
)
FENCE_OPEN_RE = re.compile(r"^( {0,3})(?P<delimiter>`{3,}|~{3,})(?P<info>.*)$")
FENCE_CLOSE_RE = re.compile(r"^( {0,3})(?P<delimiter>`{3,}|~{3,})(?P<trailing>.*)$")


@dataclass(frozen=True)
class _Fence:
    character: str
    length: int
    special_name: str | None
    opening_line: int


@dataclass(frozen=True)
class _SpecialBlock:
    name: str
    opening_line: int
    closing_line: int
    body: str


def _special_opening_name(info: str) -> str | None:
    """Return a special fence name, including malformed lookalikes."""
    for name in SPECIAL_FENCES:
        if info.startswith(name):
            return name
    return None


def _scan_markdown(text: str, source: str) -> tuple[list[_SpecialBlock], str]:
    """Scan fences once and return special blocks plus text outside all fences.

    Opening and closing delimiters must use the same character.  A close must
    be at least as long as its opener and contain no trailing characters.  A
    special opening is recognized only while no outer fence is active; this is
    what prevents a fake ledger fence embedded in a tilde/backtick example from
    becoming authoritative metadata.
    """
    lines = text.splitlines()
    blocks: list[_SpecialBlock] = []
    visible = list(lines)
    active: _Fence | None = None
    active_body_start = 0

    for index, line in enumerate(lines):
        if active is not None:
            visible[index] = ""
            nested_opening = FENCE_OPEN_RE.fullmatch(line)
            if nested_opening is not None:
                nested_special = _special_opening_name(
                    nested_opening.group("info")
                )
                if nested_special is not None:
                    raise LedgerError(
                        f"{source}: nested {nested_special} fence inside an outer "
                        f"{active.character * active.length} fence"
                    )
            close = FENCE_CLOSE_RE.fullmatch(line)
            if close is None:
                continue
            delimiter = close.group("delimiter")
            trailing = close.group("trailing")
            if delimiter[0] != active.character:
                continue
            if len(delimiter) < active.length:
                # A short matching delimiter is content; if no valid close
                # follows, the target is correctly reported as unterminated.
                continue
            if trailing:
                if active.special_name is not None:
                    raise LedgerError(
                        f"{source}: malformed {active.special_name} closing at line "
                        f"{index + 1}; closing must be exactly a delimiter with no "
                        "trailing characters"
                    )
                continue
            if active.special_name is not None and close.group(1):
                raise LedgerError(
                    f"{source}: malformed {active.special_name} closing at line "
                    f"{index + 1}; closing must be top-level"
                )
            if active.special_name is not None:
                body = "\n".join(lines[active_body_start:index])
                blocks.append(
                    _SpecialBlock(
                        active.special_name,
                        active.opening_line,
                        index,
                        body,
                    )
                )
            active = None
            continue

        opening = FENCE_OPEN_RE.fullmatch(line)
        if opening is None:
            continue
        delimiter = opening.group("delimiter")
        info = opening.group("info")
        character = delimiter[0]
        # Backtick info strings may not contain a backtick; such a line is not
        # a Markdown fence. Tilde info strings have no corresponding limit.
        if character == "`" and "`" in info:
            continue
        special = _special_opening_name(info)
        if special is not None:
            if info != special or opening.group(1):
                raise LedgerError(
                    f"{source}: malformed {special} opening at line {index + 1}; "
                    "opening must be an exact top-level fence"
                )
        active = _Fence(character, len(delimiter), special, index)
        active_body_start = index + 1
        visible[index] = ""

    if active is not None and active.special_name is not None:
        raise LedgerError(
            f"{source}: unterminated {active.special_name} fence opened at line "
            f"{active.opening_line + 1}"
        )
    return blocks, "\n".join(visible)


def strict_fence(text: str, fence_name: str, source: str) -> str | None:
    """Return one top-level special fence body, rejecting lookalikes."""
    if fence_name not in SPECIAL_FENCES:
        raise LedgerError(f"{source}: unsupported special fence {fence_name!r}")
    blocks, _ = _scan_markdown(text, source)
    matches = [block for block in blocks if block.name == fence_name]
    if len(matches) > 1:
        raise LedgerError(
            f"{source}: expected exactly one {fence_name} fence, found {len(matches)}"
        )
    return matches[0].body if matches else None


class LedgerError(ValueError):
    pass


def canonical_id(raw: str) -> tuple[str, int]:
    if not re.fullmatch(r"B-\d+", raw):
        raise LedgerError(f"malformed backlog ID {raw!r}")
    number = int(raw[2:])
    canonical = f"B-{number:03d}"
    if number <= 0 or raw != canonical:
        raise LedgerError(f"non-canonical backlog ID {raw!r}; expected {canonical!r}")
    return canonical, number


def parse_catalog(text: str, source: str, lower: int, upper: int) -> list[tuple[str, str]]:
    body = strict_fence(text, "wp-coverage", source)
    if body is None:
        raise LedgerError(f"{source}: expected exactly one wp-coverage fence, found 0")
    lines = [line.strip() for line in body.splitlines() if line.strip()]
    if not lines:
        raise LedgerError(f"{source}: wp-coverage population is empty")
    entries: list[tuple[str, str]] = []
    for line in lines:
        match = ENTRY_RE.fullmatch(line)
        if not match:
            raise LedgerError(f"{source}: unparseable coverage line {line!r}")
        item_id, number = canonical_id(match.group(1))
        if not lower <= number <= upper:
            raise LedgerError(f"{source}: {item_id} is outside B-{lower:03d}..B-{upper:03d}")
        entries.append((item_id, match.group(2)))
    return entries


def declared_wp_names(text: str, source: str) -> set[str]:
    _, visible = _scan_markdown(text, source)
    names = set(WP_HEADING_RE.findall(visible))
    if not names:
        raise LedgerError(f"{source}: no WP headings found")
    return names


def contract_section(text: str, wp: str, source: str) -> str:
    _, visible = _scan_markdown(text, source)
    match = re.search(
        r"^#{2,3}\s+" + re.escape(wp) + r"(?:\s|—|$).*?(?=^#{2,3}\s|\Z)",
        visible,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise LedgerError(f"{source}: missing contract section for {wp}")
    return match.group(0)


def validate_contract_section(section: str, wp: str, source: str) -> None:
    missing = [name for name, pattern in FIELD_PATTERNS.items() if not pattern.search(section)]
    has_scope = re.search(
        r"\bscope\b|\ballowlist\b|\barquivos(?: exclusivos)?\b", section, re.IGNORECASE
    )
    # Existing catalogs use both an explicit quality label and the established
    # "gates" wording. Both are contract quality controls; either is accepted.
    has_quality = re.search(r"\bquality\b|\bqualidade\b|\bgates?\b", section, re.IGNORECASE)
    if not has_scope:
        missing.append("scope/allowlist")
    if not has_quality:
        missing.append("quality")
    if missing:
        raise LedgerError(f"{source}: {wp} missing contract fields: {', '.join(missing)}")


def open_backlog_ids(text: str) -> set[str]:
    items = backlog_verify.parse(text)
    headings = backlog_verify.HEADING_RE.findall(text)
    if len(items) != len(headings):
        raise LedgerError(
            f"BACKLOG.md population truncated: {len(items)} blocks != {len(headings)} headings"
        )
    result: set[str] = set()
    seen: set[str] = set()
    for item, heading in zip(items, headings):
        backlog_verify.validate_schema(item)
        if item.problems or not item.raw:
            detail = "; ".join(item.problems) or item.detail or "unparseable block"
            raise LedgerError(f"BACKLOG.md:{item.line}: {detail}")
        item_id, _ = canonical_id(item.id)
        if heading != item_id:
            raise LedgerError(f"BACKLOG.md:{item.line}: heading {heading} != block {item_id}")
        if item_id in seen:
            raise LedgerError(f"BACKLOG.md:{item.line}: duplicate ID {item_id}")
        seen.add(item_id)
        if item.raw["status"] == "open":
            result.add(item_id)
    if not result:
        raise LedgerError("BACKLOG.md open population is empty")
    return result


def all_backlog_ids(text: str) -> set[str]:
    """Return every canonical ID, including terminal items usable as predecessors."""
    items = backlog_verify.parse(text)
    headings = backlog_verify.HEADING_RE.findall(text)
    if len(items) != len(headings):
        raise LedgerError(
            f"BACKLOG.md population truncated: {len(items)} blocks != {len(headings)} headings"
        )
    result: set[str] = set()
    for item, heading in zip(items, headings):
        backlog_verify.validate_schema(item)
        if item.problems or not item.raw:
            detail = "; ".join(item.problems) or item.detail or "unparseable block"
            raise LedgerError(f"BACKLOG.md:{item.line}: {detail}")
        item_id, _ = canonical_id(item.id)
        if heading != item_id:
            raise LedgerError(f"BACKLOG.md:{item.line}: heading {heading} != block {item_id}")
        if item_id in result:
            raise LedgerError(f"BACKLOG.md:{item.line}: duplicate ID {item_id}")
        result.add(item_id)
    return result


def backlog_status_counts(text: str) -> dict[str, int]:
    """Return the complete status population after schema validation."""
    items = backlog_verify.parse(text)
    headings = backlog_verify.HEADING_RE.findall(text)
    if len(items) != len(headings):
        raise LedgerError(
            f"BACKLOG.md population truncated: {len(items)} blocks != {len(headings)} headings"
        )
    counts: dict[str, int] = defaultdict(int)
    for item, heading in zip(items, headings):
        backlog_verify.validate_schema(item)
        if item.problems or not item.raw:
            detail = "; ".join(item.problems) or item.detail or "unparseable block"
            raise LedgerError(f"BACKLOG.md:{item.line}: {detail}")
        item_id, _ = canonical_id(item.id)
        if heading != item_id:
            raise LedgerError(f"BACKLOG.md:{item.line}: heading {heading} != block {item_id}")
        counts[item.raw["status"]] += 1
    return dict(counts)


def validate_predecessors(
    section: str, wp: str, source: str, backlog_ids: set[str], valid_wps: set[str]
) -> None:
    """Reject dangling machine-readable predecessor references.

    B-IDs resolve against the complete backlog (including done predecessors), WP names
    resolve against catalog headings, and ``#N`` is an explicitly typed external PR
    reference.  Free-form ``none``/owner/external statements remain valid contracts.
    """
    for token in PREDECESSOR_TOKEN_RE.findall(section):
        token = token.rstrip(".,;:)")
        # These are references to the contract template/schema, not work-package
        # predecessors.  They are intentionally outside the assignment namespace.
        if token in {"WP-CONTRACT", "WP-ID"}:
            continue
        if token.startswith("B-") and token not in backlog_ids:
            raise LedgerError(f"{source}: {wp} predecessor does not resolve: {token}")
        if token.startswith("WP-") and token not in valid_wps:
            raise LedgerError(f"{source}: {wp} predecessor does not resolve: {token}")


def parse_structured_allowlist(text: str, source: str) -> list[tuple[str, str, str]]:
    """Parse exact editable path ownership and immediate path predecessor rows."""
    body = strict_fence(text, "wp-editable-allowlist", source)
    if body is None:
        return []
    entries: list[tuple[str, str, str]] = []
    for raw in body.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = [part.strip() for part in line.split("|")]
        if len(parts) != 3 or not parts[0] or not parts[1] or not parts[2]:
            raise LedgerError(f"{source}: malformed editable allowlist row {raw!r}")
        predecessor = parts[2]
        if predecessor != "none" and not re.fullmatch(r"WP-[A-Z0-9][A-Z0-9./_-]*", predecessor):
            raise LedgerError(f"{source}: malformed path predecessor {predecessor!r}")
        entries.append((parts[0], parts[1], predecessor))
    if not entries:
        raise LedgerError(f"{source}: editable allowlist fence is empty")
    return entries


def validate_structured_allowlist(
    entries: list[tuple[str, str, str]], valid_wps: set[str]
) -> None:
    """Require a single explicit linear order for each multiply-owned path."""
    owners_by_path: dict[str, list[tuple[str, str]]] = defaultdict(list)
    for path, owner, predecessor in entries:
        if owner not in valid_wps:
            raise LedgerError(f"editable allowlist owner does not resolve: {owner}")
        owners_by_path[path].append((owner, predecessor))
    for path, rows in sorted(owners_by_path.items()):
        owners = [owner for owner, _ in rows]
        if len(owners) != len(set(owners)):
            raise LedgerError(f"duplicate editable owner for path: {path}")
        if len(rows) == 1:
            if rows[0][1] != "none":
                raise LedgerError(f"single owner path must start at none: {path}")
            continue
        roots = [owner for owner, predecessor in rows if predecessor == "none"]
        if len(roots) != 1:
            raise LedgerError(f"path must have exactly one initial owner: {path}")
        successors: dict[str, str] = {}
        for owner, predecessor in rows:
            if predecessor == "none":
                continue
            previous = successors.get(predecessor)
            if previous is not None:
                raise LedgerError(
                    f"path has multiple immediate successors: {path}: {predecessor} "
                    f"-> {previous}, {owner}"
                )
            successors[predecessor] = owner
        owner_set = set(owners)
        for owner, predecessor in rows:
            if predecessor != "none" and predecessor not in owner_set:
                raise LedgerError(
                    f"path predecessor is not an owner of {path}: {owner} after {predecessor}"
                )
        reached = {roots[0]}
        while True:
            next_owners = {
                owner for owner, predecessor in rows if predecessor in reached
            }
            expanded = reached | next_owners
            if expanded == reached:
                break
            reached = expanded
        if reached != owner_set:
            missing = ", ".join(sorted(owner_set - reached))
            raise LedgerError(f"path ordering is disconnected or cyclic for {path}: {missing}")


def parse_workflow_ownership(
    text: str, source: str
) -> list[tuple[str, str, str]]:
    """Parse the closed workflow path -> owner/status map."""
    body = strict_fence(text, WORKFLOW_OWNERSHIP_FENCE, source)
    if body is None:
        return []
    entries: list[tuple[str, str, str]] = []
    for raw in body.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = [part.strip() for part in line.split("|")]
        if len(parts) != 3 or not all(parts):
            raise LedgerError(f"{source}: malformed workflow ownership row {raw!r}")
        entries.append((parts[0], parts[1], parts[2]))
    if not entries:
        raise LedgerError(f"{source}: workflow ownership fence is empty")
    return entries


def validate_workflow_ownership(
    entries: list[tuple[str, str, str]],
    valid_wps: set[str],
    actual_paths: set[str],
) -> None:
    """Require a closed, fail-closed workflow map with no WP-150 writers."""
    mapped: dict[str, tuple[str, str]] = {}
    for path, owner, status in entries:
        if not re.fullmatch(r"\.github/workflows/[A-Za-z0-9_.-]+\.(?:yml|yaml)", path):
            raise LedgerError(f"workflow ownership path is not a workflow file: {path}")
        if path in mapped:
            raise LedgerError(f"duplicate workflow ownership path: {path}")
        if path not in actual_paths:
            raise LedgerError(f"workflow ownership path does not exist: {path}")
        if owner == "WP-150":
            raise LedgerError("WP-150 cannot own workflow edits; it is read-only inventory")
        if owner != "LEAD-BLOCKED" and owner not in valid_wps:
            raise LedgerError(f"workflow ownership owner does not resolve: {owner}")
        expected_status = "blocked" if owner == "LEAD-BLOCKED" else "owned"
        if status != expected_status:
            raise LedgerError(
                f"workflow ownership status for {path} must be {expected_status!r}"
            )
        mapped[path] = (owner, status)
    missing = sorted(actual_paths - mapped.keys())
    extra = sorted(mapped.keys() - actual_paths)
    if missing:
        raise LedgerError(
            "workflows outside ownership map: " + ", ".join(missing)
        )
    if extra:
        raise LedgerError("workflow ownership map contains nonexistent paths: " + ", ".join(extra))


def validate_workflow_population(repo_root: Path) -> set[str]:
    paths = sorted(
        path.relative_to(repo_root).as_posix()
        for path in (repo_root / ".github/workflows").iterdir()
        if path.is_file() and path.suffix in {".yml", ".yaml"}
    )
    payload = ("\n".join(paths) + "\n").encode()
    digest = hashlib.sha256(payload).hexdigest()
    if len(paths) != WORKFLOW_MANIFEST_COUNT or digest != WORKFLOW_MANIFEST_SHA256:
        raise LedgerError(
            "WP-150 workflow population drift: "
            f"expected {WORKFLOW_MANIFEST_COUNT}/{WORKFLOW_MANIFEST_SHA256}, "
            f"found {len(paths)}/{digest}"
        )
    return set(paths)


def parse_ledger_state(text: str, source: str) -> dict[str, str]:
    """Parse the live baseline/count declaration from the root ledger."""
    body = strict_fence(text, LEDGER_STATE_FENCE, source)
    if body is None:
        raise LedgerError(f"{source}: missing {LEDGER_STATE_FENCE} fence")
    state: dict[str, str] = {}
    for raw in body.splitlines():
        line = raw.strip()
        if not line:
            continue
        key, separator, value = line.partition(":")
        if not separator or not re.fullmatch(r"[a-z][a-z-]+", key.strip()):
            raise LedgerError(f"{source}: malformed ledger state row {raw!r}")
        key = key.strip()
        if key in state:
            raise LedgerError(f"{source}: duplicate ledger state key {key!r}")
        state[key] = value.strip()
    required = {
        "base-ref",
        "base-sha",
        "observed-at",
        "item-count",
        "open-count",
        "done-count",
        "parked-count",
        "catalog-counts",
    }
    missing = sorted(required - state.keys())
    if missing:
        raise LedgerError(f"{source}: ledger state missing keys: {', '.join(missing)}")
    if state["base-ref"] != LEDGER_BASE_REF:
        raise LedgerError(
            f"{source}: ledger base-ref must be {LEDGER_BASE_REF}"
        )
    if not re.fullmatch(r"[0-9a-f]{40}", state["base-sha"]):
        raise LedgerError(f"{source}: ledger base-sha is not a 40-digit hex SHA")
    for key in ("item-count", "open-count", "done-count", "parked-count"):
        if not re.fullmatch(r"[0-9]+", state[key]):
            raise LedgerError(f"{source}: ledger {key} is not a non-negative integer")
    return state


def validate_ledger_state(
    state: dict[str, str],
    *,
    source: str,
    item_count: int,
    status_counts: dict[str, int],
    catalog_counts: dict[str, int],
    expected_base_sha: str | None = None,
) -> None:
    """Reject stale baseline metadata and incomplete catalog populations."""
    if expected_base_sha is not None and state["base-sha"] != expected_base_sha:
        raise LedgerError(
            f"{source}: stale ledger base-sha {state['base-sha']}; "
            f"expected {expected_base_sha}"
        )
    expected = {
        "item-count": item_count,
        "open-count": status_counts.get("open", 0),
        "done-count": status_counts.get("done", 0),
        "parked-count": status_counts.get("parked", 0),
    }
    for key, value in expected.items():
        if int(state[key]) != value:
            raise LedgerError(
                f"{source}: stale ledger {key} {state[key]}; expected {value}"
            )
    declared_catalogs: dict[str, int] = {}
    for raw in state["catalog-counts"].split(","):
        key, separator, value = raw.strip().partition("=")
        if not separator or not re.fullmatch(r"B\d{3}-B\d{3}", key) or not value.isdigit():
            raise LedgerError(f"{source}: malformed catalog count {raw!r}")
        if key in declared_catalogs:
            raise LedgerError(f"{source}: duplicate catalog count {key}")
        declared_catalogs[key] = int(value)
    if declared_catalogs != catalog_counts:
        raise LedgerError(
            f"{source}: stale catalog population {declared_catalogs}; "
            f"expected {catalog_counts}"
        )


def parse_wp_dependency_order(text: str, source: str) -> list[tuple[str, tuple[str, ...]]]:
    """Parse the explicit executable order for a dependency lane."""
    body = strict_fence(text, WP_DEPENDENCY_ORDER_FENCE, source)
    if body is None:
        raise LedgerError(f"{source}: missing {WP_DEPENDENCY_ORDER_FENCE} fence")
    entries: list[tuple[str, tuple[str, ...]]] = []
    for raw in body.splitlines():
        line = raw.strip()
        if not line:
            continue
        parts = [part.strip() for part in line.split("|")]
        if len(parts) != 2 or not re.fullmatch(r"WP-[A-Z0-9][A-Z0-9./_-]*", parts[0]):
            raise LedgerError(f"{source}: malformed dependency row {raw!r}")
        raw_predecessors = parts[1]
        if raw_predecessors == "none":
            predecessors: tuple[str, ...] = ()
        else:
            values = tuple(value.strip() for value in raw_predecessors.split(","))
            if not values or any(
                not re.fullmatch(r"WP-[A-Z0-9][A-Z0-9./_-]*", value)
                for value in values
            ):
                raise LedgerError(
                    f"{source}: malformed dependency predecessor {raw_predecessors!r}"
                )
            if len(values) != len(set(values)):
                raise LedgerError(f"{source}: duplicate dependency predecessor in {parts[0]}")
            predecessors = values
        entries.append((parts[0], predecessors))
    if not entries:
        raise LedgerError(f"{source}: dependency order is empty")
    return entries


def validate_wp_dependency_order(
    entries: list[tuple[str, tuple[str, ...]]],
    valid_wps: set[str],
    source: str,
    required: dict[str, tuple[str, ...]] | None = None,
) -> None:
    """Require a unique, acyclic, self-contained executable dependency lane."""
    owners = [wp for wp, _ in entries]
    if len(owners) != len(set(owners)):
        raise LedgerError(f"{source}: duplicate WP in dependency order")
    unknown = sorted(set(owners) - valid_wps)
    if unknown:
        raise LedgerError(f"{source}: dependency order has unknown WP: {', '.join(unknown)}")
    declared = set(owners)
    predecessors = {wp: predecessor for wp, predecessor in entries}
    dangling = sorted(
        predecessor
        for values in predecessors.values()
        for predecessor in values
        if predecessor not in declared
    )
    if dangling:
        raise LedgerError(
            f"{source}: dependency predecessor is outside order: {', '.join(dangling)}"
        )
    if required is not None and predecessors != required:
        raise LedgerError(
            f"{source}: dependency order does not match required executable order: "
            f"{predecessors} != {required}"
        )
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(wp: str) -> None:
        if wp in visiting:
            raise LedgerError(f"{source}: cyclic WP dependency at {wp}")
        if wp in visited:
            return
        visiting.add(wp)
        for predecessor in predecessors[wp]:
            visit(predecessor)
        visiting.remove(wp)
        visited.add(wp)

    for wp in owners:
        visit(wp)


def compare(
    open_ids: set[str],
    assignments: list[tuple[str, str, str]],
    valid_wps: set[str] | None = None,
) -> None:
    if valid_wps is not None:
        unknown = sorted({wp for _, wp, _ in assignments if wp not in valid_wps})
        if unknown:
            raise LedgerError(f"phantom WP names: {', '.join(unknown)}")
    owners: dict[str, list[tuple[str, str]]] = defaultdict(list)
    for item_id, wp, source in assignments:
        owners[item_id].append((wp, source))
    duplicates = {item_id: values for item_id, values in owners.items() if len(values) != 1}
    missing = sorted(open_ids - owners.keys())
    terminal = sorted(owners.keys() - open_ids)
    if duplicates or missing or terminal:
        parts = []
        if missing:
            parts.append(f"missing open IDs: {', '.join(missing)}")
        if terminal:
            parts.append(f"assigned non-open IDs: {', '.join(terminal)}")
        if duplicates:
            rendered = ", ".join(
                f"{item_id}=>{values}" for item_id, values in sorted(duplicates.items())
            )
            parts.append(f"duplicate assignments: {rendered}")
        raise LedgerError("; ".join(parts))


def validate_git_anchor(
    repo_root: Path,
    *,
    source: str,
    base_ref: str = LEDGER_BASE_REF,
    base_sha: str = LEDGER_BASE_SHA,
    head_ref: str = "HEAD",
) -> None:
    """Require an immutable base object and ancestry-safe ref/head semantics.

    ``base_ref`` is descriptive and may move after integration; when it is
    available locally it must still contain ``base_sha``. The immutable SHA
    must also be an ancestor of the checked head, which survives squash merges
    that do not preserve D03's original commit objects.
    """
    resolved_base = subprocess.run(
        ["git", "rev-parse", "--verify", base_sha],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if resolved_base != base_sha:
        raise LedgerError(f"{source}: logical base unavailable: {base_sha}")
    resolved_ref = subprocess.run(
        ["git", "rev-parse", "--verify", base_ref],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if resolved_ref and subprocess.run(
        ["git", "merge-base", "--is-ancestor", base_sha, resolved_ref],
        cwd=repo_root,
        check=False,
    ).returncode != 0:
        raise LedgerError(f"{source}: base-ref {base_ref} does not contain {base_sha}")
    if subprocess.run(
        ["git", "merge-base", "--is-ancestor", base_sha, head_ref],
        cwd=repo_root,
        check=False,
    ).returncode != 0:
        raise LedgerError(f"{source}: base is not an ancestor of {head_ref}: {base_sha}")


def main() -> int:
    try:
        backlog_text = (REPO_ROOT / "BACKLOG.md").read_text()
        open_ids = open_backlog_ids(backlog_text)
        backlog_ids = all_backlog_ids(backlog_text)
        status_counts = backlog_status_counts(backlog_text)
        ledger_text = LEDGER_PATH.read_text()
        try:
            ledger_source = str(LEDGER_PATH.relative_to(REPO_ROOT))
        except ValueError:
            ledger_source = str(LEDGER_PATH)
        ledger_state = parse_ledger_state(ledger_text, ledger_source)
        expected_base_sha = LEDGER_BASE_SHA
        validate_git_anchor(
            REPO_ROOT,
            source=ledger_source,
            base_ref=LEDGER_BASE_REF,
            base_sha=expected_base_sha,
        )
        catalog_counts: dict[str, int] = {}
        assignments: list[tuple[str, str, str]] = []
        valid_wps: set[str] = set()
        sections: dict[str, str] = {}
        source_by_wp: dict[str, str] = {}
        ownership_rows: list[tuple[str, str, str]] = []
        ownership_fence_count = 0
        workflow_rows: list[tuple[str, str, str]] = []
        workflow_fence_count = 0
        for path, (lower, upper) in CATALOGS.items():
            if not path.is_file():
                raise LedgerError(f"missing catalog: {path.relative_to(REPO_ROOT)}")
            text = path.read_text()
            source = str(path.relative_to(REPO_ROOT))
            if strict_fence(text, "wp-editable-allowlist", source) is not None:
                ownership_fence_count += 1
            ownership_rows.extend(parse_structured_allowlist(text, source))
            if strict_fence(text, WORKFLOW_OWNERSHIP_FENCE, source) is not None:
                workflow_fence_count += 1
            workflow_rows.extend(parse_workflow_ownership(text, source))
            names = declared_wp_names(text, source)
            valid_wps.update(names)
            catalog_entries = parse_catalog(text, source, lower, upper)
            catalog_counts[f"B{lower:03d}-B{upper:03d}"] = len(catalog_entries)
            for item_id, wp in catalog_entries:
                section = contract_section(text, wp, source)
                validate_contract_section(section, wp, source)
                sections.setdefault(wp, section)
                source_by_wp.setdefault(wp, source)
                assignments.append((item_id, wp, str(path.relative_to(REPO_ROOT))))
        compare(open_ids, assignments, valid_wps)
        validate_ledger_state(
            ledger_state,
            source=ledger_source,
            item_count=sum(status_counts.values()),
            status_counts=status_counts,
            catalog_counts=catalog_counts,
            expected_base_sha=expected_base_sha,
        )
        dependency_order = parse_wp_dependency_order(
            ledger_text, ledger_source
        )
        validate_wp_dependency_order(
            dependency_order,
            valid_wps,
            ledger_source,
            required={
                "WP-140": (),
                "WP-146": (),
                "WP-148": ("WP-140", "WP-146"),
                "WP-150": ("WP-148",),
            },
        )
        for wp, section in sections.items():
            validate_predecessors(section, wp, source_by_wp[wp], backlog_ids, valid_wps)
        if ownership_fence_count != 1:
            raise LedgerError(
                f"expected exactly one editable allowlist fence, found {ownership_fence_count}"
            )
        validate_structured_allowlist(ownership_rows, valid_wps)
        if workflow_fence_count != 1:
            raise LedgerError(
                f"expected exactly one workflow ownership fence, found {workflow_fence_count}"
            )
        validate_workflow_ownership(
            workflow_rows, valid_wps, validate_workflow_population(REPO_ROOT)
        )
    except LedgerError as error:
        print(f"BROKEN: {error}", file=sys.stderr)
        return 1
    print(
        f"CONFIRMED: {len(open_ids)} open backlog IDs, {len(assignments)} unique WP assignments, "
        f"{len(CATALOGS)} catalogs"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

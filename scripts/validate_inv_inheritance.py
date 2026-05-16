#!/usr/bin/env python3
"""
validate_inv_inheritance.py — W26-P2-03 (DEFER-POST-GA absorption,
wave-30 stream-08 → wave-26 closure).

Parses the **machine-readable inheritance index** embedded as a YAML
fenced block under §3.30 of
`specs/03_architecture/invariant_registry.md` (header
`# inv-inheritance-index v1`) and enforces three integrity rules:

1. Every `inherits_from` target exists:
     - `specs/tla/<name>.tla`              → file exists on disk
     - `specs/tla/<name>.tla:<Property>`   → file exists AND
                                             `<Property>` appears as a
                                             token in the TLA source
     - `INV-<NAME>`                        → INV is defined in
                                             `invariant_registry.md`
                                             §3.X (table row)

2. Bidirectional integrity — `inherits_from` and `parents.inherited_by`
   form an undirected edge that MUST appear from both sides:
     - for every (child, parent), `parents[parent].inherited_by`
       MUST list `child`
     - for every (parent, child) in `parents`, the child's `chains`
       entry MUST list `parent` in `inherits_from`

3. Additivity — the index is OPTIONAL. INVs without an entry are not
   flagged. The validator only enforces integrity of declared chains.

Exit codes:
    0   index parses + all chains valid + bidirectional links balanced
    1   one or more integrity failures (each surfaced with `[INV-X]` or
        `[parents[T]]` prefix and a human-readable explanation)
    2   could not locate registry file or the YAML block

Uso:
    python3 scripts/validate_inv_inheritance.py
    python3 scripts/validate_inv_inheritance.py --registry <path>
    python3 scripts/validate_inv_inheritance.py --verbose

Dependencies:
    pip install pyyaml
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Iterable

try:
    import yaml
except ImportError:
    sys.exit("ERROR: pyyaml not installed. `pip install pyyaml`")


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REGISTRY = REPO_ROOT / "specs" / "03_architecture" / "invariant_registry.md"

# Pattern that fences the index — a code block starting with
# `# inv-inheritance-index v1` marker on its first line.
INDEX_BLOCK_RE = re.compile(
    r"```yaml\n(# inv-inheritance-index v1.*?)\n```",
    re.DOTALL,
)

# Same pattern used by validate_inv_promotion.py — matches table rows
# starting with `| **INV-X** ...` or `| INV-X ...`.
REGISTRY_INV_ROW_RE = re.compile(
    r"^\|\s*\*?\*?(INV-[A-Z][A-Z0-9_-]+)\*?\*?\s*\|",
    re.MULTILINE,
)

# A canonical INV-ID is uppercase letters + digits + `-` + `_`,
# starting with INV-LETTER.
INV_ID_RE = re.compile(r"^INV-[A-Z][A-Z0-9_-]+$")

# A property reference is `specs/tla/<name>.tla:<PropertyName>`.
TLA_PROPERTY_RE = re.compile(
    r"^(specs/tla/[A-Za-z0-9_./-]+\.tla):([A-Za-z_][A-Za-z0-9_]*)$"
)

# A plain TLA file reference is `specs/tla/<name>.tla` (no `:` suffix).
TLA_FILE_RE = re.compile(r"^specs/tla/[A-Za-z0-9_./-]+\.tla$")


def extract_index_yaml(registry_text: str) -> str | None:
    """Return the YAML body of the inheritance index, or None if absent."""
    match = INDEX_BLOCK_RE.search(registry_text)
    if not match:
        return None
    return match.group(1)


def extract_registered_invs(registry_text: str) -> set[str]:
    """All INV-IDs defined as table rows in §3.X of the registry."""
    return set(REGISTRY_INV_ROW_RE.findall(registry_text))


def classify_target(target: str) -> str:
    """Return one of 'inv', 'tla_property', 'tla_file', 'invalid'."""
    if INV_ID_RE.match(target):
        return "inv"
    if TLA_PROPERTY_RE.match(target):
        return "tla_property"
    if TLA_FILE_RE.match(target):
        return "tla_file"
    return "invalid"


def check_target_exists(
    target: str,
    *,
    repo_root: Path,
    registered_invs: set[str],
) -> tuple[bool, str]:
    """Return (ok, reason-if-failed)."""
    kind = classify_target(target)
    if kind == "invalid":
        return False, (
            f"target '{target}' is not a valid form "
            f"(expected 'INV-…' or 'specs/tla/<name>.tla' "
            f"or 'specs/tla/<name>.tla:<PropertyName>')"
        )
    if kind == "inv":
        if target not in registered_invs:
            return False, (
                f"INV target '{target}' is not registered in any §3.X "
                f"table row"
            )
        return True, ""
    if kind == "tla_file":
        path = repo_root / target
        if not path.is_file():
            return False, f"TLA spec file '{target}' does not exist on disk"
        return True, ""
    # tla_property
    match = TLA_PROPERTY_RE.match(target)
    assert match is not None  # guaranteed by classify_target
    file_part, prop_part = match.group(1), match.group(2)
    path = repo_root / file_part
    if not path.is_file():
        return False, (
            f"TLA spec file '{file_part}' (referenced by '{target}') "
            f"does not exist on disk"
        )
    body = path.read_text(encoding="utf-8", errors="replace")
    # Property must appear as a token (word boundary). Anchoring with `\b`
    # is sufficient — TLA property names are alphanumeric+underscore.
    if not re.search(rf"\b{re.escape(prop_part)}\b", body):
        return False, (
            f"property '{prop_part}' not found in {file_part} "
            f"(target '{target}')"
        )
    return True, ""


def validate_index(
    index_body: str,
    *,
    repo_root: Path,
    registered_invs: set[str],
) -> tuple[bool, list[str], dict]:
    """Return (ok, errors, parsed-data)."""
    try:
        data = yaml.safe_load(index_body)
    except yaml.YAMLError as exc:
        return False, [f"YAML parse error in inheritance index: {exc}"], {}

    if not isinstance(data, dict):
        return False, [
            f"inheritance index root must be a mapping; got "
            f"{type(data).__name__}"
        ], {}

    errors: list[str] = []
    chains = data.get("chains", [])
    parents = data.get("parents", [])

    if not isinstance(chains, list):
        errors.append("'chains' must be a list")
        chains = []
    if not isinstance(parents, list):
        errors.append("'parents' must be a list")
        parents = []

    # Rule 1 — every chain's `inv` must be a registered INV and every
    # `inherits_from` target must resolve.
    chain_by_inv: dict[str, list[str]] = {}
    for i, entry in enumerate(chains):
        if not isinstance(entry, dict):
            errors.append(f"[chains[{i}]] entry must be a mapping")
            continue
        inv = entry.get("inv")
        targets = entry.get("inherits_from")
        if not isinstance(inv, str):
            errors.append(f"[chains[{i}]] 'inv' missing or not a string")
            continue
        if not INV_ID_RE.match(inv):
            errors.append(
                f"[chains[{i}]] 'inv' '{inv}' is not a valid INV-ID form"
            )
            continue
        if inv not in registered_invs:
            errors.append(
                f"[{inv}] is declared in inheritance index but not "
                f"registered in any §3.X table row"
            )
        if not isinstance(targets, list) or not targets:
            errors.append(
                f"[{inv}] 'inherits_from' must be a non-empty list"
            )
            continue
        chain_by_inv.setdefault(inv, []).extend(targets)
        for t in targets:
            if not isinstance(t, str):
                errors.append(
                    f"[{inv}] inherits_from entry must be a string; "
                    f"got {type(t).__name__}"
                )
                continue
            ok, reason = check_target_exists(
                t, repo_root=repo_root, registered_invs=registered_invs
            )
            if not ok:
                errors.append(f"[{inv}] inherits_from {reason}")

    # Build parents-by-target map for symmetry checks.
    parent_by_target: dict[str, list[str]] = {}
    for j, entry in enumerate(parents):
        if not isinstance(entry, dict):
            errors.append(f"[parents[{j}]] entry must be a mapping")
            continue
        target = entry.get("target")
        children = entry.get("inherited_by")
        if not isinstance(target, str):
            errors.append(
                f"[parents[{j}]] 'target' missing or not a string"
            )
            continue
        if classify_target(target) == "invalid":
            errors.append(
                f"[parents[{target}]] target is not a valid form "
                f"(expected INV-… or specs/tla/...tla[:Prop])"
            )
        # Existence check — same as `inherits_from`.
        ok, reason = check_target_exists(
            target, repo_root=repo_root, registered_invs=registered_invs
        )
        if not ok:
            errors.append(f"[parents[{target}]] {reason}")
        if not isinstance(children, list) or not children:
            errors.append(
                f"[parents[{target}]] 'inherited_by' must be a "
                f"non-empty list"
            )
            continue
        parent_by_target.setdefault(target, []).extend(children)
        for c in children:
            if not isinstance(c, str) or not INV_ID_RE.match(c):
                errors.append(
                    f"[parents[{target}]] inherited_by entry '{c}' is "
                    f"not a valid INV-ID form"
                )
                continue
            if c not in registered_invs:
                errors.append(
                    f"[parents[{target}]] inherited_by lists '{c}' but "
                    f"that INV is not registered in any §3.X table row"
                )

    # Rule 2a — every (child, parent) in chains must appear in parents.
    for inv, targets in chain_by_inv.items():
        for t in targets:
            listed = parent_by_target.get(t, [])
            if inv not in listed:
                errors.append(
                    f"[{inv}] inherits_from cites parent '{t}' but "
                    f"parents[{t}].inherited_by does not list '{inv}' "
                    f"(asymmetric link)"
                )

    # Rule 2b — every (parent, child) in parents must appear in chains.
    for t, children in parent_by_target.items():
        for c in children:
            listed = chain_by_inv.get(c, [])
            if t not in listed:
                errors.append(
                    f"[parents[{t}]] inherited_by lists '{c}' but "
                    f"'{c}' has no chain entry citing '{t}' "
                    f"(asymmetric link)"
                )

    return (len(errors) == 0), errors, data


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--registry",
        type=Path,
        default=DEFAULT_REGISTRY,
        help=f"path to invariant_registry.md (default: {DEFAULT_REGISTRY})",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help=f"repo root (default: {REPO_ROOT})",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="print summary of validated chains on success",
    )
    args = parser.parse_args(argv)

    if not args.registry.is_file():
        print(
            f"ERROR: registry not found at {args.registry}",
            file=sys.stderr,
        )
        return 2

    text = args.registry.read_text(encoding="utf-8")
    body = extract_index_yaml(text)
    if body is None:
        # Index absent — valid by additivity. Exit 0 with note.
        print(
            "No `# inv-inheritance-index v1` block found in "
            f"{args.registry.relative_to(args.repo_root) if args.repo_root in args.registry.parents else args.registry}. "
            "Inheritance is optional; nothing to validate."
        )
        return 0

    registered = extract_registered_invs(text)
    ok, errors, data = validate_index(
        body, repo_root=args.repo_root, registered_invs=registered
    )

    if not ok:
        print("Inheritance index integrity check FAILED:")
        for e in errors:
            print(f"  - {e}")
        return 1

    chains = data.get("chains", []) or []
    parents = data.get("parents", []) or []
    print(
        f"Inheritance index OK: {len(chains)} child chain(s) across "
        f"{len(parents)} parent target(s)."
    )
    if args.verbose:
        for entry in chains:
            inv = entry.get("inv")
            targets = entry.get("inherits_from", [])
            print(f"  {inv}  ->  {', '.join(targets)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

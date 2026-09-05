#!/usr/bin/env python3
"""Fail-closed census of cross-event workflow concurrency collisions.

This is the B-150 instrument.  A ``github.ref`` group is not enough to keep a
push, scheduled run, and manual dispatch on ``main`` apart: all three resolve
to ``refs/heads/main``.  When cancellation is active, one can silently remove
the other's required check.  The parser reads YAML structure, rejects duplicate
keys and malformed workflows, and only treats a cancellation expression as
inactive when its value is provable for every main-ref event.

Exit 0 means no collision remains; exit 1 means a collision or unreadable
input. ``--self-test`` exercises parser and mutation teeth without the repo.
"""

from __future__ import annotations

import argparse
import glob
import re
import shutil
import sys
import tempfile
from pathlib import Path

import yaml

MAIN_EVENTS = frozenset({"push", "schedule", "workflow_dispatch", "workflow_run", "repository_dispatch"})
EXCLUDED = frozenset({
    "backlog-verify", "byok_kill_switch_drill_weekly", "byok_matrix_weekly",
    "dr-drill-monthly", "nightly", "perf-nightly",
})
REF_FIELD = re.compile(r"(?<![\w.])github\.ref(?![\w.])")
EVENT_FIELD = re.compile(r"(?<![\w.])github\.event_name(?![\w.])")
PR_CANCEL_EXPR = re.compile(
    r"\$\{\{\s*github\.event_name\s*([!=]=)\s*['\"]pull_request['\"]\s*\}\}"
)


class StrictLoader(yaml.SafeLoader):
    """SafeLoader variant that refuses duplicate mapping keys."""


def strict_mapping(loader: StrictLoader, node: yaml.MappingNode, deep: bool = False):
    mapping = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise yaml.constructor.ConstructorError(
                None, None, f"duplicate YAML key {key!r}", key_node.start_mark
            )
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


StrictLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, strict_mapping
)


def load_workflow(path: Path) -> dict:
    try:
        document = yaml.load(path.read_text(encoding="utf-8"), Loader=StrictLoader)
    except (OSError, yaml.YAMLError) as error:
        raise ValueError(f"{path}: unreadable YAML ({error})") from error
    if not isinstance(document, dict):
        raise ValueError(f"{path}: workflow is not a YAML mapping")
    return document


def events(document: dict, path: Path) -> frozenset[str]:
    # YAML 1.1 (PyYAML) resolves an unquoted Actions ``on`` key to True.
    trigger = document.get(True, document.get("on"))
    if isinstance(trigger, dict):
        return frozenset(str(key) for key in trigger)
    if isinstance(trigger, list):
        return frozenset(str(value) for value in trigger)
    if isinstance(trigger, str):
        return frozenset({trigger})
    raise ValueError(f"{path}: top-level on: trigger is missing or malformed")


def expressions(value: str) -> list[str]:
    return re.findall(r"\$\{\{(.*?)\}\}", value, flags=re.DOTALL)


def group_fields(value: str) -> tuple[bool, bool]:
    """Return (has ref field, has direct event-name field) structurally.

    A literal such as ``${{ 'github.ref' }}`` is not a context field. Any
    non-literal expression that mentions the field is conservatively treated as
    a possible ref discriminator; the scanner is allowed to report a false
    positive, never to manufacture a false clean result.
    """
    has_ref = has_event = False
    for expression in expressions(value):
        candidate = expression.strip()
        if re.fullmatch(r"['\"]github\.ref['\"]", candidate):
            continue
        if REF_FIELD.search(candidate):
            has_ref = True
        if re.fullmatch(r"github\.event_name", candidate):
            has_event = True
    return has_ref, has_event


def cancellation_for_main(raw: object, main_events: frozenset[str], path: Path) -> bool:
    """Return whether cancellation is possible for two main-ref events.

    Unknown expressions are conservatively active.  The only recognized false
    form is the existing PR-only guard, which is false for every MAIN event.
    """
    if isinstance(raw, bool):
        return raw and len(main_events) >= 2
    if not isinstance(raw, str):
        raise ValueError(f"{path}: cancel-in-progress is not boolean or expression")
    value = raw.strip()
    match = PR_CANCEL_EXPR.fullmatch(value)
    if match and match.group(1) == "==":
        return False
    if match and match.group(1) == "!=":
        return len(main_events) >= 2
    # An unknown expression may be true for two events.  Do not guess false.
    return len(main_events) >= 2


def scan(directory: Path) -> tuple[list[tuple[str, int]], int]:
    files = sorted(set(map(Path, glob.glob(str(directory / "*.yml")) + glob.glob(str(directory / "*.yaml")))))
    if len(files) < 50:
        raise ValueError(f"only {len(files)} workflow files found; refusing a partial census")
    violations: list[tuple[str, int]] = []
    concurrency_count = 0
    for path in files:
        document = load_workflow(path)
        block = document.get("concurrency")
        if block is None:
            continue
        if not isinstance(block, dict):
            raise ValueError(f"{path}: top-level concurrency must be a mapping")
        concurrency_count += 1
        name = path.stem
        if name in EXCLUDED:
            continue
        main_events = MAIN_EVENTS & events(document, path)
        if len(main_events) < 2:
            continue
        group = block.get("group", "")
        if not isinstance(group, str):
            raise ValueError(f"{path}: concurrency.group must be a scalar string")
        # Only an expression result can carry a ref. A comment/literal saying
        # "github.ref" must not manufacture either a finding or a pass.
        has_ref, has_event = group_fields(group)
        if not has_ref or has_event:
            continue
        if cancellation_for_main(block.get("cancel-in-progress", False), main_events, path):
            # Record a stable line anchor for the ledger and operator output.
            line = next((i for i, text in enumerate(path.read_text().splitlines(), 1)
                         if re.match(r"^\s*group\s*:", text)), 0)
            violations.append((path.name, line))
    if concurrency_count < 20:
        raise ValueError(f"only {concurrency_count} concurrency blocks found; refusing a partial census")
    return violations, concurrency_count


def fixture(directory: Path, name: str, group: str, cancel: str = "true", trigger: str = "  push:\n  schedule:\n") -> Path:
    path = directory / name
    path.write_text(
        "on:\n" + trigger + "concurrency:\n  group: " + group +
        "\n  cancel-in-progress: " + cancel +
        "\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n",
        encoding="utf-8",
    )
    return path


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="concurrency-event-census-") as raw:
        directory = Path(raw)
        # Satisfy the anti-vacancy population checks with 50 no-concurrency files.
        for index in range(50):
            fixture(directory, f"padding-{index}.yml", "literal", "false", "  push:\n")
        safe = fixture(directory, "safe.yml", "ci-${{ github.event_name }}-${{ github.ref }}")
        conditional = fixture(
            directory, "conditional.yml", "ci-${{ github.workflow }}-${{ github.ref }}",
            "${{ github.event_name == 'pull_request' }}",
        )
        bad = fixture(directory, "bad.yml", "ci-${{ github.ref }}")
        unknown = fixture(directory, "unknown.yml", "ci-${{ github.ref }}", "${{ cancelled }}")
        literal = fixture(directory, "literal.yml", "ci-${{ 'github.ref' }}")
        violations, total = scan(directory)
        names = {name for name, _ in violations}
        if total != 55 or names != {"bad.yml", "unknown.yml"}:
            print(f"SELF-TEST FAILED: got blocks={total}, violations={sorted(names)}")
            return 1

        # Mutation 1: adding the event discriminator must remove the finding.
        bad.write_text(bad.read_text().replace("ci-${{ github.ref }}", "ci-${{ github.event_name }}-${{ github.ref }}"))
        violations, _ = scan(directory)
        if any(name == "bad.yml" for name, _ in violations):
            print("SELF-TEST FAILED: event-discriminator mutation stayed green")
            return 1

        # Mutation 2: corrupt exactly one workflow; unreadable input is fatal.
        bad.write_text("on: [unclosed\n", encoding="utf-8")
        try:
            scan(directory)
        except ValueError:
            pass
        else:
            print("SELF-TEST FAILED: malformed YAML was silently omitted")
            return 1

        # Mutation 3: duplicate structural key must not be first/last-wins.
        bad.write_text(
            "on:\n  push:\n  schedule:\nconcurrency:\n  group: one\n  group: two\n"
            "  cancel-in-progress: true\njobs: {}\n", encoding="utf-8"
        )
        try:
            scan(directory)
        except ValueError:
            pass
        else:
            print("SELF-TEST FAILED: duplicate concurrency key was accepted")
            return 1
    print("SELF-TEST OK: structural YAML, event semantics, unknown-expression fail-closed, and mutations")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", nargs="?", type=Path, default=Path(".github/workflows"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    try:
        violations, total = scan(args.directory)
    except ValueError as error:
        print(f"FATAL: {error}", file=sys.stderr)
        return 1
    print(f"scanned blocks={total} collisions={len(violations)}")
    for name, line in violations:
        print(f"  VIOLATION: {name}:{line}")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())

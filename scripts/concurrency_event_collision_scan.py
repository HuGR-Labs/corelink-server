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
import ast
import glob
import re
import shutil
import sys
import tempfile
from pathlib import Path

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover - exercised on CI's stdlib lane
    yaml = None

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


class StrictLoader(yaml.SafeLoader if yaml is not None else object):
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


if yaml is not None:
    StrictLoader.add_constructor(
        yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, strict_mapping
    )


def _minimal_workflow_load(text: str, path: Path) -> dict:
    """Read only the Actions fields this census needs, without PyYAML.

    The workflow lane is intentionally stdlib-only.  This small structural
    reader is not a general YAML implementation: it handles top-level
    ``on``/``concurrency`` mappings and rejects malformed bracket values and
    duplicate keys instead of silently guessing.  Full PyYAML remains used
    when available, while this fallback keeps missing optional tooling from
    turning a runnable verifier into an environment false-negative.
    """
    lines = text.splitlines()
    result: dict = {}

    def scalar(raw: str) -> object:
        value = _without_comment(raw).strip()
        if value in {"", "{}"}:
            return {} if value == "{}" else ""
        if value in {"true", "false"}:
            return value == "true"
        if value in {"null", "~"}:
            return None
        if value.startswith("["):
            if not value.endswith("]"):
                raise ValueError(f"{path}: malformed flow sequence")
            return [item.strip().strip("'\"") for item in value[1:-1].split(",") if item.strip()]
        if value.startswith("{"):
            if not value.endswith("}"):
                raise ValueError(f"{path}: malformed flow mapping")
            return {}
        if value[:1] in {"'", '"'}:
            try:
                return ast.literal_eval(value)
            except (SyntaxError, ValueError):
                raise ValueError(f"{path}: malformed quoted scalar") from None
        return value

    def key_value(line: str) -> tuple[str, str] | None:
        match = re.match(r"^([A-Za-z_][A-Za-z0-9_-]*|['\"][^'\"]+['\"]):(?:[ \t]*(.*))?$", line)
        if not match:
            return None
        return match.group(1).strip("'\""), match.group(2) or ""

    def immediate_children(start: int) -> tuple[list[tuple[str, str]], list[str], int]:
        mappings: list[tuple[str, str]] = []
        sequence: list[str] = []
        index = start
        while index < len(lines):
            child = lines[index]
            if not child.strip() or child.lstrip().startswith("#"):
                index += 1
                continue
            if not child[0].isspace():
                break
            indent = len(child) - len(child.lstrip(" "))
            if indent < 2:
                break
            # Nested workflow data (paths, branches, job steps) is not part of
            # the trigger/concurrency map.  Looking past it was the old
            # fallback's false-negative: path list entries became event names.
            if indent != 2:
                index += 1
                continue
            stripped = child.strip()
            if stripped.startswith("-"):
                if mappings:
                    raise ValueError(f"{path}: mixed mapping and sequence")
                sequence.append(stripped[1:].strip())
            else:
                pair = key_value(stripped)
                if pair is None:
                    raise ValueError(f"{path}: ambiguous indented YAML")
                mappings.append(pair)
            index += 1
        return mappings, sequence, index

    index = 0
    while index < len(lines):
        line = lines[index]
        if not line or line[0].isspace() or line.lstrip().startswith("#"):
            index += 1
            continue
        pair = key_value(_without_comment(line))
        if pair is None:
            if line.strip() == "---" and not result:
                index += 1
                continue
            raise ValueError(f"{path}: ambiguous top-level YAML")
        key, inline = pair
        if key in result:
            raise ValueError(f"{path}: duplicate top-level key {key!r}")
        if key == "on":
            if inline.strip():
                trigger = scalar(inline)
            else:
                mappings, sequence, index = immediate_children(index + 1)
                if mappings and sequence:
                    raise ValueError(f"{path}: mixed trigger mapping and sequence")
                trigger = sequence if sequence else {child_key: scalar(value) for child_key, value in mappings}
                if len(mappings) != len(trigger):
                    raise ValueError(f"{path}: duplicate trigger key")
            result[True] = trigger
            continue
        if key == "concurrency":
            if inline.strip():
                block = scalar(inline)
                if not isinstance(block, dict):
                    raise ValueError(f"{path}: concurrency must be a mapping")
            else:
                mappings, sequence, index = immediate_children(index + 1)
                if sequence:
                    raise ValueError(f"{path}: concurrency must be a mapping")
                block = {}
                for child_key, value in mappings:
                    if child_key in block:
                        raise ValueError(f"{path}: duplicate concurrency key {child_key!r}")
                    block[child_key] = scalar(value)
            result[key] = block
            continue
        index += 1
    if "on" not in result and True not in result:
        raise ValueError(f"{path}: top-level on: trigger is missing or malformed")
    return result


def _without_comment(value: str) -> str:
    """Remove a YAML comment without truncating a quoted ``#`` value."""
    quote: str | None = None
    escaped = False
    for index, character in enumerate(value):
        if escaped:
            escaped = False
            continue
        if character == "\\" and quote == '"':
            escaped = True
            continue
        if character in {"'", '"'}:
            if quote == character:
                quote = None
            elif quote is None:
                quote = character
        elif character == "#" and quote is None and (index == 0 or value[index - 1].isspace()):
            return value[:index]
    return value


def load_workflow(path: Path) -> dict:
    try:
        text = path.read_text(encoding="utf-8")
        if yaml is None:
            return _minimal_workflow_load(text, path)
        document = yaml.load(text, Loader=StrictLoader)
    except (OSError, ValueError, (yaml.YAMLError if yaml is not None else Exception)) as error:
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
    # Exercise the actual stdlib path even when PyYAML is installed locally.
    global yaml
    saved_yaml = yaml

    def finish(code: int) -> int:
        global yaml
        yaml = saved_yaml
        return code

    yaml = None
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
        third = fixture(directory, "third.yml", "ci-${{ github.ref }}", "${{ github.event_name != 'pull_request' }}")
        literal = fixture(directory, "literal.yml", "ci-${{ 'github.ref' }}")
        violations, total = scan(directory)
        names = {name for name, _ in violations}
        if total != 56 or names != {"bad.yml", "third.yml", "unknown.yml"}:
            print(f"SELF-TEST FAILED: got blocks={total}, violations={sorted(names)}")
            return finish(1)

        # Mutations 1-3: each live collision must disappear only when its
        # group gains the event discriminator; the other two remain red.
        for target in (bad, unknown, third):
            target.write_text(target.read_text().replace(
                "ci-${{ github.ref }}", "ci-${{ github.event_name }}-${{ github.ref }}", 1
            ))
            violations, _ = scan(directory)
            if any(name == target.name for name, _ in violations):
                print(f"SELF-TEST FAILED: event-discriminator mutation stayed green for {target.name}")
                return finish(1)

        # Mutation 4: corrupt exactly one workflow; unreadable input is fatal.
        bad.write_text("on: [unclosed\n", encoding="utf-8")
        try:
            scan(directory)
        except ValueError:
            pass
        else:
            print("SELF-TEST FAILED: malformed YAML was silently omitted")
            return finish(1)

        # Mutation 5: duplicate structural key must not be first/last-wins.
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
            return finish(1)
    print("SELF-TEST OK: structural YAML, event semantics, unknown-expression fail-closed, and mutations")
    return finish(0)


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

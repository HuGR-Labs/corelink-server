#!/usr/bin/env python3
"""Check that BACKLOG.md still agrees with reality.

WHY THIS EXISTS
---------------
On 2026-08-23 a day of planning was built on notes that had quietly gone stale.
Three items recorded as open had in fact shipped days earlier; one cited a count
of hosted CI lanes that no longer matched either the file count or the job count.
Nothing was dishonest — the notes were simply written once and never re-checked,
and no mechanism existed that could notice.

So a list is not enough. The list has to be able to disagree with the world and
say so out loud. Every item in BACKLOG.md therefore carries its own verification
declaration. This gate validates the declaration and its trusted-base transition,
but deliberately does not execute it: a PR is an untrusted data source.

An item that genuinely cannot be checked by a command declares `verify: manual`
and must carry a fresh `last-verified` date. Those decay: past `max-age-days`
they go STALE and fail the gate. That is the whole point — an unverifiable claim
is allowed, but it is not allowed to sit unchallenged forever, which is exactly
what happened to the notes this replaces.

USAGE
-----
    python3 scripts/backlog_verify.py            # check every item
    python3 scripts/backlog_verify.py --id B-003 # check one
    python3 scripts/backlog_verify.py --format json
    python3 scripts/backlog_verify.py --candidate-file PR/BACKLOG.md \
        --trusted-file BASE/BACKLOG.md --candidate-root PR --trusted-root BASE

Exit code is 0 only when every item is CONFIRMED. Anything else is a red gate.
Candidate mode validates the PR register, dense-ID population, allowed field
transitions, and trusted control closure but never executes candidate
`verify:` strings or candidate verifier code.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parent.parent
BACKLOG_PATH = REPO_ROOT / "BACKLOG.md"

# A fenced ```backlog block is the machine-readable half of an item; the prose
# around it is the human half. One file, one source — no generated mirror to
# drift out of sync (this repo has been bitten by hand-editing a generated file).
BLOCK_RE = re.compile(r"^```backlog\n(.*?)^```", re.MULTILINE | re.DOTALL)

# The prose half of an item is titled `### B-0NN — …`, and every reference from
# outside this file — a CHANGELOG entry, a PR body, a runbook — cites that
# heading. The block's `id:` is what the gate reads. Nothing made the two agree,
# and on 2026-08-24 they silently disagreed: the forked-partitions item was
# headed B-026 while its block declared `id: B-024`, and the Turborepo item held
# the mirror image. Both ids were unique, so the duplicate check below was
# content; the register simply pointed the wrong way for anyone following an id.
HEADING_RE = re.compile(r"^### (B-\d+)\b", re.MULTILINE)

REQUIRED_FIELDS = ("id", "repo", "owner", "status", "verify", "verify-means", "last-verified")
VALID_STATUS = ("open", "done", "parked")
VALID_OWNER = ("tl", "owner")
DEFAULT_MAX_AGE_DAYS = 14
# Candidate and BASE trees are data. No verifier command is executed by this
# script; semantic verifier execution belongs to the exact-SHA local/orchestrator
# lane until disposable CI capacity exists.
MAX_BACKLOG_BYTES = 2_000_000
IMMUTABLE_ITEM_FIELDS = frozenset({
    "id", "repo", "verify", "verify-means", "action-packet", "source-document",
    "source-locator", "finding-title", "problem", "evidence", "acceptance",
})
ALLOWED_TRANSITION_FIELDS = frozenset({"status", "owner", "last-verified"})

# A command's polarity cannot be inferred from arbitrary shell.  We can still
# reject the known dangerous declaration: a `done` item whose human explanation
# explicitly says the check remains `open`.  Unmarked legacy entries remain
# compatible; authors may describe their inverted guard in their existing prose.
OPEN_VERIFY_MARKER_RE = re.compile(
    r"^(?:open\b|(?:polaridade|polarity)\s+(?:abert[ao]|open)\b|"
    r"(?:polaridade|polarity)\s+de\s+item\s+abert[oa]\b)",
    re.IGNORECASE,
)

CONFIRMED, DRIFTED, STALE, BROKEN = "CONFIRMED", "DRIFTED", "STALE", "BROKEN"


@dataclass
class Item:
    raw: dict
    line: int
    id: str = ""
    verdict: str = ""
    detail: str = ""
    evidence: str = ""
    problems: list[str] = field(default_factory=list)


class _NoDuplicateKeysLoader(yaml.SafeLoader):
    """`yaml.SafeLoader` that refuses a mapping with a repeated key.

    PyYAML's default is last-wins, silently. On 2026-08-24 a B-039 `verify:` +
    `verify-means:` pair was pasted into the B-040 block; both blocks parsed,
    both were unique by `id`, and the gate cheerfully ran B-039's TLA-jar check
    while reporting on B-040. The two happened to agree at the time, so nothing
    went red — the register would have started lying the moment they diverged.
    A duplicate key here is never intentional; make it BROKEN.
    """


def _no_duplicate_keys(loader, node, deep=False):
    seen = set()
    for key_node, _ in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise yaml.constructor.ConstructorError(
                None, None, f"duplicate key {key!r} in a backlog block", key_node.start_mark
            )
        seen.add(key)
    return yaml.SafeLoader.construct_mapping(loader, node, deep=deep)


_NoDuplicateKeysLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _no_duplicate_keys
)


def parse(text: str) -> list[Item]:
    items: list[Item] = []
    for m in BLOCK_RE.finditer(text):
        line = text[: m.start()].count("\n") + 1
        try:
            data = yaml.load(m.group(1), Loader=_NoDuplicateKeysLoader) or {}
        except yaml.YAMLError as e:
            items.append(Item(raw={}, line=line, id=f"<unparseable@{line}>", verdict=BROKEN,
                              detail=f"the backlog block is not valid YAML: {e}"))
            continue
        if not isinstance(data, dict):
            items.append(Item(raw={}, line=line, id=f"<malformed@{line}>", verdict=BROKEN,
                              detail="a backlog block must be a mapping of fields"))
            continue
        items.append(Item(raw=data, line=line, id=str(data.get("id", f"<no id@{line}>"))))
    return items


def validate_schema(item: Item) -> None:
    """Reject a malformed item outright rather than half-checking it.

    A silently skipped item is precisely the failure this gate exists to prevent,
    so a missing field is a hard failure, never a warning.
    """
    d = item.raw
    for f in REQUIRED_FIELDS:
        if f not in d or d[f] in (None, ""):
            item.problems.append(f"missing required field `{f}`")
    # YAML turns bare `true`, `yes`, `on`, `no`, `off` into booleans, so a verify
    # written without quotes silently stops being a command. Caught by this file's
    # own self-test on 2026-08-23; a real item could make the same mistake and would
    # otherwise report DRIFTED with a baffling "command not found".
    if "verify" in d and not isinstance(d["verify"], str):
        item.problems.append(
            f"verify must be a quoted string, got {type(d['verify']).__name__} "
            f"({d['verify']!r}) — YAML reads bare true/yes/on as booleans"
        )
    if d.get("status") not in VALID_STATUS and "status" in d:
        item.problems.append(f"status must be one of {VALID_STATUS}, got {d.get('status')!r}")
    if d.get("owner") not in VALID_OWNER and "owner" in d:
        item.problems.append(f"owner must be one of {VALID_OWNER}, got {d.get('owner')!r}")
    # `status` and `owner` were validated INDEPENDENTLY and never crossed (B-147).
    # `owner: owner` means "still needs the human" — a credential, a payment, a
    # deletion, a legal call. A finished item does not still need the human, so
    # `done` + `owner: owner` is a contradiction on its face, and the practical
    # cost is the owner's queue accumulating work nobody has to do any more (28 → 12
    # when it was last cleaned by hand, in #1510).
    # SCOPE, fixed here so the gate does not become folklore: `done` ONLY. `parked`
    # is deliberately EXCLUDED — an item parked precisely because it is waiting on
    # the owner is legitimate, and the strict reading (`!= open`) would forbid it.
    if d.get("status") == "done" and d.get("owner") == "owner":
        item.problems.append(
            "status: done with owner: owner — `owner: owner` means the item still needs "
            "the human, and a finished item does not. Set `owner: tl`, or reopen it."
        )
    if d.get("status") == "done" and has_explicit_open_verify_marker(d.get("verify-means")):
        item.problems.append(
            "status: done has an explicit open-polarity declaration in the first non-empty "
            "line of `verify-means`; invert the verify or reopen the item"
        )
    if "last-verified" in d:
        try:
            parse_date(d["last-verified"])
        except Exception:
            item.problems.append(f"last-verified must be YYYY-MM-DD, got {d.get('last-verified')!r}")


def parse_date(value) -> dt.date:
    if isinstance(value, dt.date):
        return value
    return dt.datetime.strptime(str(value), "%Y-%m-%d").date()


def has_explicit_open_verify_marker(value) -> bool:
    """Return whether ``verify-means`` explicitly declares an open polarity.

    Only an explicit first-line marker is actionable.  This rejects the known
    false-done shape without inventing a semantic parser for arbitrary prose.
    """
    if not isinstance(value, str):
        return False
    first = next((line.strip() for line in value.splitlines() if line.strip()), "")
    first = re.sub(r"[*`_]", "", first).strip()
    return bool(OPEN_VERIFY_MARKER_RE.match(first))


def age_days(item: Item, today: dt.date) -> int:
    return (today - parse_date(item.raw["last-verified"])).days


def check(
    item: Item,
    today: dt.date,
    max_age: int,
    *,
    trusted_by_id: dict[str, Item] | None = None,
    enforce_manual_age: bool = False,
) -> None:
    if item.verdict:  # already BROKEN at parse time
        return
    validate_schema(item)
    if item.problems:
        item.verdict = BROKEN
        item.detail = "; ".join(item.problems)
        return

    command = str(item.raw["verify"]).strip()
    if command == "manual":
        age = age_days(item, today)
        if enforce_manual_age and age > max_age:
            item.verdict = STALE
            item.detail = (f"last verified {age} days ago (limit {max_age}); "
                           "re-verify it by hand and update last-verified, or give it a real check")
        else:
            item.verdict = CONFIRMED
            item.detail = (
                f"manual, verified {age} day(s) ago"
                if enforce_manual_age
                else "manual declaration present; semantic freshness is deferred to exact-SHA CI"
            )
        return

    trusted = (trusted_by_id or {}).get(item.id)
    if trusted is not None and item.raw.get("verify") != trusted.raw.get("verify"):
        item.verdict = BROKEN
        item.detail = "verify declaration differs from BASE; semantic execution is deferred to exact-SHA CI"
        return
    item.verdict = CONFIRMED
    item.detail = "verify declaration is syntactically present; semantic execution is deferred to exact-SHA CI"


_CONTROL_PATH_RE = re.compile(r"(?<![A-Za-z0-9_.-])((?:scripts|tests)/[A-Za-z0-9_./-]+)")
_IMPORT_RE = re.compile(
    r"^(?:from\s+(scripts(?:\.[A-Za-z_][A-Za-z0-9_]*)*)\s+import\s+([A-Za-z_][A-Za-z0-9_]*)|"
    r"import\s+(scripts(?:\.[A-Za-z_][A-Za-z0-9_]*)*))",
    re.MULTILINE,
)


def _candidate_control_paths(trusted_root: Path, trusted_items: list[Item]) -> set[str]:
    """Resolve the trusted verifier's direct and transitive data controls.

    This is intentionally a static closure. It never imports or runs a
    candidate module, and it does not blanket-freeze unrelated source files.
    """
    paths = {"scripts/backlog_verify.py"}
    queue: list[str] = []
    for item in trusted_items:
        command = item.raw.get("verify")
        if isinstance(command, str):
            queue.extend(
                match.group(1).rstrip("'\"`),;:}")
                for match in _CONTROL_PATH_RE.finditer(command)
            )
    seen: set[str] = set()
    while queue:
        relative = queue.pop()
        if relative in seen or not relative.endswith((".py", ".sh")):
            continue
        seen.add(relative)
        paths.add(relative)
        if not relative.endswith(".py"):
            continue
        source = trusted_root / relative
        if not source.is_file() or source.is_symlink():
            continue
        text = source.read_text(encoding="utf-8")
        for match in _IMPORT_RE.finditer(text):
            module = match.group(1) or match.group(3)
            imported = match.group(2)
            if not module:
                continue
            candidate = module.replace(".", "/") + ".py"
            if (trusted_root / candidate).is_file():
                queue.append(candidate)
            elif imported:
                imported_candidate = f"{module.replace('.', '/')}/{imported}.py"
                if (trusted_root / imported_candidate).is_file():
                    queue.append(imported_candidate)
    return paths


def _regular_control(root: Path, relative: str) -> bytes:
    path = root / relative
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError as exc:
        raise RuntimeError(f"control path escapes checkout: {relative}") from exc
    try:
        current = root
        for component in Path(relative).parts[:-1]:
            current = current / component
            if current.is_symlink():
                raise RuntimeError(f"control parent is a symlink: {relative}")
        path.lstat()
    except OSError as exc:
        raise RuntimeError(f"control file unavailable: {relative}: {exc}") from exc
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"control file is not a regular non-symlink file: {relative}")
    return path.read_bytes()


def check_candidate_controls(candidate_root: Path, trusted_root: Path, trusted_items: list[Item]) -> None:
    """Fail closed when PR data changes a trusted control in the closure."""
    for relative in sorted(_candidate_control_paths(trusted_root, trusted_items)):
        trusted = _regular_control(trusted_root, relative)
        candidate = _regular_control(candidate_root, relative)
        if candidate != trusted:
            raise RuntimeError(
                f"candidate mutated trusted backlog control {relative}; "
                "candidate verifier code is data-only and was not executed"
            )


def validate_candidate_transitions(
    candidate_items: list[Item], trusted_items: list[Item], today: dt.date
) -> list[str]:
    """Validate the small, auditable set of BACKLOG changes a PR may make."""
    trusted_by_id = {item.id: item for item in trusted_items if item.raw}
    candidate_by_id = {item.id: item for item in candidate_items if item.raw}
    errors: list[str] = []
    allowed_status = {
        "open": {"open", "parked", "done"},
        "parked": {"parked", "open", "done"},
        "done": {"done"},
    }
    for item in candidate_items:
        if not item.raw or item.id not in trusted_by_id:
            if item.raw.get("status") != "open":
                errors.append(f"new item {item.id} must start status: open")
            continue
        old = trusted_by_id[item.id]
        old_raw, new_raw = old.raw, item.raw
        for field in IMMUTABLE_ITEM_FIELDS:
            if old_raw.get(field) != new_raw.get(field):
                errors.append(f"{item.id}: immutable field {field!r} changed")
        for field in set(old_raw) | set(new_raw):
            if field not in IMMUTABLE_ITEM_FIELDS | ALLOWED_TRANSITION_FIELDS:
                if old_raw.get(field) != new_raw.get(field):
                    errors.append(f"{item.id}: unsupported field {field!r} changed")
        old_status, new_status = old_raw.get("status"), new_raw.get("status")
        if new_status not in allowed_status.get(old_status, set()):
            errors.append(f"{item.id}: status transition {old_status!r} -> {new_status!r} is not allowed")
        if old_raw.get("owner") != new_raw.get("owner") and old_status == new_status:
            errors.append(f"{item.id}: owner may change only with a status transition")
        if old_raw.get("verify-means") != new_raw.get("verify-means") and old_status == new_status:
            errors.append(f"{item.id}: verify-means may change only with a status transition")
        try:
            old_date, new_date = parse_date(old_raw["last-verified"]), parse_date(new_raw["last-verified"])
            if new_date < old_date or new_date > today:
                errors.append(f"{item.id}: last-verified must move forward and not be future-dated")
        except Exception:
            pass  # validate_schema reports the precise date error
    missing = sorted(set(trusted_by_id) - set(candidate_by_id))
    if missing:
        errors.append("candidate deleted BASE item(s): " + ", ".join(missing))
    return errors


def validate_candidate_workflow(candidate_root: Path) -> None:
    """Inspect workflow policy as data; never execute the candidate workflow."""
    text = _regular_control(candidate_root, ".github/workflows/backlog-verify.yml").decode("utf-8")
    try:
        document = yaml.load(text, Loader=_NoDuplicateKeysLoader) or {}
    except yaml.YAMLError as exc:
        raise RuntimeError(f"candidate workflow policy is not valid YAML: {exc}") from exc
    if not isinstance(document, dict):
        raise RuntimeError("candidate workflow policy must be a YAML mapping")
    required = (
        "pull_request_target:",
        "github.event.pull_request.head.sha || github.sha",
        "github.event.pull_request.base.sha || github.sha",
        "types: [opened, synchronize, reopened]",
        "persist-credentials: false",
        'paths: ["**"]',
        "runs-on: corelink",
        "permissions:\n  contents: read",
        "--candidate-file",
        "--trusted-file",
    )
    missing = [fragment for fragment in required if fragment not in text]
    if missing:
        raise RuntimeError("candidate workflow policy missing: " + ", ".join(missing))
    forbidden = (
        "pull_request:\n", "refs/pull/", "pull_request.head.ref", "pull_request.base.ref",
        "GH_" + "TOKEN", "github." + "token", "shell" + "=" + "True", "bash" + " " + "-c",
    )
    found = [fragment for fragment in forbidden if fragment in text]
    if found:
        raise RuntimeError("candidate workflow contains mutable/credentialed execution policy: " + ", ".join(found))

    # The workflow is not executed for this PR, but it becomes the BASE control
    # plane after merge. Keep its executable shape closed while allowing harmless
    # comments/concurrency edits. A candidate cannot add a shell step, swap the
    # checkout action, or redirect the BASE checker without this data check going
    # red first.
    trigger = document.get(True, document.get("on")) or {}
    if trigger.get("pull_request_target", {}).get("types") != ["opened", "synchronize", "reopened"]:
        raise RuntimeError("candidate workflow policy has unexpected pull_request_target types")
    if trigger.get("pull_request_target", {}).get("paths") != ["**"]:
        raise RuntimeError("candidate workflow policy must cover the complete PR tree")
    if trigger.get("push", {}).get("branches") != ["main"] or trigger.get("push", {}).get("paths") != ["**"]:
        raise RuntimeError("candidate workflow policy has unexpected main push trigger")
    if document.get("permissions") != {"contents": "read"}:
        raise RuntimeError("candidate workflow policy must grant contents: read only")
    jobs = document.get("jobs")
    if not isinstance(jobs, dict) or set(jobs) != {"verify"}:
        raise RuntimeError("candidate workflow policy must contain only the verify job")
    job = jobs["verify"]
    steps = job.get("steps") if isinstance(job, dict) else None
    if job.get("runs-on") != "corelink" or not isinstance(steps, list) or len(steps) != 4:
        raise RuntimeError("candidate workflow policy has unexpected verify job shape")
    checkout_ref = "9f698171ed81b15d1823a05fc7211befd50c8ae0"
    expected_checkouts = (
        ("${{ github.event.pull_request.head.sha || github.sha }}", "_candidate"),
        ("${{ github.event.pull_request.base.sha || github.sha }}", "_base"),
    )
    for step, (ref, path) in zip(steps[:2], expected_checkouts):
        if not isinstance(step, dict) or step.get("uses") != f"actions/checkout@{checkout_ref}":
            raise RuntimeError("candidate workflow policy uses an unexpected checkout action")
        if step.get("with") != {
            "ref": ref, "path": path, "fetch-depth": 1, "persist-credentials": False
        }:
            raise RuntimeError("candidate workflow policy has an unsafe checkout configuration")
    expected_gate = (
        'python3 scripts/backlog_verify.py --candidate-file "$CANDIDATE_ROOT/BACKLOG.md" '
        '--trusted-file "$TRUSTED_ROOT/BACKLOG.md" --candidate-root "$CANDIDATE_ROOT" '
        '--trusted-root "$TRUSTED_ROOT"'
    )
    if steps[2].get("working-directory") != "_base" or steps[2].get("run") != expected_gate:
        raise RuntimeError("candidate workflow policy has an unexpected BASE gate command")
    if steps[3].get("working-directory") != "_base" or steps[3].get("run") != (
        "python3 -m unittest -q tests/test_backlog_verify_trust_boundary.py"
    ):
        raise RuntimeError("candidate workflow policy has an unexpected BASE test command")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--id", help="check a single item")
    ap.add_argument("--format", choices=("text", "json"), default="text")
    ap.add_argument("--max-age-days", type=int, default=DEFAULT_MAX_AGE_DAYS,
                    help="how long a `verify: manual` item may go unchecked")
    ap.add_argument("--today", help="override today's date (YYYY-MM-DD), for testing")
    ap.add_argument("--file", help="check a different backlog file (used by the self-test)")
    ap.add_argument("--candidate-file", help="PR BACKLOG.md; parsed as data only")
    ap.add_argument("--trusted-file", help="trusted-base BACKLOG.md paired with --candidate-file")
    ap.add_argument("--candidate-root", help="PR checkout root paired with --candidate-file")
    ap.add_argument("--trusted-root", help="trusted checkout root paired with --candidate-file")
    args = ap.parse_args()

    candidate_mode = bool(args.candidate_file)
    if bool(args.candidate_file) != bool(args.trusted_file):
        print("FATAL: --candidate-file and --trusted-file must be supplied together", file=sys.stderr)
        return 2
    if candidate_mode and (not args.candidate_root or not args.trusted_root):
        print("FATAL: candidate mode requires --candidate-root and --trusted-root", file=sys.stderr)
        return 2

    candidate_root = Path(args.candidate_root).resolve() if candidate_mode else None
    path = Path(args.candidate_file).resolve() if candidate_mode else Path(args.file).resolve() if args.file else BACKLOG_PATH
    if candidate_mode and (path != candidate_root / "BACKLOG.md" or not path.is_file() or path.is_symlink()):
        print("FATAL: candidate BACKLOG.md must be a regular file in the candidate root", file=sys.stderr)
        return 2
    if not path.exists():
        print(f"FATAL: {path} does not exist", file=sys.stderr)
        return 2

    today = dt.datetime.strptime(args.today, "%Y-%m-%d").date() if args.today else dt.date.today()
    try:
        if path.stat().st_size > MAX_BACKLOG_BYTES:
            print(f"FATAL: {path.name} exceeds {MAX_BACKLOG_BYTES} bytes", file=sys.stderr)
            return 2
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        print(f"FATAL: cannot read {path}: {exc}", file=sys.stderr)
        return 2
    items = parse(text)
    trusted_by_id: dict[str, Item] | None = None
    if candidate_mode:
        trusted_path = Path(args.trusted_file).resolve()
        if not trusted_path.is_file() or trusted_path.is_symlink():
            print(f"FATAL: trusted backlog is not a regular file: {trusted_path}", file=sys.stderr)
            return 2
        if trusted_path.stat().st_size > MAX_BACKLOG_BYTES:
            print(f"FATAL: trusted BACKLOG.md exceeds {MAX_BACKLOG_BYTES} bytes", file=sys.stderr)
            return 2
        trusted_items = parse(trusted_path.read_text(encoding="utf-8"))
        trusted_by_id = {item.id: item for item in trusted_items if item.raw}
        if len(trusted_by_id) != len(trusted_items) or any(item.verdict == BROKEN for item in trusted_items):
            print("FATAL: trusted-base BACKLOG.md is not parseable; refusing candidate validation", file=sys.stderr)
            return 2
        try:
            check_candidate_controls(
                candidate_root,
                Path(args.trusted_root).resolve(),
                trusted_items,
            )
            transition_errors = validate_candidate_transitions(items, trusted_items, today)
            if transition_errors:
                raise RuntimeError("candidate transition rejected:\n" + "\n".join(transition_errors))
            validate_candidate_workflow(candidate_root)
        except (OSError, RuntimeError) as exc:
            print(f"FATAL: {exc}", file=sys.stderr)
            return 2

    if not items:
        # An empty backlog is not a pass. It is far more likely that the format
        # broke, or the file was truncated, than that there is genuinely no work.
        print(f"FATAL: {path.name} contains no parseable ```backlog blocks", file=sys.stderr)
        return 2

    # Each block must sit under a heading that names the SAME id. Checked before
    # the per-item verifies so a mislabelled item cannot be "confirmed" under a
    # heading that describes different work.
    headings = [(m.start(), m.group(1)) for m in HEADING_RE.finditer(text)]
    mismatches: list[str] = []
    # An id that is not `B-<digits>` used to be SKIPPED here, and that silence was
    # the gate failing OPEN (B-143): `id: B-UNALLOCATED`, `B-131a`, `b-131` and
    # `B-TBD` all merged CONFIRMED. The heading check skipped them, and the density
    # check below only collects ids matching `B-(\d+)` — so a malformed id opens no
    # gap, collides with nothing, and is counted by nothing. The two checks that DO
    # fail loudly (missing id = gap, duplicate id) both presuppose a well-formed id.
    # The rule must be the GENERAL form, never a match on the literal placeholder:
    # a gate that greps for `B-UNALLOCATED` is decorative.
    malformed_ids: list[str] = []
    non_positive_ids: list[str] = []
    for m in BLOCK_RE.finditer(text):
        line = text[: m.start()].count("\n") + 1
        block_id = ""
        try:
            data = yaml.load(m.group(1), Loader=_NoDuplicateKeysLoader) or {}
            if isinstance(data, dict):
                block_id = str(data.get("id", ""))
        except yaml.YAMLError:
            continue  # already reported as BROKEN by parse()
        # B-167 — THE CANONICAL FORM, and the choice the item required be made.
        #
        # `^B-\d+$` was necessary and NOT sufficient. `B-0142` satisfies it and
        # coexists with the real `B-142`: density does `int("0142") == 142` so the
        # sequence stays dense, the duplicate check compares STRINGS so nothing
        # collides, and a `### B-0142` heading satisfies the heading/block check.
        # Two items every human reads as one number, both CONFIRMED, and every
        # `[B-142]` citation from outside resolving to whichever one it hits.
        #
        # Of the three rules B-167 enumerated, this is the THIRD: the spelling must
        # equal `f"B-{int(n):03d}"`.
        #   - NOT `^B-\d{3}$`: that closes the hole exactly for today's ids and
        #     forbids the day there is a `B-1000`.
        #   - NOT normalising only the duplicate key: that accepts the spelling and
        #     rejects only the collision, so `B-0500` would still merge as a lone
        #     item and read as a different number than it is.
        # This one canonicalises WITHOUT freezing the width — `B-1000` round-trips
        # (`f"B-{1000:03d}" == "B-1000"`), and all 167 ids today already satisfy it.
        canonical = ""
        if (mm := re.fullmatch(r"B-(\d+)", block_id)):
            number = int(mm.group(1))
            if number <= 0:
                non_positive_ids.append(
                    f"  line {line}: block `id: {block_id}` is non-positive — write it as `B-001` or greater"
                )
                continue
            canonical = f"B-{number:03d}"
        if not canonical:
            malformed_ids.append(
                f"  line {line}: block `id: {block_id or '<missing>'}` is not `B-<digits>`"
            )
            continue
        if block_id != canonical:
            malformed_ids.append(
                f"  line {line}: block `id: {block_id}` is not canonical — write it as "
                f"`{canonical}`. A zero-padded variant aliases the real item silently: "
                f"density satisfies int(), and the duplicate check compares strings"
            )
            continue
        prior = [h for pos, h in headings if pos < m.start()]
        if not prior:
            mismatches.append(f"  line {line}: block `id: {block_id}` has no `### B-…` heading above it")
        elif prior[-1] != block_id:
            mismatches.append(f"  line {line}: heading says {prior[-1]}, block says id: {block_id}")
    # The loop above walks BLOCKS and finds their heading. It is therefore blind
    # to the reverse failure: a heading whose block LOST ITS FENCE — a conflict
    # resolution that ate the ```backlog line, or the `id:` inside it. That item
    # simply stops existing for every check in this file, and the gate reports
    # all-green over the survivors. Observed 2026-08-31: 129 headings, 128 parsed
    # blocks, `confirmed=128, drifted=0, broken=0`. The item would have merged
    # green and vanished from the register.
    #
    # An item that is missing is indistinguishable from an item that is malformed
    # unless something compares the two populations. This does that.
    #
    # SCOPE, stated because the prose is what survives: the density rule above
    # already catches an item lost from the MIDDLE of the file (it leaves a gap).
    # The blind window is the item with the MAXIMUM id — the only one whose loss
    # leaves the sequence dense. That is the freshly added item, which is exactly
    # the one most likely to be born from a conflict resolution.
    # POSITION, not parseability. A block whose YAML is unparseable IS a block —
    # `parse()` already reports it as BROKEN (exit 1), and treating it as absent
    # here would upgrade every malformed-YAML case to a FATAL exit 2 and swallow
    # the more precise diagnosis. Caught by this repo's own gate-for-gates suite:
    # `FAIL  unparseable YAML is BROKEN (exit 2, want 1)`.
    #
    # So the question is only: does a fence open between this heading and the
    # next one? A heading with no fence at all is the item that vanishes.
    #
    # This check — and ONLY this check — reads headings from a fence-MASKED copy.
    # `### B-NN` inside a fenced block is an EXAMPLE, not an item: documenting the
    # item format inside BACKLOG.md itself would otherwise be flagged as an item
    # whose block is missing, naming an id that does not exist.
    #
    # The mask must NOT feed the divergence loop above. Fence pairing is
    # SEQUENTIAL from the top of the file, so the very defect this check hunts —
    # a lost fence opener — leaves an odd count and shifts EVERY later pairing by
    # one. Each subsequent `### B-NN` then falls inside a region believed to be
    # fenced and drops out of `headings`, and the divergence loop attributes every
    # later block to the last surviving heading. Measured on the real BACKLOG.md
    # (129 items) with B-062's opener removed: masked headings produced 64 spurious
    # `heading says B-062, block says id: B-0NN` lines and returned before the
    # density rule could run; unmasked headings give `FATAL: BACKLOG.md is missing
    # B-062 — ids must be dense.`, one exact line, as `main` did. Losing the LAST
    # item's fence — the case this check exists for — misaligns no block from its
    # heading, so the divergence loop stays silent and the orphan check is reached
    # intact either way. Masked here, unmasked there.
    #
    # SCOPE of the mask, not fixed here because it is unreachable today: `^```
    # only sees a fence in COLUMN 0. Measured 2026-08-31, BACKLOG.md carries 0
    # indented (`^\s+``` `) fences and 0 `~~~` fences — every fence in it opens in
    # column 0, so the mask pairs all of them. A `### B-NN` written in column 0
    # *inside* an indented fence, or a `~~~` fence, would still register as a
    # phantom heading here. Neither construct exists in the file; if one is ever
    # added, this mask needs a real fence tokenizer rather than a regex.
    masked = re.sub(
        r"^```.*?^```",
        lambda m: re.sub(r"[^\n]", " ", m.group(0)),
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    masked_headings = [(m.start(), m.group(1)) for m in HEADING_RE.finditer(masked)]
    block_starts = [m.start() for m in BLOCK_RE.finditer(text)]
    bounds = [pos for pos, _ in masked_headings] + [len(text)]
    orphan_headings = [
        h
        for i, (pos, h) in enumerate(masked_headings)
        if not any(pos < b < bounds[i + 1] for b in block_starts)
    ]
    # Named separately from `mismatches` on purpose. Both exit 2, but the FATAL
    # below says "a heading and its block disagree", which is FALSE for a
    # malformed id — the heading may agree perfectly. A gate whose own message
    # misdescribes what it caught is the prose that lies first.
    if malformed_ids:
        print(
            "FATAL: backlog block(s) with an id that is not `B-<digits>`.\n"
            + "\n".join(malformed_ids)
            + "\nA placeholder or malformed id is invisible to BOTH loud checks: it opens\n"
            "no gap in the density rule and collides with nothing, so it merges in\n"
            "silence. Allocate the real id before merging.\n"
            "A non-canonical id is rejected before density or duplicate checks, so it cannot\n"
            "silently alias another item. Allocate the canonical positive id before merging.",
            file=sys.stderr,
        )
        return 2

    if non_positive_ids:
        print(
            "FATAL: backlog block(s) with a non-positive id.\n"
            + "\n".join(non_positive_ids)
            + "\nIds are positive integers; B-000 and other zero forms are not allocatable.",
            file=sys.stderr,
        )
        return 2

    if mismatches:
        print(
            "FATAL: a heading and its block disagree about which item they are.\n"
            + "\n".join(mismatches)
            + "\nEvery reference from outside this file cites the HEADING; the gate reads\n"
            "the block. When they diverge the register points the wrong way and nothing\n"
            "notices, because both ids can still be unique.",
            file=sys.stderr,
        )
        return 2

    # A deleted item is invisible to per-item checks — every survivor still passes
    # while the record silently loses work. This happened on 2026-08-23: an edit
    # that rewrote one item removed its neighbour, and the gate reported all-green.
    # So ids must stay DENSE. Retiring an item means marking it, never deleting it.
    if orphan_headings:
        print(
            "FATAL: heading(s) sem bloco ```backlog parseavel: "
            + ", ".join(sorted(set(orphan_headings)))
            + "\nO item existe como titulo e NAO existe para nenhuma verificacao deste\n"
            "arquivo — some do portao sem que o portao reclame, porque ele conta o que\n"
            "consegue parsear e nada compara esse numero com quantos titulos ha.\n"
            f"(titulos: {len(masked_headings)}, blocos abertos: {len(block_starts)})",
            file=sys.stderr,
        )
        return 2

    numbered = sorted(
        int(m.group(1))
        for i in items
        if (m := re.fullmatch(r"B-(\d+)", i.id))
    )
    if numbered:
        missing = sorted(set(range(1, max(numbered) + 1)) - set(numbered))
        if missing:
            gaps = ", ".join(f"B-{n:03d}" for n in missing)
            print(
                f"FATAL: BACKLOG.md is missing {gaps} — ids must be dense. An item was "
                "deleted rather than resolved. Restore it, or mark it retired in place.",
                file=sys.stderr,
            )
            return 2

    seen: dict[str, int] = {}
    for it in items:
        if it.id in seen:
            it.verdict = BROKEN
            it.detail = f"duplicate id — also declared at line {seen[it.id]}"
        else:
            seen[it.id] = it.line

    selected = [i for i in items if not args.id or i.id == args.id]
    if args.id and not selected:
        print(f"FATAL: no backlog item with id {args.id}", file=sys.stderr)
        return 2

    for it in selected:
        check(
            it,
            today,
            args.max_age_days,
            trusted_by_id=trusted_by_id,
            enforce_manual_age=bool(args.file),
        )

    if args.format == "json":
        print(json.dumps([{
            "id": i.id, "line": i.line, "status": i.raw.get("status"),
            "owner": i.raw.get("owner"), "repo": i.raw.get("repo"),
            "verdict": i.verdict, "detail": i.detail, "evidence": i.evidence,
        } for i in selected], indent=2))
    else:
        width = max((len(i.id) for i in selected), default=8)
        for i in selected:
            print(f"  {i.verdict:9} {i.id:{width}}  {i.raw.get('status','?'):6} {i.detail}")
            if i.verdict in (DRIFTED, BROKEN) and i.evidence:
                print(f"  {'':9} {'':{width}}  └ {i.evidence}")
        counts = {v: sum(1 for i in selected if i.verdict == v) for v in (CONFIRMED, DRIFTED, STALE, BROKEN)}
        print(f"\n  {len(selected)} item(s): " + ", ".join(f"{v.lower()}={n}" for v, n in counts.items()))

    bad = [i for i in selected if i.verdict != CONFIRMED]
    if bad:
        print(f"\nBACKLOG.md disagrees with the repo on {len(bad)} item(s). "
              "Fix the item or fix the world — do not delete the check.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

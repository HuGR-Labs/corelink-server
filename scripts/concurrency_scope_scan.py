#!/usr/bin/env python3
"""Find top-level workflow concurrency groups that can cancel another ref.

``concurrency.group`` is scoped per repository, not per workflow file. A group
that is constant across refs plus ``cancel-in-progress: true`` lets a new run on
one ref cancel an in-flight run on another. That failure is absent rather than
red: a cancelled required check has no failure result to inspect.

This scanner asks a narrow question: can this top-level group be proved safe
from *cross-ref* cancellation? A group is safe only when it carries one of:

* ``github.ref`` (the complete Git ref),
* ``github.run_id`` (unique per run; this also disables supersession), or
* ``github.event.pull_request.number`` in a workflow triggered exclusively by
  ``pull_request`` and/or ``pull_request_target``.

Everything else is unsafe until proven otherwise. ``github.workflow`` is a name,
``matrix.*`` is normally shared by refs, ``github.sha`` can be shared by refs,
and ``github.head_ref`` is empty outside pull-request events. Treating any
interpolation as a ref discriminator was a false green; treating a matrix axis
as one would be the same false green in a different costume.

This is not the event-collision scanner. ``github.ref`` separates PR refs from
one another, but does not separate ``push``, ``schedule``, and manual dispatch
on ``main``; that broader issue is B-150. The live constant-group findings are
tracked by B-172. The exit code never claims either item is fixed.

Usage:
    python3 scripts/concurrency_scope_scan.py [dir]  # default .github/workflows
    python3 scripts/concurrency_scope_scan.py --self-test

Exit 0 = no cross-ref violations, 1 = violations or unreadable input.
"""

from __future__ import annotations

import glob
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import yaml


TOP_LEVEL_CONCURRENCY = re.compile(r"^concurrency\s*:\s*(?:#.*)?$")
TOP_LEVEL_CONCURRENCY_KEY = re.compile(r"^concurrency\s*:")
GROUP_KEY = re.compile(r"\s*group\s*:")
CANCEL_KEY = re.compile(r"\s*cancel-in-progress\s*:")
REF_DISCRIMINATOR = re.compile(r"(?<![\w.])github\.ref(?![\w.])")
RUN_UNIQUE_DISCRIMINATOR = re.compile(r"(?<![\w.])github\.run_id(?![\w.])")
PR_NUMBER_DISCRIMINATOR = re.compile(
    r"(?<![\w.])github\.event\.pull_request\.number(?![\w.])"
)
# These are the two established, mechanically auditable fallback forms.  They
# must stay exact: accepting a merely similar expression would reinstate the
# conditional-collapse false green this scanner exists to prevent.
PR_NUMBER_OR_REF = re.compile(
    r"github\.event\.pull_request\.number\s*\|\|\s*github\.ref"
)
ISSUE_OR_PR_NUMBER = re.compile(
    r"github\.event\.issue\.number\s*\|\|\s*github\.event\.pull_request\.number"
)
PR_ONLY_EVENTS = frozenset({"pull_request", "pull_request_target"})
ISSUE_OR_PR_EVENTS = frozenset({"issues", "pull_request", "pull_request_target"})


@dataclass(frozen=True)
class Group:
    line: str
    value: str


def strip_comment(value: str) -> str:
    """Drop a YAML comment while preserving quoted ``#`` and escaped quotes.

    YAML starts a comment only when ``#`` is outside a quote and follows
    whitespace. Treating every hash as a comment would turn ``true#suffix``
    into true and could manufacture a clean scan from an unknown scalar.
    """
    quote = None
    escaped = False
    for index, char in enumerate(value):
        if quote is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
        elif char in "\"'":
            quote = char
        elif char == "#" and (index == 0 or value[index - 1].isspace()):
            return value[:index].rstrip()
    return value.rstrip()


def parse_bool(raw: str) -> bool:
    """Read Actions' boolean-ish flag, failing closed for an unknown scalar."""
    raw = strip_comment(raw).strip()
    if "${{" in raw:
        return True
    try:
        value = yaml.safe_load(raw)
    except yaml.YAMLError as error:
        raise ValueError(f"cannot parse cancel-in-progress {raw!r}") from error
    if isinstance(value, bool):
        return value
    if value is None or value == "":
        return False
    if isinstance(value, str):
        word = value.strip().lower()
        if word in ("true", "yes", "on", "1"):
            return True
        if word in ("false", "no", "off", "0"):
            return False
    raise ValueError(
        f"cancel-in-progress is {raw!r}, neither a boolean nor an expression; "
        "refusing to guess"
    )


def expressions(value: str) -> tuple[str, ...] | None:
    """Return complete Actions expressions, or ``None`` for malformed input.

    This deliberately does not try to evaluate the Actions language. A scanner
    that cannot prove what an expression emits must leave its group unsafe.
    ``None`` therefore represents both malformed interpolation and a value with
    no expression; neither can establish a cross-ref discriminator.
    """
    found: list[str] = []
    start = 0
    while True:
        opening = value.find("${{", start)
        if opening < 0:
            return tuple(found) if found else None
        closing = value.find("}}", opening + 3)
        if closing < 0:
            # An unfinished expression cannot prove safety. `parse_bool` already
            # treats its analogous uncertainty as cancelling.
            return None
        found.append(value[opening + 3 : closing])
        start = closing + 2


def has_unconditional_discriminator(
    group_value: str, discriminator: re.Pattern[str]
) -> bool:
    """Whether a group includes a discriminator as a direct expression result.

    Mentioning a context is not enough. The Actions idiom ``A && B || C`` is a
    conditional, and can turn a discriminator into one constant fallback. For
    example, ``${{ false && github.ref || 'global' }}`` and
    ``${{ github.ref == 'refs/heads/main' && 'main' || 'other' }}`` both used to
    pass the scanner despite creating repo-global groups. We accept only a
    complete expression whose entire result is the relevant context field. This
    is intentionally narrow: functions, operators, property comparisons, and
    future expression syntax stay unsafe until their isolation is proved.
    """
    parsed = expressions(group_value)
    return bool(
        parsed
        and any(discriminator.fullmatch(expression.strip()) for expression in parsed)
    )


def has_exact_fallback(group_value: str, safe_form: re.Pattern[str]) -> bool:
    """Whether a group carries one deliberately supported fallback form.

    The fallback is accepted only when the *entire* expression is the audited
    two-operand form. In particular, a false guard, comparison, function call,
    or a third operand cannot accidentally inherit the approval.
    """
    parsed = expressions(group_value)
    return bool(
        parsed and any(safe_form.fullmatch(expression.strip()) for expression in parsed)
    )


def workflow_events(path: str) -> frozenset[str]:
    """Return trigger names, or fail rather than silently omit malformed YAML."""
    try:
        with open(path, encoding="utf-8") as source:
            document = yaml.safe_load(source)
    except (OSError, yaml.YAMLError) as error:
        raise ValueError(f"cannot parse workflow {path}: {error}") from error
    if not isinstance(document, dict):
        raise ValueError(f"workflow {path} is not a YAML mapping")
    # PyYAML 1.1 resolves the unquoted Actions key `on` as boolean True.
    triggers = document.get(True, document.get("on"))
    if isinstance(triggers, dict):
        return frozenset(str(key) for key in triggers)
    if isinstance(triggers, list):
        return frozenset(str(value) for value in triggers)
    if isinstance(triggers, str):
        return frozenset({triggers})
    raise ValueError(f"workflow {path} has no readable top-level on: trigger")


def top_level_groups(path: str) -> list[tuple[Group, str]]:
    """Read only a top-level concurrency block; comments cannot impersonate keys."""
    try:
        with open(path, encoding="utf-8") as source:
            lines = source.read().splitlines()
    except OSError as error:
        raise ValueError(f"cannot read workflow {path}: {error}") from error

    found: list[tuple[Group, str]] = []
    for index, line in enumerate(lines):
        if not TOP_LEVEL_CONCURRENCY_KEY.match(line):
            continue
        if not TOP_LEVEL_CONCURRENCY.match(line):
            raise ValueError(
                f"{path}:{index + 1}: unsupported inline concurrency value; "
                "use a block mapping so this scanner cannot skip it"
            )
        block: list[str] = []
        for nested in lines[index + 1 :]:
            if nested and not nested[0].isspace():
                break
            block.append(nested)
        keys = [nested for nested in block if not nested.lstrip().startswith("#")]
        groups = [
            nested
            for nested in keys
            if GROUP_KEY.match(strip_comment(nested))
        ]
        if len(groups) > 1:
            raise ValueError(
                f"{path}:{index + 1}: duplicate group keys in concurrency block; "
                "refusing first-wins interpretation"
            )
        if not groups:
            continue
        cancels = [
            nested
            for nested in keys
            if CANCEL_KEY.match(strip_comment(nested))
        ]
        if len(cancels) > 1:
            raise ValueError(
                f"{path}:{index + 1}: duplicate cancel-in-progress keys in concurrency block; "
                "refusing first-wins interpretation"
            )
        group_line = groups[0]
        group_value = strip_comment(group_line).split(":", 1)[1].strip()
        cancel_value = cancels[0].split(":", 1)[1].strip() if cancels else "false"
        found.append((Group(group_line.strip(), group_value), cancel_value))
    return found


def is_cross_ref_safe(group_value: str, events: frozenset[str]) -> bool:
    """Accept only discriminators whose safety follows from their semantics."""
    if has_unconditional_discriminator(group_value, REF_DISCRIMINATOR):
        return True
    if has_unconditional_discriminator(group_value, RUN_UNIQUE_DISCRIMINATOR):
        return True
    if has_unconditional_discriminator(
        group_value, PR_NUMBER_DISCRIMINATOR
    ) and events <= PR_ONLY_EVENTS:
        return True
    # This established Actions normal form yields a PR number for PR runs and
    # the full ref otherwise. It is cross-ref unique, but event collisions on a
    # shared ref remain intentionally out of this scanner's B-150 scope.
    if has_exact_fallback(group_value, PR_NUMBER_OR_REF):
        return True
    # `welcome-first-pr` legitimately handles both issues and pull requests. A
    # number from only one payload would be empty for the other event and hence
    # unsafe; the exact pair covers every event it admits.
    return bool(
        events <= ISSUE_OR_PR_EVENTS
        and has_exact_fallback(group_value, ISSUE_OR_PR_NUMBER)
    )


def scan(directory: str) -> tuple[list[tuple[str, str, str]], int, int]:
    """Return (violations, blocks, safe_blocks), failing closed on unreadable input."""
    files = sorted(
        set(
            glob.glob(os.path.join(directory, "*.yml"))
            + glob.glob(os.path.join(directory, "*.yaml"))
        )
    )
    if not files:
        raise ValueError(f"no workflow files found under {directory!r}")

    violations: list[tuple[str, str, str]] = []
    total = safe = 0
    for path in files:
        events = workflow_events(path)
        for group, cancel_value in top_level_groups(path):
            total += 1
            try:
                cancels = parse_bool(cancel_value)
            except ValueError as error:
                raise ValueError(f"{path}: {error}") from error
            if is_cross_ref_safe(group.value, events):
                safe += 1
            elif cancels:
                violations.append((os.path.basename(path), group.line[:70], cancel_value))
    return violations, total, safe


def write_workflow(
    directory: Path,
    name: str,
    group: str,
    cancel: str,
    triggers: str = "  push:\n",
    concurrency_header: str = "concurrency:",
) -> Path:
    path = directory / name
    path.write_text(
        f"on:\n{triggers}{concurrency_header}\n  group: {group}\n"
        f"  cancel-in-progress: {cancel}\n"
        "jobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: 'true'\n",
        encoding="utf-8",
    )
    return path


def require_actionlint_clean(paths: list[Path]) -> None:
    """Keep adversarial fixtures valid GitHub Actions, not parser inventions."""
    actionlint = shutil.which("actionlint")
    if actionlint is None:
        raise ValueError("actionlint is required for --self-test but is not on PATH")
    result = subprocess.run(
        [actionlint, "-no-color", *(str(path) for path in paths)],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        details = (result.stdout + result.stderr).strip()
        raise ValueError(f"actionlint rejected a supposedly valid fixture: {details}")


def require_actionlint_rejected(paths: list[Path]) -> None:
    """Prove every declared lint exception remains a deliberate bad fixture.

    The scanner must handle invalid workflow text without treating it as a clean
    result, but an invalid fixture must never be silently left out of the
    actionlint census. Keep this separate from ``require_actionlint_clean`` so
    adding a fixture forces an explicit decision: lintable, or rejected for a
    named and tested reason.
    """
    actionlint = shutil.which("actionlint")
    if actionlint is None:
        raise ValueError("actionlint is required for --self-test but is not on PATH")
    for path in paths:
        result = subprocess.run(
            [actionlint, "-no-color", str(path)],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode == 0:
            raise ValueError(
                f"actionlint unexpectedly accepted declared-invalid fixture {path.name}; "
                "move it to the lintable census"
            )


def self_test() -> int:
    """Prove the scanner can be red, including its former false-green shapes."""
    temporary = Path(tempfile.mkdtemp(prefix="concurrency-scope-scan-"))
    try:
        actionlint_clean = [
            write_workflow(temporary, "constant.yml", "literal", "true"),
            write_workflow(temporary, "workflow-name.yml", "${{ github.workflow }}", "true"),
        ]
        # Top-level `concurrency` cannot use `matrix` in Actions. We keep this
        # scanner-negative malformed fixture, but require actionlint to reject
        # it explicitly instead of silently omitting it from fixture coverage.
        actionlint_rejected = [
            write_workflow(temporary, "matrix.yml", "ci-${{ matrix.os }}", "true")
        ]
        actionlint_clean.extend([
            write_workflow(temporary, "sha.yml", "ci-${{ github.sha }}", "true"),
            write_workflow(temporary, "trailing-comment.yml", "literal", "true  # not false"),
            write_workflow(temporary, "ref.yml", "ci-${{ github.ref }}", "true"),
            write_workflow(temporary, "run.yml", "ci-${{ github.run_id }}", "true"),
            write_workflow(
                temporary,
                "pr-number.yml",
                "ci-${{ github.event.pull_request.number }}",
                "true",
                "  pull_request:\n",
            ),
            write_workflow(
                temporary,
                "pr-number-on-push.yml",
                "ci-${{ github.event.pull_request.number }}",
                "true",
            ),
            write_workflow(
                temporary,
                "pr-number-or-ref.yml",
                "ci-${{ github.event.pull_request.number || github.ref }}",
                "true",
            ),
            write_workflow(
                temporary,
                "issue-or-pr.yml",
                "ci-${{ github.event.issue.number || github.event.pull_request.number }}",
                "true",
                "  issues:\n  pull_request:\n",
            ),
            write_workflow(temporary, "literal-ref.yml", "ci-github.ref", "true"),
            write_workflow(temporary, "quoted-ref.yml", "${{ 'github.ref' }}", "true"),
            write_workflow(
                temporary,
                "commented-header.yml",
                "ci-${{ github.ref }}",
                "true",
                concurrency_header="concurrency: # a YAML comment is still this mapping",
            ),
            # A discriminator hidden behind a conditional is not a
            # discriminator. These are valid Actions expressions that all
            # collapse to a shared group, and are regression/mutation fixtures
            # for the former "mentions context" implementation.
            write_workflow(
                temporary,
                "false-ref-guard.yml",
                "ci-${{ false && github.ref || 'global' }}",
                "true",
            ),
            write_workflow(
                temporary,
                "ref-comparison.yml",
                "ci-${{ github.ref == 'refs/heads/main' && 'main' || 'other' }}",
                "true",
            ),
            write_workflow(
                temporary,
                "false-pr-number-guard.yml",
                "ci-${{ false && github.event.pull_request.number || 'global' }}",
                "true",
                "  pull_request:\n",
            ),
            write_workflow(
                temporary,
                "false-run-id-guard.yml",
                "ci-${{ false && github.run_id || 'global' }}",
                "true",
            ),
            # These are deliberately close to the two audited fallbacks. A
            # fullmatch-to-search mutation would classify them as safe; none
            # may become a new allowlist entry without a separate proof.
            write_workflow(
                temporary,
                "pr-number-or-ref-third-fallback.yml",
                "ci-${{ github.event.pull_request.number || github.ref || 'global' }}",
                "true",
            ),
            write_workflow(
                temporary,
                "false-guarded-pr-number-or-ref.yml",
                "ci-${{ false && (github.event.pull_request.number || github.ref) || 'global' }}",
                "true",
            ),
            write_workflow(
                temporary,
                "issue-or-pr-third-fallback.yml",
                "ci-${{ github.event.issue.number || github.event.pull_request.number || 'global' }}",
                "true",
                "  issues:\n  pull_request:\n",
            ),
            write_workflow(
                temporary,
                "false-guarded-issue-or-pr.yml",
                "ci-${{ false && (github.event.issue.number || github.event.pull_request.number) || 'global' }}",
                "true",
                "  issues:\n  pull_request:\n",
            ),
        ])
        # The census is load-bearing. The four critical false-guard/comparison
        # fixtures are part of `actionlint_clean`; matrix is the sole,
        # deliberately tested invalid exception.
        if len(actionlint_clean) != 21 or len(actionlint_rejected) != 1:
            print(
                "SELF-TEST FAILED: expected 21 actionlint-clean fixtures and "
                "1 explicit actionlint-rejected matrix fixture"
            )
            return 1
        require_actionlint_clean(actionlint_clean)
        require_actionlint_rejected(actionlint_rejected)
        violations, total, safe = scan(str(temporary))
        names = {entry[0] for entry in violations}
        expected = {
            "constant.yml",
            "workflow-name.yml",
            "matrix.yml",
            "pr-number-on-push.yml",
            "literal-ref.yml",
            "quoted-ref.yml",
            "sha.yml",
            "trailing-comment.yml",
            "false-ref-guard.yml",
            "ref-comparison.yml",
            "false-pr-number-guard.yml",
            "false-run-id-guard.yml",
            "pr-number-or-ref-third-fallback.yml",
            "false-guarded-pr-number-or-ref.yml",
            "issue-or-pr-third-fallback.yml",
            "false-guarded-issue-or-pr.yml",
        }
        if total != 22 or safe != 6 or names != expected:
            print(
                "SELF-TEST FAILED: expected 22 blocks / 6 safe / violations "
                f"{sorted(expected)}, got {total} / {safe} / {sorted(names)}"
            )
            return 1

        empty = temporary / "empty"
        empty.mkdir()
        try:
            scan(str(empty))
        except ValueError:
            pass
        else:
            print("SELF-TEST FAILED: empty directory reported a clean scan")
            return 1

        malformed = temporary / "malformed"
        malformed.mkdir()
        (malformed / "broken.yml").write_text("on: [unclosed\n", encoding="utf-8")
        try:
            scan(str(malformed))
        except ValueError:
            pass
        else:
            print("SELF-TEST FAILED: malformed YAML reported a clean scan")
            return 1

        for duplicate_name, duplicate_lines in (
            ("duplicate-group", "  group: first\n  group: second\n  cancel-in-progress: true\n"),
            ("duplicate-cancel", "  group: literal\n  cancel-in-progress: false\n  cancel-in-progress: true\n"),
        ):
            ambiguous = temporary / duplicate_name
            ambiguous.mkdir()
            (ambiguous / "ambiguous.yml").write_text(
                "on:\n  push:\nconcurrency:\n"
                + duplicate_lines
                + "jobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: 'true'\n",
                encoding="utf-8",
            )
            try:
                scan(str(ambiguous))
            except ValueError:
                pass
            else:
                print(f"SELF-TEST FAILED: {duplicate_name} reported a clean scan")
                return 1

        print("SELF-TEST OK: 21 actionlint-clean + 1 explicit-invalid matrix fixtures; comments/literals, conditional and near-fallback discriminator collapse, PR scope, duplicate keys, and bad input have teeth")
        return 0
    finally:
        shutil.rmtree(temporary)


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if len(sys.argv) > 2:
        print("usage: concurrency_scope_scan.py [dir] | --self-test", file=sys.stderr)
        return 2
    directory = sys.argv[1] if len(sys.argv) == 2 else ".github/workflows"
    try:
        violations, total, safe = scan(directory)
    except ValueError as error:
        print(f"FATAL: {error}", file=sys.stderr)
        return 1
    print(
        f"scanned blocks={total} cross-ref-safe={safe} "
        f"not-cross-ref-safe={total - safe} VIOLATIONS={len(violations)}"
    )
    for violation in violations:
        print("  VIOLATION:", violation)
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())

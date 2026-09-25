"""Structural and mutation tests for the B-141 runner/permission boundary."""

from __future__ import annotations

import ast
import copy
import re
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = REPO_ROOT / ".github" / "workflows"
WORKFLOW_PATHS = (*WORKFLOWS.glob("*.yml"), *WORKFLOWS.glob("*.yaml"))
TRUSTED_ASSOCIATIONS = {"OWNER", "MEMBER", "COLLABORATOR"}
ALL_PULL_REQUEST_TARGET_WORKFLOWS = {
    "dependabot-auto-merge.yml",
    "dependabot-policy.yml",
    "dependabot-policy-trust-boundary.yml",
    "file-size-ratchet.yml",
    "backlog-verify.yml",
    "issue-2176-grpc-deny-gate.yml",
    "pr-labels.yml",
    "secrets-drift.yml",
    "welcome-first-pr.yml",
}
EXPECTED_JOBS = {
    "dependabot-auto-merge.yml": {"auto-merge"},
    "dependabot-policy.yml": {"sentinel", "policy-gate"},
    "dependabot-policy-trust-boundary.yml": {"trust-boundary-teeth"},
    "file-size-ratchet.yml": {"ratchet"},
    "backlog-verify.yml": {"verify", "trusted_semantic"},
    "issue-2176-grpc-deny-gate.yml": {"deny-contract"},
    "pr-labels.yml": {"label", "size"},
    "secrets-drift.yml": {"secrets-drift"},
    "welcome-first-pr.yml": {"welcome"},
}
EXPECTED_RUNNERS = {
    "dependabot-auto-merge.yml": {
        "auto-merge": "corelink",
    },
    "dependabot-policy.yml": {
        "sentinel": ["self-hosted", "mac", "corelink-builder"],
        "policy-gate": "corelink",
    },
    "dependabot-policy-trust-boundary.yml": {
        "trust-boundary-teeth": "corelink",
    },
    "file-size-ratchet.yml": {
        "ratchet": "corelink",
    },
    "backlog-verify.yml": {"verify": "corelink", "trusted_semantic": "ubuntu-24.04"},
    "issue-2176-grpc-deny-gate.yml": {"deny-contract": "ubuntu-24.04"},
    "pr-labels.yml": {
        "label": "corelink",
        "size": "corelink",
    },
    "secrets-drift.yml": {"secrets-drift": "ubuntu-latest"},
    "welcome-first-pr.yml": {
        "welcome": ["self-hosted", "mac", "corelink-builder"],
    },
}
EXPECTED_PERMISSIONS = {
    "dependabot-auto-merge.yml": {"contents": "write", "pull-requests": "write"},
    "dependabot-policy.yml": {
        "contents": "read",
        "pull-requests": "read",
        "checks": "read",
    },
    "dependabot-policy-trust-boundary.yml": {"contents": "read"},
    "file-size-ratchet.yml": {"contents": "read"},
    "backlog-verify.yml": {"contents": "read"},
    "issue-2176-grpc-deny-gate.yml": {"contents": "read"},
    "pr-labels.yml": {
        "contents": "read",
        "pull-requests": "write",
        "issues": "write",
    },
    "secrets-drift.yml": {"contents": "read"},
    "welcome-first-pr.yml": {"issues": "write", "pull-requests": "write"},
}

# A file-disjoint hosted migration may replace the historical selector, but
# cannot introduce an arbitrary runner.  Keeping this narrow transition set
# lets the protected-boundary assertions continue to cover actor gates,
# permissions, and data-only checkout behavior while the shared workflows
# migrate one bounded bundle at a time.
HOSTED_RUNNERS = {"ubuntu-24.04", "ubuntu-latest", "macos-15", "macos-15-intel"}


_YAML_KEY = re.compile(r"^(?P<key>[^:#][^:]*?):(?:[ \t]*(?P<value>.*))?$")
_TARGET_TOKEN = re.compile(r"\bpull_request_target\b")
_TRUSTED_BASE_EXPRESSION = (
    "${{ github.event.pull_request.base.sha || github.event.before || github.sha }}"
)
_ALLOWED_BASE_REF_RUN_LINES = (
    'git show "${BASE_REF}:scripts/validate_file_size_ratchet.py" > '
    '"$RUNNER_TEMP/validate_file_size_ratchet.py"',
    '--base-ref "$BASE_REF" --head-ref "$HEAD_REF"',
)


def _workflow_lines(text: str) -> list[tuple[int, str]]:
    """Return significant lines, rejecting tabs before any interpretation.

    This is deliberately a small, closed parser for the workflow subset used by
    this gate.  It does not pretend to be a general YAML implementation: an
    unsupported/malformed line is an error rather than a silently ignored gate.
    """
    lines: list[tuple[int, str]] = []
    for lineno, raw in enumerate(text.splitlines(), 1):
        if "\t" in raw[: len(raw) - len(raw.lstrip())]:
            raise ValueError(f"line {lineno}: tabs are not supported")
        content = raw.lstrip(" ")
        if not content or content.startswith("#"):
            continue
        indent = len(raw) - len(content)
        if "#" in content:
            quote = None
            for index, char in enumerate(content):
                if char in "'\"":
                    quote = None if quote == char else char if quote is None else quote
                elif char == "#" and quote is None and (index == 0 or content[index - 1].isspace()):
                    content = content[:index].rstrip()
                    break
        if content:
            lines.append((indent, content))
    return lines


def _mapping_entry(content: str) -> tuple[str, str]:
    match = _YAML_KEY.match(content)
    if match is None:
        raise ValueError(f"unsupported YAML mapping entry: {content!r}")
    key = match.group("key").strip()
    if not key or key.startswith("-"):
        raise ValueError(f"unsupported YAML key: {key!r}")
    return key.strip("'\""), match.group("value") or ""


def _scalar(value: str):
    value = value.strip()
    if not value:
        return None
    if value.startswith("["):
        if not value.endswith("]"):
            raise ValueError(f"unterminated inline sequence: {value!r}")
        members = [member.strip() for member in value[1:-1].split(",") if member.strip()]
        return [_scalar(member) for member in members]
    if value[:1] in {"'", '"'}:
        try:
            parsed = ast.literal_eval(value)
        except (SyntaxError, ValueError) as error:
            raise ValueError(f"malformed quoted scalar: {value!r}") from error
        if not isinstance(parsed, str):
            raise ValueError(f"unsupported scalar: {value!r}")
        return parsed
    if value in {"null", "~"}:
        return None
    if value == "true":
        return True
    if value == "false":
        return False
    if re.fullmatch(r"-?[0-9]+", value):
        return int(value)
    return value


def _parse_workflow_subset(text: str) -> dict:
    """Parse only top-level triggers, permissions, and job boundary fields.

    The five target workflows use mappings, inline sequences, and folded `if:`
    scalars for these fields.  Steps and other job metadata are intentionally
    opaque; their indentation is still validated, while required boundary
    fields are decoded strictly.
    """
    lines = _workflow_lines(text)
    workflow: dict = {}
    index = 0
    while index < len(lines):
        indent, content = lines[index]
        if indent != 0:
            raise ValueError(f"expected top-level mapping, found {content!r}")
        key, value = _mapping_entry(content)
        if key in workflow:
            raise ValueError(f"duplicate top-level key: {key}")
        if value:
            parsed = _scalar(value)
            if key == "on" and _TARGET_TOKEN.search(value):
                raise ValueError(
                    "inline/scalar pull_request_target trigger is unsupported; "
                    "use a mapping event key so the boundary can audit it"
                )
            workflow[key] = parsed
            index += 1
            continue
        start = index + 1
        end = start
        while end < len(lines) and lines[end][0] > 0:
            end += 1
        section = lines[start:end]
        if key == "on":
            events: dict = {}
            for event_indent, event_content in section:
                if event_indent == 2:
                    event, event_value = _mapping_entry(event_content)
                    if event in events:
                        raise ValueError(f"duplicate trigger: {event}")
                    events[event] = _scalar(event_value) if event_value else {}
                elif event_indent < 2:
                    raise ValueError(f"invalid trigger indentation: {event_content!r}")
            workflow[key] = events
        elif key == "permissions":
            permissions: dict = {}
            for permission_indent, permission_content in section:
                if permission_indent == 2:
                    permission, permission_value = _mapping_entry(permission_content)
                    if permission in permissions or not permission_value:
                        raise ValueError(f"invalid permission entry: {permission_content!r}")
                    permissions[permission] = _scalar(permission_value)
                elif permission_indent < 2:
                    raise ValueError(f"invalid permission indentation: {permission_content!r}")
            workflow[key] = permissions
        elif key == "jobs":
            jobs: dict = {}
            cursor = 0
            while cursor < len(section):
                job_indent, job_content = section[cursor]
                if job_indent != 2:
                    cursor += 1
                    continue
                job_name, job_value = _mapping_entry(job_content)
                if job_value:
                    raise ValueError(f"job {job_name} must be a mapping")
                if job_name in jobs:
                    raise ValueError(f"duplicate job: {job_name}")
                job: dict = {}
                cursor += 1
                while cursor < len(section) and section[cursor][0] > 2:
                    field_indent, field_content = section[cursor]
                    if field_indent != 4:
                        cursor += 1
                        continue
                    field, field_value = _mapping_entry(field_content)
                    if field in job:
                        raise ValueError(f"duplicate job field: {job_name}:{field}")
                    if field_value in {">-", ">", "|-", "|"}:
                        chunks: list[str] = []
                        cursor += 1
                        while cursor < len(section) and section[cursor][0] > 4:
                            chunks.append(section[cursor][1])
                            cursor += 1
                        if not chunks:
                            raise ValueError(f"empty folded scalar: {job_name}:{field}")
                        job[field] = " ".join(chunks) if field_value.startswith(">") else "\n".join(chunks)
                        continue
                    job[field] = _scalar(field_value)
                    cursor += 1
                jobs[job_name] = job
            workflow[key] = jobs
        index = end
    return workflow


def load_workflow(name: str) -> dict:
    loaded = _parse_workflow_subset((WORKFLOWS / name).read_text(encoding="utf-8"))
    if not isinstance(loaded, dict):
        raise AssertionError(f"{name} must be a workflow mapping")
    return loaded


def _census_pull_request_target_workflows(paths: tuple[Path, ...]) -> dict[str, dict]:
    """Find and parse every workflow with a real pull_request_target trigger.

    The census is intentionally structural rather than a grep for arbitrary
    prose: comments and quoted values are stripped by ``_workflow_lines``, then
    only the top-level ``on`` section is inspected.  A quoted event key is the
    same YAML mapping key as its unquoted spelling.  Inline/scalar event syntax
    is rejected explicitly because this focused parser cannot audit it; it must
    never disappear from the population by returning ``False``.
    """
    discovered: dict[str, dict] = {}
    for path in paths:
        text = path.read_text(encoding="utf-8")
        try:
            lines = _workflow_lines(text)
            has_target = False
            index = 0
            while index < len(lines):
                indent, content = lines[index]
                if indent != 0:
                    raise ValueError(f"expected top-level mapping, found {content!r}")
                # YAML document markers are valid outside the focused workflow
                # subset.  Ignore them for discovery; a candidate is still
                # handed to the strict parser below and fails there if its
                # boundary fields cannot be decoded.
                if content in {"---", "..."}:
                    index += 1
                    continue
                key, value = _mapping_entry(content)
                start = index + 1
                end = start
                while end < len(lines) and lines[end][0] > 0:
                    end += 1
                if key == "on":
                    if value:
                        _scalar(value)
                        if _TARGET_TOKEN.search(value):
                            raise ValueError(
                                "inline/scalar pull_request_target trigger is unsupported"
                            )
                    else:
                        for event_indent, event_content in lines[start:end]:
                            if event_indent == 2:
                                event, _ = _mapping_entry(event_content)
                                if event == "pull_request_target":
                                    has_target = True
                            elif event_indent < 2:
                                raise ValueError(
                                    f"invalid trigger indentation: {event_content!r}"
                                )
                index = end if not value else index + 1

            if not has_target:
                continue
            loaded = _parse_workflow_subset(text)
            if not triggers_pull_request_target(loaded):
                raise ValueError(
                    "pull_request_target token was not represented by the parsed trigger map"
                )
            discovered[path.name] = loaded
        except ValueError as error:
            raise ValueError(f"{path.name}: {error}") from error
    return discovered


def triggers_pull_request_target(workflow: dict) -> bool:
    triggers = workflow.get("on")
    if isinstance(triggers, list) and "pull_request_target" in triggers:
        raise ValueError("inline pull_request_target trigger is unsupported")
    return isinstance(triggers, dict) and "pull_request_target" in triggers


_GATE_TOKEN = re.compile(r"\s*(\|\||&&|==|!=|\(|\)|'[^']*'|[A-Za-z_][A-Za-z0-9_.-]*)")


def parse_gate(expression: str) -> tuple:
    """Parse the restricted boolean grammar used by job-level `if:` gates."""
    tokens: list[str] = []
    offset = 0
    while offset < len(expression):
        match = _GATE_TOKEN.match(expression, offset)
        if match is None:
            raise ValueError(f"unsupported gate syntax at offset {offset}")
        tokens.append(match.group(1))
        offset = match.end()

    cursor = 0

    def peek() -> str | None:
        return tokens[cursor] if cursor < len(tokens) else None

    def consume(expected: str | None = None) -> str:
        nonlocal cursor
        token = peek()
        if token is None or (expected is not None and token != expected):
            raise ValueError(f"expected {expected!r}, found {token!r}")
        cursor += 1
        return token

    def primary() -> tuple:
        if peek() == "(":
            consume("(")
            node = disjunction()
            consume(")")
            return node
        left = consume()
        if left in {"&&", "||", "==", "!=", ")"}:
            raise ValueError(f"expected operand, found {left!r}")
        operator = consume()
        if operator not in {"==", "!="}:
            raise ValueError(f"expected comparison, found {operator!r}")
        right = consume()
        if right in {"&&", "||", "==", "!=", "(", ")"}:
            raise ValueError(f"expected comparison value, found {right!r}")
        if right.startswith("'") and right.endswith("'"):
            right = right[1:-1]
        return ("compare", operator, left, right)

    def conjunction() -> tuple:
        node = primary()
        while peek() == "&&":
            consume("&&")
            node = ("and", node, primary())
        return node

    def disjunction() -> tuple:
        node = conjunction()
        while peek() == "||":
            consume("||")
            node = ("or", node, conjunction())
        return node

    tree = disjunction()
    if peek() is not None:
        raise ValueError(f"unexpected trailing token {peek()!r}")
    return tree


def flattened(tree: tuple, operator: str) -> list[tuple]:
    if tree[0] != operator:
        return [tree]
    return flattened(tree[1], operator) + flattened(tree[2], operator)


def expected_comparisons(
    test: unittest.TestCase,
    job: dict,
    name: str,
    *,
    include_non_target_branch: bool = False,
) -> None:
    assert_transition_runner(test, job.get("runs-on"), "corelink")
    test.assertIsInstance(job.get("if"), str, f"{name} needs a job-level actor gate")
    try:
        terms = flattened(parse_gate(job["if"]), "or")
    except ValueError as error:
        test.fail(f"{name} has unsupported gate syntax: {error}")
    expected = {
        ("compare", "==", "github.event.pull_request.author_association", association)
        for association in TRUSTED_ASSOCIATIONS
    }
    if include_non_target_branch:
        expected.add(("compare", "!=", "github.event_name", "pull_request_target"))
    test.assertEqual(len(terms), len(expected), f"{name} has unexpected boolean terms")
    test.assertEqual(set(terms), expected, f"{name} must be the exact allowlist gate")


def assert_actor_gate(test: unittest.TestCase, job: dict, name: str) -> None:
    expected_comparisons(test, job, name)


def assert_dependabot_gate(test: unittest.TestCase, job: dict, name: str) -> None:
    assert_transition_runner(test, job.get("runs-on"), "corelink")
    test.assertIsInstance(job.get("if"), str, f"{name} needs a job-level actor gate")
    try:
        terms = flattened(parse_gate(job["if"]), "and")
    except ValueError as error:
        test.fail(f"{name} has unsupported gate syntax: {error}")
    expected = {
        ("compare", "==", "github.actor", "dependabot[bot]"),
        ("compare", "==", "github.event.pull_request.user.login", "dependabot[bot]"),
    }
    test.assertEqual(len(terms), len(expected), f"{name} has unexpected boolean terms")
    test.assertEqual(
        set(terms), expected, f"{name} must conjunct both Dependabot identities"
    )


def assert_labels_boundary(test: unittest.TestCase, workflow: dict) -> None:
    jobs = workflow.get("jobs")
    test.assertIsInstance(jobs, dict)
    test.assertEqual(set(jobs), {"label", "size"})
    for name in ("label", "size"):
        assert_actor_gate(test, jobs[name], name)


def assert_permissions(test: unittest.TestCase, workflow: dict, name: str) -> None:
    test.assertEqual(workflow.get("permissions"), EXPECTED_PERMISSIONS[name])


def assert_runners(test: unittest.TestCase, workflow: dict, name: str) -> None:
    for job_name, expected in EXPECTED_RUNNERS[name].items():
        assert_transition_runner(test, workflow["jobs"][job_name].get("runs-on"), expected)


def assert_transition_runner(test: unittest.TestCase, actual: object, legacy: object) -> None:
    test.assertTrue(
        actual == legacy or (isinstance(actual, str) and actual in HOSTED_RUNNERS),
        f"runner must be the frozen legacy selector or an approved GitHub-hosted selector; got {actual!r}",
    )


def assert_file_size_boundary(
    test: unittest.TestCase, workflow: dict, raw: str | None = None
) -> None:
    """Allow B-126's safe data-only exception to the actor-gated fabric rule.

    The ratchet intentionally runs for every PR, including forks, but its
    trusted-base workflow executes only a validator copied from ``BASE_REF``.
    The candidate checkout is Git-object input: no candidate script, action, or
    writable token is consumed.
    """
    jobs = workflow.get("jobs")
    test.assertIsInstance(jobs, dict)
    test.assertEqual(set(jobs), {"ratchet"})
    ratchet = jobs["ratchet"]
    assert_transition_runner(test, ratchet.get("runs-on"), "corelink")
    test.assertNotIn("if", ratchet, "data-only ratchet must not skip fork PRs")
    test.assertEqual(workflow.get("permissions"), {"contents": "read"})

    raw = raw or (WORKFLOWS / "file-size-ratchet.yml").read_text(encoding="utf-8")
    test.assertIn(
        "group: file-size-ratchet-${{ github.event_name }}-"
        "${{ github.event.pull_request.number || github.ref }}",
        raw,
        "PR runs must key cancellation by PR number while other events retain ref isolation",
    )
    test.assertIn("cancel-in-progress: true", raw)
    test.assertRegex(raw, r"(?m)^permissions:\n  contents: read\s*$")
    assert_file_size_trusted_base(test, raw)
    test.assertIn(
        'git show "${BASE_REF}:scripts/validate_file_size_ratchet.py"', raw
    )
    test.assertIn(
        'python3 "$RUNNER_TEMP/validate_file_size_ratchet.py"', raw
    )
    test.assertIn('--base-ref "$BASE_REF" --head-ref "$HEAD_REF"', raw)
    test.assertIn(
        "repository: ${{ github.event.pull_request.head.repo.full_name || github.repository }}",
        raw,
    )
    test.assertIn(
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}", raw
    )
    test.assertIn("fetch-depth: 0", raw)
    test.assertIn("persist-credentials: false", raw)
    uses = re.findall(r"^\s+uses:\s*(\S+)", raw, flags=re.MULTILINE)
    test.assertEqual(
        uses,
        [
            "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
        ],
        "the data-only lane may use only its pinned checkout action",
    )
    test.assertNotRegex(
        raw,
        r'(?m)^\s*python3\s+scripts/',
        "the ratchet must not execute a candidate-tree script",
    )
    test.assertNotRegex(
        raw,
        r'(?m)^\s*(?:bash|sh|python3)\s+"?\$GITHUB_WORKSPACE/',
        "the ratchet must not execute a candidate-tree file",
    )
    test.assertNotIn(
        'git show "$HEAD_REF:scripts/validate_file_size_ratchet.py"', raw
    )


def assert_file_size_trusted_base(test: unittest.TestCase, raw: str) -> None:
    """Require one event-aware, non-candidate-controlled base expression.

    The PR base SHA is the trusted workflow boundary. For push and dispatch,
    ``event.before`` (or the current trusted ``github.sha``) is the only valid
    fallback. Keeping this as an exact assignment check prevents a future
    alias, indirection, or ``HEAD_REF`` fallback from silently turning the
    executable validator into candidate-controlled input.
    """
    lines = _workflow_lines(raw)
    ratchet_start = next(
        (index for index, (indent, content) in enumerate(lines)
         if indent == 6 and content == "- name: Ratchet"),
        None,
    )
    test.assertIsNotNone(ratchet_start, "ratchet step must be structurally present")
    ratchet_lines = lines[ratchet_start + 1 :]
    ratchet_end = next(
        (index for index, (indent, _content) in enumerate(ratchet_lines)
         if indent <= 6),
        len(ratchet_lines),
    )
    ratchet_lines = ratchet_lines[:ratchet_end]
    env_start = next(
        (index for index, (indent, content) in enumerate(ratchet_lines)
         if indent == 8 and content == "env:"),
        None,
    )
    test.assertIsNotNone(env_start, "BASE_REF must live in the ratchet job env")
    env_lines = ratchet_lines[env_start + 1 :]
    env_end = next(
        (index for index, (indent, _content) in enumerate(env_lines)
         if indent <= 8),
        len(env_lines),
    )
    env_lines = env_lines[:env_end]
    assignments = [
        content.split(":", 1)[1].strip()
        for indent, content in env_lines
        if indent == 10 and content.startswith("BASE_REF:")
    ]
    test.assertEqual(
        assignments,
        [_TRUSTED_BASE_EXPRESSION],
        "BASE_REF must be exactly PR base.sha, then trusted before/sha fallback",
    )
    all_declarations = [
        (indent, content)
        for indent, content in lines
        if content.startswith("BASE_REF:")
    ]
    test.assertEqual(
        all_declarations,
        [(10, f"BASE_REF: {_TRUSTED_BASE_EXPRESSION}")],
        "BASE_REF must be declared exactly once in the trusted ratchet env",
    )

    run_start = next(
        (index for index, (indent, content) in enumerate(ratchet_lines)
         if indent == 8 and content == "run: |"),
        None,
    )
    test.assertIsNotNone(run_start, "ratchet command must be a structural run block")
    run_lines = ratchet_lines[run_start + 1 :]
    base_ref_lines = [
        content
        for _indent, content in run_lines
        if re.search(r"\bBASE_REF\b", content)
    ]
    test.assertEqual(
        base_ref_lines,
        list(_ALLOWED_BASE_REF_RUN_LINES),
        "every BASE_REF in run must be one of the two read-only trusted uses",
    )
    git_show_lines = [
        content for _indent, content in run_lines if re.search(r"\bgit\s+show\b", content)
    ]
    test.assertEqual(
        git_show_lines,
        [_ALLOWED_BASE_REF_RUN_LINES[0]],
        "every git show in ratchet run must be the trusted validator read",
    )
    base_argument_lines = [
        content for _indent, content in run_lines if re.search(r"--base-ref\b", content)
    ]
    test.assertEqual(
        base_argument_lines,
        [_ALLOWED_BASE_REF_RUN_LINES[1]],
        "every --base-ref in ratchet run must use the exact trusted argument",
    )


def assert_backlog_verify_boundary(test: unittest.TestCase, workflow: dict) -> None:
    """The backlog PR lane executes only BASE control over PR data."""
    jobs = workflow.get("jobs")
    test.assertEqual(set(jobs or {}), {"verify", "trusted_semantic"})
    test.assertEqual(workflow.get("permissions"), {"contents": "read"})
    assert_transition_runner(test, jobs["verify"].get("runs-on"), "corelink")
    raw = (WORKFLOWS / "backlog-verify.yml").read_text(encoding="utf-8")
    test.assertIn("pull_request_target:", raw)
    test.assertNotIn("\n  pull_request:\n", raw)
    test.assertGreaterEqual(raw.count("persist-credentials: false"), 2)
    test.assertIn("github.event.pull_request.head.sha || github.sha", raw)
    test.assertIn("github.event.pull_request.base.sha || github.sha", raw)
    test.assertNotIn("github.event.pull_request.head.ref", raw)
    test.assertNotIn("github.event.pull_request.base.ref", raw)
    test.assertNotIn("GH_TOKEN", raw)
    test.assertIn("path: _candidate", raw)
    test.assertIn("path: _base", raw)
    test.assertIn("working-directory: _base", raw)
    test.assertIn("--candidate-file", raw)
    test.assertIn("--trusted-file", raw)
    test.assertIn("--trusted-semantic", raw)
    test.assertIn("if: github.event_name == 'push' || github.event_name == 'schedule'", raw)
    trusted = jobs["trusted_semantic"]
    test.assertEqual(trusted.get("if"), "github.event_name == 'push' || github.event_name == 'schedule'")
    test.assertEqual(trusted.get("permissions"), {"contents": "read", "vulnerability-alerts": "read"})


def assert_data_only_hosted_boundary(test: unittest.TestCase, name: str) -> None:
    raw = (WORKFLOWS / name).read_text(encoding="utf-8")
    test.assertEqual(raw.count("persist-credentials: false"), 2)
    test.assertIn("github.event.pull_request.base.sha", raw)
    test.assertIn("github.event.pull_request.head.sha", raw)
    if name == "issue-2176-grpc-deny-gate.yml":
        test.assertIn("github.repository_id == '1232040291'", raw)
        test.assertIn("--trusted-base trusted-base", raw)
        test.assertIn("--candidate candidate", raw)
    else:
        test.assertIn("path: .trusted", raw)
        test.assertIn("path: .candidate", raw)
        test.assertIn("${GITHUB_WORKSPACE}/.trusted/scripts/validate_secrets_matrix.py", raw)


def assert_welcome_boundary(test: unittest.TestCase, workflow: dict) -> None:
    test.assertTrue(triggers_pull_request_target(workflow))
    triggers = workflow.get("on", workflow.get(True))
    test.assertIn("issues", triggers)
    test.assertEqual(set(workflow.get("jobs", {})), {"welcome"})
    welcome = workflow["jobs"]["welcome"]
    assert_transition_runner(test, welcome.get("runs-on"), ["self-hosted", "mac", "corelink-builder"])
    test.assertEqual(welcome.get("timeout-minutes"), 5)
    test.assertNotIn("if", welcome)


class PullRequestTargetSpawnBoundaryTest(unittest.TestCase):
    def test_parser_fails_closed_on_unsupported_or_malformed_yaml(self) -> None:
        for malformed in (
            "jobs:\n  broken\n",
            "jobs:\n  broken:\n    runs-on: corelink\n    if: >-\n",
            "jobs:\n\tbroken:\n",
            "on: [push, pull_request_target]\n",
            "on: {push: {}, pull_request_target: {}}\n",
        ):
            with self.subTest(malformed=malformed), self.assertRaises(ValueError):
                _parse_workflow_subset(malformed)

    def test_pull_request_target_population_is_closed(self) -> None:
        actual = set(_census_pull_request_target_workflows(WORKFLOW_PATHS))
        self.assertEqual(actual, ALL_PULL_REQUEST_TARGET_WORKFLOWS)

    def test_new_trigger_spellings_cannot_evade_population(self) -> None:
        quoted = """name: sixth quoted trigger
on:
  "pull_request_target":
permissions:
  contents: read
jobs:
  escaped:
    runs-on: corelink
    if: github.event.pull_request.author_association == 'OWNER'
"""
        inline = """name: sixth inline trigger
on: [push, pull_request_target]
permissions:
  contents: read
jobs:
  escaped:
    runs-on: corelink
    if: github.event.pull_request.author_association == 'OWNER'
"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            quoted_path = root / "sixth-quoted.yml"
            inline_path = root / "sixth-inline.yml"
            quoted_path.write_text(quoted, encoding="utf-8")
            inline_path.write_text(inline, encoding="utf-8")

            with self.subTest(spelling="quoted mapping"):
                discovered = _census_pull_request_target_workflows(
                    (*WORKFLOW_PATHS, quoted_path)
                )
                self.assertIn(quoted_path.name, discovered)
                with self.assertRaises(AssertionError):
                    self.assertEqual(set(discovered), ALL_PULL_REQUEST_TARGET_WORKFLOWS)

            with self.subTest(spelling="inline sequence"):
                with self.assertRaisesRegex(
                    ValueError, r"sixth-inline\.yml: inline(?:/scalar)? pull_request_target"
                ):
                    _census_pull_request_target_workflows((*WORKFLOW_PATHS, inline_path))

    def test_population_jobs_and_permissions_are_explicit(self) -> None:
        for name in ALL_PULL_REQUEST_TARGET_WORKFLOWS:
            workflow = load_workflow(name)
            self.assertEqual(set(workflow.get("jobs", {})), EXPECTED_JOBS[name])
            assert_permissions(self, workflow, name)
            assert_runners(self, workflow, name)

    def test_every_ephemeral_job_is_fail_closed(self) -> None:
        for name in ALL_PULL_REQUEST_TARGET_WORKFLOWS:
            workflow = load_workflow(name)
            jobs = workflow["jobs"]
            for job_name, job in jobs.items():
                if name == "dependabot-auto-merge.yml" or (
                    name == "dependabot-policy.yml" and job_name == "policy-gate"
                ):
                    assert_dependabot_gate(self, job, f"{name}:{job_name}")
                elif name == "dependabot-policy-trust-boundary.yml":
                    self.assertEqual(job.get("timeout-minutes"), 10)
                    assert_dependabot_gate(self, job, f"{name}:{job_name}")
                elif name == "file-size-ratchet.yml":
                    assert_file_size_boundary(self, workflow)
                elif name == "backlog-verify.yml":
                    assert_backlog_verify_boundary(self, workflow)
                elif name in {"issue-2176-grpc-deny-gate.yml", "secrets-drift.yml"}:
                    assert_data_only_hosted_boundary(self, name)
                elif name in {"dependabot-policy.yml", "welcome-first-pr.yml"}:
                    # The sentinel and greeting lanes have distinct public
                    # purposes; their runner is checked by assert_runners.
                    continue
                else:
                    assert_actor_gate(self, job, f"{name}:{job_name}")

            # The two public-purpose lanes must have a non-fabric boundary.
            if name == "dependabot-policy.yml":
                sentinel = jobs["sentinel"]
                assert_transition_runner(
                    self, sentinel.get("runs-on"), ["self-hosted", "mac", "corelink-builder"]
                )
                self.assertEqual(
                    sentinel.get("if"), "github.actor != 'dependabot[bot]'"
                )
            if name == "welcome-first-pr.yml":
                welcome = jobs["welcome"]
                assert_transition_runner(
                    self, welcome.get("runs-on"), ["self-hosted", "mac", "corelink-builder"]
                )

    def test_fabric_jobs_allow_only_trusted_associations(self) -> None:
        assert_labels_boundary(self, load_workflow("pr-labels.yml"))
        assert_file_size_boundary(self, load_workflow("file-size-ratchet.yml"))

    def test_welcome_preserves_first_timer_purpose_without_fabric_spawn(self) -> None:
        assert_welcome_boundary(self, load_workflow("welcome-first-pr.yml"))

    def test_mutations_reopen_the_boundary(self) -> None:
        labels = load_workflow("pr-labels.yml")
        for name in ("label", "size"):
            with self.subTest(mutant=f"remove {name} actor gate"):
                mutant = copy.deepcopy(labels)
                del mutant["jobs"][name]["if"]
                with self.assertRaises(AssertionError):
                    assert_labels_boundary(self, mutant)

            with self.subTest(mutant=f"widen {name} actor gate"):
                mutant = copy.deepcopy(labels)
                mutant["jobs"][name]["if"] += (
                    " || github.event.pull_request.author_association == 'CONTRIBUTOR'"
                )
                with self.assertRaises(AssertionError):
                    assert_labels_boundary(self, mutant)

            for label, mutation in (
                ("tautology", lambda condition: condition + " || true"),
                (
                    "boolean inversion",
                    lambda condition: condition.replace(" || ", " && "),
                ),
                ("boolean addition", lambda condition: condition + " && true"),
                (
                    "comparison inversion",
                    lambda condition: condition.replace("== 'OWNER'", "!= 'OWNER'", 1),
                ),
            ):
                with self.subTest(mutant=f"{name} {label}"):
                    mutant = copy.deepcopy(labels)
                    mutant["jobs"][name]["if"] = mutation(mutant["jobs"][name]["if"])
                    with self.assertRaises(AssertionError):
                        assert_labels_boundary(self, mutant)

        file_size = load_workflow("file-size-ratchet.yml")
        with self.subTest(mutant="add ratchet actor gate"):
            mutant = copy.deepcopy(file_size)
            mutant["jobs"]["ratchet"]["if"] = "true"
            with self.assertRaises(AssertionError):
                assert_file_size_boundary(self, mutant)

        file_size_text = (WORKFLOWS / "file-size-ratchet.yml").read_text(
            encoding="utf-8"
        )
        with self.subTest(mutant="base-ref-only ratchet concurrency"):
            mutant_text = file_size_text.replace(
                "${{ github.event.pull_request.number || github.ref }}",
                "${{ github.ref }}",
            )
            with self.assertRaises(AssertionError):
                assert_file_size_boundary(self, file_size, mutant_text)

        data_only_mutations = (
            (
                "execute candidate validator",
                lambda text: text.replace(
                    'python3 "$RUNNER_TEMP/validate_file_size_ratchet.py"',
                    "python3 scripts/validate_file_size_ratchet.py",
                ),
            ),
            (
                "load validator from candidate",
                lambda text: text.replace(
                    'git show "${BASE_REF}:scripts/validate_file_size_ratchet.py"',
                    'git show "${HEAD_REF}:scripts/validate_file_size_ratchet.py"',
                ),
            ),
            (
                "resolve base from candidate head",
                lambda text: text.replace(
                    _TRUSTED_BASE_EXPRESSION,
                    "${{ github.event.pull_request.head.sha || github.sha }}",
                ),
            ),
            (
                "resolve base through head alias",
                lambda text: text.replace(
                    _TRUSTED_BASE_EXPRESSION,
                    "${{ HEAD_REF }}",
                ),
            ),
            (
                "resolve base from current candidate",
                lambda text: text.replace(
                    _TRUSTED_BASE_EXPRESSION,
                    "${{ github.sha }}",
                ),
            ),
            (
                "overwrite base through shell alias",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          BASE_REF="$HEAD_REF"\n',
                ),
            ),
            (
                "export base through shell alias",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          export BASE_REF="$HEAD_REF"\n',
                ),
            ),
            (
                "declare base through shell alias",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          declare BASE_REF="$HEAD_REF"\n',
                ),
            ),
            (
                "typeset base through shell alias",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          typeset BASE_REF="$HEAD_REF"\n',
                ),
            ),
            (
                "local base through shell alias",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          local BASE_REF="$HEAD_REF"\n',
                ),
            ),
            (
                "readonly base through shell alias",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          readonly BASE_REF="$HEAD_REF"\n',
                ),
            ),
            (
                "inline base environment",
                lambda text: text.replace(
                    '          GODFILE_REPO_ROOT="$GITHUB_WORKSPACE" \\\n',
                    '          BASE_REF="$HEAD_REF" GODFILE_REPO_ROOT="$GITHUB_WORKSPACE" \\\n',
                ),
            ),
            (
                "write base through GITHUB_ENV",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          echo "BASE_REF=$HEAD_REF" >> "$GITHUB_ENV"\n',
                ),
            ),
            (
                "unknown extra base occurrence",
                lambda text: text.replace(
                    '          set -euo pipefail\n',
                    '          set -euo pipefail\n          echo "BASE_REF is trusted"\n',
                ),
            ),
            (
                "load validator through trusted-base alias",
                lambda text: text.replace(
                    'git show "${BASE_REF}:scripts/validate_file_size_ratchet.py"',
                    'git show "${TRUSTED_BASE}:scripts/validate_file_size_ratchet.py"',
                ),
            ),
            (
                "load arbitrary candidate file",
                lambda text: text.replace(
                    'git show "${BASE_REF}:scripts/validate_file_size_ratchet.py"',
                    'git show "${HEAD_REF}:evil.sh"',
                ),
            ),
            (
                "echo an untrusted base argument",
                lambda text: text.replace(
                    '--base-ref "$BASE_REF" --head-ref "$HEAD_REF"',
                    'echo --base-ref "$HEAD_REF"',
                ),
            ),
            (
                "add candidate action",
                lambda text: text.replace(
                    "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
                    "${{ github.event.pull_request.head.ref }}",
                ),
            ),
            (
                "broaden token permission",
                lambda text: text.replace("  contents: read", "  contents: write"),
            ),
            (
                "execute candidate shell file",
                lambda text: text.replace(
                    'GODFILE_REPO_ROOT="$GITHUB_WORKSPACE"',
                    'bash "$GITHUB_WORKSPACE/scripts/evil.sh"\n          GODFILE_REPO_ROOT="$GITHUB_WORKSPACE"',
                ),
            ),
        )
        for label, mutation in data_only_mutations:
            with self.subTest(mutant=f"ratchet {label}"):
                with self.assertRaises(AssertionError):
                    assert_file_size_boundary(self, file_size, mutation(file_size_text))

        for name, job_name in (
            ("dependabot-auto-merge.yml", "auto-merge"),
            ("dependabot-policy.yml", "policy-gate"),
        ):
            workflow = load_workflow(name)
            with self.subTest(mutant=f"remove {name}:{job_name} actor gate"):
                mutant = copy.deepcopy(workflow)
                del mutant["jobs"][job_name]["if"]
                with self.assertRaises(AssertionError):
                    assert_dependabot_gate(self, mutant["jobs"][job_name], job_name)

            with self.subTest(mutant=f"widen {name}:{job_name} actor gate"):
                mutant = copy.deepcopy(workflow)
                mutant["jobs"][job_name]["if"] += " || github.actor == 'attacker'"
                # The trusted bot identity must remain conjunctive; changing it
                # to a broad actor predicate is caught by the structural check.
                with self.assertRaises(AssertionError):
                    assert_dependabot_gate(self, mutant["jobs"][job_name], job_name)

            for label, mutation in (
                (
                    "operator inversion",
                    lambda condition: condition.replace(" && ", " || "),
                ),
                ("tautology", lambda condition: condition + " || true"),
                ("boolean addition", lambda condition: condition + " && true"),
                (
                    "actor comparison inversion",
                    lambda condition: condition.replace(
                        "github.actor == ", "github.actor != ", 1
                    ),
                ),
            ):
                with self.subTest(mutant=f"{name}:{job_name} {label}"):
                    mutant = copy.deepcopy(workflow)
                    mutant["jobs"][job_name]["if"] = mutation(
                        mutant["jobs"][job_name]["if"]
                    )
                    with self.assertRaises(AssertionError):
                        assert_dependabot_gate(self, mutant["jobs"][job_name], job_name)

        policy = load_workflow("dependabot-policy.yml")
        with self.subTest(mutant="move policy sentinel to fabric"):
            mutant = copy.deepcopy(policy)
            mutant["jobs"]["sentinel"]["runs-on"] = "corelink"
            with self.assertRaises(AssertionError):
                assert_runners(self, mutant, "dependabot-policy.yml")

        for name in ALL_PULL_REQUEST_TARGET_WORKFLOWS:
            with self.subTest(mutant=f"broaden {name} permissions"):
                mutant = load_workflow(name)
                # Every target lane needs pull-request metadata; changing this
                # shared scope to read is a representative permission drift.
                expected_pr = EXPECTED_PERMISSIONS[name].get("pull-requests")
                mutant["permissions"]["pull-requests"] = (
                    "read" if expected_pr == "write" else "write"
                )
                with self.assertRaises(AssertionError):
                    assert_permissions(self, mutant, name)

        welcome = load_workflow("welcome-first-pr.yml")
        for label, mutation in (
            (
                "move welcome to fabric",
                lambda job: job.__setitem__("runs-on", "corelink"),
            ),
            ("remove welcome timeout", lambda job: job.pop("timeout-minutes")),
        ):
            with self.subTest(mutant=label):
                mutant = copy.deepcopy(welcome)
                mutation(mutant["jobs"]["welcome"])
                with self.assertRaises(AssertionError):
                    assert_welcome_boundary(self, mutant)


if __name__ == "__main__":
    unittest.main()

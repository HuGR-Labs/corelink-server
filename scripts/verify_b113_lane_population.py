#!/usr/bin/env python3
"""Fail-closed population and contract check for the B-113 CI lanes.

This is intentionally a structural *population* check, not a single grep over
the seven B-113 workflow names.  Each lane has its own workflow/job boundary and its
own load-bearing markers.  A missing job, a changed runner, or a weakened
installer therefore reports the lane that drifted.  The companion tests mutate
each boundary and each marker to prove that an empty or partial population
cannot pass.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
MAX_WORKFLOW_BYTES = 300_000
BACKLOG = Path("BACKLOG.md")
INTERNAL_PACKET = Path("docs/internal/b113-six-lanes-owner-actions.md")
OWNER_PACKET = Path("docs/handoff/2026-09-05-owner-action-packets-b008-b154.json")

# B-113 owns seven workflow files and nine named jobs (Buck2 has three
# independently observable jobs).  terraform-drift is deliberately absent: it
# is the B-111-owned overlap and must never be silently counted here.
EXPECTED_WORKFLOWS = frozenset(
    {
        ".github/workflows/nightly.yml",
        ".github/workflows/sbom.yml",
        ".github/workflows/buck2-starter-ci.yml",
        ".github/workflows/fuzz-nightly.yml",
        ".github/workflows/endurance-2h-nightly.yml",
        ".github/workflows/load-test-nightly.yml",
        ".github/workflows/billing-health-daily.yml",
    }
)
EXCLUDED_WORKFLOWS = frozenset({".github/workflows/terraform-drift.yml"})


@dataclass(frozen=True)
class Lane:
    name: str
    workflow: str
    job: str
    markers: tuple[str, ...]
    workflow_markers: tuple[str, ...] = ()


# These markers must be executable shell controls, not prose or a quoted
# string.  The remaining markers are YAML/Python structure and are checked by
# their own active-line validators below.
ACTIVE_SHELL_MARKERS = frozenset(
    {
        "cargo mutants --workspace --no-shuffle --minimum-test-timeout=600",
        "time buck2 build :hello \\",
        "./scripts/benchmark.sh --iterations 10",
        '"${BUCK2_INSTALL_DIR}/buck2" --version',
        "test -s /tmp/cold-report.json",
        "jq -e 'type == \"object\"' /tmp/cold-report.json",
        "test -s /tmp/warm-report.json",
        "RATIO < 80",
        "sudo apt-get install -y --no-install-recommends zstd jq",
        "sudo apt-get install -y --no-install-recommends zstd",
        "sudo apt-get install -y --no-install-recommends zstd jq bc",
        '"${ACTUAL_SHA256}" == "${BUCK2_SHA256}"',
        "unset CORELINK_PAT",
        'export CORELINK_PAT="invalid_pat_b113"',
        "192.0.2.1/bad-cache",
        '"$SBOM_PYTHON" tests/verify_rust_sbom.py --check',
        "cp .sbom/cyclonedx-rust.json sbom.cdx.json",
        "-max_total_time=${{ env.FUZZ_DURATION }}",
        "python3 scripts/check_billing_health.py",
        "python3 scripts/rolling-7d-p99.py",
        "timeout 30s gh run list",
        '--expected-scenarios "${EXPECTED_SCENARIOS}"',
        'printf \'{"scenario":"%s","outcome":"%s"}',
    }
)
PYTHON_MARKERS = frozenset(
    {
        "expected exactly one current endurance summary",
        "rolling-7d p99 baseline is missing",
        "baseline p99 is missing or invalid",
    }
)


LANES = (
    Lane(
        "nightly-mutants",
        ".github/workflows/nightly.yml",
        "mutants-workspace",
        (
            "runs-on: [self-hosted, mac, corelink-builder]",
            "timeout-minutes: 240",
            "taiki-e/install-action@07b4745e0c39a41822af610387492e3e53aa222b",
            "tool: cargo-mutants@27.0.0",
            "fallback: cargo-binstall",
            "timeout-minutes: 225",
            "cargo mutants --workspace --no-shuffle --minimum-test-timeout=600",
        ),
    ),
    Lane(
        "sbom-generate",
        ".github/workflows/sbom.yml",
        "sbom-generate",
        (
            "runs-on: [self-hosted, mac, corelink-builder]",
            'name: "Verify committed Cargo.lock SBOM"',
            'SBOM_VENV="${RUNNER_TEMP}/corelink-sbom-venv"',
            '"$SBOM_PYTHON" tests/verify_rust_sbom.py --check',
            "cp .sbom/cyclonedx-rust.json sbom.cdx.json",
            "name: sbom-cdx-json",
            "if-no-files-found: error",
        ),
    ),
    Lane(
        "buck2-build",
        ".github/workflows/buck2-starter-ci.yml",
        "build",
        (
            "runs-on: ubuntu-latest",
            "timeout-minutes: 20",
            'name: Install Buck2 latest stable',
            '"${BUCK2_INSTALL_DIR}/buck2" --version',
            "test -s /tmp/cold-report.json",
            "jq -e 'type == \"object\"' /tmp/cold-report.json",
            "test -s /tmp/warm-report.json",
            "time buck2 build :hello \\",
            ".cache_hits | type == \"number\"",
            ".total_actions > 0",
            "RATIO < 80",
            "sudo apt-get install -y --no-install-recommends zstd jq",
        ),
        ('BUCK2_SHA256: "aa304d471a79f69233b09767d4ba9add769049b7a37f78a3a71a72983372f511"',),
    ),
    Lane(
        "buck2-negative",
        ".github/workflows/buck2-starter-ci.yml",
        "negative-scenarios",
        (
            "runs-on: ubuntu-latest",
            "timeout-minutes: 10",
            "name: Install Buck2",
            "--max-time 120",
            '"${ACTUAL_SHA256}" == "${BUCK2_SHA256}"',
            "unset CORELINK_PAT",
            'export CORELINK_PAT="invalid_pat_b113"',
            "192.0.2.1/bad-cache",
            "sudo apt-get install -y --no-install-recommends zstd",
        ),
    ),
    Lane(
        "buck2-benchmark",
        ".github/workflows/buck2-starter-ci.yml",
        "benchmark",
        (
            "runs-on: ubuntu-latest",
            "timeout-minutes: 30",
            "name: Install Buck2",
            "--max-time 120",
            '"${ACTUAL_SHA256}" == "${BUCK2_SHA256}"',
            "./scripts/benchmark.sh --iterations 10",
            "sudo apt-get install -y --no-install-recommends zstd jq bc",
        ),
    ),
    Lane(
        "fuzz-nightly",
        ".github/workflows/fuzz-nightly.yml",
        "fuzz-matrix-expansion",
        (
            "runs-on: [self-hosted, mac, corelink-builder]",
            "timeout-minutes: 40",
            "RUSTUP_TOOLCHAIN: nightly-x86_64-apple-darwin",
            'CARGO_HOME="${RUNNER_TEMP}/corelink-fuzz-cargo/${GITHUB_RUN_ID}/${GITHUB_RUN_ATTEMPT}/${FUZZ_CRATE}/${FUZZ_TARGET}"',
            'CARGO_TARGET_DIR="${RUNNER_TEMP}/corelink-fuzz-target/${GITHUB_RUN_ID}/${GITHUB_RUN_ATTEMPT}/${FUZZ_CRATE}/${FUZZ_TARGET}"',
            "-max_total_time=${{ env.FUZZ_DURATION }}",
            "timeout-minutes: 35",
        ),
        ("workflow_dispatch:", "cancel-in-progress: false"),
    ),
    Lane(
        "endurance-2h",
        ".github/workflows/endurance-2h-nightly.yml",
        "endurance-2h",
        (
            "runs-on: corelink",
            "timeout-minutes: 145",
            "K6_ENDURANCE_CONFIRM:       'yes'",
        ),
        (
            "workflow_dispatch:",
            "timeout 30s gh run list",
            "--status=completed",
            "python3 scripts/rolling-7d-p99.py",
            "--downloads-dir tests/load/results/endurance-2h-baseline/history",
            "--output tests/load/results/endurance-2h-baseline/rolling-7d-p99.json",
        ),
    ),
    Lane(
        "load-test",
        ".github/workflows/load-test-nightly.yml",
        "k6-staging",
        (
            "runs-on: corelink",
            "timeout-minutes: 45",
            "continue-on-error: true",
            "timeout-minutes: ${{ matrix.scenario.budget }}",
            "if-no-files-found: error",
            "- name: record scenario outcome",
            "OUTCOME: ${{ steps.run_k6.outcome }}",
            "path: tests/load/results/",
            'printf \'{"scenario":"%s","outcome":"%s"}',
        ),
        (
            "workflow_dispatch:",
            '--expected-scenarios "${EXPECTED_SCENARIOS}"',
            "--bootstrap",
        ),
    ),
    Lane(
        "billing-health",
        ".github/workflows/billing-health-daily.yml",
        "billing-health",
        (
            "runs-on: ubuntu-latest",
            "timeout-minutes: 10",
            "python3 scripts/check_billing_health.py",
        ),
        ("schedule:",),
    ),
)


JOB_RE = re.compile(
    r"(?ms)^  (?P<job>[A-Za-z0-9_-]+):\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)"
)


class VerificationError(RuntimeError):
    pass


def parse_workflow(workflow: str, lane: str) -> dict:
    """Parse workflow YAML so comments or free-form text cannot populate a lane."""
    try:
        parsed = yaml.safe_load(workflow)
    except yaml.YAMLError as error:
        raise VerificationError(f"{lane}: invalid workflow YAML: {error}") from error
    if not isinstance(parsed, dict):
        raise VerificationError(f"{lane}: workflow root is not a mapping")
    jobs = parsed.get("jobs")
    if not isinstance(jobs, dict):
        raise VerificationError(f"{lane}: workflow has no jobs mapping")
    return parsed


def workflow_events(parsed: dict) -> dict:
    """Read GitHub's `on` key (PyYAML 1.1 may decode it as boolean True)."""
    events = parsed.get("on", parsed.get(True, {}))
    if events is None:
        return {}
    if not isinstance(events, dict):
        raise VerificationError("workflow trigger block is not a mapping")
    return events


def assert_yaml_lane_shape(parsed: dict, lane: Lane) -> None:
    jobs = parsed["jobs"]
    job = jobs.get(lane.job)
    if not isinstance(job, dict):
        raise VerificationError(f"{lane.name}: expected executable job {lane.job!r}")
    if lane.name == "nightly-mutants":
        events = workflow_events(parsed)
        dispatch = events.get("workflow_dispatch")
        inputs = dispatch.get("inputs") if isinstance(dispatch, dict) else None
        lane_input = inputs.get("lane") if isinstance(inputs, dict) else None
        if not isinstance(lane_input, dict) or lane_input.get("type") != "choice":
            raise VerificationError("nightly-mutants: workflow_dispatch lane choice is missing")
        if lane_input.get("default") != "all" or lane_input.get("options") != ["all", "mutants"]:
            raise VerificationError("nightly-mutants: lane choice must default to all and offer all/mutants")

        all_lanes = ("tlc-extended", "proptest-extended", "fuzz-matrix")
        all_condition = "github.event_name != 'workflow_dispatch' || inputs.lane == 'all'"
        mutants_condition = (
            "github.event_name != 'workflow_dispatch' || inputs.lane == 'all' "
            "|| inputs.lane == 'mutants'"
        )
        for job_name in all_lanes:
            candidate = jobs.get(job_name)
            if not isinstance(candidate, dict) or candidate.get("if") != all_condition:
                raise VerificationError(
                    f"nightly-mutants: {job_name} must run for schedule/all and be excluded for mutants dispatch"
                )
        if job.get("if") != mutants_condition:
            raise VerificationError(
                "nightly-mutants: mutants-workspace must run for schedule, all, and mutants dispatch"
            )
    if lane.name == "fuzz-nightly":
        env = job.get("env")
        if not isinstance(env, dict) or env.get("RUSTUP_TOOLCHAIN") != "nightly-x86_64-apple-darwin":
            raise VerificationError("fuzz-nightly: nightly toolchain selection is not executable job env")
        if env.get("FUZZ_CRATE") != "${{ matrix.crate }}" or env.get("FUZZ_TARGET") != "${{ matrix.target }}":
            raise VerificationError("fuzz-nightly: matrix identity is not bound into isolated Cargo paths")
        steps = job.get("steps")
        if not isinstance(steps, list):
            raise VerificationError("fuzz-nightly: steps are missing")
        by_name = {
            step.get("name"): (index, step)
            for index, step in enumerate(steps)
            if isinstance(step, dict) and isinstance(step.get("name"), str)
        }
        required_steps = (
            "Isolate fuzz Cargo and target state per matrix leg",
            "Install cargo-fuzz (isolated CARGO_HOME — never the shared one)",
            "Run fuzz target (${{ env.FUZZ_DURATION }}s)",
        )
        if any(name not in by_name for name in required_steps):
            raise VerificationError("fuzz-nightly: isolation/install/run step boundary is incomplete")
        isolate_index, isolate = by_name[required_steps[0]]
        install_index, _install = by_name[required_steps[1]]
        run_index, run_step = by_name[required_steps[2]]
        if not isolate_index < install_index < run_index:
            raise VerificationError("fuzz-nightly: Cargo isolation must precede install and execution")
        isolate_run = isolate.get("run")
        required_exports = (
            'CARGO_HOME="${RUNNER_TEMP}/corelink-fuzz-cargo/${GITHUB_RUN_ID}/${GITHUB_RUN_ATTEMPT}/${FUZZ_CRATE}/${FUZZ_TARGET}"',
            'CARGO_TARGET_DIR="${RUNNER_TEMP}/corelink-fuzz-target/${GITHUB_RUN_ID}/${GITHUB_RUN_ATTEMPT}/${FUZZ_CRATE}/${FUZZ_TARGET}"',
            'echo "CARGO_HOME=${CARGO_HOME}" >> "${GITHUB_ENV}"',
            'echo "CARGO_TARGET_DIR=${CARGO_TARGET_DIR}" >> "${GITHUB_ENV}"',
        )
        if not isinstance(isolate_run, str) or any(value not in isolate_run for value in required_exports):
            raise VerificationError("fuzz-nightly: per-leg Cargo isolation/export is incomplete")
        run_env = run_step.get("env")
        if isinstance(run_env, dict) and ({"CARGO_HOME", "CARGO_TARGET_DIR"} & set(run_env)):
            raise VerificationError("fuzz-nightly: execution overrides isolated Cargo state")
    if lane.name == "buck2-negative":
        # Job-population errors are reported by the preflight loop below with
        # the missing lane's own name. Defer this cross-step semantic check
        # until all three Buck2 job boundaries are present.
        buck2_jobs = {item.job for item in LANES if item.workflow == lane.workflow}
        if any(not isinstance(jobs.get(name), dict) for name in buck2_jobs):
            return
        steps = job.get("steps")
        quota = [
            step for step in steps or []
            if isinstance(step, dict) and step.get("name") == "Scenario 4 — Dedicated quota PAT returns quota error"
        ]
        if len(quota) != 1 or quota[0].get("working-directory") != "examples/buck2-starter":
            raise VerificationError("buck2-negative: quota probe is not rooted in the starter project")
    if lane.workflow == ".github/workflows/buck2-starter-ci.yml":
        if job.get("runs-on") != "ubuntu-latest":
            raise VerificationError(f"{lane.name}: Buck2 jobs must use the hosted ubuntu-latest runner")
        permissions = parsed.get("permissions")
        if permissions != {"contents": "read"}:
            raise VerificationError(f"{lane.name}: workflow permissions must remain contents: read only")
        if lane.name == "buck2-benchmark":
            if "permissions" in job:
                raise VerificationError("buck2-benchmark: job-level permission overrides are forbidden")
            steps = job.get("steps")
            if not isinstance(steps, list):
                raise VerificationError("buck2-benchmark: steps are missing")
            forbidden = re.compile(r"(?m)^\s*git\s+(?:push|commit|add)\b")
            if any(forbidden.search(step.get("run", "")) for step in steps if isinstance(step, dict)):
                raise VerificationError("buck2-benchmark: repository write commands are forbidden")
            uploads = [
                step for step in steps
                if isinstance(step, dict)
                and step.get("uses") == "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
            ]
            if len(uploads) != 1:
                raise VerificationError("buck2-benchmark: expected one SHA-pinned report artifact upload")
            artifact = uploads[0].get("with")
            if not isinstance(artifact, dict) or artifact.get("name") != "buck2-benchmark-report":
                raise VerificationError("buck2-benchmark: artifact name is missing")
            if artifact.get("path") != "${{ runner.temp }}/buck2-benchmark/BENCHMARK.md":
                raise VerificationError("buck2-benchmark: report artifact path drifted")
            if artifact.get("if-no-files-found") != "error":
                raise VerificationError("buck2-benchmark: missing report must fail")
            retention = artifact.get("retention-days")
            if not isinstance(retention, int) or not 1 <= retention <= 30:
                raise VerificationError("buck2-benchmark: artifact retention must be bounded to 1–30 days")
    for marker in lane.workflow_markers:
        if marker == "schedule:" and not isinstance(workflow_events(parsed).get("schedule"), list):
            raise VerificationError(f"{lane.name}: missing YAML schedule trigger")
        if marker == "workflow_dispatch:" and "workflow_dispatch" not in workflow_events(parsed):
            raise VerificationError(f"{lane.name}: missing YAML workflow_dispatch trigger")
        if marker == "cancel-in-progress: false":
            concurrency = parsed.get("concurrency") or {}
            if not isinstance(concurrency, dict) or concurrency.get("cancel-in-progress") is not False:
                raise VerificationError(f"{lane.name}: missing YAML cancel-in-progress false")


def read_documentation() -> dict[str, str]:
    """Read the B-113 prose surfaces that must agree with the live census."""
    paths = (BACKLOG, INTERNAL_PACKET, OWNER_PACKET)
    documents: dict[str, str] = {}
    for path in paths:
        try:
            documents[path.as_posix()] = (ROOT / path).read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            raise VerificationError(f"missing or unreadable B-113 prose: {path}") from error
    return documents


def _b113_backlog_section(text: str) -> str:
    start = text.find("### B-113")
    end = text.find("\n### B-061", start + 1)
    if start < 0 or end < 0:
        raise VerificationError("B-113 backlog section boundary is missing")
    return text[start:end]


def verify_population_prose(documents: dict[str, str] | None = None) -> None:
    """Keep human-facing count and ownership claims tied to the census."""
    docs = read_documentation() if documents is None else documents
    required_paths = {path.as_posix() for path in (BACKLOG, INTERNAL_PACKET, OWNER_PACKET)}
    if set(docs) != required_paths:
        raise VerificationError("B-113 prose surface set drifted")

    backlog = _b113_backlog_section(docs[BACKLOG.as_posix()])
    backlog_markers = (
        "### B-113 — sete workflows self-hosted",
        "Sobram sete workflows",
        "workflows e nove jobs",
        "`terraform-drift.yml` fica",
        "ownership de B-111",
        "explicitamente excluída desta população",
        "Duas dessas (`nightly` e `billing-health-daily`)",
        "as sete têm causas distintas",
    )
    missing = [marker for marker in backlog_markers if marker not in backlog]
    if missing:
        raise VerificationError(f"B-113 backlog prose drifted: missing {missing}")
    if "terraform-drift do B-063" in backlog:
        raise VerificationError("B-113 backlog wrongly assigns terraform-drift to B-063")
    if "Sobram seis" in backlog or "as seis" in backlog:
        raise VerificationError("B-113 backlog still claims six lanes")

    internal = docs[INTERNAL_PACKET.as_posix()]
    internal_markers = (
        "# B-113 seven-workflow residual owner actions",
        "exact seven-workflow, nine-job population",
        "terraform-drift.yml` is explicitly excluded",
        "owned by B-111",
    )
    missing = [marker for marker in internal_markers if marker not in internal]
    if missing:
        raise VerificationError(f"B-113 internal packet prose drifted: missing {missing}")

    try:
        packet = json.loads(docs[OWNER_PACKET.as_posix()])
        b113 = next(item for item in packet["items"] if item.get("id") == "B-113")
        procedure = b113["procedure"]
        inputs = b113["inputs_and_credentials_boundary"]["inputs"]
    except (KeyError, StopIteration, TypeError, json.JSONDecodeError) as error:
        raise VerificationError("B-113 owner packet is missing its canonical shape") from error
    if not any("seven B-113 workflows (nine named job boundaries)" in step for step in procedure):
        raise VerificationError("B-113 owner packet lacks the seven-workflow/nine-job count")
    if not any("terraform-drift.yml is excluded because it is B-111-owned" in step for step in procedure):
        raise VerificationError("B-113 owner packet lacks the B-111 exclusion")
    if "seven named workflow lanes / nine named job boundaries" not in inputs:
        raise VerificationError("B-113 owner packet input census drifted")


def read_workflow(path: str) -> str:
    target = ROOT / path
    if not target.is_file():
        raise VerificationError(f"missing workflow for B-113 lane: {path}")
    text = target.read_text(encoding="utf-8")
    if len(text.encode("utf-8")) > MAX_WORKFLOW_BYTES:
        raise VerificationError(f"workflow exceeds bounded size: {path}")
    return text


def job_body(workflow: str, job: str, lane: str) -> str:
    matches = [m.group("body") for m in JOB_RE.finditer(workflow) if m.group("job") == job]
    if len(matches) != 1:
        raise VerificationError(f"{lane}: expected exactly one job boundary for {job!r}, found {len(matches)}")
    return matches[0]


def _active_shell_line(line: str, *, allow_printf: bool = False) -> str | None:
    """Return executable shell text, rejecting comments and no-op bait."""
    code = line.split("#", 1)[0].strip()
    if not code:
        return None
    if code.startswith("run:"):
        code = code[4:].strip()
    # A marker in echo/printf/:/true output is documentation, not a control.
    command = re.match(r"(?:env\s+)?(?:[A-Za-z_][A-Za-z0-9_]*=(?:[^ ]*|'.*?'|\".*?\")\s+)*(\S+)", code)
    blocked = {"echo", ":", "true", "false"}
    if not allow_printf:
        blocked.add("printf")
    if command is None or command.group(1) in blocked:
        return None
    return code


def _has_active_shell_marker(body: str, marker: str) -> bool:
    for line in body.splitlines():
        code = _active_shell_line(
            line,
            allow_printf=marker == 'printf \'{"scenario":"%s","outcome":"%s"}',
        )
        if code is not None and marker in code:
            # printf is valid only for the JSON outcome record, not as a
            # generic way to smuggle a required command into output text.
            if code.split(None, 1)[0] == "printf":
                return marker == 'printf \'{"scenario":"%s","outcome":"%s"}'
            return True
    return False


def _has_active_python_marker(body: str, marker: str) -> bool:
    for line in body.splitlines():
        code = line.split("#", 1)[0].strip()
        if marker in code and re.match(r"(?:print|raise|sys\.exit)\b|.*\bsys\.exit\(", code):
            return True
    return False


def _has_active_yaml_marker(body: str, marker: str) -> bool:
    for line in body.splitlines():
        code = line.split("#", 1)[0].strip()
        if marker in code and code:
            # YAML prose/comments and shell output are not configuration.
            if code.startswith(("echo ", "printf ", ": ")):
                continue
            return True
    return False


def remove_from_job(workflow: str, job: str, marker: str) -> tuple[str, bool]:
    """Remove one marker only inside a named job, even when siblings share it."""
    boundary = re.compile(
        rf"(?ms)^(?P<head>  {re.escape(job)}:\n)(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)"
    )
    match = boundary.search(workflow)
    if match is None:
        return workflow, False
    body = match.group("body")
    if body.count(marker) != 1:
        return workflow, False
    replaced = body.replace(marker, "", 1)
    return workflow[: match.start("body")] + replaced + workflow[match.end("body") :], True


def replace_in_job(workflow: str, job: str, marker: str, replacement: str) -> tuple[str, bool]:
    """Replace one marker only in its job, for comment/echo bait mutations."""
    boundary = re.compile(
        rf"(?ms)^(?P<head>  {re.escape(job)}:\n)(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)"
    )
    match = boundary.search(workflow)
    if match is None:
        return workflow, False
    body = match.group("body")
    if body.count(marker) != 1:
        return workflow, False
    replaced = body.replace(marker, replacement, 1)
    return workflow[: match.start("body")] + replaced + workflow[match.end("body") :], True


def verify(
    sources: dict[str, str] | None = None,
    documents: dict[str, str] | None = None,
    *,
    validate_yaml: bool = True,
) -> None:
    if sources is None:
        texts = {workflow: read_workflow(workflow) for workflow in EXPECTED_WORKFLOWS}
    else:
        texts = dict(sources)
        missing = EXPECTED_WORKFLOWS - texts.keys()
        extra = texts.keys() - EXPECTED_WORKFLOWS
        if missing:
            raise VerificationError(f"workflow population missing B-113 lane(s): {sorted(missing)}")
        if extra:
            if extra & EXCLUDED_WORKFLOWS:
                raise VerificationError("terraform-drift is B-111-owned and excluded from B-113 population")
            raise VerificationError(f"workflow population has unexpected lane(s): {sorted(extra)}")
    if set(texts) != EXPECTED_WORKFLOWS:
        raise VerificationError("workflow population is not exactly the seven B-113-owned workflows")
    verify_population_prose(documents)
    # Establish every job boundary before checking any body.  Without this
    # preflight, deleting a later YAML job could make its text appear inside a
    # sibling's body and produce a misleading rejection for the wrong lane.
    parsed: dict[str, dict] = {}
    for lane in LANES:
        workflow = texts[lane.workflow]
        if not workflow:
            raise VerificationError(f"{lane.name}: empty workflow population")
        if validate_yaml:
            parsed.setdefault(lane.workflow, parse_workflow(workflow, lane.name))
            assert_yaml_lane_shape(parsed[lane.workflow], lane)
        job_body(workflow, lane.job, lane.name)
    for lane in LANES:
        workflow = texts[lane.workflow]
        if not workflow:
            raise VerificationError(f"{lane.name}: empty workflow population")
        body = job_body(workflow, lane.job, lane.name)
        for marker in lane.markers:
            if marker in ACTIVE_SHELL_MARKERS:
                present = _has_active_shell_marker(body, marker)
            elif marker in PYTHON_MARKERS:
                present = _has_active_python_marker(body, marker)
            else:
                present = _has_active_yaml_marker(body, marker)
            if not present:
                raise VerificationError(f"{lane.name}: missing load-bearing marker {marker!r}")
        for marker in lane.workflow_markers:
            if marker in ACTIVE_SHELL_MARKERS:
                present = _has_active_shell_marker(workflow, marker)
            elif marker in PYTHON_MARKERS:
                present = _has_active_python_marker(workflow, marker)
            else:
                present = _has_active_yaml_marker(workflow, marker)
            if not present:
                raise VerificationError(f"{lane.name}: missing workflow marker {marker!r}")
        if lane.name == "buck2-negative" and re.search(r"(?m)^\s+needs:\s+build\s*$", body):
            raise VerificationError("buck2-negative: negative probes must remain independent of build")


def mutation_checks(sources: dict[str, str] | None = None) -> None:
    """Reject removal of every lane and every lane-specific contract tooth."""
    base = {lane.workflow: read_workflow(lane.workflow) for lane in LANES} if sources is None else dict(sources)
    verify(base)
    for lane in LANES:
        mutated = dict(base)
        workflow = mutated[lane.workflow]
        boundary = re.compile(rf"(?ms)^  {re.escape(lane.job)}:\n.*?(?=^  [A-Za-z0-9_-]+:\n|\Z)")
        replaced, count = boundary.subn("", workflow, count=1)
        if count != 1:
            raise VerificationError(f"{lane.name}: mutation fixture could not remove job boundary")
        mutated[lane.workflow] = replaced
        try:
            verify(mutated, validate_yaml=False)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"{lane.name}: removed job population was accepted")
    for lane in LANES:
        for marker in lane.markers:
            mutated = dict(base)
            workflow = mutated[lane.workflow]
            mutated[lane.workflow], changed = remove_from_job(workflow, lane.job, marker)
            if not changed:
                raise VerificationError(f"{lane.name}: marker mutation is ambiguous: {marker!r}")
            try:
                verify(mutated, validate_yaml=False)
            except VerificationError:
                pass
            else:
                raise VerificationError(f"{lane.name}: removed marker was accepted: {marker!r}")
        for marker in lane.workflow_markers:
            mutated = dict(base)
            workflow = mutated[lane.workflow]
            if workflow.count(marker) != 1:
                raise VerificationError(
                    f"{lane.name}: workflow marker mutation is ambiguous: {marker!r}"
                )
            mutated[lane.workflow] = workflow.replace(marker, "", 1)
            try:
                verify(mutated, validate_yaml=False)
            except VerificationError:
                pass
            else:
                raise VerificationError(f"{lane.name}: removed workflow marker was accepted: {marker!r}")

    # A required control must execute.  Replacing it with output text is a
    # common false-green mutation and must be rejected just like deletion.
    bait_mutations = (
        ("buck2-build", "test -s /tmp/cold-report.json"),
        ("load-test", 'printf \'{"scenario":"%s","outcome":"%s"}'),
        ("billing-health", "python3 scripts/check_billing_health.py"),
    )
    for lane_name, marker in bait_mutations:
        lane = next(item for item in LANES if item.name == lane_name)
        mutated = dict(base)
        mutated[lane.workflow], changed = replace_in_job(
            mutated[lane.workflow], lane.job, marker, f"echo {marker}"
        )
        if not changed:
            raise VerificationError(f"{lane_name}: active-command bait mutation is ambiguous")
        try:
            verify(mutated, validate_yaml=False)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"{lane_name}: echo bait was accepted for {marker!r}")

    dependent_negative = dict(base)
    negative = next(item for item in LANES if item.name == "buck2-negative")
    dependent_negative[negative.workflow] = dependent_negative[negative.workflow].replace(
        "    # Keep negative probes independent:", "    needs: build\n    # Keep negative probes independent:", 1
    )
    try:
        verify(dependent_negative, validate_yaml=False)
    except VerificationError:
        pass
    else:
        raise VerificationError("buck2-negative: build dependency mutation was accepted")

    quota_cwd = dict(base)
    quota_lane = next(item for item in LANES if item.name == "buck2-negative")
    quota_marker = (
        "      - name: Scenario 4 — Dedicated quota PAT returns quota error\n"
        "        working-directory: examples/buck2-starter\n"
    )
    if quota_cwd[quota_lane.workflow].count(quota_marker) != 1:
        raise VerificationError("buck2-negative: quota cwd mutation fixture is ambiguous")
    quota_cwd[quota_lane.workflow] = quota_cwd[quota_lane.workflow].replace(
        quota_marker,
        "      - name: Scenario 4 — Dedicated quota PAT returns quota error\n",
        1,
    )
    try:
        verify(quota_cwd)
    except VerificationError:
        pass
    else:
        raise VerificationError("buck2-negative: quota probe outside starter project was accepted")

    fuzz_exports = dict(base)
    fuzz_lane = next(item for item in LANES if item.name == "fuzz-nightly")
    for export in (
        'echo "CARGO_HOME=${CARGO_HOME}" >> "${GITHUB_ENV}"',
        'echo "CARGO_TARGET_DIR=${CARGO_TARGET_DIR}" >> "${GITHUB_ENV}"',
    ):
        mutated = dict(fuzz_exports)
        if mutated[fuzz_lane.workflow].count(export) != 1:
            raise VerificationError("fuzz-nightly: export mutation fixture is ambiguous")
        mutated[fuzz_lane.workflow] = mutated[fuzz_lane.workflow].replace(export, ":", 1)
        try:
            verify(mutated)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"fuzz-nightly: missing runtime export was accepted: {export}")

    # Human-facing count and ownership prose is part of the evidence boundary:
    # a correct workflow census must not coexist with a stale B-113 narrative.
    documents = read_documentation()
    prose_mutations = (
        (
            "backlog-count",
            BACKLOG.as_posix(),
            "Sobram sete workflows",
            "Sobram seis workflows",
        ),
        (
            "backlog-ownership",
            BACKLOG.as_posix(),
            "ownership de B-111",
            "ownership de B-063",
        ),
        (
            "packet-count",
            OWNER_PACKET.as_posix(),
            "seven B-113 workflows (nine named job boundaries)",
            "six B-113 workflows (nine named job boundaries)",
        ),
        (
            "packet-ownership",
            OWNER_PACKET.as_posix(),
            "terraform-drift.yml is excluded because it is B-111-owned",
            "terraform-drift.yml is excluded because it is B-063-owned",
        ),
    )
    for name, path, marker, replacement in prose_mutations:
        mutated_documents = dict(documents)
        if mutated_documents[path].count(marker) != 1:
            raise VerificationError(f"{name}: prose mutation fixture is ambiguous")
        mutated_documents[path] = mutated_documents[path].replace(marker, replacement, 1)
        try:
            verify(base, mutated_documents, validate_yaml=False)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"{name}: stale count/ownership prose was accepted")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run adversarial mutation checks")
    args = parser.parse_args()
    try:
        if args.self_test:
            mutation_checks()
        else:
            verify()
    except (OSError, VerificationError) as error:
        print(f"B-113 DRIFTED: {error}", file=sys.stderr)
        return 1
    print(
        f"B-113 population confirmed: {len(EXPECTED_WORKFLOWS)} workflows / "
        f"{len(LANES)} named job boundaries are present"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

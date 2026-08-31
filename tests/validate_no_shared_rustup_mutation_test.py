#!/usr/bin/env python3
"""
validate_no_shared_rustup_mutation_test.py — pytest suite for
`scripts/validate_no_shared_rustup_mutation.py`.

Why this file exists
--------------------
The guard decides WHICH jobs to inspect from the scalar after `runs-on:`. Until
2026-08-31 it compared that scalar with `== "corelink"`, so the repo's own
convention of justifying a runner inline —

    runs-on: corelink  # zero-hosted: python3 baked into the image

— silently removed the job from the inspected set. The guard never failed; it
just covered less and kept printing OK. Measured on `main` that day: 20
self-hosted jobs invisible, inspected count 173 instead of 193.

That is the failure mode this suite exists to make impossible to reintroduce:
**a guard whose reach shrinks without anyone being told.**

Coverage:
- `_strip_trailing_comment` on every `runs-on:` form the repo actually uses.
- `is_self_hosted` classifies a commented `corelink` job as self-hosted.
- A GitHub-hosted runner is still NOT classified as self-hosted (no
  over-broadening — the fix must be a strengthening, not a widening).
- Teeth: a provisioning step inside a *commented* self-hosted job is caught.
- Reach: the live repo yields a non-trivial inspected count, so a parser break
  cannot masquerade as "nothing to inspect".

Run:
    python3 -m pytest tests/validate_no_shared_rustup_mutation_test.py -v
"""

from __future__ import annotations

import importlib.util
import pathlib
import sys

import pytest

REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "validate_no_shared_rustup_mutation.py"


def _load():
    spec = importlib.util.spec_from_file_location("vnsrm", SCRIPT)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    sys.modules["vnsrm"] = mod
    spec.loader.exec_module(mod)
    return mod


vnsrm = _load()


# ── the strip itself ──────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("corelink", "corelink"),
        ("corelink  # zero-hosted: python3 baked into the image", "corelink"),
        ("corelink # anything at all", "corelink"),
        ("[self-hosted, mac, corelink-builder]", "[self-hosted, mac, corelink-builder]"),
        ("[self-hosted, mac, corelink-builder]  # the owner's Mac", "[self-hosted, mac, corelink-builder]"),
        ("ubuntu-latest", "ubuntu-latest"),
        ("ubuntu-latest          # datacenter IP outside our provider", "ubuntu-latest"),
    ],
)
def test_strip_trailing_comment(raw: str, expected: str) -> None:
    assert vnsrm._strip_trailing_comment(raw) == expected


def test_expression_runs_on_is_left_intact() -> None:
    """A `${{ }}` expression has no bare `#`; splitting one would be wrong."""
    expr = "${{ github.event_name == 'schedule' && 'ubuntu-latest' || fromJSON('[\"self-hosted\",\"mac\"]') }}"
    assert vnsrm._strip_trailing_comment(expr) == expr


# ── classification ────────────────────────────────────────────────────────────


def test_commented_corelink_is_self_hosted() -> None:
    """THE REGRESSION. Before the fix this returned False and the job vanished."""
    assert vnsrm.is_self_hosted("corelink  # zero-hosted (WP-CI): python3 puro") is True


def test_commented_mac_list_is_self_hosted() -> None:
    assert vnsrm.is_self_hosted("[self-hosted, mac, corelink-builder]  # owner's Mac") is True


@pytest.mark.parametrize(
    "hosted",
    [
        "ubuntu-latest",
        "ubuntu-latest  # datacenter IP genuinely outside our provider",
        "ubuntu-x64-4core  # 4-core: the full-workspace test link OOMs on 2-core",
        "windows-latest",
    ],
)
def test_github_hosted_is_not_self_hosted(hosted: str) -> None:
    """The fix must STRENGTHEN reach, never widen it onto hosted runners."""
    assert vnsrm.is_self_hosted(hosted) is False


# ── teeth: the guard must actually fail on a commented self-hosted job ────────


PROVISIONING_JOB = """\
name: fixture
on: workflow_dispatch
jobs:
  a-job:
    runs-on: corelink  # zero-hosted: justified inline, the repo convention
    steps:
      - uses: dtolnay/rust-toolchain@stable
"""

CLEAN_JOB = """\
name: fixture
on: workflow_dispatch
jobs:
  a-job:
    runs-on: corelink  # zero-hosted: justified inline, the repo convention
    steps:
      - run: echo ok
"""


def _run_against(tmp_path: pathlib.Path, content: str) -> tuple[int, str]:
    """Point the module's WORKFLOWS at a throwaway dir and run main().

    Returns (rc, stderr). stderr matters: `main()` returns non-zero for TWO very
    different reasons — a real violation, and the "inspected ZERO jobs" bail-out.
    A test that only checks `rc != 0` passes on the bail-out and would therefore
    have gone green against the very bug this file exists to pin.
    """
    import io
    from contextlib import redirect_stderr

    wf = tmp_path / ".github" / "workflows"
    wf.mkdir(parents=True)
    (wf / "fixture.yml").write_text(content)
    original = vnsrm.WORKFLOWS
    vnsrm.WORKFLOWS = wf
    err = io.StringIO()
    try:
        with redirect_stderr(err):
            rc = vnsrm.main()
    finally:
        vnsrm.WORKFLOWS = original
    return rc, err.getvalue()


def test_teeth_provisioning_in_commented_job_is_caught(tmp_path: pathlib.Path) -> None:
    """A gate that cannot fail is not a gate. This is the positive control.

    Asserts the SPECIFIC failure, not merely a non-zero exit: the job must have
    been inspected and found provisioning a toolchain.
    """
    rc, err = _run_against(tmp_path, PROVISIONING_JOB)
    assert rc != 0
    assert "provisions a toolchain" in err, err
    assert "ZERO self-hosted jobs" not in err, (
        "guard bailed out instead of inspecting the commented job — this is the "
        "pre-2026-08-31 blindness, not a real catch"
    )


def test_clean_commented_job_passes(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, CLEAN_JOB)
    assert rc == 0, err


# ── reach on the live repo ────────────────────────────────────────────────────


def test_live_repo_reach_is_not_vacuous() -> None:
    """
    Guards against the other half of the defect: a parser break that inspects
    nothing would otherwise print a cheerful OK. The script has its own
    zero-check; this pins a floor well above zero so a large silent shrink is a
    test failure rather than a quieter success message.
    """
    import io
    from contextlib import redirect_stdout

    buf = io.StringIO()
    cwd = pathlib.Path.cwd()
    import os

    os.chdir(REPO_ROOT)
    try:
        with redirect_stdout(buf):
            vnsrm.main()
    finally:
        os.chdir(cwd)
    out = buf.getvalue()
    inspected = int(out.split("self-hosted job(s)")[0].split()[-1])
    assert inspected >= 150, f"guard reach collapsed to {inspected}: {out!r}"

#!/usr/bin/env python3
"""
validate_no_shared_rustup_mutation_test.py — pytest suite for
`scripts/validate_no_shared_rustup_mutation.py`.

Why this file exists
--------------------
The guard decides WHICH jobs to inspect from the structural YAML value at
`jobs[*].runs-on`. Before the preceding fix it compared the scalar with
`== "corelink"`, so the repo's own convention of justifying a runner inline —

    runs-on: corelink  # zero-hosted: python3 baked into the image

— silently removed the job from the inspected set. The guard never failed; it
just covered less and kept printing OK. Measured that day: ~20 self-hosted jobs
invisible, which is the gap between the mutant reach and the live reach below.

That is the failure mode this suite exists to make impossible to reintroduce:
**a guard whose reach shrinks without anyone being told.** B-140 extends the
same invariant to quoted scalars and block sequences, which line-oriented
matching cannot see.

Coverage:
- `_strip_trailing_comment` on every `runs-on:` form the repo actually uses.
- Structural scalar, quoted scalar, inline list, block sequence, and expression
  values, including the stdlib-only parser fallback.
- Matrix runner resolution and conservative handling of unknown expressions.
- Step-level `uses` detection in block and flow mappings, with a false-positive
  control for `run` text.
- Double-quoted YAML escapes resolve identically in PyYAML and the stdlib
  fallback, while unsupported escapes fail closed.
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


def _run_against(
    tmp_path: pathlib.Path, content: str, *, baseline: str | None = None
) -> tuple[int, str]:
    """Point the module's WORKFLOWS at a throwaway dir and run main().

    Returns (rc, stderr). stderr matters: `main()` returns non-zero for TWO very
    different reasons — a real violation, and the "inspected ZERO jobs" bail-out.
    A test that only checks `rc != 0` passes on the bail-out and would therefore
    have gone green against the very bug this file exists to pin.
    """
    import io
    from contextlib import redirect_stderr

    wf = tmp_path / ".github" / "workflows"
    wf.mkdir(parents=True, exist_ok=True)
    (wf / "fixture.yml").write_text(content)
    args: list[str] = []
    if baseline is not None:
        baseline_wf = tmp_path / "baseline" / ".github" / "workflows"
        baseline_wf.mkdir(parents=True, exist_ok=True)
        (baseline_wf / "fixture.yml").write_text(baseline)
        args = ["--baseline-workflows", str(baseline_wf)]
    original = vnsrm.WORKFLOWS
    vnsrm.WORKFLOWS = wf
    err = io.StringIO()
    try:
        with redirect_stderr(err):
            rc = vnsrm.main(args)
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


# ── pull-request baseline comparison ─────────────────────────────────────────


def test_baseline_allows_an_unchanged_inherited_violation(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, PROVISIONING_JOB, baseline=PROVISIONING_JOB)
    assert rc == 0, err


def test_baseline_rejects_a_new_violation(tmp_path: pathlib.Path) -> None:
    candidate = PROVISIONING_JOB.replace(
        "- uses: dtolnay/rust-toolchain@stable",
        "- uses: dtolnay/rust-toolchain@stable\n      - uses: actions-rs/toolchain@stable",
    )
    rc, err = _run_against(tmp_path, candidate, baseline=PROVISIONING_JOB)
    assert rc == 1
    assert "1 new violation(s)" in err
    assert "actions-rs/toolchain@stable" in err


def test_baseline_rejects_a_new_versioned_toolchain_path(tmp_path: pathlib.Path) -> None:
    candidate = CLEAN_JOB.replace(
        "run: echo ok", "run: echo $HOME/.rustup/toolchains/1.91.1-x86_64-apple-darwin/bin"
    )
    rc, err = _run_against(tmp_path, candidate, baseline=CLEAN_JOB)
    assert rc == 1
    assert "1 new violation(s)" in err
    assert "hardcodes a versioned toolchain path" in err


def test_baseline_counts_duplicate_identical_violations(tmp_path: pathlib.Path) -> None:
    candidate = PROVISIONING_JOB.replace(
        "- uses: dtolnay/rust-toolchain@stable",
        "- uses: dtolnay/rust-toolchain@stable\n      - uses: dtolnay/rust-toolchain@stable",
    )
    rc, err = _run_against(tmp_path, candidate, baseline=PROVISIONING_JOB)
    assert rc == 1
    assert "1 new violation(s)" in err


def test_baseline_ignores_line_movement_of_an_inherited_violation(tmp_path: pathlib.Path) -> None:
    candidate = PROVISIONING_JOB.replace("steps:\n", "steps:\n      # line movement only\n")
    rc, err = _run_against(tmp_path, candidate, baseline=PROVISIONING_JOB)
    assert rc == 0, err


def test_baseline_allows_removing_an_inherited_violation(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, CLEAN_JOB, baseline=PROVISIONING_JOB)
    assert rc == 0, err


def test_baseline_parser_failure_is_loud(tmp_path: pathlib.Path) -> None:
    malformed = "name: malformed\njobs:\n  broken:\n    runs-on: [self-hosted, mac\n"
    rc, err = _run_against(tmp_path, CLEAN_JOB, baseline=malformed)
    assert rc == 2
    assert "parser failure" in err


def test_baseline_zero_reach_is_loud(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, CLEAN_JOB, baseline="name: hosted only\n")
    assert rc == 2
    assert "ZERO self-hosted jobs" in err


def test_candidate_zero_reach_is_loud_in_baseline_mode(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, "name: hosted only\n", baseline=CLEAN_JOB)
    assert rc == 2
    assert "ZERO self-hosted jobs" in err


def test_actionlint_event_baseline_contract_is_immutable_and_strict_on_push() -> None:
    workflow = (REPO_ROOT / ".github" / "workflows" / "actionlint.yml").read_text(encoding="utf-8")
    assert "expected_sha:" in workflow
    assert "baseline_sha:" in workflow
    assert "EXPECTED_SHA: ${{ inputs.expected_sha }}" in workflow
    assert "EXPECTED_BASELINE_SHA: ${{ inputs.baseline_sha }}" in workflow
    assert 'if [[ "$EXPECTED_SHA" != "$GITHUB_SHA" ]]' in workflow
    assert 'if [[ ! "$EXPECTED_BASELINE_SHA" =~ ^[0-9a-f]{40}$ ]]' in workflow
    assert "github.event.pull_request.head.sha || github.sha" in workflow
    assert "ref: ${{ github.event.pull_request.base.sha }}" in workflow
    assert "ref: main" in workflow
    assert 'ACTUAL_BASELINE_SHA="$(git -C baseline rev-parse HEAD)"' in workflow
    assert 'if [[ "$EXPECTED_BASELINE_SHA" != "$ACTUAL_BASELINE_SHA" ]]' in workflow
    assert "if: github.event_name == 'pull_request'" in workflow
    assert "if: github.event_name == 'workflow_dispatch'" in workflow
    assert "--baseline-workflows baseline/.github/workflows" in workflow
    assert workflow.count("persist-credentials: false") >= 3
    assert (
        'if [[ "${{ github.event_name }}" == "pull_request" || "${{ github.event_name }}" == "workflow_dispatch" ]]; then\n'
        "            python3 scripts/validate_no_shared_rustup_mutation.py \\\n"
        "              --baseline-workflows baseline/.github/workflows\n"
        "          else\n"
        "            python3 scripts/validate_no_shared_rustup_mutation.py\n"
        "          fi"
    ) in workflow


# ── structural YAML forms ────────────────────────────────────────────────────


STRUCTURAL_FORMS = """\
name: structural forms
on: workflow_dispatch
jobs:
  quoted:
    runs-on: "corelink"
    steps:
      - run: echo quoted
  block-sequence:
    runs-on:
      - self-hosted
      - mac
    steps:
      - run: echo block
  inline-sequence:
    runs-on: [self-hosted, mac, corelink-builder]
    steps:
      - run: echo inline
  expression:
    runs-on: ${{ matrix.runner || 'self-hosted' }}
    steps:
      - run: echo expression
"""


def test_all_structural_runs_on_forms_are_inspected(tmp_path: pathlib.Path) -> None:
    """Quoted, block, flow-list, and expression values all count as self-hosted."""
    rc, err = _run_against(tmp_path, STRUCTURAL_FORMS)
    assert rc == 0, err


def test_both_latent_forms_have_teeth(tmp_path: pathlib.Path) -> None:
    content = STRUCTURAL_FORMS.replace(
        "- run: echo quoted", "- uses: dtolnay/rust-toolchain@stable", 1
    ).replace(
        "- run: echo block", "- uses: actions-rs/toolchain@stable", 1
    )
    rc, err = _run_against(tmp_path, content)
    assert rc == 1
    assert err.count("provisions a toolchain") == 2, err
    assert "ZERO self-hosted jobs" not in err


def test_malformed_yaml_is_a_loud_parser_failure(tmp_path: pathlib.Path) -> None:
    malformed = """\
name: malformed
on: workflow_dispatch
jobs:
  broken:
    runs-on: [self-hosted, mac
    steps:
      - run: echo nope
"""
    rc, err = _run_against(tmp_path, malformed)
    assert rc == 2
    assert "parser failure" in err
    assert "ZERO self-hosted jobs" not in err


@pytest.mark.parametrize(
    "content",
    [
        "name: empty\n",
        "name: no runner\non: workflow_dispatch\njobs:\n  no-runner:\n    steps:\n      - run: echo ok\n",
        "name: hosted only\non: workflow_dispatch\njobs:\n  hosted:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n",
    ],
)
def test_empty_or_truncated_population_halts(tmp_path: pathlib.Path, content: str) -> None:
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "ZERO self-hosted jobs" in err


def test_stdlib_fallback_covers_structural_forms(tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """The self-hosted image has no reliable PyYAML; keep the fallback live."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    rc, err = _run_against(tmp_path, STRUCTURAL_FORMS)
    assert rc == 0, err


def test_stdlib_fallback_rejects_malformed_runs_on(tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(vnsrm, "yaml", None)
    malformed = """\
name: malformed
jobs:
  broken:
    runs-on: [self-hosted, mac
    steps:
      - run: echo nope
"""
    rc, err = _run_against(tmp_path, malformed)
    assert rc == 2
    assert "parser failure" in err


def test_stdlib_fallback_rejects_runs_on_aliases(tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """An alias may not make the minimal parser silently lose a self-hosted job."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    anchored = """\
name: anchored
x-runner: &runner corelink
jobs:
  anchored:
    runs-on: *runner
    steps:
      - uses: dtolnay/rust-toolchain@stable
"""
    rc, err = _run_against(tmp_path, anchored)
    assert rc == 2
    assert "anchor/alias" in err


@pytest.mark.parametrize(
    "malformed_line",
    [
        "bad: [",
        "bad: {key: value",
        'bad: "unterminated',
        "bad: key: value",
    ],
)
def test_stdlib_fallback_rejects_global_yaml_shape_errors(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, malformed_line: str
) -> None:
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: malformed global shape
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
{malformed_line}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err


@pytest.mark.parametrize(
    "malformed_line",
    [
        "bad: ]",
        "bad: }",
        'bad: "ok" trailing',
        "bad: 'ok' trailing",
        "bad: {outer: ]",
        'bad: {outer: "ok" trailing}',
        "bad: ['ok' trailing]",
    ],
)
def test_trailing_malformed_yaml_cannot_be_hidden_after_valid_jobs(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, malformed_line: str
) -> None:
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: trailing mutation
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
{malformed_line}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err


@pytest.mark.parametrize(
    "malformed_line",
    [
        'bad: "ok":',
        'bad: "ok",',
        "bad: 'ok':",
        "bad: 'ok',",
        "bad: [ , ]",
        "bad: [,]",
        'bad: {outer: "ok":}',
        "bad: {outer: 'ok':}",
        "bad: [[,]]",
    ],
)
def test_fallback_rejects_quote_delimiters_and_empty_first_items(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, malformed_line: str
) -> None:
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: trailing syntax mutation
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
{malformed_line}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err


FLOW_MAP_MALFORMED = [
    "{, key: value}",
    "{,}",
    "{key: value,,}",
    "{key: value,, other: x}",
    "{key: value, , other: x}",
]

FLOW_MAP_GRAMMAR_MALFORMED = [
    "{: value}",
    "{key: : value}",
    "{key: value, : other}",
    "{[a,]: b}",
]

FLOW_SUFFIX_MALFORMED = [
    "{key:value}trailing",
    "[a,]x",
    "{key:[a,]junk}",
    "{key:value}: trailing",
]

FLOW_MAP_EMPTY_KEY_STRICT = [
    "{:b}",
    "{:??}",
    "{::}",
    "{:foo: bar}",
    "[{:# b}, c]",
]


@pytest.mark.parametrize("flow_map", FLOW_MAP_MALFORMED)
def test_fallback_flow_map_errors_have_teeth(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, flow_map: str
) -> None:
    """Every malformed flow-map comma is a parser failure, not a silent pass."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: malformed flow map
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {flow_map}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err
    assert "ZERO self-hosted jobs" not in err


@pytest.mark.parametrize("flow_map", FLOW_MAP_GRAMMAR_MALFORMED)
def test_fallback_flow_map_grammar_errors_have_teeth(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, flow_map: str
) -> None:
    """Missing map keys/values and flow-sequence keys fail closed."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: malformed flow map grammar
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {flow_map}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err
    assert "ZERO self-hosted jobs" not in err


def test_fallback_flow_map_comma_parity_with_pyyaml(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Fallback and PyYAML agree on malformed maps and valid trailing commas."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    pyyaml = vnsrm.yaml
    for flow_map in FLOW_MAP_MALFORMED:
        content = f"""\
name: malformed flow map
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {flow_map}
"""
        pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
        assert pyyaml_rc == 2, pyyaml_err
        monkeypatch.setattr(vnsrm, "yaml", None)
        fallback_rc, fallback_err = _run_against(tmp_path, content)
        assert fallback_rc == pyyaml_rc, fallback_err
        monkeypatch.setattr(vnsrm, "yaml", pyyaml)

    valid = """\
name: valid flow commas
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
mapping: {key: value,}
sequence: [a,]
"""
    pyyaml_rc, pyyaml_err = _run_against(tmp_path, valid)
    assert pyyaml_rc == 0, pyyaml_err
    monkeypatch.setattr(vnsrm, "yaml", None)
    fallback_rc, fallback_err = _run_against(tmp_path, valid)
    assert fallback_rc == pyyaml_rc, fallback_err


def test_fallback_flow_map_grammar_parity_with_pyyaml(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The grammar guards make the same accept/reject decision as PyYAML."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    pyyaml = vnsrm.yaml
    for flow_map in FLOW_MAP_GRAMMAR_MALFORMED:
        content = f"""\
name: malformed flow map grammar
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {flow_map}
"""
        pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
        assert pyyaml_rc == 2, pyyaml_err
        monkeypatch.setattr(vnsrm, "yaml", None)
        fallback_rc, fallback_err = _run_against(tmp_path, content)
        assert fallback_rc == pyyaml_rc, fallback_err
        monkeypatch.setattr(vnsrm, "yaml", pyyaml)


def test_fallback_flow_map_preserves_nested_and_escaped_valid_forms(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Commas/colons inside quoted values and nested collections are data."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    content = r'''name: valid flow-map escapes
name: valid flow-map escapes
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
quoted: {"key": "value\", tail",}
single-quoted: {'key': 'value,}',}
nested: {key: {inner: value,}, list: [a,],}
commented: {key: value} # a YAML-approved boundary
multiline: {key: [a,]
  , other: b}
'''
    pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
    assert pyyaml_rc == 0, pyyaml_err
    monkeypatch.setattr(vnsrm, "yaml", None)
    fallback_rc, fallback_err = _run_against(tmp_path, content)
    assert fallback_rc == pyyaml_rc, fallback_err


@pytest.mark.parametrize("suffix", FLOW_SUFFIX_MALFORMED)
def test_fallback_rejects_post_flow_suffixes_with_teeth(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, suffix: str
) -> None:
    """A closed flow collection cannot silently absorb same-line suffix text."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: malformed flow suffix
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {suffix}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err
    assert "ZERO self-hosted jobs" not in err


@pytest.mark.parametrize("flow_map", FLOW_MAP_EMPTY_KEY_STRICT)
def test_fallback_rejects_colon_without_map_key_with_teeth(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, flow_map: str
) -> None:
    """A colon cannot start an empty flow-map key, even without a boundary."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = f"""\
name: malformed empty flow-map key
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {flow_map}
"""
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err
    assert "ZERO self-hosted jobs" not in err


def test_fallback_empty_key_strict_parity_with_pyyaml(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The strict empty-key class agrees with PyYAML's reject decision."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    pyyaml = vnsrm.yaml
    for flow_map in FLOW_MAP_EMPTY_KEY_STRICT:
        content = f"""\
name: malformed empty flow-map key
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {flow_map}
"""
        pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
        assert pyyaml_rc == 2, pyyaml_err
        monkeypatch.setattr(vnsrm, "yaml", None)
        fallback_rc, fallback_err = _run_against(tmp_path, content)
        assert fallback_rc == pyyaml_rc, fallback_err
        monkeypatch.setattr(vnsrm, "yaml", pyyaml)


def test_fallback_preserves_compact_keys_urls_and_flow_controls(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Compact keys, value URLs, empty maps, nesting, comments and nulls stay valid."""
    content = """\
name: valid compact flow maps
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
compact: {a:b}
url: {url: http://example.com}
empty: {}
nested: {a: {b: c,},}
null: {a: ,}
comment: {a: b} # YAML-approved comment boundary
"""
    if vnsrm.yaml is not None:
        pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
        assert pyyaml_rc == 0, pyyaml_err
    monkeypatch.setattr(vnsrm, "yaml", None)
    fallback_rc, fallback_err = _run_against(tmp_path, content)
    assert fallback_rc == 0, fallback_err


def test_fallback_post_flow_suffix_parity_with_pyyaml(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Fallback and PyYAML agree that post-flow suffixes are invalid."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    pyyaml = vnsrm.yaml
    for suffix in FLOW_SUFFIX_MALFORMED:
        content = f"""\
name: malformed flow suffix
jobs:
  valid:
    runs-on: corelink
    steps:
      - run: echo ok
bad: {suffix}
"""
        pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
        assert pyyaml_rc == 2, pyyaml_err
        monkeypatch.setattr(vnsrm, "yaml", None)
        fallback_rc, fallback_err = _run_against(tmp_path, content)
        assert fallback_rc == pyyaml_rc, fallback_err
        monkeypatch.setattr(vnsrm, "yaml", pyyaml)


def test_pyyaml_resolves_runs_on_aliases(tmp_path: pathlib.Path) -> None:
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    anchored = """\
name: anchored
x-runner: &runner corelink
jobs:
  anchored:
    runs-on: *runner
    steps:
      - uses: dtolnay/rust-toolchain@stable
"""
    rc, err = _run_against(tmp_path, anchored)
    assert rc == 1
    assert "provisions a toolchain" in err


MATRIX_SELF_HOSTED = """\
name: matrix runner
on: workflow_dispatch
jobs:
  matrix-job:
    strategy:
      matrix:
        runner: [corelink]
    runs-on: ${{ matrix.runner }}
    steps:
      - uses: dtolnay/rust-toolchain@stable
"""


def test_static_matrix_runner_resolves_to_self_hosted(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, MATRIX_SELF_HOSTED)
    assert rc == 1
    assert "provisions a toolchain" in err


def test_stdlib_fallback_treats_dynamic_matrix_runner_conservatively(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(vnsrm, "yaml", None)
    rc, err = _run_against(tmp_path, MATRIX_SELF_HOSTED)
    assert rc == 1
    assert "provisions a toolchain" in err


def test_static_hosted_matrix_runner_is_not_widened(tmp_path: pathlib.Path) -> None:
    hosted = MATRIX_SELF_HOSTED.replace("runner: [corelink]", "runner: [ubuntu-latest]")
    rc, err = _run_against(tmp_path, hosted)
    assert rc == 2
    assert "ZERO self-hosted jobs" in err


def test_unknown_dynamic_runner_is_inspected_conservatively(tmp_path: pathlib.Path) -> None:
    dynamic = MATRIX_SELF_HOSTED.replace("runs-on: ${{ matrix.runner }}", "runs-on: ${{ inputs.runner }}")
    rc, err = _run_against(tmp_path, dynamic)
    assert rc == 1
    assert "provisions a toolchain" in err


ESCAPED_RUNNER_FLOW_STEP = r"""name: escaped runner flow step
on: workflow_dispatch
jobs:
  escaped:
    runs-on: __RUNNER__
    steps: [{uses: dtolnay/rust-toolchain@stable}]
"""


@pytest.mark.parametrize(
    "runner",
    [
        r'"core\u006cink"',
        r'"self-\u0068osted"',
        r'"core\x6cink"',
    ],
)
def test_escaped_self_hosted_runner_and_flow_step_have_parser_parity(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, runner: str
) -> None:
    """Both parsers decode a self-hosted runner before scanning flow-style steps."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    content = ESCAPED_RUNNER_FLOW_STEP.replace("__RUNNER__", runner)
    pyyaml = vnsrm.yaml
    pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
    assert pyyaml_rc == 1, pyyaml_err
    assert "provisions a toolchain" in pyyaml_err

    monkeypatch.setattr(vnsrm, "yaml", None)
    fallback_rc, fallback_err = _run_against(tmp_path, content)
    assert fallback_rc == pyyaml_rc, fallback_err
    assert "provisions a toolchain" in fallback_err
    assert "ZERO self-hosted jobs" not in fallback_err
    monkeypatch.setattr(vnsrm, "yaml", pyyaml)


@pytest.mark.parametrize(
    "steps",
    [
        "steps: [{uses: dtolnay/rust-toolchain@stable}]",
        'steps: [{name: install, uses: "actions-rs/toolchain@stable"}]',
        "steps: [{run: echo clean}, {uses: actions-rust-lang/setup-rust-toolchain@stable}]",
    ],
)
def test_flow_style_steps_have_parser_parity(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch, steps: str
) -> None:
    """A valid YAML flow sequence cannot hide a provisioning action."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    content = f"""\
name: flow step
on: workflow_dispatch
jobs:
  flow:
    runs-on: corelink
    {steps}
"""
    pyyaml = vnsrm.yaml
    pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
    assert pyyaml_rc == 1, pyyaml_err
    assert "provisions a toolchain" in pyyaml_err

    monkeypatch.setattr(vnsrm, "yaml", None)
    fallback_rc, fallback_err = _run_against(tmp_path, content)
    assert fallback_rc == pyyaml_rc, fallback_err
    assert "provisions a toolchain" in fallback_err
    monkeypatch.setattr(vnsrm, "yaml", pyyaml)


def test_flow_run_text_is_not_a_uses_violation(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Flow parsing stays at the step-map key, rather than grepping step text."""
    if vnsrm.yaml is None:
        pytest.skip("PyYAML is not installed")
    content = """\
name: flow text control
on: workflow_dispatch
jobs:
  flow:
    runs-on: corelink
    steps: [{run: "echo 'uses: dtolnay/rust-toolchain@stable'"}]
"""
    pyyaml = vnsrm.yaml
    pyyaml_rc, pyyaml_err = _run_against(tmp_path, content)
    assert pyyaml_rc == 0, pyyaml_err

    monkeypatch.setattr(vnsrm, "yaml", None)
    fallback_rc, fallback_err = _run_against(tmp_path, content)
    assert fallback_rc == pyyaml_rc, fallback_err
    monkeypatch.setattr(vnsrm, "yaml", pyyaml)


def test_fallback_rejects_unknown_double_quote_escapes(tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A fallback that cannot decode a YAML escape must stop, never misclassify."""
    monkeypatch.setattr(vnsrm, "yaml", None)
    content = ESCAPED_RUNNER_FLOW_STEP.replace("__RUNNER__", r'"core\qink"')
    rc, err = _run_against(tmp_path, content)
    assert rc == 2
    assert "parser failure" in err
    assert "unsupported YAML double-quote escape" in err


def test_toolchain_text_inside_run_is_not_a_uses_violation(tmp_path: pathlib.Path) -> None:
    clean = """\
name: text control
on: workflow_dispatch
jobs:
  text:
    runs-on: corelink
    steps:
      - run: "echo 'uses: dtolnay/rust-toolchain@stable'"
"""
    rc, err = _run_against(tmp_path, clean)
    assert rc == 0, err


def test_toolchain_text_inside_multiline_run_is_not_a_uses_violation(tmp_path: pathlib.Path) -> None:
    clean = """\
name: multiline text control
on: workflow_dispatch
jobs:
  text:
    runs-on: corelink
    steps:
      - run: |
          echo "uses: dtolnay/rust-toolchain@stable"
"""
    rc, err = _run_against(tmp_path, clean)
    assert rc == 0, err


def test_toolchain_uses_key_still_has_teeth(tmp_path: pathlib.Path) -> None:
    rc, err = _run_against(tmp_path, PROVISIONING_JOB)
    assert rc == 1
    assert "uses: dtolnay/rust-toolchain@stable" in err


def test_toolchain_path_in_run_remains_banned(tmp_path: pathlib.Path) -> None:
    hardcoded = CLEAN_JOB.replace(
        "run: echo ok", "run: echo $HOME/.rustup/toolchains/1.91.1-x86_64-apple-darwin/bin"
    )
    rc, err = _run_against(tmp_path, hardcoded)
    assert rc == 1
    assert "hardcodes a versioned toolchain path" in err


# ── reach on the live repo ────────────────────────────────────────────────────


def test_live_repo_reach_is_not_vacuous() -> None:
    """Pin the live inspected runner population to a reviewable exact census."""
    import io
    from contextlib import redirect_stdout
    import os

    # #2617 moved the former policy/coverage population to GitHub-hosted runners,
    # so the old 188-job floor no longer describes protected main. Keep the
    # current identities explicit: a job addition, removal, or rename requires
    # deliberate review of the hosted census and this contract. The release
    # matrix is counted conservatively because its nested runner expression is
    # unresolved by the validator, even though its current matrix values are
    # GitHub-hosted.
    expected = {
        ("container-build-push-prod.yml", "build-push"),
        ("okf-autoreconcile.yml", "autoreconcile"),
        ("okf_nightly.yml", "okf-nightly"),
        ("release-cli.yml", "build"),
    }
    workflow_root = REPO_ROOT / vnsrm.WORKFLOWS

    actual = {
        (path.name, job.name)
        for path in workflow_root.glob("*.yml")
        for job in vnsrm._load_jobs(path)
        if vnsrm.job_is_self_hosted(job)
    }
    assert actual == expected, (
        "the self-hosted workflow/job inventory changed; review the current "
        f"runner census before updating this contract: {sorted(actual)!r}"
    )

    buf = io.StringIO()
    cwd = pathlib.Path.cwd()
    os.chdir(REPO_ROOT)
    try:
        with redirect_stdout(buf):
            rc = vnsrm.main()
    finally:
        os.chdir(cwd)
    out = buf.getvalue()
    inspected = int(out.split("self-hosted job(s)")[0].split()[-1])
    assert inspected == len(actual) > 0, f"guard census diverged: {out!r}"
    assert rc == 0, f"live validator reported a violation: {out!r}"

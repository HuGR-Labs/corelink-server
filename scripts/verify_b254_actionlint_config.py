#!/usr/bin/env python3
"""Verify the single actionlint configuration authority and its failure modes.

The repository keeps ``.actionlint.yaml`` as the canonical configuration and
``.github/actionlint.yaml`` as a symlink alias because actionlint's project
discovery prefers the latter path. This gate exercises both explicit paths
and discovery, then uses a synthetic workflow to make removal of *any* real
custom runner label observable. It also checks that missing or ambiguous
configuration entry points fail closed before actionlint is invoked, including
comment-only bait that retains a removed label or configuration variable.
"""

from __future__ import annotations

import os
import re
import signal
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/actionlint.yml"
ROOT_CONFIG = ROOT / ".actionlint.yaml"
GITHUB_CONFIG = ROOT / ".github/actionlint.yaml"
EXPECTED_LABELS = ("mac", "corelink-builder", "ubuntu-x64-4core", "corelink")
ACTIONLINT_TIMEOUT_SECONDS = 30.0
TIMEOUT_RETURN_CODE = 124


def _labels(path: Path) -> tuple[str, ...]:
    """Read the labels under ``self-hosted-runner`` without PyYAML."""

    lines = path.read_text(encoding="utf-8").splitlines()
    in_runner = False
    in_labels = False
    labels: list[str] = []
    for line in lines:
        if line == "self-hosted-runner:":
            in_runner = True
            continue
        if in_runner and line and not line[0].isspace() and not line.startswith("#"):
            break
        if not in_runner:
            continue
        if re.match(r"^\s+labels:\s*$", line):
            in_labels = True
            continue
        if not in_labels:
            continue
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        item = re.match(r"^\s+-\s+([^\s#]+)", line)
        if item:
            labels.append(item.group(1).strip("'\""))
            continue
        break
    return tuple(labels)


def _config_variables(path: Path) -> tuple[str, ...]:
    """Read the closed config-variable population without PyYAML."""

    lines = path.read_text(encoding="utf-8").splitlines()
    in_variables = False
    variables: list[str] = []
    for line in lines:
        if line == "config-variables:":
            in_variables = True
            continue
        if in_variables and line and not line[0].isspace() and not line.startswith("#"):
            break
        if not in_variables or not line.strip() or line.lstrip().startswith("#"):
            continue
        item = re.match(r"^\s+-\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", line)
        if item:
            variables.append(item.group(1))
            continue
        break
    return tuple(variables)


def _workflow_variables() -> tuple[str, ...]:
    """Derive vars.* names from active workflow text, excluding comments."""

    names: set[str] = set()
    for path in sorted((ROOT / ".github/workflows").iterdir()):
        if path.suffix not in {".yml", ".yaml"}:
            continue
        for raw_line in path.read_text(encoding="utf-8").splitlines():
            code = raw_line.split("#", 1)[0]
            names.update(re.findall(r"\bvars\.([A-Za-z_][A-Za-z0-9_]*)", code))
    return tuple(sorted(names))


def _variable_population_error(path: Path, expected: tuple[str, ...]) -> str | None:
    actual = set(_config_variables(path))
    expected_set = set(expected)
    missing = sorted(expected_set - actual)
    extra = sorted(actual - expected_set)
    if not missing and not extra:
        return None
    details: list[str] = []
    if missing:
        details.append(f"missing={missing!r}")
    if extra:
        details.append(f"stale={extra!r}")
    return f"config-variable population diverges ({', '.join(details)})"


def _authority_error(canonical: Path, alias: Path) -> str | None:
    """Return a diagnostic when the alias is missing or not canonical."""

    def display(path: Path) -> str:
        try:
            return str(path.relative_to(ROOT))
        except ValueError:
            return str(path)

    if canonical.is_symlink():
        return f"canonical config must be a regular file: {display(canonical)}"
    if not canonical.is_file():
        return f"missing canonical config: {display(canonical)}"
    if not alias.exists() and not alias.is_symlink():
        return f"missing config alias: {display(alias)}"
    if not alias.is_symlink():
        return f"config entry point is ambiguous: {display(alias)} is not a symlink"
    try:
        resolved_alias = alias.resolve(strict=True)
    except FileNotFoundError:
        return f"missing config alias target: {display(alias)}"
    if resolved_alias != canonical.resolve():
        return (
            "config entry points are ambiguous: "
            f"{display(alias)} does not alias {display(canonical)}"
        )
    return None


def _run_command(*args: str, timeout: float) -> subprocess.CompletedProcess[str]:
    """Run a tool with a hard deadline and terminate its complete process group."""

    process = subprocess.Popen(
        args,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    try:
        output, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            process.kill()
        output, _ = process.communicate()
        output += (
            f"\nTIMEOUT: command exceeded {timeout:.3f}s; "
            "process group terminated\n"
        )
        return subprocess.CompletedProcess(args, TIMEOUT_RETURN_CODE, output)
    return subprocess.CompletedProcess(args, process.returncode, output)


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    return _run_command(*args, timeout=ACTIONLINT_TIMEOUT_SECONDS)


def _fixture(directory: Path) -> Path:
    """Create a tiny workflow that exercises every real custom label."""

    path = directory / "labels.yml"
    jobs = [
        ("mac", "[self-hosted, mac, corelink-builder]"),
        ("corelink_builder", "[self-hosted, mac, corelink-builder]"),
        ("ubuntu_x64_4core", "ubuntu-x64-4core"),
        ("corelink", "corelink"),
    ]
    body = ["name: b254-label-fixture", "on: push", "jobs:"]
    for job, runner in jobs:
        body.extend(
            [
                f"  {job}:",
                f"    runs-on: {runner}",
                "    steps:",
                "      - run: echo b254",
            ]
        )
    path.write_text("\n".join(body) + "\n", encoding="utf-8")
    return path


def _lint_negative_fixture(directory: Path) -> Path:
    """Create invalid permissions and shell logic outside scoped ignore paths."""

    path = directory / "lint-negative-control.yml"
    path.write_text(
        "\n".join(
            [
                "name: b254-lint-negative-control",
                "on: push",
                "permissions:",
                "  vulnerability-alerts: read",
                "  not-a-real-permission: read",
                "jobs:",
                "  negative:",
                "    runs-on: ubuntu-latest",
                "    steps:",
                "      - run: true && false || echo fallback",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    return path


def _expect_clean(actionlint: str, config: Path, workflow: Path, label: str) -> None:
    result = _run(actionlint, "-no-color", "-config-file", str(config), str(workflow))
    if result.returncode != 0:
        print(f"REOPENED: {label} actionlint failed:\n{result.stdout.strip()}", file=sys.stderr)
        raise SystemExit(1)


def main() -> int:
    actionlint = shutil.which("actionlint")
    if actionlint is None:
        print("INDETERMINATE: actionlint is not installed", file=sys.stderr)
        return 2
    if not WORKFLOW.is_file():
        print(f"INDETERMINATE: missing {WORKFLOW.relative_to(ROOT)}", file=sys.stderr)
        return 2
    if error := _authority_error(ROOT_CONFIG, GITHUB_CONFIG):
        print(f"REOPENED: {error}", file=sys.stderr)
        return 1
    if GITHUB_CONFIG.readlink() != Path("../.actionlint.yaml"):
        print("REOPENED: .github/actionlint.yaml is not the required relative alias", file=sys.stderr)
        return 1
    labels = _labels(ROOT_CONFIG)
    if labels != EXPECTED_LABELS:
        print(
            "REOPENED: canonical custom-label population changed; "
            f"expected {EXPECTED_LABELS!r}, got {labels!r}",
            file=sys.stderr,
        )
        return 1
    workflow_variables = _workflow_variables()
    if error := _variable_population_error(ROOT_CONFIG, workflow_variables):
        print(f"REOPENED: {error}", file=sys.stderr)
        return 1

    # Match the workflow's real global invocation exactly: no path and the
    # same verbose/color flags. This is the proof that the canonical config
    # fixes the complete repository, not only the focal B-254 workflow.
    global_result = _run(actionlint, "-color", "-verbose")
    if global_result.returncode != 0:
        print(
            "REOPENED: global actionlint invocation failed:\n"
            + global_result.stdout[-12000:],
            file=sys.stderr,
        )
        return 1

    with tempfile.TemporaryDirectory(prefix="b254-actionlint-") as raw_directory:
        directory = Path(raw_directory)
        fixture = _fixture(directory)
        lint_negative_fixture = _lint_negative_fixture(directory)

        # The actionlint 1.7.12 compatibility exceptions are path-specific.
        # The identical vulnerability-alerts diagnostic must fail outside
        # backlog-verify.yml; unrelated unknown permissions and SC2015 must
        # also remain active.
        negative_result = _run(
            actionlint,
            "-no-color",
            "-config-file",
            str(ROOT_CONFIG),
            str(lint_negative_fixture),
        )
        required_diagnostics = (
            'unknown permission scope "vulnerability-alerts"',
            'unknown permission scope "not-a-real-permission"',
            "shellcheck reported issue in this script: SC2015",
        )
        if negative_result.returncode == 0 or any(
            diagnostic not in negative_result.stdout
            for diagnostic in required_diagnostics
        ):
            print(
                "REOPENED: actionlint compatibility negative control did not "
                "reject all out-of-scope diagnostics:\n"
                + negative_result.stdout.strip(),
                file=sys.stderr,
            )
            return 1

        # Both explicit entry points must validate the same complete label set.
        _expect_clean(actionlint, ROOT_CONFIG, fixture, ".actionlint.yaml")
        _expect_clean(actionlint, GITHUB_CONFIG, fixture, ".github/actionlint.yaml")

        # The normal project-discovery path must resolve the alias and pass too.
        discovered = _run(actionlint, "-no-color", str(WORKFLOW))
        if discovered.returncode != 0:
            print(
                "REOPENED: default actionlint discovery failed:\n"
                + discovered.stdout.strip(),
                file=sys.stderr,
            )
            return 1

        canonical_text = ROOT_CONFIG.read_text(encoding="utf-8")
        for label in EXPECTED_LABELS:
            mutated = directory / f"missing-{label}.yaml"
            mutated.write_text(
                canonical_text.replace(f"    - {label}\n", "", 1),
                encoding="utf-8",
            )
            result = _run(actionlint, "-no-color", "-config-file", str(mutated), str(fixture))
            if result.returncode == 0 or f'label "{label}" is unknown' not in result.stdout:
                print(
                    f"REOPENED: removing label {label!r} did not fail closed:\n"
                    + result.stdout.strip(),
                    file=sys.stderr,
                )
                return 1

        # A comment-only copy of a real label is still absent from the YAML
        # population. Retaining the label as comment bait must not make the
        # fixture pass or disguise actionlint's unknown-label diagnostic.
        comment_bait_label = EXPECTED_LABELS[0]
        comment_bait_label_config = directory / "comment-bait-label.yaml"
        comment_bait_label_config.write_text(
            canonical_text.replace(
                f"    - {comment_bait_label}\n",
                f"    # - {comment_bait_label} # comment bait\n",
                1,
            ),
            encoding="utf-8",
        )
        comment_bait_label_result = _run(
            actionlint,
            "-no-color",
            "-config-file",
            str(comment_bait_label_config),
            str(fixture),
        )
        if (
            comment_bait_label_result.returncode == 0
            or f'label "{comment_bait_label}" is unknown'
            not in comment_bait_label_result.stdout
        ):
            print(
                "REOPENED: comment-only label bait was accepted or produced "
                f"the wrong diagnostic for {comment_bait_label!r}:\n"
                + comment_bait_label_result.stdout.strip(),
                file=sys.stderr,
            )
            return 1

        # Missing and duplicate-but-divergent entry points must be rejected by
        # the authority check, rather than silently selecting one by context.
        missing_alias = directory / "missing-alias.yaml"
        if _authority_error(ROOT_CONFIG, missing_alias) is None:
            print("REOPENED: missing config alias was accepted", file=sys.stderr)
            return 1
        ambiguous_alias = directory / "ambiguous-alias.yaml"
        ambiguous_alias.write_text(canonical_text + "\n# divergent alias\n", encoding="utf-8")
        if _authority_error(ROOT_CONFIG, ambiguous_alias) is None:
            print("REOPENED: ambiguous config alias was accepted", file=sys.stderr)
            return 1

        missing_variable = workflow_variables[0]
        missing_variable_config = directory / "missing-variable.yaml"
        missing_variable_config.write_text(
            canonical_text.replace(f"  - {missing_variable}\n", "", 1),
            encoding="utf-8",
        )
        if _variable_population_error(missing_variable_config, workflow_variables) is None:
            print("REOPENED: missing config-variable mutation was accepted", file=sys.stderr)
            return 1
        missing_variable_result = _run(
            actionlint,
            "-no-color",
            "-config-file",
            str(missing_variable_config),
        )
        if (
            missing_variable_result.returncode == 0
            or f'undefined configuration variable "{missing_variable.lower()}"'
            not in missing_variable_result.stdout
        ):
            print(
                "REOPENED: actionlint did not reject removed config-variable "
                f"{missing_variable!r}:\n{missing_variable_result.stdout[-12000:]}",
                file=sys.stderr,
            )
            return 1

        # Likewise, preserve a real variable only inside a comment. The
        # semantic population check and actionlint must both identify it as
        # missing rather than treating the bait as configuration.
        comment_bait_variable = workflow_variables[0]
        comment_bait_variable_config = directory / "comment-bait-variable.yaml"
        comment_bait_variable_config.write_text(
            canonical_text.replace(
                f"  - {comment_bait_variable}\n",
                f"  # - {comment_bait_variable} # comment bait\n",
                1,
            ),
            encoding="utf-8",
        )
        if _variable_population_error(comment_bait_variable_config, workflow_variables) is None:
            print(
                "REOPENED: comment-only config-variable bait was accepted",
                file=sys.stderr,
            )
            return 1
        comment_bait_variable_result = _run(
            actionlint,
            "-no-color",
            "-config-file",
            str(comment_bait_variable_config),
        )
        if (
            comment_bait_variable_result.returncode == 0
            or f'undefined configuration variable "{comment_bait_variable.lower()}"'
            not in comment_bait_variable_result.stdout
        ):
            print(
                "REOPENED: comment-only variable bait was accepted or produced "
                f"the wrong diagnostic for {comment_bait_variable!r}:\n"
                + comment_bait_variable_result.stdout[-12000:],
                file=sys.stderr,
            )
            return 1

        stale_variable_config = directory / "stale-variable.yaml"
        stale_variable_config.write_text(
            canonical_text + "  - B254_STALE\n",
            encoding="utf-8",
        )
        if _variable_population_error(stale_variable_config, workflow_variables) is None:
            print("REOPENED: stale config-variable mutation was accepted", file=sys.stderr)
            return 1

        external_canonical = directory / "external-canonical.yaml"
        external_canonical.write_text(canonical_text, encoding="utf-8")
        symlink_canonical = directory / "symlink-canonical.yaml"
        symlink_canonical.symlink_to(external_canonical)
        alias_to_symlink = directory / "alias-to-symlink.yaml"
        alias_to_symlink.symlink_to(symlink_canonical)
        if _authority_error(symlink_canonical, alias_to_symlink) is None:
            print("REOPENED: symlink canonical config was accepted", file=sys.stderr)
            return 1

        # A hanging child must return the verifier's timeout diagnosis, not a
        # generic non-zero status. The child also creates a grandchild so the
        # process-group kill is exercised rather than merely killing a parent.
        hanging_child = (
            "import subprocess,sys,time; "
            "subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
            "time.sleep(60)"
        )
        timeout_result = _run_command(
            sys.executable,
            "-c",
            hanging_child,
            timeout=0.05,
        )
        if timeout_result.returncode != TIMEOUT_RETURN_CODE or "TIMEOUT:" not in timeout_result.stdout:
            print(
                "REOPENED: timeout self-test did not prove process-group termination:\n"
                + timeout_result.stdout.strip(),
                file=sys.stderr,
            )
            return 1

    print(
        "PASS: .actionlint.yaml is canonical; .github/actionlint.yaml aliases it; "
        "global/default/explicit actionlint paths pass; label, variable, "
        "missing-config, ambiguous-config, and comment-bait mutations fail closed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

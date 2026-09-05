#!/usr/bin/env python3
"""Fail closed if B-133's Dependabot policy gate loses its trust boundary.

``dependabot-policy.yml`` runs on ``pull_request_target`` and deliberately
checks out a merge ref. From that checkout until the final policy verdict,
every executable step is an attack surface: a local composite action, a script
looked up in the checkout, Cargo configuration, an archive symlink, a Git
configuration hook, or a tool-installer fallback can turn review data into
code execution.

This checker is intentionally a *closed-world* census. It accepts the named
post-checkout steps below only after classifying each one by source. A new or
ambiguous executable step is a named failure, not an invitation to make the
detector broader later. Cargo manifests/lockfiles come from the PR data archive;
``deny.toml`` and every executable script/checker come from the BASE tree.

Run this script from the checkout that owns the workflow. The workflow itself
runs the BASE copy before it reads any PR data.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import stat
import sys
from pathlib import Path
from typing import Any


WF = Path(".github/workflows/dependabot-policy.yml")
TEETH_WF = Path(".github/workflows/dependabot-policy-trust-boundary.yml")
TRUSTED = "_base"
UNTRUSTED = "_pr-data"
POLICY_TREE = "${{ runner.temp }}/corelink-dependabot-policy-tree"
POLICY_HOME = "${{ runner.temp }}/corelink-dependabot-policy-home"
CARGO_HOME = "${{ runner.temp }}/corelink-dependabot-cargo-home"
DENY_CONFIG = "${{ runner.temp }}/corelink-dependabot-deny.toml"
POLICY_ARCHIVE = "${{ runner.temp }}/corelink-dependabot-policy-tree.tar"
CARGO_DENY_BIN = "${{ runner.temp }}/corelink-dependabot-policy-home/.install-action/bin/cargo-deny"

CHECKOUT_SHA = "9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0"
METADATA_SHA = "25dd0e34f4fe68f24cc83900b1fe3fe149efef98"
INSTALL_SHA = "07b4745e0c39a41822af610387492e3e53aa222b"
REQUIRED_TRIGGER_TYPES = {"opened", "synchronize", "reopened", "labeled", "ready_for_review"}
REQUIRED_TEETH_TRIGGER_TYPES = {"opened", "synchronize", "reopened"}
REQUIRED_TEETH_PATHS = {
    ".github/workflows/dependabot-policy-trust-boundary.yml",
    ".github/workflows/dependabot-policy.yml",
    "scripts/check_dependabot_policy_trusted_tree.py",
    "scripts/test_dependabot_policy_trust_boundary.sh",
    "scripts/test_ci_use_host_toolchain.sh",
    "scripts/ci-use-host-toolchain.sh",
    "rust-toolchain.toml",
}
REQUIRED_TEETH_ORDER = (
    "Checkout BASE control tree (SHA-pinned)",
    "Checkout PR merge data for static inspection (SHA-pinned)",
    "Statically reject B-133 control mutations (BASE checker)",
    "Install cargo-deny for BASE trust-boundary teeth (SHA-pinned)",
    "Prove host-toolchain channel guard (BASE tree)",
    "Prove Dependabot trust-boundary teeth (BASE tree)",
)
REQUIRED_TEETH_STATIC_ENV = {
    "B133_UNTRUSTED_TREE": "${{ github.workspace }}/_pr-data",
}
DEPENDABOT_PR_AUTHOR_IF = "github.event.pull_request.user.login == 'dependabot[bot]'"
REQUIRED_AUDIT_ENV = {
    "AUDIT_PR_NUMBER": "${{ github.event.pull_request.number }}",
    "AUDIT_ECOSYSTEM": "${{ steps.metadata.outputs.package-ecosystem }}",
    "AUDIT_DEPENDENCY_NAMES": "${{ steps.metadata.outputs.dependency-names }}",
    "AUDIT_UPDATE_TYPE": "${{ steps.metadata.outputs.update-type }}",
    "AUDIT_HEAD_SHA": "${{ github.event.pull_request.head.sha }}",
}

# An interpreter-token regex is not a shell parser: a token hidden after
# `then`, an assignment prefix, a command substitution, or future shell syntax
# can evade it. The policy job is deliberately small and closed-world, so bind
# every inline `run:` body to its reviewed exact form instead. A future change
# must consciously reclassify its entire command block and update this digest;
# otherwise the BASE checker halts before the altered block executes.
RUN_BLOCK_SHA256 = {
    "Assert trust-boundary wiring (BASE checker)": "10e6a406ed71e7aaf0b9d55975532fd53aa254ae5a01a5ba022e1d1686b6e398",
    "Prepare isolated Cargo policy tree (PR data only)": "bb6e35ae2ba2b5fe7baf6575e78e1705969ea2169a30134bc8f39ab148ede642",
    "Use the workspace-pinned host toolchain (from the BASE tree; provisions nothing)": "1f8e71a94504feb3273f28fd86276e73ed342de3170ed3d83a6376afde7dcbd2",
    "Run cargo-deny licenses (fail-closed)": "0008c8b10f84c2cce4929b19466b6e58b31d8e564e2ac48b9690f7424b3b9e13",
    "Banned-license signature scan (Cargo.lock)": "ef86af926b2f5c6aea05df49e5a2c85cf56045f8f50dc5345bd594a93c948160",
    "npm banned-license scan": "1dcf8a4b63892e3381f469e0873f55840e17c10c9c8e9189249b6cc8634da018",
    "Forbid skip-hook / skip-ci flags": "7efff67a71df31c905f6547701878a3dad5f73b4c9830fb55e72d9de520236e9",
    "Forbid governance-file modifications": "1435c5d5bead77fead596abb1a9bc77962eab321b7470f164d590e85767d4baa",
    "Verify required checks are present": "36e5552ba1901a2229608e75d1e1dfe009f3f04b0a15d86dc6fbd1b47750c673",
    "Policy-gate audit log": "c9df5e449c82c56aa0f8c274be456dc3988f4c63d5c0e1d4d516b3e44cf342b6",
}

# Steps after the PR checkout are execution-bearing. This list is deliberately
# exact: a future change must add a source classification and tests for it.
POST_CHECKOUT_ORDER = (
    "Checkout PR merge ref (SHA-pinned)",
    "Checkout BASE ref into _base (trusted tree; SHA-pinned)",
    "Assert trust-boundary wiring (BASE checker)",
    "Prepare isolated Cargo policy tree (PR data only)",
    "Fetch Dependabot metadata",
    "Use the workspace-pinned host toolchain (from the BASE tree; provisions nothing)",
    "Install cargo-deny (SHA-pinned)",
    "Run cargo-deny licenses (fail-closed)",
    "Banned-license signature scan (Cargo.lock)",
    "npm banned-license scan",
    "Forbid skip-hook / skip-ci flags",
    "Forbid governance-file modifications",
    "Verify required checks are present",
    "Policy-gate audit log",
)

REQUIRED_JOB_ENV = {
    "BASE_REF": "${{ github.event.pull_request.base.ref }}",
    "CARGO_BUILD_RUSTC_WRAPPER": "",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER": "",
    "CARGO_BUILD_RUSTFLAGS": "",
    "RUSTC_WRAPPER": "",
    "RUSTC_WORKSPACE_WRAPPER": "",
    "RUSTC": "rustc",
    "RUSTFLAGS": "",
    "CARGO_NET_GIT_FETCH_WITH_CLI": "false",
    "CARGO": "cargo",
    "CARGO_REGISTRY_TOKEN": "",
    "CARGO_REGISTRIES_CRATES_IO_TOKEN": "",
    "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS": "",
    "CARGO_REGISTRIES_CRATES_IO_CREDENTIAL_PROVIDER": "",
    "CARGO_ALIAS_DENY": "",
    "BASH_ENV": "/dev/null",
    "ENV": "/dev/null",
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_CONFIG_COUNT": "0",
    "GIT_CONFIG_KEY_0": "",
    "GIT_CONFIG_VALUE_0": "",
    "GIT_CONFIG_PARAMETERS": "",
    "GIT_EXTERNAL_DIFF": "/bin/false",
    "GIT_PAGER": "cat",
    "GIT_TERMINAL_PROMPT": "0",
    "GIT_ASKPASS": "/bin/false",
    "GIT_SSH_COMMAND": "ssh -F /dev/null -oBatchMode=yes -oIdentitiesOnly=yes",
}

# This is an allowlist of the *step-level* environments, not a loose check for
# a handful of variables.  Job env is inherited by every step, and an override
# on even a `uses:` step can become executable before its action runs
# (`NODE_OPTIONS`, `BASH_ENV`, `PATH`, `CARGO`, credential providers, Git's
# external-diff hooks, ...).  Keep each step's local additions exact so adding
# any executable sink is a deliberate review event rather than an invisible
# YAML tweak.
POST_CHECKOUT_STEP_ENV = {
    "Checkout PR merge ref (SHA-pinned)": {},
    "Checkout BASE ref into _base (trusted tree; SHA-pinned)": {},
    "Assert trust-boundary wiring (BASE checker)": {},
    "Prepare isolated Cargo policy tree (PR data only)": {
        "POLICY_TREE": POLICY_TREE,
        "POLICY_HOME": POLICY_HOME,
        "HOME": POLICY_HOME,
        "CARGO_HOME": CARGO_HOME,
        "DENY_CONFIG": DENY_CONFIG,
        "POLICY_ARCHIVE": POLICY_ARCHIVE,
    },
    "Fetch Dependabot metadata": {},
    "Use the workspace-pinned host toolchain (from the BASE tree; provisions nothing)": {
        "HOST_TRIPLE": "x86_64-unknown-linux-gnu",
    },
    "Install cargo-deny (SHA-pinned)": {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME},
    "Run cargo-deny licenses (fail-closed)": {
        "POLICY_TREE": POLICY_TREE,
        "HOME": POLICY_HOME,
        "CARGO_HOME": CARGO_HOME,
        "DENY_CONFIG": DENY_CONFIG,
        "CARGO_DENY_BIN": CARGO_DENY_BIN,
    },
    "Banned-license signature scan (Cargo.lock)": {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME},
    "npm banned-license scan": {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME},
    "Forbid skip-hook / skip-ci flags": {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME},
    "Forbid governance-file modifications": {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME},
    "Verify required checks are present": {
        "HOME": POLICY_HOME,
        "CARGO_HOME": CARGO_HOME,
        "GH_TOKEN": "${{ secrets.GITHUB_TOKEN }}",
        "PR_HEAD_SHA": "${{ github.event.pull_request.head.sha }}",
        "REPO": "${{ github.repository }}",
    },
    "Policy-gate audit log": REQUIRED_AUDIT_ENV,
}

# The reviewed workflow uses GitHub's default shell for its inline blocks and
# no shell at all for `uses:` actions.  `shell:` is executable configuration:
# a PR-controlled custom template changes how an otherwise byte-identical run
# block is interpreted.  Absence is therefore the one allowlisted value today.
POST_CHECKOUT_SHELL = {name: None for name in POST_CHECKOUT_ORDER}

RUST_COMMAND = re.compile(
    r"^(?:\$\(\s*)?(?:if\s+|then\s+|do\s+)?(?:!\s+)?(?:env\s+)?"
    r"(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+)*(?:command\s+)?(?P<tool>cargo|rustc|rustup)\b"
)
GIT_COMMAND = re.compile(r"(?:^|[\s(<])git\s+(?P<subcommand>[A-Za-z-]+)\b")


def fail(message: str) -> int:
    print(message)
    return 1


def value(mapping: dict[str, Any], key: str) -> str:
    if key not in mapping:
        return "<MISSING>"
    return str(mapping[key])


def shell_commands(run: str):
    """Yield shell fragments, dropping comments but retaining substitutions."""
    for line in run.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        for segment in re.split(r"&&|\|\||[;&|]", stripped):
            segment = segment.strip()
            if segment:
                yield segment


def named_step(steps: list[dict[str, Any]], name: str, problems: list[str]) -> dict[str, Any]:
    matches = [step for step in steps if step.get("name") == name]
    if len(matches) != 1:
        problems.append(f"step obrigatório {name!r}: encontrado {len(matches)}, esperado exatamente 1")
        return {}
    return matches[0]


def check_exact_env(
    step: dict[str, Any],
    expected: dict[str, str],
    label: str,
    problems: list[str],
    *,
    reject_extras: bool = False,
) -> None:
    # Empty-but-wrong YAML types (`env: []`, `env: false`) must not collapse
    # into `{}` through truthiness: closed-world means a missing/null map is
    # acceptable, every other type is a named failure.
    env = step.get("env", {})
    if env is None:
        env = {}
    if not isinstance(env, dict):
        problems.append(f"{label}: env não é um mapa; a população de variáveis é ambígua")
        return
    for key, wanted in expected.items():
        actual = value(env, key)
        if actual != wanted:
            problems.append(f"{label}: {key}={actual!r} (esperado {wanted!r})")
    if reject_extras:
        extras = sorted(set(env) - set(expected))
        if extras:
            problems.append(f"{label}: env extras não permitidos: {', '.join(extras)}")


def check_no_inherited_run_config(scope: dict[str, Any], label: str, problems: list[str]) -> None:
    """Refuse workflow/job defaults that execute before a step's own contract.

    A checked `run:` digest and a checked step `shell:` do not describe the
    program GitHub actually invokes when an ancestor sets
    `defaults.run.shell`.  Keep the current policy explicit: neither the
    workflow nor policy-gate has a default interpreter.  Any future default is
    an execution-source change and must be designed, allowlisted, and tested.
    """
    defaults = scope.get("defaults")
    if defaults in (None, {}):
        return
    if not isinstance(defaults, dict):
        problems.append(f"{label}: defaults não é mapa; configuração herdada é ambígua")
        return
    run = defaults.get("run")
    if isinstance(run, dict) and "shell" in run:
        problems.append(
            f"{label}: defaults.run.shell={run.get('shell')!r} não está no allowlist; "
            "um interpretador herdado altera cada run pós-checkout"
        )
    else:
        problems.append(f"{label}: defaults não está no allowlist fechado")


def check_triggers(wf: dict[str, Any], problems: list[str]) -> None:
    # PyYAML 1.1 parses `on` as boolean True.
    triggers = wf.get("on", wf.get(True))
    if not isinstance(triggers, dict):
        problems.append("gatilho pull_request_target ausente ou não é um mapa")
        return
    target = triggers.get("pull_request_target")
    if not isinstance(target, dict):
        problems.append("gatilho pull_request_target ausente — o gate perdeu seu objeto")
        return
    actual_types = set(target.get("types") or [])
    missing = sorted(REQUIRED_TRIGGER_TYPES - actual_types)
    if missing:
        problems.append(
            "gatilho pull_request_target não se reexecuta para alterações de PR: "
            f"faltam types {', '.join(missing)}"
        )
    # No path filter is the only unambiguous self-trigger contract here. It
    # makes changes in the workflow/checker/test eligible for the gate rather
    # than relying on glob coverage that can silently omit a future control.
    for forbidden in ("paths", "paths-ignore"):
        if forbidden in target:
            problems.append(
                f"gatilho pull_request_target tem {forbidden}; B-133 exige sem filtro de paths "
                "para que workflow/checker/test continuem auto-observáveis"
            )


def check_premerge_teeth_workflow(yaml: Any, problems: list[str]) -> None:
    """Prove that the B-133 teeth execute BASE code and inspect PR bytes only.

    The documented ``corelink`` pool is mixed and includes a shared-home
    builder; its label cannot turn a regular pull-request job into an isolated
    executor. The only safe contract currently available is therefore a
    ``pull_request_target`` workflow whose definition and executable paths are
    selected from the BASE revision. ``_pr-data`` is a data-only checkout: any
    B-133 control byte that differs there is a red result, not something the
    host executes to discover whether it is safe.
    """
    if not TEETH_WF.is_file():
        problems.append(
            f"premerge B-133: {TEETH_WF} ausente; checker/teeth só rodariam após merge"
        )
        return
    try:
        teeth = yaml.safe_load(TEETH_WF.read_text())
    except yaml.YAMLError as exc:
        problems.append(f"premerge B-133: workflow não é YAML válido: {exc}")
        return
    if not isinstance(teeth, dict):
        problems.append("premerge B-133: workflow não é um mapa")
        return
    triggers = teeth.get("on", teeth.get(True))
    if not isinstance(triggers, dict):
        problems.append("premerge B-133: gatilhos ausentes ou ambíguos")
        return
    if "pull_request" in triggers:
        problems.append("premerge B-133: teeth não podem executar pull_request em runner compartilhado")
    pull_request_target = triggers.get("pull_request_target")
    if not isinstance(pull_request_target, dict):
        problems.append("premerge B-133: gatilho pull_request_target BASE-controlado ausente")
        return
    actual_types = set(pull_request_target.get("types") or [])
    if actual_types != REQUIRED_TEETH_TRIGGER_TYPES:
        problems.append(
            "premerge B-133: types pull_request_target divergentes "
            f"({sorted(actual_types)!r}, esperado {sorted(REQUIRED_TEETH_TRIGGER_TYPES)!r})"
        )
    paths = pull_request_target.get("paths")
    if not isinstance(paths, list):
        problems.append("premerge B-133: paths do pull_request_target ausentes; população não é exata")
    else:
        actual_paths = set(map(str, paths))
        missing = sorted(REQUIRED_TEETH_PATHS - actual_paths)
        extras = sorted(actual_paths - REQUIRED_TEETH_PATHS)
        if missing or extras or len(paths) != len(actual_paths):
            problems.append(
                "premerge B-133: paths do pull_request_target divergentes "
                f"(faltando={missing!r}, extras={extras!r}, total={len(paths)})"
            )
    for forbidden in ("paths-ignore", "branches", "branches-ignore"):
        if forbidden in pull_request_target:
            problems.append(f"premerge B-133: pull_request_target não pode ter {forbidden}")

    if teeth.get("permissions") != {"contents": "read"}:
        problems.append("premerge B-133: permissões devem ser exatamente contents: read")
    check_exact_env(teeth, {}, "premerge B-133: env do workflow", problems, reject_extras=True)
    check_no_inherited_run_config(teeth, "premerge B-133: workflow", problems)
    jobs = teeth.get("jobs") or {}
    job = jobs.get("trust-boundary-teeth") if isinstance(jobs, dict) else None
    if not isinstance(job, dict):
        problems.append("premerge B-133: job trust-boundary-teeth ausente")
        return
    check_exact_env(job, {}, "premerge B-133: env do job", problems, reject_extras=True)
    check_no_inherited_run_config(job, "premerge B-133: job", problems)
    if job.get("runs-on") != "corelink":
        problems.append("premerge B-133: este desenho BASE-only exige o label documentado corelink")
    if job.get("timeout-minutes") != 10:
        problems.append("premerge B-133: timeout-minutes deve ser 10")
    steps = job.get("steps") or []
    if not isinstance(steps, list):
        problems.append("premerge B-133: steps do job não são uma lista")
        return
    names = tuple(str(step.get("name", "")) for step in steps if isinstance(step, dict))
    if names != REQUIRED_TEETH_ORDER:
        problems.append("premerge B-133: censo/ordem fechada dos steps BASE-only divergiu")

    base_checkout = named_step(steps, REQUIRED_TEETH_ORDER[0], problems)
    pr_checkout = named_step(steps, REQUIRED_TEETH_ORDER[1], problems)
    for label, checkout in (("BASE", base_checkout), ("dados PR", pr_checkout)):
        if not checkout:
            continue
        if checkout.get("uses") != f"actions/checkout@{CHECKOUT_SHA}":
            problems.append(f"premerge B-133: checkout {label} não está no SHA confiável")
        if value(checkout.get("with") or {}, "persist-credentials") != "False":
            problems.append(f"premerge B-133: checkout {label} deve persist-credentials false")
        check_exact_env(checkout, {}, f"premerge B-133: checkout {label}", problems, reject_extras=True)
        if "shell" in checkout:
            problems.append(f"premerge B-133: checkout {label} não pode definir shell")
    if base_checkout:
        base_with = base_checkout.get("with") or {}
        if value(base_with, "path") != TRUSTED or "base.sha" not in str(base_with.get("ref", "")):
            problems.append("premerge B-133: checkout BASE deve usar base.sha em _base")
    if pr_checkout:
        pr_with = pr_checkout.get("with") or {}
        if value(pr_with, "path") != UNTRUSTED:
            problems.append("premerge B-133: checkout de dados PR deve usar _pr-data")
        ref = str(pr_with.get("ref", ""))
        if "refs/pull/" not in ref or "/merge" not in ref:
            problems.append("premerge B-133: checkout de dados PR deve apontar ao merge ref")

    static = named_step(steps, REQUIRED_TEETH_ORDER[2], problems)
    if static:
        if static.get("working-directory") != TRUSTED:
            problems.append("premerge B-133: checker estático deve rodar de _base")
        if str(static.get("run", "")).strip() != (
            'python3 scripts/check_dependabot_policy_trusted_tree.py --untrusted-tree "$B133_UNTRUSTED_TREE"'
        ):
            problems.append("premerge B-133: checker estático não usa o comando BASE exato")
        check_exact_env(
            static,
            REQUIRED_TEETH_STATIC_ENV,
            "premerge B-133: checker estático",
            problems,
            reject_extras=True,
        )
        if "shell" in static:
            problems.append("premerge B-133: checker estático não pode definir shell")

    install = named_step(steps, REQUIRED_TEETH_ORDER[3], problems)
    if install:
        if install.get("uses") != f"taiki-e/install-action@{INSTALL_SHA}":
            problems.append("premerge B-133: instalação das teeth não está no SHA confiável")
        with_ = install.get("with") or {}
        if value(with_, "tool") != "cargo-deny@0.19.8" or value(with_, "fallback") != "none":
            problems.append("premerge B-133: cargo-deny das teeth deve ser 0.19.8 sem fallback")
        check_exact_env(
            install,
            {
                "HOME": "${{ runner.temp }}/corelink-b133-teeth-home",
                "CARGO_HOME": "${{ runner.temp }}/corelink-b133-teeth-cargo-home",
            },
            "premerge B-133: instalação cargo-deny",
            problems,
            reject_extras=True,
        )
        if "shell" in install:
            problems.append("premerge B-133: instalação cargo-deny não pode definir shell")
    for name, command in (
        ("Prove host-toolchain channel guard (BASE tree)", "bash scripts/test_ci_use_host_toolchain.sh"),
        ("Prove Dependabot trust-boundary teeth (BASE tree)", "bash scripts/test_dependabot_policy_trust_boundary.sh"),
    ):
        step = named_step(steps, name, problems)
        if step:
            if step.get("working-directory") != TRUSTED:
                problems.append(f"premerge B-133: {name!r} deve rodar de _base")
            if str(step.get("run", "")).strip() != command:
                problems.append(f"premerge B-133: {name!r} deve executar exatamente {command!r}")
            check_exact_env(step, {}, f"premerge B-133: {name!r}", problems, reject_extras=True)
            if "shell" in step:
                problems.append(f"premerge B-133: {name!r} não pode definir shell")


def check_checkout_steps(steps: list[dict[str, Any]], problems: list[str]) -> None:
    pr = named_step(steps, POST_CHECKOUT_ORDER[0], problems)
    base = named_step(steps, POST_CHECKOUT_ORDER[1], problems)
    for label, step in (("checkout PR", pr), ("checkout BASE", base)):
        if not step:
            continue
        if step.get("uses") != f"actions/checkout@{CHECKOUT_SHA}":
            problems.append(f"{label}: actions/checkout precisa do SHA confiável fixo")
        with_ = step.get("with") or {}
        if value(with_, "persist-credentials") != "False":
            problems.append(f"{label}: persist-credentials deve ser false")
    if pr:
        ref = str((pr.get("with") or {}).get("ref", ""))
        if "refs/pull/" not in ref or "/merge" not in ref:
            problems.append("checkout PR não aponta ao merge ref do PR")
    if base:
        with_ = base.get("with") or {}
        if value(with_, "path") != TRUSTED or "base.sha" not in str(with_.get("ref", "")):
            problems.append("checkout BASE não usa base.sha em _base")
        sparse = str(with_.get("sparse-checkout", ""))
        for required in (
            ".github/workflows/dependabot-policy.yml",
            ".github/workflows/dependabot-policy-trust-boundary.yml",
            "scripts/ci-use-host-toolchain.sh",
            "scripts/check_dependabot_policy_trusted_tree.py",
            "rust-toolchain.toml",
            "deny.toml",
        ):
            if required not in sparse:
                problems.append(f"checkout BASE sparse não contém {required!r}")


def check_policy_steps(steps: list[dict[str, Any]], problems: list[str]) -> None:
    checker = named_step(steps, "Assert trust-boundary wiring (BASE checker)", problems)
    if checker:
        if checker.get("working-directory") != TRUSTED:
            problems.append("checker de fronteira deve rodar de _base, nunca da árvore do PR")
        if "python3 scripts/check_dependabot_policy_trusted_tree.py" not in str(checker.get("run", "")):
            problems.append("checker BASE não executa scripts/check_dependabot_policy_trusted_tree.py")

    prep = named_step(steps, "Prepare isolated Cargo policy tree (PR data only)", problems)
    if prep:
        if prep.get("if") is not None:
            problems.append("preparo de Cargo não pode ter if/hashFiles: manifest removido deve falhar fechado")
        check_exact_env(
            prep,
            {
                "POLICY_TREE": POLICY_TREE,
                "POLICY_HOME": POLICY_HOME,
                "HOME": POLICY_HOME,
                "CARGO_HOME": CARGO_HOME,
                "DENY_CONFIG": DENY_CONFIG,
                "POLICY_ARCHIVE": POLICY_ARCHIVE,
            },
            "preparo de Cargo",
            problems,
        )
        run = str(prep.get("run", ""))
        for marker in (
            "git ls-tree -r --full-tree HEAD",
            '"120000" || $1 == "160000"',
            'git archive --format=tar --output="$POLICY_ARCHIVE" HEAD',
            'tar -x -f "$POLICY_ARCHIVE" -C "$POLICY_TREE"',
            "for manifest in Cargo.toml Cargo.lock deny.toml; do",
            '[ ! -f "$POLICY_TREE/$manifest" ] || [ -L "$POLICY_TREE/$manifest" ]',
            'rm -rf "$POLICY_TREE/.cargo"',
            'rm -f "$POLICY_TREE/rust-toolchain" "$POLICY_TREE/rust-toolchain.toml"',
            '[ ! -f _base/deny.toml ] || [ -L _base/deny.toml ]',
            'cp _base/deny.toml "$DENY_CONFIG"',
            # Keep the trailing action delimiter in the sentinel. The awk
            # substitution below uses the same TOML key pattern, but is not
            # itself the selection loop; accepting it as evidence would let a
            # mutation remove the preflight while this helper text still
            # satisfied the checker.
            '^[[:space:]]*source[[:space:]]*=[[:space:]]*/ {',
            'Refusing Cargo.lock with non-crates.io source before Cargo/network',
        ):
            if marker not in run:
                problems.append(f"preparo de Cargo sem controle obrigatório: {marker}")

    host = named_step(
        steps,
        "Use the workspace-pinned host toolchain (from the BASE tree; provisions nothing)",
        problems,
    )
    if host:
        if host.get("if") is not None:
            problems.append("assertiva de toolchain BASE não pode ser pulada por hashFiles")
        if host.get("working-directory") != TRUSTED:
            problems.append("host toolchain deve usar working-directory: _base")
        if str(host.get("run", "")).strip() != "bash scripts/ci-use-host-toolchain.sh":
            problems.append("host toolchain deve invocar somente o script confiável de _base")

    install = named_step(steps, "Install cargo-deny (SHA-pinned)", problems)
    if install:
        if install.get("if") is not None:
            problems.append("instalação cargo-deny não pode ser pulada por hashFiles")
        if install.get("uses") != f"taiki-e/install-action@{INSTALL_SHA}":
            problems.append("cargo-deny deve usar a ação taiki-e no SHA fixo")
        with_ = install.get("with") or {}
        if value(with_, "tool") != "cargo-deny@0.19.8":
            problems.append("instalação cargo-deny perdeu a versão fixa 0.19.8")
        if value(with_, "fallback") != "none":
            problems.append("instalação cargo-deny: fallback deve ser 'none' (nunca cargo install do PR)")
        check_exact_env(
            install,
            {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME},
            "instalação cargo-deny isolada",
            problems,
        )

    cargo = named_step(steps, "Run cargo-deny licenses (fail-closed)", problems)
    if cargo:
        if cargo.get("if") is not None:
            problems.append("cargo-deny não pode ser pulado por hashFiles: input ausente deve falhar")
        if cargo.get("working-directory") != POLICY_TREE:
            problems.append("cargo-deny deve rodar somente em POLICY_TREE")
        check_exact_env(
            cargo,
            {
                "POLICY_TREE": POLICY_TREE,
                "HOME": POLICY_HOME,
                "CARGO_HOME": CARGO_HOME,
                "DENY_CONFIG": DENY_CONFIG,
                "CARGO_DENY_BIN": CARGO_DENY_BIN,
            },
            "cargo-deny isolado",
            problems,
        )
        run = str(cargo.get("run", ""))
        if '"$CARGO_DENY_BIN" --all-features --locked check --config "$DENY_CONFIG" sources licenses bans' not in run:
            problems.append(
                "cargo-deny deve invocar o binário confiável direto, com --locked e "
                "check --config \"$DENY_CONFIG\" sources licenses bans"
            )

    scan = named_step(steps, "Banned-license signature scan (Cargo.lock)", problems)
    if scan:
        if scan.get("if") is not None:
            problems.append("scan Cargo.lock não pode ser pulado por hashFiles")
        check_exact_env(scan, {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME}, "scan Cargo.lock isolado", problems)

    npm = named_step(steps, "npm banned-license scan", problems)
    if npm:
        check_exact_env(npm, {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME}, "scan npm isolado", problems)
        npm_run = str(npm.get("run", ""))
        for marker in ('mapfile -t changed_npm', 'grep -E "\\"license\\".*${banned_regex}" -- "$f"'):
            if marker not in npm_run:
                problems.append(f"scan npm sem iteração de filenames segura: {marker}")

    for name in (
        "Forbid skip-hook / skip-ci flags",
        "Forbid governance-file modifications",
        "Verify required checks are present",
    ):
        step = named_step(steps, name, problems)
        if step:
            check_exact_env(step, {"HOME": POLICY_HOME, "CARGO_HOME": CARGO_HOME}, f"{name} isolado", problems)

    for name in ("npm banned-license scan", "Forbid governance-file modifications", "Verify required checks are present"):
        step = named_step(steps, name, problems)
        if step and "git diff --no-ext-diff" not in str(step.get("run", "")):
            problems.append(f"{name}: git diff deve usar --no-ext-diff (GIT_EXTERNAL_DIFF é um sink)")

    audit = named_step(steps, "Policy-gate audit log", problems)
    if audit:
        check_exact_env(audit, REQUIRED_AUDIT_ENV, "audit log", problems)


def check_execution_census(steps: list[dict[str, Any]], problems: list[str]) -> list[str]:
    """Close the world of action/script/interpreter and Cargo/Git sinks."""
    classifications: list[str] = []
    pr_index = next(
        (i for i, step in enumerate(steps) if step.get("name") == POST_CHECKOUT_ORDER[0]),
        None,
    )
    if pr_index is None:
        problems.append("censo: checkout PR ausente; não há fronteira a provar")
        return classifications
    post = steps[pr_index:]
    names = [str(step.get("name", "")) for step in post]
    if tuple(names) != POST_CHECKOUT_ORDER:
        problems.append(
            "censo fechado de steps pós-checkout divergiu; cada novo/excluído/reordenado passo "
            "deve receber classificação explícita de origem"
        )

    expected_actions = {
        POST_CHECKOUT_ORDER[0]: f"actions/checkout@{CHECKOUT_SHA}",
        POST_CHECKOUT_ORDER[1]: f"actions/checkout@{CHECKOUT_SHA}",
        "Fetch Dependabot metadata": f"dependabot/fetch-metadata@{METADATA_SHA}",
        "Install cargo-deny (SHA-pinned)": f"taiki-e/install-action@{INSTALL_SHA}",
    }
    for step in post:
        name = str(step.get("name", "<sem nome>"))
        uses = step.get("uses")
        run = step.get("run")
        wd = str(step.get("working-directory") or "")
        if name not in POST_CHECKOUT_STEP_ENV:
            problems.append(f"censo {name!r}: env não tem allowlist fechada")
        else:
            check_exact_env(
                step,
                POST_CHECKOUT_STEP_ENV[name],
                f"censo {name!r}: env",
                problems,
                reject_extras=True,
            )
        expected_shell = POST_CHECKOUT_SHELL.get(name, object())
        if expected_shell is not None:
            problems.append(f"censo {name!r}: shell allowlist interno inválido")
        elif "shell" in step:
            problems.append(
                f"censo {name!r}: shell={step.get('shell')!r} não está no allowlist; "
                "todo interpretador pós-checkout deve ter classificação explícita"
            )
        if uses:
            uses = str(uses)
            if uses.startswith("./"):
                problems.append(f"censo {name!r}: ação local/composite {uses!r} vem da árvore do PR")
                continue
            if "@" not in uses or not re.search(r"@[0-9a-f]{40}$", uses):
                problems.append(f"censo {name!r}: ação não está presa a SHA de 40 hex: {uses!r}")
                continue
            if uses != expected_actions.get(name):
                problems.append(f"censo {name!r}: ação inesperada {uses!r}; origem confiável não classificada")
            else:
                classifications.append(f"{name}: ação SHA-pinned ({uses.split('@')[0]}), shell ausente allowlisted")
            continue

        if not isinstance(run, str):
            problems.append(f"censo {name!r}: step sem uses/run; origem executável ambígua")
            continue

        expected_run = RUN_BLOCK_SHA256.get(name)
        actual_run = hashlib.sha256(run.encode()).hexdigest()
        if expected_run is None:
            problems.append(
                f"censo {name!r}: run inline não tem hash no allowlist fechado; "
                "classifique a fonte antes de permitir execução"
            )
        elif actual_run != expected_run:
            problems.append(
                f"censo {name!r}: corpo run divergiu do allowlist exato "
                f"({actual_run[:12]} != {expected_run[:12]}); um interpretador/script pode "
                "estar escondido em sintaxe shell — reclassifique antes de executar"
            )

        rust_tools: list[str] = []
        git_subcommands: list[str] = []
        for command in shell_commands(run):
            rust = RUST_COMMAND.match(command)
            if rust:
                rust_tools.append(rust.group("tool"))
            git_subcommands.extend(match.group("subcommand") for match in GIT_COMMAND.finditer(command))

        if rust_tools:
            if name != "Run cargo-deny licenses (fail-closed)" or rust_tools != ["cargo"]:
                problems.append(f"censo {name!r}: sink Cargo/Rust inesperado {rust_tools!r}")
            elif wd != POLICY_TREE:
                problems.append(f"censo {name!r}: Cargo fora de POLICY_TREE")

        if git_subcommands:
            allowed_git = {
                "Prepare isolated Cargo policy tree (PR data only)": {"ls-tree", "archive"},
                "npm banned-license scan": {"diff"},
                "Forbid skip-hook / skip-ci flags": {"log"},
                "Forbid governance-file modifications": {"diff"},
                "Verify required checks are present": {"diff"},
            }.get(name, set())
            unexpected = sorted(set(git_subcommands) - allowed_git)
            if unexpected:
                problems.append(
                    f"censo {name!r}: sink Git não permitido {unexpected!r}; "
                    "Git remoto/config/exec deve ser tratado como fronteira nova"
                )
        classifications.append(f"{name}: shell inline padrão do workflow BASE, env exato")
    return classifications


def check_untrusted_control_tree(untrusted_tree: Path, problems: list[str]) -> None:
    """Compare the control census as bytes without following PR path links.

    This intentionally does not parse or invoke an untrusted script. A PR
    cannot supply a symlink in either the leaf *or any parent component* that
    makes the BASE process read a host pathname. A deleted/non-regular control
    is just as unacceptable as a changed byte. The static workflow is a deny
    gate for control changes; updating a control requires a separately trusted
    review path until an actually isolated runner label exists.
    """
    try:
        root_mode = untrusted_tree.lstat().st_mode
        root_fd = os.open(
            untrusted_tree,
            os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
        )
    except OSError:
        problems.append("premerge B-133: árvore PR para inspeção estática ausente ou não é diretório")
        return
    try:
        if not stat.S_ISDIR(root_mode):
            problems.append("premerge B-133: árvore PR para inspeção estática ausente ou não é diretório")
            return
        for relative in sorted(REQUIRED_TEETH_PATHS):
            trusted = Path(relative)
            components = Path(relative).parts
            current_fd = os.dup(root_fd)
            try:
                parent_is_safe = True
                for index, component in enumerate(components):
                    component_name = str(Path(*components[: index + 1]))
                    is_leaf = index == len(components) - 1
                    try:
                        mode = os.lstat(component, dir_fd=current_fd).st_mode
                    except FileNotFoundError:
                        if is_leaf:
                            problems.append(f"premerge B-133: PR removeu controle B-133 {relative!r}")
                        else:
                            problems.append(
                                f"premerge B-133: PR removeu pai de controle B-133 {component_name!r}"
                            )
                        parent_is_safe = False
                        break
                    except OSError:
                        problems.append(
                            f"premerge B-133: não foi possível inspecionar controle B-133 {component_name!r}; leitura recusada"
                        )
                        parent_is_safe = False
                        break

                    if not is_leaf:
                        if stat.S_ISLNK(mode):
                            problems.append(
                                f"premerge B-133: pai de controle B-133 {component_name!r} é symlink; leitura recusada"
                            )
                            parent_is_safe = False
                            break
                        if not stat.S_ISDIR(mode):
                            problems.append(
                                f"premerge B-133: pai de controle B-133 {component_name!r} não é diretório; leitura recusada"
                            )
                            parent_is_safe = False
                            break
                        try:
                            next_fd = os.open(
                                component,
                                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                dir_fd=current_fd,
                            )
                        except OSError:
                            problems.append(
                                f"premerge B-133: não foi possível abrir pai de controle B-133 {component_name!r}; leitura recusada"
                            )
                            parent_is_safe = False
                            break
                        os.close(current_fd)
                        current_fd = next_fd
                        continue

                    if not stat.S_ISREG(mode):
                        problems.append(f"premerge B-133: PR tornou controle B-133 não-regular {relative!r}")
                        parent_is_safe = False
                        break
                    if not trusted.is_file():
                        problems.append(f"INSTRUMENTO QUEBRADO: controle BASE ausente {relative!r}")
                        parent_is_safe = False
                        break
                    try:
                        control_fd = os.open(
                            component,
                            os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                            dir_fd=current_fd,
                        )
                        with os.fdopen(control_fd, "rb") as control:
                            if not stat.S_ISREG(os.fstat(control.fileno()).st_mode):
                                problems.append(
                                    f"premerge B-133: PR tornou controle B-133 não-regular {relative!r}"
                                )
                                parent_is_safe = False
                                break
                            candidate_bytes = control.read()
                    except OSError:
                        problems.append(
                            f"premerge B-133: não foi possível abrir controle B-133 {relative!r}; leitura recusada"
                        )
                        parent_is_safe = False
                        break
                    if candidate_bytes != trusted.read_bytes():
                        problems.append(
                            f"premerge B-133: PR alterou controle B-133 {relative!r}; execução recusada"
                        )
                if not parent_is_safe:
                    continue
            finally:
                os.close(current_fd)
    finally:
        os.close(root_fd)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--untrusted-tree",
        type=Path,
        help="árvore PR a comparar somente como dados; nunca é executada",
    )
    args = parser.parse_args(argv)
    try:
        import yaml
    except ImportError:
        return fail("INSTRUMENTO QUEBRADO: PyYAML não está instalado")
    if not WF.is_file():
        return fail(f"INSTRUMENTO QUEBRADO: {WF} não existe — reavalie B-133, não feche")
    try:
        wf = yaml.safe_load(WF.read_text())
    except yaml.YAMLError as exc:
        return fail(f"INSTRUMENTO QUEBRADO: {WF} não é YAML válido: {exc}")
    if not isinstance(wf, dict):
        return fail(f"INSTRUMENTO QUEBRADO: {WF} não é um documento YAML-mapa")

    problems: list[str] = []
    # Environment/defaults inherit into every post-checkout step, including
    # pinned `uses:` actions.  They are checked before the per-step census so
    # a workflow-level NODE_OPTIONS or job-level shell cannot hide behind an
    # otherwise exact step map.
    check_exact_env(wf, {}, "env do workflow", problems, reject_extras=True)
    check_no_inherited_run_config(wf, "workflow", problems)
    check_triggers(wf, problems)
    jobs = wf.get("jobs") or {}
    policy_job = jobs.get("policy-gate")
    if not isinstance(policy_job, dict):
        problems.append("job policy-gate ausente")
        policy_job = {}
    elif str(policy_job.get("if", "")) != DEPENDABOT_PR_AUTHOR_IF:
        problems.append(
            "policy-gate deve decidir pelo autor imutável do PR Dependabot, não por github.actor"
        )
    steps = policy_job.get("steps") or []
    if not isinstance(steps, list) or not steps:
        problems.append("policy-gate não tem steps; população de execução é vazia")
        steps = []
    if steps and not all(isinstance(step, dict) for step in steps):
        problems.append("policy-gate tem step não-mapa; instrumento não pode classificá-lo")
        steps = [step for step in steps if isinstance(step, dict)]

    check_exact_env(policy_job, REQUIRED_JOB_ENV, "env do policy-gate", problems, reject_extras=True)
    check_no_inherited_run_config(policy_job, "policy-gate", problems)
    check_premerge_teeth_workflow(yaml, problems)
    if args.untrusted_tree is not None:
        check_untrusted_control_tree(args.untrusted_tree, problems)
    if steps:
        check_checkout_steps(steps, problems)
        check_policy_steps(steps, problems)
        classifications = check_execution_census(steps, problems)
    else:
        classifications = []

    if problems:
        return fail("REGRESSÃO B-133 — trust boundary não comprovada:\n  " + "\n  ".join(problems))

    print("B-133 trust-boundary census (todo passo pós-checkout classificado):")
    for classification in classifications:
        print(f"  - {classification}")
    print(
        "B-133 fechado: PR fornece apenas dados regulares arquivados; "
        "BASE fornece scripts/política; HOME/CARGO/Git/SSH estão isolados."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

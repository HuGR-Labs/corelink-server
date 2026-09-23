#!/usr/bin/env python3
"""Fail-closed contract for CI migration bundle 1 (issue #2156)."""

from __future__ import annotations

from pathlib import Path


WORKFLOWS = {
    ".github/workflows/action-sha-audit.yml": {
        "job": "verify-sha-pinning",
        "trigger": (
            '  pull_request:\n'
            '    paths:\n'
            '      - ".github/workflows/**"\n'
            '      - "scripts/verify-action-sha-pinning.py"\n'
            '      - ".github/workflows/action-sha-audit.yml"\n'
            '  push:\n'
            '    branches: [main]\n'
            '    paths:\n'
            '      - ".github/workflows/**"\n'
            '      - "scripts/verify-action-sha-pinning.py"'
        ),
    },
    ".github/workflows/api-surface-parity.yml": {
        "job": "parity",
        "trigger": (
            '  pull_request:\n'
            '    paths:\n'
            '      - "openapi/corelink-v1.yaml"\n'
            '      - "crates/**/*.rs"\n'
            '      - "worker/src/**/*.ts"\n'
            '      - "apps/**"\n'
            '      - "scripts/validate_api_surface.py"\n'
            '      - "scripts/verify_b119_surface.py"\n'
            '      - "scripts/verify_b121_surface.py"\n'
            '      - ".github/workflows/api-surface-parity.yml"\n'
            '  workflow_dispatch:'
        ),
    },
    ".github/workflows/quickstart-validate.yml": {
        "job": "validate-quickstart",
        "trigger": (
            '  pull_request:\n'
            '    paths:\n'
            '      - "apps/docs/docs/tutorials/quickstart-10min.mdx"\n'
            '      - "apps/docs/scripts/validate-quickstart.sh"\n'
            '      - "tools/cli/src/main.rs"\n'
            '      - ".github/workflows/quickstart-validate.yml"\n'
            '  push:\n'
            '    branches: [main]\n'
            '    paths:\n'
            '      - "apps/docs/docs/tutorials/quickstart-10min.mdx"\n'
            '      - "apps/docs/scripts/validate-quickstart.sh"\n'
            '      - "tools/cli/src/main.rs"'
        ),
    },
}
EXPECTED_PERMISSIONS = "permissions:\n  contents: read"


def _top_level_block(source: str, heading: str) -> str:
    lines = source.splitlines()
    start = next((i for i, line in enumerate(lines) if line == heading), -1)
    if start < 0:
        return ""
    block: list[str] = [heading]
    for line in lines[start + 1 :]:
        if line and not line.startswith((" ", "\t", "#")):
            break
        if line.strip() and not line.lstrip().startswith("#"):
            block.append(line.split("#", 1)[0].rstrip())
    return "\n".join(block)


def verify_texts(texts: dict[str, str]) -> None:
    for path, contract in WORKFLOWS.items():
        source = texts.get(path)
        if source is None:
            raise AssertionError(f"missing bundle workflow: {path}")
        if _top_level_block(source, "on:") != "on:\n" + contract["trigger"]:
            raise AssertionError(f"trigger/least-privilege contract changed: {path}")
        if _top_level_block(source, "permissions:") != EXPECTED_PERMISSIONS:
            raise AssertionError(f"workflow permissions changed: {path}")
        marker = f"  {contract['job']}:"
        start = source.find(marker)
        if start < 0:
            raise AssertionError(f"missing selected job {contract['job']}: {path}")
        body = source[start:]
        if "runs-on: ubuntu-24.04" not in body:
            raise AssertionError(f"selected job is not hosted ubuntu-24.04: {path}")
        if "runs-on: corelink" in body or "runs-on: self-hosted" in body:
            raise AssertionError(f"selected job retains a self-hosted runner: {path}")
        checkout = body.find("uses: actions/checkout@")
        if checkout < 0 or "persist-credentials: false" not in body[checkout:checkout + 500]:
            raise AssertionError(f"checkout credentials are not disabled: {path}")

    quickstart = texts[".github/workflows/quickstart-validate.yml"]
    if "continue-on-error: true" not in quickstart:
        raise AssertionError("quickstart soft-fail boundary was removed")


def verify_adversarial_mutations(texts: dict[str, str]) -> None:
    mutations = (
        (".github/workflows/action-sha-audit.yml", "runs-on: ubuntu-24.04", "runs-on: corelink"),
        (".github/workflows/api-surface-parity.yml", "persist-credentials: false", "persist-credentials: true"),
        (".github/workflows/quickstart-validate.yml", "continue-on-error: true", "continue-on-error: false"),
        (".github/workflows/quickstart-validate.yml", "permissions:\n  contents: read", "permissions:\n  contents: write"),
        (".github/workflows/api-surface-parity.yml", "  workflow_dispatch:", "  schedule:\n    - cron: '0 * * * *'\n  workflow_dispatch:"),
        (".github/workflows/action-sha-audit.yml", "  push:\n    branches: [main]", "  push:\n    branches: [develop]"),
    )
    for path, needle, replacement in mutations:
        mutated = dict(texts)
        if needle not in mutated[path]:
            raise AssertionError(f"mutation target missing: {path}: {needle}")
        mutated[path] = mutated[path].replace(needle, replacement, 1)
        try:
            verify_texts(mutated)
        except AssertionError:
            continue
        raise AssertionError(f"adversarial mutation was accepted: {path}: {needle}")


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    texts = {path: (root / path).read_text(encoding="utf-8") for path in WORKFLOWS}
    verify_texts(texts)
    verify_adversarial_mutations(texts)
    print("CI bundle 1 hosted runner and safety contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

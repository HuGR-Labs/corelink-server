#!/usr/bin/env python3
"""Prove the signup-worker's pnpm pin is REAL, not merely present.

B-090. `pnpm install --frozen-lockfile` pins nothing if the lockfile has no
importer for the package being installed — the flag would still be there, the
workflow would still look fixed, and the deploy would still resolve a fresh
tree. A gate that only greps for the flag is decorative.

So this checks the pin itself: every `dependency` and `devDependency` the
worker's package.json declares must carry a `specifier:` under the
`apps/signup-worker` importer in pnpm-lock.yaml.

Parses YAML and JSON rather than matching regexes against the lockfile: pnpm
lockfiles nest and quote inconsistently, and a regex that works today is the
kind of gate that silently stops matching after a pnpm upgrade.

Exit 0 = pinned. Exit 1 = a named failure, never a silent pass: a missing file,
an unparseable lockfile, a package.json that declares nothing, a missing
importer and a missing specifier are five distinct messages.

Run: python3 scripts/check_signup_worker_pin.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

PKG = Path("apps/signup-worker/package.json")
LOCK = Path("pnpm-lock.yaml")
IMPORTER = "apps/signup-worker"
DEP_SECTIONS = ("dependencies", "devDependencies", "optionalDependencies")


def fail(msg: str) -> int:
    print(msg)
    return 1


def main() -> int:
    try:
        import yaml
    except ImportError:
        return fail("INSTRUMENTO QUEBRADO: PyYAML nao esta instalado")

    for p in (PKG, LOCK):
        if not p.is_file():
            return fail(f"INSTRUMENTO QUEBRADO: {p} nao existe")

    try:
        pkg = json.loads(PKG.read_text())
    except json.JSONDecodeError as e:
        return fail(f"INSTRUMENTO QUEBRADO: {PKG} nao e JSON valido: {e}")

    want = set(pkg.get("dependencies") or {}) | set(pkg.get("devDependencies") or {})
    # An empty set would make every assertion below vacuously true.
    if not want:
        return fail(
            f"INSTRUMENTO QUEBRADO: {PKG} nao declara dependencia nenhuma — "
            "com o conjunto vazio este portao passaria sem medir nada"
        )

    try:
        lock = yaml.safe_load(LOCK.read_text())
    except yaml.YAMLError as e:
        return fail(f"INSTRUMENTO QUEBRADO: {LOCK} nao e YAML valido: {e}")

    importers = (lock or {}).get("importers")
    if not isinstance(importers, dict):
        return fail(f"INSTRUMENTO QUEBRADO: {LOCK} nao tem bloco `importers:`")

    imp = importers.get(IMPORTER)
    if imp is None:
        return fail(
            f"REGRESSAO: {LOCK} nao tem importer `{IMPORTER}` — o pacote saiu do "
            "workspace pnpm, entao `--frozen-lockfile` nao pina nada para ele e o "
            "deploy do worker de cobranca volta a resolver a arvore do zero"
        )

    have = {
        name
        for section in DEP_SECTIONS
        for name, spec in (imp.get(section) or {}).items()
        if isinstance(spec, dict) and spec.get("specifier")
    }

    missing = sorted(want - have)
    if missing:
        return fail(
            f"REGRESSAO: {len(missing)} de {len(want)} deps do package.json sem "
            f"`specifier:` pinado sob o importer `{IMPORTER}`: {missing}"
        )

    print(
        f"  pino real: {len(want)}/{len(want)} deps do package.json tem "
        f"`specifier:` sob o importer `{IMPORTER}`"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

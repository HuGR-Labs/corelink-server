#!/usr/bin/env python3
"""Fail-closed verifier for B-095's client interface contracts.

This verifier inspects executable contract markers, not historical prose.  In
particular, the old installer comment saying ``--region`` was a no-op must not
keep a repaired item open forever.  An empty result means every required
contract is present; any missing marker is named and reopens B-095.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


class InstrumentError(RuntimeError):
    """The verifier could not read a required contract file."""


FILES = {
    "installer": Path("apps/get-corelink-worker/src/install.ts"),
    "team-ui": Path("apps/admin-ui/src/components/customer/TeamClient.tsx"),
    "team-types": Path("apps/admin-ui/src/lib/customer-types.ts"),
    "team-schema": Path("migrations/d1/0074_team_member.sql"),
    "workspaces": Path("crates/corelink-container/src/routes/workspaces.rs"),
    "workspace-ui": Path("apps/admin-ui/src/components/customer/WorkspacesClient.tsx"),
    "role-catalog": Path("apps/docs/docs/explanation/rbac/role-catalog.mdx"),
    "role-catalog-de": Path(
        "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/explanation/rbac/role-catalog.mdx"
    ),
    "role-catalog-es": Path(
        "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/explanation/rbac/role-catalog.mdx"
    ),
    "role-catalog-pt": Path(
        "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/explanation/rbac/role-catalog.mdx"
    ),
}

QUICKSTARTS = (
    Path("apps/docs/docs/tutorials/quickstart-10min.mdx"),
    Path("apps/docs/i18n/de/docusaurus-plugin-content-docs/current/tutorials/quickstart-10min.mdx"),
    Path("apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/tutorials/quickstart-10min.mdx"),
    Path("apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/tutorials/quickstart-10min.mdx"),
)


def _read(root: Path, relative: Path) -> str:
    path = root / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise InstrumentError(f"cannot read {relative}: {error}") from error


def assess(root: Path = Path(".")) -> list[str]:
    """Return named contract gaps; an empty list means B-095 is done."""
    text = {name: _read(root, path) for name, path in FILES.items()}
    gaps: list[str] = []

    installer = text["installer"]
    # Match executable case arms only.  Comments may explain the retired flag
    # without making the verifier report a historical defect.
    if re.search(r"(?m)^\s*--region(?:=|\*)", installer):
        gaps.append("installer-region-parser")
    if 'echo "FATAL: unsupported option: $1"' not in installer:
        gaps.append("installer-unsupported-options")
    if "--token=*" not in installer:
        gaps.append("installer-token-parser")

    team_ui = text["team-ui"]
    if not re.search(
        r"INVITABLE_ROLES\s*:\s*InviteRole\[\]\s*=\s*\[\s*['\"]admin['\"]\s*,\s*['\"]member['\"]\s*,\s*['\"]viewer['\"]\s*\]",
        team_ui,
    ):
        gaps.append("team-ui-canonical-invitable-roles")
    if re.search(r"\bDeveloper\b", team_ui):
        gaps.append("team-ui-developer-role")

    if not re.search(
        r"CustomerTeamMember[\s\S]{0,500}['\"]owner['\"]\s*\|\s*['\"]admin['\"]\s*\|\s*['\"]member['\"]\s*\|\s*['\"]viewer['\"]",
        text["team-types"],
    ):
        gaps.append("team-types-canonical-role-union")
    schema = text["team-schema"]
    if "'owner'" not in schema or "'admin'" not in schema or "'member'" not in schema or "'viewer'" not in schema:
        gaps.append("team-schema-role-check")
    if re.search(r"(?i)developer", schema):
        gaps.append("team-schema-developer-role")

    workspaces = text["workspaces"]
    if re.search(r"(?i)SET\s+pinned\s*=\s*1\s*-\s*pinned", workspaces):
        gaps.append("workspace-pin-toggle")
    if "SET pinned = ?3" not in workspaces or "struct PinBody" not in workspaces:
        gaps.append("workspace-pin-explicit-state")

    workspace_ui = text["workspace-ui"]
    if re.search(r"(?i)guaranteed[- ]warm|never fall out of cache|exempt from eviction", workspace_ui):
        gaps.append("workspace-ui-guaranteed-warm-claim")
    if "durable retention preference" not in workspace_ui:
        gaps.append("workspace-ui-honest-pin-copy")

    for path in QUICKSTARTS:
        quickstart = _read(root, path)
        label = str(path)
        if re.search(r"(?i)\b(?:ord|iad|lhr|nrt|syd)\b", quickstart):
            gaps.append(f"quickstart-region-id:{label}")
        if re.search(r"(?i)(?:config\.toml[^\n]*region|region[^\n]*config\.toml)", quickstart):
            gaps.append(f"quickstart-config-region:{label}")

    for name in ("role-catalog", "role-catalog-de", "role-catalog-es", "role-catalog-pt"):
        catalog = text[name]
        if "cannot grant or" not in catalog or "revoke `Owner`" not in catalog:
            gaps.append(f"{name}-owner-grant-contract")
        if not re.search(r"\| `Owner`\s*\|\s*❌", catalog):
            gaps.append(f"{name}-owner-transition-matrix")

    return gaps


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.root)
    except InstrumentError as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2
    actual = "open" if gaps else "done"
    print(f"B-095 {actual}: {len(gaps)} gap(s)")
    for gap in gaps:
        print(f"- {gap}")
    if actual != args.expect:
        print(f"expected {args.expect}, found {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

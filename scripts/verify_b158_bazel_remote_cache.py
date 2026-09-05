#!/usr/bin/env python3
"""Fail-closed, offline verifier for the published B-158 Bazel recipe.

The verifier models stock Bazel's path construction from ``--remote_cache``
and compares the resulting CAS/AC paths with the exact Axum registrations. It
also mutates the documentation and route source in memory; each mutation must
be detected, so a vacuous green check cannot close the ledger item.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import urlparse


DOC = "apps/docs/docs/integrations/bazel.md"
ROUTES = "crates/corelink-container/src/routes/bazel_v2.rs"
EXPECTED_CACHE = "https://corelink-api.humangr.com/bazel/cache"
HASH = "a" * 64
CAS_ROUTE = "/bazel/cache/cas/{hash}"
AC_ROUTE = "/bazel/cache/ac/{hash}"
WRONG_CAS_ROUTE = "/bazel/v2/cas/{hash}"
WRONG_AC_ROUTE = "/bazel/v2/ac/{hash}"


class VerificationError(RuntimeError):
    """The verifier could not safely inspect its required inputs."""


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"required regular file missing or symlinked: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise VerificationError(f"cannot read {relative}: {error}") from error


def _ini_block(document: str) -> str:
    match = re.search(
        r"## Configure `\.bazelrc`.*?```ini\n(?P<ini>.*?)\n```",
        document,
        re.DOTALL,
    )
    if match is None:
        raise VerificationError("published .bazelrc ini block is missing")
    return match.group("ini")


def _registered_templates(routes: str) -> set[str]:
    return {
        match.group(1)
        for match in re.finditer(r"\.route\(\s*\"([^\"]+)\"", routes)
    }


def violations(document: str, routes: str) -> list[str]:
    """Return structural B-158 violations for supplied (possibly mutated) text."""
    problems: list[str] = []
    try:
        ini = _ini_block(document)
    except VerificationError as error:
        return [str(error)]

    cache_lines = re.findall(r"^build --remote_cache=(\S+)$", ini, re.MULTILINE)
    if cache_lines != [EXPECTED_CACHE]:
        problems.append(
            "published stock-Bazel block must contain exactly "
            f"build --remote_cache={EXPECTED_CACHE}"
        )
    if re.search(r"^build --remote_instance_name=", ini, re.MULTILINE):
        problems.append("stock HTTP recipe must not advertise --remote_instance_name")

    if cache_lines:
        parsed = urlparse(cache_lines[0])
        prefix = parsed.path.rstrip("/")
        emitted = (f"{prefix}/cas/{HASH}", f"{prefix}/ac/{HASH}")
        expected = (f"{CAS_ROUTE.replace('{hash}', HASH)}", f"{AC_ROUTE.replace('{hash}', HASH)}")
        if parsed.scheme != "https" or parsed.netloc != urlparse(EXPECTED_CACHE).netloc:
            problems.append("published remote-cache URL has an unexpected scheme or host")
        if emitted != expected:
            problems.append(
                "stock Bazel path construction does not land on the registered "
                f"CAS/AC aliases: {emitted!r}"
            )

    registered = _registered_templates(routes)
    for template in (CAS_ROUTE, AC_ROUTE):
        if template not in registered:
            problems.append(f"required registered route is missing: {template}")
    for template in (WRONG_CAS_ROUTE, WRONG_AC_ROUTE):
        if template in registered:
            problems.append(f"unregistered-prefix route was added: {template}")

    return problems


def mutation_self_test(document: str, routes: str) -> None:
    """Prove that changing either side of the mapping turns the verdict red."""
    if violations(document, routes):
        raise VerificationError("baseline B-158 mapping is not green")

    mutated_document = re.sub(
        r"(^build --remote_cache=https://corelink-api\.humangr\.com)/bazel/cache$",
        r"\1/bazel/v2",
        document,
        count=1,
        flags=re.MULTILINE,
    )
    if mutated_document == document or not violations(mutated_document, routes):
        raise VerificationError("remote-cache prefix mutation was not detected")

    mutated_routes = routes.replace(f'"{CAS_ROUTE}"', f'"{WRONG_CAS_ROUTE}"', 1)
    mutated_routes = mutated_routes.replace(f'"{AC_ROUTE}"', f'"{WRONG_AC_ROUTE}"', 1)
    if mutated_routes == routes or not violations(document, mutated_routes):
        raise VerificationError("route registration mutation was not detected")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        document = _read(args.root, DOC)
        routes = _read(args.root, ROUTES)
        problems = violations(document, routes)
        mutation_self_test(document, routes)
    except (OSError, VerificationError) as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2

    actual = "done" if not problems else "open"
    print(f"B-158 {actual}: {len(problems)} structural gap(s); path construction and mutations checked")
    for problem in problems:
        print(f"- {problem}")
    if actual != args.expect:
        print(f"expected {args.expect}, found {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

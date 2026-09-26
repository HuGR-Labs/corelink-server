#!/usr/bin/env python3
"""Credentialless static guard for the #1700 provider preflight workflow."""
from __future__ import annotations

import ast
from pathlib import Path


WORKFLOW = Path(".github/workflows/staging-provider-preflight.yml")
PROVIDER = Path("scripts/staging_bootstrap_provider.py")


def main() -> int:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    provider = PROVIDER.read_text(encoding="utf-8")
    required_workflow = (
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main' && github.ref_protected",
        'test "$EXPECTED_SHA" = "$GITHUB_SHA"',
        'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"',
        "persist-credentials: false",
        "timeout-minutes: 10",
        "scripts/staging_bootstrap_provider.py --phase preflight",
        "actions/upload-artifact@",
        "retention-days: 30",
    )
    missing = [item for item in required_workflow if item not in workflow]
    if missing:
        raise SystemExit(f"preflight workflow is missing safety guards: {missing}")
    if "pull_request:" not in workflow or "if: github.event_name == 'pull_request'" not in workflow:
        raise SystemExit("pull request checks must remain credentialless and separate")
    for forbidden in ("wrangler deploy", "secret put", "dns_records", "workers/routes"):
        if forbidden in workflow:
            raise SystemExit(f"provider workflow contains a forbidden mutation surface: {forbidden}")
    if 'choices=("preflight", "quarantine", "postflight")' not in provider:
        raise SystemExit("provider phase interface changed; re-review the read-only preflight")
    preflight = provider.split('if args.phase in {"preflight", "quarantine"}', 1)[0]
    if 'if args.phase == "postflight"' in preflight or 'args.phase in {"quarantine", "postflight"}' in preflight:
        raise SystemExit("preflight can reach post-deployment provider checks")
    tree = ast.parse(provider, filename=str(PROVIDER))
    calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]

    def dotted_name(node: ast.expr) -> str:
        if isinstance(node, ast.Name):
            return node.id
        if isinstance(node, ast.Attribute):
            return f"{dotted_name(node.value)}.{node.attr}"
        return ""

    requests = [node for node in calls if dotted_name(node.func) == "urllib.request.Request"]
    urlopens = [node for node in calls if dotted_name(node.func) == "urllib.request.urlopen"]
    if len(requests) != 1 or len(urlopens) != 1:
        raise SystemExit("provider access must use exactly one request builder and one urlopen boundary")
    request = requests[0]
    request_keywords = {keyword.arg: keyword.value for keyword in request.keywords}
    body = request_keywords.get("data")
    method = request_keywords.get("method")
    body_is_empty = body is None or (isinstance(body, ast.Constant) and body.value is None)
    method_is_get = method is None or (isinstance(method, ast.Constant) and method.value == "GET")
    positional_body_is_empty = len(request.args) < 2 or (
        isinstance(request.args[1], ast.Constant) and request.args[1].value is None
    )
    if len(request.args) > 2 or not body_is_empty or not method_is_get or not positional_body_is_empty:
        raise SystemExit("provider requests must be GET-only and carry no body")
    urlopen = urlopens[0]
    urlopen_keywords = {keyword.arg: keyword.value for keyword in urlopen.keywords}
    urlopen_data = urlopen_keywords.get("data")
    urlopen_has_no_body = urlopen_data is None or (
        isinstance(urlopen_data, ast.Constant) and urlopen_data.value is None
    )
    if (
        len(urlopen.args) != 1
        or not isinstance(urlopen.args[0], ast.Name)
        or urlopen.args[0].id != "request"
        or not urlopen_has_no_body
    ):
        raise SystemExit("urlopen must send only the GET request object with no body")
    get_function = next(
        (node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "get"),
        None,
    )
    if get_function is None or urlopens[0] not in ast.walk(get_function):
        raise SystemExit("all provider network access must remain inside the readback GET helper")
    print("staging provider preflight safety contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

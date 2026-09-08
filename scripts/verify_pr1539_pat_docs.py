#!/usr/bin/env python3
"""Verify the four PR #1539 PAT-document contracts independently.

This is intentionally a small, read-only docs gate.  Each rule names one
document and one claim so a stale phrase cannot be hidden by a broad grep or by
another document containing the right route.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from typing import Mapping


def _literal_marker(value: str) -> str:
    """Represent a literal as code-safe text while retaining its exact value."""

    return "__CORELINK_LITERAL_" + value.encode("utf-8").hex() + "__"


TARGETS = {
    "tenancy": "apps/docs/docs/concepts/tenancy.md",
    "security": "apps/docs/docs/security.md",
    "go_ci": "apps/docs/docs/how-to/sdk-go/06-ci-integration.mdx",
    "python_ci": "apps/docs/docs/how-to/sdk-python/06-ci-integration.mdx",
}

# These are the implementation anchors audited for this delta.  The verifier
# checks them too, so a future route/scope/TTL change cannot leave the prose
# green merely because its old positive words remain.
SOURCE_CONTRACTS = (
    ("crates/corelink-container/src/routes/customer.rs", "/v1/customer/keys", "customer key routes"),
    ("crates/corelink-container/src/customer_d1.rs", "SELF_SERVE_PAT_TTL: Duration = Duration::from_secs(90 * 86_400)", "customer PAT TTL"),
    ("crates/corelink-container/src/scope.rs", '"cache:read"', "cache read scope vocabulary"),
    ("crates/corelink-container/src/scope.rs", '"cache:write"', "cache write scope vocabulary"),
    ("crates/corelink-container/src/scope.rs", '"cache:find-missing"', "find-missing scope vocabulary"),
    ("worker/src/lib/pat_verify_cache.ts", "const KV_PAT_ROW_TTL_S = 30", "Worker PAT verification cache bound"),
    ("crates/corelink-container/src/routes/byok_admin.rs", "byok_not_available", "BYOK availability boundary"),
)

# Executable anchors deliberately live outside SOURCE_CONTRACTS: comments and
# type documentation are not evidence that a guard is wired.  These patterns
# cover the actual route extraction, adapter comparison, and signup allocation.
EXECUTABLE_CONTRACTS = (
    (
        "worker/src/index.ts",
        re.compile(
            r"if \(path\.startsWith\(" + re.escape(_literal_marker("/bazel/v2/")) + r"\s*\)\) \{\s*"
            r"const rest = path\.slice\(" + re.escape(_literal_marker("/bazel/v2/")) + r"\s*\.length\);\s*"
            r"const tenant = extractFirstSegment\(" + re.escape(_literal_marker("/")) + r"\s*\+ rest\) \?\?\s*"
            + re.escape(_literal_marker("_anonymous"))
            + r"\s*;\s*return \{ tenantId: tenant, pathSuffix: path, routeKind:\s*"
            + re.escape(_literal_marker("bazel_v2"))
            + r"\s* \};",
            re.DOTALL,
        ),
        "Worker Bazel tenant extraction execution",
    ),
    (
        "crates/corelink-bazel-bridge/src/adapter.rs",
        re.compile(
            r'fn check_tenant\(instance: &str, caller_tenant: &str\)[^{]*\{\s*'
            r'if instance != caller_tenant \{',
            re.DOTALL,
        ),
        "Bazel adapter instance comparison execution",
    ),
    (
        "crates/corelink-container/src/routes/signup.rs",
        re.compile(r'let tenant_id = Uuid::now_v7\(\);'),
        "pilot signup UUIDv7 allocation execution",
    ),
    (
        "apps/signup-worker/src/webhooks/clerk.ts",
        re.compile(r'const tenantId = crypto\.randomUUID\(\);'),
        "customer Clerk signup UUIDv4 allocation execution",
    ),
)

# Source mutations replace only executable statements; surrounding comments
# are left untouched.  Each wrong implementation must be rejected by the
# executable patterns above, preventing comment-only evidence from passing.
EXECUTABLE_MUTATIONS = (
    (
        "worker/src/index.ts",
        'const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";',
        'const tenant = extractSecondSegment("/" + rest) ?? "_anonymous";',
        "Worker extraction altered",
    ),
    (
        "crates/corelink-bazel-bridge/src/adapter.rs",
        "if instance != caller_tenant {",
        "if instance == caller_tenant {",
        "Bazel adapter tenant guard inverted",
    ),
    (
        "crates/corelink-container/src/routes/signup.rs",
        "let tenant_id = Uuid::now_v7();",
        "let tenant_id = Uuid::new_v4();",
        "signup UUIDv7 allocation altered",
    ),
    (
        "worker/src/index.ts",
        'const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";\n',
        "",
        "Worker extraction removed",
    ),
    (
        "crates/corelink-bazel-bridge/src/adapter.rs",
        "    if instance != caller_tenant {\n",
        "",
        "Bazel adapter tenant guard removed",
    ),
    (
        "crates/corelink-container/src/routes/signup.rs",
        "    let tenant_id = Uuid::now_v7();\n",
        "",
        "signup UUIDv7 allocation removed",
    ),
    (
        "apps/signup-worker/src/webhooks/clerk.ts",
        "const tenantId = crypto.randomUUID();",
        "const tenantId = crypto.randomUUIDv7();",
        "customer Clerk signup UUIDv4 allocation altered",
    ),
    (
        "apps/signup-worker/src/webhooks/clerk.ts",
        "  const tenantId = crypto.randomUUID();\n",
        "",
        "customer Clerk signup UUIDv4 allocation removed",
    ),
)

# (document, substring, human-readable failure).  Keep these checks distinct:
# each one represents a separately auditable stale or missing claim.
FORBIDDEN = (
    ("tenancy", "The tenant ID appears in every API path:", "tenant-path overclaim"),
    ("tenancy", "every CAS and AC request carries the tenant ID in the URL path", "CAS/AC path overclaim"),
    ("tenancy", "acme-prod", "non-UUID tenant example"),
    ("tenancy", "Expiry | Optional; set at creation time; defaults to non-expiring", "non-expiring expiry claim"),
    ("tenancy", "cas:read", "retired cas:read scope spelling"),
    ("tenancy", "cas:write", "retired cas:write scope spelling"),
    ("tenancy", "ac:read", "retired ac:read scope spelling"),
    ("tenancy", "ac:write", "retired ac:write scope spelling"),
    ("security", "cas:read", "retired cas:read scope spelling"),
    ("security", "cas:write", "retired cas:write scope spelling"),
    ("security", "ac:read", "retired ac:read scope spelling"),
    ("security", "ac:write", "retired ac:write scope spelling"),
    ("security", "once\nself-service issuance ships", "pre-self-service rotation claim"),
    ("security", "< 100 ms", "unfounded revocation latency claim"),
    ("security", "acme-prod", "non-UUID tenant example"),
    ("security", "Enterprise plan tenants can supply their own AES-256 key", "unavailable BYOK claim"),
    ("security", "Contact sales to enable BYOK", "unavailable BYOK activation claim"),
    ("go_ci", "There is no self-service API to mint", "obsolete Go self-service claim"),
    ("python_ci", "There is no self-service API to mint", "obsolete Python self-service claim"),
)

# Positive markers are also mutation points.  Removing any one must make the
# verifier fail, proving that each contract is load-bearing in the gate.
REQUIRED = {
    "tenancy": (
        ("UUIDv7 tenant ID", "signup tenant identifier format"),
        ("Native REAPI v1 routes", "native REAPI tenant resolution"),
        ("REAPI v2 carries the tenant ID as its `instance` path segment", "Bazel instance tenant binding"),
        ("that instance does not equal the PAT-resolved tenant", "Bazel URL tenant check"),
        ("tenant-addressed `/v1/cas/<tenant>/...`", "tenant-addressed CAS routing"),
        ("Customer-issued PATs expire after 90 days", "customer PAT TTL"),
        ("The customer mint request has no caller-selected expiry field", "mint expiry shape"),
        ("cache:read", "read scope"),
        ("cache:write", "write scope"),
        ("cache:find-missing", "find-missing scope"),
        ('"scopes":["cache:write"]', "CI mint payload"),
        ("POST /v1/customer/keys/{pat_id}/revoke", "revoke route"),
    ),
    "security": (
        ("Customer-managed BYOK is not currently shipped", "BYOK availability"),
        ("operator-only activation route", "BYOK operator boundary"),
        ("501 byok_not_available", "BYOK unavailable response"),
        ("no customer self-service", "BYOK customer boundary"),
        ("starter PAT stored with the canonical `read-write` scope", "starter scope"),
        ('"scopes": ["cas:rw"]', "self-service scope example"),
        ("fixed 90-day lifetime", "customer PAT TTL"),
        ("up to 30 seconds", "revocation cache bound"),
        ("cache:find-missing", "find-missing scope"),
        ("cache-write capability", "mint privilege gate"),
        ("native REAPI routes have no tenant segment", "native tenant resolution"),
    ),
    "go_ci": (
        ("POST\n/v1/customer/keys", "self-service mint route"),
        ('"scopes": ["cas:rw"]', "Go scope example"),
        ("no caller-selected expiry", "Go mint expiry shape"),
        ("Customer-issued PATs expire after 90 days", "Go customer PAT TTL"),
        ("CORELINK_TENANT: ${{ vars.CORELINK_TENANT }}", "GitHub tenant variable"),
        ("CORELINK_TENANT: $CORELINK_TENANT", "non-GitHub tenant variable"),
        ("server-trusted owner/admin team role", "Go revoke role"),
    ),
    "python_ci": (
        ("POST\n/v1/customer/keys", "self-service mint route"),
        ('"scopes": ["cas:rw"]', "Python scope example"),
        ("no caller-selected expiry", "Python mint expiry shape"),
        ("Customer-issued PATs expire after 90 days", "Python customer PAT TTL"),
        ("CORELINK_TENANT: ${{ vars.CORELINK_TENANT }}", "GitHub tenant variable"),
        ("CORELINK_TENANT: $CORELINK_TENANT", "non-GitHub tenant variable"),
        ("server-trusted owner/admin team role", "Python revoke role"),
    ),
}


def verify_texts(texts: Mapping[str, str]) -> list[str]:
    """Return one diagnostic per failed document/claim; empty means valid."""

    errors: list[str] = []
    for name, relative in TARGETS.items():
        if name not in texts:
            errors.append(f"{name}: document not loaded ({relative})")

    for name, needle, label in FORBIDDEN:
        if name in texts and needle in texts[name]:
            errors.append(f"{name}: forbidden {label}: {needle!r}")

    for name, checks in REQUIRED.items():
        if name not in texts:
            continue
        for needle, label in checks:
            if needle not in texts[name]:
                errors.append(f"{name}: missing {label}: {needle!r}")
    return errors


def load(root: Path) -> dict[str, str]:
    return {name: (root / relative).read_text(encoding="utf-8") for name, relative in TARGETS.items()}


def _mask_non_code(source: str, relative: str) -> str:
    """Blank comments and quoted literals before applying executable regexes.

    The mask preserves newlines so line-anchored Rust patterns retain their
    meaning. JavaScript/TypeScript template literals are wholly non-code here;
    interpolation is not an implementation anchor and must not smuggle one
    into a string. Rust raw strings (including hash-delimited variants) and
    nested block comments are handled explicitly.
    """

    rust = relative.endswith(".rs")
    masked: list[str] = []
    index = 0
    length = len(source)

    def blank_until(end: int) -> None:
        segment = source[index:end]
        masked.extend("\n" if char == "\n" else " " for char in segment)

    def append_literal(value: str) -> None:
        masked.append(_literal_marker(value))
        # Keep line structure stable even though the literal's contents are
        # removed from the token stream.
        masked.extend("\n" for char in value if char == "\n")

    def quoted_end(position: int, quote: str) -> int:
        position += 1
        while position < length:
            if source[position] == "\\":
                position += 2
            elif source[position] == quote:
                return position + 1
            else:
                position += 1
        return length

    while index < length:
        if source.startswith("//", index):
            end = source.find("\n", index + 2)
            end = length if end < 0 else end
            blank_until(end)
            index = end
            continue
        if source.startswith("/*", index):
            if rust:
                depth = 1
                end = index + 2
                while end < length and depth:
                    if source.startswith("/*", end):
                        depth += 1
                        end += 2
                    elif source.startswith("*/", end):
                        depth -= 1
                        end += 2
                    else:
                        end += 1
            else:
                # ECMAScript block comments do not nest.  Documentation often
                # contains glob-like `/*` examples, so treating those as a
                # nested opener would hide the remainder of the source.
                closing = source.find("*/", index + 2)
                end = length if closing < 0 else closing + 2
            blank_until(end)
            index = end
            continue
        if rust and source[index] in {"r", "b"}:
            raw_start = index + 1
            if source[index] == "b" and raw_start < length and source[raw_start] == "r":
                raw_start += 1
            marker = raw_start
            while marker < length and source[marker] == "#":
                marker += 1
            if marker < length and source[marker] == '"':
                hashes = source[raw_start:marker]
                terminator = '"' + hashes
                closing = source.find(terminator, marker + 1)
                end = length if closing < 0 else closing + len(terminator)
                value_end = length if closing < 0 else closing
                append_literal(source[marker + 1:value_end])
                if end > value_end:
                    masked.extend("\n" if char == "\n" else " " for char in source[value_end:end])
                index = end
                continue
        if source[index] == '"' or (not rust and source[index] in {"'", "`"}):
            quote = source[index]
            end = quoted_end(index, quote)
            value_end = end - 1 if end <= length and end > index and source[end - 1] == quote else length
            append_literal(source[index + 1:value_end])
            if end > value_end:
                masked.extend("\n" if char == "\n" else " " for char in source[value_end:end])
            index = end
            continue
        masked.append(source[index])
        index += 1
    return "".join(masked)


def verify_sources(root: Path) -> list[str]:
    errors: list[str] = []
    for relative, needle, label in SOURCE_CONTRACTS:
        path = root / relative
        if not path.is_file():
            errors.append(f"source missing for {label}: {relative}")
        elif needle not in path.read_text(encoding="utf-8"):
            errors.append(f"source contract drift for {label}: {relative} lacks {needle!r}")
    return errors


def verify_executable_sources(root: Path) -> list[str]:
    """Require the implementation pattern, not a nearby explanatory comment."""

    errors: list[str] = []
    for relative, pattern, label in EXECUTABLE_CONTRACTS:
        path = root / relative
        if not path.is_file():
            errors.append(f"executable source missing for {label}: {relative}")
        elif not pattern.search(_mask_non_code(path.read_text(encoding="utf-8"), relative)):
            errors.append(f"executable contract drift for {label}: {relative}")
    return errors


def executable_mutation_cases(root: Path) -> list[tuple[str, list[str]]]:
    """Return diagnostics for implementation mutations that retain comments."""

    cases: list[tuple[str, list[str]]] = []
    for relative, old, new, label in EXECUTABLE_MUTATIONS:
        path = root / relative
        original = path.read_text(encoding="utf-8")
        mutated = original.replace(old, new, 1)
        cases.append((label, verify_executable_sources_text({relative: mutated})))
    return cases


def verify_executable_sources_text(sources: Mapping[str, str]) -> list[str]:
    errors: list[str] = []
    for relative, pattern, label in EXECUTABLE_CONTRACTS:
        text = sources.get(relative)
        if text is not None and not pattern.search(_mask_non_code(text, relative)):
            errors.append(f"executable contract drift for {label}: {relative}")
    return errors


def mutation_cases(texts: Mapping[str, str]) -> list[tuple[str, str, list[str]]]:
    """Mutate every positive marker and return the resulting diagnostics."""

    cases: list[tuple[str, str, list[str]]] = []
    for name, checks in REQUIRED.items():
        for needle, label in checks:
            mutated = dict(texts)
            # Remove every occurrence: some contracts are repeated in a
            # workflow matrix (GitLab + CircleCI) or in a prose/example pair.
            # The mutation models deleting the claim, not editing one copy.
            mutated[name] = mutated[name].replace(needle, "")
            cases.append((name, label, verify_texts(mutated)))
    return cases


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true", help="run adversarial positive-marker mutations")
    args = parser.parse_args()

    texts = load(args.root)
    errors = verify_sources(args.root) + verify_executable_sources(args.root) + verify_texts(texts)
    if errors:
        for error in errors:
            print(f"FAIL: {error}")
        return 1

    if args.self_test:
        cases = mutation_cases(texts)
        undetected = [(name, label) for name, label, diagnostics in cases if not diagnostics]
        if undetected:
            for name, label in undetected:
                print(f"FAIL: mutation was not detected for {name}: {label}")
            return 1
        executable_cases = executable_mutation_cases(args.root)
        executable_undetected = [label for label, diagnostics in executable_cases if not diagnostics]
        if executable_undetected:
            for label in executable_undetected:
                print(f"FAIL: executable mutation was not detected: {label}")
            return 1
        print(
            f"PASS: {len(cases)} independent document mutations and "
            f"{len(executable_cases)} executable mutations rejected"
        )

    print(f"PASS: {len(TARGETS)} PR #1539 PAT documents satisfy their current contracts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

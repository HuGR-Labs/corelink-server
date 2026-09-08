#!/usr/bin/env python3
"""Executable, inverted closure gate for B181..B192.

Each contract is bound to a live implementation artifact and a regression test
in that artifact (or its focused test file).  The gate also removes one
load-bearing source clause in memory and requires the contract to turn red;
source comments and symlink substitutions cannot satisfy it.  This is a cheap
focal gate: it performs no network, Cargo, CI, or production calls.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ClosureError(ValueError):
    pass


# (primary artifact, executable clauses, mutation needle, extra artifacts).
# Clauses are deliberately source-specific and include the actual regression
# witness, not a copy of the backlog prose.
CONTRACTS: dict[str, tuple[str, tuple[str, ...], str, tuple[tuple[str, tuple[str, ...]], ...]]] = {
    "B-181": (
        "worker/src/lib/quota.ts",
        (
            r"export async function checkStorageQuota",
            r"if \(tierResult\.d1Error\)\s*\{\s*return storageD1ErrorResult\(isMutating\);",
            r"catch \{[\s\S]{0,300}?return storageD1ErrorResult\(isMutating\);",
        ),
        "if (tierResult.d1Error) {",
        (("worker/src/lib/quota_storage_cache.ts", (r"if \(tierD1Error\)\s*\{\s*return \{\s*ok: false,",)),),
    ),
    "B-182": (
        "worker/src/index_auth.ts",
        (r"isPatExpiryLive\(row\.expires_ms\)",),
        "isPatExpiryLive(row.expires_ms)",
        (("worker/src/lib/pat_expiry.ts", (r"export function isPatExpiryLive", r"return expiresMs === 0 \|\| expiresMs > nowMs;")),
         ("crates/corelink-container/src/adapter_pat_lookup.rs", (r"pub\(super\)\s+const\s+PAT_LOOKUP_SQL\b", r"expires_ms = 0 OR expires_ms >"))),
    ),
    "B-183": (
        "crates/corelink-container/src/adapter_pat_verifier.rs",
        (
            r"Ok\(Err\(PatError::HashError\(kind\)\)\)\s*=>\s*VerifyFlight::Backend",
            r"VerifyFlight::Backend\(msg\)\s*=>\s*return\s+Err\(VerifyError::Backend",
        ),
        "Ok(Err(PatError::HashError(kind))) => VerifyFlight::Backend(",
        (("crates/corelink-container/src/adapter_pat_lookup.rs", (r"pub\s+enum\s+VerifyError\b",)),
         ("crates/corelink-container/src/adapter_pat_crypto.rs", (r"pub\(super\)\s+enum\s+VerifyFlight\b",))),
    ),
    "B-184": (
        "worker/src/index_auth.ts",
        (r"if \(!isPatExpiryLive\(row\.expires_ms\)\)", r"isPatExpiryLive\(row\.expires_ms\)"),
        "if (!isPatExpiryLive(row.expires_ms))",
        (("worker/src/lib/pat_expiry.ts", (r"expiresMs === 0 \|\| expiresMs > nowMs",)),
         ("crates/corelink-container/src/adapter_pat_lookup.rs", (r"expires_ms = 0 OR expires_ms >",))),
    ),
    "B-185": (
        "worker/src/lib/clerk_auth.ts",
        (
            r"isProductionEnvironment\(env\)",
            r"CLERK_ISSUER_URL unset in production",
            r"CLERK_ISSUER_URL",
        ),
        "isProductionEnvironment(env)",
        (("worker/tests/customer_clerk_bridge.test.ts", (r"fails closed when the production issuer pin is absent", r'environment: "prod"')),),
    ),
    "B-186": (
        "apps/admin-ui/src/middleware.ts",
        (
            r'process\.env\["NEXT_PUBLIC_E2E_TEST_MODE"\] === "1"\s*&&\s*process\.env\["NODE_ENV"\] !== "production"',
            r"const isE2E",
        ),
        "const isE2E =",
        (("apps/admin-ui/tests/middleware-failclosed.test.ts", (r"production \+ E2E flag cannot bypass auth",)),),
    ),
    "B-187": (
        "crates/corelink-container/src/main_boot.rs",
        (
            r"const RUNNER_PRICE_ENV_TABLE",
            r'"STRIPE_PRICE_ID_RUNNER_STARTER"\s*,\s*20\s*,\s*100',
            r'"STRIPE_PRICE_ID_RUNNER_PRO"\s*,\s*40\s*,\s*240',
            r'"STRIPE_PRICE_ID_RUNNER_TEAM"\s*,\s*80\s*,\s*600',
            r'"STRIPE_PRICE_ID_RUNNER_SCALE"\s*,\s*160\s*,\s*1200',
            r'"STRIPE_PRICE_ID_RUNNER_MAX"\s*,\s*320\s*,\s*2400',
        ),
        "pub(super) const RUNNER_PRICE_ENV_TABLE",
        (("crates/corelink-container/src/main_tests.rs", (r"runners_resolver_maps_every_ratified_price_and_rejects_placeholders",)),),
    ),
    "B-188": (
        "crates/corelink-container/src/auth_tenant.rs",
        (
            r"pub fn is_canonical_tenant_id",
            r"Uuid::parse_str\(raw\)",
            r"if !is_canonical_tenant_id\(raw\)",
            r"opaque_tenant_is_rejected_by_the_extractor",
        ),
        "if !is_canonical_tenant_id(raw) {",
        (),
    ),
    "B-189": (
        "crates/corelink-container/src/native_pat_gate.rs",
        (
            r"match self\.verifier\.verify_capability\(token\)\.await",
            r"PatVerifier",
            r"forged_token_rejected_401",
            r"genuine_pat_for_wrong_tenant_rejected_401",
        ),
        "match self.verifier.verify_capability(token).await {",
        (),
    ),
    "B-190": (
        "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs",
        (
            r"if !is_canonical_ac_digest\(action_digest\)",
        ),
        "if !is_canonical_ac_digest(action_digest) {",
        (("crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs", (r"fn is_canonical_ac_digest",)),
         ("crates/corelink-container/src/storage/r2_s3_parts/tests_2.rs", (r"is_canonical_ac_digest\(&\"A\"\.repeat\(64\)\)",))),
    ),
    "B-191": (
        "crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs",
        (
            r"self\.client\.put_if_absent\(&key, payload\)",
            r"classify_prior\(prior\.as_deref\(\),\s*&stored_view\)",
            r"Ok\(false\) =>",
        ),
        "self.client.put_if_absent(&key, payload)",
        (("crates/corelink-container/src/storage/r2_s3_parts/client.rs", (r"\.if_none_match\(\"\*\"\)",)),
         ("crates/corelink-container/src/storage/r2_s3_parts/tests_3.rs", (r"classify_prior_divergent_refuses", r"lost_race_resolution_mirrors_pre_put_compare"))),
    ),
    "B-192": (
        "crates/corelink-container/src/storage/r2_kv.rs",
        (
            r"let uid = Uuid::try_parse\(tenant\)",
            r"if uid\.to_string\(\) != tenant",
            r"self\.tdk\.as_ref\(\)\.ok_or_else\(non_derivable_tenant_err\)",
            r"derive_prefix\(tdk,\s*uid\)",
            r"non_derivable_tenant_err",
            r"iso_object_key_rejects_non_uuid_tenants",
            r"iso_object_key_rejects_missing_tdk_even_for_uuid_tenant",
        ),
        "if uid.to_string() != tenant {",
        (("crates/corelink-container/src/auth_tenant.rs", (r"if !is_canonical_tenant_id\(raw\)",)),),
    ),
}


def _without_comments(text: str) -> str:
    """Remove comments while retaining string literals and line structure."""
    out: list[str] = []
    i = 0
    quote: str | None = None
    block = False
    while i < len(text):
        if block:
            if text.startswith("*/", i):
                block = False
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        if quote:
            ch = text[i]
            out.append(ch)
            if ch == "\\" and i + 1 < len(text):
                out.append(text[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if text.startswith("/*", i):
            block = True
            out.extend("  ")
            i += 2
            continue
        if text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        ch = text[i]
        if ch in "'\"`":
            quote = ch
        out.append(ch)
        i += 1
    return "".join(out)


def _without_comments_and_strings(text: str) -> str:
    """Return executable-looking source with comments and literals masked.

    B-182/B-183 are split across Rust fragments.  Their witnesses must be
    declarations/match arms, never copied into a comment or a diagnostic
    string.  Preserve newlines and replace literal bytes with spaces so regex
    matches cannot cross a lexical boundary.
    """
    out: list[str] = []
    i = 0
    quote: str | None = None
    block = False
    while i < len(text):
        if block:
            if text.startswith("*/", i):
                block = False
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        if quote:
            ch = text[i]
            out.append("\n" if ch == "\n" else " ")
            if ch == "\\" and i + 1 < len(text):
                out.append("\n" if text[i + 1] == "\n" else " ")
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if text.startswith("/*", i):
            block = True
            out.extend("  ")
            i += 2
            continue
        if text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        ch = text[i]
        # Rust lifetimes (`'static`, `'a`) are code, not character literals.
        if ch == "'" and i + 1 < len(text) and (text[i + 1].isalpha() or text[i + 1] == "_"):
            out.append(ch)
            i += 1
            continue
        if ch in "'\"`":
            quote = ch
            out.append(" ")
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def _text(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ClosureError(f"missing/non-regular artifact: {relative}")
    return path.read_text(encoding="utf-8")


def _check(
    identifier: str,
    root: Path,
    text_override: str | None = None,
    overrides: dict[str, str] | None = None,
) -> None:
    artifact, patterns, needle, extras = CONTRACTS[identifier]
    overrides = overrides or {}
    raw = overrides.get(artifact, _text(root, artifact) if text_override is None else text_override)
    code = _without_comments_and_strings(raw) if identifier in {"B-182", "B-183"} else _without_comments(raw)
    missing = [p for p in patterns if re.search(p, code, re.IGNORECASE | re.DOTALL) is None]
    if missing:
        raise ClosureError(f"{identifier}: missing executable clause {missing[0]!r} in {artifact}")
    if needle not in _without_comments_and_strings(raw):
        raise ClosureError(f"{identifier}: load-bearing clause {needle!r} is absent in {artifact}")
    for extra_artifact, extra_patterns in extras:
        extra_raw = overrides.get(extra_artifact, _text(root, extra_artifact))
        extra_code = _without_comments_and_strings(extra_raw) if identifier in {"B-182", "B-183"} else _without_comments(extra_raw)
        extra_literals = _without_comments(extra_raw)
        missing_extra = []
        for p in extra_patterns:
            # The declaration itself must be executable code.  SQL text is a
            # literal by design, so its expiry predicate is checked in the
            # comment-free view as a second, narrower clause.
            if identifier == "B-182" and extra_artifact.endswith("adapter_pat_lookup.rs") and p.startswith(r"pub\("):
                found = re.search(p, extra_code, re.IGNORECASE | re.DOTALL)
            else:
                found = re.search(p, extra_code, re.IGNORECASE | re.DOTALL) or re.search(
                    p, extra_literals, re.IGNORECASE | re.DOTALL
                )
            if found is None:
                missing_extra.append(p)
        if missing_extra:
            raise ClosureError(f"{identifier}: missing executable clause {missing_extra[0]!r} in {extra_artifact}")
    if identifier == "B-192" and "pad16" in code:
        raise ClosureError("B-192: forbidden pad16 fallback remains in Turbo production artifact")


def _mutation_self_test(identifier: str, root: Path) -> None:
    artifact, _, needle, extras = CONTRACTS[identifier]
    cases = [(artifact, needle)]
    if identifier == "B-182":
        cases.append(("crates/corelink-container/src/adapter_pat_lookup.rs", "pub(super) const PAT_LOOKUP_SQL"))
    if identifier == "B-183":
        cases.extend(
            [
                ("crates/corelink-container/src/adapter_pat_lookup.rs", "pub enum VerifyError"),
                ("crates/corelink-container/src/adapter_pat_crypto.rs", "pub(super) enum VerifyFlight"),
            ]
        )
    for target, target_needle in cases:
        raw = _text(root, target)
        if target_needle not in raw:
            raise ClosureError(f"{identifier}: mutation needle is absent ({target_needle!r})")
        # Replace every copy: a load-bearing guard is often mirrored across
        # the read and write paths, and mutating only the first copy can leave
        # a decorative duplicate satisfying the contract.
        mutated = raw.replace(target_needle, "__B181_192_MUTATION_REMOVED__")
        try:
            _check(identifier, root, mutated if target == artifact else None, {target: mutated})
        except ClosureError:
            continue
        raise ClosureError(f"{identifier}: load-bearing mutation was not detected ({target})")


def verify(root: Path = ROOT, identifier: str | None = None) -> dict[str, int]:
    ids = [identifier] if identifier is not None else list(CONTRACTS)
    unknown = [item for item in ids if item not in CONTRACTS]
    if unknown:
        raise ClosureError(f"unknown closure id(s): {', '.join(unknown)}")
    for item in ids:
        _check(item, root)
        _mutation_self_test(item, root)
    return {"done": len(ids), "mutations": len(ids)}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--id")
    parser.add_argument("--expect", choices=("done", "open"), default="done")
    args = parser.parse_args(argv)
    try:
        if args.expect == "open":
            raise ClosureError("closure verifier is inverted: open is not a done result")
        report = verify(Path(args.root).resolve(), args.id)
    except (ClosureError, OSError, UnicodeDecodeError) as exc:
        print(f"B-181..B-192 closure gate: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"B-181..B-192 closure gate: PASS: {report['done']} item(s), {report['mutations']} mutation(s) red")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

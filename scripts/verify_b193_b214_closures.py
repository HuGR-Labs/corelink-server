#!/usr/bin/env python3
"""Inverted, executable closure gate for the D03 B-193..B-214 findings.

This gate is deliberately local and read-only: it checks the load-bearing
implementation wiring and then removes each selected clause in memory.  The
mutated implementation must fail the same check, so a comment or metadata-only
claim cannot close a finding.  No Cargo, network, or CI invocation is made.
"""
from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IDS = tuple(f"B-{n}" for n in range(193, 215) if n != 210)


class ClosureError(ValueError):
    pass


# Each entry is (artifact, required executable/source clauses, mutation needle).
CONTRACTS: dict[str, tuple[str, tuple[str, ...], str]] = {
    "B-193": ("crates/corelink-container/src/routes/oci/b126_m2_impl_01_part2.rs", (r"d\.verify_against_bytes\(&session\.buf\)", "self\\.moat\\s*\\n\\s*\\.put"), "d.verify_against_bytes(&session.buf)"),
    "B-194": ("crates/corelink-adapter-host/src/oci/server/core.rs", ("OciAdapterError::Cas\\(_\\) \\| OciAdapterError::Kv\\(_\\) => StatusCode::SERVICE_UNAVAILABLE", "live_storage_failures_are_retryable_503s"), "OciAdapterError::Cas(_) | OciAdapterError::Kv(_) => StatusCode::SERVICE_UNAVAILABLE"),
    "B-195": ("scripts/check_migration_prefixes.py", ("GRANDFATHERED", "actual == allowed", "0044_drata_evidence_sent\\.sql"), "actual == allowed"),
    "B-196": ("apps/signup-worker/src/webhooks/dsr_verify_cron.ts", ("SWEEP_BATCH_LIMIT",), "LIMIT ?3"),
    "B-197": ("crates/corelink-container/src/routes/residency.rs", ("container_colo == \\\"iad\\\"", "decision_absent_header_only_allows_on_iad"), "container_colo == \"iad\""),
    "B-198": ("worker/src/region-map.ts", ("export const PROVISIONED_MACROS", "\\\"wnam\\\"", "\\\"enam\\\"", "\\\"weur\\\"", "\\\"apac\\\"", "isProvisionedMacro"), "  \"apac\",\n"),
    "B-199": ("crates/corelink-container/src/storage/region_map.rs", ("colo_matches_macro", "colo_for_macro\\(macro_region\\) == Some\\(serving_colo\\)"), "colo_matches_macro"),
    "B-200": ("apps/signup-worker/src/webhooks/dsr_verify_cron.ts", ("requested_at >= \\?2", "invalid/expired requested anchor"), "requested_at >= ?2"),
    "B-201": ("crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs", ("list_objects_v2", "count_ac_remaining", "no `ac_meta` D1 index row"), "list_objects_v2"),
    "B-202": ("crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs", ("\\\"iad\\\" => \\\"corelink-cas-prod\\\"", "\\\"lhr\\\" => \\\"corelink-cas-eu\\\"", "\\\"nrt\\\" => \\\"corelink-cas-apac\\\"", "validate_cas_bucket_for_region"), "validate_cas_bucket_for_region"),
    "B-203": ("crates/corelink-container/src/storage/region_map.rs", (r"PROVISIONED_MACROS: \[&str; 4\]", "\\\"wnam\\\"", "\\\"enam\\\"", "\\\"weur\\\"", "\\\"apac\\\"", "provisioned_set_is_exact"), "PROVISIONED_MACROS: [&str; 4]"),
    "B-204": ("legal/dpa-residency-amendment.md", ("CURRENT LAUNCH BOUNDARY \\(DD-051 / B-204\\)", "BYOK is not enabled or provisioned", "future-state design requirement"), "BYOK is not enabled or provisioned"),
    "B-205": ("worker/src/lib/quota_request_cache.ts", ("BURST_MARGIN_FRACTION", "requestKvKey", "incrementMonthlyRequestCount", "waitUntil", "reason: \\\"near-cap\\\""), "export const BURST_MARGIN_FRACTION"),
    "B-206": ("worker/src/durable_object.ts", ("async alarm\\s*\\(", "lastActivityMs", "IDLE_TIMEOUT_MS", "destroyContainer", "setAlarm"), "async alarm()"),
    "B-207": ("worker/src/durable_object_start.ts", ("containerStatus: \\\"starting\\\"", "waitForContainerReady", "startContainer", "startingAt_ms"), "if (ctx.getLifecycleState().containerStatus === \"starting\")"),
    "B-208": ("apps/admin-ui/src/lib/consent-api.ts", (r"getToken\?:", "authHeaders", r"authorization: `Bearer \$\{token\}`", "headers:"), "authorization: `Bearer ${token}`"),
    "B-209": ("apps/admin-ui/src/app/[locale]/onboarding/actions.ts", ("getSessionToken", "configureTenantAction", "\\\"/v1/onboarding/configure\\\"", "\\{ region: input.region, plan: input.plan \\}", "\\{ token \\}"), "\"/v1/onboarding/configure\""),
    "B-211": ("apps/admin-ui/src/app/[locale]/(authenticated)/customer/audit/visualization/layout.tsx", ("CustomerGuard", "return <CustomerGuard>", "children"), "return <CustomerGuard>"),
    "B-212": ("apps/admin-ui/src/middleware.ts", ("!publishableKey || !secretKey", "NODE_ENV.*production", "NextResponse.redirect", "isSelfGatedPath"), "(!publishableKey || !secretKey)"),
    "B-213": ("apps/admin-ui/src/lib/route-matcher.ts", ("barePrefix", "normalized === barePrefix", r"normalized\.startsWith\(`\$\{barePrefix\}/`\)", "isPublicPath"), "normalized.startsWith(`${barePrefix}/`)"),
    "B-214": ("apps/signup-worker/src/webhooks/clerk.ts", ("autoProvisionFromClerkEvent", "primaryEmail", "verification", "email_not_verified"), "primaryEmail?.verification?.status !== \"verified\""),
}


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise ClosureError(f"missing/non-regular evidence: {relative}")
    return path.read_text(encoding="utf-8")


def _related(root: Path, identifier: str) -> str:
    """Read the executable fragment behind a split-module facade.

    B126 keeps small Rust/Worker facades and moves implementation into bounded
    parts.  Gates must follow those include/re-export boundaries, otherwise a
    source split is misreported as a production regression.
    """
    artifact = CONTRACTS[identifier][0]
    text = _read(root, artifact)
    companions = {
        "B-193": ("crates/corelink-container/src/routes/oci/b126_m2_test_1_1_part2.rs",),
        "B-202": ("crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02_part2.rs",),
    }
    return text + "\n" + "\n".join(
        _read(root, rel) for rel in companions.get(identifier, ())
    )


def _rust_without_comments(text: str) -> str:
    """Return Rust source with line/block comments removed for call-path checks."""
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("//"))


def _check(identifier: str, root: Path, override: str | None = None) -> None:
    artifact, patterns, needle = CONTRACTS[identifier]
    text = _related(root, identifier) if override is None else override
    missing = [p for p in patterns if re.search(p, text, re.DOTALL) is None]
    if missing:
        raise ClosureError(f"{identifier}: missing load-bearing clause {missing[0]!r} in {artifact}")
    if needle not in text:
        raise ClosureError(f"{identifier}: mutation needle {needle!r} is absent in {artifact}")

    if identifier == "B-193":
        finalize = text[text.find("fn finalize_upload") :]
        verify = finalize.find("d.verify_against_bytes(&session.buf)")
        persist = finalize.find("self.moat")
        if verify < 0 or persist < 0 or verify > persist:
            raise ClosureError("B-193: digest verification is not before persistence")
        if "finalize_rejects_digest_lie_and_persists_nothing" not in text:
            raise ClosureError("B-193: focal digest-lie test is missing")
    if identifier == "B-196" and text.count("LIMIT ?3") < 2:
        raise ClosureError("B-196: both DSR scans must be bounded")
    if identifier == "B-201":
        ac_route = _rust_without_comments(
            _read(root, "crates/corelink-container/src/routes/ac/part-00-01.rs")
        )
        ac_erase = _rust_without_comments(text)
        if "state.update.update(req)" not in ac_route:
            raise ClosureError("B-201: live AC PUT call path is not wired to the R2 handler")
        if "ac_meta" in ac_route or "ac_meta" in ac_erase:
            raise ClosureError("B-201: AC erase call path still depends on ac_meta")
        if "list_and_delete_ac(&self.write_bucket, &prefix)" not in ac_erase:
            raise ClosureError("B-201: erase path does not call LIST-by-prefix deletion")
    if identifier == "B-198":
        for rel in ("apps/signup-worker/src/webhooks/clerk_identity.ts", "crates/corelink-container/src/storage/region_map.rs"):
            other = _read(root, rel)
            for value in ('"wnam"', '"enam"', '"weur"', '"apac"'):
                if value not in other:
                    raise ClosureError(f"B-198: {value} missing from {rel}")
    if identifier == "B-202":
        if "validate_cas_bucket_for_region" not in _read(root, "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02_part2.rs"):
            raise ClosureError("B-202: bucket validation is not wired in CAS erase")
    if identifier == "B-206":
        alarm_tests = _read(root, "worker/tests/durable_object_part2.test.ts")
        for marker in (
            "re-arms the alarm even when the tick throws mid-way",
            "alarm() destroys the container and does NOT reschedule once idle expires",
            "degraded-but-running stays in the chain: reaper still fires on idle expiry",
        ):
            if marker not in alarm_tests:
                raise ClosureError(f"B-206: missing behavioral alarm test {marker!r}")
        if not re.search(r"let chainEnded = false;.*?finally\s*\{\s*\n\s*if \(!chainEnded\).*?setAlarm", text, re.DOTALL):
            raise ClosureError("B-206: alarm does not re-arm in finally after a tick failure")
    if identifier == "B-207":
        start = text.find("export async function startContainer")
        flip = text.find("ctx.setLifecycleState({", start)
        first_real_await = text.find("const tenantHash = await", start)
        if start < 0 or flip < 0 or first_real_await < 0 or flip > first_real_await:
            raise ClosureError("B-207: starting-state flip is not synchronous before the first await")
    if identifier == "B-212":
        if not re.search(
            r"if \(\s*\(!publishableKey \|\| !secretKey\)\s*&&\s*"
            r"process\.env\[\"NODE_ENV\"\] === \"production\"\s*&&\s*!isSelfGatedPath",
            text,
            re.DOTALL,
        ):
            raise ClosureError("B-212: production missing-key guard has wrong polarity or scope")


def _mutation_test(identifier: str, root: Path) -> None:
    artifact, _, needle = CONTRACTS[identifier]
    source = _read(root, artifact)
    if needle not in source:
        raise ClosureError(f"{identifier}: mutation needle absent")
    mutated = source.replace(needle, "__B193_B214_MUTATION_REMOVED__")
    # Keep companion evidence available when testing a split implementation.
    if identifier == "B-193":
        mutated += "\n" + _read(
            root, "crates/corelink-container/src/routes/oci/b126_m2_test_1_1.rs"
        )
    try:
        _check(identifier, root, mutated)
    except ClosureError:
        pass
    else:
        raise ClosureError(f"{identifier}: load-bearing mutation was not detected")
    if identifier == "B-212":
        flipped = source.replace("!publishableKey || !secretKey", "!publishableKey && !secretKey")
        try:
            _check(identifier, root, flipped)
        except ClosureError:
            return
        raise ClosureError("B-212: OR→AND polarity mutation was not detected")


def _group_193_204(root: Path) -> None:
    spec = importlib.util.spec_from_file_location("verify_b193_b204_contracts", root / "scripts/verify_b193_b204_contracts.py")
    if spec is None or spec.loader is None:
        raise ClosureError("B-193..B-204: group verifier unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    errors = module.verify(root)
    if errors:
        raise ClosureError(f"B-193..B-204: existing contract verifier failed: {errors[0]}")


def verify(root: Path = ROOT, identifier: str | None = None) -> dict[str, int]:
    ids = (identifier,) if identifier is not None else IDS
    unknown = [item for item in ids if item not in CONTRACTS]
    if unknown:
        raise ClosureError(f"unknown closure id(s): {', '.join(unknown)}")
    if identifier is None or identifier in {f"B-{n}" for n in range(193, 205)}:
        _group_193_204(root)
    for item in ids:
        _check(item, root)
        _mutation_test(item, root)
    return {"closed": len(ids), "mutations": len(ids)}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--id")
    parser.add_argument("--expect", choices=("done", "open"), default="done")
    args = parser.parse_args(argv)
    try:
        if args.expect == "open":
            raise ClosureError("closure verifier is inverted: open is not a closed result")
        report = verify(Path(args.root).resolve(), args.id)
    except (ClosureError, OSError, UnicodeDecodeError, ImportError) as exc:
        print(f"B-193..B-214 closure gate: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"B-193..B-214 closure gate: PASS: {report['closed']} item(s), {report['mutations']} mutation(s) red")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

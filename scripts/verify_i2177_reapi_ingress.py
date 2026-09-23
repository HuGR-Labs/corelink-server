#!/usr/bin/env python3
"""Credentialless, mutation-tested source contract for issue #2177."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
INGRESS_FILES = (
    ROOT / "crates/corelink-container/src/reapi_ingress.rs",
    ROOT / "crates/corelink-container/src/reapi_ingress/admission.rs",
    ROOT / "crates/corelink-container/src/reapi_ingress/validation.rs",
)
ROUTES = ROOT / "crates/corelink-container/src/routes/build.rs"
OKF_CONCEPT = ROOT / "docs/knowledge/surfaces/reapi-authenticated-ingress.md"


def call_span(source: str, name: str) -> tuple[int, int] | None:
    start = source.find(name)
    if start < 0:
        return None
    opening = source.find("(", start + len(name))
    if opening < 0:
        return None
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "(":
            depth += 1
        elif source[index] == ")":
            depth -= 1
            if depth == 0:
                return start, index + 1
    return None


def violations(ingress: str, routes: str, okf_concept: str) -> list[str]:
    errors: list[str] = []
    required_ingress = (
        "verify_capability(bearer)",
        "VerifyError::InvalidPat => AuthenticationFailure::Invalid",
        "VerifyError::Backend(_) => AuthenticationFailure::Unavailable",
        "Code::Unauthenticated",
        "Code::Unavailable",
        "Code::PermissionDenied",
        "Code::ResourceExhausted",
        'Status::new(Code::ResourceExhausted, "cache admission limit reached")',
        ".admit(&tenant_id)",
        "pub fn sha256_digest(bytes: &[u8]) -> String",
        "pub fn validate_digest(hash: &str, size_bytes: i64)",
        "pub fn validate_instance_name(instance_name: &str, tenant_id: &str)",
        "pub fn validate_blob_resource_name(",
        "self.access == Access::Write && self.principal.can_write",
        "resolve_storage_cap(&tenant)",
    )
    for item in required_ingress:
        if item not in ingress:
            errors.append(f"ingress is missing contract element: {item}")

    auth_at = ingress.find(".authenticate(bearer)")
    instance_at = ingress.find("validate_instance_name(instance_name, &tenant_id)?")
    scope_at = ingress.find("if access == Access::Write && !can_write")
    admission_at = ingress.find(".admit(&tenant_id)")
    if not (0 <= auth_at < instance_at < scope_at < admission_at):
        errors.append("authorization ordering must be auth → tenant instance → scope → admission")

    required_routes = (
        "build_with_factory_and_byok_and_reapi_ingress",
        "Arc::new(crate::reapi_ingress::QuotaConcurrencyAdmission::new(quota))",
        "ReapiIngress::from_shared_handlers(",
        "D1TenantCapResolver::new(d1.clone())",
        "pub reapi_ingress: Option<crate::reapi_ingress::ReapiIngress>",
    )
    for item in required_routes:
        if item not in routes:
            errors.append(f"route factory is missing shared bundle element: {item}")
    span = call_span(routes, "ReapiIngress::from_shared_handlers")
    bundle = " "
    if span is not None:
        bundle = " ".join(routes[span[0] : span[1]].split())
    shared_handlers = (
        "cas_read.clone(), cas_write.clone(), ac_lookup.clone(), ac_update.clone(),"
    )
    if span is None or shared_handlers not in bundle:
        errors.append("REAPI ingress must receive the route factory's shared decorated CAS/AC handlers")
    if "router = router.merge(reapi_ingress" in routes:
        errors.append("REAPI ingress must remain unmounted pending #2176 and service contracts")

    required_okf = (
        "type: \"CacheSurface\"",
        "crates/corelink-container/src/reapi_ingress.rs",
        "crates/corelink-container/src/reapi_ingress/admission.rs",
        "crates/corelink-container/src/reapi_ingress/validation.rs",
        "crates/corelink-container/src/reapi_ingress/tests.rs",
        "crates/corelink-container/src/routes/build.rs",
        "gRPC remains unmounted",
    )
    for item in required_okf:
        if item not in okf_concept:
            errors.append(f"OKF ingress concept is missing contract element: {item}")
    return errors


def self_test() -> list[str]:
    ingress = "\n".join(path.read_text(encoding="utf-8") for path in INGRESS_FILES)
    routes = ROUTES.read_text(encoding="utf-8")
    okf_concept = OKF_CONCEPT.read_text(encoding="utf-8")
    errors = violations(ingress, routes, okf_concept)
    span = call_span(routes, "ReapiIngress::from_shared_handlers")
    route_prefix = routes
    route_bundle = ""
    route_suffix = ""
    if span is not None:
        route_prefix = routes[: span[0]]
        route_bundle = routes[span[0] : span[1]]
        route_suffix = routes[span[1] :]
    mutations = (
        (
            ingress.replace("verify_capability(bearer)", "verify(bearer)"),
            routes,
            okf_concept,
            "PAT verifier bypass",
        ),
        (
            ingress.replace(
                'Status::new(Code::ResourceExhausted, "cache admission limit reached")',
                'Status::new(Code::Unavailable, "cache admission limit reached")',
            ),
            routes,
            okf_concept,
            "capacity remap",
        ),
        (
            ingress.replace("validate_instance_name(instance_name, &tenant_id)?", "Ok(())?"),
            routes,
            okf_concept,
            "tenant instance bypass",
        ),
        (
            ingress,
            route_prefix
            + route_bundle.replace(
                "cas_read.clone(),",
                "Arc::new(corelink_handler_cas::InMemoryCasHandler::new()),",
                1,
            ),
            okf_concept,
            "parallel CAS handler",
        ),
        (
            ingress,
            routes,
            okf_concept.replace("crates/corelink-container/src/reapi_ingress.rs", ""),
            "ungrounded ingress source",
        ),
    )
    for mutated_ingress, mutated_routes, mutated_okf_concept, label in mutations:
        if not violations(mutated_ingress, mutated_routes, mutated_okf_concept):
            errors.append(f"mutation was not detected: {label}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    errors = self_test() if args.self_test else violations(
        "\n".join(path.read_text(encoding="utf-8") for path in INGRESS_FILES),
        ROUTES.read_text(encoding="utf-8"),
        OKF_CONCEPT.read_text(encoding="utf-8"),
    )
    result = {"check": "i2177-reapi-ingress", "ok": not errors, "errors": errors}
    print(json.dumps(result, sort_keys=True) if args.json else "\n".join(errors or ["#2177 ingress contract: PASS"]))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())

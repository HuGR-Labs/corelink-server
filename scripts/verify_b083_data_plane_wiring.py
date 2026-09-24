#!/usr/bin/env python3
"""Hermetic B-083 guard for production CAS/AC BYOK engagement.

This guard does not contact D1, R2, or a KMS.  It protects the load-bearing
builder calls and the two state-transition rules that must hold before a real
KMS lifecycle drill is meaningful.
"""

from __future__ import annotations

import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CAS_BUILDER = ROOT / "crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs"
AC_BUILDER = ROOT / "crates/corelink-container/src/storage/r2_s3_parts/ac_builder.rs"
BYOK_CACHE = ROOT / "crates/corelink-container/src/storage/byok_cas/part-00.rs"
BYOK_POLICY = ROOT / "crates/corelink-container/src/storage/byok_cas/part-01.rs"
DO_START = ROOT / "worker/src/durable_object_start.ts"
WORKER_ENV = ROOT / "worker/src/index_env.ts"
CONTAINER_CARGO = ROOT / "crates/corelink-container/Cargo.toml"


def require(source: str, needle: str, label: str) -> None:
    if needle not in source:
        raise AssertionError(f"{label}: missing {needle!r}")


def require_provider_feature(manifest: str) -> None:
    try:
        feature = tomllib.loads(manifest).get("features", {}).get("byok-aws-real")
    except tomllib.TOMLDecodeError:
        raise AssertionError("real-provider cfg: invalid container Cargo.toml") from None
    if feature != ["corelink-byok/aws"]:
        raise AssertionError("real-provider cfg: missing active byok-aws-real dependency")


def verify(cas: str, ac: str, cache: str, policy: str, do_start: str, worker_env: str, container_cargo: str) -> None:
    require_provider_feature(container_cargo)
    for label, builder in (("CAS", cas), ("AC", ac)):
        require(builder, "crate::byok_orchestrator::make_provider()", f"{label} real KMS")
        require(builder, "D1ByokConfigReader::new", f"{label} config source")
        require(builder, "D1ByokSecretReader::new", f"{label} wrapped-Tcs source")
        require(builder, "D1ByokEnvelopeStore::new", f"{label} Mode-B envelope store")
        require(
            builder,
            "handler.with_byok(config, tcs).with_byok_random(mode_b)",
            f"{label} production data-plane attachment",
        )
        require(builder, "return Some(", f"{label} fail-closed result propagation")

    require(cache, "let cacheable = matches!", "activation-safe config cache")
    require(
        cache,
        "Some(ByokState::Active | ByokState::Partial | ByokState::Shredded)",
        "security-relevant cached states",
    )
    require(cache, "guard.remove(tenant)", "negative-cache eviction")
    require(policy, "ByokState::Shredded => ByokEngagement::FailClosed", "terminal shred")
    for name in (
        "CORELINK_BYOK_KMS_ACCESS_KEY_ID",
        "CORELINK_BYOK_KMS_SECRET_ACCESS_KEY",
        "CORELINK_BYOK_KMS_SESSION_TOKEN",
    ):
        require(worker_env, f"{name}?: string", f"{name} Worker binding")
        require(do_start, f"ctx.env.{name} ?? \"\"", f"{name} container forward")


def mutation_self_test(
    cas: str, ac: str, cache: str, policy: str, do_start: str, worker_env: str, container_cargo: str
) -> None:
    mutations = {
        "cas-attachment-removed": (
            cas.replace(
                "handler.with_byok(config, tcs).with_byok_random(mode_b)", "handler", 1
            ),
            ac,
            cache,
            policy,
            do_start,
            worker_env,
            container_cargo,
        ),
        "ac-attachment-removed": (
            cas,
            ac.replace(
                "handler.with_byok(config, tcs).with_byok_random(mode_b)", "handler", 1
            ),
            cache,
            policy,
            do_start,
            worker_env,
            container_cargo,
        ),
        "negative-cache-restored": (
            cas,
            ac,
            cache.replace("let cacheable = matches!", "let cacheable = always_matches!", 1),
            policy,
            do_start,
            worker_env,
            container_cargo,
        ),
        "shredded-plaintext": (
            cas,
            ac,
            cache,
            policy.replace(
                "ByokState::Shredded => ByokEngagement::FailClosed",
                "ByokState::Shredded => ByokEngagement::Plaintext //",
                1,
            ),
            do_start,
            worker_env,
            container_cargo,
        ),
        "kms-secret-forward-removed": (
            cas,
            ac,
            cache,
            policy,
            do_start.replace(
                "ctx.env.CORELINK_BYOK_KMS_SECRET_ACCESS_KEY ?? \"\"",
                '""',
                1,
            ),
            worker_env,
            container_cargo,
        ),
    }
    for label, candidate in mutations.items():
        try:
            verify(*candidate)
        except AssertionError:
            continue
        raise AssertionError(f"B083 mutation was accepted: {label}")


def main() -> None:
    sources = tuple(
        path.read_text(encoding="utf-8")
        for path in (CAS_BUILDER, AC_BUILDER, BYOK_CACHE, BYOK_POLICY, DO_START, WORKER_ENV, CONTAINER_CARGO)
    )
    verify(*sources)
    mutation_self_test(*sources)
    print("B083 data-plane wiring PASS: CAS/AC real KMS, activation-safe cache, shred closed")


if __name__ == "__main__":
    main()

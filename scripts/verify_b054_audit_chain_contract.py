#!/usr/bin/env python3
"""Cheap, load-bearing B-054 contract gate.

The bundle lane owns the eventual ``cargo test`` requirement.  This verifier
keeps the per-item backlog oracle bounded by proving the runtime and archive
boundaries are present and by killing mutations that silently accept unknown or
partial epoch metadata.
"""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class ContractError(RuntimeError):
    pass


def assess(files: dict[str, str]) -> None:
    epoch = files["epoch"]
    archive = files["archive"]
    migration = files["migration"]
    required = {
        "unknown algorithm is rejected": "other => Err(EpochError::UnknownAlgorithm { id: other })",
        "keyed epoch requires key": "(LinkAlgorithm::KeyedV2, None) => Err(EpochError::MissingKey)",
        "epoch validates before link": "epoch.validate()?",
        "archive metadata is closed": "audit_outbox sealed epoch metadata is partial, negative, or a downgrade",
        "archive refuses downgrade": "versioned archive rows require the witnessed epoch archive runtime; refusing downgrade",
        "epoch migration present": "CHECK (",
    }
    for label, needle in required.items():
        haystack = migration if label == "epoch migration present" else (archive if "archive" in label else epoch)
        if needle not in haystack:
            raise ContractError(f"{label}: missing {needle}")
    if "match (algorithm, epoch, key)" not in archive:
        raise ContractError("archive metadata parser no longer matches the complete tuple")
    if "_ => Err(" not in archive:
        raise ContractError("archive metadata parser lost its fail-closed fallback")


def mutation_self_test(files: dict[str, str]) -> None:
    mutations = (
        ("unknown version", "other => Err(EpochError::UnknownAlgorithm { id: other })", "other => Ok(LinkAlgorithm::UnkeyedV1)"),
        ("missing key", "(LinkAlgorithm::KeyedV2, None) => Err(EpochError::MissingKey)", "(LinkAlgorithm::KeyedV2, None) => Ok(())"),
        ("archive fallback", "_ => Err(", "_ => Ok((0, 0, None)),"),
    )
    for label, old, new in mutations:
        target = "epoch" if label != "archive fallback" else "archive"
        if old not in files[target]:
            raise ContractError(f"mutation fixture missing: {label}")
        mutant = dict(files)
        mutant[target] = mutant[target].replace(old, new, 1)
        try:
            assess(mutant)
        except ContractError:
            continue
        raise ContractError(f"mutation survived: {label}")


def main() -> int:
    files = {
        "epoch": (ROOT / "crates/corelink-audit-chain/src/epoch.rs").read_text(encoding="utf-8"),
        "archive": (ROOT / "crates/corelink-container/src/routes/audit_archive.rs").read_text(encoding="utf-8"),
        "migration": (ROOT / "migrations/d1/0109_audit_chain_epoch_contract.sql").read_text(encoding="utf-8"),
    }
    assess(files)
    mutation_self_test(files)
    print("B-054 contract: PASS (unknown/partial/downgrade epoch metadata fail closed; mutations red)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ContractError, OSError, UnicodeError) as error:
        raise SystemExit(f"B-054 contract FAILED: {error}")

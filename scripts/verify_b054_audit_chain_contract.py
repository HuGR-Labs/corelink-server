#!/usr/bin/env python3
"""Cheap, load-bearing B-054 contract gate.

The bundle lane owns the eventual ``cargo test`` requirement.  This verifier
keeps the per-item backlog oracle bounded by proving the runtime and archive
boundaries are present and by killing mutations that silently accept unknown or
partial epoch metadata.
"""

from __future__ import annotations

import tomllib
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class ContractError(RuntimeError):
    pass


ARCHIVE_LIVE_MARKER = "B054_ARCHIVE_LIVE_BOUNDARY"
ARCHIVE_LIVE_TOKENS = (
    "B054_ARCHIVE_AUTH_TRUST_ROOT",
    "B054_ARCHIVE_AUTH_LEDGER_CONTIGUOUS",
    "B054_ARCHIVE_AUTH_LEDGER_HASH",
    "B054_ARCHIVE_AUTH_WITNESS_CONTIGUOUS",
    "B054_ARCHIVE_AUTH_WITNESS_LATEST",
    "B054_ARCHIVE_MANIFEST_OBJECT_REQUIRED",
    "B054_ARCHIVE_MANIFEST_OBJECT_HASH",
    "B054_ARCHIVE_MANIFEST_OBJECT_RANGE",
    "B054_ARCHIVE_MANIFEST_OBJECT_EXTRA",
    "B054_ARCHIVE_R2_WRITE_BOUNDARY",
    "B054_ARCHIVE_D1_EXACT_CAS",
)
ARCHIVE_LIVE_AUTH_TOKENS = ARCHIVE_LIVE_TOKENS[:5]
AUDIT_SIGNING_SEED_NAME = "AUDIT_CHAIN_SIGNING_SEED_HEX"
AUDIT_SIGNING_SEED_ASSIGNMENT = re.compile(
    rf"\b{re.escape(AUDIT_SIGNING_SEED_NAME)}\b\s*=\s*([\"'])"
    rf"[0-9a-fA-F]{{64}}\1"
)


def _toml_keys(value: object):
    """Yield TOML table and inline-table keys without inspecting string values."""
    if isinstance(value, dict):
        for key, child in value.items():
            yield key
            yield from _toml_keys(child)
    elif isinstance(value, list):
        for child in value:
            yield from _toml_keys(child)


def assess_deployment_secret_boundary(wrangler: str) -> None:
    """Keep audit signing seed material out of versioned Worker config.

    The seed is intentionally accepted only through the write-only secret
    binding. Parse the TOML so bare and quoted keys are treated identically,
    while comments and string values remain data rather than configuration.
    """
    try:
        document = tomllib.loads(wrangler)
    except tomllib.TOMLDecodeError as error:
        raise ContractError(f"wrangler TOML is invalid: {error}") from error
    if AUDIT_SIGNING_SEED_NAME in _toml_keys(document):
        raise ContractError(
            f"production config must not assign {AUDIT_SIGNING_SEED_NAME}; use a write-only secret"
        )


def deployment_secret_boundary_mutation_self_test(wrangler: str) -> None:
    """Prove exact TOML key forms are rejected without text false positives."""
    assess_deployment_secret_boundary(wrangler)
    marker = 'R2_CAS_REGION = "iad"'
    if marker not in wrangler:
        raise ContractError("deployment secret mutation fixture marker is missing")

    def with_inline_assignment(key: str) -> str:
        return wrangler.replace(
            marker,
            marker + ", " + key + ' = "' + ("0" * 64) + '"',
            1,
        )

    for label, mutant in (
        ("bare key", with_inline_assignment(AUDIT_SIGNING_SEED_NAME)),
        ("quoted key", with_inline_assignment(f'"{AUDIT_SIGNING_SEED_NAME}"')),
    ):
        try:
            assess_deployment_secret_boundary(mutant)
        except ContractError:
            continue
        raise ContractError(f"plaintext deployment secret mutation survived: {label}")

    for label, mutant in (
        (
            "comment",
            wrangler + f'\n# {AUDIT_SIGNING_SEED_NAME} = "' + ("0" * 64) + '"\n',
        ),
        (
            "string value",
            wrangler
            + '\nB054_CUSTODY_NOTE = "'
            + AUDIT_SIGNING_SEED_NAME
            + ' is intentionally write-only"\n',
        ),
    ):
        try:
            assess_deployment_secret_boundary(mutant)
        except ContractError:
            raise ContractError(f"deployment secret text false positive: {label}")


def assess_seed_artifact_boundary(artifacts: dict[str, str]) -> None:
    """Reject seed assignments in tracked source and evidence artifacts.

    A write-only secret must not be copied into a report, log, fixture, or
    generated artifact.  The name may appear in operator documentation, but a
    complete 32-byte hex value next to an assignment is always a leak.
    """
    for path, text in artifacts.items():
        if AUDIT_SIGNING_SEED_ASSIGNMENT.search(text):
            raise ContractError(
                f"tracked artifact contains plaintext {AUDIT_SIGNING_SEED_NAME}: {path}"
            )


def seed_artifact_boundary_mutation_self_test() -> None:
    """Prove a complete seed assignment is rejected while redacted text passes."""
    assess_seed_artifact_boundary(
        {"report.md": 'AUDIT_CHAIN_SIGNING_SEED_HEX = "<REDACTED-64-HEX>"'}
    )
    try:
        assess_seed_artifact_boundary(
            {"report.md": 'AUDIT_CHAIN_SIGNING_SEED_HEX = "' + ("0" * 64) + '"'}
        )
    except ContractError:
        return
    raise ContractError("plaintext seed artifact mutation survived")


def tracked_text_artifacts() -> dict[str, str]:
    """Read tracked files for the seed leak scan without inspecting the worktree."""
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    artifacts: dict[str, str] = {}
    for raw_path in result.stdout.split(b"\0"):
        if not raw_path:
            continue
        path = raw_path.decode("utf-8")
        candidate = ROOT / path
        try:
            artifacts[path] = candidate.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            # Binary assets cannot contain a textual assignment and do not
            # belong to this source/evidence contract scan.
            continue
    return artifacts


def assess_archive_live_boundary(archive: str) -> None:
    """Require explicit review anchors and the irreversible-operation order.

    These anchors are intentionally comments/constants rather than Rust symbol
    names. Refactors may rename helpers, but must preserve an auditable map of
    the trust-root, complete-ledger, complete-witness, exact-object and final
    CAS boundaries. The marker is added only when the live loader replaces the
    fail-closed HOLD path.
    """
    if ARCHIVE_LIVE_MARKER not in archive:
        if "B054_ARCHIVE_AUTH_EVIDENCE_UNAVAILABLE" not in archive:
            raise ContractError("archive has neither live authenticated boundary nor fail-closed HOLD")
        return
    if "B054_ARCHIVE_AUTH_EVIDENCE_UNAVAILABLE" in archive:
        raise ContractError("archive live boundary still contains obsolete keyed HOLD")
    for token in ARCHIVE_LIVE_TOKENS:
        if archive.count(token) != 1:
            raise ContractError(f"archive live boundary requires exactly one {token}")

    r2 = archive.index("B054_ARCHIVE_R2_WRITE_BOUNDARY")
    d1 = archive.index("B054_ARCHIVE_D1_EXACT_CAS")
    if any(archive.index(token) >= r2 for token in ARCHIVE_LIVE_AUTH_TOKENS):
        raise ContractError("archive live boundary permits R2 before authentication completes")
    if r2 >= d1:
        raise ContractError("archive live boundary permits final D1 CAS before R2 publication")


def archive_live_mutation_self_test() -> None:
    """Prove every live-boundary anchor and both ordering checks kill mutants."""
    fixture = "\n".join((ARCHIVE_LIVE_MARKER, *ARCHIVE_LIVE_TOKENS))
    assess_archive_live_boundary(fixture)
    for position, token in enumerate(ARCHIVE_LIVE_TOKENS):
        mutant = fixture.replace(token, f"BROKEN_LIVE_ANCHOR_{position}", 1)
        try:
            assess_archive_live_boundary(mutant)
        except ContractError:
            continue
        raise ContractError(f"archive live mutation survived: {token}")

    for label, mutant in (
        (
            "R2 before authentication",
            fixture.replace(
                "B054_ARCHIVE_AUTH_TRUST_ROOT\nB054_ARCHIVE_AUTH_LEDGER_CONTIGUOUS",
                "B054_ARCHIVE_R2_WRITE_BOUNDARY\nB054_ARCHIVE_AUTH_LEDGER_CONTIGUOUS",
                1,
            ).replace(
                "B054_ARCHIVE_R2_WRITE_BOUNDARY\nB054_ARCHIVE_D1_EXACT_CAS",
                "B054_ARCHIVE_AUTH_TRUST_ROOT\nB054_ARCHIVE_D1_EXACT_CAS",
                1,
            ),
        ),
        (
            "D1 before R2",
            fixture.replace(
                "B054_ARCHIVE_R2_WRITE_BOUNDARY\nB054_ARCHIVE_D1_EXACT_CAS",
                "B054_ARCHIVE_D1_EXACT_CAS\nB054_ARCHIVE_R2_WRITE_BOUNDARY",
                1,
            ),
        ),
    ):
        try:
            assess_archive_live_boundary(mutant)
        except ContractError:
            continue
        raise ContractError(f"archive live ordering mutation survived: {label}")


def assess(files: dict[str, str]) -> None:
    assess_deployment_secret_boundary(files["wrangler"])
    epoch = files["epoch"]
    archive = files["archive"]
    migration = files["migration"]
    witness = files["witness"]
    witness_migration = files["witness_migration"]
    drain = files["drain"]
    d1_commit = files["d1_commit"]
    admin = files["admin"]
    drain_state = files["drain_state"]
    daily_verifier = files["daily_verifier"]
    secret_custody = files["secret_custody"]
    sealed_archive = files["sealed_archive"]
    daily_workflow = files["daily_workflow"]
    required = {
        "unknown algorithm is rejected": "other => Err(EpochError::UnknownAlgorithm { id: other })",
        "keyed epoch requires key": "(LinkAlgorithm::KeyedV2, None) => Err(EpochError::MissingKey)",
        "epoch validates before link": "epoch.validate()?",
        "archive metadata is closed": "audit_outbox sealed epoch metadata is partial, negative, or a downgrade",
        "archive authenticates exact signed manifest": "fn authenticate_archive_manifest",
        "archive binds exact immutable object bytes": "archive object bytes differ from signed manifest",
        "archive recomputes object digest": "hash_domain_payload(&[], bytes) != object.blake3_hash",
        "archive data key is content addressed": '"{}.epoch-{}.{}"',
        "archive ledger supports signing rotation": "load_archive_signing_key_by_id(state, key_id).await?",
        "archive witness heads support signing rotation": "load_archive_signing_key_by_id(state, anchor.signing_key_id).await?",
        "archive binds challenged witness head": "archive manifest differs from authenticated witness head",
        "epoch migration present": "CHECK (",
        "witness latest is challenged": "challenge_b64",
        "witness transport has no proxy": ".no_proxy()",
        "witness receipt signature is verified": "RECEIPT_DOMAIN",
        "v2 drain witnesses before D1": ".append(&head_jcs",
        "v2 D1 rollback assertion": "audit_chain_v2_tx_assert",
        "v2 D1 prefix bounded before witness": "bounded_v2_sealed_prefix(&sealed, now)?",
        "v2 D1 BLOB bytes are explicit": "CAST(?7 AS BLOB),CAST(?8 AS BLOB)",
        "receipt index is append-only": "audit_chain_witness_receipt_no_update",
        "E0 bootstrap verifies complete legacy prefix": "fn b054_verify_legacy_prefix(",
        "legacy bootstrap rejects noncanonical bytes": "B054 legacy row is not exact RFC-8785 JCS",
        "E1 transition requires authenticated E0": "authenticated_e0: &B054AuthenticatedE0Checkpoint",
        "signing registry requires OOB root": "B054 signing registry names unknown OOB trust root",
        "epoch admin requires Security approval header": "B054_SECURITY_APPROVAL_HEADER",
        "epoch admin fails closed without approver": "security_approver_unavailable",
        "daily verifier segments mixed epochs": "while start < lines.len()",
        "daily verifier requires historical keys": "historical link-key id {link_key_id} is not present in keyring",
        "daily verifier rejects epoch gaps": "sealed archive epoch sequence is not forward-contiguous",
        "daily verifier rejects link-key id reuse": "audit link-key rotation reused an earlier key id",
        "daily verifier requires external bootstrap": "authenticated external bootstrap anchor is absent or ambiguous",
        "daily verifier authenticates historical witness receipt": "witness receipt fields do not bind historical record",
        "daily verifier executes Ed25519 verification": "receipt_key,\n        RECEIPT_DOMAIN,",
        "daily verifier selects exact predecessor": "authenticated external bootstrap anchor is absent or ambiguous",
        "daily verifier rejects duplicate sequence": "duplicate sequence number",
        "daily verifier rejects duplicate row": "duplicate row id",
        "daily verifier rejects witness replay": "duplicate/replayed sequence",
        "daily verifier preserves object key boundaries": 'prefix == "staging"',
        "daily verifier challenges witness latest": "challenge_latest(args",
        "daily verifier verifies head signature": "verify_head_signature(head_keys",
        "daily verifier binds epoch ledger key": "epoch ledger hash does not bind link_key_id",
        "daily verifier rejects mixed object partitions": '            "archive object {path} mixes tenant/region partitions"',
        "daily verifier binds every row to day": "contains a cross-day row",
        "daily verifier binds source object key": "archive object path does not match authenticated row key",
        "daily verifier authenticates keyed object suffix": "domain_hash(&[], object_bytes)",
        "daily checkpoint schema is closed": "#[serde(deny_unknown_fields)]",
        "daily checkpoint MAC is domain separated": "hasher.update(CHECKPOINT_MAC_DOMAIN)",
        "daily checkpoint rejects replay": "checkpoint replay or wrong verification day",
        "daily checkpoint binds first row": "first archive row does not match authenticated checkpoint expectations",
        "daily checkpoint output is create-only": "std::fs::hard_link(&temp, path)",
        "daily workflow fetches exact prior checkpoint": "PREV_KEY=\"audit-checkpoints/${PREV_D//-//}/partitions.ndjson\"",
        "daily workflow publishes only nonempty checkpoint after PASS": "elif [[ -s \"${CHECKPOINT_OUT}\" ]]",
        "daily workflow uses immutable conditional create": "-H \"If-None-Match: *\"",
        "daily workflow fetches complete receipt history": "FROM audit_chain_witness_receipt WHERE tenant_id=?1 AND region=?2 ORDER BY witness_sequence",
        "daily workflow discovers keyed objects": "-name '*.ndjson.epoch-*.*'",
        "daily workflow never publishes empty checkpoint": "elif [[ -s \"${CHECKPOINT_OUT}\" ]]",
        "daily workflow captures real verifier status": "VERIFIER_RC=${PIPESTATUS[0]}",
        "daily workflow rejects silent verifier failure": "[[ \"${VERIFIER_RC}\" -ne 0 ]]",
        "daily workflow passes witness latest URL": "--witness-url \"${AUDIT_WITNESS_URL}\"",
        "daily workflow passes head registry": "--witness-head-public-keys \"${WITNESS_HEAD_KEYS_PATH}\"",
        "daily workflow passes epoch evidence": "--witness-epoch-ledger-dir \"${WITNESS_EPOCH_LEDGER_DIR}\"",
        "daily workflow masks witness append token": "add-mask::${AUDIT_WITNESS_APPEND_TOKEN:-}",
        "daily workflow cleans witness evidence": "WITNESS_EVIDENCE_ROOT:-}",
        "archive verifier rejects sequence overflow": "expected_seq.checked_add(1)",
        "key custody retains historical link keys": "retain historical keys for the full audit-retention lifetime",
        "key custody makes historic loss indeterminate": "loss of a historical key is `INDETERMINATE`",
    }
    for label, needle in required.items():
        if label == "epoch migration present":
            haystack = migration
        elif label == "archive verifier rejects sequence overflow":
            haystack = sealed_archive
        elif "archive" in label:
            haystack = archive
        elif label.startswith("witness"):
            haystack = witness
        elif label in {"v2 D1 rollback assertion", "v2 D1 BLOB bytes are explicit"}:
            haystack = d1_commit
        elif label.startswith("v2"):
            haystack = drain
        elif label.startswith("receipt"):
            haystack = witness_migration
        elif label == "epoch admin requires Security approval header":
            haystack = drain_state
        elif label.startswith("daily workflow"):
            haystack = daily_workflow
        elif label.startswith("daily verifier") or label.startswith("daily checkpoint"):
            haystack = daily_verifier
        elif label.startswith("key custody"):
            haystack = secret_custody
        elif label.startswith(("E0", "E1", "legacy", "signing", "epoch admin")):
            haystack = admin
        else:
            haystack = epoch
        if needle not in haystack:
            raise ContractError(f"{label}: missing {needle}")
    if "match (algorithm, epoch, key)" not in archive:
        raise ContractError("archive metadata parser no longer matches the complete tuple")
    if "_ => Err(" not in archive:
        raise ContractError("archive metadata parser lost its fail-closed fallback")
    assess_archive_live_boundary(archive)
    if "remove the old key after all links are migrated" in secret_custody:
        raise ContractError("key custody still permits destruction of retained historic-key evidence")
    if daily_workflow.count("VERIFIER_RC=${PIPESTATUS[0]}") != 2:
        raise ContractError("daily workflow must capture the verifier process status in both branches")


def mutation_self_test(files: dict[str, str]) -> None:
    mutations = (
        ("unknown version", "other => Err(EpochError::UnknownAlgorithm { id: other })", "other => Ok(LinkAlgorithm::UnkeyedV1)"),
        ("missing key", "(LinkAlgorithm::KeyedV2, None) => Err(EpochError::MissingKey)", "(LinkAlgorithm::KeyedV2, None) => Ok(())"),
        ("archive fallback", "_ => Err(", "_ => Ok((0, 0, None)),"),
        (
            "archive object bytes",
            "hash_domain_payload(&[], bytes) != object.blake3_hash",
            "false",
        ),
        (
            "daily mixed epochs",
            "while start < lines.len()",
            "while false",
        ),
        (
            "historical key custody",
            "retain historical keys for the full audit-retention lifetime",
            "remove historical keys after rotation",
        ),
        (
            "external bootstrap",
            "authenticated external bootstrap anchor is absent or ambiguous",
            "local E0 bootstrap accepted",
        ),
        (
            "archive sequence overflow",
            "expected_seq.checked_add(1)",
            "expected_seq.saturating_add(1)",
        ),
        (
            "checkpoint MAC",
            "hasher.update(CHECKPOINT_MAC_DOMAIN)",
            "// checkpoint MAC domain removed",
        ),
        (
            "checkpoint replay",
            "checkpoint replay or wrong verification day",
            "checkpoint day accepted without binding",
        ),
        (
            "checkpoint immutable local publish",
            "std::fs::hard_link(&temp, path)",
            "std::fs::rename(&temp, path)",
        ),
        (
            "bootstrap signature",
            "receipt_key,\n        RECEIPT_DOMAIN,",
            "Ok::<(), String>(()) // receipt signature bypass",
        ),
        ("duplicate sequence", "duplicate sequence number", "duplicate sequence accepted"),
        ("witness replay", "duplicate/replayed sequence", "witness replay accepted"),
        (
            "object key boundary",
            'prefix == "staging"',
            'prefix == "copiedstaging"',
        ),
        ("witness latest challenge", "challenge_latest(args", "historical receipt accepted"),
        ("head signature", "verify_head_signature(head_keys", "head signature trusted"),
        ("epoch ledger binding", "epoch ledger hash does not bind link_key_id", "ledger key omitted"),
        (
            "mixed object partition",
            '            "archive object {path} mixes tenant/region partitions"',
            '            "archive object {path} mixed partition accepted"',
        ),
        ("cross-day copy", "contains a cross-day row", "cross-day row accepted"),
        ("keyed object suffix", "domain_hash(&[], object_bytes)", "\"00\".repeat(32)"),
        (
            "witness latest URL wiring",
            "--witness-url \"${AUDIT_WITNESS_URL}\"",
            "--witness-url \"\"",
        ),
        (
            "witness evidence cleanup",
            "WITNESS_EVIDENCE_ROOT:-}",
            "WITNESS_EVIDENCE_ROOT_MISSING:-}",
        ),
        ("empty checkpoint publish", "elif [[ -s \"${CHECKPOINT_OUT}\" ]]", "elif [[ -f \"${CHECKPOINT_OUT}\" ]]"),
        ("silent verifier nonzero", "VERIFIER_RC=${PIPESTATUS[0]}", "VERIFIER_RC=0"),
    )
    for label, old, new in mutations:
        if label in {"empty checkpoint publish", "silent verifier nonzero", "witness latest URL wiring", "witness evidence cleanup"}:
            target = "daily_workflow"
        elif label in {"daily mixed epochs", "external bootstrap", "bootstrap signature", "duplicate sequence", "witness replay", "object key boundary", "witness latest challenge", "head signature", "epoch ledger binding", "mixed object partition", "cross-day copy", "keyed object suffix", "checkpoint MAC", "checkpoint replay", "checkpoint immutable local publish"}:
            target = "daily_verifier"
        elif label == "historical key custody":
            target = "secret_custody"
        elif label == "archive sequence overflow":
            target = "sealed_archive"
        else:
            target = "epoch" if label not in {"archive fallback", "archive object bytes"} else "archive"
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
        "witness": (ROOT / "crates/corelink-container/src/routes/audit_drain/b054_witness.rs").read_text(encoding="utf-8"),
        "drain": (ROOT / "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs").read_text(encoding="utf-8"),
        "d1_commit": "".join(
            (ROOT / path).read_text(encoding="utf-8")
            for path in (
                "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs",
                "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs",
            )
        ),
        "admin": "".join(
            (ROOT / path).read_text(encoding="utf-8")
            for path in (
                "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin.rs",
                "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin_part2.rs",
            )
        ),
        "drain_state": (ROOT / "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs").read_text(encoding="utf-8"),
        "witness_migration": (ROOT / "migrations/d1/0124_audit_chain_witness_receipts.sql").read_text(encoding="utf-8"),
        "daily_verifier": (ROOT / "crates/corelink-audit-chain/src/bin/verifier.rs").read_text(encoding="utf-8"),
        "secret_custody": (ROOT / "docs/internal/secrets-checklist.md").read_text(encoding="utf-8"),
        "sealed_archive": (ROOT / "crates/corelink-audit-chain/src/sealed_archive.rs").read_text(encoding="utf-8"),
        "daily_workflow": (ROOT / ".github/workflows/audit-chain-daily-verify.yml").read_text(encoding="utf-8"),
        "wrangler": (ROOT / "wrangler.toml").read_text(encoding="utf-8"),
    }
    assess(files)
    mutation_self_test(files)
    archive_live_mutation_self_test()
    deployment_secret_boundary_mutation_self_test(files["wrangler"])
    seed_artifact_boundary_mutation_self_test()
    assess_seed_artifact_boundary(tracked_text_artifacts())
    print("B-054 contract: PASS (unknown/partial/downgrade epoch metadata fail closed; mutations red)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ContractError, OSError, UnicodeError) as error:
        raise SystemExit(f"B-054 contract FAILED: {error}")

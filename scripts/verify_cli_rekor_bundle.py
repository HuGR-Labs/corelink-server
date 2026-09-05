#!/usr/bin/env python3
"""Require a Rekor inclusion entry for the exact cosign-signed payload."""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
import re
from pathlib import Path


def _decode_hash(value: object, label: str) -> bytes:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{label} must be a 32-byte SHA-256 hash")
    if len(value) == 64:
        try:
            return bytes.fromhex(value)
        except ValueError:
            pass
    try:
        decoded = base64.b64decode(value + "=" * (-len(value) % 4), validate=True)
    except (ValueError, binascii.Error) as error:
        raise ValueError(f"{label} must be a 32-byte SHA-256 hash") from error
    if len(decoded) != hashlib.sha256().digest_size:
        raise ValueError(f"{label} must be a 32-byte SHA-256 hash")
    return decoded


def _decode_body(value: object) -> bytes:
    if not isinstance(value, str) or not value:
        raise ValueError("canonicalizedBody is missing")
    try:
        return base64.b64decode(value + "=" * (-len(value) % 4), validate=True)
    except (ValueError, binascii.Error) as error:
        raise ValueError("canonicalizedBody is not valid base64") from error


def _nonnegative_int(value: object) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value if value >= 0 else None
    if isinstance(value, str) and value.isdigit():
        return int(value)
    return None


def _inclusion_root(leaf: bytes, index: int, tree_size: int,
                    siblings: list[bytes]) -> bytes:
    """Recompute an RFC 6962 inclusion root from a Rekor proof path.

    Rekor's inclusion proof is a Merkle proof, not merely a non-empty JSON
    field.  RFC 6962 recursively splits an ``n``-leaf tree at the largest
    power of two strictly smaller than ``n``.  This matters for a right-edge
    leaf in a non-power-of-two tree: its audit path can contain fewer siblings
    than a level-by-level ceil-halving walk would predict.
    """
    if index < 0 or tree_size <= 0 or index >= tree_size:
        raise ValueError("Rekor inclusion proof has an invalid leaf index/tree size")

    def node(left: bytes, right: bytes) -> bytes:
        return hashlib.sha256(b"\x01" + left + right).digest()

    cursor = 0

    def largest_power_below(value: int) -> int:
        power = 1
        while power * 2 < value:
            power *= 2
        return power

    def walk(leaf_index: int, size: int) -> bytes:
        nonlocal cursor
        if size == 1:
            return leaf
        split = largest_power_below(size)
        if leaf_index < split:
            left = walk(leaf_index, split)
            if cursor >= len(siblings):
                raise ValueError("Rekor inclusion proof is missing a sibling hash")
            right = siblings[cursor]
        else:
            right = walk(leaf_index - split, size - split)
            if cursor >= len(siblings):
                raise ValueError("Rekor inclusion proof is missing a sibling hash")
            left = siblings[cursor]
        cursor += 1
        return node(left, right)

    root = walk(index, tree_size)
    if cursor != len(siblings):
        raise ValueError(
            f"Rekor inclusion proof has {len(siblings)} hashes; expected {cursor}"
        )
    return root


def verify(payload_path: Path, bundle_path: Path) -> None:
    expected = hashlib.sha256(payload_path.read_bytes()).hexdigest()
    bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    entries = bundle.get("verificationMaterial", {}).get("tlogEntries", [])
    if not entries:
        raise ValueError("cosign bundle has no Rekor tlog entry")
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        # Zero is valid for the first log entry and for synthetic/frozen-time
        # fixtures.  Presence, type, and range are the checks; truthiness is
        # not a cryptographic predicate.
        log_index = _nonnegative_int(entry.get("logIndex"))
        integrated_time = _nonnegative_int(entry.get("integratedTime"))
        if log_index is None:
            continue
        if integrated_time is None:
            continue
        log_id = entry.get("logId")
        if (not isinstance(log_id, dict)
                or not isinstance(log_id.get("keyId"), str)
                or not log_id["keyId"]):
            continue
        body = entry.get("canonicalizedBody")
        proof = entry.get("inclusionProof")
        if not isinstance(proof, dict):
            continue
        try:
            decoded = _decode_body(body)
            record = json.loads(decoded)
            proof_index = _nonnegative_int(proof.get("logIndex", log_index))
            tree_size = _nonnegative_int(proof.get("treeSize"))
            hashes = proof.get("hashes")
            if proof_index != log_index:
                continue
            if (tree_size is None or tree_size <= log_index
                    or not isinstance(hashes, list)):
                continue
            siblings = [_decode_hash(value, "inclusionProof.hashes entry")
                        for value in hashes]
            root = _decode_hash(proof.get("rootHash"), "inclusionProof.rootHash")
            observed_root = _inclusion_root(
                hashlib.sha256(b"\x00" + decoded).digest(),
                log_index, tree_size, siblings,
            )
            if observed_root != root:
                continue
        except (ValueError, json.JSONDecodeError, TypeError):
            continue
        try:
            hash_record = record["spec"]["data"]["hash"]
            algorithm = hash_record["algorithm"]
            digest = hash_record["value"]
        except (KeyError, TypeError):
            continue
        if (algorithm == "sha256" and isinstance(digest, str)
                and re.fullmatch(r"[0-9a-fA-F]{64}", digest)
                and digest.lower() == expected):
            return
    raise ValueError("cosign bundle has no Rekor inclusion bound to the provenance digest")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--payload", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    args = parser.parse_args()
    try:
        verify(args.payload, args.bundle)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"cli-release-rekor: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

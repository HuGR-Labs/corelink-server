"""Content-addressable-storage (CAS) helpers shared by the sync and async
clients.

The CoreLink native CAS is **BLAKE3-keyed**: a blob's address is the 64-char
lowercase hex BLAKE3 digest of its bytes, with no algorithm prefix. The server
recomputes ``blake3(body)`` on every write and rejects a claim that does not
match (HTTP 422), so the digest the client puts in the URL path is load-bearing
— it MUST be a real BLAKE3 digest, never SHA-256.

Wire contract (native routes, ``crates/corelink-container/src/routes/cas.rs``):

  Operation | Method | Path                        | Body        | Success
  --------- | ------ | --------------------------- | ----------- | -------
  put       | PUT    | /v1/cas/{tenant}/{digest}   | raw bytes   | 201 / 200
  get       | GET    | /v1/cas/{tenant}/{digest}   | —           | 200 (bytes)
  stat      | HEAD   | /v1/cas/{tenant}/{digest}   | —           | 200 / 404

Auth is ``Authorization: Bearer <PAT>``; the fronting Worker resolves the
tenant from the PAT and the path ``{tenant}`` is a client echo that must match.
"""

from __future__ import annotations

import re
from collections.abc import Iterator
from dataclasses import dataclass
from typing import IO

from blake3 import blake3

from .exceptions import CoreLinkDigestMismatchError

# Default streaming chunk size (4 MiB) — matches the how-to guide and stays well
# under the container's 10 MiB body limit for the single-object PUT path.
DEFAULT_CHUNK_SIZE = 4 * 1024 * 1024

# Canonical CAS digest: 64 lowercase hex chars, no prefix. Mirrors the server's
# `is_canonical_digest` gate (a malformed hash is rejected 400 before any R2 key
# is derived), so we reject it client-side too and never issue a doomed request.
_DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")


@dataclass(frozen=True)
class StatResult:
    """Metadata for a stored CAS blob (returned by ``stat``)."""

    digest: str
    """BLAKE3 hex digest (64 hex chars, no prefix)."""
    size_bytes: int
    """Size of the stored blob in bytes (0 when the blob is absent)."""
    exists: bool
    """Whether the blob is present in the tenant's CAS."""


def is_canonical_digest(digest: str) -> bool:
    """Return ``True`` iff *digest* is a canonical 64-char lowercase-hex BLAKE3
    key (the exact shape the server's ``is_canonical_digest`` gate accepts)."""
    return bool(_DIGEST_RE.match(digest))


def validate_digest(digest: str) -> str:
    """Return *digest* unchanged if canonical, else raise :class:`ValueError`.

    Used to fail fast on a caller-supplied ``expected_digest`` before it is
    baked into a request URL.
    """
    if not is_canonical_digest(digest):
        raise ValueError(
            f"invalid BLAKE3 digest {digest!r}: expected 64 lowercase hex chars"
        )
    return digest


def compute_digest(data: bytes) -> str:
    """Compute the canonical BLAKE3 hex digest of *data*."""
    return blake3(data).hexdigest()


def resolve_put_digest(data: bytes, expected_digest: str | None) -> str:
    """Resolve the digest to key a ``put`` under.

    When *expected_digest* is supplied it is trusted (format-validated only) so
    the caller can skip recomputation, exactly as the how-to documents; a stale
    or forged value is caught server-side (422 → :class:`CoreLinkDigestMismatchError`).
    Otherwise the digest is computed over *data*.
    """
    if expected_digest is not None:
        return validate_digest(expected_digest)
    return compute_digest(data)


def hash_stream(fileobj: IO[bytes], chunk_size: int) -> str:
    """Compute the BLAKE3 digest of a seekable binary file **without loading it
    fully into memory**, then rewind it to its starting offset so the same
    handle can be streamed to the server.

    Requires a seekable handle (a real file). Raises :class:`ValueError` for a
    non-seekable stream — the single-object PUT path must know the digest up
    front (it is the URL), so a one-pass upload of an unseekable stream is not
    possible on this route.
    """
    if not fileobj.seekable():
        raise ValueError(
            "put_stream requires a seekable file object (the BLAKE3 digest is "
            "the URL and must be computed before upload); pass expected_digest= "
            "for a non-seekable stream"
        )
    start = fileobj.tell()
    hasher = blake3()
    while True:
        chunk = fileobj.read(chunk_size)
        if not chunk:
            break
        hasher.update(chunk)
    fileobj.seek(start)
    return hasher.hexdigest()


def iter_file(fileobj: IO[bytes], chunk_size: int) -> Iterator[bytes]:
    """Yield *fileobj* in ``chunk_size`` byte chunks for a streaming upload."""
    while True:
        chunk = fileobj.read(chunk_size)
        if not chunk:
            break
        yield chunk


def verify_bytes(digest: str, data: bytes) -> None:
    """Raise :class:`CoreLinkDigestMismatchError` if ``blake3(data) != digest``.

    The client-side half of CTRL-CAS-002 (default-on verify): a ``get`` whose
    bytes were corrupted on the wire never reaches the caller as valid data.
    """
    actual = compute_digest(data)
    if actual != digest:
        raise CoreLinkDigestMismatchError(
            "client-side BLAKE3 verify failed: bytes do not match requested digest",
            expected=digest,
            actual=actual,
        )

"""
CoreLink Python SDK — type stubs for IDE intellisense.

Generated from corelink-py PyO3 extension module.
Client-verify is default-on per CTRL-CAS-002.
"""

from typing import Optional

class StatResult:
    """Metadata returned by CoreLinkClient.stat()."""

    digest: str
    """BLAKE3 hex digest (64 hex chars, no prefix)."""
    size_bytes: int
    """Size of the stored blob in bytes."""
    exists: bool
    """Whether the blob is present in the CAS."""

    def __repr__(self) -> str: ...

class CoreLinkClient:
    """
    CoreLink CAS client for Python.

    Wraps the Rust corelink-client-verify crate (single source of truth
    per ADR-0016) via PyO3 FFI. All BLAKE3 integrity verification uses
    the canonical Rust implementation — no per-language drift.

    Example::

        import asyncio
        import os
        from corelink import CoreLinkClient

        async def main() -> None:
            client = CoreLinkClient(
                pat=os.environ["CORELINK_PAT"],
                tenant_id="acme-corp",
            )
            digest = await client.put(b"hello world")
            data = await client.get(digest)
            stat = await client.stat(digest)
            print(f"size={stat.size_bytes} exists={stat.exists}")

        asyncio.run(main())

    """

    def __init__(
        self,
        pat: str,
        tenant_id: str,
        client_verify: bool = True,
    ) -> None:
        """
        Construct a new CoreLinkClient.

        Args:
            pat: Personal Access Token. Read from ``CORELINK_PAT`` env var
                in idiomatic usage.
            tenant_id: Tenant scope for all CAS operations.
            client_verify: BLAKE3 integrity check after every ``get()``.
                Defaults to ``True`` per CTRL-CAS-002. Passing ``False``
                logs a warning (``"DISABLE NOT RECOMMENDED"``) and opts out.
        """
        ...

    @property
    def _inner_client_verify_enabled(self) -> bool:
        """
        Whether client-side BLAKE3 verify is enabled.

        Exposed for test inspection:
        ``assert client._inner_client_verify_enabled == True``.
        """
        ...

    def get(self, digest: str) -> bytes:
        """
        Download a blob by BLAKE3 hex digest.

        Args:
            digest: 64-char hex BLAKE3 digest (no ``blake3:`` prefix).

        Returns:
            Raw bytes of the stored blob.

        Raises:
            RuntimeError: BLAKE3 mismatch (``COR_CAS_DIGEST_MISMATCH``) or
                server error.
            ValueError: Invalid digest format.
        """
        ...

    def put(self, data: bytes) -> str:
        """
        Upload bytes to the CAS.

        Returns:
            BLAKE3 hex digest (64 chars) of the uploaded content.

        Raises:
            RuntimeError: Upload failed (auth or network).
        """
        ...

    def stat(self, digest: str) -> StatResult:
        """
        Return metadata for a stored blob.

        Args:
            digest: 64-char BLAKE3 hex digest.

        Returns:
            :class:`StatResult` with size and existence info.

        Raises:
            ValueError: Invalid digest format.
            RuntimeError: Server error.
        """
        ...

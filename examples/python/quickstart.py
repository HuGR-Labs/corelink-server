#!/usr/bin/env python3
"""
CoreLink Python SDK quickstart — corelink-py.

Requires:
    pip install corelink-py

Environment:
    CORELINK_PAT=<your-pat>

Usage:
    python quickstart.py
"""

import asyncio
import os
import sys

from corelink import CoreLinkClient  # type: ignore[import]


async def main() -> None:
    """Demonstrate put/get/stat with client-verify default-on."""
    pat = os.environ.get("CORELINK_PAT", "")
    if not pat:
        print("ERROR: CORELINK_PAT environment variable not set", file=sys.stderr)
        sys.exit(1)

    # client_verify defaults to True per CTRL-CAS-002.
    # Pass client_verify=False to opt out (warning logged).
    client = CoreLinkClient(
        pat=pat,
        tenant_id="acme-corp",
        # client_verify=True  # default; shown for clarity
    )

    print(f"client_verify_enabled: {client._inner_client_verify_enabled}")

    # Put
    data = b"hello from CoreLink Python SDK"
    digest = client.put(data)
    print(f"Uploaded:  blake3:{digest}")

    # Get (client-verify enforced by Rust single truth)
    downloaded = client.get(digest)
    print(f"Downloaded: {len(downloaded)} bytes")
    assert downloaded == data, "data round-trip mismatch"

    # Stat
    stat = client.stat(digest)
    print(f"Stat:      digest={stat.digest[:8]}... size={stat.size_bytes} exists={stat.exists}")

    print("OK — Python SDK quickstart complete.")


if __name__ == "__main__":
    asyncio.run(main())

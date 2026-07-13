# corelink-py

CoreLink content-addressable-storage (CAS) client for Python — `async`/`await`
native, with client-side BLAKE3 integrity verification **on by default**
(CTRL-CAS-002).

The client wraps the canonical Rust `corelink-client-verify` crate (single
source of truth per ADR-0016) over PyO3 FFI, so BLAKE3 hashing and verification
never drift between languages.

## Install

```bash
pip install corelink-py
```

Requires Python ≥ 3.10.

## Quickstart

```python
import asyncio
import os
from corelink import CoreLinkClient

async def main() -> None:
    client = CoreLinkClient(
        pat=os.environ["CORELINK_PAT"],
        tenant_id="acme-corp",
    )
    digest = await client.put(b"hello world")   # -> 64-char hex BLAKE3
    data = await client.get(digest)             # verifies BLAKE3 on download
    stat = await client.stat(digest)
    print(f"size={stat.size_bytes} exists={stat.exists}")

asyncio.run(main())
```

## API

| Method | Description |
| --- | --- |
| `CoreLinkClient(pat, tenant_id, client_verify=True)` | Construct a client. Read `pat` from the `CORELINK_PAT` env var in idiomatic usage; all operations are scoped to `tenant_id`. |
| `await put(data: bytes) -> str` | Store a blob; returns its 64-char hex BLAKE3 digest. |
| `await get(digest: str) -> bytes` | Download by digest. Raises `RuntimeError` (`COR_CAS_DIGEST_MISMATCH`) if the returned bytes fail the BLAKE3 check. |
| `await stat(digest: str) -> StatResult` | Existence + `size_bytes` without downloading the body. |

### Client-verify

`client_verify` defaults to `True` (CTRL-CAS-002): every `get()` re-hashes the
downloaded bytes with BLAKE3 and rejects a mismatch. Passing `client_verify=False`
logs a `"DISABLE NOT RECOMMENDED"` warning and opts out — do not disable it in
production.

## Examples

Runnable quickstarts live in [`examples/`](./examples): `quickstart_put.py`,
`quickstart_get.py`, `quickstart_stats.py`, `quickstart_list.py`, and more.

## License

UNLICENSED — © HumanGuardrail. Internal / customer distribution only.

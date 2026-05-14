# CoreLink Python SDK (`corelink-py`)

> PyO3-based Python wrapper with async/await native asyncio integration.
> Client-verify default-on per CTRL-CAS-002 via single Rust truth.

## Installation

```sh
pip install corelink-py
```

## Quick Start

```python
import asyncio
import os
from corelink import CoreLinkClient

async def main() -> None:
    client = CoreLinkClient(
        pat=os.environ["CORELINK_PAT"],
        tenant_id="acme-corp",
    )
    # client_verify=True by default (CTRL-CAS-002)
    digest = client.put(b"hello world")
    data = client.get(digest)
    stat = client.stat(digest)
    print(f"digest={digest[:16]}... size={stat.size_bytes}")

asyncio.run(main())
```

## API Reference

### `CoreLinkClient(pat, tenant_id, client_verify=True)`

| Parameter | Type | Default | Description |
|---|---|---|---|
| `pat` | `str` | required | Personal Access Token (`CORELINK_PAT` env var). |
| `tenant_id` | `str` | required | Tenant scope for CAS operations. |
| `client_verify` | `bool` | `True` | BLAKE3 integrity check after `get()`. Opt-out logs a warning. |

#### `client.put(data: bytes) -> str`

Upload bytes; returns 64-char BLAKE3 hex digest.

```python
digest = client.put(b"artifact bytes")
print(f"blake3:{digest}")
```

#### `client.get(digest: str) -> bytes`

Download blob by digest. Raises `RuntimeError` (`COR_CAS_DIGEST_MISMATCH`)
on integrity failure.

```python
data = client.get("6b86b273ff34fc...2e6c...abcd")
```

#### `client.stat(digest: str) -> StatResult`

Return metadata: `digest`, `size_bytes`, `exists`.

```python
stat = client.stat(digest)
if not stat.exists:
    print("blob not found")
```

#### `client._inner_client_verify_enabled: bool`

Test inspection property. `True` when client-verify is active.

## Client-Verify Opt-Out

Opt-out is explicit and logs a canonical warning:

```python
client = CoreLinkClient(
    pat=os.environ["CORELINK_PAT"],
    tenant_id="acme-corp",
    client_verify=False,   # "DISABLE NOT RECOMMENDED" warning logged
)
assert client._inner_client_verify_enabled is False
```

## Type Stubs

The package ships `corelink.pyi` for IDE intellisense (PyCharm, VS Code + Pylance).

## Error Codes

| Code | Meaning |
|---|---|
| `COR_CAS_DIGEST_MISMATCH` | BLAKE3 hash of downloaded bytes does not match requested digest. |
| `COR_CAS_VERIFY_DISABLED` | Verify was disabled and `get()` was called anyway. |

## Memory Safety

The Python wrapper is validated with valgrind on every CI build:

```sh
valgrind --leak-check=full pytest tests/
```

Target: 0 leaks, 0 invalid reads/writes.

## Build from Source

```sh
# Install maturin
pip install maturin

# Build the extension module
cd crates/corelink-py
maturin build --release --features extension-module

# Run Rust unit tests
cargo test -p corelink-py
```

## Design

Client-verify uses the single Rust truth (`corelink-client-verify`, S-02 SEALED)
via PyO3 FFI. See `specs/_decisions/ADR-0016-ffi-vs-native-http.md` for
the trade-off analysis (FFI vs native HTTP per language).

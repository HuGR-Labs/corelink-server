# CoreLink Go SDK (`corelink-go`)

> cgo-based Go wrapper with context-based API.
> Client-verify default-on per CTRL-CAS-002 via single Rust truth.

## Installation

```sh
go get github.com/humangr-labs/corelink-go/v1
```

Requires `libcorelink_go.{so,dylib,a}` compiled from `crates/corelink-go`:

```sh
cargo build --release -p corelink-go
```

Set `CGO_LDFLAGS` to point at the release target:

```sh
export CGO_LDFLAGS="-L$(pwd)/target/release -lcorelink_go"
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "os"

    corelink "github.com/humangr-labs/corelink-go/v1"
)

func main() {
    client, err := corelink.NewClient(corelink.Config{
        PAT:      os.Getenv("CORELINK_PAT"),
        TenantID: "acme-corp",
        // ClientVerify defaults to true per CTRL-CAS-002
    })
    if err != nil { panic(err) }
    defer client.Close()

    ctx := context.Background()
    digest, err := client.Put(ctx, []byte("hello world"))
    if err != nil { panic(err) }
    fmt.Printf("Uploaded: blake3:%s\n", digest)

    data, err := client.Get(ctx, digest)
    if err != nil { panic(err) }
    fmt.Printf("Downloaded %d bytes\n", len(data))

    stat, err := client.Stat(ctx, digest)
    if err != nil { panic(err) }
    fmt.Printf("Exists: %v\n", stat.Exists)
}
```

## API Reference

### `corelink.NewClient(cfg Config) (*Client, error)`

| Field | Type | Default | Description |
|---|---|---|---|
| `PAT` | `string` | required | Personal Access Token. |
| `TenantID` | `string` | required | Tenant scope. |
| `ClientVerify` | `bool` | `true` (enforced) | BLAKE3 verify on `Get()`. |
| `ClientVerifyExplicitFalse` | `bool` | `false` | Must be `true` to opt out. Prevents accidental opt-out via zero-value Config. |

Returns `error` if PAT is empty or cgo construction fails.

### `(*Client).Put(ctx, data []byte) (string, error)`

Upload bytes; returns 64-char BLAKE3 hex digest.

```go
digest, err := client.Put(ctx, []byte("artifact"))
```

### `(*Client).Get(ctx, digest string) ([]byte, error)`

Download blob by digest. Returns `error` containing
`COR_CAS_DIGEST_MISMATCH` on integrity failure.

```go
data, err := client.Get(ctx, "6b86b273ff34fc...")
if err != nil {
    log.Printf("integrity check failed: %v", err)
}
```

### `(*Client).Stat(ctx, digest string) (StatResult, error)`

Return metadata: `Digest`, `SizeBytes`, `Exists`.

### `(*Client).IsClientVerifyEnabled() bool`

Test inspection: `assert client.IsClientVerifyEnabled() == true`.

### `(*Client).Close()`

Release cgo resources. Call via `defer client.Close()`.

## Client-Verify Opt-Out

Zero-value Config does NOT opt out (guarded by `ClientVerifyExplicitFalse`):

```go
client, err := corelink.NewClient(corelink.Config{
    PAT:                       os.Getenv("CORELINK_PAT"),
    TenantID:                  "acme-corp",
    ClientVerify:              false,
    ClientVerifyExplicitFalse: true, // both required to opt out
})
// Warning "DISABLE NOT RECOMMENDED" is logged
assert.False(t, client.IsClientVerifyEnabled())
```

## Memory Safety and Race Detector

```sh
go test -race ./...
```

All tests pass under the Go race detector. The Rust layer is validated
separately via valgrind (see `crates/corelink-go`).

## Error Reference

| Error string | Meaning |
|---|---|
| `COR_CAS_DIGEST_MISMATCH` | Downloaded bytes did not match requested digest. |
| `COR_CAS_VERIFY_DISABLED` | Verify was disabled and `Get()` was called. |

## Build from Source

```sh
# Build the Rust cdylib
cargo build --release -p corelink-go

# Run Go tests (race detector)
cd corelink-go
CGO_LDFLAGS="-L../target/release -lcorelink_go" go test -race ./...
```

## Design

Client-verify uses the single Rust truth (`corelink-client-verify`, S-02 SEALED)
via cgo. See `specs/_decisions/ADR-0016-ffi-vs-native-http.md`.

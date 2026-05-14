// Package corelink provides a context-based Go client for the CoreLink CAS.
//
// Client-verify (BLAKE3 integrity check post-download) is default-on per
// CTRL-CAS-002, enforced via the single Rust truth in
// crates/corelink-go (which wraps corelink-client-verify S-02).
//
// # Usage
//
//	client, err := corelink.NewClient(corelink.Config{
//	    PAT:      os.Getenv("CORELINK_PAT"),
//	    TenantID: "acme-corp",
//	})
//	if err != nil { panic(err) }
//	defer client.Close()
//
//	ctx := context.Background()
//	digest, err := client.Put(ctx, []byte("hello"))
//	data, err := client.Get(ctx, digest)
//	stat, err := client.Stat(ctx, digest)
//
// # Build
//
// Requires the corelink-go Rust crate to be compiled first:
//
//	cargo build --release -p corelink-go
//
// Then build the Go module with cgo:
//
//	CGO_LDFLAGS="-L../target/release -lcorelink_go" go build ./...
package corelink

/*
#cgo LDFLAGS: -lcorelink_go
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// Forward declarations for the Rust C-ABI symbols.
// These are defined in crates/corelink-go (libcorelink_go).

typedef struct CorelinkGoClient CorelinkGoClient;

extern CorelinkGoClient* corelink_go_client_new(
    const uint8_t* pat_ptr, size_t pat_len,
    const uint8_t* tenant_id_ptr, size_t tenant_id_len,
    uint8_t client_verify
);
extern void corelink_go_client_free(CorelinkGoClient* handle);
extern uint8_t corelink_go_client_is_verify_enabled(const CorelinkGoClient* handle);
extern int32_t corelink_go_client_put(
    const uint8_t* body_ptr, size_t body_len,
    uint8_t* out_digest_hex
);
extern int32_t corelink_go_client_verify_get(
    const CorelinkGoClient* handle,
    const uint8_t* body_ptr, size_t body_len,
    const uint8_t* digest_hex_ptr, size_t digest_hex_len
);
*/
import "C"

import (
	"context"
	"errors"
	"fmt"
	"unsafe"
)

// Config holds construction parameters for a [Client].
type Config struct {
	// PAT is the Personal Access Token. Read from CORELINK_PAT env var in
	// idiomatic usage. Never passed in CLI args (CTRL-CRED-001).
	PAT string

	// TenantID scopes all CAS operations.
	TenantID string

	// ClientVerify enables BLAKE3 integrity check after every Get().
	// Defaults to true per CTRL-CAS-002. Setting false emits a warning log.
	// Zero-value (false) is overridden to true in NewClient unless
	// explicitly disabled via ClientVerifyExplicitFalse.
	ClientVerify bool

	// ClientVerifyExplicitFalse must be set to true alongside
	// ClientVerify=false to opt out. This prevents accidental opt-out via
	// zero-initialization of Config (where ClientVerify would be false by
	// default in Go). Pattern mirrors the Rust split-constructor design.
	ClientVerifyExplicitFalse bool
}

// Client is a CoreLink CAS client for Go.
//
// Wraps the Rust corelink-go crate via cgo. All BLAKE3 integrity
// verification uses the canonical Rust implementation (single source of
// truth, ADR-0016). The client is safe to call from multiple goroutines
// after construction.
type Client struct {
	handle *C.CorelinkGoClient
}

// NewClient constructs a new [Client] from cfg.
//
// Client-verify defaults to true. To opt out, set both
// cfg.ClientVerify = false AND cfg.ClientVerifyExplicitFalse = true.
// Opt-out emits a warning log with "DISABLE NOT RECOMMENDED".
//
// Returns an error if PAT is empty or cfg is otherwise invalid.
func NewClient(cfg Config) (*Client, error) {
	if cfg.PAT == "" {
		return nil, errors.New("corelink: PAT must not be empty")
	}
	if cfg.TenantID == "" {
		return nil, errors.New("corelink: TenantID must not be empty")
	}

	// Enforce default-on: if ClientVerifyExplicitFalse is not set, treat
	// ClientVerify as true regardless of its zero value.
	clientVerify := uint8(1)
	if cfg.ClientVerifyExplicitFalse && !cfg.ClientVerify {
		clientVerify = 0
	}

	patBytes := []byte(cfg.PAT)
	tidBytes := []byte(cfg.TenantID)

	// SAFETY: patBytes and tidBytes are valid Go slices for the duration of
	// the cgo call. corelink_go_client_new copies the data.
	handle := C.corelink_go_client_new(
		(*C.uint8_t)(unsafe.Pointer(&patBytes[0])), C.size_t(len(patBytes)),
		(*C.uint8_t)(unsafe.Pointer(&tidBytes[0])), C.size_t(len(tidBytes)),
		C.uint8_t(clientVerify),
	)
	if handle == nil {
		return nil, errors.New("corelink: failed to construct client (invalid PAT or TenantID encoding)")
	}
	return &Client{handle: handle}, nil
}

// Close releases resources held by the client. Must be called exactly once
// (idiomatic: `defer client.Close()`).
func (c *Client) Close() {
	if c.handle != nil {
		C.corelink_go_client_free(c.handle)
		c.handle = nil
	}
}

// IsClientVerifyEnabled returns true if client-verify (BLAKE3 check) is
// active on this client.
//
// Test inspection: assert client.IsClientVerifyEnabled() == true.
func (c *Client) IsClientVerifyEnabled() bool {
	return C.corelink_go_client_is_verify_enabled(c.handle) != 0
}

// Put uploads data to the CAS and returns the BLAKE3 hex digest (64 chars).
//
// The digest is computed locally using the single Rust truth. The production
// impl then sends the blob + digest to the CoreLink server.
func (c *Client) Put(_ context.Context, data []byte) (string, error) {
	var outHex [64]byte
	var bodyPtr *C.uint8_t
	if len(data) > 0 {
		bodyPtr = (*C.uint8_t)(unsafe.Pointer(&data[0]))
	}
	rc := C.corelink_go_client_put(
		bodyPtr,
		C.size_t(len(data)),
		(*C.uint8_t)(unsafe.Pointer(&outHex[0])),
	)
	if rc != 0 {
		return "", fmt.Errorf("corelink: put failed (rc=%d)", rc)
	}
	return string(outHex[:]), nil
}

// Get downloads a blob by BLAKE3 hex digest.
//
// When client-verify is enabled (default), the returned bytes are
// BLAKE3-verified against digest before returning. Returns an error on
// integrity mismatch (COR_CAS_DIGEST_MISMATCH).
//
// Note: this is a stub that returns empty bytes for the empty-blob digest
// and errors otherwise. The production network layer is wired in the
// shipping SDK binary.
func (c *Client) Get(_ context.Context, digest string) ([]byte, error) {
	if len(digest) != 64 {
		return nil, fmt.Errorf("corelink: invalid digest length %d (expected 64)", len(digest))
	}

	// Stub: production impl fetches body from server.
	body := []byte{}

	if !c.IsClientVerifyEnabled() {
		return body, nil
	}

	digestBytes := []byte(digest)
	var bodyPtr *C.uint8_t // null for empty body
	rc := C.corelink_go_client_verify_get(
		c.handle,
		bodyPtr,
		C.size_t(0),
		(*C.uint8_t)(unsafe.Pointer(&digestBytes[0])),
		C.size_t(len(digestBytes)),
	)
	switch int(rc) {
	case 0:
		return body, nil
	case 1:
		return nil, fmt.Errorf("COR_CAS_DIGEST_MISMATCH: digest mismatch for %s", digest)
	case 2:
		return nil, errors.New("COR_CAS_VERIFY_DISABLED")
	default:
		return nil, fmt.Errorf("corelink: get verify failed (rc=%d)", rc)
	}
}

// StatResult holds metadata for a stored blob.
type StatResult struct {
	// Digest is the BLAKE3 hex digest.
	Digest string
	// SizeBytes is the blob size in bytes.
	SizeBytes int64
	// Exists indicates whether the blob is present.
	Exists bool
}

// Stat returns metadata for a stored blob.
//
// Note: stub implementation returns exists=false. Production impl queries
// the CoreLink server.
func (c *Client) Stat(_ context.Context, digest string) (StatResult, error) {
	if len(digest) != 64 {
		return StatResult{}, fmt.Errorf("corelink: invalid digest length %d", len(digest))
	}
	return StatResult{
		Digest:    digest,
		SizeBytes: 0,
		Exists:    false,
	}, nil
}

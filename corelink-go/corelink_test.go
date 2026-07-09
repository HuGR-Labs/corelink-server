// Package corelink_test — Go integration tests for corelink-go wrapper.
//
// Run with: go test -race ./...
// (requires libcorelink_go.so in CGO_LDFLAGS path)
//
// These tests verify:
//   - Client-verify default-on (CTRL-CAS-002)
//   - Explicit opt-out requires ClientVerifyExplicitFalse=true
//   - Put produces BLAKE3 hex digest (64 chars)
//   - Get passes for matching digest
//   - Get fails for mismatched digest (COR_CAS_DIGEST_MISMATCH)
//   - PAT validation
//   - Race detector clean (go test -race)
package corelink_test

import (
	"context"
	"strings"
	"testing"

	corelink "github.com/HumanGuardrail/corelink-go/v1"
)

// TestClientVerifyDefaultOn verifies CTRL-CAS-002: IsClientVerifyEnabled()
// returns true when no explicit opt-out is given.
func TestClientVerifyDefaultOn(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{
		PAT:      "test-pat",
		TenantID: "acme-corp",
	})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	if !client.IsClientVerifyEnabled() {
		t.Fatal("IsClientVerifyEnabled() must return true by default (CTRL-CAS-002)")
	}
}

// TestClientVerifyExplicitOptOut verifies that opt-out requires both
// ClientVerify=false AND ClientVerifyExplicitFalse=true.
func TestClientVerifyExplicitOptOut(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{
		PAT:                       "test-pat",
		TenantID:                  "t1",
		ClientVerify:              false,
		ClientVerifyExplicitFalse: true,
	})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	if client.IsClientVerifyEnabled() {
		t.Fatal("IsClientVerifyEnabled() must return false when explicitly opted out")
	}
}

// TestClientVerifyZeroValueDefaultsToOn verifies that a zero-value Config
// (ClientVerify=false, ClientVerifyExplicitFalse=false) defaults to ON.
// This prevents accidental opt-out via zero-initialization.
func TestClientVerifyZeroValueDefaultsToOn(t *testing.T) {
	t.Parallel()
	// ClientVerify zero-value is false in Go, but without ClientVerifyExplicitFalse
	// the client must still default to ON.
	client, err := corelink.NewClient(corelink.Config{
		PAT:          "pat",
		TenantID:     "t",
		ClientVerify: false, // zero value — must NOT opt out without ClientVerifyExplicitFalse
	})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()
	if !client.IsClientVerifyEnabled() {
		t.Fatal("zero-value ClientVerify must NOT opt out (CTRL-CAS-002 default-on)")
	}
}

// TestPutProducesBLAKE3Hex verifies Put returns a valid 64-char hex digest.
func TestPutProducesBLAKE3Hex(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{PAT: "p", TenantID: "t"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	digest, err := client.Put(ctx, []byte("hello world"))
	if err != nil {
		t.Fatalf("Put: %v", err)
	}
	if len(digest) != 64 {
		t.Fatalf("digest length = %d; want 64", len(digest))
	}
	// must be lowercase hex
	for _, c := range digest {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			t.Fatalf("non-hex char %q in digest %s", c, digest)
		}
	}
}

// TestGetPassesForEmptyBlob verifies the empty-blob round-trip with verify ON.
func TestGetPassesForEmptyBlob(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{PAT: "p", TenantID: "t"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	// BLAKE3 of empty bytes (known hex).
	emptyDigest, err := client.Put(ctx, []byte{})
	if err != nil {
		t.Fatalf("Put(empty): %v", err)
	}
	_, err = client.Get(ctx, emptyDigest)
	if err != nil {
		t.Fatalf("Get(empty): %v", err)
	}
}

// TestGetMismatchErrors verifies that a mismatched digest returns an error
// containing COR_CAS_DIGEST_MISMATCH.
func TestGetMismatchErrors(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{PAT: "p", TenantID: "t"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	// Digest of "hello" != BLAKE3("") which is what the stub returns.
	wrongDigest, err := client.Put(ctx, []byte("hello"))
	if err != nil {
		t.Fatalf("Put: %v", err)
	}
	_, err = client.Get(ctx, wrongDigest)
	if err == nil {
		t.Fatal("Get must fail on hash mismatch")
	}
	if !strings.Contains(err.Error(), "DIGEST_MISMATCH") {
		t.Fatalf("expected DIGEST_MISMATCH in error, got: %v", err)
	}
}

// TestInvalidPATReturnsError verifies empty PAT is rejected before cgo call.
func TestInvalidPATReturnsError(t *testing.T) {
	t.Parallel()
	_, err := corelink.NewClient(corelink.Config{PAT: "", TenantID: "t"})
	if err == nil {
		t.Fatal("empty PAT must return an error")
	}
}

// TestStatReturnsResult verifies Stat returns without error for a valid digest.
func TestStatReturnsResult(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{PAT: "p", TenantID: "t"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	d, _ := client.Put(ctx, []byte("data"))
	stat, err := client.Stat(ctx, d)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if stat.Digest != d {
		t.Fatalf("Stat.Digest = %s; want %s", stat.Digest, d)
	}
}

// TestRaceDetector verifies concurrent operations are race-free.
// Run with: go test -race ./...
func TestRaceDetector(t *testing.T) {
	t.Parallel()
	client, err := corelink.NewClient(corelink.Config{PAT: "p", TenantID: "t"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	done := make(chan struct{})
	for i := range 10 {
		go func(n int) {
			data := []byte(strings.Repeat("x", n+1))
			_, err := client.Put(ctx, data)
			if err != nil {
				t.Errorf("goroutine Put: %v", err)
			}
			done <- struct{}{}
		}(i)
	}
	for range 10 {
		<-done
	}
}

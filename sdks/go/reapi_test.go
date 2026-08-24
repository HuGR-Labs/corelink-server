package corelink

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
)

// newTestClient builds a Client wired to srv, with an Instance set (needed
// for every /bazel/v2/{instance}/... path).
func newTestClient(t *testing.T, srv *httptest.Server) *Client {
	t.Helper()
	cli, err := NewClient(Config{
		Endpoint: srv.URL,
		PAT:      "test-pat",
		Instance: "acme-corp",
		TenantID: "acme-corp",
	})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	return cli
}

// ─── ByteStream round trip ─────────────────────────────────────────────────

// TestByteStreamRoundTrip exercises Write then Read against an in-memory
// httptest server, with a blob large enough (256 KiB, well over io.Copy's
// 32 KiB default chunk size) that Read must perform more than one
// underlying read to reassemble it.
func TestByteStreamRoundTrip(t *testing.T) {
	const blobSize = 256 * 1024 // > 32 KiB io.Copy buffer, several times over

	data := make([]byte, blobSize)
	if _, err := rand.Read(data); err != nil {
		t.Fatalf("generating random blob: %v", err)
	}
	digest := ComputeBLAKE3(data)

	var (
		mu    sync.Mutex
		store = map[string][]byte{}
	)

	mux := http.NewServeMux()
	mux.HandleFunc("PUT /bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		hash := r.PathValue("hash")
		body, err := readAll(r)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		mu.Lock()
		store[hash] = body
		mu.Unlock()
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("GET /bazel/v2/{instance}/blobs/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		hash := r.PathValue("hash")
		mu.Lock()
		blob, ok := store[hash]
		mu.Unlock()
		if !ok {
			http.Error(w, "not found", http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Type", "application/octet-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(blob)
	})

	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)
	ctx := context.Background()

	if err := cli.ByteStream().Write(ctx, digest, bytes.NewReader(data)); err != nil {
		t.Fatalf("ByteStream.Write: %v", err)
	}

	var out bytes.Buffer
	n, err := cli.ByteStream().Read(ctx, digest, &out)
	if err != nil {
		t.Fatalf("ByteStream.Read: %v", err)
	}
	if n != int64(blobSize) {
		t.Fatalf("expected %d bytes read, got %d", blobSize, n)
	}
	if !bytes.Equal(out.Bytes(), data) {
		t.Fatalf("round-tripped blob does not match original")
	}
}

// TestByteStreamReadDigestMismatch verifies Read rejects a blob whose bytes
// don't hash to the requested digest, per the client-side verification
// contract (Config.ClientVerify's doc comment on corelink.go).
func TestByteStreamReadDigestMismatch(t *testing.T) {
	real := []byte("the real content")
	wrong := ComputeBLAKE3([]byte("something else entirely"))

	mux := http.NewServeMux()
	mux.HandleFunc("GET /bazel/v2/{instance}/blobs/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(real)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)

	_, err := cli.ByteStream().Read(context.Background(), wrong, &bytes.Buffer{})
	if err == nil {
		t.Fatalf("expected a digest-mismatch error")
	}
	var mismatch *DigestMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("expected *DigestMismatchError, got %T: %v", err, err)
	}
}

// TestByteStreamWriteContextCancellation checks that Write honours ctx
// cancellation mid-transfer instead of blocking until the whole reader is
// drained.
func TestByteStreamWriteContextCancellation(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("PUT /bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		// Drain slowly is unnecessary; just block reading the body so the
		// client-side cancellation is what ends the request.
		<-r.Context().Done()
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)

	ctx, cancel := context.WithCancel(context.Background())
	// io.Pipe's Read blocks until something is written (or the pipe is
	// closed), so Write's request body has no data to send and stays in
	// flight until ctx is cancelled.
	pr, pw := io.Pipe()
	defer pw.Close()

	done := make(chan error, 1)
	go func() {
		done <- cli.ByteStream().Write(ctx, Digest{Hash: fmt64Hash("x"), SizeBytes: 1 << 30}, pr)
	}()

	cancel()

	err := <-done
	if err == nil {
		t.Fatalf("expected Write to fail once ctx was cancelled")
	}
}

// ─── FindMissing ────────────────────────────────────────────────────────────

// TestFindMissingSubset verifies FindMissing round-trips the blobDigests /
// missingBlobDigests JSON shape and returns exactly the subset the server
// names as missing.
func TestFindMissingSubset(t *testing.T) {
	all := []Digest{
		ComputeBLAKE3([]byte("alpha")),
		ComputeBLAKE3([]byte("beta")),
		ComputeBLAKE3([]byte("gamma")),
	}
	// The server reports only "beta" as missing.
	wantMissing := all[1]

	mux := http.NewServeMux()
	mux.HandleFunc("POST /bazel/v2/{instance}/findMissingBlobs", func(w http.ResponseWriter, r *http.Request) {
		var req findMissingRequestBody
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		if len(req.BlobDigests) != len(all) {
			http.Error(w, fmt.Sprintf("expected %d digests, got %d", len(all), len(req.BlobDigests)), http.StatusBadRequest)
			return
		}
		resp := findMissingResponseBody{MissingBlobDigests: []digestJSON{
			{Hash: wantMissing.Hash, SizeBytes: wantMissing.SizeBytes},
		}}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(resp)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)

	missing, err := cli.CAS().FindMissing(context.Background(), all)
	if err != nil {
		t.Fatalf("FindMissing: %v", err)
	}
	if len(missing) != 1 {
		t.Fatalf("expected exactly 1 missing digest, got %d: %v", len(missing), missing)
	}
	if !missing[0].Equal(wantMissing) {
		t.Fatalf("expected missing digest %v, got %v", wantMissing, missing[0])
	}
}

// TestFindMissingAuthError verifies a 401 from findMissingBlobs surfaces as
// a typed *AuthError, checkable with errors.As.
func TestFindMissingAuthError(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /bazel/v2/{instance}/findMissingBlobs", func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, `{"message":"bad PAT"}`, http.StatusUnauthorized)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)

	_, err := cli.CAS().FindMissing(context.Background(), []Digest{ComputeBLAKE3([]byte("x"))})
	if err == nil {
		t.Fatalf("expected an error")
	}
	var authErr *AuthError
	if !errors.As(err, &authErr) {
		t.Fatalf("expected *AuthError, got %T: %v", err, err)
	}
	if authErr.StatusCode != http.StatusUnauthorized {
		t.Fatalf("expected status 401, got %d", authErr.StatusCode)
	}
}

// ─── ActionCache ────────────────────────────────────────────────────────────

// TestActionCacheGetMiss verifies a 404 on the AC read path is surfaced as
// ErrActionCacheMiss, checkable via errors.Is -- the mechanism the
// published examples rely on.
//
// ActionCacheClient.Get is implemented in cas.go, not reapi.go; this test
// exercises it as a caller would, through the public surface this file's
// Update shares a wire format with.
func TestActionCacheGetMiss(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /bazel/v2/{instance}/blobs/ac/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "not found", http.StatusNotFound)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)

	actionDigest := ComputeBLAKE3([]byte("some-action"))
	_, err := cli.ActionCache().Get(context.Background(), actionDigest)
	if err == nil {
		t.Fatalf("expected an error on a cache miss")
	}
	if !errors.Is(err, ErrActionCacheMiss) {
		t.Fatalf("expected errors.Is(err, ErrActionCacheMiss), got %T: %v", err, err)
	}
}

// TestActionCacheUpdateThenGet round-trips an ActionResult through Update
// and Get against an in-memory AC store, verifying Update PUTs to the
// right path with a payload Get can decode back into the same result.
func TestActionCacheUpdateThenGet(t *testing.T) {
	var (
		mu    sync.Mutex
		store = map[string][]byte{}
	)

	mux := http.NewServeMux()
	mux.HandleFunc("PUT /bazel/v2/{instance}/blobs/ac/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		hash := r.PathValue("hash")
		body, err := readAll(r)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		mu.Lock()
		store[hash] = body
		mu.Unlock()
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("GET /bazel/v2/{instance}/blobs/ac/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		hash := r.PathValue("hash")
		mu.Lock()
		payload, ok := store[hash]
		mu.Unlock()
		if !ok {
			http.Error(w, "not found", http.StatusNotFound)
			return
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(payload)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)
	ctx := context.Background()

	actionDigest := ComputeBLAKE3([]byte("build //foo:bar"))
	result := ActionResult{
		ExitCode:  0,
		StdoutRaw: []byte("build succeeded\n"),
		OutputFiles: []OutputFile{
			{Path: "bazel-out/foo.o", Digest: ComputeBLAKE3([]byte("object code")), IsExecutable: false},
		},
	}

	if err := cli.ActionCache().Update(ctx, actionDigest, result); err != nil {
		t.Fatalf("ActionCache.Update: %v", err)
	}

	got, err := cli.ActionCache().Get(ctx, actionDigest)
	if err != nil {
		t.Fatalf("ActionCache.Get: %v", err)
	}
	if got.ExitCode != result.ExitCode {
		t.Fatalf("ExitCode: expected %d, got %d", result.ExitCode, got.ExitCode)
	}
	if !bytes.Equal(got.StdoutRaw, result.StdoutRaw) {
		t.Fatalf("StdoutRaw: expected %q, got %q", result.StdoutRaw, got.StdoutRaw)
	}
	if len(got.OutputFiles) != len(result.OutputFiles) {
		t.Fatalf("OutputFiles: expected %d entries, got %d", len(result.OutputFiles), len(got.OutputFiles))
	}
}

// ─── CAS batch surface ──────────────────────────────────────────────────────

// TestCASBatchUpdateAndRead exercises BatchUpdate/BatchRead against the
// same single-blob routes BatchUpdate/BatchRead are implemented on top of
// (there is no native batch route on this server -- see BatchUpdate's doc
// comment in reapi.go).
func TestCASBatchUpdateAndRead(t *testing.T) {
	var (
		mu    sync.Mutex
		store = map[string][]byte{}
	)

	mux := http.NewServeMux()
	mux.HandleFunc("PUT /bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		hash := r.PathValue("hash")
		body, err := readAll(r)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		mu.Lock()
		store[hash] = body
		mu.Unlock()
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("GET /bazel/v2/{instance}/blobs/{hash}/{size}", func(w http.ResponseWriter, r *http.Request) {
		hash := r.PathValue("hash")
		mu.Lock()
		blob, ok := store[hash]
		mu.Unlock()
		if !ok {
			http.Error(w, "not found", http.StatusNotFound)
			return
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(blob)
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)
	ctx := context.Background()

	blobs := map[Digest][]byte{
		ComputeBLAKE3([]byte("one")):   []byte("one"),
		ComputeBLAKE3([]byte("two")):   []byte("two"),
		ComputeBLAKE3([]byte("three")): []byte("three"),
	}

	if err := cli.CAS().BatchUpdate(ctx, blobs); err != nil {
		t.Fatalf("BatchUpdate: %v", err)
	}

	digests := make([]Digest, 0, len(blobs))
	for d := range blobs {
		digests = append(digests, d)
	}

	read, err := cli.CAS().BatchRead(ctx, digests)
	if err != nil {
		t.Fatalf("BatchRead: %v", err)
	}
	if len(read) != len(blobs) {
		t.Fatalf("expected %d blobs read back, got %d", len(blobs), len(read))
	}
	for d, want := range blobs {
		got, ok := read[d]
		if !ok {
			t.Fatalf("missing digest %v in BatchRead result", d)
		}
		if !bytes.Equal(got, want) {
			t.Fatalf("digest %v: expected %q, got %q", d, want, got)
		}
	}
}

// ─── Health / Capabilities ──────────────────────────────────────────────────

func TestHealth(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /_health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status":"ok","storage":"r2"}`))
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	cli := newTestClient(t, srv)

	status, err := cli.Health(context.Background())
	if err != nil {
		t.Fatalf("Health: %v", err)
	}
	if !status.OK {
		t.Fatalf("expected HealthStatus.OK to be true")
	}
}

func TestCapabilitiesNotAvailable(t *testing.T) {
	// No server needed: Capabilities never issues a request because no
	// such endpoint exists (see reapi.go's doc comment on Capabilities).
	cli, err := NewClient(Config{Endpoint: "https://example.invalid", PAT: "pat", Instance: "acme"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if _, err := cli.Capabilities(context.Background()); err == nil {
		t.Fatalf("expected Capabilities to return an error (no server surface exists)")
	}
}

// ─── test helpers ───────────────────────────────────────────────────────────

func readAll(r *http.Request) ([]byte, error) {
	defer r.Body.Close()
	buf := new(bytes.Buffer)
	if _, err := buf.ReadFrom(r.Body); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func fmt64Hash(seed string) string {
	d := ComputeBLAKE3([]byte(seed))
	return d.Hash
}

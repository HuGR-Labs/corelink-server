package corelink

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func newCASClient(t *testing.T, srv *httptest.Server) *Client {
	t.Helper()
	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "pat"}, WithTenantID("acme-corp"))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	return cli
}

// TestPutGetRoundTrip proves Put uploads to PUT /v1/cas/{tenant}/{digest}
// with the client-computed BLAKE3 digest in the path, and that Get, hitting
// the same path, returns the same bytes it verifies against that digest.
func TestPutGetRoundTrip(t *testing.T) {
	want := []byte("hello corelink")
	wantDigest := ComputeBLAKE3(want)

	store := map[string][]byte{}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		wantPath := "/v1/cas/acme-corp/" + wantDigest.Hash
		if r.URL.Path != wantPath {
			t.Errorf("request path = %q, want %q", r.URL.Path, wantPath)
		}
		switch r.Method {
		case http.MethodPut:
			body, readErr := io.ReadAll(r.Body)
			if readErr != nil {
				t.Errorf("reading PUT body: %v", readErr)
			}
			store[r.URL.Path] = body
			w.WriteHeader(http.StatusCreated)
		case http.MethodGet:
			data, ok := store[r.URL.Path]
			if !ok {
				w.WriteHeader(http.StatusNotFound)
				return
			}
			w.WriteHeader(http.StatusOK)
			_, _ = w.Write(data)
		default:
			t.Errorf("unexpected method %s", r.Method)
		}
	}))
	defer srv.Close()

	cli := newCASClient(t, srv)

	gotDigest, err := cli.Put(context.Background(), want)
	if err != nil {
		t.Fatalf("Put: %v", err)
	}
	if !gotDigest.Equal(wantDigest) {
		t.Fatalf("Put digest = %s, want %s", gotDigest, wantDigest)
	}

	gotBytes, err := cli.Get(context.Background(), gotDigest)
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if string(gotBytes) != string(want) {
		t.Fatalf("Get bytes = %q, want %q", gotBytes, want)
	}
}

// TestGetRejectsPoisonedBody is the poison test: it feeds Get a body that
// does not hash to the digest that was asked for -- simulating a poisoned
// or corrupted cache entry -- and asserts Get refuses to hand the bytes
// back, surfacing a *DigestMismatchError instead. This is the client-side
// half of CTRL-CAS-002 and must not be skippable by accident.
func TestGetRejectsPoisonedBody(t *testing.T) {
	requested := ComputeBLAKE3([]byte("the real content"))
	poisoned := []byte("not the real content at all")

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(poisoned)
	}))
	defer srv.Close()

	cli := newCASClient(t, srv)

	if !cli.IsClientVerifyEnabled() {
		t.Fatalf("precondition failed: client-side verify must be enabled for this test to mean anything")
	}

	data, err := cli.Get(context.Background(), requested)
	if err == nil {
		t.Fatalf("expected Get to reject a poisoned body, got nil error and %d bytes", len(data))
	}
	if data != nil {
		t.Fatalf("expected no bytes returned on a rejected poisoned body, got %d bytes", len(data))
	}

	var mismatch *DigestMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("expected *DigestMismatchError via errors.As, got %T: %v", err, err)
	}
	if !mismatch.Expected.Equal(requested) {
		t.Fatalf("DigestMismatchError.Expected = %s, want %s", mismatch.Expected, requested)
	}
	wantActual := ComputeBLAKE3(poisoned)
	if !mismatch.Actual.Equal(wantActual) {
		t.Fatalf("DigestMismatchError.Actual = %s, want %s", mismatch.Actual, wantActual)
	}
}

// TestGetNotFound proves a 404 from the server surfaces through Get as the
// typed *NotFoundError, not a bare status or a generic error.
func TestGetNotFound(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_, _ = w.Write([]byte(`{"message":"no such digest"}`))
	}))
	defer srv.Close()

	cli := newCASClient(t, srv)

	d := ComputeBLAKE3([]byte("does not exist"))
	_, err := cli.Get(context.Background(), d)
	if err == nil {
		t.Fatalf("expected an error for a 404 response")
	}
	var notFound *NotFoundError
	if !errors.As(err, &notFound) {
		t.Fatalf("expected *NotFoundError via errors.As, got %T: %v", err, err)
	}
}

// TestPutDigestMismatch proves a 422 from the server -- the shape the
// server uses when it recomputes BLAKE3 and disagrees with the digest in
// the URL -- surfaces through Put as *DigestMismatchError, not a bare
// status.
func TestPutDigestMismatch(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnprocessableEntity)
		_, _ = w.Write([]byte(`{"code":"HashMismatch","message":"digest does not match body"}`))
	}))
	defer srv.Close()

	cli := newCASClient(t, srv)

	body := []byte("some content")
	wantDigest := ComputeBLAKE3(body)

	_, err := cli.Put(context.Background(), body)
	if err == nil {
		t.Fatalf("expected an error for a 422 response")
	}
	var mismatch *DigestMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("expected *DigestMismatchError via errors.As, got %T: %v", err, err)
	}
	if !mismatch.Expected.Equal(wantDigest) {
		t.Fatalf("DigestMismatchError.Expected = %s, want %s", mismatch.Expected, wantDigest)
	}
}

// TestStatExists proves Stat issues a HEAD and reports presence + size from
// Content-Length without ever reading a body.
func TestStatExists(t *testing.T) {
	d := ComputeBLAKE3([]byte("stat me"))

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodHead {
			t.Errorf("Stat used method %s, want HEAD", r.Method)
		}
		wantPath := "/v1/cas/acme-corp/" + d.Hash
		if r.URL.Path != wantPath {
			t.Errorf("request path = %q, want %q", r.URL.Path, wantPath)
		}
		w.Header().Set("Content-Length", "7")
		w.WriteHeader(http.StatusOK)
	}))
	defer srv.Close()

	cli := newCASClient(t, srv)

	st, err := cli.Stat(context.Background(), d)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if !st.Exists {
		t.Fatalf("Stat.Exists = false, want true")
	}
	if st.Digest != d.Hash {
		t.Fatalf("Stat.Digest = %q, want %q", st.Digest, d.Hash)
	}
	if st.SizeBytes != 7 {
		t.Fatalf("Stat.SizeBytes = %d, want 7", st.SizeBytes)
	}
}

// TestStatAbsent proves Stat reports Stat{Exists: false} rather than an
// error when the server answers 404 -- mirroring corelink-py's stat(),
// which treats an absent blob as a negative result, not an exception.
func TestStatAbsent(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
	}))
	defer srv.Close()

	cli := newCASClient(t, srv)

	d := ComputeBLAKE3([]byte("absent"))
	st, err := cli.Stat(context.Background(), d)
	if err != nil {
		t.Fatalf("Stat: unexpected error for an absent blob: %v", err)
	}
	if st.Exists {
		t.Fatalf("Stat.Exists = true, want false")
	}
}

// TestCASOperationsRequireTenant proves Put/Get/Stat fail client-side, with
// no network call, when the client was built without a tenant -- the
// native CAS routes are tenant-scoped in the URL path.
func TestCASOperationsRequireTenant(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Errorf("no request should reach the server: got %s %s", r.Method, r.URL.Path)
	}))
	defer srv.Close()

	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	if _, err := cli.Put(context.Background(), []byte("x")); err == nil {
		t.Fatalf("expected Put to fail without a tenant")
	}
	d := ComputeBLAKE3([]byte("x"))
	if _, err := cli.Get(context.Background(), d); err == nil {
		t.Fatalf("expected Get to fail without a tenant")
	}
	if _, err := cli.Stat(context.Background(), d); err == nil {
		t.Fatalf("expected Stat to fail without a tenant")
	}
}

// TestGetHonoursContextCancellation proves a cancelled context aborts a
// slow Get instead of waiting for the server.
func TestGetHonoursContextCancellation(t *testing.T) {
	unblock := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		<-unblock
		w.WriteHeader(http.StatusOK)
	}))
	defer srv.Close()
	defer close(unblock)

	cli := newCASClient(t, srv)

	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	d := ComputeBLAKE3([]byte("slow"))
	_, err := cli.Get(ctx, d)
	if err == nil {
		t.Fatalf("expected an error when the context times out before the server responds")
	}
	if !errors.Is(err, context.DeadlineExceeded) && !strings.Contains(err.Error(), "context deadline exceeded") {
		t.Fatalf("expected a context-deadline error, got %T: %v", err, err)
	}
}

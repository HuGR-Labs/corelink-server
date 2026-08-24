package corelink

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestNewClientRequiresEndpoint(t *testing.T) {
	_, err := NewClient(Config{PAT: "pat"})
	if err == nil {
		t.Fatalf("expected error when Endpoint is empty")
	}
}

func TestNewClientRequiresPAT(t *testing.T) {
	_, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com"})
	if err == nil {
		t.Fatalf("expected error when PAT is empty")
	}
}

func TestNewClientNoNetworkIO(t *testing.T) {
	// A bogus, unreachable endpoint must not cause NewClient itself to
	// fail or block: it validates and returns, it does not connect.
	cli, err := NewClient(Config{
		Endpoint: "https://127.0.0.1.invalid.example:1",
		PAT:      "pat",
	})
	if err != nil {
		t.Fatalf("NewClient performed network I/O or otherwise failed: %v", err)
	}
	if cli == nil {
		t.Fatalf("expected a non-nil client")
	}
}

func TestNewClientDefaults(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com/", PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if cli.cfg.Endpoint != "https://corelink-api.humangr.com" {
		t.Fatalf("expected trailing slash to be trimmed, got %q", cli.cfg.Endpoint)
	}
	if cli.cfg.UserAgent == "" {
		t.Fatalf("expected a default UserAgent to be set")
	}
	if cli.cfg.Digest != DigestBLAKE3 {
		t.Fatalf("expected zero-value Digest to default to DigestBLAKE3, got %v", cli.cfg.Digest)
	}
	if cli.http == nil {
		t.Fatalf("expected a default http.Client to be constructed")
	}
	if cli.http.Timeout != defaultTimeout {
		t.Fatalf("expected default timeout %v, got %v", defaultTimeout, cli.http.Timeout)
	}
}

func TestNewClientRespectsSuppliedHTTPClient(t *testing.T) {
	custom := &http.Client{Timeout: 7 * time.Second}
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat", HTTPClient: custom})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if cli.http != custom {
		t.Fatalf("expected the supplied HTTPClient to be used as-is")
	}
	if cli.http.Timeout != 7*time.Second {
		t.Fatalf("expected the caller's own timeout to be left alone, got %v", cli.http.Timeout)
	}
}

func TestNewClientAppliesOptions(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat"}, WithTenantID("acme-corp"))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if cli.cfg.TenantID != "acme-corp" {
		t.Fatalf("expected WithTenantID option to be applied, got %q", cli.cfg.TenantID)
	}
}

func TestClientCloseIsIdempotent(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if err := cli.Close(); err != nil {
		t.Fatalf("first Close: %v", err)
	}
	if err := cli.Close(); err != nil {
		t.Fatalf("second Close should also succeed: %v", err)
	}
}

func TestSetDigestFunction(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if cli.cfg.Digest != DigestBLAKE3 {
		t.Fatalf("expected default digest to be BLAKE3")
	}
	cli.SetDigestFunction(DigestSHA256)
	if cli.cfg.Digest != DigestSHA256 {
		t.Fatalf("SetDigestFunction did not take effect")
	}
}

func TestIsClientVerifyEnabledIsAlwaysTrue(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if !cli.IsClientVerifyEnabled() {
		t.Fatalf("expected client-side verification to be enabled by default")
	}
}

func TestAccessorsReturnBoundSurfaces(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if cas := cli.CAS(); cas == nil || cas.c != cli {
		t.Fatalf("CAS() should return a non-nil CASClient bound to the client")
	}
	if bs := cli.ByteStream(); bs == nil || bs.c != cli {
		t.Fatalf("ByteStream() should return a non-nil ByteStreamClient bound to the client")
	}
	if ac := cli.ActionCache(); ac == nil || ac.c != cli {
		t.Fatalf("ActionCache() should return a non-nil ActionCacheClient bound to the client")
	}
}

func TestDoSendsAuthAndTenantHeaders(t *testing.T) {
	var gotAuth, gotTenant, gotUA string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		gotTenant = r.Header.Get("X-CoreLink-Tenant")
		gotUA = r.Header.Get("User-Agent")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
	}))
	defer srv.Close()

	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "secret-pat"}, WithTenantID("acme-corp"))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	resp, err := cli.do(context.Background(), http.MethodGet, "/v1/ping", nil)
	if err != nil {
		t.Fatalf("do: %v", err)
	}
	defer resp.Body.Close()

	if gotAuth != "Bearer secret-pat" {
		t.Fatalf("Authorization header = %q, want %q", gotAuth, "Bearer secret-pat")
	}
	if gotTenant != "acme-corp" {
		t.Fatalf("X-CoreLink-Tenant header = %q, want %q", gotTenant, "acme-corp")
	}
	if gotUA == "" {
		t.Fatalf("expected a non-empty User-Agent header")
	}
}

func TestDoRejectsPathWithoutLeadingSlash(t *testing.T) {
	cli, err := NewClient(Config{Endpoint: "https://corelink-api.humangr.com", PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if _, err := cli.do(context.Background(), http.MethodGet, "v1/ping", nil); err == nil {
		t.Fatalf("expected an error for a path without a leading slash")
	}
}

func TestDoConvertsAuthError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`{"message":"bad PAT"}`))
	}))
	defer srv.Close()

	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	_, err = cli.do(context.Background(), http.MethodGet, "/v1/cas/x", nil)
	if err == nil {
		t.Fatalf("expected an error for a 401 response")
	}
	var authErr *AuthError
	if !errors.As(err, &authErr) {
		t.Fatalf("expected *AuthError via errors.As, got %T: %v", err, err)
	}
	if authErr.StatusCode != http.StatusUnauthorized {
		t.Fatalf("AuthError.StatusCode = %d, want %d", authErr.StatusCode, http.StatusUnauthorized)
	}
	if authErr.Message != "bad PAT" {
		t.Fatalf("AuthError.Message = %q, want %q", authErr.Message, "bad PAT")
	}
}

func TestDoConvertsNotFoundError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
	}))
	defer srv.Close()

	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	_, err = cli.do(context.Background(), http.MethodGet, "/v1/cas/missing", nil)
	var notFound *NotFoundError
	if !errors.As(err, &notFound) {
		t.Fatalf("expected *NotFoundError via errors.As, got %T: %v", err, err)
	}
}

func TestDoConvertsRateLimitErrorWithRetryAfter(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Retry-After", "5")
		w.WriteHeader(http.StatusTooManyRequests)
	}))
	defer srv.Close()

	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	_, err = cli.do(context.Background(), http.MethodGet, "/v1/cas/x", nil)
	var rateLimit *RateLimitError
	if !errors.As(err, &rateLimit) {
		t.Fatalf("expected *RateLimitError via errors.As, got %T: %v", err, err)
	}
	if rateLimit.RetryAfter != 5*time.Second {
		t.Fatalf("RateLimitError.RetryAfter = %v, want 5s", rateLimit.RetryAfter)
	}
}

func TestDoSucceedsOn2xx(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("hello"))
	}))
	defer srv.Close()

	cli, err := NewClient(Config{Endpoint: srv.URL, PAT: "pat"})
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	resp, err := cli.do(context.Background(), http.MethodGet, "/v1/cas/x", nil)
	if err != nil {
		t.Fatalf("do: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("StatusCode = %d, want 200", resp.StatusCode)
	}
}

// TestSetDigestFunctionIsRaceSafe fails under -race if the setter and a reader
// touch cfg without the lock. It is here because the first cut of this client
// had no mutex and the setter was a bare write.
func TestSetDigestFunctionIsRaceSafe(t *testing.T) {
	c, err := NewClient(Config{Endpoint: "https://example.invalid", PAT: "pat_x"})
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan struct{})
	go func() {
		for i := 0; i < 500; i++ {
			c.SetDigestFunction(DigestSHA256)
			c.SetDigestFunction(DigestBLAKE3)
		}
		close(done)
	}()
	for i := 0; i < 500; i++ {
		_ = c.digestFunction()
		_ = c.IsClientVerifyEnabled()
	}
	<-done
}

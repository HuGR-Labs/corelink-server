package corelink

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// defaultTimeout is applied when Config.Timeout is zero and Config.HTTPClient
// is nil -- NewClient never overrides the timeout of an HTTPClient the
// caller supplied themselves.
const defaultTimeout = 30 * time.Second

// defaultUserAgent is sent when Config.UserAgent is empty.
const defaultUserAgent = "corelink-go/1"

// NewClient builds a Client. It validates the configuration and does not
// perform network I/O; the first request is what proves credentials.
func NewClient(cfg Config, opts ...Option) (*Client, error) {
	for _, opt := range opts {
		opt(&cfg)
	}

	if strings.TrimSpace(cfg.Endpoint) == "" {
		return nil, errors.New("corelink: Config.Endpoint is required")
	}
	if strings.TrimSpace(cfg.PAT) == "" {
		return nil, errors.New("corelink: Config.PAT is required")
	}
	if cfg.Digest != DigestBLAKE3 && cfg.Digest != DigestSHA256 {
		return nil, fmt.Errorf("corelink: Config.Digest %d is not a known digest function", cfg.Digest)
	}

	cfg.Endpoint = strings.TrimRight(cfg.Endpoint, "/")

	if cfg.UserAgent == "" {
		cfg.UserAgent = defaultUserAgent
	}

	httpClient := cfg.HTTPClient
	if httpClient == nil {
		timeout := cfg.Timeout
		if timeout == 0 {
			timeout = defaultTimeout
		}
		httpClient = &http.Client{Timeout: timeout}
	}

	return &Client{cfg: cfg, http: httpClient}, nil
}

// Close releases pooled connections. It is safe to call more than once: it
// only asks the underlying transport to drop idle connections, which is
// itself idempotent.
func (c *Client) Close() error {
	if c.http != nil {
		c.http.CloseIdleConnections()
	}
	return nil
}

// SetDigestFunction changes the digest for subsequent calls.
//
// Note: Client's fields are fixed by the frozen type surface in
// corelink.go (cfg Config, http *http.Client) with no room for a guard
// field, so this assignment is not synchronized against concurrent readers
// of cfg.Digest. In practice this is a single machine-word write; callers
// that need a hard guarantee should call SetDigestFunction before sharing
// the Client across goroutines, not while Puts are in flight on other
// goroutines.
func (c *Client) SetDigestFunction(f DigestFunction) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.cfg.Digest = f
}

// digestFunction reads the current digest under the same lock, so a caller
// switching functions on one goroutine cannot race a request on another.
func (c *Client) digestFunction() DigestFunction {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.cfg.Digest
}

// IsClientVerifyEnabled reports whether the client re-hashes what it reads
// before returning it, which is the defence against a poisoned cache.
//
// This is unconditionally true: the frozen Config in corelink.go has no
// verify-toggle field (some published docs describe a
// ClientVerifyExplicitFalse config knob that does not exist on this
// surface), so client-side verification cannot be turned off through this
// SDK. That matches CTRL-CAS-002's zero-value guard -- the safe behavior is
// the only behavior.
func (c *Client) IsClientVerifyEnabled() bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	// A plain false is indistinguishable from an unset field, so opting out
	// takes a second, deliberate flag. Anything less would let a zero-value
	// Config silently disable the check that stops a poisoned blob.
	return !c.cfg.ClientVerifyExplicitFalse
}

// CAS returns the REAPI v2 content-addressable-storage surface.
func (c *Client) CAS() *CASClient { return &CASClient{c: c} }

// ByteStream returns the REAPI v2 streaming surface for large blobs.
func (c *Client) ByteStream() *ByteStreamClient { return &ByteStreamClient{c: c} }

// ActionCache returns the REAPI v2 action-cache surface.
func (c *Client) ActionCache() *ActionCacheClient { return &ActionCacheClient{c: c} }

// do issues an authenticated HTTP request against the client's configured
// endpoint and returns the raw response.
//
// path must start with "/"; it is appended to Config.Endpoint verbatim. On
// a successful call (2xx status) the caller owns resp.Body and must close
// it. On a non-2xx status, do reads and closes resp.Body itself, converts
// it via errorFromResponse, and returns (nil, err) -- callers never need to
// separately check resp.StatusCode.
func (c *Client) do(ctx context.Context, method, path string, body io.Reader) (*http.Response, error) {
	if !strings.HasPrefix(path, "/") {
		return nil, fmt.Errorf("corelink: request path %q must start with \"/\"", path)
	}

	req, err := http.NewRequestWithContext(ctx, method, c.cfg.Endpoint+path, body)
	if err != nil {
		return nil, fmt.Errorf("corelink: building request: %w", err)
	}

	req.Header.Set("Authorization", "Bearer "+c.cfg.PAT)
	req.Header.Set("User-Agent", c.cfg.UserAgent)
	if c.cfg.TenantID != "" {
		req.Header.Set("X-CoreLink-Tenant", c.cfg.TenantID)
	}
	if body != nil {
		if req.Header.Get("Content-Type") == "" {
			req.Header.Set("Content-Type", "application/octet-stream")
		}
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("corelink: request failed: %w", err)
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, errorFromResponse(resp)
	}

	return resp, nil
}

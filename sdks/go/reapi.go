package corelink

import (
	"context"
	"io"
)

// Health queries the server's readiness.
func (c *Client) Health(ctx context.Context) (HealthStatus, error) { panic("not implemented") }

// Capabilities queries what the REAPI instance supports.
func (c *Client) Capabilities(ctx context.Context) (ServerCapabilities, error) {
	panic("not implemented")
}

// CASClient is the REAPI v2 batched CAS surface.
type CASClient struct{ c *Client }

// FindMissing reports which of the given digests the server does not hold.
func (b *CASClient) FindMissing(ctx context.Context, ds []Digest) ([]Digest, error) {
	panic("not implemented")
}

// BatchUpdate uploads several small blobs in one request.
func (b *CASClient) BatchUpdate(ctx context.Context, blobs map[Digest][]byte) error {
	panic("not implemented")
}

// BatchRead downloads several small blobs in one request.
func (b *CASClient) BatchRead(ctx context.Context, ds []Digest) (map[Digest][]byte, error) {
	panic("not implemented")
}

// ByteStreamClient streams blobs too large for a batch request.
type ByteStreamClient struct{ c *Client }

// Read streams a blob into w and returns the number of bytes written.
func (b *ByteStreamClient) Read(ctx context.Context, d Digest, w io.Writer) (int64, error) {
	panic("not implemented")
}

// Write streams a blob from r. Size must match the digest.
func (b *ByteStreamClient) Write(ctx context.Context, d Digest, r io.Reader) error {
	panic("not implemented")
}

// ActionCacheClient is the REAPI v2 action cache.
type ActionCacheClient struct{ c *Client }

// Update stores an ActionResult for an action digest.
func (a *ActionCacheClient) Update(ctx context.Context, actionDigest Digest, r ActionResult) error {
	panic("not implemented")
}

package corelink

import (
	"context"
)

// Put uploads a blob to the tenant's CAS and returns its digest.
func (c *Client) Put(ctx context.Context, body []byte) (Digest, error) { panic("not implemented") }

// Get downloads a blob and verifies it against its digest before returning.
func (c *Client) Get(ctx context.Context, d Digest) ([]byte, error) { panic("not implemented") }

// Stat reports blob presence and size without transferring the body.
func (c *Client) Stat(ctx context.Context, d Digest) (Stat, error) { panic("not implemented") }

// Get returns a cached ActionResult, or ErrActionCacheMiss.
func (a *ActionCacheClient) Get(ctx context.Context, actionDigest Digest) (ActionResult, error) {
	panic("not implemented")
}

package corelink

// NewClient builds a Client. It validates the configuration and does not
// perform network I/O; the first request is what proves credentials.
func NewClient(cfg Config, opts ...Option) (*Client, error) { panic("not implemented") }

// Close releases pooled connections. It is safe to call more than once.
func (c *Client) Close() error { panic("not implemented") }

// SetDigestFunction changes the digest for subsequent calls.
func (c *Client) SetDigestFunction(f DigestFunction) { panic("not implemented") }

// IsClientVerifyEnabled reports whether the client re-hashes what it reads
// before returning it, which is the defence against a poisoned cache.
func (c *Client) IsClientVerifyEnabled() bool { panic("not implemented") }

// CAS returns the REAPI v2 content-addressable-storage surface.
func (c *Client) CAS() *CASClient { panic("not implemented") }

// ByteStream returns the REAPI v2 streaming surface for large blobs.
func (c *Client) ByteStream() *ByteStreamClient { panic("not implemented") }

// ActionCache returns the REAPI v2 action-cache surface.
func (c *Client) ActionCache() *ActionCacheClient { panic("not implemented") }

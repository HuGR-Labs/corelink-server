package corelink

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
)

// casTenant returns the tenant used to key the native CAS path,
// /v1/cas/{tenant}/{digest}. Config.TenantID is optional for NewClient in
// general (a PAT carries its own tenant), but the native CAS routes are
// path-scoped by tenant, so a Put/Get/Stat call needs one -- mirrors
// corelink-py's CoreLinkClient._cas_url, which raises the same way when
// tenant_id is unset.
func (c *Client) casTenant() (string, error) {
	if c.cfg.TenantID == "" {
		return "", errors.New("corelink: CAS operations require a tenant; construct the client with WithTenantID or Config.TenantID")
	}
	return c.cfg.TenantID, nil
}

// casPath builds the tenant-scoped CAS object path for a digest hex.
func casPath(tenant, hex string) string {
	return fmt.Sprintf("/v1/cas/%s/%s", tenant, hex)
}

// Put uploads a blob to the tenant's CAS and returns its digest.
//
// PUT /v1/cas/{tenant}/{digest} -- corelink-py's CoreLinkClient.put
// (sdks/python/corelink/client.py:158-181) is the reference: the digest is
// computed client-side with the client's current digest function and sent
// as the URL path, so the server can verify it and reply 422 if it
// disagrees. That 422 is translated here into a *DigestMismatchError
// (errors.go's errorFromResponse cannot populate Expected/Actual for a 422
// itself -- see its comment -- so the caller that already knows the
// expected digest builds the typed error).
func (c *Client) Put(ctx context.Context, body []byte) (Digest, error) {
	tenant, err := c.casTenant()
	if err != nil {
		return Digest{}, err
	}

	digest := Compute(body, c.digestFunction())

	resp, err := c.do(ctx, http.MethodPut, casPath(tenant, digest.Hash), bytes.NewReader(body))
	if err != nil {
		var sme *statusMessageError
		if errors.As(err, &sme) && sme.statusCode == http.StatusUnprocessableEntity {
			return Digest{}, &DigestMismatchError{Expected: digest}
		}
		return Digest{}, err
	}
	defer resp.Body.Close()
	// Drain and discard: a successful Put's response body carries nothing
	// this method returns, but the connection should be reusable.
	_, _ = io.Copy(io.Discard, resp.Body)

	return digest, nil
}

// Get downloads a blob and verifies it against its digest before returning.
//
// GET /v1/cas/{tenant}/{digest} -- corelink-py's CoreLinkClient.get
// (sdks/python/corelink/client.py:217-235). When IsClientVerifyEnabled is
// true (the default, and on this SDK the only behavior -- see
// Client.IsClientVerifyEnabled), the bytes read are re-hashed with the
// client's current digest function and compared against d before they are
// handed back to the caller; a mismatch returns *DigestMismatchError and no
// bytes, exactly as corelink-py's verify_bytes does for CTRL-CAS-002.
func (c *Client) Get(ctx context.Context, d Digest) ([]byte, error) {
	tenant, err := c.casTenant()
	if err != nil {
		return nil, err
	}
	if verr := d.Validate(); verr != nil {
		return nil, verr
	}

	resp, err := c.do(ctx, http.MethodGet, casPath(tenant, d.Hash), nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("corelink: reading CAS response body: %w", err)
	}

	if c.IsClientVerifyEnabled() {
		if verr := VerifyBytes(data, d, c.digestFunction()); verr != nil {
			return nil, verr
		}
	}

	return data, nil
}

// Stat reports blob presence and size without transferring the body.
//
// HEAD /v1/cas/{tenant}/{digest} -- corelink-py's CoreLinkClient.stat
// (sdks/python/corelink/client.py:238-249): axum serves HEAD on the GET
// route. A 404 means the blob is absent and is reported as Stat{Exists:
// false}, not an error -- matching corelink-py, which treats 404/410 as a
// negative result rather than raising. errors.go only gives this SDK a
// typed error for 404 (there is no typed 410 case), so that is the one
// absence status this method special-cases; see PYTHON SDK DIVERGENCES.
func (c *Client) Stat(ctx context.Context, d Digest) (Stat, error) {
	tenant, err := c.casTenant()
	if err != nil {
		return Stat{}, err
	}
	if verr := d.Validate(); verr != nil {
		return Stat{}, verr
	}

	resp, err := c.do(ctx, http.MethodHead, casPath(tenant, d.Hash), nil)
	if err != nil {
		var notFound *NotFoundError
		if errors.As(err, &notFound) {
			return Stat{Exists: false}, nil
		}
		return Stat{}, err
	}
	defer resp.Body.Close()

	size := resp.ContentLength
	if size < 0 {
		size = 0
	}

	return Stat{Exists: true, Digest: d.Hash, SizeBytes: size}, nil
}

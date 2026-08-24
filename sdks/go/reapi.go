package corelink

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"hash"
	"io"
	"net/http"
	"net/url"

	"lukechampine.com/blake3"
)

// ─── Health / Capabilities ─────────────────────────────────────────────────

// healthResponse is the body of GET /_health, as produced by
// crates/corelink-container/src/main.rs's health_handler:
//
//	{"status":"ok","storage":"r2"|"inmemory"}
//
// It carries no region or version field, so HealthStatus.Region and
// HealthStatus.Version always come back empty from this call.
type healthResponse struct {
	Status  string `json:"status"`
	Storage string `json:"storage"`
}

// Health queries the server's readiness via GET /_health -- the DO
// container-readiness probe mounted in main.rs (search "/_health" there).
// It is not a REAPI-specific endpoint, but it is the only readiness surface
// the server exposes, and Health's own doc comment on the frozen type
// promises "the server's own view of its readiness".
//
// The response body carries no region or version, so HealthStatus.Region
// and HealthStatus.Version are always "" on success.
func (c *Client) Health(ctx context.Context) (HealthStatus, error) {
	resp, err := c.do(ctx, http.MethodGet, "/_health", nil)
	if err != nil {
		return HealthStatus{}, err
	}
	defer resp.Body.Close()

	var hr healthResponse
	if err := json.NewDecoder(resp.Body).Decode(&hr); err != nil {
		return HealthStatus{}, fmt.Errorf("corelink: decoding /_health response: %w", err)
	}

	return HealthStatus{OK: hr.Status == "ok"}, nil
}

// Capabilities queries what the REAPI instance supports.
//
// There is no such endpoint. crates/corelink-container/src/routes/bazel_v2.rs
// mounts exactly five REAPI v2 routes -- CAS read, CAS write, AC read, AC
// write, and findMissingBlobs -- and no other route file in
// crates/corelink-container/src/routes defines a capabilities surface
// either (grepped for "capabilities" case-insensitively across the whole
// routes/ tree: no hits besides doc comments). Rather than invent a path
// that would silently 404 (or worse, get "fixed" later by someone routing
// it to something unrelated), this returns a clear, typed error and the
// zero-value ServerCapabilities.
func (c *Client) Capabilities(ctx context.Context) (ServerCapabilities, error) {
	return ServerCapabilities{}, fmt.Errorf(
		"corelink: no REAPI Capabilities endpoint exists on this server " +
			"(bazel_v2.rs mounts only CAS read/write, AC read/write, and " +
			"findMissingBlobs); use Client.Health for a readiness check instead",
	)
}

// ─── path helpers ───────────────────────────────────────────────────────────
//
// Every path below follows the wire scheme documented at the top of
// crates/corelink-container/src/routes/bazel_v2.rs:
//
//	GET  /bazel/v2/{instance}/blobs/{hash}/{size}                   CAS read
//	PUT  /bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}     CAS write
//	GET  /bazel/v2/{instance}/blobs/ac/{hash}/{size}                 AC read
//	PUT  /bazel/v2/{instance}/blobs/ac/{hash}/{size}                 AC write
//	POST /bazel/v2/{instance}/findMissingBlobs                       batch find-missing

func casBlobPath(instance, hash string, size int64) string {
	return fmt.Sprintf("/bazel/v2/%s/blobs/%s/%d", url.PathEscape(instance), url.PathEscape(hash), size)
}

func casUploadPath(instance, uploadID, hash string, size int64) string {
	return fmt.Sprintf("/bazel/v2/%s/uploads/%s/blobs/%s/%d",
		url.PathEscape(instance), url.PathEscape(uploadID), url.PathEscape(hash), size)
}

func acBlobPath(instance, hash string, size int64) string {
	return fmt.Sprintf("/bazel/v2/%s/blobs/ac/%s/%d", url.PathEscape(instance), url.PathEscape(hash), size)
}

func findMissingPath(instance string) string {
	return fmt.Sprintf("/bazel/v2/%s/findMissingBlobs", url.PathEscape(instance))
}

// newUploadID mints a UUIDv4 for the CAS-write upload-correlation segment.
// The server (bazel_v2.rs's handle_cas_write) only logs this value for
// tracing and performs no resumable-upload protocol with it, so any unique
// string would do; a real UUIDv4 is what a REAPI client is expected to
// send. There is no uuid package in go.mod (and this task may not run `go
// get`), so it's generated directly from crypto/rand per RFC 4122 §4.4.
func newUploadID() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", fmt.Errorf("corelink: generating upload id: %w", err)
	}
	b[6] = (b[6] & 0x0f) | 0x40 // version 4
	b[8] = (b[8] & 0x3f) | 0x80 // variant 10
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16]), nil
}

// newDigestHasher returns a streaming hash.Hash matching fn, so a large
// blob can be verified while it streams instead of being buffered whole.
func newDigestHasher(fn DigestFunction) hash.Hash {
	if fn == DigestSHA256 {
		return sha256.New()
	}
	return blake3.New(32, nil)
}

// ─── CASClient ───────────────────────────────────────────────────────────────

// CASClient is the REAPI v2 batched CAS surface.
type CASClient struct{ c *Client }

// digestJSON is the wire shape findMissingBlobs uses for a digest, matching
// crates/corelink-bazel-bridge/src/digest.rs's DigestJson: {"hash":...,
// "sizeBytes":...}.
type digestJSON struct {
	Hash      string `json:"hash"`
	SizeBytes int64  `json:"sizeBytes"`
}

type findMissingRequestBody struct {
	BlobDigests []digestJSON `json:"blobDigests"`
}

type findMissingResponseBody struct {
	MissingBlobDigests []digestJSON `json:"missingBlobDigests"`
}

// FindMissing reports which of the given digests the server does not hold.
//
// POST /bazel/v2/{instance}/findMissingBlobs (bazel_v2.rs:325,
// handle_find_missing at bazel_v2.rs:811..878; request/response shape
// documented and defined in
// crates/corelink-bazel-bridge/src/find_missing.rs).
func (b *CASClient) FindMissing(ctx context.Context, ds []Digest) ([]Digest, error) {
	reqBody := findMissingRequestBody{BlobDigests: make([]digestJSON, len(ds))}
	for i, d := range ds {
		reqBody.BlobDigests[i] = digestJSON{Hash: d.Hash, SizeBytes: d.SizeBytes}
	}

	payload, err := json.Marshal(reqBody)
	if err != nil {
		return nil, fmt.Errorf("corelink: encoding findMissingBlobs request: %w", err)
	}

	resp, err := b.c.do(ctx, http.MethodPost, findMissingPath(b.c.cfg.Instance), bytes.NewReader(payload))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var out findMissingResponseBody
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, fmt.Errorf("corelink: decoding findMissingBlobs response: %w", err)
	}

	missing := make([]Digest, len(out.MissingBlobDigests))
	for i, d := range out.MissingBlobDigests {
		missing[i] = Digest{Hash: d.Hash, SizeBytes: d.SizeBytes}
	}
	return missing, nil
}

// reapiCASWrite performs one CAS-write PUT against the uploads path,
// streaming body without buffering it into memory first. Shared by
// CASClient.BatchUpdate and ByteStreamClient.Write, since bazel_v2.rs has
// exactly one CAS-write route regardless of blob size.
func (c *Client) reapiCASWrite(ctx context.Context, d Digest, body io.Reader) error {
	uploadID, err := newUploadID()
	if err != nil {
		return err
	}
	resp, err := c.do(ctx, http.MethodPut, casUploadPath(c.cfg.Instance, uploadID, d.Hash, d.SizeBytes), body)
	if err != nil {
		return err
	}
	return resp.Body.Close()
}

// reapiCASRead performs one CAS-read GET, buffering the full blob -- for
// BatchRead's "small blobs" contract. ByteStreamClient.Read has its own
// streaming path for blobs too large to buffer.
func (c *Client) reapiCASRead(ctx context.Context, d Digest) ([]byte, error) {
	resp, err := c.do(ctx, http.MethodGet, casBlobPath(c.cfg.Instance, d.Hash, d.SizeBytes), nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("corelink: reading blob body: %w", err)
	}
	if err := VerifyBytes(data, d, c.digestFunction()); err != nil {
		return nil, err
	}
	return data, nil
}

// BatchUpdate uploads several small blobs in one request.
//
// There is no batch-write REAPI route on this server -- bazel_v2.rs mounts
// only the single-blob CAS write (PUT
// /bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}, bazel_v2.rs:318).
// This issues one such PUT per blob. Returns the first error encountered,
// wrapped with which digest failed; already-uploaded blobs in the same call
// are not rolled back (CAS writes are idempotent by content address, so a
// retried BatchUpdate is safe).
func (b *CASClient) BatchUpdate(ctx context.Context, blobs map[Digest][]byte) error {
	for d, data := range blobs {
		if err := b.c.reapiCASWrite(ctx, d, bytes.NewReader(data)); err != nil {
			return fmt.Errorf("corelink: BatchUpdate: uploading %s: %w", d, err)
		}
	}
	return nil
}

// BatchRead downloads several small blobs in one request.
//
// There is no batch-read REAPI route on this server -- bazel_v2.rs mounts
// only the single-blob CAS read (GET
// /bazel/v2/{instance}/blobs/{hash}/{size}, bazel_v2.rs:304). This issues
// one such GET per digest and verifies each blob against its digest before
// returning it (see Config.ClientVerify's contract on corelink.go).
func (b *CASClient) BatchRead(ctx context.Context, ds []Digest) (map[Digest][]byte, error) {
	out := make(map[Digest][]byte, len(ds))
	for _, d := range ds {
		data, err := b.c.reapiCASRead(ctx, d)
		if err != nil {
			return nil, fmt.Errorf("corelink: BatchRead: reading %s: %w", d, err)
		}
		out[d] = data
	}
	return out, nil
}

// ─── ByteStreamClient ────────────────────────────────────────────────────────

// ByteStreamClient streams blobs too large for a batch request.
type ByteStreamClient struct{ c *Client }

// Read streams a blob into w and returns the number of bytes written.
//
// GET /bazel/v2/{instance}/blobs/{hash}/{size} (bazel_v2.rs:304-308,
// handle_cas_read at bazel_v2.rs:534..595). The response body is copied
// straight from the HTTP response into w via io.Copy, which moves data in
// fixed-size (32 KiB) chunks rather than buffering the whole blob -- see
// the STREAMING note in the accompanying report. A digest hasher is fed
// the same bytes through io.MultiWriter so the transfer is verified
// (client-side, per Config.ClientVerify's contract) without a second pass
// over the data. ctx cancellation mid-transfer surfaces because resp.Body's
// reads are tied to the request's context; io.Copy returns promptly with
// ctx.Err() once the underlying connection is torn down.
func (b *ByteStreamClient) Read(ctx context.Context, d Digest, w io.Writer) (int64, error) {
	resp, err := b.c.do(ctx, http.MethodGet, casBlobPath(b.c.cfg.Instance, d.Hash, d.SizeBytes), nil)
	if err != nil {
		return 0, err
	}
	defer resp.Body.Close()

	h := newDigestHasher(b.c.digestFunction())
	n, err := io.Copy(io.MultiWriter(w, h), resp.Body)
	if err != nil {
		return n, fmt.Errorf("corelink: streaming blob: %w", err)
	}

	got := Digest{Hash: hex.EncodeToString(h.Sum(nil)), SizeBytes: n}
	if !got.Equal(d) {
		return n, &DigestMismatchError{Expected: d, Actual: got}
	}
	return n, nil
}

// Write streams a blob from r. Size must match the digest.
//
// PUT /bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}
// (bazel_v2.rs:319-322, handle_cas_write at bazel_v2.rs:602..675). r is
// passed straight through to http.NewRequestWithContext as the request
// body (via Client.do), so the Go HTTP client streams it in chunks rather
// than this SDK reading r into memory first -- see the STREAMING note in
// the accompanying report. Size verification is the server's job: it
// hashes the received bytes and returns 422 (surfaced here as a typed
// error via errorFromResponse) on a mismatch.
func (b *ByteStreamClient) Write(ctx context.Context, d Digest, r io.Reader) error {
	return b.c.reapiCASWrite(ctx, d, r)
}

// ─── ActionCacheClient ───────────────────────────────────────────────────────

// ActionCacheClient is the REAPI v2 action cache.
type ActionCacheClient struct{ c *Client }

// actionResultWire is this SDK's own JSON encoding of ActionResult. The
// server stores the AC payload as an opaque byte blob (see
// crates/corelink-bazel-bridge/src/adapter.rs's ac_get/ac_put: "the raw
// action result payload" / "the payload bytes are stored ... under the
// given action digest" -- it never parses or interprets it), so there is
// no wire-format spec to match; this is simply a format Update and Get
// must agree on to round-trip an ActionResult through that opaque store.
type actionResultWire struct {
	ExitCode    int32            `json:"exit_code"`
	StdoutRaw   []byte           `json:"stdout_raw,omitempty"`
	OutputFiles []outputFileWire `json:"output_files,omitempty"`
}

type outputFileWire struct {
	Path         string `json:"path"`
	Hash         string `json:"hash"`
	SizeBytes    int64  `json:"size_bytes"`
	IsExecutable bool   `json:"is_executable,omitempty"`
}

func encodeActionResult(r ActionResult) ([]byte, error) {
	w := actionResultWire{
		ExitCode:  r.ExitCode,
		StdoutRaw: r.StdoutRaw,
	}
	for _, of := range r.OutputFiles {
		w.OutputFiles = append(w.OutputFiles, outputFileWire{
			Path:         of.Path,
			Hash:         of.Digest.Hash,
			SizeBytes:    of.Digest.SizeBytes,
			IsExecutable: of.IsExecutable,
		})
	}
	return json.Marshal(w)
}

// Update stores an ActionResult for an action digest.
//
// PUT /bazel/v2/{instance}/blobs/ac/{hash}/{size} (bazel_v2.rs:314-317,
// handle_ac_write at bazel_v2.rs:737..805). r is encoded with
// encodeActionResult before it is sent -- see actionResultWire's doc
// comment for why this SDK owns that encoding.
func (a *ActionCacheClient) Update(ctx context.Context, actionDigest Digest, r ActionResult) error {
	payload, err := encodeActionResult(r)
	if err != nil {
		return fmt.Errorf("corelink: encoding ActionResult: %w", err)
	}
	resp, err := a.c.do(ctx, http.MethodPut, acBlobPath(a.c.cfg.Instance, actionDigest.Hash, actionDigest.SizeBytes), bytes.NewReader(payload))
	if err != nil {
		return err
	}
	return resp.Body.Close()
}

// Get returns a cached ActionResult, or ErrActionCacheMiss.
//
// GET /bazel/v2/{instance}/blobs/ac/{hash}/{size}, the read side of the route
// Update writes to. The server stores the payload opaquely, so this decodes
// the same actionResultWire encoding Update produced; a payload written by
// anything else is not expected to parse.
//
// A miss is a 404, which errorFromResponse turns into *NotFoundError. That is
// translated here into ErrActionCacheMiss, because a miss is the ordinary case
// for a cache and the published examples test it with errors.Is.
func (a *ActionCacheClient) Get(ctx context.Context, actionDigest Digest) (ActionResult, error) {
	resp, err := a.c.do(ctx, http.MethodGet, acBlobPath(a.c.cfg.Instance, actionDigest.Hash, actionDigest.SizeBytes), nil)
	if err != nil {
		var nf *NotFoundError
		if errors.As(err, &nf) {
			return ActionResult{}, ErrActionCacheMiss
		}
		return ActionResult{}, err
	}
	defer resp.Body.Close()

	payload, err := io.ReadAll(resp.Body)
	if err != nil {
		return ActionResult{}, fmt.Errorf("corelink: reading ActionResult: %w", err)
	}
	return decodeActionResult(payload)
}

// decodeActionResult is the inverse of encodeActionResult.
func decodeActionResult(payload []byte) (ActionResult, error) {
	var w actionResultWire
	if err := json.Unmarshal(payload, &w); err != nil {
		return ActionResult{}, fmt.Errorf("corelink: decoding ActionResult: %w", err)
	}
	out := ActionResult{ExitCode: w.ExitCode, StdoutRaw: w.StdoutRaw}
	for _, f := range w.OutputFiles {
		out.OutputFiles = append(out.OutputFiles, OutputFile{
			Path:         f.Path,
			Digest:       Digest{Hash: f.Hash, SizeBytes: f.SizeBytes},
			IsExecutable: f.IsExecutable,
		})
	}
	return out, nil
}

package corelink

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"

	"lukechampine.com/blake3"
)

// ComputeBLAKE3 hashes data with BLAKE3, the digest function CoreLink's
// native CAS surface requires. Sending a digest computed any other way to
// /v1/cas is rejected with 422 HashMismatch.
func ComputeBLAKE3(data []byte) Digest {
	sum := blake3.Sum256(data)
	return Digest{Hex: hex.EncodeToString(sum[:]), Size: int64(len(data))}
}

// ComputeSHA256 hashes data with SHA-256, for REAPI instances that negotiate
// it via Capabilities. It is never accepted by the native /v1/cas surface.
func ComputeSHA256(data []byte) Digest {
	sum := sha256.Sum256(data)
	return Digest{Hex: hex.EncodeToString(sum[:]), Size: int64(len(data))}
}

// Compute hashes data with the given digest function.
func Compute(data []byte, fn DigestFunction) Digest {
	switch fn {
	case DigestSHA256:
		return ComputeSHA256(data)
	default:
		return ComputeBLAKE3(data)
	}
}

// String renders the digest as "<hex>/<size>", the conventional REAPI
// digest string form. It never panics on a zero-value Digest.
func (d Digest) String() string {
	return fmt.Sprintf("%s/%d", d.Hex, d.Size)
}

// IsZero reports whether d is the zero-value Digest.
func (d Digest) IsZero() bool {
	return d.Hex == "" && d.Size == 0
}

// Equal reports whether d and other identify the same content: same hex
// digest and same size. A mismatched size with a matching hex (or vice
// versa) is never equal -- that combination indicates corruption, not a
// benign difference.
func (d Digest) Equal(other Digest) bool {
	return d.Hex == other.Hex && d.Size == other.Size
}

// Validate reports whether d looks like a well-formed digest: 64 lower-case
// hex characters and a non-negative size. It does not verify the digest
// against any content.
func (d Digest) Validate() error {
	if len(d.Hex) != 64 {
		return fmt.Errorf("corelink: digest hex must be 64 characters, got %d", len(d.Hex))
	}
	for _, r := range d.Hex {
		isLowerHex := (r >= '0' && r <= '9') || (r >= 'a' && r <= 'f')
		if !isLowerHex {
			return fmt.Errorf("corelink: digest hex must be lower-case hex, got %q", d.Hex)
		}
	}
	if d.Size < 0 {
		return fmt.Errorf("corelink: digest size must be non-negative, got %d", d.Size)
	}
	return nil
}

// VerifyBytes reports whether data actually hashes to d under fn. Get uses
// this for client-side verification: it re-hashes what it reads before
// returning it, so a poisoned or corrupted cache entry is caught locally
// rather than trusted.
func VerifyBytes(data []byte, d Digest, fn DigestFunction) error {
	got := Compute(data, fn)
	if !got.Equal(d) {
		return &DigestMismatchError{Expected: d, Actual: got}
	}
	return nil
}

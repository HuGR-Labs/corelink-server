package corelink

import (
	"errors"
	"testing"
)

// TestComputeBLAKE3Vector pins the BLAKE3 digest of the literal bytes
// "corelink". Verified by hand against the Python SDK's blake3 and the Rust
// server's blake3::hash(body) (crates/corelink-hash/src/digest.rs:31) -- if
// this test breaks, the bug is in this code, not in the vector.
func TestComputeBLAKE3Vector(t *testing.T) {
	const want = "21d7b8c6ecec7d61df09e9be6c873658fca6ace2f986a69f7febe372863c2217"
	got := ComputeBLAKE3([]byte("corelink"))
	if got.Hash != want {
		t.Fatalf("ComputeBLAKE3(%q).Hash = %s, want %s", "corelink", got.Hash, want)
	}
	if got.SizeBytes != 8 {
		t.Fatalf("ComputeBLAKE3(%q).SizeBytes = %d, want 8", "corelink", got.SizeBytes)
	}
}

func TestComputeSHA256EmptyInput(t *testing.T) {
	const want = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
	got := ComputeSHA256([]byte{})
	if got.Hash != want {
		t.Fatalf("ComputeSHA256(empty).Hash = %s, want %s", got.Hash, want)
	}
	if got.SizeBytes != 0 {
		t.Fatalf("ComputeSHA256(empty).SizeBytes = %d, want 0", got.SizeBytes)
	}
}

func TestComputeDispatchesByFunction(t *testing.T) {
	data := []byte("corelink")
	blake3 := Compute(data, DigestBLAKE3)
	sha256 := Compute(data, DigestSHA256)
	if blake3.Hash == sha256.Hash {
		t.Fatalf("BLAKE3 and SHA-256 digests of the same bytes must differ, got the same hex")
	}
	if blake3.Hash != ComputeBLAKE3(data).Hash {
		t.Fatalf("Compute(data, DigestBLAKE3) did not match ComputeBLAKE3(data)")
	}
	if sha256.Hash != ComputeSHA256(data).Hash {
		t.Fatalf("Compute(data, DigestSHA256) did not match ComputeSHA256(data)")
	}
}

func TestDigestEqual(t *testing.T) {
	a := Digest{Hash: "aa", SizeBytes: 2}
	b := Digest{Hash: "aa", SizeBytes: 2}
	c := Digest{Hash: "aa", SizeBytes: 3}
	if !a.Equal(b) {
		t.Fatalf("expected equal digests to be Equal")
	}
	if a.Equal(c) {
		t.Fatalf("expected digests with mismatched size to not be Equal")
	}
}

func TestDigestValidate(t *testing.T) {
	valid := ComputeBLAKE3([]byte("corelink"))
	if err := valid.Validate(); err != nil {
		t.Fatalf("Validate() on a real digest returned error: %v", err)
	}

	tooShort := Digest{Hash: "abc", SizeBytes: 1}
	if err := tooShort.Validate(); err == nil {
		t.Fatalf("Validate() on a short hex should return an error")
	}

	upperCase := Digest{Hash: "AB" + valid.Hash[2:], SizeBytes: valid.SizeBytes}
	if err := upperCase.Validate(); err == nil {
		t.Fatalf("Validate() should reject upper-case hex")
	}

	negativeSize := Digest{Hash: valid.Hash, SizeBytes: -1}
	if err := negativeSize.Validate(); err == nil {
		t.Fatalf("Validate() should reject a negative size")
	}
}

func TestDigestString(t *testing.T) {
	d := Digest{Hash: "deadbeef", SizeBytes: 4}
	if got, want := d.String(), "deadbeef/4"; got != want {
		t.Fatalf("String() = %q, want %q", got, want)
	}
}

func TestDigestIsZero(t *testing.T) {
	var z Digest
	if !z.IsZero() {
		t.Fatalf("zero-value Digest should report IsZero() == true")
	}
	if ComputeBLAKE3([]byte("x")).IsZero() {
		t.Fatalf("a real digest should report IsZero() == false")
	}
}

func TestVerifyBytesMatch(t *testing.T) {
	data := []byte("corelink")
	d := ComputeBLAKE3(data)
	if err := VerifyBytes(data, d, DigestBLAKE3); err != nil {
		t.Fatalf("VerifyBytes on matching data/digest returned error: %v", err)
	}
}

func TestVerifyBytesMismatch(t *testing.T) {
	data := []byte("corelink")
	wrong := Digest{Hash: ComputeBLAKE3([]byte("not-corelink")).Hash, SizeBytes: int64(len(data))}

	err := VerifyBytes(data, wrong, DigestBLAKE3)
	if err == nil {
		t.Fatalf("VerifyBytes on mismatched data/digest should return an error")
	}

	var mismatch *DigestMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("VerifyBytes error should be a *DigestMismatchError via errors.As, got %T", err)
	}
	if !mismatch.Expected.Equal(wrong) {
		t.Fatalf("DigestMismatchError.Expected = %v, want %v", mismatch.Expected, wrong)
	}
	if !mismatch.Actual.Equal(ComputeBLAKE3(data)) {
		t.Fatalf("DigestMismatchError.Actual = %v, want the real digest of data", mismatch.Actual)
	}
}

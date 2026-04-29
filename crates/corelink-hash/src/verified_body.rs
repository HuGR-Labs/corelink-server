//! `VerifiedBody` envelope — body + digest pair that **must** have been
//! verified at construction time.

use bytes::Bytes;

use crate::digest::Digest;
use crate::error::HashMismatch;

/// A body whose BLAKE3 digest has been computed and verified to match the
/// claimed digest. The only way to produce a `VerifiedBody` is
/// [`VerifiedBody::new`], which performs a constant-time verification.
///
/// Storage adapters (R2, KV, D1) accept a `&VerifiedBody` rather than a raw
/// body, so the type system enforces the invariant `INV-CAS-INTEGRITY` /
/// CTRL-CAS-001: it is impossible to write a body whose digest has not been
/// verified. Forgetting the check is a compile error, not a code-review
/// oversight.
#[derive(Clone)]
pub struct VerifiedBody {
    body: Bytes,
    digest: Digest,
}

impl VerifiedBody {
    /// Compute the BLAKE3 digest of `body` and verify it matches `claimed`
    /// in constant time.
    ///
    /// On success, returns a `VerifiedBody` carrying both the body (zero-copy
    /// `Bytes`) and the now-verified digest. On mismatch, returns
    /// [`HashMismatch`] without revealing how many bytes of `claimed` were
    /// correct (the verification is constant-time and the error type carries
    /// no payload).
    pub fn new(body: Bytes, claimed: Digest) -> Result<Self, HashMismatch> {
        let computed = Digest::compute(&body);
        if !computed.verify_constant_time(&claimed) {
            return Err(HashMismatch);
        }
        Ok(Self {
            body,
            digest: claimed,
        })
    }

    /// Borrow the verified body bytes.
    #[must_use]
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    /// Borrow the (now-verified) digest.
    #[must_use]
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// Consume the envelope, returning the body + digest pair.
    #[must_use]
    pub fn into_parts(self) -> (Bytes, Digest) {
        (self.body, self.digest)
    }
}

impl core::fmt::Debug for VerifiedBody {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Body bytes are application data and may contain PII; avoid
        // accidental dumps. Surface only the digest + length.
        f.debug_struct("VerifiedBody")
            .field("digest", &self.digest)
            .field("len", &self.body.len())
            .finish()
    }
}

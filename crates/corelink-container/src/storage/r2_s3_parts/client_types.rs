/// The body-crypto plan for a CAS/AC object under a resolved BYOK config
/// (surface-neutral — shared by [`R2CasHandler`] and [`R2AcHandler`]). The
/// physical R2 key digest is resolved alongside it (see [`ByokResolved`]).
enum ByokBodyPlan {
    /// Store/serve `req.bytes` unchanged (non-BYOK / `_public` / inactive).
    Plaintext,
    /// Mode A — convergent: body keyed by the Tcs-derived DEK (dedup-preserving).
    Convergent { tcs: Tcs, ctx: CryptoContext },
    /// Mode B — random: body keyed by a random per-blob DEK persisted in
    /// `byok_envelope` (no dedup). The crypto material is held by the handler's
    /// [`ModeBEncryptor`]; only the (real-digest) context travels in the plan.
    Random { ctx: CryptoContext },
}

/// The full BYOK resolution for a CAS op: the **physical** R2 key digest (the
/// §4-hardened HMAC for an active tenant, audit H-4; the raw digest otherwise)
/// plus the body crypto [`ByokBodyPlan`].
struct ByokResolved {
    physical_digest: String,
    plan: ByokBodyPlan,
}

/// How many CAS existence probes (`HeadObject`) [`R2CasHandler::exists_batch`]
/// keeps in flight at once.
///
/// Deliberately small. The container runs on a Cloudflare Containers `basic`
/// instance — **0.25 vCPU** — so the ceiling has to stay well under what would
/// make TLS/HTTP bookkeeping for the in-flight requests contend for that
/// quarter-core; the probes themselves are pure I/O wait (~60 ms per R2 HEAD
/// measured from IAD), so a small window already recovers nearly all of the
/// serial loss. R2 request rate limits are also shared across tenants on the
/// account, so one tenant's 4096-digest `findMissingBlobs` must not be able to
/// open a wide burst against the bucket every other tenant reads through.
///
/// 16 turns the 4096-digest worst case from 4096 serial HEADs into 256 waves
/// (~15 s of HEAD time instead of ~246 s) and a typical 100-digest Bazel call
/// into 7 waves (~0.42 s instead of ~6 s) — the linear term is broken without
/// betting the shared bucket budget or the quarter-core on a large number.
/// Raise it only against a measurement, never on intuition.
const MAX_CONCURRENT_EXISTS_PROBES: usize = 16;

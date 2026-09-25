# Issue #2047 Buck2 warm-cache admission contract

**BLOCKED — static contract only.** The credentialless source checker may prove
that the admission controls below remain present. It does not prove an
authenticated request, a public gRPC endpoint, a CoreLink cache write or read,
a warm-cache hit ratio, a benchmark measurement, or any provider/deployment
state.

The current public Worker rejects `application/grpc` before routing under
#2176's transport-denial contract. That denial remains the admission state
until #2176 supplies its protected deployed transport receipt and #2183 merges
the complete authenticated cache-only service composition. A root-authorized,
manual #2047 runtime dispatch is permitted only after those prerequisites.

## Frozen admission axioms

| Axiom | Static falsifier | Runtime evidence still required |
| --- | --- | --- |
| Auth | Remove the pre-Buck2 PAT refusal or change the bearer header source. | A valid PAT is accepted by CoreLink and an invalid PAT is rejected without token disclosure. |
| Cold/warm | Remove `buck2 clean`, the current warm report parser, or the `>=80%` gate. | A cold build writes and a clean warm build reads from CoreLink with nonzero remote operations and at least 80% hits. |
| Isolation | Broaden the repository/SHA key or add a restore-key fallback. | The receipt records the exact repository/SHA namespace and tenant-safe provenance. |
| Negative controls | Remove a credentialless mutation or allow the benchmark after a failed warm admission. | Missing PAT fails before Buck2 and invalid PAT receives a protocol-correct CoreLink rejection. |
| Exact head | Check out a merge ref or omit the expected-head equality assertion. | The protected merged-main dispatch receipt identifies the deployed commit and endpoint. |

No runner-local `actions/cache` outcome can satisfy a CoreLink remote-cache
claim. No REST, gRPC-Web, endpoint substitution, local fallback, execution
capability, or benchmark receipt is admitted by the static contract.

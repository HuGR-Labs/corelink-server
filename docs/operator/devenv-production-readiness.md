# DevEnv production readiness contract

**Status: BLOCKED — not deployable.** The Server Worker intentionally has no
live `RUNNER_DEVENV_DO` binding. Its typed destination is recorded in
`tests/fixtures/deployment/devenv-cross-worker-binding.toml`; that fixture is a
release input, not a fragment to copy into `wrangler.toml` today.

## Evidence and release boundary

`RunnerDevEnvDO` belongs to the Runners Worker, whose script name is
`corelink-spawn-worker`. Its configuration declares the class and migration tag
`v6`. The Server owns no class of that name; it owns only a typed optional
`DurableObjectNamespace<RunnerDevEnvRpc>`.

The B2 Runners source target is now sufficient as a *release candidate*, not as
live evidence: branch head `b281859` includes the authorized RPC surface, and
consumer commit `b26d785` supplies `prepareAuthorizedCompute`,
`abandonAuthorizedCompute`, and `startAuthorizedDevenv` plus grant verification.
Its desired outer `RunnerDevEnvDO` Container image is pinned to
`corelink-runner-devenv@sha256:0145e461fc1d35e2672f4a2190008fef4214c691bc8485ec68902cb8852fd441`.
This does not establish that the deployed `corelink-spawn-worker` target is on
that version, exports the class to the Server binding, has the public-key map,
or has been qualified. Do not add the binding until those live checks pass. A
Server deploy with an unresolved cross-worker target can be rejected as a whole,
including unrelated changes.

## Compute-grant trust contract

The Server issuer requires two values on every Server production environment:

- `COMPUTE_GRANT_SIGNING_KEY`: base64 PKCS#8 Ed25519 private key. It is a
  Server-only secret and must never be sent to Runners or written to config.
- `COMPUTE_GRANT_KEY_ID`: the active public-key identifier. It is configuration,
  but is provisioned and verified with the signer because a mismatch makes all
  grants fail closed.

Runners must separately receive `FABRIC_COMPUTE_GRANT_PUBLIC_KEYS`: a JSON map
from the same key id to a standard-base64, raw 32-byte Ed25519 public key. This
is public material, but it must be deployed with the Runners verifier before any
Server issuer is activated. The Server does not currently consume or configure
that map, so adding only its signer and key id cannot make DevEnv runnable.

The verifier release must accept only the Server's compact
`base64url(payload).base64url(signature)` encoding; verify the Ed25519
signature against the mapped public key; and reject an unknown key id, malformed
encoding, expired or not-yet-valid grant, payload/RPC disagreement, wrong
workload kind, excess vCPU or wall-time, or a token larger than 8 KiB. The
grant's 90-second issuance lifetime is not an activation lifetime: the bound
maximum wall time is separately capped at eight hours. Keep both checks.

## Safe order

1. Preflight the Runners release in its repository: exact
   `corelink-spawn-worker` target exists in the same account; `RunnerDevEnvDO`
   is exported and migrated; the three RPC methods have the Server wire types;
   the verifier has an active `FABRIC_COMPUTE_GRANT_PUBLIC_KEYS` key-id-to-raw-
   Ed25519-public-key map; and the DevEnv image is the published immutable
   digest recorded above. Deploy and verify that target first.
2. Preflight the Server checkout: run
   `python3 scripts/verify_devenv_deploy_contract.py`, validate the D1 migration
   set with the repository migration checker, and verify the pending range is
   exactly `0118` through `0130`. Do not apply only `0127`--`0130`: those
   credential tables depend on the prior transition and audit-chain migrations
   being in order.
3. Provision the Server signer and key id through the approved write-only secret
   process on every Server environment that can issue DevEnv credentials. Verify
   names and deployment scope only; never print, commit, or retrieve secret
   values. Confirm Runners already recognizes the selected key id and public
   key before enabling issuance.
4. Apply D1 migrations sequentially, `0118` → `0130`, to the production
   `corelink-config-prod` database. Take the normal D1 backup/ledger evidence
   and stop on the first failure. These migrations are additive but include
   `ALTER TABLE`, backfill, triggers, and durable lifecycle receipts; do not
   skip or reorder them.
5. Generate the Server release configuration from the fixture. It must mirror
   the exact `script_name`, `class_name`, and binding name in root, `prod`,
   `prod-sam`, `prod-lhr`, `prod-nrt`, and `prod-syd`, because named Wrangler
   environment DO lists are non-inherited. Dry-run that release only after the
   target preflight succeeds, then deploy Server.
6. Run a controlled authenticated DevEnv start and verify: Runners accepted a
   correctly signed grant, the Server obligation advanced through prepare/issue/
   adopt, an invalid key id or signature is rejected before container start, and
   compensation calls `abandonAuthorizedCompute` on failed issuance. Enable
   customer discoverability only after these checks pass.

## Preflight evidence

Record version IDs and command exit status, not credentials or grant bodies:

- the deployed Runners version, exported class, migration tag, immutable image
  digest, public-key-map key id, and the three RPC method probes;
- the Server intended deployment version, selected key id, all six declared
  bindings, and a successful dry-run;
- D1 migration ledger before and after the ordered range; and
- the controlled positive and negative grant results plus no unintended
  container start on the negative case.

## Rollback limits

Do not roll back D1 `0118`--`0130` by destructive DDL. They are additive and
must remain in the ledger; revert behavior with a forward migration or disable
the Server binding/issuer path. If the Runners public key or ABI is wrong, leave
the Server binding absent (or remove it in a follow-up deployment) and retain
the Runners verifier release until active grants expire. A signer rotation is a
two-sided rollout: publish the new Runners public key first, deploy Server with
the new key id second, wait beyond the 90-second grant lifetime, then retire the
old public key. Never retire a verifier key before every Server issuer has
stopped using it.

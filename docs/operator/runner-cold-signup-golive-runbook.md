# Runbook — runner installation provision, canary, and deprovision

This runbook covers one operator-controlled production canary using a private,
disposable GitHub repository and a dedicated CoreLink tenant. It exercises the
signup-worker installation map and repository allowlist, runs at most one
bounded GitHub Actions job, and removes the exact fixture state through the
canonical API.

The canary is a separate live operator gate. This document and a code merge do
not prove that the production Worker has the route or dedicated secret bound,
and do not authorize a production dispatch. Before starting, the responsible
operator must independently confirm that the deployed `corelink-signup-worker`
contains both methods on the route, the production D1 binding is correct, and
the live runner can accept this private fixture. If any check is unknown, stop
before provisioning.

## Safety contract

- Use only `CORELINK_RUNNER_PROVISION_AUTH_KEY`, bound for the
  `runner_provision` consumer. It must be at least 32 characters. The shared
  `CORELINK_INTERNAL_AUTH_KEY` is not accepted by these endpoints.
- Load the dedicated key from the approved secret store into the current
  operator process. Disable shell tracing (`set +x`) before loading it. Never
  print, copy into a command transcript, log, hash, or place the key in a file,
  issue, or chat. Do not use `curl -v`, `--trace`, or a shell command that
  expands the key into its recorded command line.
- Use a private fixture repository owned by the intended GitHub organization,
  and a tenant created for this canary alone. The tenant must have its expected
  runner entitlement. The fixture installation must grant the GitHub App
  access only to that fixture repository. Do not use a customer repository or
  a tenant with other runner allowlist entries.
- Confirm the tenant identity and authorization from the approved source of
  truth. In GitHub, confirm the signed-in owner/org, private repository,
  installation ID, and that the installation is the one that can access the
  fixture. Record the numeric installation ID and canonical `owner/repo`.
  These are operator preconditions; the internal endpoint trusts the supplied
  tenant ID and does not establish the caller's identity for you.
- Run the D1 statements below only through the production D1 remote-read path.
  They are all `SELECT`; do not run migrations or write SQL. Never repair or
  clean up D1 state with direct `DELETE` or another direct mutation.
- Provision once, dispatch exactly one workflow run, and never dispatch again
  when the result is ambiguous. Resolve the one run by its unique run name and
  inspect or cancel that run. Deprovision through the API before uninstalling
  the fixture installation or deleting the GitHub repository.

Set identifiers only after the identity checks. The examples use shell
variables for non-secret identifiers; replace each angle-bracket placeholder
with its verified value before running the command.

```sh
set +x
export SIGNUP_BASE='https://corelink-signup.humangr.com'
export TENANT_ID='<verified-dedicated-tenant-id>'
export INSTALLATION_ID='<verified-github-app-installation-id>'
export FIXTURE_REPO='<verified-owner>/<private-fixture-repo>'
export FIXTURE_REF='<verified-fixture-default-branch>'
export DEPROVISION_REQUEST_ID='runner-deprovision:<new-unique-uuid>'
```

`DEPROVISION_REQUEST_ID` must be unique for this deprovision operation and
match `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`. Generate it once and retain it for
the complete cleanup attempt. Reuse the same ID and identical DELETE body for
an intentional retry; never reuse it for another installation or payload.

## 1. Read-only production prechecks

Run these separately with the production D1 database selected explicitly:

```sh
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, primary_region FROM tenant WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, max_concurrency, plan, created_at_ms FROM runners_entitlement WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT installation_id, tenant_id FROM tenant_gh_installation_map WHERE installation_id = '<INSTALLATION_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT installation_id, tenant_id FROM tenant_gh_installation_map WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, repo_full_name FROM runner_repo_allowlist WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, repo_full_name FROM runner_repo_allowlist WHERE repo_full_name = '<OWNER>/<REPO>';"
```

Proceed only when the tenant and its expected entitlement each resolve to the
intended single row, the installation has no existing mapping, the dedicated
tenant has no installation mappings or allowlist entries, and the fixture
repository is not allowlisted for any tenant. Save the entitlement values for
exact before/after comparison. If a query returns unexpected state, stop and
investigate through the owning control plane; do not overwrite it.

## 2. Provision the exact fixture

The POST contract is exactly this three-field JSON object; it has no
`request_id` field:

```json
{
  "installation_id": "<INSTALLATION_ID>",
  "tenant_id": "<TENANT_ID>",
  "repositories": ["<OWNER>/<REPO>"]
}
```

The operator validates the canonical `owner/repo` spelling before sending it.
The handler's POST currently filters nonempty strings and writes with
`INSERT OR IGNORE`; do not rely on it to reject malformed repository names or
conflicting pre-existing rows. The prechecks above establish the intended
empty starting state. A repeated POST is naturally idempotent for the same
installation, tenant, and repository, but it has no request-ID replay record;
after a timeout, do read-only state checks and do not send a second canary
dispatch while the provisioning result is unresolved.

Load the key from the approved store without printing it, with shell tracing
still disabled. This curl pattern passes the header through a process
substitution pipe and prints only the response body and HTTP status:

```sh
: "${CORELINK_RUNNER_PROVISION_AUTH_KEY:?load the dedicated key from the approved secret store}"
PROVISION_BODY=$(printf '{"installation_id":"%s","tenant_id":"%s","repositories":["%s"]}' \
  "$INSTALLATION_ID" "$TENANT_ID" "$FIXTURE_REPO")
curl --silent --show-error --request POST \
  --config <(printf 'header = "Authorization: Bearer %s"\n' "$CORELINK_RUNNER_PROVISION_AUTH_KEY") \
  --header 'Content-Type: application/json' \
  --data-binary "$PROVISION_BODY" \
  --write-out '\nHTTP %{http_code}\n' \
  "$SIGNUP_BASE/internal/v1/runner/provision-installation"
```

Expected success is HTTP `200` with
`{"installation_id":"…","repos_added":1}`. The provisioning handler's
other responses are `405 {"error":"method_not_allowed"}` for a wrong method,
`503 {"error":"unavailable"}` when the dedicated key is absent or too short,
`401 {"error":"unauthorized"}` for a wrong key, `400 {"error":"invalid_json"}`
for malformed JSON, `400 {"error":"installation_id and tenant_id are required"}`
when either ID is missing, `500 {"error":"config_db_unavailable"}` when D1 is
unbound, and `500 {"error":"provision_failed"}` on a write failure. Any
non-200 response is an abort; use the read-only checks to determine whether
any state was written before deciding the next action.

Verify all four installation-map and allowlist queries before dispatching: the
installation maps exactly once to `TENANT_ID`; that tenant has exactly that
one installation mapping and exactly one fixture allowlist row; and the
repository-wide query returns exactly that same tenant/repository pair. If the
observed state differs, abort and use the canonical DELETE endpoint to remove
only state proven to belong to this fixture.

## 3. Dispatch one bounded private canary

The disposable private fixture must contain a workflow that has all of these
properties before dispatch:

- `workflow_dispatch` only; one job on the approved `corelink` runner label;
- a two-minute job timeout and a wall-clock operator deadline of five minutes;
- a required unique `canary_request_id` input and
  `run-name: corelink-installation-canary:${{ inputs.canary_request_id }}`;
- no repository or organization secrets, no checkout, no write permissions,
  and no steps that modify the repository or external state.

Run this once with a fresh non-secret ID. `gh workflow run` targets the
fixture's default branch; the workflow filename, input, and `run-name` above
must match exactly. `gh api` is quoted for zsh so `?per_page` is not treated as
a glob. The follow-up query is read-only and must return exactly one run; poll
or cancel that run by ID only.

```sh
CANARY_ID='runner-canary:<fresh-unique-uuid>'
gh workflow run corelink-installation-canary.yml --repo "$FIXTURE_REPO" \
  --ref "$FIXTURE_REF" -f canary_request_id="$CANARY_ID"
gh api "repos/$FIXTURE_REPO/actions/runs?per_page=20" --jq \
  ".workflow_runs[] | select(.event == \"workflow_dispatch\" and .display_title == \"corelink-installation-canary:$CANARY_ID\") | {id,status,conclusion,head_branch,repository:.repository.full_name}"
```

Confirm one matching run and the fixture repository/ref. Never repeat the
dispatch if the CLI response or run lookup is ambiguous. If queued or running
beyond the five-minute wall-clock deadline, cancel only that run with
`gh run cancel "$RUN_ID" --repo "$FIXTURE_REPO"` and wait for a terminal
state before cleanup. Missing/duplicate run records or failed cancellation
are aborts; do not dispatch again.

## 4. Deprovision before removing the GitHub fixture

After the canary reaches a terminal state, call
`DELETE /internal/v1/runner/provision-installation` with this exact five-field
JSON body. For `remove_installation: true`, the request must name every
allowlisted repository for the tenant. The dedicated one-fixture tenant
precondition makes that set exactly the fixture repository.

```json
{
  "request_id": "<DEPROVISION_REQUEST_ID>",
  "installation_id": "<INSTALLATION_ID>",
  "tenant_id": "<TENANT_ID>",
  "repositories": ["<OWNER>/<REPO>"],
  "remove_installation": true
}
```

Use the same secret-safe curl pattern, changing the method and body:

```sh
DEPROVISION_BODY=$(printf '{"request_id":"%s","installation_id":"%s","tenant_id":"%s","repositories":["%s"],"remove_installation":true}' \
  "$DEPROVISION_REQUEST_ID" "$INSTALLATION_ID" "$TENANT_ID" "$FIXTURE_REPO")
curl --silent --show-error --request DELETE \
  --config <(printf 'header = "Authorization: Bearer %s"\n' "$CORELINK_RUNNER_PROVISION_AUTH_KEY") \
  --header 'Content-Type: application/json' \
  --data-binary "$DEPROVISION_BODY" \
  --write-out '\nHTTP %{http_code}\n' \
  "$SIGNUP_BASE/internal/v1/runner/provision-installation"
```

Expected first success is HTTP `200` with
`{"installation_id":"…","repos_removed":1,"installation_removed":true,"replayed":false}`.
An identical retry with the same `request_id` and exact body returns HTTP `200`
with `"replayed":true` if the original operation committed and the deleted
state still matches. Reusing a request ID with a different tenant, installation,
repository list, or removal flag returns `409 {"error":"request_id_conflict"}`.
Other responses are `405 {"error":"method_not_allowed"}`,
`503 {"error":"unavailable"}`, `401 {"error":"unauthorized"}`,
`400 {"error":"invalid_json"}` or `400 {"error":"invalid_body"}`,
`500 {"error":"config_db_unavailable"}`,
`404 {"error":"installation_not_found"}`,
`409 {"error":"tenant_mismatch"}`,
`500 {"error":"tenant_region_unavailable"}`,
`404 {"error":"repository_not_found"}`,
`409 {"error":"installation_repositories_remain"}`,
`500 {"error":"deprovision_verify_failed"}`, or
`500 {"error":"deprovision_failed"}`. Stop on any non-200 and inspect using
read-only queries. Do not change the request ID or issue direct D1 writes.

## 5. Zero-readback, audit, and fixture removal

Run the following statements against `corelink-prod-d1` with `--remote`; every
statement is read-only:

```sh
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT installation_id, tenant_id FROM tenant_gh_installation_map WHERE installation_id = '<INSTALLATION_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT installation_id, tenant_id FROM tenant_gh_installation_map WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, repo_full_name FROM runner_repo_allowlist WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, repo_full_name FROM runner_repo_allowlist WHERE repo_full_name = '<OWNER>/<REPO>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT tenant_id, max_concurrency, plan, created_at_ms FROM runners_entitlement WHERE tenant_id = '<TENANT_ID>';"
npx wrangler d1 execute corelink-prod-d1 --remote --command \
  "SELECT request_id, tenant_id, event_type, payload_json FROM audit_outbox WHERE request_id = '<DEPROVISION_REQUEST_ID>' AND event_type = 'corelink.runner.installation_deprovision_requested';"
```

Completion requires: no row for the installation mapping; no mapping to the
tenant; no allowlist row for the tenant or fixture repository; the entitlement
row's `tenant_id`, `max_concurrency`, `plan`, and `created_at_ms` unchanged
from the precheck; and exactly one matching audit-outbox event for the unique
request ID. Confirm the event belongs to the tenant and installation
and is sanitized: its data has only `installation_id`,
`repositories_sha256`, `repository_count`, and `remove_installation`; it does
not contain the authorization key or repository names. The event records the
hash of the sorted repository-name list, not a hash of any credential. If the
audit event is absent, duplicated, mismatched, or contains unexpected data,
stop and preserve the fixture for investigation.

Only after all zero-readbacks and the audit check pass may the operator
uninstall the GitHub App from the fixture and delete the private fixture repo.
Then confirm the installation is no longer present in the GitHub App's
installation UI. Keep the sanitized response/status, identifiers, query
results, run URL, audit result, and cleanup confirmation as evidence; never
include the auth key.

## Abort and rollback rules

- Before a successful provision, any identity, entitlement, or empty-state
  mismatch means stop without writes or a canary dispatch.
- After provision, every exit path goes through the canonical DELETE. If the
  canary is still active, cancel and confirm the exact run is terminal first.
- If a provision or DELETE response is lost, use read-only D1 checks to
  determine the state. For an ambiguous DELETE, retry only the identical body
  with the same request ID; its replay contract returns the committed result.
  Do not dispatch another canary. For ambiguous provisioning, do not dispatch
  until the exact mapping and allowlist state is known.
- If DELETE fails, D1 readback is nonzero, audit evidence is missing, or the
  runner cannot be stopped, keep the GitHub installation and private fixture
  intact and escalate to the service owner. Never delete the fixture first and
  never issue direct D1 `DELETE`, `UPDATE`, or `INSERT` statements.
- The tenant and its runner entitlement are not part of fixture cleanup. Do
  not delete or alter either as rollback.

## Separate public cold-signup activation gate

The bounded canary above verifies the internal installation mapping lifecycle.
It does not prove public cold self-serve signup, OAuth ownership verification,
admin-ui configuration, or production deployment readiness. The following
configuration procedure preserves the earlier Option-B operational path; run
it only under the separate owner-approved live gate. This document does not
assert the present production secret bindings, App visibility, or deployment
state. The older runbook named GitHub App ID `144561227`; verify the intended
production App's identity in GitHub before changing any setting.

### Configuration sequence

1. In the verified GitHub App settings, set **Setup URL** to
   `https://corelink-signup.humangr.com/install/github/callback`. Keep
   **Redirect on update** enabled if installation updates should revisit the
   idempotent callback. Enable **Request user authorization (OAuth) during
   installation** before binding OAuth credentials.
2. Bind `GITHUB_APP_CLIENT_ID` and `GITHUB_APP_CLIENT_SECRET` on the production
   `corelink-signup-worker` through the approved secret-binding path. Enter
   values only into its protected input; never put literal values in shell
   commands, logs, chat, or evidence. Confirm both secret *names* are bound
   using the deployment control plane; the Worker secret values are write-only.
   The callback requires the OAuth code and proves that the installing user
   controls the presented installation whenever both credentials are bound.
3. After both OAuth credential bindings and the GitHub OAuth setting are
   verified, configure `GITHUB_APP_PUBLIC=true` on the signup-worker and
   deploy it through the approved release path. Then switch the verified GitHub
   App to public. The callback has a structural fail-closed guard: a public App
   without both OAuth credentials returns HTTP 403 before provisioning. Do not
   interpret this documentation or a configured variable as proof of deploy.
4. For the admin-ui Install button, verify `GITHUB_APP_SLUG` and
   `INSTALL_STATE_SIGNING_KEY` on the production admin-ui. The signing key must
   match the signup-worker's `INSTALL_STATE_SIGNING_KEY` byte-for-byte. Set the
   signup-worker's `ADMIN_UI_PUBLIC_URL` to the current public app base
   `https://humangr.com/corelink`; the callback appends
   `/settings/runners`. The `/corelink` base is confirmed by
   `apps/admin-ui/next.config.ts` and `apps/admin-ui/wrangler.toml`. Do not copy
   write-only values into evidence; verify their names and then test via an
   authenticated tenant session.
5. As an authenticated tenant, use the Install button and complete the GitHub
   OAuth ownership prompt. Verify the browser returns to
   `/corelink/settings/runners` with `runner_install=ok`, then perform the
   separate owner-approved live proof: confirm the installation's tenant map
   and exact repository allowlist with read-only D1 queries, and run the
   one-authorized runner mint/acquire check for that tenant. The bounded fixture
   canary above does not substitute for this self-serve identity proof.

The former “Faster dogfood proof” POST example is obsolete and intentionally
not retained: it used `CORELINK_INTERNAL_AUTH_KEY`, while the current
`resolveRunnerProvisionKey` accepts only the dedicated
`CORELINK_RUNNER_PROVISION_AUTH_KEY` and returns 503 when it is absent or too
short. The explicit fixture lifecycle above also requires canonical DELETE,
read-only zero-readback, and audit verification before GitHub fixture removal.

Do not mark public cold-signup go-live complete until the separate live gate
has its own verified evidence for the App settings, both deployed Worker
configurations, OAuth ownership check, and tenant-scoped runner proof.

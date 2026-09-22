# B-012 GitHub App provisioning packet

This repository uses a private GitHub App to create automatic pull requests.
Each creator job mints a one-hour installation token for exactly
`HuGR-dev/corelink-server`; no PAT or installation token is stored as a durable
Actions secret.

## App manifest

Create the App from the GitHub App manifest flow while authenticated as an
owner of `HuGR-dev`. The manifest is intentionally limited to the permissions
needed by the five automatic PR creators:

```json
{
  "name": "corelink-bot-ci",
  "url": "https://github.com/HuGR-dev/corelink-server",
  "redirect_url": "https://github.com/HuGR-dev/corelink-server",
  "public": false,
  "default_permissions": {
    "contents": "write",
    "pull_requests": "write",
    "metadata": "read"
  },
  "default_events": []
}
```

Use the organization App settings flow at
`https://github.com/organizations/HuGR-dev/settings/apps/new`, choose the
manifest option, and paste the JSON above. Leave webhooks and all other event
subscriptions disabled. Install the resulting App on **Only select
repositories** → `corelink-server`.

Record the following metadata in the owner’s password manager. Do not put the
private key, its hash, or a secret value in this repository:

| Item | Required value |
| --- | --- |
| App name | `corelink-bot-ci` |
| Installation owner | `HuGR-dev` |
| Installation repository | `corelink-server` only |
| App permissions | `contents: write`, `pull_requests: write`, `metadata: read` |
| Webhooks/events | disabled / none |
| Repository secret 1 | `CORELINK_BOT_APP_ID` = numeric App ID |
| Repository secret 2 | `CORELINK_BOT_APP_PRIVATE_KEY` = generated PEM |
| Durable installation token | forbidden |
| PAT (`BOT_PR_TOKEN`) | forbidden and must remain absent |

## Secret storage

Check names only, then provide values through protected stdin. The PEM must
come from the one-time App key download or password manager; it must never be
an argument, shell history entry, log line, commit, or evidence field.

```bash
gh secret list --repo HuGR-dev/corelink-server
printf '%s' "$APP_ID" | gh secret set CORELINK_BOT_APP_ID \
  --repo HuGR-dev/corelink-server --body-stdin
gh secret set CORELINK_BOT_APP_PRIVATE_KEY \
  --repo HuGR-dev/corelink-server --body-stdin < /protected/path/corelink-bot-ci.pem
gh secret list --repo HuGR-dev/corelink-server
```

The final listing should show the two names and must not be copied into an
issue comment or evidence file with timestamps or values. If `BOT_PR_TOKEN`
exists, stop and revoke it before enabling this App path.

## Workflow contract

The five creators invoke the pinned
`actions/create-github-app-token@def152b8a737443d7af6c5722c6389146fe90c90`
action with `owner: HuGR-dev`, `repositories: corelink-server`,
  `permission-metadata: read`, `permission-contents: write`, and
  `permission-pull-requests: write`. The
checkout, branch push, PR creation, comment, and draft-release steps consume
only `steps.app-token.outputs.token`; a missing App secret fails the job before
any mutation. `scripts/verify_bot_pr_auth.py` is the fail-closed static guard.

The App cannot merge a PR through repository automation: generated PRs still
require the normal human review and branch protection rules. App installation
or repository secret changes are owner actions and are not performed by this
repository change.

The `pull_request` drift checks in `api-reference-sync.yml` and
`subprocessors-sync.yml` are fork-safe read-only jobs. Their default
`GITHUB_TOKEN` has no pull-request write permission, and checkout does not
persist credentials. The trusted post-merge/scheduled creator jobs use the
short-lived App token for every write.

## Rotation and revocation

Rotate the App private key at least every 90 days, or immediately on suspected
exposure. Generate a replacement key in the App settings, update
`CORELINK_BOT_APP_PRIVATE_KEY` through protected stdin, run one bounded probe,
then revoke the previous key. For full revocation, uninstall the App from
`corelink-server`, delete both repository secrets, and review open bot branches
and PRs. Never revoke release credentials as part of this procedure.

## Bounded proof probe and receipt

After the App is installed and the repository secret names are present, open
one harmless, App-authored proof PR and keep it unmerged. Record only the
metadata in `evidence/owner-actions/B-012/bot-pr-checks.json`. The proof checks
that a PR opened by the App can run the hosted DCO and rustfmt jobs. It does
not exercise the five scheduled creator workflows or prove that the separate
`corelink` runner is available.

The canonical receipt is `evidence/owner-actions/B-012/bot-pr-checks.json`;
`scripts/verify_b012_bot_pr_evidence.py` enforces its exact schema and the
hosted-job, scope, and redaction requirements.

Never record the installation ID, App private key, JWT, installation token,
or secret values. The receipt is valid only when both hosted jobs completed
successfully, the PR was created by the App identity, no approval was pending,
and no zero-job or `startup_failure` run was used as proof. This runtime
receipt is the boundary for closing issue #1642; the repository verifier
cannot fabricate it.

#!/usr/bin/env bash
# cut-v1-0-0-ga-tag.sh — D-day Owner-action script for applying the
# cryptographically signed `v1.0.0-GA` tag to `main` after the 2-key ceremony.
#
# This is the *single command* the Owner runs on D-day, after:
#   (a) `framework-v1-0-0-ga` tag is already applied (lote-6 Owner sign-off
#       per ADR-0034b §Eligibility — framework FROZEN cut);
#   (b) RB-GA-CUTOVER §3 11-step cutover sequence has executed GREEN against
#       the production tenant ring;
#   (c) 6/6 greenlights GREEN per `dashboards/alerts/dash-ga-greenlight.yml`;
#   (d) two distinct trusted keys have signed commits binding the release SHA
#       and exact evidence digests;
#   (e) every tag-draft placeholder has been replaced with real evidence.
#
# What this DOES:
#   - Pre-flight assertions: main tip matches expected SHA + freeze still
#     active (no working-tree changes outside §3.b allowed paths) + no
#     unstaged modifications + all wave-N impl-sealed tags present.
#   - 2-key sign-off check: verifies two distinct identities, commits and key
#     fingerprints, each bound to the exact release and evidence digests.
#   - Tag application: `git tag -s v1.0.0-GA -F <tag-draft-final> <main-tip>`.
#   - Push: `git push origin v1.0.0-GA`, followed by exact peeled-object
#     readback from the canonical remote.
#   - Release notes remain DRAFT for the tag-triggered editorial/release PR;
#     this script never leaves an uncommitted local thaw behind.
#
# What this does NOT do:
#   - It does NOT create the `framework-v1-0-0-ga` tag (that is a
#     pre-requisite, applied at lote-6 Owner sign-off prep — see
#     `specs/_audits/sealed/2026-05-16-lote-6-owner-signoff-prep.md` cross-ref).
#   - It does NOT execute RB-GA-CUTOVER; it verifies the content-addressed
#     execution attestation and deployment readback supplied by the operator.
#   - It does NOT page on-call, send customer comms, or publish marketing
#     copy — those are downstream of `doc_status` flipping to ACTIVE.
#   - It does NOT manufacture approvals or accept unverifiable envelope text.
#
# Charter constraints honored:
#   - No unsafe code (bash + git + python3 + grep only).
#   - Always exits with an explicit code; on failure, leaves the
#     working tree unchanged (no partial tag, no partial push).
#   - --dry-run mode performs all assertions and prints the command
#     sequence it would run, but does NOT mutate refs.
#   - It makes no commits and leaves release notes to the reviewed editorial PR.
#
# Usage:
#   bash scripts/cut-v1-0-0-ga-tag.sh \
#     --expected-main-sha <SHA> \
#     --cutover-attestation <evidence-commit>:<completed-attestation.json> \
#     --deployment-evidence <production-readback.json> \
#     --ci-evidence <bundled-ci.json> \
#     --signoff-manifest <two-key-signoffs.json>
#
# Exit codes:
#   0 — pre-flight + signed tag application + push + readback all succeeded
#       (or, in --dry-run, all assertions passed).
#   1 — assertion failed (state mismatch, placeholders remaining,
#       missing wave-N tag, working-tree dirty, etc.).
#   2 — invocation error (bad flags, git not available, jq/python3
#       missing, tag-draft file not readable).
#   3 — signing, push, or remote readback failed after local tag creation.

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
DRY_RUN=0
EXPECTED_MAIN_SHA=""
TAG_NAME="v1.0.0-GA"
REMOTE="origin"
TAG_DRAFT="docs/release/v1.0.0-GA-tag-draft-final.txt"
FRAMEWORK_TAG="framework-v1-0-0-ga"
ALLOWED_SIGNERS=".github/release-allowed-signers"
SIGNING_POLICY=".github/release-signing-policy.json"
CUTOVER_ATTESTATION=""
DEPLOYMENT_EVIDENCE=""
CI_EVIDENCE=""
SIGNOFF_MANIFEST=""
EVIDENCE_REF="refs/tags/release-evidence-v1.0.0-GA"
OWNER_SIGNOFF_REF="refs/tags/release-signoff-owner-v1.0.0-GA"
OPS_SIGNOFF_REF="refs/tags/release-signoff-ops-v1.0.0-GA"

EXPECTED_WAVE_TAGS=(
  "wave-14-impl-sealed"
  "wave-15-impl-sealed"
  "wave-16-impl-sealed"
  "wave-17-impl-sealed"
  "wave-18-impl-sealed"
  "wave-19-impl-sealed"
  "wave-20-impl-sealed"
  "wave-21-impl-sealed"
  "wave-22-impl-sealed"
  "wave-23-impl-sealed"
  "wave-24-impl-sealed"
  "wave-25-impl-sealed"
  "wave-26-impl-sealed"
)

# ---------------------------------------------------------------------------
# Arg parse
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --expected-main-sha) EXPECTED_MAIN_SHA="$2"; shift 2 ;;
    --cutover-attestation) CUTOVER_ATTESTATION="$2"; shift 2 ;;
    --deployment-evidence) DEPLOYMENT_EVIDENCE="$2"; shift 2 ;;
    --ci-evidence) CI_EVIDENCE="$2"; shift 2 ;;
    --signoff-manifest) SIGNOFF_MANIFEST="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^set -euo/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "ERROR: unknown flag: $1" >&2
      echo "Run with --help for usage." >&2
      exit 2
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------------
log_info()  { printf '[ cut-v1-tag ] %s\n' "$*"; }
log_ok()    { printf '[ cut-v1-tag ] OK   : %s\n' "$*"; }
log_fail()  { printf '[ cut-v1-tag ] FAIL : %s\n' "$*" >&2; }
log_plan()  { printf '[ cut-v1-tag ] PLAN : %s\n' "$*"; }

# ---------------------------------------------------------------------------
# Repo root resolution + cd
# ---------------------------------------------------------------------------
if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  log_fail "not inside a git repository"
  exit 2
fi
cd "$REPO_ROOT"

log_info "repo root: $REPO_ROOT"
log_info "dry-run  : $DRY_RUN"
log_info "tag      : $TAG_NAME"
log_info "draft    : $TAG_DRAFT"
log_info "remote   : $REMOTE"

# ---------------------------------------------------------------------------
# Pre-flight § A — required tooling
# ---------------------------------------------------------------------------
for bin in git python3 grep; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    log_fail "required tool not on PATH: $bin"
    exit 2
  fi
done
SIGNING_FORMAT="$(git config --get gpg.format || true)"
if [[ "$SIGNING_FORMAT" != "ssh" ]]; then
  log_fail "release contract requires gpg.format=ssh; observed ${SIGNING_FORMAT:-unset}"
  exit 1
fi
if [[ ! -r "$ALLOWED_SIGNERS" || ! -r "$SIGNING_POLICY" ]]; then
  log_fail "release SSH trust root or role policy is missing"
  exit 1
fi

# ---------------------------------------------------------------------------
# Pre-flight § B — branch + tip SHA
# ---------------------------------------------------------------------------
CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$CURRENT_BRANCH" != "main" ]]; then
  log_fail "must be on branch 'main' (current: $CURRENT_BRANCH)"
  exit 1
else
  log_ok "branch is main"
fi

MAIN_TIP="$(git rev-parse HEAD)"
log_info "main tip : $MAIN_TIP"

if [[ -z "$EXPECTED_MAIN_SHA" ]]; then
  log_fail "--expected-main-sha is mandatory for a release cut"
  exit 2
fi
if [[ ! "$EXPECTED_MAIN_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  log_fail "--expected-main-sha must be exactly 40 lowercase hex characters"
  exit 2
fi
if ! EXPECTED_MAIN_SHA_FULL="$(git rev-parse --verify "${EXPECTED_MAIN_SHA}^{commit}" 2>/dev/null)"; then
  log_fail "--expected-main-sha does not resolve to a commit: $EXPECTED_MAIN_SHA"
  exit 2
fi
if [[ "$MAIN_TIP" != "$EXPECTED_MAIN_SHA_FULL" ]]; then
  log_fail "main tip SHA mismatch: expected $EXPECTED_MAIN_SHA_FULL, got $MAIN_TIP"
  exit 1
fi
log_ok "main tip matches exact expected SHA"

# ---------------------------------------------------------------------------
# Pre-flight § C — working tree clean
# ---------------------------------------------------------------------------
if [[ -n "$(git status --porcelain)" ]]; then
  log_fail "working tree has uncommitted changes; refuse to tag"
  git status --short >&2
  exit 1
else
  log_ok "working tree clean"
fi

# ---------------------------------------------------------------------------
# Pre-flight § D — freeze still active (best-effort)
# ---------------------------------------------------------------------------
FREEZE_CHECKER="scripts/check-ga-freeze-allowed.py"
if [[ -x "$FREEZE_CHECKER" ]] || [[ -f "$FREEZE_CHECKER" ]]; then
  if python3 "$FREEZE_CHECKER" --check-empty >/dev/null 2>&1; then
    log_ok "GA freeze gate clean (no unstaged frozen-surface violations)"
  else
    log_fail "GA freeze gate reports violations — refuse to tag"
    python3 "$FREEZE_CHECKER" --check-empty >&2 || true
    exit 1
  fi
else
  log_fail "required freeze checker not found at $FREEZE_CHECKER"
  exit 1
fi

# ---------------------------------------------------------------------------
# Pre-flight § E — all wave-N impl-sealed tags present
# ---------------------------------------------------------------------------
MISSING_WAVE_TAGS=()
for t in "${EXPECTED_WAVE_TAGS[@]}"; do
  if ! git rev-parse -q --verify "refs/tags/$t" >/dev/null; then
    MISSING_WAVE_TAGS+=("$t")
  elif ! git merge-base --is-ancestor "refs/tags/$t" "$MAIN_TIP"; then
    log_fail "wave tag is not ancestral to release commit: $t"
    exit 1
  fi
done
if [[ ${#MISSING_WAVE_TAGS[@]} -gt 0 ]]; then
  log_fail "missing wave tags: ${MISSING_WAVE_TAGS[*]}"
  exit 1
fi
log_ok "all ${#EXPECTED_WAVE_TAGS[@]} wave-N impl-sealed tags present"

# The framework release is a cryptographically signed, prior promotion gate.
if [[ "$(git cat-file -t "refs/tags/$FRAMEWORK_TAG" 2>/dev/null || true)" != "tag" ]]; then
  log_fail "required signed framework tag is absent or lightweight: $FRAMEWORK_TAG"
  exit 1
fi
if ! git -c "gpg.ssh.allowedSignersFile=$ALLOWED_SIGNERS" verify-tag "$FRAMEWORK_TAG" >/dev/null 2>&1; then
  log_fail "framework tag signature does not verify: $FRAMEWORK_TAG"
  exit 1
fi
FRAMEWORK_SIGNATURE="$(git -c "gpg.ssh.allowedSignersFile=$ALLOWED_SIGNERS" verify-tag --format='%GF%n%GS' "$FRAMEWORK_TAG" 2>/dev/null)"
EXPECTED_OWNER_SIGNATURE="$(python3 -c 'import json,sys; p=json.load(open(sys.argv[1]))["roles"]["owner"]; print(p["fingerprint"]+"\n"+p["principal"])' "$SIGNING_POLICY")"
if [[ "$FRAMEWORK_SIGNATURE" != "$EXPECTED_OWNER_SIGNATURE" ]]; then
  log_fail "framework tag is not signed by the policy owner key"
  exit 1
fi
FRAMEWORK_COMMIT="$(git rev-list -n 1 "$FRAMEWORK_TAG")"
if ! git merge-base --is-ancestor "$FRAMEWORK_COMMIT" "$MAIN_TIP"; then
  log_fail "$FRAMEWORK_TAG target is not an ancestor of release commit"
  exit 1
fi
if ! python3 - "$FRAMEWORK_COMMIT" <<'PY'
import subprocess
import sys

sha = sys.argv[1]
text = subprocess.check_output(["git", "show", f"{sha}:specs/00_framework.md"], text=True)
frontmatter = text.split("---", 2)[1]
if 'doc_status: "FROZEN"' not in frontmatter or 'version: "1.0.0"' not in frontmatter:
    raise SystemExit("framework must be FROZEN at version 1.0.0")
PY
then
  log_fail "release commit does not contain the promoted FROZEN framework v1.0.0"
  exit 1
fi
log_ok "signed framework promotion is verified and ancestral"

# These are release gates, not comments: execute them at the exact release SHA.
python3 scripts/validate_specs.py >/dev/null || {
  log_fail "spec validation failed"
  exit 1
}
python3 scripts/validate_references.py >/dev/null || {
  log_fail "cross-reference validation failed"
  exit 1
}
log_ok "spec and cross-reference validators passed"

# ---------------------------------------------------------------------------
# Pre-flight § F — tag does not already exist
# ---------------------------------------------------------------------------
if git rev-parse -q --verify "refs/tags/$TAG_NAME" >/dev/null; then
  log_fail "tag $TAG_NAME already exists at $(git rev-parse refs/tags/$TAG_NAME)"
  exit 1
fi
log_ok "tag $TAG_NAME does not yet exist locally"

# ---------------------------------------------------------------------------
# Pre-flight § G — tag draft readable + placeholders replaced
# ---------------------------------------------------------------------------
if [[ ! -r "$TAG_DRAFT" ]]; then
  log_fail "tag draft not readable: $TAG_DRAFT"
  exit 2
fi
log_ok "tag draft readable: $TAG_DRAFT"

EVIDENCE_COMMIT=""
for evidence_var in CUTOVER_ATTESTATION DEPLOYMENT_EVIDENCE CI_EVIDENCE; do
  evidence_locator="${!evidence_var}"
  if [[ ! "$evidence_locator" =~ ^[0-9a-f]{40}:[A-Za-z0-9_./-]+$ ]] ||
     ! git cat-file -e "$evidence_locator" 2>/dev/null; then
    log_fail "--${evidence_var,,} must be an immutable <40-hex-commit>:<path> locator"
    exit 2
  fi
  evidence_commit="${evidence_locator%%:*}"
  if ! git merge-base --is-ancestor "$MAIN_TIP" "$evidence_commit"; then
    log_fail "evidence commit must descend from the exact release commit: $evidence_locator"
    exit 1
  fi
  if [[ -n "$EVIDENCE_COMMIT" && "$evidence_commit" != "$EVIDENCE_COMMIT" ]]; then
    log_fail "all release evidence must live in one immutable evidence commit"
    exit 1
  fi
  EVIDENCE_COMMIT="$evidence_commit"
done

file_sha256() {
  python3 - "$1" <<'PY'
import hashlib
import subprocess
import sys
print(hashlib.sha256(subprocess.check_output(["git", "show", sys.argv[1]])).hexdigest())
PY
}
CUTOVER_SHA256="$(file_sha256 "$CUTOVER_ATTESTATION")"
DEPLOYMENT_SHA256="$(file_sha256 "$DEPLOYMENT_EVIDENCE")"
CI_SHA256="$(file_sha256 "$CI_EVIDENCE")"
FRAMEWORK_TAG_OBJECT="$(git rev-parse "refs/tags/$FRAMEWORK_TAG")"

OCI_DIGEST="$(git show "$DEPLOYMENT_EVIDENCE" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("oci_digest", ""))')"
python3 - "$CUTOVER_ATTESTATION" "$DEPLOYMENT_EVIDENCE" "$CI_EVIDENCE" "$MAIN_TIP" "$OCI_DIGEST" <<'PY' || {
import datetime
import json
import subprocess
import sys

cutover_path, deployment_path, ci_path, release_sha, oci_digest = sys.argv[1:]
def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result

def load(path):
    return json.loads(subprocess.check_output(["git", "show", path]), object_pairs_hook=unique_object)

def timestamp(data):
    stamp = datetime.datetime.fromisoformat(data["generated_at"].replace("Z", "+00:00"))
    if stamp.tzinfo is None or stamp > datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=5):
        raise ValueError("evidence timestamp is missing timezone or is in the future")

cutover = load(cutover_path)
deployment = load(deployment_path)
ci = load(ci_path)
for item in (cutover, deployment, ci):
    timestamp(item)
if (
    cutover.get("release_sha") != release_sha
    or cutover.get("verdict") != "GREEN"
    or cutover.get("greenlights") != {"passed": 6, "total": 6}
    or cutover.get("steps") != {"passed": 11, "total": 11}
):
    raise SystemExit("cutover evidence schema/value mismatch")
expected_envs = {"prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"}
if deployment.get("release_sha") != release_sha or deployment.get("oci_digest") != oci_digest:
    raise SystemExit("deployment evidence release/digest mismatch")
envs = deployment.get("environments")
if not isinstance(envs, list) or {row.get("environment") for row in envs} != expected_envs:
    raise SystemExit("deployment evidence must contain exactly five production environments")
for row in envs:
    if row.get("oci_digest") != oci_digest or row.get("failed") != 0:
        raise SystemExit("deployment environment digest/failure mismatch")
    if not isinstance(row.get("desired"), int) or row["desired"] <= 0 or row.get("healthy") != row["desired"]:
        raise SystemExit("deployment environment is not fully healthy")
release_tree = subprocess.check_output(["git", "show", "-s", "--format=%T", release_sha], text=True).strip()
tested_sha = ci.get("tested_sha")
tested_tree = subprocess.check_output(["git", "show", "-s", "--format=%T", tested_sha], text=True).strip()
if (
    ci.get("tested_tree") != tested_tree
    or ci.get("release_sha") != release_sha
    or ci.get("release_tree") != release_tree
    or (ci.get("pass"), ci.get("fail"), ci.get("infra")) != (19, 0, 0)
    or tested_tree != release_tree
):
    raise SystemExit("bundled CI evidence is not a 19/19 tree-identical release result")
PY
  log_fail "structured release evidence validation failed"
  exit 1
}
log_ok "CI, cutover, and five-environment deployment evidence are structurally release-bound"

if [[ -z "$SIGNOFF_MANIFEST" || ! -r "$SIGNOFF_MANIFEST" ]]; then
  log_fail "--signoff-manifest must name a readable two-key manifest"
  exit 2
fi
SIGNOFF_ROWS="$(python3 - "$SIGNOFF_MANIFEST" "$SIGNING_POLICY" "$MAIN_TIP" "$CUTOVER_SHA256" "$DEPLOYMENT_SHA256" "$CI_SHA256" <<'PY'
import datetime
import json
import pathlib
import subprocess
import sys

path, policy_path, release_sha, cutover_sha, deployment_sha, ci_sha = sys.argv[1:]
data = json.loads(pathlib.Path(path).read_text())
policy = json.loads(pathlib.Path(policy_path).read_text())
roles = policy.get("roles", {})
if set(roles) != {"owner", "ops"}:
    raise SystemExit("signing policy must define exactly owner and ops roles")
for role, authority in roles.items():
    if not authority.get("principal") or not authority.get("fingerprint"):
        raise SystemExit(f"signing policy role is not provisioned: {role}")
expected = {
    "release_sha": release_sha,
    "cutover_attestation_sha256": cutover_sha,
    "deployment_evidence_sha256": deployment_sha,
    "ci_evidence_sha256": ci_sha,
}
for key, value in expected.items():
    if data.get(key) != value:
        raise SystemExit(f"signoff manifest {key} is not bound to release evidence")
signers = data.get("signers")
if not isinstance(signers, list) or len(signers) != 2:
    raise SystemExit("signoff manifest requires exactly two signers")
principals = set()
commits = set()
seen_roles = set()
for signer in signers:
    role = signer.get("role", "")
    principal = signer.get("principal", "")
    commit = signer.get("signed_commit", "")
    signed_at = signer.get("signed_at", "")
    if role not in roles or role in seen_roles:
        raise SystemExit("signers must contain exactly one owner and one ops role")
    if principal != roles[role]["principal"] or principal in principals:
        raise SystemExit("signer principals must be non-empty and distinct")
    if not isinstance(commit, str) or len(commit) != 40 or commit in commits:
        raise SystemExit("signed commits must be distinct full object IDs")
    try:
        stamp = datetime.datetime.fromisoformat(signed_at.replace("Z", "+00:00"))
    except (AttributeError, ValueError):
        raise SystemExit("signed_at must be RFC3339")
    if stamp.tzinfo is None:
        raise SystemExit("signed_at must include a timezone")
    commit_stamp = datetime.datetime.fromisoformat(
        subprocess.check_output(["git", "show", "-s", "--format=%aI", commit], text=True).strip()
    )
    if stamp != commit_stamp or stamp > datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(minutes=5):
        raise SystemExit("signed_at must equal the signed commit timestamp and not be in the future")
    seen_roles.add(role)
    principals.add(principal)
    commits.add(commit)
    print(f"{role}\t{principal}\t{roles[role]['fingerprint']}\t{commit}\t{signed_at}")
PY
)" || {
  log_fail "two-key signoff manifest validation failed"
  exit 1
}
SEEN_SIGNOFF_FINGERPRINTS=""
while IFS=$'\t' read -r role principal expected_fingerprint signed_commit signed_at; do
  [[ -n "$principal" ]] || continue
  if ! git -c "gpg.ssh.allowedSignersFile=$ALLOWED_SIGNERS" verify-commit "$signed_commit" >/dev/null 2>&1; then
    log_fail "signoff commit is not cryptographically trusted: $signed_commit"
    exit 1
  fi
  SIGNER_PRINCIPAL="$(git -c "gpg.ssh.allowedSignersFile=$ALLOWED_SIGNERS" show -s --format=%GS "$signed_commit")"
  SIGNER_FINGERPRINT="$(git -c "gpg.ssh.allowedSignersFile=$ALLOWED_SIGNERS" show -s --format=%GF "$signed_commit")"
  if [[ "$SIGNER_PRINCIPAL" != "$principal" || "$SIGNER_FINGERPRINT" != "$expected_fingerprint" ]]; then
    log_fail "signoff identity does not match trusted signature: $principal"
    exit 1
  fi
  if grep -Fqx "$SIGNER_FINGERPRINT" <<<"$SEEN_SIGNOFF_FINGERPRINTS"; then
    log_fail "two-key signoff reuses one cryptographic key: $SIGNER_FINGERPRINT"
    exit 1
  fi
  SEEN_SIGNOFF_FINGERPRINTS="${SEEN_SIGNOFF_FINGERPRINTS}${SIGNER_FINGERPRINT}"$'\n'
  SIGNOFF_BODY="$(git show -s --format=%B "$signed_commit")"
  for binding in \
    "CoreLink-Release-Role: $role" \
    "CoreLink-Release-SHA: $MAIN_TIP" \
    "Cutover-Attestation-SHA256: $CUTOVER_SHA256" \
    "Deployment-Evidence-SHA256: $DEPLOYMENT_SHA256" \
    "CI-Evidence-SHA256: $CI_SHA256"; do
    grep -Fq "$binding" <<<"$SIGNOFF_BODY" || {
      log_fail "signoff commit $signed_commit lacks binding: $binding"
      exit 1
    }
  done
  if [[ "$role" == "owner" ]]; then
    OWNER_COMMIT="$signed_commit"; OWNER_TIMESTAMP="$signed_at"
  else
    OPS_PRINCIPAL="$principal"; OPS_COMMIT="$signed_commit"; OPS_TIMESTAMP="$signed_at"
  fi
done <<<"$SIGNOFF_ROWS"
log_ok "two distinct cryptographic signoffs are bound to release and evidence"

TAG_MESSAGE="$(mktemp "${TMPDIR:-/tmp}/corelink-release-tag-message.XXXXXX")"
python3 - "$TAG_DRAFT" "$TAG_MESSAGE" \
  "$MAIN_TIP" "$FRAMEWORK_TAG_OBJECT" "$CI_EVIDENCE" "$CI_SHA256" \
  "$OCI_DIGEST" "$DEPLOYMENT_EVIDENCE" "$DEPLOYMENT_SHA256" \
  "$CUTOVER_ATTESTATION" "$CUTOVER_SHA256" "$OWNER_TIMESTAMP" \
  "$OWNER_COMMIT" "$OPS_PRINCIPAL" "$OPS_TIMESTAMP" "$OPS_COMMIT" <<'PY'
import pathlib
import sys

template, output, *values = sys.argv[1:]
keys = [
    "__RELEASE_SHA__", "__FRAMEWORK_TAG_OBJECT__", "__CI_EVIDENCE_PATH__",
    "__CI_EVIDENCE_SHA256__", "__CONTAINER_DIGEST__",
    "__DEPLOYMENT_EVIDENCE_PATH__", "__DEPLOYMENT_EVIDENCE_SHA256__",
    "__CUTOVER_ATTESTATION_PATH__", "__CUTOVER_ATTESTATION_SHA256__",
    "__OWNER_TIMESTAMP__", "__OWNER_SHA__", "__SREL_NAME__",
    "__SREL_TIMESTAMP__", "__SREL_SHA__",
]
source = pathlib.Path(template).read_text()
replacements = dict(zip(keys, values, strict=True))
for placeholder, value in replacements.items():
    if source.count(placeholder) < 1:
        raise SystemExit(f"missing tag-template placeholder: {placeholder}")
    source = source.replace(placeholder, value)
if "__" in source:
    raise SystemExit("rendered tag message retains an unresolved placeholder")
pathlib.Path(output).write_text(source)
PY
trap 'find "$TAG_MESSAGE" "${SIGN_PROBE:-}" "${SIGN_PROBE:-}.sig" -depth -delete 2>/dev/null || true' EXIT
log_ok "release tag message rendered without modifying the release tree"

# Prove that the configured local key can sign before creating any ref.
SIGNING_KEY="$(git config --get user.signingkey || true)"
if [[ -z "$SIGNING_KEY" ]]; then
  log_fail "user.signingkey is required"
  exit 1
fi
PRIVATE_SIGNING_KEY="${SIGNING_KEY%.pub}"
if [[ ! -r "$PRIVATE_SIGNING_KEY" ]] || ! command -v ssh-keygen >/dev/null 2>&1; then
  log_fail "configured SSH signing key is not usable"
  exit 1
fi
SIGN_PROBE="$(mktemp "${TMPDIR:-/tmp}/corelink-release-sign.XXXXXX")"
trap 'find "$TAG_MESSAGE" "$SIGN_PROBE" "$SIGN_PROBE.sig" -depth -delete 2>/dev/null || true' EXIT
printf '%s\n' "CoreLink release signing preflight $MAIN_TIP" > "$SIGN_PROBE"
ssh-keygen -Y sign -q -f "$PRIVATE_SIGNING_KEY" -n git "$SIGN_PROBE" >/dev/null || {
  log_fail "SSH release-signing preflight failed"
  exit 1
}
SIGNING_KEY_BODY="$(awk '{print $1 " " $2}' "$SIGNING_KEY")"
if ! grep -Fq "$SIGNING_KEY_BODY" "$ALLOWED_SIGNERS"; then
  log_fail "configured SSH signing key is not in $ALLOWED_SIGNERS"
  exit 1
fi
LOCAL_SIGNING_FINGERPRINT="$(ssh-keygen -lf "$SIGNING_KEY" | awk '{print $2}')"
OWNER_SIGNING_FINGERPRINT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["roles"]["owner"]["fingerprint"] or "")' "$SIGNING_POLICY")"
if [[ "$LOCAL_SIGNING_FINGERPRINT" != "$OWNER_SIGNING_FINGERPRINT" ]]; then
  log_fail "release tag must be signed by the policy owner key"
  exit 1
fi
log_ok "cryptographic release-signing key preflight passed"

# ---------------------------------------------------------------------------
# Pre-flight § H — release notes draft is the wave-26 DRAFT
# ---------------------------------------------------------------------------
RELEASE_NOTES="RELEASE-NOTES-v1.0.0-GA.md"
if [[ ! -r "$RELEASE_NOTES" ]]; then
  log_fail "release notes not readable: $RELEASE_NOTES"
  exit 2
fi
if grep -qE '^doc_status:[[:space:]]*"DRAFT"' "$RELEASE_NOTES"; then
  log_ok "release notes are in DRAFT state (will flip to ACTIVE post-tag)"
elif grep -qE '^doc_status:[[:space:]]*"ACTIVE"' "$RELEASE_NOTES"; then
  log_fail "release notes are ACTIVE before the release tag exists"
  exit 1
else
  log_fail "release notes doc_status is neither DRAFT nor ACTIVE — unexpected"
  exit 1
fi

# ---------------------------------------------------------------------------
# Pre-flight § I — exact canonical remote CAS readback
# ---------------------------------------------------------------------------
if [[ "$REMOTE" != "origin" ]]; then
  log_fail "canonical release remote must be origin"
  exit 2
fi
REMOTE_URL="$(git remote get-url origin 2>/dev/null || true)"
case "$REMOTE_URL" in
  https://github.com/HuGR-Labs/corelink-server.git|git@github.com:HuGR-Labs/corelink-server.git) ;;
  *) log_fail "origin is not the canonical CoreLink repository: $REMOTE_URL"; exit 1 ;;
esac
REMOTE_MAIN="$(git ls-remote --heads origin refs/heads/main | awk 'NR == 1 {print $1}')"
if [[ -z "$REMOTE_MAIN" || "$REMOTE_MAIN" != "$MAIN_TIP" ]]; then
  log_fail "remote main CAS mismatch: expected $MAIN_TIP, observed ${REMOTE_MAIN:-absent}"
  exit 1
fi
REMOTE_ALL_TAGS="$(git ls-remote --tags origin)"
for wave_tag in "${EXPECTED_WAVE_TAGS[@]}"; do
  remote_wave_object="$(awk -v ref="refs/tags/$wave_tag" '$2 == ref {print $1}' <<<"$REMOTE_ALL_TAGS")"
  if [[ "$remote_wave_object" != "$(git rev-parse "refs/tags/$wave_tag")" ]]; then
    log_fail "wave tag object is absent or different on canonical origin: $wave_tag"
    exit 1
  fi
done
REMOTE_FRAMEWORK_OBJECT="$(awk -v ref="refs/tags/$FRAMEWORK_TAG" '$2 == ref {print $1}' <<<"$REMOTE_ALL_TAGS")"
if [[ "$REMOTE_FRAMEWORK_OBJECT" != "$(git rev-parse "refs/tags/$FRAMEWORK_TAG")" ]]; then
  log_fail "framework tag object is absent or different on canonical origin"
  exit 1
fi
for new_ref in "refs/tags/$TAG_NAME" "$EVIDENCE_REF" "$OWNER_SIGNOFF_REF" "$OPS_SIGNOFF_REF"; do
  if awk -v ref="$new_ref" '$2 == ref {found=1} END {exit !found}' <<<"$REMOTE_ALL_TAGS"; then
    log_fail "release publication ref already exists on canonical remote: $new_ref"
    exit 1
  fi
done
log_ok "canonical origin/main equals exact release SHA and release tag is absent"

# ---------------------------------------------------------------------------
# All pre-flight assertions passed
# ---------------------------------------------------------------------------
log_ok "all pre-flight assertions passed"

# ---------------------------------------------------------------------------
# § Action — create cryptographically signed tag
# ---------------------------------------------------------------------------
if [[ "$DRY_RUN" -eq 1 ]]; then
  log_plan "git tag -s $TAG_NAME -F <rendered-tag-message> $MAIN_TIP"
  log_plan "git verify-tag $TAG_NAME"
  log_plan "git push --atomic origin signed release tag + evidence/owner/ops anchor refs"
  log_plan "verify all four remote object IDs"
  log_info "DRY-RUN complete; no refs mutated."
  exit 0
fi

log_info "creating signed tag $TAG_NAME at $MAIN_TIP"
git tag -s "$TAG_NAME" -F "$TAG_MESSAGE" "$MAIN_TIP"
if ! git -c "gpg.ssh.allowedSignersFile=$ALLOWED_SIGNERS" verify-tag "$TAG_NAME" >/dev/null 2>&1; then
  log_fail "new release tag signature did not verify; refusing push"
  exit 3
fi
log_ok "tag $TAG_NAME created and signature verified locally"

# ---------------------------------------------------------------------------
# § Action — push
# ---------------------------------------------------------------------------
log_info "pushing signed tag $TAG_NAME to canonical origin"
if ! git push --atomic origin \
  "$EVIDENCE_COMMIT:$EVIDENCE_REF" \
  "$OWNER_COMMIT:$OWNER_SIGNOFF_REF" \
  "$OPS_COMMIT:$OPS_SIGNOFF_REF" \
  "refs/tags/$TAG_NAME"; then
  log_fail "tag push failed; signed tag remains local for forensic recovery"
  exit 3
fi
REMOTE_TAG_TARGET="$(git ls-remote --tags origin "refs/tags/$TAG_NAME^{}" | awk 'NR == 1 {print $1}')"
REMOTE_TAG_OBJECT="$(git ls-remote --tags origin "refs/tags/$TAG_NAME" | awk 'NR == 1 {print $1}')"
LOCAL_TAG_OBJECT="$(git rev-parse "refs/tags/$TAG_NAME")"
REMOTE_EVIDENCE_OBJECT="$(git ls-remote --tags origin "$EVIDENCE_REF" | awk 'NR == 1 {print $1}')"
REMOTE_OWNER_OBJECT="$(git ls-remote --tags origin "$OWNER_SIGNOFF_REF" | awk 'NR == 1 {print $1}')"
REMOTE_OPS_OBJECT="$(git ls-remote --tags origin "$OPS_SIGNOFF_REF" | awk 'NR == 1 {print $1}')"
if [[ "$REMOTE_TAG_OBJECT" != "$LOCAL_TAG_OBJECT" || "$REMOTE_TAG_TARGET" != "$MAIN_TIP" ||
      "$REMOTE_EVIDENCE_OBJECT" != "$EVIDENCE_COMMIT" || "$REMOTE_OWNER_OBJECT" != "$OWNER_COMMIT" ||
      "$REMOTE_OPS_OBJECT" != "$OPS_COMMIT" ]]; then
  log_fail "remote tag readback mismatch: expected $MAIN_TIP, observed ${REMOTE_TAG_TARGET:-absent}"
  exit 3
fi
log_ok "remote signed tag peeled target matches exact release SHA"

log_ok "v1.0.0-GA tag application complete"
exit 0

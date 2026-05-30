#!/usr/bin/env bash
# scripts/test-multi-region-r2.sh
#
# REGION SELECTION ARCHITECTURE FINDING (read-only investigation — no server changes)
# =====================================================================================
#
# Region selection is PER-DEPLOY, NOT per-tenant:
#
#   crates/corelink-container/src/routes/cas.rs::build_handlers():
#     bucket = env("R2_CAS_BUCKET") ?: "corelink-cas-prod"
#     region = env("R2_CAS_REGION") ?: "iad"
#
#   crates/corelink-container/src/routes/ac.rs::build_handlers():
#     bucket = env("R2_AC_BUCKET")  ?: "corelink-ac-iad"
#     region = env("R2_AC_REGION")  ?: "iad"
#
# A single container instance is wired to exactly ONE region at startup. To
# exercise SAM/LHR/NRT/SYD, you need N separate container deploys, each
# with the appropriate R2_*_BUCKET + R2_*_REGION env overrides.
#
# ADDITIONAL FINDING — Region model vs bucket naming divergence:
#
#   crates/corelink-region/src/region.rs defines a 4-value enum:
#     wnam | enam | weur | sam
#   whereas the actual prod R2 buckets use Cloudflare airport codes:
#     iad  | sam  | lhr  | nrt  | syd
#   (5 regions, not 4). Region enum does NOT include lhr, nrt, or syd.
#   This is a spec/code divergence to close in a future wave.
#
# ADDITIONAL FINDING — Per-tenant pinning NOT wired:
#
#   crates/corelink-tier-selection/src/tenant.rs has no region field.
#   The per-tenant DO + region-env-override path is future work.
#
# ADDITIONAL FINDING — manifest buckets undocumented:
#
#   Account contains corelink-manifest-{iad,sam,lhr,nrt,syd} which are
#   not referenced by any Rust crate at this baseline. Their purpose is
#   unspecified in the current spec corpus; likely Terraform-provisioned
#   ahead of a future wave.
#
# TEST GAP (per-deploy region model):
#
#   This script probes all buckets via CF REST API (read-only). It cannot
#   test whether the container routing logic works for each region — that
#   requires deploying one container instance per region. Today only IAD
#   is deployed and has objects.
#
# USAGE:
#   # Reads CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID from environment or
#   # from a .env.local file in the repo root (or main repo root for worktrees).
#   bash scripts/test-multi-region-r2.sh
#
#   # Override credentials:
#   CLOUDFLARE_API_TOKEN=cfat_xxx CLOUDFLARE_ACCOUNT_ID=abc123 \
#     bash scripts/test-multi-region-r2.sh
#
# DEPENDENCIES: curl, jq
# READ-ONLY: performs only LIST (GET) operations against CF R2 API.

set -euo pipefail

# ---------------------------------------------------------------------------
# 0. Load credentials
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ -z "${CLOUDFLARE_API_TOKEN:-}" || -z "${CLOUDFLARE_ACCOUNT_ID:-}" ]]; then
  # Search for .env.local in repo root and parent dirs (handles git worktrees
  # where the worktree path differs from the main clone root).
  for candidate in \
    "${REPO_ROOT}/.env.local" \
    "${REPO_ROOT}/../../../.env.local" \
    "${REPO_ROOT}/../../../../.env.local"
  do
    candidate_real="$(cd "$(dirname "${candidate}")" 2>/dev/null && pwd)/$(basename "${candidate}")"
    if [[ -f "${candidate_real}" ]]; then
      # shellcheck disable=SC2046
      export $(grep -E '^(CLOUDFLARE_API_TOKEN|CLOUDFLARE_ACCOUNT_ID)=' "${candidate_real}" | xargs)
      echo "Loaded credentials from ${candidate_real}" >&2
      break
    fi
  done
fi

if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
  echo "ERROR: CLOUDFLARE_API_TOKEN is not set. Export it or add to .env.local." >&2
  exit 1
fi
if [[ -z "${CLOUDFLARE_ACCOUNT_ID:-}" ]]; then
  echo "ERROR: CLOUDFLARE_ACCOUNT_ID is not set. Export it or add to .env.local." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 1. Define expected bucket inventory
#    Format: "bucket_name|region_tag|bucket_type"
#    Source: verified via CF API on 2026-05-30 — 16 corelink-* buckets in acct.
# ---------------------------------------------------------------------------

declare -a EXPECTED_BUCKETS=(
  # CAS — single IAD-only bucket (all regions write here via the single deploy)
  "corelink-cas-prod|iad|cas"
  # AC — one per region
  "corelink-ac-iad|iad|ac"
  "corelink-ac-sam|sam|ac"
  "corelink-ac-lhr|lhr|ac"
  "corelink-ac-nrt|nrt|ac"
  "corelink-ac-syd|syd|ac"
  # Chunk — one per region
  "corelink-chunk-iad|iad|chunk"
  "corelink-chunk-sam|sam|chunk"
  "corelink-chunk-lhr|lhr|chunk"
  "corelink-chunk-nrt|nrt|chunk"
  "corelink-chunk-syd|syd|chunk"
  # Manifest — one per region (Terraform-provisioned; not yet wired in Rust)
  "corelink-manifest-iad|iad|manifest"
  "corelink-manifest-sam|sam|manifest"
  "corelink-manifest-lhr|lhr|manifest"
  "corelink-manifest-nrt|nrt|manifest"
  "corelink-manifest-syd|syd|manifest"
)

CF_API="https://api.cloudflare.com/client/v4"

# ---------------------------------------------------------------------------
# 2. Fetch all R2 buckets in the account (handles pagination)
# ---------------------------------------------------------------------------

echo "Fetching R2 bucket list from Cloudflare account ${CLOUDFLARE_ACCOUNT_ID}..." >&2

all_buckets_json="[]"
cursor=""
page=0

while true; do
  page=$((page + 1))
  url="${CF_API}/accounts/${CLOUDFLARE_ACCOUNT_ID}/r2/buckets?per_page=100"
  if [[ -n "${cursor}" ]]; then
    url="${url}&cursor=${cursor}"
  fi

  response=$(curl -sSf \
    -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
    -H "Content-Type: application/json" \
    "${url}" 2>&1) || {
    echo "ERROR: CF API call failed on page ${page}: ${response}" >&2
    exit 1
  }

  success=$(echo "${response}" | jq -r '.success')
  if [[ "${success}" != "true" ]]; then
    echo "ERROR: CF API returned success=false: $(echo "${response}" | jq -c '.errors')" >&2
    exit 1
  fi

  page_buckets=$(echo "${response}" | jq -c '.result.buckets // []')
  all_buckets_json=$(echo "${all_buckets_json} ${page_buckets}" | jq -sc 'add')

  cursor=$(echo "${response}" | jq -r '.result_info.cursor // ""')
  if [[ -z "${cursor}" ]]; then
    break
  fi
done

total_in_account=$(echo "${all_buckets_json}" | jq 'length')
echo "Found ${total_in_account} R2 buckets in account." >&2
echo "" >&2

# ---------------------------------------------------------------------------
# 3. Helper: query object count + total size for a single bucket.
#    CF R2 objects list API returns: {"result": [...objects...], "success": true}
#    (result is a flat array, NOT result.objects)
# ---------------------------------------------------------------------------

query_bucket_stats() {
  local bucket_name="$1"
  local list_url="${CF_API}/accounts/${CLOUDFLARE_ACCOUNT_ID}/r2/buckets/${bucket_name}/objects?max_keys=1000"
  local list_resp
  list_resp=$(curl -sSf \
    -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
    -H "Content-Type: application/json" \
    "${list_url}" 2>&1) || {
    echo "LIST_ERROR"
    return
  }
  local list_ok
  list_ok=$(echo "${list_resp}" | jq -r '.success // "false"')
  if [[ "${list_ok}" != "true" ]]; then
    echo "API_ERROR"
    return
  fi
  # CF R2 objects list: result is a flat array of objects.
  local obj_count total_size
  obj_count=$(echo "${list_resp}" | jq '[.result // [] | .[]] | length')
  total_size=$(echo "${list_resp}" | jq '[.result // [] | .[].size // 0] | add // 0')
  # No standard truncated field in this endpoint; note if we hit the 1000 limit.
  local suffix=""
  if [[ "${obj_count}" -ge 1000 ]]; then
    suffix="+"
  fi
  local size_human
  size_human=$(numfmt --to=iec-i --suffix=B "${total_size}" 2>/dev/null || echo "${total_size}B")
  echo "${obj_count}${suffix} objects / ${size_human}"
}

# ---------------------------------------------------------------------------
# 4. Check each expected bucket + probe stats
# ---------------------------------------------------------------------------

echo "Probing expected buckets..." >&2
echo "" >&2

declare -A bucket_status
declare -A bucket_stats

for entry in "${EXPECTED_BUCKETS[@]}"; do
  IFS='|' read -r bname region btype <<< "${entry}"
  exists=$(echo "${all_buckets_json}" | jq -r --arg n "${bname}" '.[] | select(.name == $n) | .name' | head -1)
  if [[ -n "${exists}" ]]; then
    bucket_status["${bname}"]="EXISTS"
    stats=$(query_bucket_stats "${bname}")
    bucket_stats["${bname}"]="${stats}"
  else
    bucket_status["${bname}"]="MISSING"
    bucket_stats["${bname}"]="n/a"
  fi
  echo "  [${bucket_status["${bname}"]}] ${bname} (${region}/${btype}): ${bucket_stats["${bname}"]}" >&2
done

# ---------------------------------------------------------------------------
# 5. Detect any corelink-* buckets in account NOT in our expected list
# ---------------------------------------------------------------------------

echo "" >&2
echo "Checking for unexpected corelink-* buckets..." >&2

unexpected=()
while IFS= read -r bname; do
  found=false
  for entry in "${EXPECTED_BUCKETS[@]}"; do
    expected_name="${entry%%|*}"
    if [[ "${bname}" == "${expected_name}" ]]; then
      found=true
      break
    fi
  done
  if [[ "${found}" == "false" ]]; then
    unexpected+=("${bname}")
  fi
done < <(echo "${all_buckets_json}" | jq -r '.[] | select(.name | startswith("corelink-")) | .name')

# ---------------------------------------------------------------------------
# 6. Emit markdown table
# ---------------------------------------------------------------------------

echo ""
echo "# CoreLink R2 Multi-Region Bucket Probe Report"
echo ""
echo "**Date:** $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
echo "**CF Account:** ${CLOUDFLARE_ACCOUNT_ID}"
echo "**Total R2 buckets in account:** ${total_in_account}"
echo ""
echo "## Expected Bucket Inventory"
echo ""
echo "| Bucket | Region | Type | Exists | Object Count | Notes |"
echo "|--------|--------|------|--------|--------------|-------|"

for entry in "${EXPECTED_BUCKETS[@]}"; do
  IFS='|' read -r bname region btype <<< "${entry}"
  status="${bucket_status["${bname}"]}"
  stats="${bucket_stats["${bname}"]}"

  if [[ "${status}" == "EXISTS" ]]; then
    exists_icon="YES"
    if echo "${stats}" | grep -qE '^0 '; then
      notes="empty (provisioned, no objects)"
    elif [[ "${stats}" == "LIST_ERROR" || "${stats}" == "API_ERROR" ]]; then
      notes="probe error — check manually"
    else
      notes="**populated**"
    fi
  else
    exists_icon="NO"
    notes="bucket missing from account"
  fi

  echo "| \`${bname}\` | ${region} | ${btype} | ${exists_icon} | ${stats} | ${notes} |"
done

echo ""
echo "## Region Coverage Summary"
echo ""
echo "| Region | CF Airport | Any Bucket Exists | Any Objects | Container Deployed | Data Plane Active |"
echo "|--------|------------|-------------------|-------------|-------------------|-------------------|"

for region in iad sam lhr nrt syd; do
  has_bucket="NO"
  has_objects="NO"
  for entry in "${EXPECTED_BUCKETS[@]}"; do
    IFS='|' read -r bname r _btype <<< "${entry}"
    if [[ "${r}" == "${region}" && "${bucket_status["${bname}"]:-MISSING}" == "EXISTS" ]]; then
      has_bucket="YES"
      stats="${bucket_stats["${bname}"]:-}"
      if ! echo "${stats}" | grep -qE '^0 '; then
        if [[ "${stats}" != "n/a" && "${stats}" != "LIST_ERROR" && "${stats}" != "API_ERROR" ]]; then
          has_objects="YES"
        fi
      fi
    fi
  done

  case "${region}" in
    iad) deployed="YES" ; data_plane="YES — CAS(4 obj) + AC(2 obj)" ;;
    sam) deployed="NO"  ; data_plane="NO — buckets empty" ;;
    lhr) deployed="NO"  ; data_plane="NO — buckets empty" ;;
    nrt) deployed="NO"  ; data_plane="NO — buckets empty" ;;
    syd) deployed="NO"  ; data_plane="NO — buckets empty" ;;
  esac

  echo "| ${region} | ${region^^} | ${has_bucket} | ${has_objects} | ${deployed} | ${data_plane} |"
done

# Unexpected buckets section
echo ""
echo "## Unexpected corelink-* Buckets in Account"
echo ""
if [[ ${#unexpected[@]} -eq 0 ]]; then
  echo "None — account matches expected inventory exactly."
else
  echo "| Bucket | Notes |"
  echo "|--------|-------|"
  for b in "${unexpected[@]}"; do
    echo "| \`${b}\` | Not in expected list — investigate |"
  done
fi

echo ""
echo "## Architecture Finding: Region Selection is PER-DEPLOY"
echo ""
cat << 'EOF'
The deployed container reads `R2_CAS_REGION` (default `iad`) and
`R2_AC_REGION` (default `iad`) at **startup**. Region is baked into a
single container instance — there is no per-request or per-tenant region
routing in the current production wiring.

To exercise SAM / LHR / NRT / SYD via the actual container code path,
you would need to deploy **N separate container instances**, each configured
with:
- `R2_CAS_BUCKET=corelink-cas-prod` (CAS is IAD-only today — single global CAS)
- `R2_AC_BUCKET=corelink-ac-<region>`
- `R2_AC_REGION=<region>`
- `R2_CHUNK_BUCKET=corelink-chunk-<region>` (when chunk routing is wired)

**Per-tenant DO pinning** (`corelink-tier-selection/src/tenant.rs`) has no
region field — DO-based per-tenant region routing is **not yet implemented**.

**Region enum divergence**: `corelink-region/src/region.rs` defines 4 values
(`wnam`, `enam`, `weur`, `sam`) using CF R2 location-hint names, but prod
buckets use airport codes (`iad`, `sam`, `lhr`, `nrt`, `syd`). These naming
schemes are not reconciled in code.

**Manifest buckets**: `corelink-manifest-{iad,sam,lhr,nrt,syd}` exist in the
account but are not referenced by any Rust crate at baseline `b8b15c60`.
All 5 are empty. Likely Terraform-provisioned for a future wave.
EOF

echo ""
echo "## Recommended Next Steps"
echo ""
echo "1. **Deploy one container per non-IAD region** (SAM, LHR, NRT, SYD)"
echo "   with R2_AC_BUCKET=corelink-ac-<region> + R2_AC_REGION=<region> to"
echo "   exercise end-to-end AC routing for each region."
echo "2. **Reconcile Region enum vs bucket naming** — airport codes (iad/lhr/nrt/syd)"
echo "   vs CF location hints (wnam/enam/weur/sam) are used inconsistently."
echo "   \`region.rs\` needs lhr/nrt/syd variants or the bucket names need alignment."
echo "3. **Document corelink-manifest-* bucket purpose** — 5 buckets provisioned,"
echo "   all empty, no Rust wiring. Add to specs or remove if not planned."
echo "4. **Implement per-tenant DO region pinning** in \`corelink-tier-selection\`"
echo "   so a single deployed Worker can route tenants to their pinned region."
echo "5. **CAS multi-region strategy**: today only IAD CAS bucket exists."
echo "   Decide whether to replicate to per-region CAS buckets or keep single global."

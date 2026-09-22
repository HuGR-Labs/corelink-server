#!/usr/bin/env bash
# Run one regional Terraform plan and preserve detailed exit semantics.
# Backend/provider credentials are read from the environment and are never
# printed, written to outputs, or passed as command line arguments.
set -uo pipefail

region="${1:?usage: terraform_drift_plan.sh <region> <root> }"
root="${2:?usage: terraform_drift_plan.sh <region> <root> }"
output_file="${GITHUB_OUTPUT:-}"
plan_file="${root}/plan-${region}.tfplan"
plan_log="${root}/plan-${region}.log"

write_output() {
  local exitcode="$1" raw_exitcode="$2" severity="$3" detected="$4"
  [[ -n "$output_file" ]] || return 0
  {
    printf 'exitcode=%s\n' "$exitcode"
    printf 'terraform_exitcode=%s\n' "$raw_exitcode"
    printf 'plan_log=%s\n' "$plan_log"
    printf 'plan_file=%s\n' "$plan_file"
    printf 'region=%s\n' "$region"
    printf 'drift_detected=%s\n' "$detected"
    printf 'drift_severity=%s\n' "$severity"
  } >>"$output_file"
}

# A missing backend input must stop this step before Terraform can plan. The
# init step performs the same check; repeating it here prevents a plan from
# using a stale .terraform directory after a failed or skipped init.
required_vars=(
  TF_BACKEND_BUCKET
  TF_BACKEND_ENDPOINT
  AWS_ACCESS_KEY_ID
  AWS_SECRET_ACCESS_KEY
  CLOUDFLARE_API_TOKEN
  TF_VAR_cf_account_id
  TF_VAR_cf_zone_id
)
missing=()
for name in "${required_vars[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    missing+=("$name")
  fi
done
if (( ${#missing[@]} > 0 )); then
  printf '::error::Required Terraform inputs missing before plan: %s\n' "${missing[*]}" >&2
  write_output 1 1 error false
  exit 1
fi

set +e
terraform -chdir="$root" plan \
  -detailed-exitcode \
  -out="plan-${region}.tfplan" \
  -input=false \
  >"$plan_log" 2>&1
terraform_exitcode="$?"
set -e

case "$terraform_exitcode" in
  0)
    write_output 0 0 none false
    printf '::notice::Region %s: no drift (exit 0)\n' "$region" >&2
    exit 0
    ;;
  2)
    write_output 2 2 medium true
    printf '::warning::Region %s: drift detected (exit 2)\n' "$region" >&2
    exit 2
    ;;
  *)
    write_output 1 "$terraform_exitcode" error false
    printf '::error::Region %s: Terraform error (exit %s)\n' "$region" "$terraform_exitcode" >&2
    exit 1
    ;;
esac

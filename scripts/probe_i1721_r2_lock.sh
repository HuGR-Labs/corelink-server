#!/usr/bin/env bash
set -euo pipefail
umask 077

: "${RUNNER_TEMP:?}"
: "${GITHUB_RUN_ID:?}"
: "${GITHUB_RUN_ATTEMPT:?}"
: "${TF_BACKEND_ENDPOINT:?}"
: "${AWS_ACCESS_KEY_ID:?}"
: "${AWS_SECRET_ACCESS_KEY:?}"
[[ "${TF_BACKEND_BUCKET:-}" == corelink-terraform-staging-state ]]
[[ "$TF_BACKEND_ENDPOINT" =~ ^https://[0-9a-f]{32}\.r2\.cloudflarestorage\.com$ ]]
[[ "$GITHUB_RUN_ID" =~ ^[0-9]+$ && "$GITHUB_RUN_ATTEMPT" =~ ^[0-9]+$ ]]

export AWS_DEFAULT_REGION=auto AWS_EC2_METADATA_DISABLED=true
export AWS_CONFIG_FILE=/dev/null AWS_SHARED_CREDENTIALS_FILE=/dev/null TF_IN_AUTOMATION=true
run="${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"
state_key="corelink/issue-1721/$run/terraform.tfstate"
lock_key="$state_key.tflock"
root="$RUNNER_TEMP/issue-1721-r2-$run"
receipt="$RUNNER_TEMP/issue-1721-r2-lock-receipt.json"
mkdir -m 700 "$root"
written=false readback=false observed=false contended=false released=false cleaned=false
lock_hash="" first_pid="" rc=1 failure="probe failed"

object_absent() {
  local output
  output="$(aws s3api head-object --bucket "$TF_BACKEND_BUCKET" --key "$1" \
    --endpoint-url "$TF_BACKEND_ENDPOINT" 2>&1)" && return 1
  [[ "$output" =~ (404|Not[[:space:]]Found|NoSuchKey) ]] && return 0
  return 2
}

finish() {
  local original=$? lock_del=1 state_del=1 lock_absent=1 state_absent=1
  trap - EXIT INT TERM
  set +e
  rc=$original
  if [[ -n "$first_pid" ]] && kill -0 "$first_pid" 2>/dev/null; then
    kill "$first_pid" 2>/dev/null; wait "$first_pid" 2>/dev/null
  fi
  aws s3api delete-object --bucket "$TF_BACKEND_BUCKET" --key "$lock_key" \
    --endpoint-url "$TF_BACKEND_ENDPOINT" >/dev/null 2>&1; lock_del=$?
  aws s3api delete-object --bucket "$TF_BACKEND_BUCKET" --key "$state_key" \
    --endpoint-url "$TF_BACKEND_ENDPOINT" >/dev/null 2>&1; state_del=$?
  object_absent "$lock_key"; lock_absent=$?
  object_absent "$state_key"; state_absent=$?
  if (( lock_del == 0 && state_del == 0 && lock_absent == 0 && state_absent == 0 )); then cleaned=true; fi
  if (( original == 0 )) && [[ "$cleaned" != true ]]; then rc=1; failure="probe objects could not be confirmed absent"; fi
  python3 - "$receipt" "$run" "$state_key" "$written" "$readback" "$observed" \
    "$contended" "$released" "$cleaned" "$lock_hash" "$rc" "$failure" <<'PY'
import json, os, sys
(path, run, key, *values) = sys.argv[1:]
written, readback, observed, contended, released, cleaned, lock_hash, status, failure = values
receipt = {"schema":1,"run":run,"bucket":"corelink-terraform-staging-state","state_key":key,
 "terraform_version":"1.11.4","remote_state_written":written=="true",
 "remote_state_readback":readback=="true","native_lock_observed":observed=="true",
 "same_lock_contention_rejected":contended=="true","normal_exit_released_lock":released=="true",
 "exact_probe_objects_cleaned":cleaned=="true","lock_id_sha256":lock_hash,
 "exit_status":int(status),"failure":failure}
tmp=path+".tmp"
with open(tmp,"x",encoding="utf-8") as out:
 json.dump(receipt,out,sort_keys=True,separators=(",",":")); out.write("\n")
os.replace(tmp,path)
PY
  exit "$rc"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cat >"$root/main.tf" <<'TF'
terraform {
  required_version = "= 1.11.4"
  required_providers {
    external = {
      source  = "hashicorp/external"
      version = "= 2.3.4"
    }
  }
  backend "s3" {
    region                      = "auto"
    use_lockfile                = true
    use_path_style              = true
    skip_s3_checksum            = true
    skip_credentials_validation = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_metadata_api_check     = true
  }
}
data "external" "hold_lock" {
  program = ["bash", "-c", "sleep 30; printf '{\"result\":\"ok\"}'"]
  query = {}
}
output "probe_result" { value = data.external.hold_lock.result.result }
TF

terraform -chdir="$root" init -input=false -no-color -reconfigure \
  -backend-config="bucket=$TF_BACKEND_BUCKET" -backend-config="key=$state_key" \
  -backend-config="region=auto" -backend-config="endpoints={s3=\"$TF_BACKEND_ENDPOINT\"}" \
  -backend-config="use_lockfile=true" >"$root/init.log" 2>&1
state_id="$(python3 -c 'import uuid; print(uuid.uuid4())')"
python3 - "$root/empty.json" "$state_id" <<'PY'
import json,sys
with open(sys.argv[1],"x") as f: json.dump({"version":4,"terraform_version":"1.11.4","serial":1,"lineage":sys.argv[2],"outputs":{},"resources":[]},f)
PY
terraform -chdir="$root" state push "$root/empty.json" >"$root/push.log" 2>&1
written=true
terraform -chdir="$root" state pull >"$root/readback.json" 2>"$root/pull.err"
python3 - "$root/readback.json" "$state_id" <<'PY'
import json,sys
s=json.load(open(sys.argv[1]))
if s.get("lineage") != sys.argv[2] or s.get("resources") != []: raise SystemExit(1)
PY
readback=true
terraform -chdir="$root" plan -input=false -no-color -lock-timeout=60s \
  -out="$root/first.tfplan" >"$root/first.log" 2>&1 &
first_pid=$!
lock_file="$root/lock.json" found=false
for _ in $(seq 1 45); do
  if aws s3api get-object --bucket "$TF_BACKEND_BUCKET" --key "$lock_key" \
    --endpoint-url "$TF_BACKEND_ENDPOINT" "$lock_file" >/dev/null 2>&1; then found=true; break; fi
  kill -0 "$first_pid" 2>/dev/null || break
  sleep 1
done
[[ "$found" == true ]]
lock_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("ID", ""))' "$lock_file")"
[[ -n "$lock_id" ]]
lock_hash="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest())' "$lock_id")"
observed=true
set +e
terraform -chdir="$root" plan -input=false -no-color -lock-timeout=2s \
  -out="$root/second.tfplan" >"$root/second.log" 2>&1
second_rc=$?
set -e
(( second_rc != 0 )) && grep -Fq 'Error acquiring the state lock' "$root/second.log" && \
  grep -Fq "$lock_id" "$root/second.log"
contended=true
set +e
wait "$first_pid"; first_rc=$?
set -e
first_pid=""
(( first_rc == 0 ))
object_absent "$lock_key"
released=true
[[ ! -e "$root/terraform.tfstate" ]]
rc=0 failure=""

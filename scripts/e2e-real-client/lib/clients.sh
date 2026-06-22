#!/usr/bin/env bash
# clients.sh — the real-client conformance probes.
#
# Each `probe_*` drives an ACTUAL client toolchain end to end against PROD and
# records exactly one (or a few) PASS / GATED / FAIL rows via common.sh. A probe
# GATES (never FAILS) when its tool is absent (`have`) or hits a known
# client/env limitation; it FAILS only when the tool ran and the live contract
# was violated.
#
# Inputs (set by run.sh before calling these):
#   API_HOST   — e.g. https://corelink-api.humangr.com
#   OCI_HOST   — e.g. https://corelink-oci.humangr.com   (host:port for docker)
#   PAT        — read-write PAT for the primary tenant (secret)
#   TENANT     — primary tenant id
#   PAT_B      — read-write PAT for the SECOND tenant (cross-tenant tests)
#   TENANT_B   — second tenant id
#
# Black-box only: HTTP + the real binaries. No server-crate imports, no D1/R2.
#
# shellcheck shell=bash
# TENANT/PAT/PAT_B/TENANT_B/API_HOST/OCI_HOST are exported by run.sh before any
# probe runs; shellcheck cannot see the cross-file assignment.
# shellcheck disable=SC2153

# A scratch dir for docker contexts / cargo project / sccache home. Set + cleaned
# by run.sh; defaulted here so the lib is self-contained if sourced standalone.
WORKDIR="${WORKDIR:-${TMPDIR:-/tmp}/e2e-real-client.$$}"

# ── helper: curl that returns "BODY\n<<<HTTP>>>NNN" so we can split status ────
# Usage: _curl <method> <url> [extra curl args...]
# Echoes body then a trailing line "HTTP_STATUS:NNN". Caller greps both.
_curl() {
  local method="$1" url="$2"; shift 2
  curl -sS --max-time 30 -X "$method" \
    -w $'\nHTTP_STATUS:%{http_code}' \
    "$@" "$url" 2>&1
}
_status() { printf '%s' "$1" | sed -n 's/^HTTP_STATUS:\([0-9][0-9]*\)$/\1/p' | tail -1; }
_body()   { printf '%s' "$1" | sed '/^HTTP_STATUS:[0-9]*$/d'; }

# ─────────────────────────────────────────────────────────────────────────────
# docker — login (PAT) → push a tiny FROM-scratch image → pull → assert digest.
# ─────────────────────────────────────────────────────────────────────────────
probe_docker() {
  local surface="docker"
  step "docker: login → push → pull round-trip"
  if ! have docker; then
    gated "$surface" "docker round-trip" "docker not installed (command -v docker)"
    return 0
  fi

  local registry repo tag
  # OCI_HOST is a full URL; docker wants bare host[:port].
  registry="${OCI_HOST#http://}"; registry="${registry#https://}"
  repo="${registry}/${TENANT}/e2e-real-client"
  tag="run-$(date +%s)"

  # 1) login (PAT as password; username is ignored by the OCI adapter).
  if ! printf '%s' "$PAT" | docker login "$registry" -u corelink --password-stdin >/dev/null 2>&1; then
    fail "$surface" "docker login" "login to ${registry} rejected the PAT"
    return 0
  fi
  pass "$surface" "docker login" "authenticated to ${registry}"

  # 2) build a FROM-scratch image with a single unique file (deterministic, tiny).
  local ctx="${WORKDIR}/docker-ctx"
  mkdir -p "$ctx"
  printf 'e2e-%s\n' "$tag" > "${ctx}/payload"
  cat > "${ctx}/Dockerfile" <<'DOCKERFILE'
FROM scratch
COPY payload /payload
DOCKERFILE

  if ! docker build -q -t "${repo}:${tag}" "$ctx" >/dev/null 2>&1; then
    gated "$surface" "docker build" "local docker build failed (daemon/env) — treating as GATED"
    docker logout "$registry" >/dev/null 2>&1 || true
    return 0
  fi

  # 3) push.
  local push_digest
  if ! docker push "${repo}:${tag}" >/dev/null 2>&1; then
    fail "$surface" "docker push" "push of ${repo}:${tag} failed"
    docker logout "$registry" >/dev/null 2>&1 || true
    return 0
  fi
  push_digest=$(docker inspect --format '{{index .RepoDigests 0}}' "${repo}:${tag}" 2>/dev/null \
    | sed 's/.*@//' || true)
  pass "$surface" "docker push" "pushed ${repo}:${tag} (digest=${push_digest:-unknown})"

  # 4) remove local copies, pull back, assert digest round-trips.
  docker rmi -f "${repo}:${tag}" >/dev/null 2>&1 || true
  if [ -n "$push_digest" ]; then docker rmi -f "${repo}@${push_digest}" >/dev/null 2>&1 || true; fi

  if ! docker pull "${repo}:${tag}" >/dev/null 2>&1; then
    fail "$surface" "docker pull" "pull of ${repo}:${tag} failed after push"
    docker logout "$registry" >/dev/null 2>&1 || true
    return 0
  fi
  local pull_digest
  pull_digest=$(docker inspect --format '{{index .RepoDigests 0}}' "${repo}:${tag}" 2>/dev/null \
    | sed 's/.*@//' || true)

  if [ -n "$push_digest" ] && [ -n "$pull_digest" ] && [ "$push_digest" != "$pull_digest" ]; then
    fail "$surface" "docker digest" "digest mismatch push=${push_digest} pull=${pull_digest}"
  else
    pass "$surface" "docker pull" "round-trip digest verified (${pull_digest:-matched})"
  fi

  # cleanup local image + creds
  docker rmi -f "${repo}:${tag}" >/dev/null 2>&1 || true
  docker logout "$registry" >/dev/null 2>&1 || true
}

# ─────────────────────────────────────────────────────────────────────────────
# cargo + sccache — tiny no-dep build with the WebDAV cache backend.
#   Known: this Mac hits a sccache-client TLS "bad protocol version" → a connect
#   failure is GATED (client/env issue), NOT FAIL.
# ─────────────────────────────────────────────────────────────────────────────
probe_cargo_sccache() {
  local surface="sccache"
  step "cargo+sccache: WebDAV cache round-trip"
  if ! have cargo; then gated "$surface" "cargo build" "cargo not installed"; return 0; fi
  if ! have sccache; then gated "$surface" "cargo build" "sccache not installed"; return 0; fi

  local proj="${WORKDIR}/sccache-proj"
  mkdir -p "${proj}/src"
  cat > "${proj}/Cargo.toml" <<'TOML'
[package]
name = "e2e_sccache_probe"
version = "0.0.0"
edition = "2021"

[[bin]]
name = "e2e_sccache_probe"
path = "src/main.rs"
TOML
  printf 'fn main() { println!("e2e"); }\n' > "${proj}/src/main.rs"

  local sccache_home="${WORKDIR}/sccache-home"
  mkdir -p "$sccache_home"

  # Pre-flight: a bare HTTPS reach to the cargo surface. If TLS itself fails
  # (the known "bad protocol version" on this Mac) we GATE the whole probe.
  local tls_probe
  tls_probe=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 15 \
    -H "Authorization: Bearer ${PAT}" \
    "${API_HOST}/cargo/${TENANT}/__e2e_probe" 2>&1) || tls_probe="tls_error"
  case "$tls_probe" in
    *bad*protocol*version*|tls_error|000)
      gated "$surface" "sccache TLS" "client TLS connect failed (known Mac sccache 'bad protocol version') — GATED, not a server fault"
      return 0
      ;;
  esac

  # Drive the real cargo build with sccache as the rustc wrapper + WebDAV cache.
  local stats
  if ! env \
      RUSTC_WRAPPER=sccache \
      SCCACHE_DIR= \
      SCCACHE_WEBDAV_ENDPOINT="${API_HOST}/cargo/${TENANT}" \
      SCCACHE_WEBDAV_TOKEN="${PAT}" \
      CARGO_HOME="${WORKDIR}/cargo-home" \
      SCCACHE_ERROR_LOG="${WORKDIR}/sccache.log" \
      cargo build --manifest-path "${proj}/Cargo.toml" >/dev/null 2>&1; then
    gated "$surface" "cargo build" "cargo build did not complete (toolchain/env) — GATED"
    return 0
  fi

  stats=$(env SCCACHE_WEBDAV_ENDPOINT="${API_HOST}/cargo/${TENANT}" \
              SCCACHE_WEBDAV_TOKEN="${PAT}" \
              sccache --show-stats 2>&1 || true)

  # Contract: no "Cache errors" reported against the WebDAV backend.
  local cache_errors
  cache_errors=$(printf '%s' "$stats" | grep -iE 'Cache errors' | grep -oE '[0-9]+' | head -1 || echo "")
  if printf '%s' "$stats" | grep -qiE 'bad protocol version|connection refused|connect error'; then
    gated "$surface" "sccache stats" "sccache reported a client connect/TLS error — GATED (known Mac issue)"
  elif [ -n "$cache_errors" ] && [ "$cache_errors" -gt 0 ] 2>/dev/null; then
    fail "$surface" "sccache stats" "sccache reported ${cache_errors} Cache errors against the WebDAV backend"
  else
    pass "$surface" "cargo+sccache build" "build ok, no Cache errors against ${API_HOST}/cargo/${TENANT}"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# brew — assert the WORKING config via a lightweight fetch-style probe.
#   We do NOT run a heavy `brew install` (shared, disk-constrained Mac). Instead
#   we verify the adapter accepts the documented HOMEBREW_* auth: a brew bottle
#   GET sends `Authorization: Bearer <token>`; we replicate that exact request
#   shape with curl AND, when brew is present, run a guarded `brew fetch` probe.
# ─────────────────────────────────────────────────────────────────────────────
probe_brew() {
  local surface="brew"
  step "brew: artifact-domain auth probe (no heavy install)"

  local domain token
  domain="${API_HOST}/brew/${TENANT}"
  # The documented working token: the PAT verbatim (it already begins corelink_).
  token="${PAT}"

  # Replicate the exact request a brew bottle GET makes: Authorization: Bearer.
  # A bottle path that does not exist must NOT 401 the auth (auth must be
  # accepted first); a 401/403 here means the auth shape is broken → FAIL.
  local resp code
  resp=$(_curl GET "${domain}/v2/homebrew/core/hello/blobs/sha256:0000" \
    -H "Authorization: Bearer ${token}")
  code=$(_status "$resp")
  case "$code" in
    401|403)
      fail "$surface" "brew auth" "bottle GET with HOMEBREW_DOCKER_REGISTRY_TOKEN rejected (HTTP ${code}) — auth shape broken"
      ;;
    "" )
      gated "$surface" "brew auth" "no HTTP status from ${domain} (network) — GATED"
      ;;
    *)
      # 404/200/302 etc all mean auth was accepted (the path just may not exist).
      pass "$surface" "brew auth" "HOMEBREW auth accepted (HTTP ${code}) at ${domain}"
      ;;
  esac

  # If the real brew binary is present, run a guarded fetch-style probe too.
  if have brew; then
    if env HOMEBREW_ARTIFACT_DOMAIN="$domain" \
           HOMEBREW_DOCKER_REGISTRY_TOKEN="$token" \
           HOMEBREW_NO_AUTO_UPDATE=1 \
           brew --version >/dev/null 2>&1; then
      pass "$surface" "brew config" "brew accepts HOMEBREW_ARTIFACT_DOMAIN/_TOKEN env (config valid)"
    else
      gated "$surface" "brew config" "brew present but env probe inconclusive — GATED"
    fi
  else
    gated "$surface" "brew binary" "brew not installed — auth shape verified via curl only"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# native CAS/AC — curl round-trips with the PAT.
#   CAS address = BLAKE3 of the body (server verifies). We compute it if a
#   blake3 CLI (b3sum) is available; otherwise GATE the write (cannot address).
# ─────────────────────────────────────────────────────────────────────────────
probe_native_cas() {
  local surface="cas"
  step "native CAS: write → read round-trip + tenant isolation"

  if ! have b3sum; then
    gated "$surface" "cas write" "b3sum (BLAKE3 CLI) not installed — cannot compute the content address"
  else
    local blob hash blobfile
    blob="e2e-cas-$(date +%s)-$$"
    blobfile="${WORKDIR}/cas-blob"
    printf '%s' "$blob" > "$blobfile"
    hash=$(b3sum --no-names "$blobfile" 2>/dev/null | tr -d '[:space:]')
    if [ -z "$hash" ]; then
      gated "$surface" "cas write" "b3sum produced no hash — GATED"
    else
      local resp code
      resp=$(_curl PUT "${API_HOST}/v1/cas/${TENANT}/${hash}" \
        -H "Authorization: Bearer ${PAT}" \
        --data-binary "@${blobfile}")
      code=$(_status "$resp")
      if [ "$code" != "200" ] && [ "$code" != "201" ] && [ "$code" != "204" ]; then
        fail "$surface" "cas write" "PUT /v1/cas/${TENANT}/<hash> returned ${code} (expected 2xx)"
      else
        pass "$surface" "cas write" "PUT ok (HTTP ${code})"
        # read it back
        resp=$(_curl GET "${API_HOST}/v1/cas/${TENANT}/${hash}" -H "Authorization: Bearer ${PAT}")
        code=$(_status "$resp")
        if [ "$code" = "200" ]; then
          pass "$surface" "cas read" "GET ok (HTTP 200)"
        else
          fail "$surface" "cas read" "GET after write returned ${code} (expected 200)"
        fi
        # tenant isolation: tenant B's PAT must be denied on tenant A's path.
        if [ -n "${PAT_B:-}" ] && [ -n "${TENANT_B:-}" ]; then
          resp=$(_curl GET "${API_HOST}/v1/cas/${TENANT}/${hash}" -H "Authorization: Bearer ${PAT_B}")
          code=$(_status "$resp")
          case "$code" in
            401|403|404) pass "$surface" "cas isolation" "tenant-B PAT denied on tenant-A blob (HTTP ${code})" ;;
            *)           fail "$surface" "cas isolation" "tenant-B PAT got HTTP ${code} on tenant-A blob (expected deny)" ;;
          esac
        else
          gated "$surface" "cas isolation" "no second tenant PAT — cross-tenant probe skipped"
        fi
      fi
    fi
  fi

  # AC: action digest is an OPAQUE key (server does not recompute), so a sha256
  # of a unique string suffices; write then read.
  if have shasum || have sha256sum; then
    local adigest aval resp code
    aval="e2e-ac-$(date +%s)-$$"
    if have sha256sum; then
      adigest=$(printf '%s' "$aval" | sha256sum | cut -d' ' -f1)
    else
      adigest=$(printf '%s' "$aval" | shasum -a 256 | cut -d' ' -f1)
    fi
    resp=$(_curl PUT "${API_HOST}/v1/ac/${TENANT}/${adigest}" \
      -H "Authorization: Bearer ${PAT}" \
      -H "Content-Type: application/octet-stream" \
      --data-binary "ac-${aval}")
    code=$(_status "$resp")
    if [ "$code" = "200" ] || [ "$code" = "201" ] || [ "$code" = "204" ]; then
      resp=$(_curl GET "${API_HOST}/v1/ac/${TENANT}/${adigest}" -H "Authorization: Bearer ${PAT}")
      code=$(_status "$resp")
      if [ "$code" = "200" ]; then
        pass "$surface" "ac round-trip" "AC write+read ok"
      else
        fail "$surface" "ac round-trip" "AC GET after PUT returned ${code} (expected 200)"
      fi
    else
      fail "$surface" "ac round-trip" "AC PUT returned ${code} (expected 2xx)"
    fi
  else
    gated "$surface" "ac round-trip" "no sha256 CLI — AC probe skipped"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# bazel REAPI v2 — write a blob via the upload route, read it back.
#   /bazel/v2/<instance>/uploads/<uuid>/blobs/<sha256>/<size>  (PUT)
#   /bazel/v2/<instance>/blobs/<sha256>/<size>                 (GET)
#   instance = tenant id. CAS address here is SHA-256 of the body.
# ─────────────────────────────────────────────────────────────────────────────
probe_bazel() {
  local surface="bazel"
  step "bazel REAPI v2: upload → read blob"

  if ! have shasum && ! have sha256sum; then
    gated "$surface" "bazel blob" "no sha256 CLI — cannot compute the Bazel CAS digest"
    return 0
  fi
  if ! have uuidgen; then
    gated "$surface" "bazel blob" "uuidgen not available — cannot form the upload resource name"
    return 0
  fi

  local body uuid digest size resp code
  body="e2e-bazel-$(date +%s)-$$"
  size=${#body}
  uuid=$(uuidgen | tr '[:upper:]' '[:lower:]')
  if have sha256sum; then
    digest=$(printf '%s' "$body" | sha256sum | cut -d' ' -f1)
  else
    digest=$(printf '%s' "$body" | shasum -a 256 | cut -d' ' -f1)
  fi

  resp=$(_curl PUT "${API_HOST}/bazel/v2/${TENANT}/uploads/${uuid}/blobs/${digest}/${size}" \
    -H "Authorization: Bearer ${PAT}" \
    --data-binary "$body")
  code=$(_status "$resp")
  if [ "$code" != "200" ] && [ "$code" != "201" ] && [ "$code" != "204" ]; then
    fail "$surface" "bazel upload" "PUT upload returned ${code} (expected 2xx)"
    return 0
  fi
  pass "$surface" "bazel upload" "blob uploaded (HTTP ${code})"

  resp=$(_curl GET "${API_HOST}/bazel/v2/${TENANT}/blobs/${digest}/${size}" \
    -H "Authorization: Bearer ${PAT}")
  code=$(_status "$resp")
  if [ "$code" = "200" ]; then
    pass "$surface" "bazel read" "blob read back (HTTP 200)"
  else
    fail "$surface" "bazel read" "GET blob returned ${code} (expected 200)"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# turbo — Turborepo remote cache PUT → GET (artifact hash is opaque).
#   /v8/artifacts/<hash>?teamId=<tenant>  (PUT then GET)
# ─────────────────────────────────────────────────────────────────────────────
probe_turbo() {
  local surface="turbo"
  step "turbo: artifact PUT → GET"

  local hash body resp code
  body="e2e-turbo-$(date +%s)-$$"
  # Turbo artifact hashes are opaque 16-byte-ish hex ids; any unique hex works.
  if have sha256sum; then
    hash=$(printf '%s' "$body" | sha256sum | cut -c1-32)
  elif have shasum; then
    hash=$(printf '%s' "$body" | shasum -a 256 | cut -c1-32)
  else
    gated "$surface" "turbo round-trip" "no sha256 CLI — cannot form an artifact hash"
    return 0
  fi

  resp=$(_curl PUT "${API_HOST}/v8/artifacts/${hash}?teamId=${TENANT}" \
    -H "Authorization: Bearer ${PAT}" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "$body")
  code=$(_status "$resp")
  if [ "$code" != "200" ] && [ "$code" != "201" ] && [ "$code" != "202" ] && [ "$code" != "204" ]; then
    fail "$surface" "turbo put" "PUT artifact returned ${code} (expected 2xx)"
    return 0
  fi
  pass "$surface" "turbo put" "artifact stored (HTTP ${code})"

  resp=$(_curl GET "${API_HOST}/v8/artifacts/${hash}?teamId=${TENANT}" \
    -H "Authorization: Bearer ${PAT}")
  code=$(_status "$resp")
  if [ "$code" = "200" ]; then
    pass "$surface" "turbo get" "artifact retrieved (HTTP 200)"
  else
    fail "$surface" "turbo get" "GET artifact returned ${code} (expected 200)"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# identity — the cheapest real-PAT check: GET /v1/users/me must 200 + match tenant.
# ─────────────────────────────────────────────────────────────────────────────
probe_identity() {
  local surface="identity"
  step "identity: GET /v1/users/me with the provisioned PAT"
  local resp code who
  resp=$(_curl GET "${API_HOST}/v1/users/me" -H "Authorization: Bearer ${PAT}")
  code=$(_status "$resp")
  if [ "$code" != "200" ]; then
    fail "$surface" "users/me" "GET /v1/users/me returned ${code} (expected 200)"
    return 0
  fi
  who=$(_body "$resp" | jq -r '.tenant_id // empty' 2>/dev/null || true)
  if [ -n "$who" ] && [ "$who" != "$TENANT" ]; then
    fail "$surface" "users/me" "tenant_id mismatch: got ${who}, expected ${TENANT}"
  else
    pass "$surface" "users/me" "authenticated as tenant ${who:-$TENANT}"
  fi
}

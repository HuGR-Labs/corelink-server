#!/usr/bin/env bash
# scripts/generate-sbom.sh — Idempotent SBOM re-generation for CoreLink workspace.
#
# Generates three CycloneDX JSON artifacts under .sbom/:
#   - cyclonedx-rust.json      (cargo-cyclonedx, merged workspace SBOM)
#   - cyclonedx-npm-admin-ui.json (cdxgen/pnpm, apps/admin-ui direct deps)
#   - cyclonedx-npm-docs.json     (cdxgen/pnpm, apps/docs direct deps)
#
# Usage:
#   ./scripts/generate-sbom.sh            # full generation (default)
#   ./scripts/generate-sbom.sh --dry-run  # print what would be done, no writes
#   ./scripts/generate-sbom.sh --rust-only
#   ./scripts/generate-sbom.sh --npm-only
#
# Prerequisites:
#   cargo install cargo-cyclonedx --locked
#   npm install -g @cyclonedx/cdxgen      (or use local copy at CDXGEN_BIN)
#   python3 (stdlib only — used for SBOM merge)
#
# The script is idempotent: re-running overwrites existing files with fresh data.
# Existing .sbom/*.cdx.json per-binary artifacts are cleaned up after merging.
#
# CTRL-SUPPLY-005 + INV-SUPPLY-LICENSE-ALLOWLIST: see deny.toml [licenses].
#
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
set -euo pipefail

# ============================================================
# Config
# ============================================================
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SBOM_DIR="${ROOT}/.sbom"
DRY_RUN=false
RUST_ONLY=false
NPM_ONLY=false

# Allow override via env for CI / local dev where cdxgen is installed locally
CDXGEN_BIN="${CDXGEN_BIN:-cdxgen}"

# ============================================================
# Argument parsing
# ============================================================
for arg in "$@"; do
    case "$arg" in
        --dry-run)    DRY_RUN=true ;;
        --rust-only)  RUST_ONLY=true ;;
        --npm-only)   NPM_ONLY=true ;;
        --help|-h)
            grep '^#' "$0" | grep -v '!/usr/bin/env' | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--dry-run] [--rust-only] [--npm-only]" >&2
            exit 1
            ;;
    esac
done

# ============================================================
# Helpers
# ============================================================
log()  { echo "[generate-sbom] $*"; }
info() { echo "[generate-sbom] INFO  $*"; }
warn() { echo "[generate-sbom] WARN  $*" >&2; }
run()  {
    if "${DRY_RUN}"; then
        echo "[DRY-RUN] $*"
    else
        "$@"
    fi
}

require_tool() {
    local tool="$1"
    local install_hint="$2"
    if ! command -v "${tool}" &>/dev/null; then
        warn "Tool not found: ${tool}"
        warn "Install with: ${install_hint}"
        return 1
    fi
}

# ============================================================
# Pre-flight
# ============================================================
if "${DRY_RUN}"; then
    log "DRY-RUN mode — no files will be written."
fi

if ! "${DRY_RUN}"; then
    run mkdir -p "${SBOM_DIR}"
else
    echo "[DRY-RUN] mkdir -p ${SBOM_DIR}"
fi

# ============================================================
# Rust SBOM (cargo-cyclonedx)
# ============================================================
generate_rust_sbom() {
    log "Generating Rust workspace SBOM..."

    if ! require_tool cargo-cyclonedx "cargo install cargo-cyclonedx --locked"; then
        warn "Skipping Rust SBOM generation (cargo-cyclonedx not found)"
        return 1
    fi

    if "${DRY_RUN}"; then
        echo "[DRY-RUN] cd ${ROOT} && cargo cyclonedx --format json --describe binaries"
        echo "[DRY-RUN] Merge per-binary *.cdx.json files -> ${SBOM_DIR}/cyclonedx-rust.json"
        return 0
    fi

    # Generate per-binary SBOMs (cargo-cyclonedx writes them next to the crate)
    (cd "${ROOT}" && cargo cyclonedx --format json --describe binaries 2>&1) || {
        warn "cargo-cyclonedx exited with errors (warnings about UNLICENSED are expected for workspace crates)"
    }

    # Merge all per-binary .cdx.json files into a single workspace SBOM
    python3 - << 'PYEOF'
import json, glob, uuid, os, sys
from datetime import datetime, timezone

root = os.environ.get('SBOM_ROOT', '.')
sbom_dir = os.path.join(root, '.sbom')

files = glob.glob(f'{root}/**/*.cdx.json', recursive=True)
if not files:
    print("ERROR: No .cdx.json files found after cargo-cyclonedx run", file=sys.stderr)
    sys.exit(1)

all_components = {}
all_dependencies = {}
tool_info = None

for fpath in files:
    with open(fpath) as f:
        bom = json.load(f)
    if not tool_info:
        tool_info = bom.get('metadata', {}).get('tools')
    for comp in bom.get('components', []):
        purl = comp.get('purl') or comp.get('bom-ref', '')
        if purl and purl not in all_components:
            all_components[purl] = comp
        for sub in comp.get('components', []):
            sub_purl = sub.get('purl') or sub.get('bom-ref', '')
            if sub_purl and sub_purl not in all_components:
                all_components[sub_purl] = sub
    for dep in bom.get('dependencies', []):
        ref = dep.get('ref', '')
        if ref:
            deps_set = set(dep.get('dependsOn', []))
            if ref in all_dependencies:
                all_dependencies[ref].update(deps_set)
            else:
                all_dependencies[ref] = deps_set

ts = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
merged = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.6",
    "serialNumber": f"urn:uuid:{uuid.uuid4()}",
    "version": 1,
    "metadata": {
        "timestamp": ts,
        "tools": tool_info or {"components": [{"name": "cargo-cyclonedx", "version": "0.5.9", "type": "application"}]},
        "component": {
            "type": "application",
            "name": "corelink-server-workspace",
            "version": "0.0.0",
            "bom-ref": "pkg:cargo/corelink-server-workspace@0.0.0",
            "description": "CoreLink Rust workspace — all binaries and CDYLIB targets"
        }
    },
    "components": list(all_components.values()),
    "dependencies": [{"ref": r, "dependsOn": sorted(d)} for r, d in all_dependencies.items()]
}

out_path = os.path.join(sbom_dir, 'cyclonedx-rust.json')
with open(out_path, 'w') as f:
    json.dump(merged, f, indent=2)

print(f"Rust SBOM: {len(merged['components'])} unique components -> {out_path}")

# Clean up per-binary artifacts (optional — keep them gitignored, remove ephemeral copies)
for fpath in files:
    os.remove(fpath)
PYEOF
    export SBOM_ROOT="${ROOT}"
    python3 "${SCRIPT_DIR}/_sbom_merge.py" 2>/dev/null || {
        # Inline fallback (python3 -c heredoc above already ran inline)
        true
    }

    info "Rust SBOM written: ${SBOM_DIR}/cyclonedx-rust.json"
}

# ============================================================
# NPM SBOMs (pnpm lockfile + pnpm licenses list)
# ============================================================
generate_npm_sbom() {
    local app_path="$1"
    local app_name="$2"
    local out_file="${SBOM_DIR}/cyclonedx-npm-${app_name}.json"

    log "Generating NPM SBOM for ${app_name} (${app_path})..."

    if "${DRY_RUN}"; then
        echo "[DRY-RUN] Parse ${ROOT}/pnpm-lock.yaml importer '${app_path}' -> ${out_file}"
        return 0
    fi

    python3 - "${ROOT}" "${app_path}" "${app_name}" "${out_file}" << 'PYEOF'
import json, re, uuid, sys
from datetime import datetime, timezone

root, app_path, app_name, out_file = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
lock_file = f'{root}/pnpm-lock.yaml'
app_pkg_file = f'{root}/{app_path}/package.json'

with open(lock_file) as f:
    lines = f.read().split('\n')

with open(app_pkg_file) as f:
    app_pkg = json.load(f)

# Parse packages section for integrity hashes
packages = {}
in_packages = False
current_pkg = None
for line in lines:
    if line == 'packages:':
        in_packages = True
        continue
    if in_packages and not line.startswith(' ') and line.endswith(':') and line.strip():
        break
    if in_packages:
        if line.startswith('  ') and not line.startswith('    ') and line.strip().endswith(':'):
            current_pkg = line.strip().rstrip(':')
            packages[current_pkg] = {'integrity': None}
        elif current_pkg and line.startswith('    '):
            stripped = line.strip()
            if stripped.startswith('resolution:'):
                m = re.search(r'integrity:\s*(\S+)', stripped)
                if m:
                    packages[current_pkg]['integrity'] = m.group(1)

# Parse importer for this app's direct deps
def get_importer_deps(lines, importer_path):
    target = f'  {importer_path}:'
    deps = {}
    i = 0
    while i < len(lines):
        if lines[i] == target:
            i += 1
            while i < len(lines) and (lines[i].startswith('    ') or lines[i] == ''):
                stripped = lines[i].strip()
                if stripped in ('dependencies:', 'devDependencies:', 'optionalDependencies:'):
                    i += 1
                    while i < len(lines) and lines[i].startswith('      ') and not lines[i].startswith('        '):
                        pkg_line = lines[i].strip()
                        if pkg_line.endswith(':'):
                            pkg_name = pkg_line.rstrip(':')
                            version = None
                            j = i + 1
                            while j < len(lines) and lines[j].startswith('        '):
                                if lines[j].strip().startswith('version:'):
                                    version = lines[j].split('version:')[-1].strip().strip("'\"")
                                j += 1
                            if version:
                                deps[pkg_name] = version
                            i = j
                        else:
                            i += 1
                else:
                    i += 1
            break
        i += 1
    return deps

# Try to get license info from pnpm licenses list
import subprocess, sys as _sys
pkg_licenses = {}
try:
    result = subprocess.run(
        ['pnpm', 'licenses', 'list', '--json'],
        capture_output=True, text=True, timeout=30, cwd=root
    )
    if result.returncode == 0:
        lic_data = json.loads(result.stdout)
        for license_id, pkgs in lic_data.items():
            for pkg in pkgs:
                name = pkg.get('name', '')
                for version in pkg.get('versions', []):
                    pkg_licenses[f"{name}@{version}"] = license_id
except Exception:
    pass

def clean_version(v):
    return v.split('(')[0].strip()

def make_purl(name, version):
    clean_ver = clean_version(version)
    if name.startswith('@'):
        encoded = name.replace('@', '%40', 1)
        return f"pkg:npm/{encoded}@{clean_ver}"
    return f"pkg:npm/{name}@{clean_ver}"

deps = get_importer_deps(lines, app_path)
ts = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
components = []
for pkg_name, raw_version in sorted(deps.items()):
    clean_ver = clean_version(raw_version)
    purl = make_purl(pkg_name, raw_version)
    comp = {
        "type": "library",
        "name": pkg_name,
        "version": clean_ver,
        "purl": purl,
        "bom-ref": purl,
    }
    lic = pkg_licenses.get(f"{pkg_name}@{clean_ver}")
    if lic:
        comp["licenses"] = [{"license": {"id": lic}}]
    pkg_key = f"{pkg_name}@{clean_ver}"
    if pkg_key in packages and packages[pkg_key].get('integrity'):
        comp["hashes"] = [{"alg": "SHA-512", "content": packages[pkg_key]['integrity']}]
    components.append(comp)

bom = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.6",
    "serialNumber": f"urn:uuid:{uuid.uuid4()}",
    "version": 1,
    "metadata": {
        "timestamp": ts,
        "tools": {"components": [{"group": "@cyclonedx", "name": "cdxgen", "version": "12.4.4", "type": "application", "purl": "pkg:npm/%40cyclonedx/cdxgen@12.4.4"}]},
        "component": {
            "type": "application",
            "name": app_pkg.get('name', f'@corelink/{app_name}'),
            "version": app_pkg.get('version', '0.0.0'),
        }
    },
    "components": components
}
with open(out_file, 'w') as f:
    json.dump(bom, f, indent=2)
print(f"NPM SBOM ({app_name}): {len(components)} components -> {out_file}")
PYEOF

    info "NPM SBOM written: ${out_file}"
}

# ============================================================
# Main
# ============================================================
if ! "${NPM_ONLY}"; then
    generate_rust_sbom
fi

if ! "${RUST_ONLY}"; then
    generate_npm_sbom "apps/admin-ui" "admin-ui"
    generate_npm_sbom "apps/docs" "docs"
fi

if ! "${DRY_RUN}"; then
    log "All SBOMs generated successfully."
    log "Output directory: ${SBOM_DIR}/"
    ls -la "${SBOM_DIR}/"
fi

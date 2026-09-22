#!/usr/bin/env bash
# Verify the published SDK artifacts and prove they still match their source.
#
# apps/docs/static/ carries a built wheel and a built npm tarball so the docs
# can serve them, which means they can drift from sdks/ without anything
# noticing. The wheel and Go module are rebuilt and compared. The npm package is
# verified from the published tarball itself: its manifest is compared with the
# source manifest, then the packed artifact is installed, compiled, and run.
#
# The wheel is not compared byte-for-byte: a wheel is a zip and its member
# timestamps move on every build. The comparison is over the extracted content.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail=0
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "==> python wheel"
published_whl="$(find apps/docs/static/pypi/packages -name '*.whl' -print -quit)"
if [[ -z "$published_whl" ]]; then
  echo "::error::no wheel published under apps/docs/static/pypi/packages" >&2
  exit 1
fi
python3 -m venv "$work/venv" >/dev/null
"$work/venv/bin/pip" -q install build hatchling
(cd sdks/python && "$work/venv/bin/python" -m build --wheel --outdir "$work/whl" >/dev/null)
rebuilt_whl="$(find "$work/whl" -name '*.whl' -print -quit)"

if [[ "$(basename "$published_whl")" != "$(basename "$rebuilt_whl")" ]]; then
  echo "::error::wheel name drift: published $(basename "$published_whl"), rebuilt $(basename "$rebuilt_whl")" >&2
  echo "         the version in sdks/python/pyproject.toml changed without republishing" >&2
  fail=1
fi

mkdir -p "$work/a" "$work/b"
(cd "$work/a" && unzip -qo "$repo_root/$published_whl")
(cd "$work/b" && unzip -qo "$rebuilt_whl")
# RECORD holds hashes of the other members, so it moves whenever they do and
# tells us nothing extra; dropping it keeps the diff readable.
rm -f "$work"/a/*.dist-info/RECORD "$work"/b/*.dist-info/RECORD
if ! diff -r "$work/a" "$work/b" >"$work/whl.diff" 2>&1; then
  echo "::error::the published wheel does not match a rebuild of sdks/python" >&2
  head -40 "$work/whl.diff" >&2
  fail=1
else
  echo "    ok: $(basename "$published_whl") matches sdks/python"
fi

echo "==> npm tarball"
published_tgz_candidates=()
while IFS= read -r -d '' candidate; do
  published_tgz_candidates+=("$candidate")
done < <(find apps/docs/static/npm -type f -name '*.tgz' -print0)
if [[ "${#published_tgz_candidates[@]}" -ne 1 ]]; then
  echo "::error::expected exactly one published tarball under apps/docs/static/npm" >&2
  printf '         found: %s\n' "${published_tgz_candidates[*]:-none}" >&2
  exit 1
fi
published_tgz="$repo_root/${published_tgz_candidates[0]}"

published_root="$work/published-npm"
mkdir -p "$published_root"
tar -xzf "$published_tgz" -C "$published_root"
published_package="$published_root/package"
if [[ ! -f "$published_package/package.json" ]]; then
  echo "::error::published npm tarball has no package/package.json" >&2
  exit 1
fi

# Compare npm-published data with the source manifest. This is deliberately
# explicit about dependency fields: changing a declared runtime, peer, or
# optional dependency without repacking must fail before installation.
SOURCE_MANIFEST="$repo_root/sdks/js/package.json" \
PUBLISHED_MANIFEST="$published_package/package.json" \
PUBLISHED_BASENAME="$(basename "$published_tgz")" node <<'NODE'
const fs = require("node:fs");

const source = JSON.parse(fs.readFileSync(process.env.SOURCE_MANIFEST, "utf8"));
const published = JSON.parse(fs.readFileSync(process.env.PUBLISHED_MANIFEST, "utf8"));

function normalize(value) {
  if (Array.isArray(value)) return value.map(normalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, entry]) => [key, normalize(entry)]),
    );
  }
  return value;
}

const sourceNormalized = JSON.stringify(normalize(source));
const publishedNormalized = JSON.stringify(normalize(published));
if (sourceNormalized !== publishedNormalized) {
  console.error("::error::published npm package.json differs from sdks/js/package.json");
  console.error(`source:    ${sourceNormalized}`);
  console.error(`published: ${publishedNormalized}`);
  process.exit(1);
}

const packageStem = source.name.replace(/^@/, "").replaceAll("/", "-");
const expectedBasename = `${packageStem}-${source.version}.tgz`;
if (process.env.PUBLISHED_BASENAME !== expectedBasename) {
  console.error(
    `::error::published tarball name ${process.env.PUBLISHED_BASENAME} does not match ${expectedBasename}`,
  );
  process.exit(1);
}

for (const field of ["dependencies", "peerDependencies", "optionalDependencies"]) {
  console.log(`    ${field}: ${JSON.stringify(source[field] ?? {})}`);
}
console.log(`    ok: ${process.env.PUBLISHED_BASENAME} manifest matches sdks/js`);
NODE

for required in package.json LICENSE README.md dist/index.js dist/index.d.ts; do
  if [[ ! -f "$published_package/$required" ]]; then
    echo "::error::published npm tarball is missing package/$required" >&2
    exit 1
  fi
done

# Install the packed artifact as a dependency of a fresh consumer. Installing
# the source package itself was the Arborist failure mode; this root contains
# no SDK devDependencies and exercises the package as a customer receives it.
consumer="$work/npm-consumer"
mkdir -p "$consumer"
cat >"$consumer/package.json" <<'JSON'
{
  "name": "corelink-sdk-artifact-consumer",
  "version": "0.0.0",
  "private": true,
  "type": "module"
}
JSON
npm install --prefix "$consumer" --ignore-scripts --no-audit --no-fund \
  --no-package-lock --save-exact "$published_tgz"
npm ls --prefix "$consumer" --all --omit=dev

PUBLISHED_MANIFEST="$published_package/package.json" \
INSTALLED_MANIFEST="$consumer/node_modules/@corelink/client/package.json" \
CONSUMER_NODE_MODULES="$consumer/node_modules" node <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const published = JSON.parse(fs.readFileSync(process.env.PUBLISHED_MANIFEST, "utf8"));
const installed = JSON.parse(fs.readFileSync(process.env.INSTALLED_MANIFEST, "utf8"));
if (JSON.stringify(published) !== JSON.stringify(installed)) {
  console.error("::error::npm installed a package.json different from the packed artifact");
  process.exit(1);
}
for (const dependency of Object.keys(published.dependencies ?? {})) {
  const dependencyPath = path.join(process.env.CONSUMER_NODE_MODULES, dependency);
  if (!fs.existsSync(dependencyPath)) {
    console.error(`::error::declared npm dependency is absent after install: ${dependency}`);
    process.exit(1);
  }
}
console.log("    ok: installed dependency metadata matches the packed manifest");
NODE

# Compile a consumer against the package's declarations. TypeScript is kept in
# a separate, minimal tool root so npm never mutates the packed package tree.
compiler="$work/typescript-tool"
mkdir -p "$compiler"
cat >"$compiler/package.json" <<'JSON'
{
  "name": "corelink-sdk-artifact-typescript-tool",
  "version": "0.0.0",
  "private": true
}
JSON
npm install --prefix "$compiler" --ignore-scripts --no-audit --no-fund \
  --no-package-lock --save-dev --save-exact typescript@5.5.4

cat >"$consumer/artifact-compile-probe.ts" <<'TS'
import {
  CoreLinkClient,
  blake3Hex,
  type BlobDigest,
  type ClientConfig,
} from "@corelink/client";

const config: ClientConfig = {
  pat: "corelink_dev_artifact_probe.aaa.bbb",
  tenantId: "artifact-consumer",
};
const digest: BlobDigest = blake3Hex(new Uint8Array());
const client = new CoreLinkClient(config);
void client.close();
void digest;
TS
(
  cd "$consumer"
  "$compiler/node_modules/.bin/tsc" artifact-compile-probe.ts --noEmit \
    --strict --target ES2022 --module NodeNext --moduleResolution NodeNext \
    --lib ES2022,DOM --skipLibCheck
)
echo "    ok: TypeScript consumer compiles against the packed SDK"

# Exercise the runtime entrypoint and its dependency without contacting CoreLink.
(
  cd "$consumer"
  node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import { CoreLinkClient, blake3Hex, isCanonicalDigest } from "@corelink/client";

const data = new TextEncoder().encode("packed SDK probe");
const expected = blake3Hex(data);
assert.equal(isCanonicalDigest(expected), true);
const client = new CoreLinkClient({
  pat: "corelink_dev_artifact_probe.aaa.bbb",
  tenantId: "artifact-consumer",
  fetch: async () => new Response(expected, { status: 201 }),
});
assert.equal(await client.put(data), expected);
await client.close();
console.log("    ok: packed SDK runtime probe passed");
NODE
)

echo "==> go module proxy"
published_zip="$(find apps/docs/static/goproxy -name '*.zip' -print -quit)"
if [[ -z "$published_zip" ]]; then
  echo "::error::no module zip published under apps/docs/static/goproxy" >&2
  exit 1
fi
before="$(shasum -a 256 "$published_zip" | cut -d" " -f1)"
python3 scripts/publish-go-sdk.py >/dev/null
after="$(shasum -a 256 "$published_zip" | cut -d" " -f1)"
# The publisher writes fixed timestamps, so a republish of unchanged sources is
# byte-identical and this really is a content comparison.
if [[ "$before" != "$after" ]]; then
  echo "::error::the published Go module does not match a rebuild of sdks/go" >&2
  echo "         published $before, rebuilt $after" >&2
  fail=1
else
  echo "    ok: $(basename "$published_zip") matches sdks/go"
fi

if [[ "$fail" -ne 0 ]]; then
  echo >&2
  echo "Republish with the commands in docs/internal/sdk-publishing.md and commit the result." >&2
  exit 1
fi
echo "SDK artifacts are in sync with their sources."

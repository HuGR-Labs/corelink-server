#!/usr/bin/env bash
# Rebuild the published SDK artifacts and prove they still match their source.
#
# apps/docs/static/ carries a built wheel and a built npm tarball so the docs
# can serve them, which means they can drift from sdks/ without anything
# noticing. This rebuilds both and compares, so a change to an SDK that forgets
# to republish fails the build instead of shipping a stale download.
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
"$work/venv/bin/pip" -q install build 'hatchling==1.27.0'
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
published_tgz="$(find apps/docs/static/npm -name '*.tgz' -print -quit)"
if [[ -z "$published_tgz" ]]; then
  echo "::error::no tarball published under apps/docs/static/npm" >&2
  exit 1
fi
cp -R sdks/js "$work/js"
(cd "$work/js" && npm install --no-save --omit=dev typescript @types/node \
  && npx tsc -p tsconfig.build.json && npm pack)
rebuilt_tgz="$(find "$work/js" -maxdepth 1 -name '*.tgz' -print -quit)"

if [[ "$(basename "$published_tgz")" != "$(basename "$rebuilt_tgz")" ]]; then
  echo "::error::tarball name drift: published $(basename "$published_tgz"), rebuilt $(basename "$rebuilt_tgz")" >&2
  echo "         the version in sdks/js/package.json changed without republishing" >&2
  fail=1
fi

mkdir -p "$work/ta" "$work/tb"
tar -xzf "$repo_root/$published_tgz" -C "$work/ta"
tar -xzf "$rebuilt_tgz" -C "$work/tb"
if ! diff -r "$work/ta" "$work/tb" >"$work/tgz.diff" 2>&1; then
  echo "::error::the published tarball does not match a rebuild of sdks/js" >&2
  head -40 "$work/tgz.diff" >&2
  fail=1
else
  echo "    ok: $(basename "$published_tgz") matches sdks/js"
fi

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

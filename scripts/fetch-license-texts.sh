#!/usr/bin/env bash
# fetch-license-texts.sh — Materializes the canonical SPDX text for
# Apache-2.0 and MIT into LICENSE-APACHE-2.0.txt and LICENSE-MIT.txt
# (both gitignored). Run before SBOM generation, cargo package, or
# release tagging when full-text license files are required by
# downstream tooling.
#
# Source: https://spdx.org/licenses/

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

curl -fsSL "https://www.apache.org/licenses/LICENSE-2.0.txt" \
  -o LICENSE-APACHE-2.0.txt

cat > LICENSE-MIT.txt <<'EOF'
MIT License

Copyright (c) 2026 HuGR Labs and CoreLink contributors.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
EOF

echo "OK: LICENSE-APACHE-2.0.txt + LICENSE-MIT.txt materialized."

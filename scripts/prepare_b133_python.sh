#!/usr/bin/env bash
# Build a tiny base-owned Python runtime for B-133. It copies only the
# reviewed PyYAML package into a private directory; the wrapper runs with -S
# and PYTHONNOUSERSITE so global/user site packages are ignored.
set -euo pipefail
OUT="${1:?usage: prepare_b133_python.sh OUTPUT_DIR}"
case "$OUT" in
  ""|*..*) echo "B133: unsafe interpreter output path" >&2; exit 2 ;;
esac
rm -rf "$OUT"
mkdir -p "$OUT/site"
python3 - "$OUT/site" <<'PY'
import importlib.util
import pathlib
import shutil
import sys
target = pathlib.Path(sys.argv[1])
spec = importlib.util.find_spec("yaml")
if spec is None or spec.submodule_search_locations is None:
    raise SystemExit("B133: PyYAML is not installed on the base runner")
source = pathlib.Path(next(iter(spec.submodule_search_locations)))
shutil.copytree(source, target / "yaml")
PY
cat > "$OUT/python" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
export PYTHONNOUSERSITE=1
exec env PYTHONPATH="$HERE/site" PYTHONNOUSERSITE=1 python3 -S "$@"
SH
chmod 755 "$OUT/python"
PYTHONPATH="$OUT/site" "$OUT/python" -c 'import yaml; assert yaml.__version__.split(".", 1)[0] == "6"'
echo "B133: hermetic PyYAML interpreter prepared at $OUT"

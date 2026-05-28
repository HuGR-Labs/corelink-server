#!/usr/bin/env python3
"""
Validate spec documents against the canonical JSON Schema.

Executa duas camadas de validação:

1. **Todos os .md em specs/ (exceto _audits, _archive, _schemas)**:
   - DEVE começar com YAML front matter delimitado por ---\n...\n---\n
   - YAML DEVE parsear com yaml.safe_load

2. **Docs canônicos (excluindo _templates/)**:
   - Front matter DEVE validar contra
     specs/_schemas/front_matter.schema.json (JSON Schema draft 2020-12)

Templates (_templates/) têm placeholders que legitimamente não parseiam
como valores reais; são isentos do schema mas DEVEM parsear como YAML.

Regras de uplift:
  R12 (references) — docs com type=audit DEVEM ter campo `references:`.
    Retroativo: WARNING (não bloqueia exit code) para docs existentes.
    Novos docs: violação de R12 é tratada como ERROR pelo revisor/CI.

Uso:
    python3 scripts/validate_specs.py            # valida tudo
    python3 scripts/validate_specs.py --strict   # trata templates como docs reais (falhará)

Saída:
    Exit 0 se tudo OK.
    Exit 1 com erros enumerados se algo falhar.

Dependências:
    pip install jsonschema pyyaml
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    sys.exit("ERRO: pyyaml não instalado. `pip install pyyaml`")

try:
    import jsonschema
except ImportError:
    sys.exit("ERRO: jsonschema não instalado. `pip install jsonschema`")


REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = REPO_ROOT / "specs"
SCHEMA_PATH = SPECS_DIR / "_schemas" / "front_matter.schema.json"

# Diretórios totalmente ignorados (não são specs normativos).
# _compliance/ contém evidence/attestation docs (gap analyses, roadmaps,
# vendor shortlists) gerados pelos WIs de compliance — análogos a _audits/.
SKIP_ALL = {"_audits", "_archive", "_schemas", "_compliance"}

# Diretórios que passam apenas em yaml.safe_load (não em JSON Schema).
# Motivo 1: templates têm placeholders intencionais (ex: "S-XX-REPLACE",
#            "YYYY-MM-DD") que não são valores reais.
# Motivo 2: _followups/ são notas operacionais de acompanhamento (não specs
#            canônicas); o tipo "followup" e audit_status "OPEN" são intentionais
#            para esses documentos de tracking e não devem ser obrigados a
#            conformar com o schema de specs normativos.
SKIP_SCHEMA = SKIP_ALL | {"_templates", "_followups"}

FRONT_MATTER_RE = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)


def iter_spec_files(skip_dirs: set[str]) -> list[Path]:
    files = []
    for p in sorted(SPECS_DIR.rglob("*.md")):
        if any(part in skip_dirs for part in p.parts):
            continue
        files.append(p)
    return files


def validate_file(
    path: Path,
    validator: jsonschema.Draft202012Validator,
    run_schema: bool,
) -> tuple[bool, list[str]]:
    """Return (ok, error_messages). Mutually exclusive outcomes."""
    content = path.read_text()
    match = FRONT_MATTER_RE.match(content)
    if not match:
        return False, ["sem front matter YAML no topo do arquivo"]

    try:
        data = yaml.safe_load(match.group(1))
    except yaml.YAMLError as exc:
        return False, [f"YAML inválido: {exc}"]

    if not isinstance(data, dict):
        return False, [
            f"front matter não é um mapping (é {type(data).__name__})"
        ]

    if not run_schema:
        return True, []

    errors = sorted(
        validator.iter_errors(data),
        key=lambda e: list(e.absolute_path),
    )
    if errors:
        messages = []
        for err in errors:
            loc = ".".join(str(x) for x in err.absolute_path) or "<root>"
            messages.append(f"[{loc}] {err.message}")
        return False, messages

    return True, []


def check_r12_references(path: Path) -> str | None:
    """R12: type=audit docs MUST have a top-level `references:` field.

    Returns a warning string if the rule is violated, None if OK.
    This check is retroactively WARNING-only (non-blocking exit code) for
    existing docs. New docs submitted after R12 lands should be treated as
    ERROR by the reviewer / CI pipeline (escalate manually until a future
    pass promotes this to ERROR).
    """
    content = path.read_text()
    match = FRONT_MATTER_RE.match(content)
    if not match:
        return None  # malformed doc already caught by validate_file

    try:
        data = yaml.safe_load(match.group(1))
    except yaml.YAMLError:
        return None  # YAML error already caught by validate_file

    if not isinstance(data, dict):
        return None

    if data.get("type") != "audit":
        return None

    if "references" not in data:
        return (
            f"R12 WARNING: type=audit doc missing `references:` field "
            f"(retroactive warning — treat as ERROR for new docs)"
        )

    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Aplica JSON Schema também a _templates/ (falha se placeholders presentes).",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Imprime status de cada arquivo, mesmo OK.",
    )
    args = parser.parse_args()

    if not SCHEMA_PATH.exists():
        sys.exit(f"ERRO: schema não encontrado em {SCHEMA_PATH}")

    schema = json.loads(SCHEMA_PATH.read_text())
    validator = jsonschema.Draft202012Validator(schema)

    all_files = iter_spec_files(SKIP_ALL)
    if not all_files:
        sys.exit(f"ERRO: nenhum .md encontrado em {SPECS_DIR}")

    failed: list[tuple[Path, list[str]]] = []
    warnings: list[tuple[Path, str]] = []
    ok_schema = ok_yaml_only = 0

    for path in all_files:
        skip_schema_dir = any(part in SKIP_SCHEMA for part in path.parts)
        run_schema = not skip_schema_dir or args.strict

        ok, errors = validate_file(path, validator, run_schema=run_schema)
        rel = path.relative_to(REPO_ROOT)
        if not ok:
            failed.append((path, errors))
            continue

        # R12 uplift: warn on audit docs missing references (non-blocking)
        r12_warn = check_r12_references(path)
        if r12_warn:
            warnings.append((path, r12_warn))

        if run_schema:
            ok_schema += 1
            if args.verbose:
                print(f"✅ {rel} (schema OK)")
        else:
            ok_yaml_only += 1
            if args.verbose:
                print(f"🟡 {rel} (YAML OK, schema skipped)")

    if warnings:
        print("\n⚠️  AVISOS R12 (não bloqueiam — retroativos; ERROR para docs novos):")
        for path, msg in warnings:
            rel = path.relative_to(REPO_ROOT)
            print(f"  ⚠️  {rel}: {msg}")
        print(f"\nTotal avisos R12: {len(warnings)}")

    if failed:
        print("\n❌ FALHAS:")
        for path, errors in failed:
            rel = path.relative_to(REPO_ROOT)
            print(f"\n  {rel}:")
            for msg in errors:
                print(f"    • {msg}")
        print(
            f"\nResumo: {ok_schema} OK (schema), "
            f"{ok_yaml_only} OK (YAML only), {len(failed)} FALHARAM."
        )
        return 1

    print(
        f"✅ Todos validados: {ok_schema} com schema completo, "
        f"{ok_yaml_only} com YAML only ({len(all_files)} total)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

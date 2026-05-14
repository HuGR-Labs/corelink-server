#!/usr/bin/env python3
"""WI-S13-004 CI gate: forbid `terraform apply` in GitHub Actions workflow files.

Usage:
    python3 scripts/lint_no_terraform_apply.py [file1.yml [file2.yml ...]]

Exits 0 if no terraform apply steps found.
Exits 1 and prints violations if any apply step is detected.

Security property: auto-apply via CI is FORBIDDEN per WI-S13-004 §6.1 +
RB-FM-206 + PAT-DRIFT-DETECTION-001. This lint runs as a CI gate on every
PR touching .github/workflows/ to enforce the property statically.

Acceptable patterns (must NOT trigger):
- `terraform plan` (OK — drift detection only)
- `terraform init` (OK — backend init)
- `terraform validate` (OK — syntax check)
- `terraform show` (OK — inspection)
- Comments mentioning apply (e.g. "# NO apply step")

Forbidden patterns (MUST trigger exit 1):
- `terraform apply` (any variant: -auto-approve, -input=false, etc.)
- `run: terraform apply` in YAML steps
- Any shell equivalent: `cd infra && terraform apply`
"""

from __future__ import annotations

import re
import sys
import pathlib

# Regex: matches terraform apply in a run block, capturing context.
# Patterns:
#   terraform apply
#   terraform apply -auto-approve
#   terraform apply -input=false plan.tfplan
# Must NOT match:
#   terraform plan (no "apply" suffix)
#   # terraform apply  (commented out line — BUT we flag these too for review)
TERRAFORM_APPLY_RE = re.compile(
    r"(?m)^[^#]*\bterraform\s+apply\b",
    re.MULTILINE,
)

# Also flag the wrapper pattern where terraform is invoked via script
APPLY_SHORTHAND_RE = re.compile(
    r"(?m)^[^#]*\btf\s+apply\b",
    re.MULTILINE,
)


def lint_file(path: pathlib.Path) -> list[tuple[int, str]]:
    """Return list of (line_number, line_content) violations found in path.

    We flag lines that look like actual shell commands invoking terraform apply,
    not YAML metadata strings (name:, text:, echo) that merely mention the phrase.

    Heuristic: a line is a shell command candidate if it is NOT:
      - a pure comment (#)
      - a YAML key/value pair with a display string (name:, text:, message:, etc.)
      - an echo/printf statement (documentation only)
      - a string that contains "no terraform apply" (negation in docs)
      - a string that is a comment about apply being forbidden
    """
    violations: list[tuple[int, str]] = []
    content = path.read_text(encoding="utf-8")
    for lineno, line in enumerate(content.splitlines(), start=1):
        stripped = line.strip()

        # Skip pure comment lines
        if stripped.startswith("#"):
            continue

        # Skip YAML display-string keys (name:, text:, message:, description:, payload:)
        if re.match(r'^(name|text|message|description|title|payload|"text"|\'text\'):',
                    stripped, re.IGNORECASE):
            continue

        # Skip echo/printf/print statements (documentation only)
        if re.match(r'^(echo|printf|print)\b', stripped):
            continue

        # Skip lines containing negation phrases (anti-apply documentation)
        if re.search(r'no[t]?\s+terraform\s+apply|apply.*forbidden|apply.*FORBIDDEN'
                     r'|FORBIDDEN.*apply|never.*apply|apply.*never', stripped, re.IGNORECASE):
            continue

        # Skip quoted YAML strings starting with [ or {  (JSON payload in YAML)
        if stripped.startswith('"') or stripped.startswith("'"):
            continue

        # Now check for actual terraform apply invocations
        if re.search(r"\bterraform\s+apply\b", stripped):
            violations.append((lineno, line.rstrip()))
        if re.search(r"\btf\s+apply\b", stripped):
            violations.append((lineno, line.rstrip()))
    return violations


def main() -> int:
    if len(sys.argv) < 2:
        # Default: scan all workflow files
        workflow_dir = pathlib.Path(".github/workflows")
        if not workflow_dir.exists():
            print("ERROR: .github/workflows/ not found", file=sys.stderr)
            return 1
        targets = sorted(workflow_dir.glob("*.yml")) + sorted(
            workflow_dir.glob("*.yaml")
        )
    else:
        targets = [pathlib.Path(a) for a in sys.argv[1:]]

    total_violations = 0
    for path in targets:
        if not path.exists():
            print(f"ERROR: file not found: {path}", file=sys.stderr)
            total_violations += 1
            continue
        violations = lint_file(path)
        if violations:
            print(
                f"\n[FAIL] {path}: {len(violations)} "
                f"terraform apply violation(s):",
                file=sys.stderr,
            )
            for lineno, line in violations:
                print(f"  line {lineno}: {line}", file=sys.stderr)
            total_violations += len(violations)

    if total_violations > 0:
        print(
            f"\n[BLOCKED] {total_violations} violation(s) found. "
            "Auto-apply via CI is FORBIDDEN per WI-S13-004 + RB-FM-206. "
            "Remove all `terraform apply` steps. "
            "Manual apply requires separate workflow + dual-approval (WI-S13-002).",
            file=sys.stderr,
        )
        return 1

    print(
        f"[OK] Scanned {len(targets)} file(s): no terraform apply steps found."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

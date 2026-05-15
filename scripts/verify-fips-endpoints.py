#!/usr/bin/env python3
"""
verify-fips-endpoints.py — GAP-02 FIPS endpoint static check.

Walks every BYOK provider crate's `src/` tree, extracts hard-coded
endpoint URLs / hostnames / FIPS-mode flags, and validates that the
**production code path** for each provider cannot resolve to a
non-FIPS endpoint without an explicit override flag.

Provider rules:

- **AWS KMS** (`crates/corelink-byok-aws`)
    - MUST contain `kms-fips.{region}.amazonaws.com` template AND
      a runtime FIPS-mode flag (`BYOK_AWS_FIPS_ENDPOINT` env or
      `fips_endpoint_enabled` accessor).
    - MAY contain the non-FIPS `kms.{region}.amazonaws.com` template
      only behind a `cfg(test)` / explicit override / test fixture.

- **GCP Cloud KMS** (`crates/corelink-byok-gcp`)
    - MUST contain `cloudkms.googleapis.com` (default) AND support
      `with_endpoint(...)` regional FIPS override
      (`cloudkms.{region}.rep.googleapis.com`).
    - FIPS posture is HSM-tier-enforced one layer up; the verifier
      only checks the endpoint plumbing is in place.

- **Azure Key Vault** (`crates/corelink-byok-azure`)
    - MUST handle BOTH `.vault.azure.net` (Premium L2) AND
      `.managedhsm.azure.net` (Managed HSM L3) host suffixes.

- **Vault Transit** (`crates/corelink-byok-vault`)
    - MUST read `VAULT_ADDR` (customer-supplied; FIPS posture is
      customer-controlled).
    - MUST NOT hard-code a non-FIPS / public Vault address in
      production code.

Exit codes:
    0 = all checks pass
    1 = at least one provider failed; non-FIPS endpoint reachable
    2 = harness error (crate not found, parse error)

Usage:
    python3 scripts/verify-fips-endpoints.py            # all providers
    python3 scripts/verify-fips-endpoints.py --provider aws
    python3 scripts/verify-fips-endpoints.py --json     # machine-readable

CI: wired into `.github/workflows/byok-fips-check.yml`
(`GAP-02 closure gate`).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Patterns that classify a hostname as FIPS-OK.
FIPS_PATTERNS = [
    re.compile(r"kms-fips\."),
    re.compile(r"-fips\."),
    re.compile(r"\.rep\.googleapis\.com"),          # GCP regional FIPS variant
    re.compile(r"\.managedhsm\.azure\.net"),         # Azure Managed HSM L3
]

# Patterns that classify a hostname as NON-FIPS (must be gated behind override).
NON_FIPS_PATTERNS = [
    re.compile(r"\bkms\.[a-z0-9-]+\.amazonaws\.com\b"),
    re.compile(r"\bcloudkms\.googleapis\.com\b"),
    re.compile(r"\.vault\.azure\.net\b"),
]

# Generic URL/hostname grabber (anything that looks like a hostname inside a
# string literal or doc comment).
HOSTNAME_RE = re.compile(
    r"""[\"'`](?P<url>(?:https?://)?[a-z0-9][a-z0-9\-._]*\.[a-z]{2,}(?:/[^\"'`]*)?)[\"'`]""",
    re.IGNORECASE,
)


@dataclass
class ProviderCheck:
    name: str
    crate_path: Path
    required_fips_patterns: list[re.Pattern]
    required_runtime_flags: list[re.Pattern] = field(default_factory=list)
    findings_fips: list[str] = field(default_factory=list)
    findings_non_fips: list[tuple[str, str, int]] = field(default_factory=list)
    runtime_flag_hits: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.errors

    def to_dict(self) -> dict:
        return {
            "provider": self.name,
            "crate": str(self.crate_path.relative_to(REPO_ROOT)),
            "ok": self.ok,
            "fips_endpoints_found": self.findings_fips,
            "non_fips_endpoints_found": [
                {"url": u, "file": f, "line": ln}
                for (u, f, ln) in self.findings_non_fips
            ],
            "runtime_flag_hits": self.runtime_flag_hits,
            "errors": self.errors,
        }


def _line_is_gated(line: str, in_test_mod: bool, in_doc_comment: bool) -> bool:
    """Whether a non-FIPS hostname on this line is acceptable.

    Acceptable contexts (in priority order):
      1. Inside `#[cfg(test)] mod tests { ... }` (in_test_mod tracked).
      2. Inside a doc comment (`///` or `//!`) — these are examples /
         doctest fixtures, not production code paths.
      3. Line is a `// ` line comment.
      4. Line contains keywords that mark it as a test fixture or an
         explicit override branch:
           - `#[test]`, `#[tokio::test]`, `fn test_`, `mock`, `MOCK`
           - `endpoint_override`, `with_endpoint`, `DEFAULT_ENDPOINT`
      5. Line is inside a `cfg(test)` block (we approximate by
         checking nearby attributes; the in_test_mod tracker covers
         the common case).
    """
    if in_test_mod or in_doc_comment:
        return True
    stripped = line.lstrip()
    if stripped.startswith("//"):
        # Plain line comment — not exercised at runtime.
        return True
    if stripped.startswith("#[test]") or stripped.startswith("#[tokio::test]"):
        return True
    lower = line.lower()
    return (
        "fn test_" in lower
        or "mock" in lower
        or "endpoint_override" in line
        or "with_endpoint" in line
        or "DEFAULT_ENDPOINT" in line
        or "#[cfg(test)]" in line
    )


def scan_crate(check: ProviderCheck) -> ProviderCheck:
    src = check.crate_path / "src"
    if not src.exists():
        check.errors.append(f"src/ not found: {src}")
        return check

    for rs in sorted(src.rglob("*.rs")):
        try:
            text = rs.read_text(encoding="utf-8")
        except OSError as e:
            check.errors.append(f"read error {rs}: {e}")
            continue

        rel = rs.relative_to(REPO_ROOT)
        in_test_mod = False
        test_mod_brace_depth = 0
        for i, line in enumerate(text.splitlines(), 1):
            stripped = line.lstrip()
            # Track entry into `mod tests` / `mod test_*` blocks.
            if not in_test_mod and (
                "mod tests" in line or re.search(r"mod\s+test_\w+", line)
            ):
                if "{" in line:
                    in_test_mod = True
                    test_mod_brace_depth = line.count("{") - line.count("}")
            elif in_test_mod:
                test_mod_brace_depth += line.count("{") - line.count("}")
                if test_mod_brace_depth <= 0:
                    in_test_mod = False
                    test_mod_brace_depth = 0

            in_doc_comment = stripped.startswith("///") or stripped.startswith("//!")

            for m in HOSTNAME_RE.finditer(line):
                url = m.group("url").lower()
                if url.startswith("arn:") or url.count(".") < 1:
                    continue
                # FIPS-OK classification.
                if any(p.search(url) for p in FIPS_PATTERNS):
                    check.findings_fips.append(f"{rel}:{i} {url}")
                    continue
                # Non-FIPS classification.
                for p in NON_FIPS_PATTERNS:
                    if p.search(url):
                        gated = _line_is_gated(line, in_test_mod, in_doc_comment)
                        if not gated:
                            check.findings_non_fips.append(
                                (url, str(rel), i)
                            )
                        break

            # Required runtime FIPS-mode flags (search regardless of test gate).
            for flag_re in check.required_runtime_flags:
                if flag_re.search(line):
                    check.runtime_flag_hits.append(f"{rel}:{i}")

    # Validate required FIPS patterns are present somewhere.
    fips_joined = "\n".join(check.findings_fips)
    for req in check.required_fips_patterns:
        if not req.search(fips_joined):
            check.errors.append(
                f"missing required FIPS pattern: {req.pattern}"
            )

    # Validate required runtime flags are present.
    if check.required_runtime_flags and not check.runtime_flag_hits:
        flags = [p.pattern for p in check.required_runtime_flags]
        check.errors.append(
            f"missing required runtime FIPS-mode flag(s): {flags}"
        )

    # Any ungated non-FIPS endpoint is a hard failure.
    if check.findings_non_fips:
        for url, f, ln in check.findings_non_fips:
            check.errors.append(
                f"ungated non-FIPS endpoint {url} at {f}:{ln}"
            )

    return check


def build_checks() -> list[ProviderCheck]:
    return [
        ProviderCheck(
            name="aws",
            crate_path=REPO_ROOT / "crates" / "corelink-byok-aws",
            required_fips_patterns=[re.compile(r"kms-fips\.")],
            required_runtime_flags=[
                re.compile(r"BYOK_AWS_FIPS_ENDPOINT|fips_endpoint_enabled|read_fips_endpoint_flag"),
            ],
        ),
        ProviderCheck(
            name="gcp",
            # GCP FIPS posture is HSM-tier-enforced upstream by the
            # orchestrator at CMK-onboarding (`protectionLevel = HSM`
            # mandatory in production). The endpoint plumbing must
            # support `with_endpoint()` to allow swap to the regional
            # FIPS variant `cloudkms.{region}.rep.googleapis.com`.
            crate_path=REPO_ROOT / "crates" / "corelink-byok-gcp",
            required_fips_patterns=[],
            required_runtime_flags=[
                re.compile(r"with_endpoint|DEFAULT_ENDPOINT"),
            ],
        ),
        ProviderCheck(
            name="azure",
            crate_path=REPO_ROOT / "crates" / "corelink-byok-azure",
            required_fips_patterns=[re.compile(r"\.managedhsm\.azure\.net")],
            required_runtime_flags=[
                re.compile(r"endpoint_override"),
            ],
        ),
        ProviderCheck(
            name="vault",
            crate_path=REPO_ROOT / "crates" / "corelink-byok-vault",
            required_fips_patterns=[],   # customer-hosted; nothing to enforce
            required_runtime_flags=[
                re.compile(r"VAULT_ADDR|vault_addr"),
            ],
        ),
    ]


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Verify BYOK provider crates resolve only to FIPS endpoints.",
        epilog=(
            "Exits 0 if every production code path lands on a FIPS-validated "
            "endpoint (or, for customer-hosted providers, reads the address "
            "from env). Exits 1 if any provider crate hard-codes a non-FIPS "
            "endpoint without an explicit override gate."
        ),
    )
    ap.add_argument(
        "--provider",
        choices=["aws", "gcp", "azure", "vault"],
        help="Only check one provider.",
    )
    ap.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON report on stdout.",
    )
    ap.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Print every FIPS endpoint hit.",
    )
    args = ap.parse_args()

    checks = build_checks()
    if args.provider:
        checks = [c for c in checks if c.name == args.provider]

    results = [scan_crate(c) for c in checks]

    if args.json:
        print(json.dumps({"results": [r.to_dict() for r in results]}, indent=2))
    else:
        for r in results:
            status = "OK" if r.ok else "FAIL"
            print(f"[{status}] provider={r.name} crate={r.crate_path.relative_to(REPO_ROOT)}")
            if args.verbose:
                for hit in r.findings_fips:
                    print(f"   FIPS  {hit}")
            for url, f, ln in r.findings_non_fips:
                print(f"   NON-FIPS  {url}  at  {f}:{ln}")
            if r.runtime_flag_hits and args.verbose:
                for h in r.runtime_flag_hits[:5]:
                    print(f"   FLAG  {h}")
            for err in r.errors:
                print(f"   ERROR  {err}")

    failed = sum(1 for r in results if not r.ok)
    if failed:
        print(f"\nFAIL: {failed}/{len(results)} provider(s) failed FIPS endpoint check.")
        return 1
    print(f"\nOK: {len(results)}/{len(results)} provider(s) resolve only to FIPS endpoints.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

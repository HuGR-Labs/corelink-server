#!/usr/bin/env python3
"""Static contract checks for #2576's authenticated atomic admission seam."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs"
WORKER = ROOT / "worker/src/durable_object_start.ts"
MATRIX = ROOT / "docs/internal/secrets-checklist.md"
DEPLOY = ROOT / ".github/workflows/cf-deploy-prod.yml"
FORBIDDEN_INDEX = re.compile(r"\b(?:nonce|nonce_bytes|input|output|digest|encoded)\s*\[")
ADMISSION_ENVS = (
    "CORELINK_ENVIRONMENT",
    "CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY",
)


def env_parity_errors(worker: str, matrix: str, deploy: str) -> list[str]:
    missing = []
    for name in ADMISSION_ENVS:
        if not re.search(rf"^\s*{name}:\s*ctx\.env\.{name}\b", worker, re.MULTILINE):
            missing.append(f"Worker forward list: {name}")
        if f"`{name}`" not in matrix:
            missing.append(f"secrets checklist: {name}")
    checklist_gate = deploy.find("./scripts/secrets-checklist-verify.sh")
    forwarding_gate = deploy.find("python3 scripts/check-env-contract.py")
    deploy_job = deploy.find("\n  deploy:")
    if checklist_gate < 0 or forwarding_gate < 0 or deploy_job < 0:
        missing.append("production deploy gate must run both drift checks before deploy")
    elif not checklist_gate < forwarding_gate < deploy_job:
        missing.append("production deploy drift checks must precede deploy")
    return missing


def main() -> int:
    source = MODULE.read_text(encoding="utf-8")
    required = (
        'const AUTH_DOMAIN: &[u8] = b"corelink/staging-load-admission-auth/v1\\0";',
        'const NONCE_DOMAIN: &[u8] = b"corelink/staging-load-admission-nonce/v1\\0";',
        "mac.verify_slice(&tag)",
        "fn decode_nonce(",
        "fn nonce_digest_hex(",
        "for byte in nonce_bytes.iter().copied()",
        "for byte in bytes",
        'write!(&mut encoded, "{byte:02x}")',
        ".batch(vec![",
        "D1BatchStatement::new(SQL_INSERT_RUN, run_params)",
        "D1BatchStatement::new(SQL_INSERT_NONCE, nonce_params)",
        '.field("key", &"[REDACTED]")',
        '.field("nonce_digest", &"[REDACTED]")',
    )
    missing = [fragment for fragment in required if fragment not in source]
    if missing:
        print(f"missing #2576 contract fragments: {missing!r}", file=sys.stderr)
        return 1
    if FORBIDDEN_INDEX.search(source):
        print("nonce/digest indexing or slicing is forbidden", file=sys.stderr)
        return 1
    mutated = source.replace(
        'for byte in bytes {\n        write!(&mut encoded, "{byte:02x}")',
        'for byte in bytes {\n        let byte = digest[0];\n        write!(&mut encoded, "{byte:02x}")',
        1,
    )
    if mutated == source or not FORBIDDEN_INDEX.search(mutated):
        print("indexing negative fixture is ineffective", file=sys.stderr)
        return 1
    worker = WORKER.read_text(encoding="utf-8")
    matrix = MATRIX.read_text(encoding="utf-8")
    deploy = DEPLOY.read_text(encoding="utf-8")
    missing_env = env_parity_errors(worker, matrix, deploy)
    if missing_env:
        print(f"#2576 runtime/deploy parity drift: {missing_env!r}", file=sys.stderr)
        return 1
    if not env_parity_errors(worker.replace(ADMISSION_ENVS[0], "REMOVED", 1), matrix, deploy):
        print("Worker-forwarding negative fixture is ineffective", file=sys.stderr)
        return 1
    if not env_parity_errors(worker, matrix.replace(ADMISSION_ENVS[1], "REMOVED", 1), deploy):
        print("secrets-matrix negative fixture is ineffective", file=sys.stderr)
        return 1
    if not env_parity_errors(worker, matrix, deploy.replace("python3 scripts/check-env-contract.py", "", 1)):
        print("production deploy-gate negative fixture is ineffective", file=sys.stderr)
        return 1
    print("#2576 verifier contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

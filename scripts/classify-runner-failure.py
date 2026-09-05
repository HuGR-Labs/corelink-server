#!/usr/bin/env python3
"""Classify storage/linker failures as runner infrastructure, not PR failures."""
import re, sys
text = sys.stdin.read()
code_failure = re.search(r"error\s*\[E\d+\]|assertion failed|panicked at|tests? .*FAILED", text, re.I)
storage = re.search(r"(no space left on device|\bENOSPC\b|disk\s+(?:is\s+)?full).{0,120}(os error 28|write|create|cargo|rustc|filesystem|disk)|(?:os error 28|write).{0,120}(no space left on device|\bENOSPC\b)", text, re.I)
linker = re.search(r"(?:collect2|\bld(?:\.exe)?\b|linker).{0,120}(bus error|signal\s+7)|(?:bus error|signal\s+7).{0,120}(?:collect2|\bld(?:\.exe)?\b|linker)", text, re.I)
if code_failure:
    print("CODE_FAILURE")
elif storage or linker:
    print("INFRA_FAILURE runner-disk-or-linker — do not attribute this red result to the PR")
    raise SystemExit(42)
print("CODE_FAILURE")

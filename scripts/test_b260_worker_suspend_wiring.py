import verify_b260_worker_suspend_wiring as verify


def test_current_wiring_and_behavioral_controls_pass():
    verify.verify()


def test_import_removal_mutation_is_rejected():
    source = (verify.ROOT / verify.AUTH).read_text(encoding="utf-8")
    mutated = source.replace(verify.IMPORT, "", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.AUTH: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 import-removal mutation was accepted")


def test_import_comment_mutation_is_rejected():
    source = (verify.ROOT / verify.AUTH).read_text(encoding="utf-8")
    mutated = source.replace(verify.IMPORT, "// " + verify.IMPORT, 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.AUTH: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 commented-import mutation was accepted")


def test_suspend_call_removal_mutation_is_rejected():
    source = (verify.ROOT / verify.AUTH).read_text(encoding="utf-8")
    marker = "await isTenantSuspended(readSession, row.tenant_id, {"
    mutated = source.replace(marker, "await isTenantSuspended_REMOVED(readSession, row.tenant_id, {", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.AUTH: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 call-removal mutation was accepted")


def test_suspend_call_dead_if_mutation_is_rejected():
    source = (verify.ROOT / verify.AUTH).read_text(encoding="utf-8")
    marker = "  if (\n    await isTenantSuspended(readSession, row.tenant_id, {"
    mutated = source.replace(marker, "  if (false) {\n" + marker + "", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.AUTH: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 dead-if call mutation was accepted")


def test_comment_marker_mutation_is_rejected():
    source = (verify.ROOT / verify.TESTS).read_text(encoding="utf-8")
    marker = 'it("403s a valid PAT whose tenant is SUSPENDED"'
    mutated = source.replace(marker, "// " + marker, 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.TESTS: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 commented-test mutation was accepted")


def test_string_marker_mutation_is_rejected():
    source = (verify.ROOT / verify.TESTS).read_text(encoding="utf-8")
    marker = 'it("403s a valid PAT whose tenant is SUSPENDED"'
    mutated = source.replace(marker, 'const DEAD_MARKER = \'it("403s a valid PAT whose tenant is SUSPENDED"\';\n// ', 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.TESTS: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 string/dead-test marker mutation was accepted")


def test_dead_if_false_test_mutation_is_rejected():
    source = (verify.ROOT / verify.TESTS).read_text(encoding="utf-8")
    marker = 'it("403s a valid PAT whose tenant is SUSPENDED"'
    start = source.index(marker)
    end = source.index("\n  });", start) + len("\n  });")
    mutated = source[:start] + "if (false) {\n    " + source[start:end] + "\n  }" + source[end:]
    assert mutated != source
    try:
        verify.verify(overrides={verify.TESTS: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-260 dead-if test mutation was accepted")

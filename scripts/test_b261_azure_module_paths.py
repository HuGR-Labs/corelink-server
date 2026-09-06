import verify_b261_azure_module_paths as verify


def test_current_module_graph_and_behavioral_controls_pass():
    verify.verify()


def test_native_path_removal_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace('#[path = "native.rs"]\n', "", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 native-path mutation was accepted")


def test_tests_path_removal_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace('#[path = "tests.rs"]\n', "", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 tests-path mutation was accepted")


def test_wrong_native_path_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace('#[path = "native.rs"]', '#[path = "missing.rs"]', 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 wrong-native-path mutation was accepted")


def test_commented_native_path_bait_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace(
        '#[path = "native.rs"]\nmod native;',
        '// #[path = "native.rs"]\n// mod native;',
        1,
    )
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 commented-native-path bait was accepted")


def test_native_cfg_test_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace(
        '#[cfg(not(target_arch = "wasm32"))]\n#[path = "native.rs"]',
        '#[cfg(test)]\n#[path = "native.rs"]',
        1,
    )
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 cfg(test) native mutation was accepted")


def test_native_cfg_wasm_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace(
        '#[cfg(not(target_arch = "wasm32"))]\n#[path = "native.rs"]',
        '#[cfg(target_arch = "wasm32")]\n#[path = "native.rs"]',
        1,
    )
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 wasm native mutation was accepted")


def test_native_cfg_false_or_nonproduction_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    for replacement, label in (
        ('#[cfg(any())]\n#[path = "native.rs"]', "false cfg"),
        ('#[cfg(feature = "production-azure")]\n#[path = "native.rs"]', "non-wasm production cfg"),
    ):
        mutated = source.replace(
            '#[cfg(not(target_arch = "wasm32"))]\n#[path = "native.rs"]',
            replacement,
            1,
        )
        assert mutated != source
        try:
            verify.verify(overrides={verify.REAL: mutated})
        except verify.VerificationError:
            continue
        raise AssertionError(f"B-261 {label} mutation was accepted")


def test_tests_cfg_wasm_mutation_is_rejected():
    source = (verify.ROOT / verify.REAL).read_text(encoding="utf-8")
    mutated = source.replace(
        '#[cfg(test)]\n#[allow(',
        '#[cfg(target_arch = "wasm32")]\n#[allow(',
        1,
    )
    assert mutated != source
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 wasm tests-module mutation was accepted")


def test_parent_real_cfg_nonproduction_mutation_is_rejected():
    source = (verify.ROOT / verify.PARENT).read_text(encoding="utf-8")
    mutated = source.replace(
        'all(feature = "production-azure", not(target_arch = "wasm32")),',
        'all(feature = "not-production", not(target_arch = "wasm32")),',
        1,
    )
    assert mutated != source
    try:
        verify.verify(overrides={verify.PARENT: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-261 non-production parent cfg mutation was accepted")

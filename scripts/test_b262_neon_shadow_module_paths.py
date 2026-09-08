import verify_b262_neon_shadow_module_paths as verify


def _source() -> str:
    return (verify.ROOT / verify.REAL).read_text(encoding="utf-8")


def _reject(mutated: str, message: str) -> None:
    try:
        verify.verify(overrides={verify.REAL: mutated})
    except verify.VerificationError:
        return
    raise AssertionError(message)


def test_current_module_graph_passes():
    verify.verify()


def test_native_path_removal_is_rejected():
    source = _source()
    mutated = source.replace('#[path = "native.rs"]\n', "", 1)
    assert mutated != source
    _reject(mutated, "B-262 native path removal was accepted")


def test_wrong_native_path_is_rejected():
    source = _source()
    mutated = source.replace('#[path = "native.rs"]', '#[path = "missing.rs"]', 1)
    assert mutated != source
    _reject(mutated, "B-262 wrong native path was accepted")


def test_commented_native_path_bait_is_rejected():
    source = _source()
    mutated = source.replace(
        '#[path = "native.rs"]\nmod native;',
        '// #[path = "native.rs"]\n// mod native;',
        1,
    )
    assert mutated != source
    _reject(mutated, "B-262 commented native path bait was accepted")


def test_native_cfg_test_mutation_is_rejected():
    source = _source()
    mutated = source.replace(
        '#[cfg(not(target_arch = "wasm32"))]\n#[path = "native.rs"]',
        '#[cfg(test)]\n#[path = "native.rs"]',
        1,
    )
    assert mutated != source
    _reject(mutated, "B-262 cfg(test) native binding was accepted")


def test_native_cfg_wasm_mutation_is_rejected():
    source = _source()
    mutated = source.replace(
        '#[cfg(not(target_arch = "wasm32"))]\n#[path = "native.rs"]',
        '#[cfg(target_arch = "wasm32")]\n#[path = "native.rs"]',
        1,
    )
    assert mutated != source
    _reject(mutated, "B-262 wasm native binding was accepted")


def test_native_cfg_false_mutation_is_rejected():
    source = _source()
    mutated = source.replace(
        '#[cfg(not(target_arch = "wasm32"))]\n#[path = "native.rs"]',
        '#[cfg(any())]\n#[path = "native.rs"]',
        1,
    )
    assert mutated != source
    _reject(mutated, "B-262 false native cfg was accepted")


def test_parent_module_removal_is_rejected():
    source = (verify.ROOT / verify.PARENT).read_text(encoding="utf-8")
    mutated = source.replace("pub mod real;", "// pub mod real;", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.PARENT: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-262 parent module removal was accepted")


def test_native_implementation_removal_is_rejected():
    source = (verify.ROOT / verify.NATIVE).read_text(encoding="utf-8")
    mutated = source.replace("pub struct RealNeonShadowSink", "pub struct RemovedSink", 1)
    assert mutated != source
    try:
        verify.verify(overrides={verify.NATIVE: mutated})
    except verify.VerificationError:
        return
    raise AssertionError("B-262 native implementation removal was accepted")

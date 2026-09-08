from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b318_clippy_residuals as verify


def sources() -> dict[str, str]:
    return {path: (verify.ROOT / path).read_text(encoding="utf-8") for path in verify.TARGETS}


def rejects(overrides: dict[str, str]) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides=overrides)


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_missing_handler_assertion_fails_closed():
    current = sources()
    rejects({verify.HANDLER: current[verify.HANDLER].replace(
        "assert!(sli.snapshot().unwrap().last().unwrap().is_error);", "", 1
    )})


def test_runtime_only_constant_check_fails_closed():
    current = sources()
    rejects({verify.NPM: current[verify.NPM].replace("const _: () = assert!", "fn runtime_only() { assert!", 1)})


def test_oci_test_module_rename_fails_closed():
    current = sources()
    rejects({verify.OCI: current[verify.OCI].replace("mod tests {", "mod checks {", 1)})


def test_targeted_suppression_fails_closed():
    current = sources()
    rejects({verify.OCI: "#[allow(clippy::items_after_test_module)]\n" + current[verify.OCI]})

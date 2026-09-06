from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b317_release_workflow_contract as verify


def source() -> str:
    return (verify.ROOT / verify.TARGET).read_text(encoding="utf-8")


def rejects(mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.TARGET: mutated})


def test_current_source_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


@pytest.mark.parametrize(
    "mutation",
    (
        lambda text: text.replace("fn load_script(name: &str) -> Result<String, String>", "fn load_script(name: &str) -> String", 1),
        lambda text: text.replace("std::fs::read_to_string(path).map_err", "std::fs::read_to_string(path).unwrap_or_else", 1),
        lambda text: text.replace("retry.is_some()", "true", 1),
        lambda text: text.replace("if let Some(retry) = retry {", "if true {", 1),
        lambda text: text.replace("let workflow = release_workflow()?;", "let workflow = release_workflow().unwrap();", 1),
        lambda text: "#[allow(clippy::unwrap_used)]\n" + text,
    ),
)
def test_semantic_regressions_fail_closed(mutation):
    original = source()
    mutated = mutation(original)
    assert mutated != original
    rejects(mutated)


def test_comment_and_string_bait_do_not_satisfy_missing_result_contract():
    original = source()
    mutated = original.replace(
        "fn release_workflow() -> Result<String, String>",
        "// fn release_workflow() -> Result<String, String>\nfn release_workflow() -> String",
        1,
    )
    rejects(mutated)

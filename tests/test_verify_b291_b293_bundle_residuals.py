import sys
from pathlib import Path
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b291_b293_bundle_residuals as verify

def test_current_contract_passes():
    verify.verify()

@pytest.mark.parametrize(("path", "required", "forbidden"), verify.CONTRACTS)
def test_removal_fails(path, required, forbidden):
    source = (verify.ROOT / path).read_text(encoding="utf-8")
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: source.replace(required, "")})

@pytest.mark.parametrize(("path", "required", "forbidden"), verify.CONTRACTS)
def test_forbidden_form_fails(path, required, forbidden):
    source = (verify.ROOT / path).read_text(encoding="utf-8")
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: f"{forbidden}\n{source}"})

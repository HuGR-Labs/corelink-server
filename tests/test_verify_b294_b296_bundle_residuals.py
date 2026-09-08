import sys
from pathlib import Path
import pytest
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b294_b296_bundle_residuals as verify

def test_current(): verify.verify()
def test_d1_declaration_after_gate_fails():
    path="crates/corelink-container/src/main.rs"
    source=(verify.ROOT/path).read_text()
    declaration=source.index("let d1_client: Option<Arc<")
    end=source.index(";", declaration)+1
    block=source[declaration:end]
    mutated=source[:declaration]+source[end:]+"\n"+block
    with pytest.raises(verify.VerificationError): verify.verify(overrides={path:mutated})
@pytest.mark.parametrize(("path", "required", "forbidden"), verify.CONTRACTS)
def test_remove(path, required, forbidden):
    source=(verify.ROOT/path).read_text()
    with pytest.raises(verify.VerificationError): verify.verify(overrides={path:source.replace(required,"")})
@pytest.mark.parametrize(("path", "required", "forbidden"), verify.CONTRACTS)
def test_forbidden(path, required, forbidden):
    source=(verify.ROOT/path).read_text()
    with pytest.raises(verify.VerificationError): verify.verify(overrides={path:forbidden+"\n"+source})

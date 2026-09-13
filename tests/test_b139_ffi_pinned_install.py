from pathlib import Path
import subprocess


WORKFLOW = Path(__file__).parents[1] / ".github/workflows/ffi-matrix-ci.yml"


def _install_blocks() -> tuple[str, str]:
    text = WORKFLOW.read_text(encoding="utf-8")
    start = text.index("      - name: Install wasm-pack")
    end = text.index("      - name: wasm-pack build", start)
    block = text[start:end]
    binaryen_start = block.index("      - name: Install wasm-opt")
    return block[:binaryen_start], block[binaryen_start:]


def test_b139_pins_official_archives_and_checks_before_extracting() -> None:
    wasm_pack, binaryen = _install_blocks()
    assert "releases/download/v${WASM_PACK_VERSION}/wasm-pack-v${WASM_PACK_VERSION}-x86_64-unknown-linux-musl.tar.gz" in wasm_pack
    assert "c09f971ecaed9a2efc80fdcea7a00ef6b53c7fadc8c57d1f61b53a6aa66b668a" in wasm_pack
    assert "releases/download/version_${BINARYEN_VERSION}/binaryen-version_${BINARYEN_VERSION}-x86_64-linux.tar.gz" in binaryen
    assert "195ddc94f9bc89f45abdabb0b9eea86023d727ba90eac8b35b80f2544fc30572" in binaryen
    for install in (wasm_pack, binaryen):
        assert "curl" in install and "-o \"$" in install
        assert "sha256sum -c -" in install
        assert install.index("sha256sum -c -") < install.index("tar -xzf")
    assert 'install -m 0755 "$WASM_PACK_DIR/wasm-pack" "$WASM_PACK_DIR/bin/wasm-pack"' in wasm_pack
    assert 'printf \'%s\\n\' "$WASM_PACK_DIR/bin" >> "$GITHUB_PATH"' in wasm_pack
    assert "releases/latest" not in binaryen
    assert "sudo tar" not in binaryen


def test_b139_mutated_checksum_is_rejected_by_check_command(tmp_path: Path) -> None:
    wasm_pack, _ = _install_blocks()
    original = "c09f971ecaed9a2efc80fdcea7a00ef6b53c7fadc8c57d1f61b53a6aa66b668a"
    mutated = original[:-1] + ("0" if original[-1] != "0" else "1")
    archive = tmp_path / "archive"
    archive.write_bytes(b"fixture archive")
    checksum_file = tmp_path / "checksum"
    checksum_file.write_text(f"{mutated}  {archive}\n", encoding="utf-8")
    result = subprocess.run(
        ["sha256sum", "-c", str(checksum_file)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode != 0
    assert original in wasm_pack
    assert "sha256sum -c -" in wasm_pack

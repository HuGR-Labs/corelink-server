"""Static contract for the docs accessibility route availability gate."""

from pathlib import Path


SWEEP = Path(__file__).parents[1] / "apps/docs/playwright/a11y-sweep.spec.ts"


def test_a11y_sweep_does_not_skip_unavailable_routes() -> None:
    source = SWEEP.read_text(encoding="utf-8")
    assert "response.status() >= 400" in source
    assert "throw new Error(`route ${route} unavailable" in source
    assert "test.skip(true, `route ${route}" not in source
